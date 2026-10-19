#!/usr/bin/env python3
"""Summarize the common-fixture comparison and verify Rust replay equivalence."""
import argparse
import json
import statistics
from pathlib import Path

WORK_KEYS = [
    "fingerprint", "population", "sizes", "generations", "event", "contacts",
    "contact_pairs", "candidate_pairs", "outside_playfield", "escaped_walls",
]


def read(path):
    return json.loads(path.read_text())


def mean(values):
    return statistics.mean(values)


def rust(runs):
    samples = [s for run in runs for s in run["samples"]]
    assert not any(run.get("ccd_instrumented") for run in runs)
    return {
        "run_step_means_ms": [mean(s["step_us"] for s in r["samples"]) / 1000 for r in runs],
        "timings_ms": {k.removesuffix("_us"): mean(s[k] for s in samples) / 1000
                       for k in samples[0] if k.endswith("_us") and samples[0][k] is not None},
        "mean_live": mean(s["population"]["balls"] for s in samples),
        "final_live": runs[0]["final"]["balls"],
        "deaths": runs[0]["final"]["births"]["exploded"],
        "max_escaped": max(s["escaped_walls"] for s in samples),
    }


def paired(directory, prefix):
    result = {}
    for name in ["mixed-500", "mixed-1000", "mixed-2500", "mixed-5000", "ordinary-on"]:
        runs = {impl: [read(directory / f"{prefix}{name}-{impl}-{r}.json") for r in [1, 2, 3]]
                for impl in ["baseline", "lazy"]}
        reference = [{k: s[k] for k in WORK_KEYS} for s in runs["baseline"][0]["samples"]]
        for group in runs.values():
            for run in group:
                actual = [{k: s[k] for k in WORK_KEYS} for s in run["samples"]]
                assert actual == reference, f"work diverged: {directory}/{name}"
        result[name] = {impl: rust(group) for impl, group in runs.items()}
        result[name]["matched_updates"] = len(reference) * 3
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("desktop", type=Path)
    parser.add_argument("--pi", type=Path)
    args = parser.parse_args()
    result = {"desktop_paired": paired(args.desktop, "paired-"), "common_fixture": {}}
    for n in [500, 1000, 2500, 5000]:
        js = read(args.desktop / f"chrome-{n}.json")
        samples = [s for run in js["runs"] for s in run["samples"]]
        result["common_fixture"][n] = {
            "rust": rust([read(args.desktop / f"rust-{n}-{r}.json") for r in [1, 2, 3]]),
            "chrome": {
                "version": js["chrome"], "source": js["referenceCommit"],
                "run_step_means_ms": [mean(s["stepMs"] for s in run["samples"]) for run in js["runs"]],
                "timings_ms": {k: mean(s[k] for s in samples) for k in samples[0] if k.endswith("Ms")},
                "mean_live": mean(s["balls"] for s in samples),
                "mean_particles": mean(s["particles"] for s in samples),
                "final_live": js["runs"][0]["samples"][-1]["balls"],
                "final_particles": js["runs"][0]["samples"][-1]["particles"],
                "deaths": sum(s["lifecycle"]["removedBalls"] for s in js["runs"][0]["samples"]),
            },
        }
    if args.pi:
        result["pi_paired"] = paired(args.pi, "")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
