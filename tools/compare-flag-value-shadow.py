#!/usr/bin/env python3
"""Replay fixed engineering cases, requiring unchanged bots and flag surveys."""
import argparse
from collections import Counter
import gzip
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess

SPEC = importlib.util.spec_from_file_location("flags", Path(__file__).with_name("compare-flag-surveys.py"))
F = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(F)


def rows(path):
    with path.open() as stream:
        return [json.loads(line) for line in stream]


def preference(report):
    return report.get("value_comparison", {}).get("preferred")


def validate_reports(reports, evaluations, samples):
    baselines = {(r["actor"], r["source_tick"]): r for r in evaluations}
    surveys = {(s["actor"], s["source_tick"], s["site"]["bearing"]): s for s in samples}
    rejections, unknown = Counter(), Counter()
    used, changed, complete, numeric_added = [], [], 0, 0
    last_admission, seen_sources = {}, set()
    for r in reports:
        assert r["model"] == "capture_flag_value_shadow_v1" and r["observational"]
        b, a = r["baseline"], r["augmented"]
        identity = (r["actor"], b["source_tick"])
        assert identity not in seen_sources
        seen_sources.add(identity)
        assert b == baselines[(r["actor"], b["source_tick"])]
        assert b["policy"] == "material_mission_v13"
        assert b["source_tick"] <= b["completed_tick"] <= r["admitted_tick"] <= r["completed_tick"]
        if r["actor"] in last_admission:
            assert r["admitted_tick"] - last_admission[r["actor"]] >= 60
        last_admission[r["actor"]] = r["admitted_tick"]
        assert a["source_tick"] == b["source_tick"]
        assert a["transfer_source"] == b["transfer_source"]
        assert a["match_context"] == b["match_context"]
        assert len(a["candidates"]) <= 3
        assert [c["planet"] for c in a["candidates"]] == [c["planet"] for c in b["candidates"]]
        derived = {"model", "completed_tick", "charged_work", "candidates", "fastest_supported",
                   "preferred_by_time", "value_comparison", "comparison_reason"}
        assert {k: v for k, v in a.items() if k not in derived} == {k: v for k, v in b.items() if k not in derived}
        assert a["charged_work"] == len(a["candidates"]) + 1
        assert a["completed_tick"] == r["completed_tick"]
        assert len(r["admissions"]) <= 2
        patched = set()
        for item in r["admissions"]:
            if item["reason"]:
                rejections[item["reason"]] += 1
            if not item["used"]:
                continue
            assert item["reason"] is None
            s = surveys[(r["actor"], item["source_tick"], item["site"]["bearing"])]
            assert s["reason"] is None and item["site"] == s["site"]
            assert item["generation"] == s["generation"] <= item["source_tick"]
            assert item["source_tick"] <= item["completed_tick"] == item["validated_tick"] <= b["source_tick"]
            assert item["completed_tick"] == s["completed_tick"]
            assert item["source_objective"] == s["validation"]["source_objective"]
            assert item["source_age_ticks"] == r["admitted_tick"] - s["source_tick"] <= 1800
            planet = item["site"]["planet"]
            assert planet not in patched
            patched.add(planet)
            old = next(c for c in b["candidates"] if c["planet"] == planet)
            new = next(c for c in a["candidates"] if c["planet"] == planet)
            assert old["local"] is None and not old["current"]
            assert old["unknown_reason"] in {"remote or local surface unmeasured", "objective route unmeasured", "site round trip unmeasured"}
            assert new["site"] == s["site"] and new["evidence_tick"] == s["source_tick"]
            assert new["route_source_tick"] == s["source_tick"] and new["route_validated_tick"] == s["validated_tick"]
            costs = dict(landing=23.033333, exit=1/60,
                         outbound=s["route"]["outbound"]["length"] / 5 + 0.15833333,
                         claim=6 - 0.5/60, return_board=s["route"]["returning"]["length"] / 5 + 2/60,
                         departure=3.8166666)
            assert set(new["local"]) == set(costs)
            assert all(math.isclose(new["local"][k], v, abs_tol=1e-5) for k, v in costs.items())
            modified = {"site", "local", "unknown_reason", "evidence_tick", "evidence_age_ticks",
                        "route_source_tick", "route_validated_tick", "evidence_kind", "travel_seconds",
                        "transfer", "total_seconds", "reference_exceeds_match_time", "value"}
            assert {k: v for k, v in old.items() if k not in modified} == {k: v for k, v in new.items() if k not in modified}
            assert {k: v for k, v in old["value"].items() if k != "seconds_per_unit"} == {
                k: v for k, v in new["value"].items() if k != "seconds_per_unit"}
            if new["unknown_reason"] is not None:
                assert new["total_seconds"] is None and new["value"]["seconds_per_unit"] is None
            else:
                transfer = new["transfer"]
                assert math.isclose(new["travel_seconds"], sum(transfer[k] for k in
                    ["settle_seconds", "turn_seconds", "climb_seconds", "cruise_seconds"]), abs_tol=1e-4)
                assert math.isclose(new["total_seconds"], sum(costs.values()) + new["travel_seconds"], abs_tol=1e-4)
                units = new["value"]["priority_units"]
                assert units > 0 and math.isclose(new["value"]["seconds_per_unit"], new["total_seconds"] / units, abs_tol=1e-5)
                remaining = b["match_context"].get("remaining_seconds") if b["match_context"] else None
                assert new["reference_exceeds_match_time"] == (new["total_seconds"] > remaining if remaining is not None else None)
            numeric_added += new["total_seconds"] is not None
            used.append(dict(actor=r["actor"], source_tick=s["source_tick"], site=s["site"],
                             comparison_source=b["source_tick"], admitted_tick=r["admitted_tick"],
                             reference_seconds=new["total_seconds"], reason=new["unknown_reason"]))
        for old, new in zip(b["candidates"], a["candidates"]):
            assert old["planet"] == new["planet"]
            if old["planet"] not in patched:
                assert old == new
            if new["unknown_reason"]:
                unknown[new["unknown_reason"]] += 1
        if r["completion_reason"]:
            assert preference(a) is None and a["preferred_by_time"] is None and not r["preference_changed"]
        else:
            assert r["completed_tick"] - b["source_tick"] <= 120
            assert all(r["completed_tick"] - i["source_tick"] <= 1800 for i in r["admissions"] if i["used"])
            assert r["preference_changed"] == (preference(b) != preference(a))
        supported = all(c["total_seconds"] is not None for c in a["candidates"])
        eligible = [(c["value"]["seconds_per_unit"], c["planet"]) for c in a["candidates"]
                    if c["total_seconds"] is not None and c.get("reference_exceeds_match_time") is not True]
        expected = min(eligible)[1] if (eligible and supported and not a.get("inactive_reason")
                                       and not a.get("candidates_truncated") and not r["completion_reason"]) else None
        assert preference(a) == expected
        if preference(a) is not None:
            complete += 1
        if r["preference_changed"]:
            changed.append(dict(actor=r["actor"], source_tick=b["source_tick"],
                                baseline=preference(b), augmented=preference(a)))
    unique = {(r["actor"], r["source_tick"], r["site"]["bearing"]) for r in used}
    return dict(reports=len(reports), used_references=len(used), unique_surveys_used=len(unique),
                numeric_candidates_added=numeric_added, complete_value_comparisons=complete,
                preference_changes=changed, admissions_rejected=dict(rejections),
                candidate_unknown=dict(unknown), used=used,
                scope="Repeated historical comparisons are not independent opportunities or strength evidence.")


