import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("flag_surveys", Path(__file__).resolve().parents[1] / "compare-flag-surveys.py")
TOOL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOOL)


class FlagSurveyAccounting(unittest.TestCase):
    def test_a_missing_sample_cannot_silently_reduce_coverage(self):
        samples = [dict(reason=None), dict(reason="outside patch")]
        telemetry = dict(completed=2, walking_round_trips=1, unknown={"outside patch": 1})
        TOOL.validate_sample_counts(samples, telemetry)
        with self.assertRaises(AssertionError):
            TOOL.validate_sample_counts(samples[:1], telemetry)

    def test_plan_preserves_frozen_regression_and_pairs_all_fresh_conditions(self):
        rows = TOOL.plan()
        self.assertEqual(len(rows), 11)
        self.assertEqual(len({r["name"] for r in rows}), 11)
        for group in {r["group"] for r in rows}:
            variants = [r for r in rows if r["group"] == group]
            self.assertEqual({r["mode"] for r in variants},
                             {"frozen", "off", "on"} if group == "regression" else {"off", "on"})
            self.assertEqual(len({r["seed"] for r in variants}), 1)

    def test_combined_charge_includes_the_work_spent_before_the_observer(self):
        row = dict(remaining_after_evaluation=dict(graph=2, physics_queries=200),
                   allocation=dict(charged=dict(graph=1, physics_queries=100),
                                   jobs=[dict(charged=dict(graph=1, physics_queries=100))]))
        self.assertEqual(TOOL.charge(row), dict(graph=3, physics_queries=284))
        row["allocation"]["charged"]["physics_queries"] = 201
        with self.assertRaises(AssertionError):
            TOOL.charge(row)
        row["allocation"]["charged"]["physics_queries"] = 99
        with self.assertRaises(AssertionError):
            TOOL.charge(row)


if __name__ == "__main__":
    unittest.main()
