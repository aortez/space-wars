#!/usr/bin/env python3
"""Compare point-mass and bounded-interior Pizza gravity runs.

`extract` reduces capacity-lab reports from tools/pizza-gravity-runs.sh to the
per-update CSV columns retained under docs/data. `summarize` reads those CSVs,
checks replay equivalence and prints the summary JSON; `--pi` adds the paired
Pi 4 production captures from tools/pizza-live-capture.sh. See
docs/pizza-bounded-gravity.md.
"""
import argparse
import csv
import importlib.util
import json
import math
import statistics
from pathlib import Path

PROFILE_SPEC = importlib.util.spec_from_file_location(
    "pizza_profile", Path(__file__).with_name("pizza-profile-summary.py")
)
PROFILE = importlib.util.module_from_spec(PROFILE_SPEC)
PROFILE_SPEC.loader.exec_module(PROFILE)

SIDES = ["baseline", "candidate"]
SEEDS = range(1, 21)
REPEATS = range(1, 6)
MIXED = [500, 1000, 2500, 5000]
# Production capture names by side; the capture script always uses seed 7.
PI_LAWS = {"baseline": "point", "candidate": "bounded"}
PI_RUNS = {"on": ["r1", "r2"], "off": ["r1"]}
PHASES = ["gravity_us", "physics_total_us", "narrow_phase_us", "solver_us", "ccd_us"]
COLUMNS = [
    "tick", "fingerprint", "balls", "limit", "exploded", "fragments_created",
    "parents_skipped", "exploded_gen0", "exploded_gen1", "exploded_gen2",
    "exploded_gen3", "contacts", "candidate_pairs", "escaped_walls", "step_us",
    "gravity_us", "physics_total_us", "narrow_phase_us", "solver_us", "ccd_us",
    "shape_casts", "toi_calls", "thickness_rejected", "resweep_candidates",
]


def extract_rows(report):
    profiles = report.get("ccd_profiles")
    rows = []
    for index, sample in enumerate(report["samples"]):
        population = sample["population"]
        births = population["births"]
        row = {
            "tick": sample["tick"],
            "fingerprint": sample["fingerprint"],
            "balls": population["balls"],
            "limit": population["limit"],
            "exploded": births["exploded"],
            "fragments_created": births["fragments_created"],
            "parents_skipped": births["fragment_parents_skipped"],
            "contacts": sample["contacts"],
            "candidate_pairs": sample["candidate_pairs"],
            "escaped_walls": sample["escaped_walls"],
        }
        for generation, count in enumerate(births["exploded_by_generation"]):
            row[f"exploded_gen{generation}"] = count
        for key in ["step_us", "gravity_us", "physics_total_us", "narrow_phase_us",
                    "solver_us", "ccd_us"]:
            row[key] = sample[key] or 0.0
        if profiles:
            profile = profiles[index]
            prefilter = profile["prefilter"]
            row["shape_casts"] = sum(pair["queries"] for pair in profile["pairs"].values())
            row["toi_calls"] = prefilter["toi_calls"]
            row["thickness_rejected"] = prefilter["thickness_rejected"]
            row["resweep_candidates"] = prefilter["resweep_candidates"]
        rows.append(row)
    return rows


def extract(reports, out):
    out.mkdir(parents=True, exist_ok=True)
    for path in sorted(reports.glob("*.json")):
        report = json.loads(path.read_text())
        meta = {key: value for key, value in report.items()
                if not isinstance(value, (list, dict))}
        meta["initial"] = report["initial"]
        meta["final"] = report["final"]
        (out / f"{path.stem}.meta.json").write_text(json.dumps(meta, indent=2) + "\n")
        with (out / f"{path.stem}.csv").open("w", newline="") as handle:
            writer = csv.DictWriter(handle, COLUMNS, restval="")
            writer.writeheader()
            writer.writerows(extract_rows(report))


def number(value):
    try:
        return int(value)
    except ValueError:
        return float(value)


def read_rows(path):
    with path.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    for row in rows:
        for key, value in row.items():
            # Fingerprints stay exact strings; empty cells are absent counters.
            if key != "fingerprint" and value != "":
                row[key] = number(value)
    return rows


