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
                                 unknown_reason="remote or local surface unmeasured")])
    sample = dict(actor="player_1", site=site, generation=50, source_tick=60, completed_tick=80,
                  validated_tick=80, reason=None, validation=dict(source_objective={}),
                  route=dict(outbound=dict(length=5), returning=dict(length=5)))
    augmented = copy.deepcopy(base)
    augmented.update(completed_tick=104, charged_work=2, value_comparison=dict(preferred=1))
    augmented["candidates"][0].update(local=dict(outbound=1.15833333, return_board=1 + 2/60),
                                      unknown_reason=None, total_seconds=40, site=site, evidence_tick=60,
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
        for mutation in ["baseline", "future", "renewed", "wrong_site", "cost", "preference"]:
            bad = copy.deepcopy(reports)
            r = bad[0]
            if mutation == "baseline": r["baseline"]["source_tick"] += 1
            if mutation == "future": r["admissions"][0]["completed_tick"] = 101
            if mutation == "renewed": r["admissions"][0]["source_age_ticks"] = 0
            if mutation == "wrong_site": r["augmented"]["candidates"][0]["site"]["bearing"] += 1
            if mutation == "cost": r["augmented"]["candidates"][0]["local"]["outbound"] = 0
            if mutation == "preference": r["preference_changed"] = False
            with self.subTest(mutation=mutation), self.assertRaises((AssertionError, KeyError)):
                T.validate_reports(bad, baselines, samples)

    def test_shadow_work_cannot_exceed_or_reset_shared_remaining_quota(self):
        flag = dict(tick=1, remaining_after_evaluation=dict(graph=2, physics_queries=384),
                    allocation=dict(charged=dict(graph=1, physics_queries=0),
                                    jobs=[dict(charged=dict(graph=1, physics_queries=0))]))
        shadow = dict(tick=1, remaining_after_flag_survey=dict(graph=1, physics_queries=0),
                      charged=dict(graph=1, physics_queries=0))
        result = T.validate_work([flag], [shadow], dict(charged=1))
        self.assertEqual(result["maximum_combined_graph"], 4)
        for mutation in ["allowance", "charge", "queries", "total"]:
            bad = copy.deepcopy(shadow)
            summary = dict(charged=1)
            if mutation == "allowance": bad["remaining_after_flag_survey"]["graph"] = 4
            if mutation == "charge": bad["charged"]["graph"] = 2
            if mutation == "queries": bad["charged"]["physics_queries"] = 1
            if mutation == "total": summary["charged"] = 2
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                T.validate_work([flag], [bad], summary)


if __name__ == "__main__":
    unittest.main()
