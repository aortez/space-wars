#!/usr/bin/env python3
"""Audit retained three-arm results; never execute or retry a simulation."""
import argparse
from collections import Counter
import hashlib
import itertools
import json
from pathlib import Path


def read(path):
    return json.loads(path.read_text())


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def rows(path):
    with path.open() as stream:
        for line in stream:
            yield json.loads(line)


def stream_difference(left, right):
    first = {k: None for k in ("actions", "sites", "decision_fnv64", "scenario_before_step_fnv64")}
    counts = {k: 0 for k in first}
    count = 0
    for a, b in itertools.zip_longest(rows(left), rows(right)):
        assert a is not None and b is not None, (left, right, "length")
        assert a["tick"] == b["tick"]
        for key in first:
            if a[key] != b[key]:
                counts[key] += 1
                if first[key] is None:
                    first[key] = a["tick"]
        count += 1
    return {"ticks": count, "first_difference": first, "different_ticks": counts}


def pilot(state, seat):
    return state["pilots"][seat]["recovery"]["flight"]["pilot"]


def feature_receipt(path):
    counts = Counter()
    reasons = Counter()
    max_work = Counter()
    work_by_tick = Counter()
    forecasts = []
    normals = []
    for row in rows(path):
        kind = row["kind"]
        counts[kind] += 1
        if kind == "survey":
            work = row["survey"].get("refinement") or {}
            for name, limit in (("coarse_candidates", 8), ("refined_candidates", 8),
                                ("offset_checks", 240), ("staging_route_checks", 16)):
                n = work.get(name, 0)
                assert n <= limit, (row["tick"], name, n)
                max_work[name] = max(max_work[name], n)
        elif kind in ("preview_normal", "forecast_selection"):
            event = row["event"]
            counts[kind + ":" + event["kind"]] += 1
            if event.get("reason") is not None:
                reasons[kind + ":" + event["reason"]] += 1
            if kind == "preview_normal":
                normals.append(event)
            elif event["kind"] == "work":
                work_by_tick[event["tick"]] += event["steps"]
                assert work_by_tick[event["tick"]] <= 4
            elif event["kind"] in ("started", "evaluated", "invalidated", "cancelled", "exhausted"):
                forecasts.append(event)
    return {"events": dict(counts), "reasons": dict(reasons), "max_survey_work": dict(max_work),
            "forecast_steps": sum(work_by_tick.values()), "max_forecast_steps_per_tick": max(work_by_tick.values(), default=0),
            "normal_events": normals, "forecast_events": forecasts}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("package", type=Path)
    args = parser.parse_args()
    root, package = args.root.resolve(), args.package.resolve()
    runtime = read(root / "candidate-result.json")
    assert runtime["exit_code"] == 0 and runtime["case_count"] == 4
    assert digest(root / "candidate-comparison") == runtime["binary_sha256"]
    assert digest(root / "candidate-source-inputs.json") == runtime["source_inputs_sha256"]
    source_inputs = read(root / "candidate-source-inputs.json")
    for name, expected in source_inputs.items():
        path = root / "combined" / name
        actual = hashlib.sha256(str(path.readlink()).encode()).hexdigest() if path.is_symlink() else digest(path)
        assert actual == expected, ("frozen runtime input changed", name)
    results = []
    for reference in sorted((package / "cases").iterdir()):
        name = reference.name
        baseline = root / "combined-controls-output" / name
        trial = root / "candidate-output" / name
        summary = read(trial / "summary.json")
        seat = summary["subject_seat"]
        retained = []
        # The adapter and candidate arm cannot perturb the other two arms.
        for arm in ("control", "reset"):
            for suffix in ("actions.jsonl", "samples.jsonl", "events.jsonl"):
                filename = f"{arm}-{suffix}"
                assert digest(baseline / filename) == digest(trial / filename), (name, filename)
                retained.append(filename)
        for filename in ("fork.json", "blocking-decision.json", "reset-before-first-decision.json",
                         "first-control-decision.json", "first-reset-decision.json"):
            assert digest(baseline / filename) == digest(trial / filename), (name, filename)
            retained.append(filename)
        comparison = stream_difference(trial / "reset-actions.jsonl", trial / "candidate-actions.jsonl")
        assert comparison["ticks"] == 18000
        assert comparison["first_difference"]["actions"] == summary["first_candidate_action_difference_from_fresh_control"]
        old_comparisons = {arm: stream_difference(reference / f"{arm}-actions.jsonl", baseline / f"{arm}-actions.jsonl")
                           for arm in ("control", "reset")}
        final = read(trial / "candidate-final.json")
        completion = summary["candidate"]["first_completion"][seat]
        receipt = None
        if completion is not None:
            state = read(trial / "candidate-first-completion.json")
            p = pilot(state, seat)
            b = state["brains"][seat]
            assert p["tick"] == completion
            assert p["location"] == {"aboard": seat}
            assert p["ship_form"] == "ship"
            assert p["landing"]["phase"] == "landed"
            assert p["landing"]["supported_feet"] == 2
            assert b["recovery"]["status"] == "succeeded"
            receipt = {"tick": completion, "landing": p["landing"], "native_recovery": p["recovery"],
                       "ship_health": p["ship_health"], "task": b["recovery"]}
        features = feature_receipt(trial / "candidate-features.jsonl")
        assert features["events"].get("preview_normal:captured", 0) == summary["features"]["captures"]
        assert features["events"].get("preview_normal:arrived", 0) == summary["features"]["arrivals"]
        sample_landings = []
        if completion is not None:
            built = receipt["task"]["rebuilt_tick"]
            for sample in rows(trial / "candidate-samples.jsonl"):
                if built <= sample["tick"] <= completion:
                    p = pilot(sample["state"], seat)
                    if p["landing"]["phase"] == "landed":
                        sample_landings.append({"tick": sample["tick"], "landing": p["landing"]})
        results.append({"case": name, "summary": summary, "same_runtime_control_files_retained": retained,
                        "fresh_candidate_comparison": comparison, "original_to_combined_controls": old_comparisons,
                        "features": features, "completion_receipt": receipt,
                        "first_sampled_landed_after_rebuild": sample_landings[0] if sample_landings else None,
                        "final_pilot": pilot(final, seat), "final_brain": final["brains"][seat],
                        "complete_without_new_subject_loss": completion is not None and summary["candidate"]["first_new_loss"][seat] is None})
    assert len(results) == 4
    receipts = {}
    for directory in (root / "reproduced", root / "combined-controls-output", root / "candidate-output"):
        for path in sorted(directory.rglob("*")):
            if path.is_file():
                receipts[str(path.relative_to(root))] = {"bytes": path.stat().st_size, "sha256": digest(path)}
    report = {"schema": 1, "runtime": runtime, "archive_sha256": "cc8df0c516ab948690ce0ba704745266b4600705544e2acec5fac63ee86750e9",
              "production_retry_policy_changed": False, "cases": results,
              "qualified_case_count": sum(r["complete_without_new_subject_loss"] for r in results),
              "total_case_count": len(results), "artifact_receipts": receipts,
              "auditor_sha256": digest(Path(__file__))}
    (root / "comparison-audit.json").write_text(json.dumps(report, indent=2) + "\n")
    for result in results:
        s = result["summary"]; seat = s["subject_seat"]
        print(json.dumps({"case": result["case"], "completion": s["candidate"]["first_completion"][seat],
                          "new_subject_loss": s["candidate"]["first_new_loss"][seat],
                          "first_action_difference": s["first_candidate_action_difference_from_fresh_control"],
                          "features": s["features"]}))
    print(f"Verified {len(receipts)} retained artifacts; {report['qualified_case_count']}/{len(results)} cases complete without a new subject loss.")


if __name__ == "__main__":
    main()