def run_stats(rows):
    """Cascade and CCD work for one run. Explosion updates follow the lab."""
    exploded = [row["exploded"] for row in rows]
    stats = {
        "exploded": rows[-1]["exploded"],
        "fragments_created": rows[-1]["fragments_created"],
        "parents_skipped": rows[-1]["parents_skipped"],
        "exploded_by_generation": [rows[-1][f"exploded_gen{g}"] for g in range(4)],
        "explosion_updates": sum(
            current > previous for previous, current in zip([0] + exploded, exploded)
        ),
        "mean_balls": statistics.mean(row["balls"] for row in rows),
        "updates_at_limit": sum(row["balls"] >= row["limit"] for row in rows),
        "mean_contacts": statistics.mean(row["contacts"] for row in rows),
        "max_escaped_walls": max(row["escaped_walls"] for row in rows),
    }
    if rows[0].get("shape_casts", "") != "":
        worst = max(rows, key=lambda row: (row["shape_casts"], -row["tick"]))
        stats.update({
            "max_shape_casts": worst["shape_casts"],
            "max_shape_casts_tick": worst["tick"],
            "total_shape_casts": sum(row["shape_casts"] for row in rows),
            "max_resweep_candidates": max(row["resweep_candidates"] for row in rows),
            "total_resweep_candidates": sum(row["resweep_candidates"] for row in rows),
            "total_toi_calls": sum(row["toi_calls"] for row in rows),
            "updates_over_1000_shape_casts": sum(row["shape_casts"] > 1000 for row in rows),
        })
    return stats


def spread(values):
    values = list(values)
    return {
        "mean": statistics.mean(values),
        "median": statistics.median(values),
        "min": min(values),
        "max": max(values),
    }


def compare_seeds(per_seed):
    """Aggregate per-seed run stats for both sides and their explosion ratios."""
    result = {}
    keys = per_seed[0]["baseline"].keys()
    for side in SIDES:
        result[side] = {
            key: spread(seed[side][key] for seed in per_seed)
            for key in keys
            if not isinstance(per_seed[0][side][key], list) and not key.endswith("_tick")
        }
        result[side]["exploded_by_generation"] = [
            sum(seed[side]["exploded_by_generation"][g] for seed in per_seed)
            for g in range(4)
        ]
    ratios = [seed["candidate"]["exploded"] / seed["baseline"]["exploded"] for seed in per_seed]
    result["exploded_ratio"] = {
        "geometric_mean": math.exp(statistics.mean(math.log(ratio) for ratio in ratios)),
        "min": min(ratios),
        "max": max(ratios),
        "seeds_higher": sum(ratio > 1 for ratio in ratios),
        "seeds_lower": sum(ratio < 1 for ratio in ratios),
    }
    return result


def check_replay(reference, others, label):
    expected = [row["fingerprint"] for row in reference]
    for rows in others:
        assert [row["fingerprint"] for row in rows] == expected, f"replay diverged: {label}"


def quantile(values, fraction):
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int(fraction * len(ordered)))]


def timing(groups):
    result = {}
    for side, runs in groups.items():
        steps = [[row["step_us"] / 1000 for row in rows] for rows in runs]
        result[side] = {
            "run_step_means_ms": [statistics.mean(run) for run in steps],
            "step_mean_ms": statistics.median(statistics.mean(run) for run in steps),
            "step_p95_ms": statistics.median(quantile(run, 0.95) for run in steps),
            "step_p99_ms": statistics.median(quantile(run, 0.99) for run in steps),
            "step_max_ms": max(max(run) for run in steps),
            "gravity_mean_ms": statistics.median(
                statistics.mean(row["gravity_us"] for row in rows) / 1000 for rows in runs
            ),
        }
    return result


def seed_timing(per_seed):
    """Mean update cost across differently evolving seeds, one run per seed."""
    result = {}
    for side in SIDES:
        runs = [seed[side] for seed in per_seed]
        means = [statistics.mean(row["step_us"] for row in rows) / 1000 for rows in runs]
        result[side] = {
            "step_mean_ms": statistics.mean(means),
            "seed_step_means_ms": means,
            "step_p99_ms": spread(
                quantile([row["step_us"] / 1000 for row in rows], 0.99) for rows in runs
            ),
            "step_max_ms": spread(max(row["step_us"] for row in rows) / 1000 for rows in runs),
            "gravity_mean_ms": statistics.mean(
                statistics.mean(row["gravity_us"] for row in rows) / 1000 for rows in runs
            ),
        }
    ratios = [
        statistics.mean(row["step_us"] for row in seed["candidate"])
        / statistics.mean(row["step_us"] for row in seed["baseline"])
        for seed in per_seed
    ]
    result["step_mean_ratio"] = {
        "geometric_mean": math.exp(statistics.mean(math.log(ratio) for ratio in ratios)),
        "min": min(ratios),
        "max": max(ratios),
    }
    return result


