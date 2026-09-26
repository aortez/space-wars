#!/usr/bin/env python3
"""Predeclared observational flag-survey trials; require exact controller parity."""
import argparse
from collections import Counter
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess

SPEC = importlib.util.spec_from_file_location(
    "destinations", Path(__file__).with_name("compare-capture-destinations.py"))
D = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(D)


def plan():
    rows = [dict(name=f"regression-{mode}", group="regression", mode=mode,
                 seed=186767996776005237, interval=0, policies=[10, 13])
            for mode in ["frozen", "off", "on"]]
    for world in range(2):
        seed = int.from_bytes(hashlib.sha256(f"native-flag-survey-v1:{world}".encode()).digest()[:8], "little")
        for interval in [0, 3]:
            group = f"world{world}-asteroids{interval}"
            for mode in (["off", "on"] if world == 0 else ["on", "off"]):
                rows.append(dict(name=f"{group}-{mode}", group=group, mode=mode,
                                 seed=seed, interval=interval, policies=[13, 13]))
    return rows


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def charge(row):
    remaining, report = row["remaining_after_evaluation"], row["allocation"]
    for kind, cap in [("graph", 4), ("physics_queries", 384)]:
        assert 0 <= remaining[kind] <= cap
        used = report["charged"][kind]
        assert 0 <= used <= remaining[kind]
        assert used == sum(j["charged"][kind] for j in report["jobs"])
    assert report["charged"]["graph"] <= 2
    assert all(j["charged"]["graph"] <= 1 for j in report["jobs"])
    return {kind: cap - remaining[kind] + report["charged"][kind]
            for kind, cap in [("graph", 4), ("physics_queries", 384)]}


def coverage(path):
    samples = [json.loads(line) for line in (path / "flag-survey.jsonl").open()]
    telemetry = json.loads((path / "report.json").read_text())["flag_survey"]["telemetry"]
    validate_sample_counts(samples, telemetry)
    keys = [(s["actor"], s["source_tick"], s["site"]["bearing"]) for s in samples]
    assert len(keys) == len(set(keys))
    for sample in samples:
        assert sample["measurement"]["tick"] == sample["source_tick"] <= sample["completed_tick"]
        if sample["reason"] is None:
            assert sample["validated_tick"] == sample["completed_tick"]
            assert sample["completed_tick"] - sample["source_tick"] <= 1800
            for route in [sample["route"]["outbound"], sample["route"]["returning"]]:
                assert route["failure"] is None and not route["partial"]
                assert route["jumps"] == route["flights"] == 0
    maxima = dict(graph=0, physics_queries=0)
    totals = dict(graph=0, physics_queries=0)
    for row in map(json.loads, (path / "flag-survey-work.jsonl").open()):
        used = charge(row)
        for kind in maxima:
            maxima[kind] = max(maxima[kind], used[kind])
            totals[kind] += row["allocation"]["charged"][kind]
    return dict(samples=len(samples), positive=sum(s["reason"] is None for s in samples),
                unknown=dict(Counter(s["reason"] for s in samples if s["reason"])),
                maximum_combined_work=maxima, flag_work=totals,
                positive_samples=[s for s in samples if s["reason"] is None])


def validate_sample_counts(samples, telemetry):
    assert len(samples) == telemetry["completed"]
    assert sum(s["reason"] is None for s in samples) == telemetry["walking_round_trips"]
    assert dict(Counter(s["reason"] for s in samples if s["reason"])) == telemetry["unknown"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/surface_mission_soak"))
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    opts.baseline = opts.baseline.resolve(strict=True)
    opts.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=plan(), runs={}, comparisons=[],
                  scope="Observational on/off pairs, two fresh worlds and two pressure levels; frozen v13 regression retained. No behavior or strength change expected.",
                  source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                  source_dirty=bool(subprocess.check_output(["git", "status", "--porcelain"], text=True).strip()),
                  binaries={"new": digest(opts.binary), "frozen": digest(opts.baseline)})
    D.write(opts.out / "summary.json", result)
    for item in result["plan"]:
        args = ["--world", "generated", "--seed", str(item["seed"]), "--mode", "duel",
                "--match", "true", "--seat", "0", "--asteroid-interval", str(item["interval"]),
                "--p1-policy", f'material_mission_v{item["policies"][0]}',
                "--p2-policy", f'material_mission_v{item["policies"][1]}',
                "--trace", "true", "--trace-start-tick", "0", "--trace-end-tick", "36002"]
        if item["mode"] != "frozen":
            args += ["--survey-capture-flags", str(item["mode"] == "on").lower()]
        binary = opts.baseline if item["mode"] == "frozen" else opts.binary
        run = D.run(binary, opts.out, item["name"], args, 1, seconds=600, require_finish=True)
        path = opts.out / item["name"]
        report = json.loads((path / "report.json").read_text())
        run["evaluation_sha256"] = digest(path / "mission-evaluations.jsonl")
        run["controller_trace_sha256"] = digest(path / "trace.jsonl")
        with (path / "trace.jsonl").open("rb") as source, gzip.open(path / "trace.jsonl.gz", "wb", compresslevel=3) as target:
            shutil.copyfileobj(source, target)
        (path / "trace.jsonl").unlink()
        if item["mode"] == "on":
            run["flag_survey"] = report["flag_survey"]
            run["coverage"] = coverage(path)
            for kind in ["graph", "physics_queries"]:
                assert run["coverage"]["flag_work"][kind] == report["flag_survey"]["telemetry"][kind]
        result["runs"][item["name"]] = run
        D.write(opts.out / "summary.json", result)
    for group in dict.fromkeys(r["group"] for r in result["plan"]):
        for left, right in ([("frozen", "off"), ("off", "on")] if group == "regression" else [("off", "on")]):
            a, b = [f"{group}-{mode}" for mode in [left, right]]
            reports = [json.loads((opts.out / name / "report.json").read_text()) for name in [a, b]]
            assert reports[0]["missions"] == reports[1]["missions"]
            assert D.same_physical_outcomes(*reports)
            for key in ["evaluation_sha256", "controller_trace_sha256"]:
                assert result["runs"][a][key] == result["runs"][b][key], (a, b, key)
            result["comparisons"].append(dict(baseline=a, observed=b, exact_controller_trace=True,
                                              exact_evaluation=True, same_physical_outcomes=True))
    assert digest(opts.binary) == result["binaries"]["new"]
    assert digest(opts.baseline) == result["binaries"]["frozen"]
    D.write(opts.out / "summary.json", result)


if __name__ == "__main__":
    main()
