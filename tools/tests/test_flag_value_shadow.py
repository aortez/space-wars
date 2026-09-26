import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("shadow", Path(__file__).resolve().parents[1] / "compare-flag-value-shadow.py")
T = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(T)


def fixture():
    site = dict(planet=1, bearing=0)
    base = dict(actor="player_1", source_tick=100, completed_tick=101, policy="material_mission_v13",
                transfer_source={}, match_context={}, value_comparison=dict(preferred=None),
                candidates=[dict(planet=1, current=False, local=None, total_seconds=None,
                                 value=dict(ownership_swing=2, priority_units=2, seconds_per_unit=None),
                                 unknown_reason="remote or local surface unmeasured")])
    sample = dict(actor="player_1", site=site, generation=50, source_tick=60, completed_tick=80,
                  validated_tick=80, reason=None, validation=dict(source_objective={}),
                  route=dict(outbound=dict(length=5), returning=dict(length=5)))
    augmented = copy.deepcopy(base)
    augmented.update(completed_tick=104, charged_work=2, value_comparison=dict(preferred=1))
    costs = dict(landing=23.033333, exit=1/60, outbound=1.15833333,
                 claim=6 - 0.5/60, return_board=1 + 2/60, departure=3.8166666)
    total = sum(costs.values()) + 10
    augmented["candidates"][0].update(local=costs, travel_seconds=10,
                                      transfer=dict(settle_seconds=1, turn_seconds=1, climb_seconds=3, cruise_seconds=5),
                                      value=dict(ownership_swing=2, priority_units=2, seconds_per_unit=total/2),
                                      reference_exceeds_match_time=None,
                                      unknown_reason=None, total_seconds=total, site=site, evidence_tick=60,
                                      route_source_tick=60, route_validated_tick=80)
    report = dict(model="capture_flag_value_shadow_v1", observational=True, actor="player_1",
                  admitted_tick=102, completed_tick=104, baseline=base, augmented=augmented,
                  preference_changed=True, completion_reason=None,
                  admissions=[dict(site=site, generation=50, source_tick=60, completed_tick=80,
                                   validated_tick=80, source_age_ticks=42, source_objective={},
                                   used=True, reason=None)])
    return [report], [base], [sample]


class FlagShadowAccounting(unittest.TestCase):
    def test_report_corruption_cannot_create_new_evidence(self):
        reports, baselines, samples = fixture()
        result = T.validate_reports(reports, baselines, samples)
        self.assertEqual(result["unique_surveys_used"], 1)
        self.assertEqual(result["complete_value_comparisons"], 1)
        for mutation in ["baseline", "future", "renewed", "wrong_site", "cost", "preference", "landing", "total", "units"]:
            bad = copy.deepcopy(reports)
            r = bad[0]
            if mutation == "baseline": r["baseline"]["source_tick"] += 1
            if mutation == "future": r["admissions"][0]["completed_tick"] = 101
            if mutation == "renewed": r["admissions"][0]["source_age_ticks"] = 0
            if mutation == "wrong_site": r["augmented"]["candidates"][0]["site"]["bearing"] += 1
            if mutation == "cost": r["augmented"]["candidates"][0]["local"]["outbound"] = 0
            if mutation == "preference": r["preference_changed"] = False
            if mutation == "landing": r["augmented"]["candidates"][0]["local"]["landing"] = -100
            if mutation == "total": r["augmented"]["candidates"][0]["total_seconds"] = 0
            if mutation == "units": r["augmented"]["candidates"][0]["value"]["priority_units"] = 20
            with self.subTest(mutation=mutation), self.assertRaises((AssertionError, KeyError)):
                T.validate_reports(bad, baselines, samples)

    def test_unknown_shortlist_member_and_source_uniqueness_are_preserved(self):
        reports, baselines, samples = fixture()
        unknown = dict(planet=2, total_seconds=None, unknown_reason="remote or local surface unmeasured")
        baselines[0]["candidates"].append(unknown)
        reports[0]["augmented"]["charged_work"] = 2
        with self.assertRaises(AssertionError):
            T.validate_reports(reports, baselines, samples)
        reports, baselines, samples = fixture()
        repeated = copy.deepcopy(reports[0])
        repeated["admitted_tick"] += 60
        repeated["completed_tick"] += 60
        repeated["augmented"]["completed_tick"] += 60
        repeated["admissions"][0]["source_age_ticks"] += 60
        with self.assertRaises(AssertionError):
            T.validate_reports(reports + [repeated], baselines, samples)

    def test_shadow_work_cannot_exceed_or_reset_shared_remaining_quota(self):
        flag = dict(tick=1, remaining_after_evaluation=dict(graph=2, physics_queries=384),
                    allocation=dict(charged=dict(graph=1, physics_queries=0),
                                    jobs=[dict(charged=dict(graph=1, physics_queries=0))]))
        shadow = dict(tick=1, remaining_after_flag_survey=dict(graph=1, physics_queries=0),
                      charged=dict(graph=1, physics_queries=0))
        reports = [dict(augmented=dict(charged_work=1))]
        result = T.validate_work([flag], [shadow], dict(charged=1, pending=[False, False]), reports)
        self.assertEqual(result["maximum_combined_graph"], 4)
        for mutation in ["allowance", "charge", "queries", "total", "missing_report"]:
            bad = copy.deepcopy(shadow)
            summary = dict(charged=1, pending=[False, False])
            if mutation == "allowance": bad["remaining_after_flag_survey"]["graph"] = 4
            if mutation == "charge": bad["charged"]["graph"] = 2
            if mutation == "queries": bad["charged"]["physics_queries"] = 1
            if mutation == "total": summary["charged"] = 2
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                T.validate_work([flag], [bad], summary, [] if mutation == "missing_report" else reports)


if __name__ == "__main__":
    unittest.main()
