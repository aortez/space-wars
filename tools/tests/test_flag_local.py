import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("flag_local", Path(__file__).resolve().parents[1] / "validate-flag-local.py")
T = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(T)


def fixture():
    sample = dict(source_tick=1, completed_tick=100, validated_tick=100, reason=None, physics_queries=30,
                  measurement=dict(queries=20, tick=1), geometry=dict(acceptance_prefix=dict(valid=False)),
                  validation=dict(model="captured_query_unions_v1", source_areas=[{}] * 5,
                                  captured_queries=10, walking_queries=10, complete=True,
                                  predicates_valid=True, predicate_failure=None,
                                  geometry=dict(valid=True, area_tests=4)))
    telemetry = dict(local_checks=1, local_area_tests=4, local_rescued=1, local_withheld=0)
    return [sample], telemetry


class FlagLocalAccounting(unittest.TestCase):
    def test_rescued_rejection_is_reconciled_without_renewing_source(self):
        samples, telemetry = fixture()
        self.assertEqual(len(T.validate_local(samples, telemetry)["rescued"]), 1)
        for mutation in ["missing", "count", "capture", "expired", "incomplete"]:
            bad = copy.deepcopy(samples)
            if mutation == "missing": bad = []
            if mutation == "count": bad[0]["validation"]["geometry"]["area_tests"] += 1
            if mutation == "capture": bad[0]["validation"]["walking_queries"] += 1
            if mutation == "expired": bad[0]["source_tick"] = -2000
            if mutation == "incomplete": bad[0]["validation"]["complete"] = False
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                T.validate_local(bad, telemetry)

    def test_local_rejection_can_have_a_valid_circle(self):
        samples, telemetry = fixture()
        samples[0].update(reason="source scalar gravity changed", validated_tick=None, geometry=None)
        samples[0]["validation"].update(predicates_valid=False, predicate_failure="source scalar gravity changed")
        telemetry.update(local_rescued=0, local_withheld=1)
        self.assertEqual(len(T.validate_local(samples, telemetry)["withheld"]), 1)
        telemetry["local_withheld"] = 0
        with self.assertRaises(AssertionError): T.validate_local(samples, telemetry)


if __name__ == "__main__":
    unittest.main()
