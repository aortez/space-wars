import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("flag_geometry", Path(__file__).resolve().parents[1] / "investigate-flag-geometry.py")
TOOL = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TOOL)


def fixture():
    report = dict(complete=True, unavailable=None, changed_colliders=1, region_changes=1,
                  area_tests=2, unsupported_tests=0, area_changes=[1], omitted_changes=0,
                  changes=[dict(previous=None, current=dict(collider=dict(entity=10000)), areas=[0], unsupported_tests=0)])
    samples = [dict(reason="geometry changed since source measurement", geometry=dict(
        model="source_envelope_overlaps_v1", material_queries_dirty=False,
        acceptance_prefix=dict(valid=False), envelopes=[dict(name="climb_60")],
        report=report, entity_kinds={"10000": "ship or pod"}))]
    telemetry = dict(geometry_diagnostics=1, diagnostic_area_tests=2, diagnostic_region_changes=1,
                     diagnostic_omitted_changes=0, diagnostic_incomplete=0)
    return samples, telemetry


class FlagGeometryAccounting(unittest.TestCase):
    def test_missing_rejection_or_mismatched_work_cannot_disappear(self):
        samples, telemetry = fixture()
        summary = TOOL.validate_diagnostics(samples, telemetry)
        self.assertEqual(summary["potential_stage_overlaps"], {"climb_60": 1})
        self.assertEqual(summary["retained_collider_kinds"], {"ship or pod": 1})
        with self.assertRaises(AssertionError):
            TOOL.validate_diagnostics([], telemetry)
        bad = copy.deepcopy(telemetry)
        bad["diagnostic_area_tests"] += 1
        with self.assertRaises(AssertionError):
            TOOL.validate_diagnostics(samples, bad)

    def test_truncated_records_require_complete_totals_and_are_not_complete_kind_counts(self):
        samples, telemetry = fixture()
        report = samples[0]["geometry"]["report"]
        report.update(changed_colliders=10, region_changes=10, omitted_changes=9, area_changes=[10])
        telemetry.update(diagnostic_region_changes=10, diagnostic_omitted_changes=9)
        summary = TOOL.validate_diagnostics(samples, telemetry)
        self.assertEqual(summary["potential_stage_overlaps"], {"climb_60": 10})
        self.assertEqual(summary["retained_collider_kinds"], {"ship or pod": 1})
        report["area_changes"] = [11]
        with self.assertRaises(AssertionError):
            TOOL.validate_diagnostics(samples, telemetry)

    def test_empty_incomplete_report_cannot_look_like_no_geometry_changed(self):
        samples, telemetry = fixture()
        report = samples[0]["geometry"]["report"]
        report.update(complete=False, unavailable="collider capacity", changes=[], region_changes=0,
                      changed_colliders=0, area_changes=[], area_tests=0)
        telemetry.update(diagnostic_incomplete=1, diagnostic_region_changes=0, diagnostic_area_tests=0)
        TOOL.validate_diagnostics(samples, telemetry)
        report.update(complete=True, unavailable=None)
        with self.assertRaises(AssertionError):
            TOOL.validate_diagnostics(samples, telemetry)


if __name__ == "__main__":
    unittest.main()