def validate_work(flag_rows, shadow_rows, summary, reports):
    assert len(flag_rows) == len(shadow_rows)
    total, maximum = 0, 0
    for flag, shadow in zip(flag_rows, shadow_rows):
        assert flag["tick"] == shadow["tick"]
        previous = F.charge(flag)
        available = shadow["remaining_after_flag_survey"]
        charged = shadow["charged"]
        assert available == dict(graph=4 - previous["graph"], physics_queries=0)
        assert 0 <= charged["graph"] <= available["graph"] and charged["physics_queries"] == 0
        total += charged["graph"]
        maximum = max(maximum, previous["graph"] + charged["graph"])
    assert total == summary["charged"]
    completed_work = sum(r["augmented"]["charged_work"] for r in reports)
    assert 0 <= total - completed_work <= 3 * sum(summary["pending"])
    return dict(shadow_graph=total, maximum_combined_graph=maximum, physics_queries_added=0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/surface_mission_soak"))
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    reference = json.loads((opts.reference / "summary.json").read_text())
    plan = [r for r in F.plan() if r["mode"] == "on"]
    assert reference["plan"] == plan and len(reference["runs"]) == 5
    opts.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=plan, runs={},
                  scope="Fixed engineering replays, not new strength trials. Existing controls, evaluator, physics, surveys and their work must stay exact. Only the last-priority shadow consumes additional remaining graph work.",
                  source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                  source_dirty=bool(subprocess.check_output(["git", "status", "--porcelain"], text=True).strip()),
                  binary_sha256=F.digest(opts.binary), reference_source=reference["source_commit"],
                  reference_summary_sha256=F.digest(opts.reference / "summary.json"))
    F.D.write(opts.out / "summary.json", result)
    for item in plan:
        name = item["name"]
        args = ["--world", "generated", "--seed", str(item["seed"]), "--mode", "duel", "--match", "true", "--seat", "0",
                "--asteroid-interval", str(item["interval"]), "--p1-policy", f'material_mission_v{item["policies"][0]}',
                "--p2-policy", f'material_mission_v{item["policies"][1]}', "--trace", "true", "--trace-start-tick", "0",
                "--trace-end-tick", "36002", "--survey-capture-flags", "true", "--shadow-capture-flags", "true"]
        run = F.D.run(opts.binary, opts.out, name, args, 1, seconds=600, require_finish=True)
        old_path, path = opts.reference / name, opts.out / name
        old_report, report = [json.loads((p / "report.json").read_text()) for p in [old_path, path]]
        assert F.digest(old_path / "report.json") == reference["runs"][name]["report_sha256"]
        assert F.D.same_physical_outcomes(old_report, report) and old_report["missions"] == report["missions"]
        for key, file in [("controller_trace_sha256", "trace.jsonl"), ("evaluation_sha256", "mission-evaluations.jsonl")]:
            run[key] = F.digest(path / file)
            assert run[key] == reference["runs"][name][key], (name, key)
        for file in ["flag-survey.jsonl", "flag-survey-work.jsonl"]:
            assert F.digest(path / file) == F.digest(old_path / file), (name, file)
        shadows = rows(path / "flag-value-shadow.jsonl")
        run["comparison"] = validate_reports(shadows, rows(path / "mission-evaluations.jsonl"), rows(path / "flag-survey.jsonl"))
        run["shadow"] = report["flag_survey"]["shadow"]
        assert len(shadows) == run["shadow"]["completed"]
        run["work"] = validate_work(rows(path / "flag-survey-work.jsonl"), rows(path / "flag-value-shadow-work.jsonl"), run["shadow"], shadows)
        run["shadow_sha256"] = F.digest(path / "flag-value-shadow.jsonl")
        run["shadow_work_sha256"] = F.digest(path / "flag-value-shadow-work.jsonl")
        run["exact_existing_controls_evaluator_physics_surveys_and_work"] = True
        with (path / "trace.jsonl").open("rb") as source, gzip.open(path / "trace.jsonl.gz", "wb", compresslevel=3) as target:
            shutil.copyfileobj(source, target)
        (path / "trace.jsonl").unlink()
        result["runs"][name] = run
        F.D.write(opts.out / "summary.json", result)
    assert F.digest(opts.binary) == result["binary_sha256"]


if __name__ == "__main__":
    main()
