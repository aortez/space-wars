#!/usr/bin/env python3
"""Audit the retained Round opening comparisons without rerunning physics."""
import argparse
from collections import Counter
import hashlib
import importlib.util
import io
import itertools
import json
import math
from pathlib import Path
import struct
import subprocess
import tarfile


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def equal_range(left, right, start, end):
    left = (r for r in left if start <= r["tick"] < end)
    right = (r for r in right if start <= r["tick"] < end)
    count = 0
    for a, b in itertools.zip_longest(left, right):
        assert a is not None and b is not None, ("retained length", start, end, count)
        assert a == b, ("retained row differs", a["tick"])
        count += 1
    return count


def local_actor(pilot):
    actor = pilot["actor"]
    if actor is None:
        return None
    frame = pilot["planet"]["motion"]
    x = actor["position"]["x"] - frame["position"]["x"]
    y = actor["position"]["y"] - frame["position"]["y"]
    c, s = math.cos(frame["angle"]), math.sin(frame["angle"])
    return x * c + y * s, -x * s + y * c


def check_probe(probe):
    if "unavailable" in probe:
        return
    assert probe["physics_unchanged"] and not probe["controller_input"]
    assert probe["maximum_sweep_distance"] == 6
    assert len(probe["straight_sweeps"]) == 8 and len(probe["ungated_crawl_queries"]) == 2
    for sweep in probe["straight_sweeps"]:
        assert sweep["clearance"] is None or 0 <= sweep["clearance"] <= 6
    for crawl in probe["ungated_crawl_queries"]:
        assert crawl["clearance"] is None or 0 <= crawl["clearance"] <= 0.640001
        assert crawl["distance_after_margin"] is None or 0 <= crawl["distance_after_margin"] <= 0.600001


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("footing", type=Path)
    parser.add_argument("geometry", type=Path)
    args = parser.parse_args()
    helpers = Path(__file__).parent
    audit = module("integration_audit", helpers / "audit-terrain-recovery-integration.py")
    geometry = module("geometry_audit", helpers / "audit-terrain-recovery-round-geometry.py")
    root, footing, prior = args.root.resolve(), args.footing.resolve() / "output", args.geometry.resolve() / "output"
    runtime = audit.read(root / "result.json")
    assert runtime["exit_code"] == 0 and runtime["fork_ticks"] == [25500, 25800]
    assert audit.digest(root / "round-opening-comparison") == runtime["binary_sha256"]
    assert audit.digest(root / "source-inputs.json") == runtime["source_inputs_sha256"]
    inputs = audit.read(root / "source-inputs.json")
    repo = helpers.parent.resolve()
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
    prefix_ticks = equal_range(audit.rows(out / "prefix-actions.jsonl"),
                               audit.rows(prior / "control-actions.jsonl"), 0, 25800)
    assert prefix_ticks == 25800
    old_samples = lambda: itertools.chain(audit.rows(footing / "prefix-samples.jsonl"),
                                          audit.rows(footing / "blocked/run-samples.jsonl"))
    prefix_samples = equal_range(audit.rows(out / "prefix-samples.jsonl"), old_samples(), 0, 25801)
    assert prefix_samples == 430
    cases = []
    for name, tick in (("opening-25500", 25500), ("balanced-25800", 25800)):
        path = out / name
        fork = audit.read(path / "fork.json")
        old = next(r["state"] for r in old_samples() if r["tick"] == tick)
        assert fork == old
        initial_material = audit.read(path / "fork-material.json")
        old_material = next(r for r in audit.rows(prior / "material-samples.jsonl") if r["tick"] == tick)
        assert initial_material == old_material
        fork_component = geometry.component(initial_material, True)
        assert fork_component["failure"] is None and fork_component["exterior_connected"]
        probe = audit.read(path / "fork-clearance.json")
        check_probe(probe)
        retained_ticks = equal_range(audit.rows(path / "blocked/run-actions.jsonl"),
                                     audit.rows(prior / "control-actions.jsonl"), tick, 34500)
        assert retained_ticks == 34500 - tick
        retained_samples = equal_range(audit.rows(path / "blocked/run-samples.jsonl"), old_samples(), tick + 1, 34501)
        assert retained_samples == (34500 - tick) // 60
        arms = []
        for arm in ("blocked", "existing", "search-arrival", "full"):
            here = path / arm
            summary = audit.read(here / "summary.json")
            initial = audit.read(here / "before-first-decision.json")
            assert initial["scenario"] == fork["scenario"] and initial["brains"] == fork["brains"]
            comparison = audit.stream_difference(path / "blocked/run-actions.jsonl", here / "run-actions.jsonl")
            assert comparison["ticks"] == 18000
            fresh_comparison = audit.stream_difference(path / "existing/run-actions.jsonl", here / "run-actions.jsonl")
            controls = Counter()
            for r in audit.rows(here / "run-actions.jsonl"):
                for action in r["actions"]:
                    a = action.get("Scenario", {})
                    if a.get("kind") == 0x53550002 and a["payload"][7] == 0:
                        payload = a["payload"]
                        controls["horizontal_ticks"] += struct.unpack("<f", bytes(payload[:4]))[0] != 0
                        controls["primary_ticks"] += bool(payload[4])
                        controls["interact_ticks"] += bool(payload[5])
                        controls["brake_ticks"] += bool(payload[6])
            posture_rows = list(audit.rows(here / "posture.jsonl"))
            assert [r["tick"] for r in posture_rows] == list(range(tick, tick + 900))
            postures = Counter()
            first_posture = {}
            max_getups = 0
            for r in posture_rows:
                p = r["posture"]
                if p:
                    key = f'{p["balance"]}:{p["get_up_result"]}'
                    postures[key] += 1
                    first_posture.setdefault(key, r["tick"])
                    max_getups = max(max_getups, p["get_up_attempts"])
            probes = list(audit.rows(here / "clearance.jsonl"))
            materials = list(audit.rows(here / "material.jsonl"))
            assert len(probes) == len(materials) == 30
            material_receipts = []
            for index, (clearance, material) in enumerate(zip(probes, materials, strict=True)):
                check_probe(clearance)
                assert material["tick"] == tick + index * 30 and material["physical_state_unchanged"]
                assert "unavailable" in clearance or clearance["tick"] == material["tick"]
                component = geometry.component(material, True)
                material_receipts.append({"tick": material["tick"], "actor_local": material["actor_local"],
                                          "component": component, "posture": material["posture"]})
            first_block = None
            if summary["trace"]["first_block"][0] is not None:
                state = audit.read(here / "first-block.json")
                check_probe(audit.read(here / "first-block-clearance.json"))
                first_block = {"brain": state["brains"][0], "pilot": audit.pilot(state, 0),
                               "posture": state["pilots"][0]["recovery"]["posture"]}
            completion = None
            if summary["trace"]["first_completion"][0] is not None:
                state = audit.read(here / "first-completion.json")
                p = audit.pilot(state, 0)
                assert p["location"] == {"aboard": 0} and p["ship_form"] == "ship"
                assert p["landing"]["phase"] == "landed" and p["landing"]["supported_feet"] == 2
                assert state["brains"][0]["recovery"]["status"] == "succeeded"
                completion = {"pilot": p, "brain": state["brains"][0]}
            origin = local_actor(audit.pilot(fork, 0))
            max_distance = 0
            first_rebuilt = None
            sample_count = 0
            for row in audit.rows(here / "run-samples.jsonl"):
                sample_count += 1
                p = audit.pilot(row["state"], 0)
                actor = local_actor(p)
                if actor is not None:
                    max_distance = max(max_distance, math.dist(origin, actor))
                if p["recovery"]["rebuilds"] and first_rebuilt is None:
                    first_rebuilt = row["tick"]
            assert sample_count == 300
            final = audit.read(here / "final.json")
            result = {"arm": arm, "summary": summary, "comparison_to_blocked": comparison,
                      "comparison_to_fresh_ordinary": fresh_comparison,
                      "controls": dict(controls), "posture_counts_first_900_ticks": dict(postures),
                      "first_posture_ticks": first_posture, "get_up_attempts_first_900_ticks": max_getups,
                      "max_sampled_local_displacement": max_distance, "first_sampled_native_rebuild": first_rebuilt,
                      "first_block": first_block, "completion": completion, "early_material": material_receipts,
                      "final_pilot": audit.pilot(final, 0),
                      "features": audit.feature_receipt(here / "candidate-features.jsonl") if summary["profile"] else None}
            arms.append(result)
            print(json.dumps({"fork": tick, "arm": arm, "block": summary["trace"]["first_block"][0],
                              "completion": summary["trace"]["first_completion"][0], "controls": dict(controls),
                              "get_up_attempts": max_getups, "max_sampled_local_displacement": max_distance}))
        cases.append({"case": name, "fork_tick": tick, "retained_ticks": retained_ticks,
                      "retained_samples": retained_samples, "fork_component": fork_component,
                      "fork_clearance": probe, "arms": arms})
    overlapping_ticks = equal_range(audit.rows(out / "opening-25500/blocked/run-actions.jsonl"),
                                    audit.rows(out / "balanced-25800/blocked/run-actions.jsonl"), 25800, 43500)
    assert overlapping_ticks == 17700
    receipts = {str(p.relative_to(root)): {"bytes": p.stat().st_size, "sha256": audit.digest(p)}
                for p in sorted(out.rglob("*")) if p.is_file()}
    report = {"schema": 1, "runtime": runtime, "prefix_ticks": prefix_ticks, "prefix_samples": prefix_samples,
              "overlapping_blocked_ticks": overlapping_ticks, "cases": cases, "artifact_receipts": receipts,
              "auditor_sha256": audit.digest(Path(__file__)),
              "helper_sha256": audit.digest(helpers / "audit-terrain-recovery-integration.py"),
              "geometry_helper_sha256": audit.digest(helpers / "audit-terrain-recovery-round-geometry.py")}
    (root / "audit.json").write_text(json.dumps(report, indent=2) + "\n")
    print(f"Verified {len(inputs)} frozen inputs and {len(receipts)} evidence files.")


if __name__ == "__main__":
    main()
