#!/usr/bin/env python3
"""Compare serial match surveys and optional identical-edit geometry workloads."""
import argparse
import json
import pathlib
import statistics


def read(path):
    return json.loads(path.read_text())


def weighted_timing(reports, key):
    values = [r[key] for r in reports]
    count = sum(v["count"] for v in values)
    return dict(count=count,
                mean_ms=sum(v["mean_ms"] * v["count"] for v in values) / count,
                over_16_67_ms=sum(v["over_16_67_ms"] for v in values),
                median_case_p95_ms=statistics.median(v["p95_ms"] for v in values))


def load_survey(root):
    manifest = read(root / "manifest.json")
    assert manifest["jobs"] == 1 and manifest["measure_draw"], "serial measured runs required"
    rows = read(root / "summary.json")
    assert len(rows) == len(manifest["seeds"]) * len(manifest["asteroid_intervals"])
    reports = {}
    for row in rows:
        assert row["exit_code"] == 0 and row["physics_ok"], row["case"]
        report = read(root / row["case"] / "report.json")
        assert report["steps"]["count"] == report["measured_tick"]["count"] == report["elapsed_ticks"]
        reports[row["case"]] = report
    return manifest, rows, reports


def aggregate(rows, reports):
    values = list(reports.values())
    audits = [a for r in values for a in [r["initial_audit"], r["final_audit"]]
              + [s["audit"] for s in r["samples"]]]
    return dict(runs=len(rows), physics_passes=sum(r["physics_ok"] for r in rows),
                simulated_seconds=sum(r["simulated_seconds"] for r in rows),
                finished=sum(r["termination"] == "round_finished" for r in rows),
                claims=sum(r["claims"] for r in rows),
                sorties=sum(sum(r["completed_sorties"]) for r in rows),
                weapon_contact_runs=sum(r["weapon_contact"] for r in rows),
                ships_lost=sum(sum(r["ships_lost"]) for r in rows),
                rebuilds=sum(sum(r["rebuilds"]) for r in rows),
                peak_sampled_colliders=max(a["terrain_colliders"] for a in audits),
                peak_sampled_fragments=max(a["fragments"] for a in audits),
                peak_sampled_speed=max(a["max_speed"] for a in audits),
                timing={key: weighted_timing(values, key) for key in
                        ["steps", "sensors", "policy", "draw_lists", "measured_tick"]})


def geometry(before, after):
    comparisons = []
    assert {p.name for p in before.iterdir()} == {p.name for p in after.iterdir()}
    for source in sorted(before.iterdir()):
        a = read(source / "report.json")
        b = read(after / source.name / "report.json")
        assert a["seed"] == b["seed"] and a["repeats"] == b["repeats"]
        assert len(a["reports"]) == len(b["reports"])
        for x, y in zip(a["reports"], b["reports"]):
            assert (x["planet"], x["repeat"], x["radius"]) == (y["planet"], y["repeat"], y["radius"])
            assert len(x["samples"]) == len(y["samples"]) == 60
            for left, right in [(x["initial"], y["initial"])] + [
                    (u["geometry"], v["geometry"]) for u, v in zip(x["samples"], y["samples"])]:
                for key in ["material_hash", "occupied_cells", "cell_bytes"]:
                    assert left[key] == right[key], (source.name, x["planet"], key)
            for u, v in zip(x["samples"], y["samples"]):
                assert u["changed_cells"] == v["changed_cells"]
            comparisons.append(dict(seed=a["seed"], planet=x["planet"], repeat=x["repeat"],
                                    before={k: x[k] for k in ["initial_build_ms", "initial", "edits", "refresh"]},
                                    after={k: y[k] for k in ["initial_build_ms", "initial", "edits", "refresh"]},
                                    final_shapes=[x["samples"][-1]["geometry"]["shapes"],
                                                  y["samples"][-1]["geometry"]["shapes"]]))
    return dict(material_equal_after_every_cut=True, comparisons=comparisons,
                means={side: {metric: statistics.mean(
                    r[side][metric]["mean_ms"] if metric != "initial_build_ms" else r[side][metric]
                    for r in comparisons) for metric in ["initial_build_ms", "edits", "refresh"]}
                       for side in ["before", "after"]})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["before", "after", "out"]:
        parser.add_argument("--" + name, type=pathlib.Path, required=True)
    parser.add_argument("--before-geometry", type=pathlib.Path)
    parser.add_argument("--after-geometry", type=pathlib.Path)
    args = parser.parse_args()
    assert bool(args.before_geometry) == bool(args.after_geometry)
    ma, ra, a = load_survey(args.before)
    mb, rb, b = load_survey(args.after)
    for key in ["seeds", "asteroid_intervals", "seconds", "combat_break_interval", "combat_break_duration",
                "world", "mode", "match_rules", "mirror", "measure_draw"]:
        assert ma[key] == mb[key], key
    assert a.keys() == b.keys()
    pairs = []
    for case in a:
        x, y = a[case], b[case]
        assert x["initial_audit"]["surface_sample_bytes"] == 0
        assert y["initial_audit"]["surface_sample_bytes"] > 0
        assert x["initial_audit"]["occupied_cells"] == y["initial_audit"]["occupied_cells"]
        assert x["initial_world"]["planets"] == y["initial_world"]["planets"]
        assert x["initial_world"]["sun"] == y["initial_world"]["sun"]
        pairs.append(dict(case=case, seconds=[x["elapsed_ticks"] / 60, y["elapsed_ticks"] / 60],
                          outcomes=[x["round"]["outcome"], y["round"]["outcome"]],
                          tick_p95_ms=[x["measured_tick"]["p95_ms"], y["measured_tick"]["p95_ms"]],
                          step_p95_ms=[x["steps"]["p95_ms"], y["steps"]["p95_ms"]],
                          draw_p95_ms=[x["draw_lists"]["p95_ms"], y["draw_lists"]["p95_ms"]],
                          initial_colliders=[x["initial_audit"]["terrain_colliders"], y["initial_audit"]["terrain_colliders"]],
                          initial_sample_bytes=y["initial_audit"]["surface_sample_bytes"]))
    result = dict(version=1, before=aggregate(ra, a), after=aggregate(rb, b), pairs=pairs,
                  inputs={"before":ma, "after":mb},
                  caveat="Timing includes differing gameplay trajectories and durations. Median case P95 is not a pooled P95. Geometry means use identical edits; timings exclude audit/report IO and display presentation.")
    if args.before_geometry:
        result["geometry"] = geometry(args.before_geometry, args.after_geometry)
    args.out.mkdir(parents=True, exist_ok=False)
    (args.out / "comparison.json").write_text(json.dumps(result, indent=2) + "\n")
    lines = ["| Case | Seconds before / after | Tick P95 ms before / after | Initial colliders before / after |",
             "| --- | --- | --- | --- |"]
    for row in pairs:
        lines.append(f"| {row['case']} | " + " / ".join(f"{v:.2f}" for v in row['seconds']) + " | "
                     + " / ".join(f"{v:.3f}" for v in row['tick_p95_ms']) + " | "
                     + " / ".join(str(v) for v in row['initial_colliders']) + " |")
    (args.out / "table.md").write_text("\n".join(lines) + "\n")
    print(json.dumps({side: result[side] for side in ["before", "after"]}, indent=2))


if __name__ == "__main__":
    main()
