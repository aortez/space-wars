"""Verify explicit UI coverage classification against nextest discovery."""

import argparse
import json
from pathlib import Path
import sys


MANIFEST = Path(__file__).with_name("ui-tests.json")


def classified_tests(manifest):
    if not isinstance(manifest, dict) or set(manifest) != {"smoke", "full_only"}:
        raise ValueError("UI manifest must contain smoke and full_only lists")
    if not isinstance(manifest["smoke"], list) or not isinstance(manifest["full_only"], list):
        raise ValueError("UI manifest classifications must be lists")
    smoke = set()
    full_only = set()

    def add(names, name):
        if not isinstance(name, str) or not name.strip() or name != name.strip():
            raise ValueError(f"Invalid UI test name: {name!r}")
        if name in smoke or name in full_only:
            raise ValueError(f"Duplicate UI classification: {name}")
        names.add(name)

    for name in manifest["smoke"]:
        add(smoke, name)
    if not smoke:
        raise ValueError("No PR smoke tests classified")
    for entry in manifest["full_only"]:
        if not isinstance(entry, dict) or set(entry) != {"test", "reason"}:
            raise ValueError("Each full-only UI test needs a test name and reason")
        if not isinstance(entry["reason"], str) or not entry["reason"].strip():
            raise ValueError(f"Missing deferral reason for {entry['test']!r}")
        add(full_only, entry["test"])
    return smoke, full_only


def is_ui_case(suite, case):
    return (suite["package-name"] == "engine-client"
            and suite["binary-name"] == "ui_control_functional"
            and case["ignored"] is True)


def selected_ui_tests(report):
    selected = set()
    for suite in report["rust-suites"].values():
        for name, case in suite["testcases"].items():
            if case["filter-match"]["status"] != "matches":
                continue
            if not is_ui_case(suite, case):
                raise ValueError(f"Unexpected non-UI test selected: {name}")
            if name in selected:
                raise ValueError(f"Duplicate UI test selected: {name}")
            selected.add(name)
    if not selected:
        raise ValueError("No UI tests selected")
    return selected


def assert_ui_selection(full_report, pr_report, manifest):
    smoke, full_only = classified_tests(manifest)
    full = selected_ui_tests(full_report)
    pr = selected_ui_tests(pr_report)
    discovered = {
        name for suite in full_report["rust-suites"].values()
        for name, case in suite["testcases"].items() if is_ui_case(suite, case)
    }
    if full != discovered:
        raise ValueError(f"Full UI selection omitted eligible tests: {sorted(discovered - full)}")
    unclassified = full - (smoke | full_only)
    stale = (smoke | full_only) - full
    if unclassified or stale:
        raise ValueError(
            f"UI classification mismatch: unclassified={sorted(unclassified)}, stale={sorted(stale)}"
        )
    missing = smoke - pr
    unexpected = pr - smoke
    if missing or unexpected:
        raise ValueError(
            f"Unexpected PR UI selection: missing={sorted(missing)}, "
            f"unexpected={sorted(unexpected)}"
        )
    return f"UI selection verified: {len(pr)} PR smoke workflows; {len(full_only)} full-only; {len(full)} total."


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("full_report", type=Path)
    parser.add_argument("pr_report", type=Path)
    parser.add_argument("--manifest", type=Path, default=MANIFEST)
    args = parser.parse_args()
    try:
        print(assert_ui_selection(
            json.loads(args.full_report.read_text()), json.loads(args.pr_report.read_text()),
            json.loads(args.manifest.read_text()),
        ))
    except (OSError, ValueError, KeyError) as error:
        print(f"UI selection check failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
