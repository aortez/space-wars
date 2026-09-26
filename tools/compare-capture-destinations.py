#!/usr/bin/env python3
"""Reproducible native-sensor v10/v12 trials; no model fitting or winner filtering."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import subprocess


def measurements(report, seat):
    mission = report["missions"][seat]
    metrics = report["metrics"][seat]
    visits = metrics["visits"]
    first = lambda key: min((v[key] for v in visits if v[key] is not None), default=None)
    return {
        "physics_ok": report["physics_ok"],
        "elapsed_ticks": report["elapsed_ticks"],
        "first_claim_tick": first("claimed_tick"),
        "first_boarded_tick": first("boarded_tick"),
        "first_departed_tick": first("departed_tick"),
        "all_owned_tick": metrics["first_all_owned_tick"],
        "completed_sorties": mission["completed_sorties"],
        "completed_recoveries": mission["completed_recoveries"],
        "destination_planning": mission.get("destination_planning"),
        "visits": visits,
        "round": report["round"],
        "queries": report["live_objective_planning"]["destination_cover"],
        "evaluation_work": report["mission_evaluation"]["charged"],
    }


def run(binary, out, name, args, seat):
    path = out / name
    command = [str(binary), "--seconds", "180", "--live-objective-planning", "true",
               "--live-objective-seats", "none", "--objective-graph-budget", "4",
               "--objective-query-budget", "384", "--evaluate-missions", "true",
               "--survey-capture-alternative", "true", "--out", str(path), *args]
    with (out / (name + ".log")).open("w") as log:
        subprocess.run(command, check=True, stdout=log, stderr=log)
    report = json.loads((path / "report.json").read_text())
    result = measurements(report, seat)
    result["command"] = command
    result["report_sha256"] = hashlib.sha256((path / "report.json").read_bytes()).hexdigest()
    with (path / "live-planning.csv").open() as stream:
        rows = list(csv.DictReader(stream))
    result["maximum_remote_queries_per_tick"] = max((int(r["total_queries"]) for r in rows), default=0)
    assert result["maximum_remote_queries_per_tick"] <= 384
    assert result["physics_ok"], name
    print(name, "claims", result["first_claim_tick"], "departures", result["completed_sorties"],
          "choices", (result["destination_planning"] or {}).get("switches", 0), flush=True)
    return result


def compare_pair(out, pair):
    reports = [json.loads((out / f'{pair["name"]}-v{v}' / "report.json").read_text()) for v in [10, 12]]
    fields = ["metrics", "round", "final_pilots", "final_planets", "final_audit", "final_combat", "asteroid_events"]
    pair["matching_recorded_physical_outcomes"] = all(reports[0][field] == reports[1][field] for field in fields)
    if pair["v12"]["destination_planning"]["switches"] == 0:
        assert pair["matching_recorded_physical_outcomes"], pair["name"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/surface_mission_soak"))
    parser.add_argument("--out", type=Path, default=Path("target/capture-destination-planner/matrix"))
    opts = parser.parse_args()
    opts.out.mkdir(parents=True, exist_ok=True)
    results = {"schema": 1, "seconds_per_run": 180, "native_local_sensors": True,
               "scope": "First-footing latency and complete trips, not calibrated utility or a win-rate estimate. Includes unchanged/failing cases.",
               "binary_sha256": hashlib.sha256(opts.binary.read_bytes()).hexdigest(),
               "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
               "pairs": []}
    # The negative bearing cases retain the known unsupported/stalled routes.
    for seat, mirror in [(0, False), (1, True), (0, True), (1, False)]:
        bearing = -0.8 if (seat == 1) ^ mirror else 0.8
        name = f"flag-seat{seat}-mirror{int(mirror)}"
        args = ["--world", "destination", "--seat", str(seat), "--mirror", str(mirror).lower(),
                "--mode", "quiet", "--flag-bearing", str(bearing)]
        pair = {"name": name, "kind": "controlled_flag", "seat": seat}
        for version in [10, 12]:
            pair[f"v{version}"] = run(opts.binary, opts.out, f"{name}-v{version}",
                                     args + [f"--p{seat+1}-policy", f"material_mission_v{version}"], seat)
        compare_pair(opts.out, pair)
        results["pairs"].append(pair)
    for world in range(2):
        seed = int.from_bytes(hashlib.sha256(f"native-destination-v1:{world}".encode()).digest()[:8], "little")
        for seat in [0, 1]:
            for interval in [0, 3]:
                name = f"generated-{world}-seat{seat}-asteroids{interval}"
                args = ["--world", "generated", "--seed", str(seed), "--mode", "duel", "--match", "true",
                        "--seat", str(seat), "--asteroid-interval", str(interval)]
                pair = {"name": name, "kind": "generated_match", "seat": seat, "seed": seed}
                for version in [10, 12]:
                    policies = [10, 10]
                    policies[seat] = version
                    pair[f"v{version}"] = run(opts.binary, opts.out, f"{name}-v{version}", args + [
                        "--p1-policy", f"material_mission_v{policies[0]}", "--p2-policy", f"material_mission_v{policies[1]}"], seat)
                compare_pair(opts.out, pair)
                results["pairs"].append(pair)
    (opts.out / "summary.json").write_text(json.dumps(results, indent=2, allow_nan=False) + "\n")


if __name__ == "__main__":
    main()
