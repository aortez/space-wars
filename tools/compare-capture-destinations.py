#!/usr/bin/env python3
"""Reproducible native-sensor v10/v12 trials; no model fitting or winner filtering."""
import argparse
from collections import Counter
import csv
import hashlib
import json
from pathlib import Path
import subprocess


PHYSICAL_FIELDS = ("metrics", "round", "final_pilots", "final_planets", "final_audit",
                   "final_combat", "asteroid_events", "elapsed_ticks")


def same_physical_outcomes(a, b):
    # Damage events carry diagnostic mission telemetry as well as physical
    # state. Policy identity is expected to differ even for identical damage.
    def damage(report):
        return [{k: v for k, v in event.items() if k != "mission"}
                for event in report["pilot_damage_events"]]
    return all(a[field] == b[field] for field in PHYSICAL_FIELDS) and damage(a) == damage(b)


def write(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def outcome(report, seat):
    result = (report.get("round") or {}).get("outcome")
    if result is None:
        return "unfinished"
    winner = result.get("winner") if isinstance(result, dict) else None
    return "draw" if winner is None else "win" if winner == f"player_{seat + 1}" else "loss"


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


def run(binary, out, name, args, seat, *, seconds=180, require_finish=False):
    path = out / name
    command = [str(binary), "--seconds", str(seconds), "--live-objective-planning", "true",
               "--live-objective-seats", "none", "--objective-graph-budget", "4",
               "--objective-query-budget", "384", "--evaluate-missions", "true",
               "--survey-capture-alternative", "true", "--out", str(path), *args]
    if require_finish:
        command += ["--require-finish", "true", "--timing-csv", "true"]
    with (out / (name + ".log")).open("w") as log:
        subprocess.run(command, check=True, stdout=log, stderr=log, timeout=900)
    report = json.loads((path / "report.json").read_text())
    result = measurements(report, seat)
    result["command"] = command
    result["report_sha256"] = hashlib.sha256((path / "report.json").read_bytes()).hexdigest()
    with (path / "live-planning.csv").open() as stream:
        rows = list(csv.DictReader(stream))
    result["maximum_remote_queries_per_tick"] = max((int(r["total_queries"]) for r in rows), default=0)
    assert result["maximum_remote_queries_per_tick"] <= 384
    assert result["physics_ok"], name
    if require_finish:
        assert report["termination"] == "round_finished" and outcome(report, seat) != "unfinished"
    print(name, "claims", result["first_claim_tick"], "departures", result["completed_sorties"],
          "choices", (result["destination_planning"] or {}).get("switches", 0), flush=True)
    return result


def compare_pair(out, pair):
    reports = [json.loads((out / f'{pair["name"]}-v{v}' / "report.json").read_text()) for v in [10, 12]]
    pair["matching_recorded_physical_outcomes"] = same_physical_outcomes(*reports)
    if pair["v12"]["destination_planning"]["switches"] == 0:
        assert pair["matching_recorded_physical_outcomes"], pair["name"]


def finished_player(report, seat):
    result = measurements(report, seat)
    pilot = report["round"]["pilots"][seat]
    physical = report["final_pilots"][seat]
    # A switch can recur in many event snapshots; retain each decision once.
    history = [report["missions"][seat]]
    history += [e["telemetry"] for e in report["events"] if e["seat"] == seat]
    history += [s["missions"][seat] for s in report["samples"]]
    switches = {}
    for mission in history:
        switch = (mission.get("destination_planning") or {}).get("last_switch")
        if switch is not None:
            previous = switches.setdefault(switch["tick"], switch)
            assert previous == switch, "conflicting snapshots of a destination switch"
    assert len(switches) == (result["destination_planning"] or {}).get("switches", 0)
    result.update(
        outcome=outcome(report, seat), pilot_health=pilot["health"], death_tick=pilot["death_tick"],
        owned_planets=report["round"]["owned_planets"][seat],
        recovery=physical["recovery"], final_location=physical["location"],
        longest_phase_ticks=report["metrics"][seat]["longest_phase_ticks"],
        switches=[switches[tick] for tick in sorted(switches)],
    )
    return result


def evaluation_coverage(path):
    coverage = [Counter(), Counter()]
    unknown = [Counter(), Counter()]
    with path.open() as stream:
        for line in stream:
            row = json.loads(line)
            seat = {"player_1": 0, "player_2": 1}[row["actor"]]
            count = coverage[seat]
            count["reports"] += 1
            count["inactive_reports"] += row["inactive_reason"] is not None
            if row["preferred_by_time"] is not None:
                count["complete_preferences"] += 1
                if row["current_target"] is None:
                    count["preferences_without_current_target"] += 1
                else:
                    count["alternative_preferences"] += row["preferred_by_time"] != row["current_target"]
            count["multiple_numeric_destinations"] += sum(
                c["total_seconds"] is not None for c in row["candidates"]) >= 2
            for candidate in row["candidates"]:
                if candidate["total_seconds"] is None:
                    unknown[seat][candidate["unknown_reason"] or "unknown"] += 1
    return [dict(counts=dict(c), unknown_candidates=dict(u)) for c, u in zip(coverage, unknown)]


def finished_comparison(out, baseline, candidate, seat):
    reports = [json.loads((out / name / "report.json").read_text()) for name in (baseline, candidate)]
    assert reports[0]["initial_world"] == reports[1]["initial_world"], "comparison changed initial world"
    expected = ["material_mission_v10", "material_mission_v10"]
    assert [p["policy"] for p in reports[0]["policy_configuration"]] == expected
    expected[seat] = "material_mission_v12"
    assert [p["policy"] for p in reports[1]["policy_configuration"]] == expected
    same = same_physical_outcomes(*reports)
    switches = reports[1]["missions"][seat]["destination_planning"]["switches"]
    assert switches or same, "v12 changed physical outcomes without a destination switch"
    return dict(baseline=baseline, candidate=candidate, seat=seat,
                matching_recorded_physical_outcomes=same, switches=switches,
                baseline_outcome=outcome(reports[0], seat), candidate_outcome=outcome(reports[1], seat))


def finished_matches(opts, results):
    seeds = [int.from_bytes(hashlib.sha256(f"native-destination-finished-v1:{i}".encode()).digest()[:8],
                            "little") for i in range(4)]
    results.update(schema=2, suite="finished_matches", seconds_per_run=600, seeds=seeds,
                   scope="Finished-match strength and paired behavior checks; four worlds are not independent per-seat samples. Controls are reused for both seats. No parameter fitting.",
                   runs={}, comparisons=[])
    del results["pairs"]
    plan = []
    for world, seed in enumerate(seeds):
        for pressure, interval in enumerate([0, 3]):
            prefix = f"world{world}-asteroids{interval}"
            # Rotate execution order without changing any matchup or world.
            seats = [None, 0, 1]
            rotation = (world + pressure) % len(seats)
            for seat in seats[rotation:] + seats[:rotation]:
                name = f"{prefix}-" + ("v10" if seat is None else f"v12-p{seat + 1}")
                plan.append(dict(name=name, seed=seed, asteroid_interval=interval, candidate_seat=seat))
    results["plan"] = plan
    write(opts.out / "summary.json", results)
    for item in plan:
        seat = item["candidate_seat"]
        policies = ["material_mission_v10", "material_mission_v10"]
        if seat is not None:
            policies[seat] = "material_mission_v12"
        args = ["--world", "generated", "--seed", str(item["seed"]), "--mode", "duel",
                "--match", "true", "--seat", "0", "--asteroid-interval", str(item["asteroid_interval"]),
                "--p1-policy", policies[0], "--p2-policy", policies[1]]
        result = run(opts.binary, opts.out, item["name"], args, seat or 0, seconds=600, require_finish=True)
        report = json.loads((opts.out / item["name"] / "report.json").read_text())
        assert report["seed"] == item["seed"] and report["world"] == "generated"
        assert report["mode"] == "duel" and report["match_rules"] and report["seconds"] == 600
        assert report["round"]["time_limit_seconds"] == 600
        assert report["live_objective_planning"]["enabled_seats"] == []
        assert report["live_objective_planning"]["allowance"] == {"graph": 4, "physics_queries": 384}
        assert report["mission_evaluation"]["alternative_survey"]
        result["players"] = [finished_player(report, s) for s in range(2)]
        result["evaluation_coverage"] = evaluation_coverage(opts.out / item["name"] / "mission-evaluations.jsonl")
        result["finish_reason"] = report["round"]["reason"]
        result["timings"] = {key: report[key] for key in ("sensors", "policy", "steps", "measured_tick")}
        results["runs"][item["name"]] = result
        write(opts.out / "summary.json", results)
        print(item["name"], "finished", report["round"]["reason"],
              [p["outcome"] for p in result["players"]], flush=True)
    for world in range(len(seeds)):
        for interval in [0, 3]:
            prefix = f"world{world}-asteroids{interval}"
            for seat in [0, 1]:
                results["comparisons"].append(finished_comparison(
                    opts.out, f"{prefix}-v10", f"{prefix}-v12-p{seat + 1}", seat))
    results["candidate_outcomes"] = dict(Counter(c["candidate_outcome"] for c in results["comparisons"]))
    results["baseline_same_seat_outcomes"] = dict(Counter(c["baseline_outcome"] for c in results["comparisons"]))
    assert hashlib.sha256(opts.binary.read_bytes()).hexdigest() == results["binary_sha256"], "runner changed"
    write(opts.out / "summary.json", results)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/surface_mission_soak"))
    parser.add_argument("--out", type=Path)
    parser.add_argument("--finished-matches", action="store_true",
                        help="24 full matches: four fresh worlds, both seats, quiet/asteroids and v10 controls")
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    opts.out = opts.out or Path("target/capture-destination-planner") / ("finished" if opts.finished_matches else "matrix")
    opts.out.mkdir(parents=True, exist_ok=False)
    results = {"schema": 1, "seconds_per_run": 180, "native_local_sensors": True,
               "scope": "First-footing latency and complete trips, not calibrated utility or a win-rate estimate. Includes unchanged/failing cases.",
               "binary_sha256": hashlib.sha256(opts.binary.read_bytes()).hexdigest(),
               "source_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
               "pairs": []}
    if opts.finished_matches:
        finished_matches(opts, results)
        return
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
