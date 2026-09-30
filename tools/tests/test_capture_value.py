import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "capture_value", Path(__file__).resolve().parents[1] / "compare-capture-value.py")
TOOL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOOL)


class ValueTrialAccounting(unittest.TestCase):
    def test_plan_keeps_regression_separate_and_pairs_both_seats_on_each_fresh_world(self):
        plan = TOOL.plan()
        self.assertEqual(len(plan), 43)
        self.assertEqual(len({r["name"] for r in plan}), 43)
        regression = [r for r in plan if r["group"] == "regression"]
        self.assertEqual({r["version"] for r in regression}, {10, 12, 13})
        fresh = [r for r in plan if r["group"] != "regression"]
        self.assertEqual(len({r["seed"] for r in fresh}), 4)
        self.assertNotIn(regression[0]["seed"], {r["seed"] for r in fresh})
        for group in {r["group"] for r in fresh}:
            rows = [r for r in fresh if r["group"] == group]
            self.assertEqual({(r["version"], r["seat"]) for r in rows},
                             {(10, None), (12, 0), (12, 1), (13, 0), (13, 1)})

    def test_time_preference_does_not_count_as_a_value_preference(self):
        row = dict(actor="player_1", inactive_reason=None, current_target=0,
                   preferred_by_time=1, value_comparison={"preferred": 0},
                   candidates=[dict(total_seconds=20, transfer={"climb_seconds": 3})])
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evaluations.jsonl"
            path.write_text(json.dumps(row) + "\n")
            result = TOOL.coverage(path)[0]["counts"]
        self.assertEqual(result["alternative_preferences"], 1)
        self.assertEqual(result["complete_value_preferences"], 1)
        self.assertNotIn("alternative_value_preferences", result)
        self.assertEqual(result["reports_with_climb_cost"], 1)


if __name__ == "__main__":
    unittest.main()
