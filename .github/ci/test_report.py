#!/usr/bin/env python3
"""Summarize GNU time wall durations and nextest JUnit using only the stdlib."""

import argparse
import html
import math
from pathlib import Path
import xml.etree.ElementTree as ET


PHASES = {
    "build": "Compile workspace (optimized CI profile)",
    "workspace": "Execute workspace tests",
    "ui": "Execute UI tests (serial, real-time application)",
    "kms": "Build and test vendored LinuxKMS",
}


def seconds(value):
    result = float(value)
    if not math.isfinite(result) or result < 0:
        raise ValueError(f"invalid duration: {value!r}")
    return result


def cell(value):
    return (
        html.escape(str(value))
        .replace("|", "&#124;")
        .replace("\n", " ")
        .replace("\r", " ")
    )


def summarize(timings_dir, reports, cache_hit="", limit=20):
    lines = ["## Linux build and test timings", ""]
    if cache_hit:
        lines += [
            f"Exact Rust cache-key hit: **{cell(cache_hit)}**. "
            "A non-exact hit can still restore dependencies; see the cache step.",
            "",
        ]
    lines += ["| Phase | Wall time |", "|---|---:|"]
    for phase, label in PHASES.items():
        path = timings_dir / f"{phase}.seconds"
        # On failure GNU time prefixes the duration with an exit-status line.
        elapsed = "Not recorded"
        if path.exists():
            recorded = path.read_text().strip().splitlines()
            # An active/interrupted command can leave an empty output file.
            if recorded:
                elapsed = f"{seconds(recorded[-1]):.2f}s"
        lines.append(f"| {label} | {elapsed} |")
    lines += [
        "",
        "Setup/cache transfer time is shown in the Actions steps, not this table. "
        "Execution phases include the runner's cached build check and test discovery.",
        "",
        "| Test report | Passed | Failed/errors | Skipped | Runner wall time |",
        "|---|---:|---:|---:|---:|",
    ]
    cases = []
    for path in reports:
        if not path.exists() or path.stat().st_size == 0:
            lines.append(f"| {cell(path)} | — | — | — | Not produced |")
            continue
        root = ET.parse(path).getroot()
        counts = {"PASS": 0, "FAIL": 0, "SKIP": 0}
        for case in root.iter("testcase"):
            status = "PASS"
            if case.find("failure") is not None or case.find("error") is not None:
                status = "FAIL"
            elif case.find("skipped") is not None:
                status = "SKIP"
            counts[status] += 1
            if status != "SKIP":
                cases.append((
                    seconds(case.get("time", "0")),
                    case.get("classname", ""),
                    case.get("name", ""),
                    status,
                ))
        elapsed = seconds(root.get("time", "0"))
        lines.append(
            f"| {cell(path)} | {counts['PASS']} | {counts['FAIL']} | "
            f"{counts['SKIP']} | {elapsed:.2f}s |"
        )
    lines += [
        "",
        "### Slowest executed tests",
        "",
        "Per-test durations can overlap; summing them is not job wall time. "
        "Full per-test results and Cargo compilation timelines are in the timing artifact. "
        "Ignored tests are unchanged; JUnit normally lists only selected tests.",
        "",
        "| Test binary | Test | Status | Duration |",
        "|---|---|---|---:|",
    ]
    for elapsed, binary, name, status in sorted(cases, reverse=True)[:limit]:
        lines.append(
            f"| {cell(binary)} | {cell(name)} | {status} | {elapsed:.3f}s |"
        )
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timings-dir", type=Path, required=True)
    parser.add_argument("--cache-hit", default="")
    parser.add_argument("reports", type=Path, nargs="+")
    args = parser.parse_args()
    print(summarize(args.timings_dir, args.reports, args.cache_hit), end="")


if __name__ == "__main__":
    main()
