"""Predeclared physical gates for clear-control-v1. No reward-based gate."""
from collections import Counter


def physical(rows, recipe="clear-control-v1"):
    strata = []
    for i in range(4):
        group = [r for r in rows if r["recipe"] == f"{recipe}/stratum-{i}"]
        if len(group) != 4:
            raise ValueError("gate requires exactly four tasks in every stratum")
        strata.append(dict(Counter(r["outcome"] for r in group)))
    if len(rows) != 16:
        raise ValueError("gate requires exactly 16 tasks")
    counts = dict(Counter(r["outcome"] for r in rows))
    return {"outcomes": counts, "strata": strata,
            "successes": counts.get("success", 0),
            "physical_failures": sum(v for k,v in counts.items() if k not in ("success", "deadline"))}


def seed_gates(initial, final, recipe="clear-control-v1"):
    before, after = physical(initial, recipe), physical(final, recipe)
    improvement = after["successes"] - before["successes"]
    return {"initial": before, "final": after, "success_gain": improvement,
            "learning_evidence": after["successes"] >= 4 and improvement >= 4,
            "basic_flight_gate": after["successes"] >= 12 and improvement >= 8
                and after["physical_failures"] == 0
                and all(s.get("success", 0) >= 2 for s in after["strata"])}


def aggregate(seeds):
    if len(seeds) != 3:
        raise ValueError("three declared learner seeds required")
    return {"learning_evidence": sum(s["learning_evidence"] for s in seeds) >= 2,
            "basic_flight_gate": sum(s["basic_flight_gate"] for s in seeds) >= 2}
