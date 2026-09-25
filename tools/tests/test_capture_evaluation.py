import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("capture_evaluation",
    Path(__file__).resolve().parents[1] / "compare-capture-evaluation.py")
TOOL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOOL)


def row(selected, source, prediction, planet=0):
    return {
        "actor": "player_1", "source_tick": source, "selected_tick": selected,
        "preferred_by_time": None, "candidates": [{
            "planet": planet, "current": True, "site": {"planet": planet, "bearing": 1},
            "revision": 3, "total_seconds": prediction,
            "unknown_reason": "unmeasured" if prediction is None else None,
        }],
    }


class PredictionAccounting(unittest.TestCase):
    def test_first_numeric_prediction_is_frozen_and_failures_remain_in_denominator(self):
        report = {"metrics": [{"visits": [
            {"planet": 0, "selected_tick": 1, "departed_tick": 901, "abandoned_tick": None},
            {"planet": 0, "selected_tick": 1000, "departed_tick": None, "abandoned_tick": 1400},
            {"planet": 1, "selected_tick": 1500, "departed_tick": None, "abandoned_tick": None},
        ]}, {"visits": []}]}
        initial = [row(1, 61, None), row(1, 121, 10), row(1000, 1000, 2), row(1500, 1500, None, 1)]
        changes = {(0, 0): [(0, 3), (200, 4)]}
        first = TOOL.prediction_results(report, initial, changes)
        later = TOOL.prediction_results(report, initial + [row(1, 800, 1)], changes)
        self.assertEqual(first["attempts"], later["attempts"])
        self.assertEqual(first["visits"], 3)
        self.assertEqual(first["visits_with_numeric_prediction"], 2)
        self.assertEqual(first["visit_outcomes"], {"completed": 1, "abandoned": 1})
        self.assertEqual(first["completed_median_absolute_error_seconds"], 3)
        self.assertTrue(first["attempts"][0]["material_changed_after_prediction"])
        self.assertIsNone(first["attempts"][1]["error_seconds"])

    def test_prediction_cannot_claim_an_unmatched_visit_or_past_departure(self):
        report = {"metrics": [{"visits": [
            {"planet": 0, "selected_tick": 1, "departed_tick": 20, "abandoned_tick": None},
        ]}, {"visits": []}]}
        with self.assertRaises(ValueError):
            TOOL.prediction_results(report, [row(2, 10, 3)])
        with self.assertRaises(ValueError):
            TOOL.prediction_results(report, [row(1, 21, 3)])


if __name__ == "__main__":
    unittest.main()
