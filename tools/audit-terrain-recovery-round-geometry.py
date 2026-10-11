#!/usr/bin/env python3
"""Check native empty-cell connectivity in the unchanged Round continuation."""
import argparse
from collections import deque
import hashlib
import importlib.util
import io
import itertools
import json
import math
from pathlib import Path
import subprocess
import tarfile


def component(sample, diagonal):
    width, height = sample["width"], sample["height"]
    cells = sample["material_ids_row_major"]
    assert len(cells) == width * height
    start = sample["actor_cell"]
    if start is None:
        return {"failure": "no_actor"}
    start = (start["x"], start["y"])
    inside = lambda x, y: 0 <= x < width and 0 <= y < height
    if not inside(*start):
        return {"failure": "outside_field"}
    if cells[start[1] * width + start[0]]:
        return {"failure": "actor_center_in_material"}
    directions = [(x, y) for x in (-1, 0, 1) for y in (-1, 0, 1)
                  if (x or y) and (diagonal or abs(x) + abs(y) == 1)]
    seen, boundary, pending = {start}, set(), deque([start])
    exterior = False
    while pending:
        x, y = pending.popleft()
        for dx, dy in directions:
            point = (x + dx, y + dy)
            if not inside(*point):
                exterior = True
            elif cells[point[1] * width + point[0]]:
                boundary.add(point)
            elif point not in seen:
                seen.add(point)
                pending.append(point)
    return {"failure": None, "exterior_connected": exterior, "cell_count": len(seen),
            "cells": sorted(seen) if not exterior else None,
            "occupied_boundary": sorted(boundary) if not exterior else None}


def rendered_enclosure(frame, fork, sample, occupied):
    motion = fork["pilots"][0]["recovery"]["flight"]["pilot"]["planet"]["motion"]
    cosine, sine = math.cos(motion["angle"]), math.sin(motion["angle"])

    def local(p):
        x, y = p["x"] - motion["position"]["x"], p["y"] - motion["position"]["y"]
        return (x * cosine + y * sine, -x * sine + y * cosine)

    rectangles = []
    for layer in frame["layers"]:
        if layer["z"] != -10:
            continue
        for primitive in layer["primitives"]:
            polygon = primitive.get("Polygon")
            if not polygon or not polygon.get("fill") or len(polygon["points"]) != 4:
                continue
            points = [local(p) for p in polygon["points"]]
            x0, x1 = min(p[0] for p in points), max(p[0] for p in points)
            y0, y1 = min(p[1] for p in points), max(p[1] for p in points)
            if all(min(abs(x - x0), abs(x - x1)) < 0.0002
                   and min(abs(y - y0), abs(y - y1)) < 0.0002 for x, y in points):
                rectangles.append((x0, y0, x1, y1))
    size = sample["cell_size"]
    receipts = []
    for x, y in occupied:
        left, bottom = (x - sample["width"] / 2) * size, (y - sample["height"] / 2) * size
        covering = [r for r in rectangles if r[0] <= left + 0.0002 and r[1] <= bottom + 0.0002
                    and r[2] >= left + size - 0.0002 and r[3] >= bottom + size - 0.0002]
        assert covering, ("native enclosing cell lacks a matching rendered solid rectangle", x, y)
        receipts.append({"cell": [x, y], "local_bounds": [left, bottom, left + size, bottom + size],
                         "rendered_rectangle": covering[0]})
    return receipts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("comparison", type=Path)
    args = parser.parse_args()
    helper = Path(__file__).with_name("audit-terrain-recovery-integration.py")
    spec = importlib.util.spec_from_file_location("integration_audit", helper)
    audit = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(audit)
    root, reference = args.root.resolve(), args.comparison.resolve() / "output"
    runtime = audit.read(root / "result.json")
    assert runtime["exit_code"] == 0
    assert audit.digest(root / "round-geometry-inspection") == runtime["binary_sha256"]
    manifest = root / "source-inputs.json"
    assert audit.digest(manifest) == runtime["source_inputs_sha256"]
    inputs = audit.read(manifest)
    repo = Path(__file__).resolve().parent.parent
    frozen = subprocess.check_output(["git", "archive", runtime["source_commit"]], cwd=repo)
    checked = set()
    with tarfile.open(fileobj=io.BytesIO(frozen)) as archive:
        for item in archive:
            if item.name not in inputs:
                continue
            payload = item.linkname.encode() if item.issym() else archive.extractfile(item).read()
            assert hashlib.sha256(payload).hexdigest() == inputs[item.name], item.name
            checked.add(item.name)
    assert checked == set(inputs)
    out = root / "output"
    old = itertools.chain(audit.rows(reference / "prefix-actions.jsonl"),
                          audit.rows(reference / "blocked/run-actions.jsonl"))
    count = 0
    for a, b in itertools.zip_longest(old, audit.rows(out / "control-actions.jsonl")):
        assert a is not None and b is not None and a == b, ("control differs", count)
        count += 1
    assert count == 34500
    retained = []
    for name, prior in (("fork", reference / "fork"), ("final", reference / "blocked/final")):
        for suffix in (".json", "-controllers.txt", "-frame-p1.json", "-frame-p2.json"):
            assert audit.digest(out / (name + suffix)) == audit.digest(Path(str(prior) + suffix)), (name, suffix)
            retained.append(name + suffix)
    samples = []
    first = None
    for row in audit.rows(out / "material-samples.jsonl"):
        if first is None:
            first = row
        assert row["physical_state_unchanged"]
        four, eight = component(row, False), component(row, True)
        record = {k: v for k, v in row.items() if k != "material_ids_row_major"}
        record.update(four_neighbor=four, eight_neighbor=eight)
        samples.append(record)
    assert [s["tick"] for s in samples] == list(range(16500, 34501, 60))
    initial = samples[0]["eight_neighbor"]
    boundary = None
    if initial["failure"] is None and not initial["exterior_connected"]:
        boundary = rendered_enclosure(audit.read(out / "fork-frame-p1.json"),
                                      audit.read(out / "fork.json"), first, initial["occupied_boundary"])
    open_samples = [s["tick"] for s in samples if s["eight_neighbor"].get("exterior_connected")]
    failures = [{"tick": s["tick"], "failure": s["eight_neighbor"]["failure"]}
                for s in samples if s["eight_neighbor"]["failure"]]
    receipts = {str(p.relative_to(root)): {"bytes": p.stat().st_size, "sha256": audit.digest(p)}
                for p in sorted(out.rglob("*")) if p.is_file()}
    report = {"schema": 1, "runtime": runtime, "retained_ticks": count,
              "retained_files": retained, "samples": samples, "fork_rendered_boundary": boundary,
              "first_sample_with_exterior_connection": open_samples[0] if open_samples else None,
              "open_sample_count": len(open_samples), "sample_failures": failures,
              "artifact_receipts": receipts, "auditor_sha256": audit.digest(Path(__file__)),
              "helper_sha256": audit.digest(helper)}
    (root / "audit.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({"fork_component": initial, "first_open_sample": report["first_sample_with_exterior_connection"],
                      "open_samples": len(open_samples), "failures": failures}))
    print(f"Verified {len(inputs)} frozen inputs, {count} retained ticks, {len(samples)} material samples and {len(receipts)} files.")


if __name__ == "__main__":
    main()
