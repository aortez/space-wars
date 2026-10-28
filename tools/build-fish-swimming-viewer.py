#!/usr/bin/env python3
"""Build a self-contained before/after viewer from Clock swimming captures."""

import argparse
import gzip
import json
from pathlib import Path


def compact(value):
    if isinstance(value, float):
        return round(value, 2)
    if isinstance(value, list):
        return [compact(item) for item in value]
    if isinstance(value, dict):
        return {key: compact(item) for key, item in value.items()}
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    captures = [
        json.loads(
            gzip.decompress(path.read_bytes()) if path.suffix == ".gz" else path.read_text()
        )
        for path in (args.before, args.after)
    ]
    if len(captures[0]) != len(captures[1]):
        parser.error("before and after must contain the same clips")
    for before, after in zip(*captures):
        for key in ("name", "seed", "fps", "start", "bounds", "obstacles"):
            if compact(before[key]) != compact(after[key]):
                parser.error(f"clip {before['name']}: mismatched {key}")
        if len(before["frames"]) != len(after["frames"]):
            parser.error("before and after must have the same duration")
        for clip in (before, after):
            if clip.get("pose") not in ("heading", "pitch-facing"):
                parser.error("capture must declare heading or pitch-facing pose encoding")
            if clip["failed"] or clip["exited"] != 12:
                parser.error("captures must retain twelve successful physical exits")
    data = json.dumps(compact(captures), separators=(",", ":"), allow_nan=False)
    template = Path(__file__).with_name("fish-swimming-viewer.html.in").read_text()
    result = template.replace("/*__FISH_DATA__*/", data)
    if len(result.encode()) >= 1_000_000:
        parser.error("viewer exceeds the inline size budget")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(result)
    print(f"Wrote {args.output} ({len(result.encode()):,} bytes)")


if __name__ == "__main__":
    main()
