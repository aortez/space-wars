#!/usr/bin/env python3
"""Read the fixed component comparison; never run or retry a simulation."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess


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
    assert runtime["exit_code"] == 0
    assert audit.digest(root / "components-comparison") == runtime["binary_sha256"]
    manifest = root / "source-inputs.json"
    assert audit.digest(manifest) == runtime["source_inputs_sha256"]
    # Git retains the frozen source even when subsequent experiments are added.
    repo = Path(__file__).resolve().parent.parent
    frozen = subprocess.check_output(["git", "archive", runtime["source_commit"]], cwd=repo)
    import io
    import tarfile
    inputs = audit.read(manifest)
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
    assert audit.digest(out / "fork.json") == audit.digest(reference / "fork.json")
    retained = []
    for arm, previous in (("existing", "reset"), ("full", "candidate")):
        for suffix in ("actions.jsonl", "samples.jsonl", "events.jsonl"):
            path = out / arm / f"run-{suffix}"
            old = reference / f"{previous}-{suffix}"
            assert audit.digest(path) == audit.digest(old), (arm, suffix)
            retained.append(str(path.relative_to(root)))
        for path, old in (("before-first-decision.json", f"{previous}-before-first-decision.json"),
                          ("first-decision.json", f"first-{previous}-decision.json"),
                          ("final.json", f"{previous}-final.json")):
            assert audit.digest(out / arm / path) == audit.digest(reference / old), (arm, path)
            retained.append(str((out / arm / path).relative_to(root)))
    summaries = []
    for arm in ("existing", "search-arrival", "placement-forecast", "full"):
        path = out / arm
        summary = audit.read(path / "summary.json")
        trace = summary["trace"]
        comparison = audit.stream_difference(out / "existing/run-actions.jsonl", path / "run-actions.jsonl")
        assert comparison["ticks"] == 18000
        completion = trace["first_completion"][1]
        receipt = None
        if completion is not None:
            state = audit.read(path / "first-completion.json")
            pilot = audit.pilot(state, 1)
            assert pilot["tick"] == completion
            assert pilot["location"] == {"aboard": 1} and pilot["ship_form"] == "ship"
            assert pilot["landing"]["phase"] == "landed" and pilot["landing"]["supported_feet"] == 2
            assert state["brains"][1]["recovery"]["status"] == "succeeded"
            receipt = {"pilot": pilot, "brain": state["brains"][1]}
        features = audit.feature_receipt(path / "candidate-features.jsonl") if arm != "existing" else None
        summaries.append({"arm": arm, "summary": summary, "comparison_to_existing": comparison,
                          "features": features, "completion_receipt": receipt})
        print(json.dumps({"arm": arm, "completion": completion, "block": trace["first_block"][1],
                          "new_loss": trace["first_new_loss"][1],
                          "first_action_difference": comparison["first_difference"]["actions"],
                          "features": summary["features"]}))
    receipts = {str(p.relative_to(root)): {"bytes": p.stat().st_size, "sha256": audit.digest(p)}
                for p in sorted(out.rglob("*")) if p.is_file()}
    report = {"schema": 1, "runtime": runtime, "retained_files": retained,
              "arms": summaries, "artifact_receipts": receipts,
              "auditor_sha256": audit.digest(Path(__file__)), "helper_sha256": audit.digest(helper)}
    (root / "audit.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"Verified {len(inputs)} frozen inputs, {len(retained)} retained controls and {len(receipts)} evidence files.")


if __name__ == "__main__":
    main()
