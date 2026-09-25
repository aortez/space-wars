#!/usr/bin/env python3
"""Paired observational-evaluator trials and conditional prediction accounting."""
import argparse
import concurrent.futures
from collections import Counter
import csv
import gzip
import hashlib
import json
from itertools import zip_longest
from pathlib import Path
import shutil
import statistics
import subprocess


def write(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def prediction_results(report, evaluations, material_changes=None):
    """Freeze the first numeric current-mission forecast, including later failures."""
    visits = {
        (seat, v["planet"], v["selected_tick"]): v
        for seat, metrics in enumerate(report["metrics"]) for v in metrics["visits"]
    }
    first = {}
    reasons = Counter()
    coverage = Counter()
    comparisons = []
    for evaluation in evaluations:
        coverage["reports"] += 1
        numeric = [c for c in evaluation["candidates"] if c["total_seconds"] is not None]
        if len(numeric) >= 2:
            coverage["reports_with_multiple_numeric_destinations"] += 1
            if len(comparisons) < 8:
                comparisons.append({"actor": evaluation["actor"], "source_tick": evaluation["source_tick"],
                    "preferred_by_time": evaluation["preferred_by_time"], "candidates": numeric})
        coverage["numeric_alternative_records"] += sum(not c["current"] for c in numeric)
        if evaluation["preferred_by_time"] is not None:
            coverage["complete_shortlist_comparisons"] += 1
        for candidate in evaluation["candidates"]:
            coverage["candidate_records"] += 1
            if candidate["total_seconds"] is None:
                reasons[candidate["unknown_reason"] or "unknown"] += 1
                continue
            coverage["numeric_candidate_records"] += 1
            if not candidate["current"] or evaluation["selected_tick"] is None:
                continue
            actor = {"player_1": 0, "player_2": 1}[evaluation["actor"]]
            key = (actor, candidate["planet"], evaluation["selected_tick"])
            first.setdefault(key, (evaluation, candidate))
    attempts = []
    for key, (evaluation, candidate) in first.items():
        if key not in visits:
            raise ValueError(f"forecast has no matching actual visit: {key}")
        visit = visits[key]
        source = evaluation["source_tick"]
        departure = visit["departed_tick"]
        if departure is not None and departure < source:
            raise ValueError("forecast is after the departure it predicts")
        outcome = ("completed" if departure is not None else
                   "abandoned" if visit["abandoned_tick"] is not None else "unfinished")
        actual = (departure - source) / 60 if departure is not None else None
        end = departure if departure is not None else (
            visit["abandoned_tick"] if visit["abandoned_tick"] is not None else report.get("elapsed_ticks", source))
        changed = None if material_changes is None else any(
            source < tick <= end and revision != candidate["revision"]
            for tick, revision in material_changes.get((key[0], key[1]), []))
        attempts.append({
            "seat": key[0], "planet": key[1], "selected_tick": key[2], "source_tick": source,
            "site": candidate["site"], "revision": candidate["revision"],
            "predicted_remaining_seconds": candidate["total_seconds"], "outcome": outcome,
            "actual_remaining_seconds": actual,
            "error_seconds": candidate["total_seconds"] - actual if actual is not None else None,
            "material_changed_after_prediction": changed,
        })
    errors = [abs(a["error_seconds"]) for a in attempts if a["error_seconds"] is not None]
    return {
        "coverage": dict(coverage), "unknown_reasons": dict(reasons),
        "comparison_examples": comparisons,
        "visits": len(visits), "visits_with_numeric_prediction": len(first),
        "visit_outcomes": dict(Counter(a["outcome"] for a in attempts)),
        "predictions_with_material_changes": sum(a["material_changed_after_prediction"] is True for a in attempts),
        "completed_median_absolute_error_seconds": statistics.median(errors) if errors else None,
        "completed_max_absolute_error_seconds": max(errors) if errors else None,
        "attempts": attempts,
        "scope": "first numeric current-mission forecast only; success-conditioned references; failures stay in coverage/outcomes, not completed-duration errors; alternatives have no observed outcome",
    }


def material_history(path):
    changes = {}
    with gzip.open(path, "rt") as stream:
        for line in stream:
            row = json.loads(line)
            for planet in row["observation"]["planets"]:
                key = (row["seat"], planet["index"])
                history = changes.setdefault(key, [])
                if not history or history[-1][1] != planet["revision"]:
                    history.append((row["tick"], planet["revision"]))
    return changes


def normalize_mission_ids(mission):
    capture = mission.get("capture")
    acquisition = capture.get("acquisition") if capture else None
    if acquisition is not None and acquisition.get("generation") is not None:
        acquisition["generation"] = 0
    return mission


def normalize_survey_trace(row):
    # Remote jobs reserve IDs in the shared namespace without moving its
    # scheduling cursor. Keep None versus present, all ticks, and every other
    # observation/control/state field; only the opaque local job ID may differ.
    evidence = row["observation"]["local"].get("objective_evidence")
    if evidence is not None and evidence.get("generation") is not None:
        evidence["generation"] = 0
    normalize_mission_ids(row.get("mission", {}))
    return row


def compare_survey_traces(paths):
    rows = changed_ids = 0
    digest = hashlib.sha256()
    changes = {}
    with gzip.open(paths[0] / "trace.jsonl.gz", "rt") as a, gzip.open(paths[1] / "trace.jsonl.gz", "rt") as b:
        for left, right in zip_longest(a, b):
            if left is None or right is None:
                raise ValueError("trace lengths differ")
            left, right = json.loads(left), json.loads(right)
            changed_ids += left != right
            if normalize_survey_trace(left) != normalize_survey_trace(right):
                raise ValueError(f"non-ID trace difference at tick {left['tick']} seat {left['seat']}")
            rows += 1
            digest.update(json.dumps(left, sort_keys=True, separators=(",", ":")).encode())
            digest.update(b"\n")
            for planet in right["observation"]["planets"]:
                key = (right["seat"], planet["index"])
                history = changes.setdefault(key, [])
                if not history or history[-1][1] != planet["revision"]:
                    history.append((right["tick"], planet["revision"]))
    return {"rows": rows, "rows_with_different_job_ids": changed_ids,
            "normalized_sha256": digest.hexdigest(), "all_other_fields_identical": True,
            "normalized_fields": ["observation.local.objective_evidence.generation",
                                  "mission.capture.acquisition.generation"],
            "normalization": "present opaque IDs only; absent IDs, ticks, payloads and all other fields retained"}, changes


def run_matrix(binary, out, workers, survey_alternatives=False):
    out.mkdir(parents=True, exist_ok=False)
    cases = []
    for world in range(2):
        seed_namespace = "native-capture-alternative-v1" if survey_alternatives else "native-capture-evaluation-v1"
        seed = int.from_bytes(hashlib.sha256(f"{seed_namespace}:{world}".encode()).digest()[:8], "big")
        policies = ["material_mission_v10", "material_mission_v11"][::1 if world == 0 else -1]
        for asteroid in (0, 3):
            for enabled in (False, True):
                name = f"world{world}-asteroids{asteroid}-{'enabled' if enabled else 'disabled'}"
                path = out / name
                command = [
                    str(binary.resolve()), "--world", "generated", "--seed", str(seed),
                    "--mode", "duel", "--match", "true", "--require-finish", "true", "--seconds", "600",
                    "--p1-policy", policies[0], "--p2-policy", policies[1],
                    "--asteroid-interval", str(asteroid), "--trace", "true",
                    "--trace-end-tick", "36000", "--live-objective-planning", "true",
                    "--reuse-objective-ground", "true", "--objective-dependencies", "routes",
                    "--early-objective-routes", "true", "--evaluate-missions", str(enabled or survey_alternatives).lower(),
                    "--survey-capture-alternative", str(enabled and survey_alternatives).lower(),
                    "--out", str(path),
                ]
                cases.append({"name": name, "world": world, "seed": seed, "asteroid": asteroid,
                              "enabled": enabled, "command": command})
    write(out / "experiment.json", {
        "binary_sha256": digest(binary), "revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True).strip(), "cases": cases,
        "survey_alternatives": survey_alternatives,
        "criteria": "identical complete controller traces and non-timing outcomes in all four pairs; unchanged local allocations; remote charges share the existing quota; report comparison coverage and attempted outcomes without fitting" if survey_alternatives else "identical complete traces and non-timing outcomes in all four pairs; zero physical-query work; report forecast coverage and all attempted outcomes without fitting",
    })
    (out / "tested.patch").write_bytes(subprocess.check_output(["git", "diff", "--binary", "HEAD"]))

    def run(case):
        path = out / case["name"]
        with (out / (case["name"] + ".log")).open("wb") as log:
            subprocess.run(case["command"], stdout=log, stderr=subprocess.STDOUT, check=True)
        trace = path / "trace.jsonl"
        trace_digest = digest(trace)
        with trace.open("rb") as src, gzip.open(path / "trace.jsonl.gz", "wb", compresslevel=1) as dest:
            shutil.copyfileobj(src, dest)
        trace.unlink()
        write(path / "trace-sha256.json", {"sha256": trace_digest})
        print(case["name"], "complete", flush=True)
    with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
        list(pool.map(run, cases))


def allocation_audit(paths):
    local = []
    totals = []
    for path in paths:
        charges = Counter()
        rows = []
        by_tick = {}
        with (path / "live-planning.csv").open() as stream:
            for row in csv.DictReader(stream):
                tick = int(row["tick"])
                total = by_tick.setdefault(tick, {"graph": 0, "queries": 0, "row": row})
                for kind in ("graph", "queries"):
                    total[kind] += int(row[kind])
                    charges[f'{row["task"]}_{kind}'] += int(row[kind])
                if row["task"] == "landing_objective":
                    rows.append(tuple(row[k] for k in ("tick", "actor", "age", "graph", "queries", "phase")))
        for total in by_tick.values():
            row = total["row"]
            if not (total["graph"] == int(row["total_graph"]) <= int(row["graph_budget"])
                    and total["queries"] == int(row["total_queries"]) <= int(row["query_budget"])):
                raise ValueError(f"planning quota/accounting mismatch in {path} at {row['tick']}")
        local.append(rows)
        totals.append(dict(charges))
    if local[0] != local[1]:
        raise ValueError("alternative work changed local allocations")
    return {"local_allocations_identical": True, "local_rows": len(local[0]), "charges": totals}


def analyze(out):
    experiment = json.loads((out / "experiment.json").read_text())
    pairs = []
    for world in range(2):
        for asteroid in (0, 3):
            prefix = f"world{world}-asteroids{asteroid}"
            paths = [out / f"{prefix}-{option}" for option in ("disabled", "enabled")]
            reports = [json.loads((p / "report.json").read_text()) for p in paths]
            hashes = [json.loads((p / "trace-sha256.json").read_text()) for p in paths]
            surveyed = experiment.get("survey_alternatives")
            if surveyed:
                trace_comparison, changes = compare_survey_traces(paths)
                for report in reports:
                    for mission in report["missions"]:
                        normalize_mission_ids(mission)
            elif hashes[0] != hashes[1]:
                raise ValueError(f"physical traces changed in {prefix}")
            else:
                changes = material_history(paths[1] / "trace.jsonl.gz")
            for field in ("round", "metrics", "missions", "final_pilots", "final_planets",
                          "final_audit", "final_combat", "elapsed_ticks", "landing_queries"):
                if reports[0][field] != reports[1][field]:
                    raise ValueError(f"{field} changed in {prefix}")
            if not all(r["physics_ok"] for r in reports):
                raise ValueError(f"physical audit failed in {prefix}")
            with (paths[1] / "mission-evaluations.jsonl").open() as stream:
                predictions = prediction_results(reports[1], (json.loads(line) for line in stream),
                    changes)
            pair = {"case": prefix, "round": reports[1]["round"],
                    "evaluation": reports[1]["mission_evaluation"], "predictions": predictions}
            if surveyed:
                pair["trace_comparison"] = trace_comparison
                pair["raw_trace_hashes"] = hashes
                pair["allocation_audit"] = allocation_audit(paths)
                pair["survey"] = reports[1]["live_objective_planning"]["destination_cover"]
                with (paths[0] / "mission-evaluations.jsonl").open() as stream:
                    pair["baseline_predictions"] = prediction_results(reports[0], (json.loads(line) for line in stream))
            else:
                pair["identical_trace"] = hashes[0]
            write(paths[1] / "prediction-results.json", predictions)
            pairs.append(pair)
    result = {"binary_sha256": experiment["binary_sha256"], "pairs": pairs}
    write(out / "comparison.json", result)
    print(json.dumps([{k: p[k] for k in ("case", "evaluation")} |
                      {"predictions": {k:v for k,v in p["predictions"].items() if k != "attempts"}}
                      for p in pairs], indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("run", "analyze"))
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/surface_mission_soak"))
    parser.add_argument("--workers", type=int, choices=range(1, 5), default=2)
    parser.add_argument("--survey-alternatives", action="store_true")
    args = parser.parse_args()
    if args.mode == "run":
        run_matrix(args.binary, args.out, args.workers, args.survey_alternatives)
    analyze(args.out)


if __name__ == "__main__":
    main()
