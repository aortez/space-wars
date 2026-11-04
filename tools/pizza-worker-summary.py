#!/usr/bin/env python3
"""Compare Pizza Pi captures with and without the simulation worker.

Reads paired runs from tools/pizza-live-capture.sh named direct-RUN and
worker-RUN (or other prefixes given with `--pair`), checks that each pair
replays the same updates, and prints the summary JSON. `--desktop` also checks every run against the desktop seed-7
extracts retained with the bounded-gravity evidence. See
docs/pizza-simulation-worker.md.
"""
import argparse
import csv
import importlib.util
import json
import statistics
from pathlib import Path

PROFILE_SPEC = importlib.util.spec_from_file_location(
    "pizza_profile", Path(__file__).with_name("pizza-profile-summary.py")
)
PROFILE = importlib.util.module_from_spec(PROFILE_SPEC)
PROFILE_SPEC.loader.exec_module(PROFILE)

MODES = ["direct", "worker"]
RUNS = {"on-r1": "on", "on-r2": "on", "off-r1": "off"}
# The lab requests the gravity event at update 120; the event lasts 900 updates.
EVENT = (120, 1020)
PHASES = ["gravity_us", "physics_total_us", "broad_phase_us", "narrow_phase_us", "solver_us",
          "ccd_us"]
STAGES = ["loop_wall_ms", "app_host_step_wall_ms", "render_wall_ms", "flip_wait_wall_ms"]


def status_series(path, key):
    """Numeric status values by sample index."""
    series = {}
    for status in path.glob("status-*.txt"):
        index = int(status.stem.split("-")[1])
        for line in status.read_text().splitlines():
            name, _, value = line.partition("=")
            if name != key:
                continue
            if key == "pizza":
                series[index] = json.loads(value)["tick"]
            else:
                try:
                    series[index] = float(value)
                except ValueError:
                    pass
    return series


def uptimes(path):
    result = {}
    for block in (path / "cpu.txt").read_text().split("BEGIN ")[1:]:
        lines = block.splitlines()
        result[int(lines[0])] = float(lines[1].split()[0])
    return result


def game_speed(path, start, end):
    """Simulated time over wall time between two update ticks.

    Status ticks and CPU-sample uptimes share their sample index; wall times
    for the requested ticks are interpolated between neighboring samples.
    """
    ticks = status_series(path, "pizza")
    clock = uptimes(path)
    points = sorted((clock[index], tick) for index, tick in ticks.items() if index in clock)

    def wall_time(target):
        for (t0, k0), (t1, k1) in zip(points, points[1:]):
            if k0 <= target <= k1 and k1 > k0:
                return t0 + (t1 - t0) * (target - k0) / (k1 - k0)
        raise ValueError(f"{path.name}: tick {target} not bracketed by status samples")

    return (end - start) / 60 / (wall_time(end) - wall_time(start))


def run(path, field):
    capture = PROFILE.capture(path)
    selected = capture["before_export"]
    flips = [
        interval
        for trace in selected["render"]
        for interval in PROFILE.flip_intervals(PROFILE.trace_rows(path / trace["file"]))
    ]
    rows = [row for trace in selected["render"] for row in PROFILE.trace_rows(path / trace["file"])]
    samples = json.loads((path / "simulation.json").read_text())["samples"]
    dropped = status_series(path, "pizza_worker_dropped_backlogs")
    return {
        "simulation": simulation_mode(path),
        "ui_core_mean_pct": selected["cpu"]["main_thread_pct"]["mean"],
        "process_mean_pct": selected["cpu"]["process_pct"]["mean"],
        "longest_ui_core_ge95_s": selected["cpu"]["longest_main_thread_ge95_seconds"],
        "cpu_intervals": len(selected["cpu"]["intervals"]),
        "kms_windows": len(selected["render"]),
        "flip_intervals": len(flips),
        "flip_ms": PROFILE.summary(flips),
        "flip_intervals_over_25ms": sum(interval > 25 for interval in flips),
        "ui_stage_mean_ms": {
            key.removesuffix("_wall_ms"): statistics.mean(float(row[key]) for row in rows)
            for key in STAGES
        },
        "update_ms": capture["simulation"]["timings_ms"]["step_us"],
        "phase_mean_ms": {
            key.removesuffix("_us"): statistics.mean(sample[key] for sample in samples) / 1000
            for key in PHASES
        },
        "peak_updates_per_frame": max(status_series(path, "host_updates_per_callback").values()),
        "worker_dropped_backlogs": max(dropped.values()) if dropped else None,
        "event_game_speed": game_speed(path, *EVENT) if field == "on" else None,
        "temperature_c": selected["cpu"]["temperature_c"],
        "throttle_bits": selected["cpu"]["throttle_bits"],
    }, [str(sample["fingerprint"]) for sample in samples]


def simulation_mode(path):
    """The requested mode, confirmed by the client's own status report.

    A client without the worker ignores the switch, so the request alone does
    not show where updates ran.
    """
    requested = (path / "simulation.txt").read_text().strip()
    reported = "worker" if status_series(path, "pizza_simulation_worker") else "direct"
    assert requested == reported, f"{path.name}: requested {requested}, ran {reported}"
    return reported


def compare_pair(name, fingerprints):
    # Where updates run never changes what they compute.
    first, second = fingerprints.values()
    assert first == second, f"{name} pair diverged"
    return {"updates": len(first), "fingerprints_identical": True}


def summarize(directory, pair=tuple(MODES), desktop=None):
    runs, pairs = {}, {}
    for name, field in RUNS.items():
        fingerprints = {}
        for side in pair:
            label = f"{side}-{name}"
            runs[label], fingerprints[side] = run(directory / label, field)
            if side in MODES:
                assert runs[label]["simulation"] == side, f"{label} ran {runs[label]['simulation']}"
            if desktop is not None:
                with (desktop / f"ordinary-{field}-s7-candidate.csv").open(newline="") as handle:
                    expected = [row["fingerprint"] for row in csv.DictReader(handle)]
                assert fingerprints[side] == expected, f"{label} diverged from the desktop"
        pairs[name] = compare_pair(name, fingerprints)
    return {
        "scope": "Pi 4 production GPU captures of seed 7; conservative pre-export windows.",
        "desktop_replay_checked": desktop is not None,
        "pairs": pairs,
        "runs": runs,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("captures", type=Path)
    parser.add_argument("--pair", nargs=2, default=MODES, metavar=("FIRST", "SECOND"),
                        help="run-name prefixes to compare (default: direct worker)")
    parser.add_argument("--desktop", type=Path, help="bounded-gravity desktop extracts")
    args = parser.parse_args()
    print(json.dumps(summarize(args.captures, tuple(args.pair), args.desktop), indent=2))


if __name__ == "__main__":
    main()