def pi_run(path, desktop):
    """One production capture: pre-export UI-core and flip windows, and update work.

    Flip quantiles pool the intervals within each selected KMS window without
    joining the gaps between windows.
    """
    capture = PROFILE.capture(path)
    selected = capture["before_export"]
    flips = [
        interval
        for trace in selected["render"]
        for interval in PROFILE.flip_intervals(PROFILE.trace_rows(path / trace["file"]))
    ]
    report = json.loads((path / "simulation.json").read_text())
    assert report["seed"] == 7 and report["ordinary"], f"unexpected capture: {path}"
    rows = [dict(row, fingerprint=str(row["fingerprint"])) for row in extract_rows(report)]
    # The device must replay the desktop trajectory of the same law.
    check_replay(desktop, [rows], f"pi {path.name}")
    slowest = max(rows, key=lambda row: (row["step_us"], -row["tick"]))
    return {
        "ui_core_mean_pct": selected["cpu"]["main_thread_pct"]["mean"],
        "longest_ui_core_ge95_s": selected["cpu"]["longest_main_thread_ge95_seconds"],
        "cpu_intervals": len(selected["cpu"]["intervals"]),
        "kms_windows": len(selected["render"]),
        "flip_intervals": len(flips),
        "flip_ms": PROFILE.summary(flips),
        "flip_intervals_over_25ms": sum(interval > 25 for interval in flips),
        "temperature_c": selected["cpu"]["temperature_c"],
        "throttle_bits": selected["cpu"]["throttle_bits"],
        "update_ms": capture["simulation"]["timings_ms"]["step_us"],
        "phase_mean_ms": {
            key.removesuffix("_us"): statistics.mean(row[key] for row in rows) / 1000
            for key in PHASES
        },
        "max_ccd_ms": max(row["ccd_us"] for row in rows) / 1000,
        "slowest_update": {
            "tick": slowest["tick"],
            "contacts": slowest["contacts"],
            **{key.removesuffix("_us") + "_ms": slowest[key] / 1000 for key in ["step_us"] + PHASES},
        },
        "cascade": run_stats(rows),
    }


def pi_summary(directory, extracts):
    runs = {}
    for field, repeats in PI_RUNS.items():
        for side, law in PI_LAWS.items():
            desktop = read_rows(extracts / f"ordinary-{field}-s7-{side}.csv")
            for repeat in repeats:
                name = f"{law}-{field}-{repeat}"
                runs[name] = pi_run(directory / name, desktop)
    return {
        "scope": "Pi 4 production GPU captures of seed 7; conservative pre-export windows.",
        "runs": runs,
    }


def summarize(directory, pi=None):
    def rows(name):
        return read_rows(directory / f"{name}.csv")

    result = {"scope": "Desktop headless reports; timing is host-dependent."}
    for field in ["on", "off"]:
        per_seed = []
        for seed in SEEDS:
            suffix = "-diag" if field == "on" else ""
            per_seed.append({
                side: run_stats(rows(f"ordinary-{field}-s{seed}-{side}{suffix}"))
                for side in SIDES
            })
        result[f"ordinary_{field}"] = {"seeds": per_seed, "summary": compare_seeds(per_seed)}

    uninstrumented = []
    for seed in SEEDS:
        runs = {side: rows(f"ordinary-on-s{seed}-{side}") for side in SIDES}
        for side in SIDES:
            check_replay(rows(f"ordinary-on-s{seed}-{side}-diag"), [runs[side]],
                         f"ordinary-on seed {seed} {side}")
        uninstrumented.append(runs)
    result["timing"] = {"ordinary_on_seeds": seed_timing(uninstrumented)}

    timing_groups = {}
    for side in SIDES:
        repeats = [rows(f"ordinary-on-s7-{side}-r{rep}") for rep in REPEATS]
        # Instrumentation and repetition must reproduce the same simulation.
        check_replay(rows(f"ordinary-on-s7-{side}-diag"), repeats, f"ordinary-on {side}")
        timing_groups[side] = repeats
    result["timing"]["ordinary_on_seed7"] = timing(timing_groups)
    for n in MIXED:
        groups = {}
        for side in SIDES:
            groups[side] = [rows(f"mixed-{n}-{side}-r{rep}") for rep in REPEATS]
            check_replay(groups[side][0], groups[side][1:], f"mixed-{n} {side}")
        result["timing"][f"mixed_{n}"] = timing(groups)
        result["timing"][f"mixed_{n}"]["cascade"] = {
            side: run_stats(groups[side][0]) for side in SIDES
        }
    if pi is not None:
        result["pi"] = pi_summary(pi, directory)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    extract_command = commands.add_parser("extract")
    extract_command.add_argument("reports", type=Path)
    extract_command.add_argument("out", type=Path)
    summarize_command = commands.add_parser("summarize")
    summarize_command.add_argument("extracts", type=Path)
    summarize_command.add_argument("--pi", type=Path, help="paired Pi 4 production captures")
    args = parser.parse_args()
    if args.command == "extract":
        extract(args.reports, args.out)
    else:
        print(json.dumps(summarize(args.extracts, args.pi), indent=2))


if __name__ == "__main__":
    main()
