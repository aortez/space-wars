"""Verify that PR UI coverage differs from the full suite only by approved cases."""

import argparse
import json
from pathlib import Path
import sys


# Intentionally independent of the nextest filter: detect typos, renamed tests,
# broad exclusions and unexpected changes in the actual discovered selection.
DEFERRED = {
    "spacewars_match::normal_spacewars_physical_round_reaches_result_and_play_again",
    "clock::demo_profile_automatically_runs_multiple_bounded_events",
    "clock::rain_settings_preview_pause_cleanup_and_persistence",
    "clock::duck_runs_jumps_exits_and_supports_live_controls_and_cleanup",
}


def selected_ui_tests(report):
    selected = set()
    for suite in report["rust-suites"].values():
        for name, case in suite["testcases"].items():
            if case["filter-match"]["status"] != "matches":
                continue
            if (suite["package-name"] != "engine-client"
                    or suite["binary-name"] != "ui_control_functional"
                    or case["ignored"] is not True):
                raise ValueError(f"Unexpected non-UI test selected: {name}")
            if name in selected:
                raise ValueError(f"Duplicate UI test selected: {name}")
            selected.add(name)
    if not selected:
        raise ValueError("No UI tests selected")
    return selected


def assert_ui_selection(full_report, pr_report):
    full = selected_ui_tests(full_report)
    pr = selected_ui_tests(pr_report)
    stale = DEFERRED - full
    if stale:
        raise ValueError(f"Deferred tests missing from full suite: {sorted(stale)}")
    missing = (full - DEFERRED) - pr
    unexpected = pr - (full - DEFERRED)
    if missing or unexpected:
        raise ValueError(
            f"Unexpected PR UI selection: missing={sorted(missing)}, "
            f"unexpected={sorted(unexpected)}"
        )
    return f"UI selection verified: {len(pr)} PR workflows; {len(DEFERRED)} deferred; {len(full)} total."


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("full_report", type=Path)
    parser.add_argument("pr_report", type=Path)
    args = parser.parse_args()
    try:
        print(assert_ui_selection(
            json.loads(args.full_report.read_text()), json.loads(args.pr_report.read_text()),
        ))
    except (OSError, ValueError, KeyError) as error:
        print(f"UI selection check failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
