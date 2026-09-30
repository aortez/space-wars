#!/usr/bin/env python3
"""Replay the five recorded enabled surveys; diagnose overlaps without retuning."""
import argparse
from collections import Counter
import gzip
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess

SPEC = importlib.util.spec_from_file_location("flags", Path(__file__).with_name("compare-flag-surveys.py"))
F = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(F)


def validate_diagnostics(samples, telemetry, *, local_validation=False):
    # Local publication can rescue a circular rejection or reject an area
    # outside that circle. Preserve the old strict contract for old studies.
    rejected = [s for s in samples if s.get("geometry") is not None] if local_validation else [
        s for s in samples if s["reason"] == "geometry changed since source measurement"]
    assert all(s.get("geometry") is None for s in samples if s not in rejected)
    assert len(rejected) == telemetry["geometry_diagnostics"]
    stage_counts, kinds = Counter(), Counter()
    reports = []
    for sample in rejected:
        geometry = sample["geometry"]
        assert geometry is not None
        report = geometry["report"]
        assert geometry["model"] == "source_envelope_overlaps_v1"
        assert not geometry["acceptance_prefix"]["valid"] or geometry["material_queries_dirty"]
        assert len(report["changes"]) <= 8
        assert len(geometry["envelopes"]) <= 8
        assert sum(c["unsupported_tests"] for c in report["changes"]) <= report["unsupported_tests"]
        if report["omitted_changes"] == 0:
            assert sum(c["unsupported_tests"] for c in report["changes"]) == report["unsupported_tests"]
        if report["complete"]:
            assert report["unavailable"] is None
            assert report["region_changes"] == len(report["changes"]) + report["omitted_changes"]
            assert report["region_changes"] > 0 or geometry["material_queries_dirty"]
            assert len(report["area_changes"]) == len(geometry["envelopes"])
            assert report["changed_colliders"] >= report["region_changes"]
            for i, area in enumerate(geometry["envelopes"]):
                assert 0 <= report["area_changes"][i] <= report["region_changes"]
                count = sum(i in c["areas"] for c in report["changes"])
                assert count <= report["area_changes"][i] <= count + report["omitted_changes"]
                stage_counts[area["name"]] += report["area_changes"][i]
        else:
            assert report["unavailable"] is not None
        for change in report["changes"]:
            assert change["previous"] is not None or change["current"] is not None
            assert len(change["areas"]) == len(set(change["areas"]))
            assert all(0 <= i < len(geometry["envelopes"]) for i in change["areas"])
            ids = {p["collider"]["entity"] for p in [change["previous"], change["current"]]
                   if p is not None and p["collider"] is not None}
            for entity in ids:
                kinds[geometry["entity_kinds"][str(entity)]] += 1
        reports.append(report)
    for field, key in [("area_tests", "diagnostic_area_tests"), ("region_changes", "diagnostic_region_changes"),
                       ("omitted_changes", "diagnostic_omitted_changes")]:
        assert sum(r[field] for r in reports) == telemetry[key]
    assert sum(not r["complete"] for r in reports) == telemetry["diagnostic_incomplete"]
    return dict(rejections=rejected, potential_stage_overlaps=dict(stage_counts),
                retained_collider_kinds=dict(kinds),
                scope="Counts are collider-envelope overlaps, not blocked routes or independent decisions. Kinds cover retained details only.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/surface_mission_soak"))
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    reference = json.loads((opts.reference / "summary.json").read_text())
    assert reference["plan"] == F.plan()
    assert len(reference["runs"]) == 11 and len(reference["comparisons"]) == 6
    opts.out.mkdir(parents=True, exist_ok=False)
    results = dict(schema=1, plan=[r for r in F.plan() if r["mode"] == "on"], runs={},
                   scope="Diagnostic engineering replays of recorded worlds, not fresh strength trials; acceptance and allocations must stay identical.",
                   source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                   source_dirty=bool(subprocess.check_output(["git", "status", "--porcelain"], text=True).strip()),
                   binary_sha256=F.digest(opts.binary), reference_summary_sha256=F.digest(opts.reference / "summary.json"),
                   reference_source=reference["source_commit"])
    F.D.write(opts.out / "summary.json", results)
    for item in results["plan"]:
        name = item["name"]
        args = ["--world", "generated", "--seed", str(item["seed"]), "--mode", "duel",
                "--match", "true", "--seat", "0", "--asteroid-interval", str(item["interval"]),
                "--p1-policy", f'material_mission_v{item["policies"][0]}',
                "--p2-policy", f'material_mission_v{item["policies"][1]}',
                "--trace", "true", "--trace-start-tick", "0", "--trace-end-tick", "36002",
                "--survey-capture-flags", "true"]
        run = F.D.run(opts.binary, opts.out, name, args, 1, seconds=600, require_finish=True)
        old_path, path = opts.reference / name, opts.out / name
        old_report, report = [json.loads((p / "report.json").read_text()) for p in [old_path, path]]
        assert F.digest(old_path / "report.json") == reference["runs"][name]["report_sha256"]
        assert F.D.same_physical_outcomes(old_report, report)
        assert old_report["missions"] == report["missions"]
        run["controller_trace_sha256"] = F.digest(path / "trace.jsonl")
        run["evaluation_sha256"] = F.digest(path / "mission-evaluations.jsonl")
        for key in ["controller_trace_sha256", "evaluation_sha256"]:
            assert run[key] == reference["runs"][name][key], (name, key)
        assert F.digest(path / "flag-survey-work.jsonl") == F.digest(old_path / "flag-survey-work.jsonl")
        samples = [json.loads(line) for line in (path / "flag-survey.jsonl").read_text().splitlines()]
        old_samples = [json.loads(line) for line in (old_path / "flag-survey.jsonl").read_text().splitlines()]
        assert [{k: v for k, v in s.items() if k != "geometry"} for s in samples] == old_samples
        telemetry = report["flag_survey"]["telemetry"]
        for key, value in old_report["flag_survey"]["telemetry"].items():
            if not key.endswith("_ms"):
                assert telemetry[key] == value, (name, key)
        run["diagnostics"] = validate_diagnostics(samples, telemetry)
        run["coverage"] = F.coverage(path)
        run["flag_survey"] = report["flag_survey"]
        for kind in ["graph", "physics_queries"]:
            assert run["coverage"]["flag_work"][kind] == telemetry[kind]
        run["exact_controls_evaluation_physics_samples_and_work"] = True
        with (path / "trace.jsonl").open("rb") as source, gzip.open(path / "trace.jsonl.gz", "wb", compresslevel=3) as target:
            shutil.copyfileobj(source, target)
        (path / "trace.jsonl").unlink()
        results["runs"][name] = run
        F.D.write(opts.out / "summary.json", results)
    assert F.digest(opts.binary) == results["binary_sha256"]


if __name__ == "__main__":
    main()
