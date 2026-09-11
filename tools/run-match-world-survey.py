#!/usr/bin/env python3
"""Bounded fresh-world matches; preserve failures, decisions and exact reproduction."""
import argparse
import concurrent.futures
import hashlib
import json
import pathlib
import subprocess
import time


# Chosen before observing outcomes. Stable SHA-256 labels avoid dependence on
# Python's random implementation, and keep the historical 0/2/7/42 set separate.
DEFAULT_SEEDS = [int.from_bytes(hashlib.sha256(
    f"spacewars-fresh-world-survey-v1:{i}".encode()).digest()[:8], "big")
    for i in range(8)]


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def task_path(mission):
    result = [mission["goal"]]
    for name in ("capture", "recovery"):
        task = mission.get(name)
        if task:
            result += [f"{name}:{task.get('goal', task.get('status'))}"]
            for child_name in ("landing", "ground", "stabilization"):
                child = task.get(child_name)
                state = child.get("goal", child.get("status")) if child else None
                if state is not None:
                    result += [f"{child_name}:{state}"]
    return "/".join(result)


def summarize(report):
    pilots = report["final_pilots"]
    counters = [p["recovery"] for p in pilots]
    claims = sum(p["claim"]["captures"] for p in report["final_planets"] if p["claim"])
    damage = [p["last_damage"] for p in report["round"]["pilots"] if p["death_tick"]]
    # These are review candidates, not an assertion of being stuck. A stable
    # task name can describe active travel, pursuit or productive ground motion.
    long_tasks = []
    for seat in range(2):
        runs = []
        for sample in report["samples"]:
            key = task_path(sample["missions"][seat])
            if runs and runs[-1]["task"] == key:
                runs[-1]["end_second"] = sample["second"]
            else:
                runs.append(dict(task=key, start_second=sample["second"],
                                 end_second=sample["second"]))
        long_tasks += [dict(seat=seat, **run) for run in runs
                       if run["end_second"] - run["start_second"] >= 30]
    return dict(
        seed=report["seed"], asteroid_interval=report["asteroids"]["settings"]["interval_seconds"],
        physics_ok=report["physics_ok"], termination=report["termination"],
        elapsed_ticks=report["elapsed_ticks"], simulated_seconds=report["elapsed_ticks"] / 60,
        outcome=report["round"]["outcome"], claims=claims,
        neutralizations=sum(p["claim"]["neutralizations"] for p in report["final_planets"] if p["claim"]),
        weapon_contact=any(c["cannon_hits"] or c["laser_hit_ticks"] for c in report["final_combat"]),
        ships_lost=[c["ships_lost"] for c in counters], rebuilds=[c["rebuilds"] for c in counters],
        pilot_health=[p["health"] for p in report["round"]["pilots"]],
        death_causes=[dict(cause=d["cause"], contact=d["contact"]["other_kind"] if d["contact"] else None)
                      for d in damage],
        final_tasks=[task_path(m) for m in report["missions"]],
        completed_sorties=[m["completed_sorties"] for m in report["missions"]],
        long_task_review_candidates=long_tasks,
        asteroid_arrivals=report["asteroids"]["spawned"],
        asteroid_contacts=report["asteroids"]["contacts"],
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=pathlib.Path, required=True)
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument("--seeds", type=int, nargs="+", default=DEFAULT_SEEDS)
    parser.add_argument("--asteroid-intervals", type=int, nargs="+", default=[0, 3])
    parser.add_argument("--seconds", type=int, default=180, choices=range(1, 181), metavar="1..180")
    parser.add_argument("--jobs", type=int, default=1)
    parser.add_argument("--wall-timeout", type=int, default=600)
    args = parser.parse_args()
    if args.jobs < 1 or args.wall_timeout < 1 or any(not 0 <= s < 2**64 for s in args.seeds):
        parser.error("jobs/timeout must be positive and seeds must fit u64")
    if any(i < 0 for i in args.asteroid_intervals):
        parser.error("asteroid intervals must be nonnegative")
    cases = [(s, i) for s in args.seeds for i in args.asteroid_intervals]
    if len(set(cases)) != len(cases):
        parser.error("duplicate cases")
    args.out.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve(strict=True)
    manifest = dict(version=1, seed_selection="SHA-256 label defaults or explicit --seeds",
                    seeds=args.seeds, asteroid_intervals=args.asteroid_intervals,
                    seconds=args.seconds, jobs=args.jobs,
                    binary=str(binary), binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                    combat_break_interval=15, combat_break_duration=4,
                    world="generated", mode="duel", match_rules=True, mirror=False,
                    wall_timeout_seconds=args.wall_timeout)
    write_json(args.out / "manifest.json", manifest)

    def run_case(case):
        seed, interval = case
        name = f"s{seed}-asteroids{interval}"
        out = (args.out / name).resolve()
        out.mkdir()
        command = [str(binary), "--world", "generated", "--mode", "duel", "--match", "true",
                   "--seed", str(seed), "--seat", "0", "--mirror", "false",
                   "--seconds", str(args.seconds), "--asteroid-interval", str(interval),
                   "--break-interval", "15", "--break-duration", "4",
                   "--frames", "true", "--trace", "true", "--out", str(out)]
        write_json(out / "command.json", command)
        start = time.monotonic()
        row = dict(case=name, seed=seed, asteroid_interval=interval, command=command)
        try:
            with (out / "run.log").open("w") as log:
                run = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT,
                                     timeout=args.wall_timeout)
            row["exit_code"] = run.returncode
            raw = (out / "report.json").read_bytes()
            report = json.loads(raw)
            assert report["seed"] == seed and report["seconds"] == args.seconds
            assert report["mode"] == "duel" and report["match_rules"] and not report["mirror"]
            assert report["asteroids"]["settings"] == dict(interval_seconds=interval, severity="mixed")
            assert report["combat_breaks"] == dict(interval_seconds=15, duration_seconds=4)
            assert all(p["tick"] == report["elapsed_ticks"] for p in report["final_pilots"])
            assert report["round"]["outcome"] is not None or report["elapsed_ticks"] == args.seconds * 60
            assert report["round"]["outcome"] is None or report["round"]["finished_tick"] == report["elapsed_ticks"]
            row.update(summarize(report))
            row["report_sha256"] = hashlib.sha256(raw).hexdigest()
        except (OSError, ValueError, KeyError, AssertionError, subprocess.SubprocessError) as error:
            row.update(physics_ok=False, error=f"{type(error).__name__}: {error}")
        row["wall_seconds"] = round(time.monotonic() - start, 3)
        write_json(out / "result.json", row)
        return row

    rows = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as executor:
        for row in executor.map(run_case, cases):
            rows.append(row)
            write_json(args.out / "summary.json", rows)
            print(json.dumps({k: v for k, v in row.items()
                              if k not in ("command", "long_task_review_candidates")}), flush=True)
    raise SystemExit(int(any(r.get("exit_code") != 0 or not r["physics_ok"] for r in rows)))


if __name__ == "__main__":
    main()
