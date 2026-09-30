#!/usr/bin/env python3
"""Replay the five recorded flag studies with captured local dependencies."""
import argparse
from collections import Counter
import gzip
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess

SPEC = importlib.util.spec_from_file_location("geometry", Path(__file__).with_name("investigate-flag-geometry.py"))
G = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(G)
F = G.F


def validate_local(samples, telemetry):
    rows = [s for s in samples if s.get("validation") is not None]
    assert len(rows) == telemetry["local_checks"]
    assert sum(s["validation"]["geometry"]["area_tests"] for s in rows) == telemetry["local_area_tests"]
    rescued, withheld = [], []
    for s in rows:
        v = s["validation"]
        assert v["model"] == "captured_query_unions_v1"
        assert len(v["source_areas"]) <= 5
        assert 0 < v["captured_queries"] <= s["measurement"]["queries"] <= 192
        assert 0 < v["walking_queries"] <= s["physics_queries"] - s["measurement"]["queries"]
        assert v["captured_queries"] <= 1024 and v["walking_queries"] <= 1024
        assert v["predicates_valid"] == (v["predicate_failure"] is None)
        valid = v["complete"] and v["predicates_valid"] and v["geometry"]["valid"]
        assert valid == (s["reason"] is None)
        assert (s["validated_tick"] == s["completed_tick"]) if valid else (s["validated_tick"] is None)
        if valid:
            assert s["completed_tick"] - s["source_tick"] <= 1800
            assert s["source_tick"] == s["measurement"]["tick"]
        if valid and s.get("geometry") is not None:
            rescued.append(s)
        if not valid and s.get("geometry") is None:
            withheld.append(s)
    assert all(s.get("validation") is not None for s in samples if s["reason"] is None)
    assert len(rescued) == telemetry["local_rescued"]
    assert len(withheld) == telemetry["local_withheld"]
    return dict(checks=len(rows), rescued=rescued, withheld=withheld,
                predicate_failures=dict(Counter(s["validation"]["predicate_failure"] for s in rows
                                               if not s["validation"]["predicates_valid"])),
                samples=rows,
                scope="Historical landing/walking evidence; cover, optimality, swept flight and live action permissions are not validated.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/surface_mission_soak"))
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    reference = json.loads((opts.reference / "summary.json").read_text())
    plan = [r for r in F.plan() if r["mode"] == "on"]
    assert reference["plan"] == plan and len(reference["runs"]) == 5
    opts.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=plan, runs={},
                  scope="Engineering replays, not fresh strength data. Controls, evaluator, physical outcomes and per-tick work must be exact; only publication decisions and their diagnostics may change.",
                  source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                  source_dirty=bool(subprocess.check_output(["git", "status", "--porcelain"], text=True).strip()),
                  binary_sha256=F.digest(opts.binary), reference_summary_sha256=F.digest(opts.reference / "summary.json"),
                  reference_source=reference["source_commit"])
    F.D.write(opts.out / "summary.json", result)
    for item in plan:
        name = item["name"]
        args = ["--world", "generated", "--seed", str(item["seed"]), "--mode", "duel", "--match", "true", "--seat", "0",
                "--asteroid-interval", str(item["interval"]), "--p1-policy", f'material_mission_v{item["policies"][0]}',
                "--p2-policy", f'material_mission_v{item["policies"][1]}', "--trace", "true", "--trace-start-tick", "0",
                "--trace-end-tick", "36002", "--survey-capture-flags", "true"]
        run = F.D.run(opts.binary, opts.out, name, args, 1, seconds=600, require_finish=True)
        old_path, path = opts.reference / name, opts.out / name
        old_report, report = [json.loads((p / "report.json").read_text()) for p in [old_path, path]]
        assert F.digest(old_path / "report.json") == reference["runs"][name]["report_sha256"]
        assert F.D.same_physical_outcomes(old_report, report)
        assert old_report["missions"] == report["missions"]
        for key, file in [("controller_trace_sha256", "trace.jsonl"), ("evaluation_sha256", "mission-evaluations.jsonl")]:
            run[key] = F.digest(path / file)
            assert run[key] == reference["runs"][name][key], (name, key)
        run["work_sha256"] = F.digest(path / "flag-survey-work.jsonl")
        assert run["work_sha256"] == F.digest(old_path / "flag-survey-work.jsonl")
        samples, old_samples = [[json.loads(line) for line in (p / "flag-survey.jsonl").read_text().splitlines()]
                                for p in [path, old_path]]
        # Full original measurements/routes and rejection diagnostics remain
        # exact; publication decision/timestamp and new local evidence may differ.
        stable = lambda rows: [{k: v for k, v in s.items() if k not in {"reason", "validated_tick", "validation"}} for s in rows]
        assert stable(samples) == stable(old_samples), name
        telemetry = report["flag_survey"]["telemetry"]
        for key, value in old_report["flag_survey"]["telemetry"].items():
            if not key.endswith("_ms") and key not in {"walking_round_trips", "unknown"}:
                assert telemetry[key] == value, (name, key)
        run["local_validation"] = validate_local(samples, telemetry)
        run["diagnostics"] = G.validate_diagnostics(samples, telemetry, local_validation=True)
        run["coverage"] = F.coverage(path)
        run["flag_survey"] = report["flag_survey"]
        run["old_positive_count"] = sum(s["reason"] is None for s in old_samples)
        for kind in ["graph", "physics_queries"]:
            assert run["coverage"]["flag_work"][kind] == telemetry[kind]
        run["exact_controls_evaluation_physics_measurements_and_work"] = True
        with (path / "trace.jsonl").open("rb") as source, gzip.open(path / "trace.jsonl.gz", "wb", compresslevel=3) as target:
            shutil.copyfileobj(source, target)
        (path / "trace.jsonl").unlink()
        result["runs"][name] = run
        F.D.write(opts.out / "summary.json", result)
    assert F.digest(opts.binary) == result["binary_sha256"]


if __name__ == "__main__":
    main()
