#!/usr/bin/env python3
"""Audit automatic recovery, its preserved controls and bounded retry receipt."""
import argparse
import hashlib
import importlib.util
import io
import itertools
import json
from pathlib import Path
import subprocess
import struct
import tarfile


def f32_values(value):
    # Snapshot JSON passes through Value (f64), whereas Decision writes f32
    # directly. Check exact source float bits rather than textual rounding.
    if isinstance(value, float):
        return ("f32", struct.pack("<f", value).hex())
    if isinstance(value, dict):
        return {k: f32_values(v) for k, v in value.items()}
    if isinstance(value, list):
        return [f32_values(v) for v in value]
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("reference", type=Path)
    parser.add_argument("components", type=Path)
    args = parser.parse_args()
    helper = Path(__file__).with_name("audit-terrain-recovery-integration.py")
    spec = importlib.util.spec_from_file_location("integration_audit", helper)
    audit = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(audit)
    root = args.root.resolve()
    runtime = audit.read(root / "result.json")
    assert runtime["exit_code"] == 0 and runtime["case_count"] == 4
    assert audit.digest(root / "automatic-comparison") == runtime["binary_sha256"]
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
            content = item.linkname.encode() if item.issym() else archive.extractfile(item).read()
            assert hashlib.sha256(content).hexdigest() == inputs[item.name], item.name
            checked.add(item.name)
    assert checked == set(inputs)
    cases = []
    for path in sorted((root / "output").iterdir()):
        summary = audit.read(path / "summary.json")
        seat = summary["subject_seat"]
        old = args.reference / path.name
        retained = []
        for name in ("control-actions.jsonl", "control-samples.jsonl", "control-events.jsonl", "fork.json"):
            assert audit.digest(path / name) == audit.digest(old / name), (path.name, name)
            retained.append(name)
        fork = audit.read(path / "fork.json")
        original = fork["brains"][seat]["recovery"]
        first = audit.read(path / "first-automatic-decision.json")["brains"][seat]["recovery"]
        for key in ("started_tick", "landing_retries", "landing_rejections", "relocations", "scuttle_attempts"):
            assert f32_values(original[key]) == f32_values(first[key]), (path.name, key)
        review = first.get("pod_reconsideration")
        if review:
            assert review["blocked_tick"] == original["goal_since"]
            assert review["surveys"] <= 15
            assert review["previous_reason"] == original["reason"]
            if review["started_tick"] is not None:
                assert review["blocked_tick"] < review["started_tick"] <= review["blocked_tick"] + 900
                assert review["deadline_tick"] == review["started_tick"] + 5400
                assert review["alternative"]["id"] != review["abandoned_site"]["id"]
        control = (r for r in audit.rows(path / "control-actions.jsonl") if r["tick"] >= summary["fork_tick"])
        changed = []
        count = 0
        for left, right in itertools.zip_longest(control, audit.rows(path / "automatic-actions.jsonl")):
            assert left is not None and right is not None
            assert left["tick"] == right["tick"]
            count += 1
            if left["actions"] != right["actions"]:
                changed.append(left["tick"])
        assert count == 18000
        assert (changed[0] if changed else None) == summary["first_action_difference"]
        completion = summary["automatic"]["first_completion"][seat]
        receipt = None
        if completion is not None:
            state = audit.read(path / "automatic-first-completion.json")
            pilot = audit.pilot(state, seat)
            task = state["brains"][seat]["recovery"]
            assert pilot["tick"] == completion
            assert pilot["location"] == {"aboard": seat} and pilot["ship_form"] == "ship"
            assert pilot["landing"]["phase"] == "landed" and pilot["landing"]["supported_feet"] == 2
            assert task["status"] == "succeeded"
            assert task["started_tick"] == original["started_tick"]
            assert task["landing_rejections"][:len(original["landing_rejections"])] == original["landing_rejections"]
            assert task["landing_retries"] >= original["landing_retries"]
            assert task["relocations"] <= 4 and task["scuttle_attempts"] == original["scuttle_attempts"]
            assert f32_values(task["pod_reconsideration"]) == f32_values(review)
            receipt = {"pilot": pilot, "brain": state["brains"][seat]}
        final = audit.read(path / "automatic-final.json")
        paired = None
        if path.name == "angular-pod-immediate":
            assert summary["from_tick_zero_prefix_verified"]
            paired = audit.stream_difference(args.components / "output/search-arrival/run-actions.jsonl", path / "automatic-actions.jsonl")
        result = {"case": path.name, "summary": summary, "retained_control_files": retained,
                  "changed_action_ticks": len(changed), "review": review,
                  "features": audit.feature_receipt(path / "automatic-features.jsonl"),
                  "completion_receipt": receipt, "fresh_search_comparison": paired,
                  "final_brain": final["brains"][seat], "final_pilot": audit.pilot(final, seat)}
        cases.append(result)
        print(json.dumps({"case": path.name, "completion": completion,
                          "new_loss": summary["automatic"]["first_new_loss"][seat],
                          "review_started": review and review["started_tick"],
                          "changed_action_ticks": len(changed)}))
    receipts = {str(p.relative_to(root)): {"bytes": p.stat().st_size, "sha256": audit.digest(p)}
                for p in sorted((root / "output").rglob("*")) if p.is_file()}
    (root / "audit.json").write_text(json.dumps({"schema": 1, "runtime": runtime, "cases": cases,
        "artifact_receipts": receipts, "auditor_sha256": audit.digest(Path(__file__)),
        "helper_sha256": audit.digest(helper)}, indent=2) + "\n")
    print(f"Verified {len(inputs)} source inputs and {len(receipts)} evidence files.")


if __name__ == "__main__":
    main()
