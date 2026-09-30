#!/usr/bin/env python3
"""Frozen regression plus predeclared fresh v10/v12/v13 finished matches."""
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location(
    "destinations", Path(__file__).with_name("compare-capture-destinations.py"))
D = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(D)


def plan():
    rows = []
    for version in [10, 12, 13]:
        rows.append(dict(name=f"regression-v{version}", group="regression", version=version,
                         seed=186767996776005237, interval=0, seat=1))
    for world in range(4):
        seed = int.from_bytes(hashlib.sha256(f"native-capture-value-v1:{world}".encode()).digest()[:8], "little")
        for interval in [0, 3]:
            group = f"world{world}-asteroids{interval}"
            variants = [(10, None), (12, 0), (12, 1), (13, 0), (13, 1)]
            rotation = (world + interval) % len(variants)
            for version, seat in variants[rotation:] + variants[:rotation]:
                name = group + ("-v10" if seat is None else f"-v{version}-p{seat + 1}")
                rows.append(dict(name=name, group=group, version=version,
                                 seed=seed, interval=interval, seat=seat))
    return rows


def coverage(path):
    result = D.evaluation_coverage(path)
    for row in map(json.loads, path.read_text().splitlines()):
        value = row.get("value_comparison")
        if value is None:
            continue
        seat = {"player_1": 0, "player_2": 1}[row["actor"]]
        c = result[seat]["counts"]
        def count(key):
            c[key] = c.get(key, 0) + 1
        if value["preferred"] is not None:
            count("complete_value_preferences")
            if row["current_target"] is not None and value["preferred"] != row["current_target"]:
                count("alternative_value_preferences")
        if any(c.get("transfer") and c["transfer"]["climb_seconds"] > 0 for c in row["candidates"]):
            count("reports_with_climb_cost")
    return result


def compare(out, baseline, candidate, seat):
    a, b = [json.loads((out / name / "report.json").read_text()) for name in [baseline, candidate]]
    assert a["initial_world"] == b["initial_world"]
    same = D.same_physical_outcomes(a, b)
    # Any policy without a switch must retain v10's physical behavior.
    if a["policy_configuration"][seat]["policy"] == "material_mission_v10":
        assert b["missions"][seat]["destination_planning"]["switches"] or same
    return dict(baseline=baseline, candidate=candidate, seat=seat,
                matching_recorded_physical_outcomes=same,
                baseline_outcome=D.outcome(a, seat), candidate_outcome=D.outcome(b, seat))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/surface_mission_soak"))
    parser.add_argument("--out", type=Path, required=True)
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    opts.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=plan(), runs={}, comparisons=[],
                  scope="Three regression runs and 40 fresh finished matches. Four world seeds, reused controls, no weight fitting; not 32 independent samples.",
                  source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                  source_dirty=bool(subprocess.check_output(["git", "status", "--porcelain"], text=True).strip()),
                  binary_sha256=hashlib.sha256(opts.binary.read_bytes()).hexdigest())
    D.write(opts.out / "summary.json", result)
    for item in result["plan"]:
        seat = item["seat"]
        policies = [10, 10]
        if seat is not None:
            policies[seat] = item["version"]
        args = ["--world", "generated", "--seed", str(item["seed"]), "--mode", "duel",
                "--match", "true", "--seat", "0", "--asteroid-interval", str(item["interval"]),
                "--p1-policy", f"material_mission_v{policies[0]}", "--p2-policy", f"material_mission_v{policies[1]}"]
        run = D.run(opts.binary, opts.out, item["name"], args, seat or 0, seconds=600, require_finish=True)
        report = json.loads((opts.out / item["name"] / "report.json").read_text())
        assert report["seed"] == item["seed"] and report["round"]["time_limit_seconds"] == 600
        assert [p["policy"] for p in report["policy_configuration"]] == [f"material_mission_v{v}" for v in policies]
        assert report["live_objective_planning"]["enabled_seats"] == []
        assert report["live_objective_planning"]["allowance"] == {"graph": 4, "physics_queries": 384}
        run["players"] = [D.finished_player(report, s) for s in range(2)]
        run["evaluation_coverage"] = coverage(opts.out / item["name"] / "mission-evaluations.jsonl")
        run["finish_reason"] = report["round"]["reason"]
        run["timings"] = {k: report[k] for k in ["sensors", "policy", "steps", "mission_evaluation"]}
        result["runs"][item["name"]] = run
        D.write(opts.out / "summary.json", result)
    for item in result["plan"]:
        if item["version"] == 10:
            continue
        base = "regression-v10" if item["group"] == "regression" else item["group"] + "-v10"
        result["comparisons"].append(compare(opts.out, base, item["name"], item["seat"]))
        if item["version"] == 13:
            base = item["name"].replace("-v13", "-v12")
            result["comparisons"].append(compare(opts.out, base, item["name"], item["seat"]))
    result["fresh_outcomes"] = {}
    for version in [12, 13]:
        pairs = [c for c in result["comparisons"] if c["baseline"].endswith("-v10")
                 and not c["baseline"].startswith("regression") and f"-v{version}-" in c["candidate"]]
        result["fresh_outcomes"][str(version)] = dict(
            candidate=dict(Counter(c["candidate_outcome"] for c in pairs)),
            control_same_seat=dict(Counter(c["baseline_outcome"] for c in pairs)))
    assert hashlib.sha256(opts.binary.read_bytes()).hexdigest() == result["binary_sha256"]
    D.write(opts.out / "summary.json", result)


if __name__ == "__main__":
    main()
