#!/usr/bin/env python3
"""Audit the frozen Round footing comparison without running a simulation."""
import argparse
import copy
import hashlib
import importlib.util
import io
import itertools
import json
from pathlib import Path
import subprocess
import tarfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("reference", type=Path)
    args = parser.parse_args()
    helper = Path(__file__).with_name("audit-terrain-recovery-integration.py")
    spec = importlib.util.spec_from_file_location("integration_audit", helper)
    audit = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(audit)
    root, reference = args.root.resolve(), args.reference.resolve()
    runtime = audit.read(root / "result.json")
    assert runtime["exit_code"] == 0 and runtime["arm_count"] == 4
    assert audit.digest(root / "round-footing-comparison") == runtime["binary_sha256"]
    manifest = root / "source-inputs.json"
    assert audit.digest(manifest) == runtime["source_inputs_sha256"]
    inputs = audit.read(manifest)
    repo = Path(__file__).resolve().parent.parent
    frozen = subprocess.check_output(["git", "archive", runtime["source_commit"]], cwd=repo)
    checked = set()
    with tarfile.open(fileobj=io.BytesIO(frozen)) as archive:
        for item in archive:
            if item.name not in inputs:
                continue
            payload = item.linkname.encode() if item.issym() else archive.extractfile(item).read()
            assert hashlib.sha256(payload).hexdigest() == inputs[item.name], item.name
            checked.add(item.name)
    assert checked == set(inputs)
    out = root / "output"
    fork = audit.read(out / "fork.json")
    prior = next(r["state"] for r in audit.rows(reference / "control-samples.jsonl")
                 if r["tick"] == runtime["fork_tick"])
    assert fork == prior, "fork differs from retained sample"
    p = audit.pilot(fork, 0)
    assert p["balanced"] and p["queries_ready"]
    assert p["location"] == "on_foot" and p["supported_planet"] == p["planet"]["index"]
    posture = fork["pilots"][0]["recovery"]["posture"]
    assert posture["stable"] and posture["standing_clear"]
    retained = {}
    for suffix, expected in (("actions.jsonl", 32400), ("samples.jsonl", 540)):
        new = itertools.chain(audit.rows(out / f"prefix-{suffix}"),
                              audit.rows(out / "blocked" / f"run-{suffix}"))
        end = 32400 if suffix == "samples.jsonl" else 32399
        new = (r for r in new if r["tick"] <= end)
        count = 0
        for a, b in itertools.zip_longest(new, audit.rows(reference / f"control-{suffix}")):
            assert a is not None and b is not None, (suffix, "length")
            assert a == b, (suffix, a["tick"], "retained control differs")
            count += 1
        assert count == expected, (suffix, count)
        retained[suffix] = count
    arms = []
    for name in ("blocked", "existing", "search-arrival", "full"):
        path = out / name
        summary = audit.read(path / "summary.json")
        before = audit.read(path / "before-first-decision.json")
        assert before["scenario"] == fork["scenario"], name
        assert before["brains"] == fork["brains"], name  # cached telemetry, before stepping
        decision = audit.read(path / "first-decision.json")
        trace = summary["trace"]
        comparison = audit.stream_difference(out / "blocked/run-actions.jsonl", path / "run-actions.jsonl")
        assert comparison["ticks"] == runtime["continuation_ticks"]
        first_block = None
        if trace["first_block"][0] is not None:
            state = audit.read(path / "first-block.json")
            pilot = audit.pilot(state, 0)
            assert pilot["tick"] == trace["first_block"][0][0]
            first_block = {"tick": pilot["tick"], "pilot": pilot,
                           "posture": state["pilots"][0]["recovery"]["posture"],
                           "brain": state["brains"][0]}
        completion = None
        if trace["first_completion"][0] is not None:
            state = audit.read(path / "first-completion.json")
            pilot = audit.pilot(state, 0)
            assert pilot["tick"] == trace["first_completion"][0]
            assert pilot["location"] == {"aboard": 0} and pilot["ship_form"] == "ship"
            assert pilot["landing"]["phase"] == "landed" and pilot["landing"]["supported_feet"] == 2
            assert state["brains"][0]["recovery"]["status"] == "succeeded"
            completion = {"pilot": pilot, "brain": state["brains"][0]}
        features = audit.feature_receipt(path / "candidate-features.jsonl") if summary["profile"] else None
        sample_count = placement_differences = 0
        claim_transitions = []
        previous_claim = None
        for a, b in itertools.zip_longest(audit.rows(out / "blocked/run-samples.jsonl"),
                                         audit.rows(path / "run-samples.jsonl")):
            assert a is not None and b is not None and a["tick"] == b["tick"]
            left, right = copy.deepcopy(a["state"]["scenario"]), copy.deepcopy(b["state"]["scenario"])
            placement_differences += int(left != right)
            for state in (left, right):
                # Refinement expands native placement attempts; failure status
                # may change with the extra offsets even without construction.
                state["players"][0]["recovery"].pop("placement", None)
                state["players"][0]["recovery"].pop("status", None)
            assert left == right, (name, a["tick"], "world differs beyond native placement diagnostics")
            assert a["state"]["audit"] == b["state"]["audit"]
            sample_count += 1
            claim = audit.pilot(b["state"], 0)["planet"]["claim"]
            key = tuple(claim[k] for k in ("owner", "phase", "status", "captures", "neutralizations"))
            if key != previous_claim:
                claim_transitions.append({"tick": b["tick"], "claim": claim})
                previous_claim = key
        assert sample_count == 300
        events = [r for r in audit.rows(path / "run-events.jsonl") if r["seat"] == 0]
        progress = [{"tick": r["tick"], "task": r["brain"]["recovery"],
                     "claim": r["observation"]["recovery"]["flight"]["pilot"]["planet"]["claim"]}
                    for r in events]
        result = {"arm": name, "summary": summary, "comparison_to_blocked": comparison,
                  "first_decision_brain": decision["brains"][0], "first_block": first_block,
                  "completion": completion, "features": features, "progress_events": progress,
                  "equal_world_samples_except_subject_placement_and_status": sample_count,
                  "samples_with_native_placement_differences": placement_differences,
                  "sampled_claim_transitions": claim_transitions}
        arms.append(result)
        print(json.dumps({"arm": name, "completion": trace["first_completion"][0],
                          "block": trace["first_block"][0], "new_loss": trace["first_new_loss"][0],
                          "changed_actions": comparison["different_ticks"]["actions"],
                          "features": summary["features"]}))
    receipts = {str(p.relative_to(root)): {"bytes": p.stat().st_size, "sha256": audit.digest(p)}
                for p in sorted(out.rglob("*")) if p.is_file()}
    report = {"schema": 1, "runtime": runtime, "reference": str(reference),
              "retained_control_rows": retained, "fork_route": audit.read(out / "fork-route.json"),
              "arms": arms, "artifact_receipts": receipts,
              "auditor_sha256": audit.digest(Path(__file__)), "helper_sha256": audit.digest(helper)}
    (root / "audit.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"Verified {len(inputs)} frozen inputs, {retained} retained rows and {len(receipts)} evidence files.")


if __name__ == "__main__":
    main()
