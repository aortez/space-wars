#!/usr/bin/env python3
"""Seat-swapped material matches using the same policies as the launcher.

No work quota is implemented yet: this measures behavior and actual cost,
not equal-budget strength. Run sequentially on an otherwise idle host for timing.
"""
import argparse
from collections import Counter
import csv
import hashlib
import json
import pathlib
import platform
import subprocess
import time


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def timings(values):
    values = sorted(values)
    if not values:
        return None
    return dict(count=len(values), mean_ms=sum(values) / len(values),
                p95_ms=values[min(len(values) * 95 // 100, len(values) - 1)],
                max_ms=values[-1], over_16_67_ms=sum(v > 1000 / 60 for v in values))


def summarize(report, csv_path, roles):
    with csv_path.open() as file:
        rows = list(csv.DictReader(file))
    work = [Counter(), Counter()]
    profiles = csv_path.parent / "sensors.jsonl"
    if profiles.exists():
        with profiles.open() as file:
            for line in file:
                sample = json.loads(line)
                work[sample["seat"]].update(sample["profile"]["counters"])
    result = []
    outcome = report["round"]["outcome"]
    winner = outcome.get("winner") if isinstance(outcome, dict) else None
    for seat, role in enumerate(roles):
        mission = report["missions"][seat]
        metrics = report["metrics"][seat]
        visits = metrics["visits"]
        result.append(dict(
            role=role, seat=seat, configuration=report["policy_configuration"][seat],
            outcome=("unfinished" if outcome is None else "draw" if winner is None
                     else "win" if winner == f"player_{seat + 1}" else "loss"),
            owned_planets=report["round"]["owned_planets"][seat],
            pilot_health=report["round"]["pilots"][seat]["health"],
            completed_sorties=mission["completed_sorties"],
            completed_recoveries=mission["completed_recoveries"],
            replans=mission["replans"],
            captured_visits=sum(v["claimed_tick"] is not None for v in visits),
            boarded_visits=sum(v["boarded_tick"] is not None for v in visits),
            abandoned_visits=[v for v in visits if v["abandoned_tick"] is not None],
            # Long phases warrant review; they are not proof of a stalled bot.
            longest_phase_ticks=metrics["longest_phase_ticks"],
            sensor_work=dict(work[seat]) if profiles.exists() else None,
            sensors=timings([float(r[f"sensor_p{seat+1}_ms"]) for r in rows]),
            policy=timings([float(r[f"policy_p{seat+1}_ms"]) for r in rows]),
        ))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument("--baseline", default="material_mission_v9")
    parser.add_argument("--candidate", default="material_mission_v10")
    parser.add_argument("--seeds", type=int, nargs="+", default=[
        int.from_bytes(hashlib.sha256(f"mission-policy-comparison-v1:{i}".encode()).digest()[:8], "big")
        for i in range(4)])
    parser.add_argument("--seconds", type=int, default=180, choices=range(1, 601), metavar="1..600")
    parser.add_argument("--asteroid-interval", type=int, default=0)
    parser.add_argument("--wall-timeout", type=int, default=900)
    args = parser.parse_args()
    if (len(set(args.seeds)) != len(args.seeds) or any(not 0 <= s < 2**64 for s in args.seeds)
            or args.asteroid_interval < 0 or args.wall_timeout < 1):
        parser.error("seeds must be distinct u64s, interval nonnegative and timeout positive")
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    manifest = dict(version=1, binary=str(binary), binary_sha256=digest,
                    host=platform.platform(), machine=platform.machine(),
                    baseline=args.baseline, candidate=args.candidate, seeds=args.seeds,
                    seconds=args.seconds, asteroid_interval=args.asteroid_interval,
                    mode="duel", world="generated", match_rules=True,
                    landing_survey_hz=4, strict_work_quota=None,
                    budget_comparison="unbounded policies; actual cost only",
                    runs=[])
    summary = []
    for seed in args.seeds:
        initial = None
        same_policy_result = None
        for swap in (False, True):
            roles = ["baseline", "candidate"] if not swap else ["candidate", "baseline"]
            policies = [getattr(args, role) for role in roles]
            destination = args.out.resolve() / f"seed-{seed}-swap-{int(swap)}"
            destination.mkdir()
            command = [str(binary), "--world", "generated", "--seed", str(seed),
                       "--seat", "0", "--mode", "duel", "--match", "true",
                       "--seconds", str(args.seconds), "--asteroid-interval", str(args.asteroid_interval),
                       "--p1-policy", policies[0], "--p2-policy", policies[1],
                       "--timing-csv", "true", "--out", str(destination)]
            manifest["runs"].append(dict(command=command, roles=roles))
            write_json(args.out / "manifest.json", manifest)
            started = time.monotonic()
            with (destination / "runner.log").open("w") as log:
                process = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT,
                                         timeout=args.wall_timeout)
            report = json.loads((destination / "report.json").read_text())
            assert process.returncode == 0 and report["physics_ok"], f"failed run: {destination}"
            assert [c["policy"] for c in report["policy_configuration"]] == policies
            assert report["landing_survey_hz"] == 4 and report["mode"] == "duel"
            assert report["seed"] == seed and report["seconds"] == args.seconds and report["match_rules"]
            assert all(c["planning_work_quota"] is None for c in report["policy_configuration"])
            if initial is None:
                initial = report["initial_world"]
            assert initial == report["initial_world"], "seat swap changed initial world"
            if args.baseline == args.candidate:
                deterministic = {key: report[key] for key in (
                    "events", "samples", "metrics", "final_pilots", "final_planets",
                    "round", "missions", "elapsed_ticks", "final_audit", "final_combat")}
                if same_policy_result is None:
                    same_policy_result = deterministic
                assert same_policy_result == deterministic, "same-policy seat swap changed behavior"
            row = dict(seed=seed, swap=swap, report=str(destination / "report.json"),
                       wall_seconds=time.monotonic()-started, elapsed_ticks=report["elapsed_ticks"],
                       termination=report["termination"], physics_ok=report["physics_ok"],
                       finish_reason=report["round"]["reason"],
                       players=summarize(report, destination / "timing.csv", roles))
            summary.append(row)
            write_json(args.out / "summary.json", summary)
            print(json.dumps(row), flush=True)
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == digest, "runner changed during comparison"
    totals = {}
    for role in ("baseline", "candidate"):
        players = [p for run in summary for p in run["players"] if p["role"] == role]
        totals[role] = {outcome: sum(p["outcome"] == outcome for p in players)
                        for outcome in ("win", "loss", "draw", "unfinished")}
        for key in ("completed_sorties", "completed_recoveries", "captured_visits", "boarded_visits", "replans"):
            totals[role][key] = sum(p[key] for p in players)
    write_json(args.out / "totals.json", totals)


if __name__ == "__main__":
    main()
