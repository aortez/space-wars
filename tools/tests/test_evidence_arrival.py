import importlib.util
from pathlib import Path
import unittest


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).parents[1]/filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


A = module('arrival', 'analyze-evidence-arrival.py')
V = module('neutral', 'validate-current-neutral-costs.py')


def fixture():
    visit = dict(planet=1, selected_tick=1, arrived_tick=10, departed_tick=30, abandoned_tick=None)
    report = dict(metrics=[dict(visits=[visit]), dict(visits=[])])
    current = dict(planet=1, current=True, observed_owner=None, ownership_known=True,
        evidence_tick=2, total_seconds=20, unknown_reason=None)
    row = dict(actor='player_1', source_tick=3, completed_tick=4, current_target=1, selected_tick=1,
        candidates=[current, dict(current, planet=2, current=False)], inactive_reason=None,
        candidates_truncated=False, model='example')
    return report, row


class EvidenceArrivalTests(unittest.TestCase):
    def test_completion_time_is_the_availability_boundary(self):
        report, row = fixture()
        row.update(source_tick=9, completed_tick=10)
        result = A.analyze(report, [row], 0)
        self.assertEqual(result['counts']['first_pair_at_or_after_arrival'], 1)
        self.assertNotIn('first_pair_before_arrival', result['counts'])

    def test_no_arrival_unknown_and_abandoned_visits_are_retained(self):
        report, row = fixture()
        visit = report['metrics'][0]['visits'][0]
        visit.update(arrived_tick=None, departed_tick=None, abandoned_tick=8)
        report['metrics'][0]['visits'].append(dict(visit, selected_tick=9, abandoned_tick=None))
        result = A.analyze(report, [row], 0)
        self.assertEqual(result['counts']['visits'], 2)
        self.assertEqual(result['counts']['first_pair_no_arrival_observed'], 1)
        self.assertEqual(result['counts']['first_pair_unavailable'], 1)

    def test_reselection_on_same_planet_cannot_inherit_an_old_prediction(self):
        report, row = fixture()
        report['metrics'][0]['visits'][0].update(departed_tick=None, abandoned_tick=5)
        report['metrics'][0]['visits'].append(dict(planet=1, selected_tick=5, arrived_tick=10,
            departed_tick=30, abandoned_tick=None))
        row.update(source_tick=4, completed_tick=5)
        result = A.analyze(report, [row], 0)
        self.assertEqual(result['counts']['first_pair_unavailable'], 2)
        self.assertEqual(result['counts']['reports_outside_active_visit'], 1)

    def test_first_prediction_stays_frozen_and_other_seat_is_ignored(self):
        report, row = fixture()
        later = dict(row, source_tick=6, completed_tick=7)
        other = dict(row, actor='player_2', source_tick=0, completed_tick=0)
        result = A.analyze(report, [other, row, later], 0)
        self.assertEqual(result['visits'][0]['first_pair']['source_tick'], 3)
        self.assertEqual(result['counts']['active_reports'], 2)

    def test_partial_shortlist_and_two_alternatives_are_not_a_complete_comparison(self):
        report, row = fixture()
        third = dict(row['candidates'][1], planet=3, total_seconds=None)
        row['candidates'].append(third)
        result = A.analyze(report, [row], 0)
        self.assertEqual(result['counts']['first_pair_before_arrival'], 1)
        self.assertEqual(result['counts']['first_complete_unavailable'], 1)
        row['candidates'][0]['total_seconds'] = None
        third['total_seconds'] = 10
        result = A.analyze(report, [row], 0)
        self.assertEqual(result['counts']['first_pair_unavailable'], 1)

    def test_malformed_join_or_clock_is_rejected(self):
        report, row = fixture()
        for update in [dict(selected_tick=2), dict(source_tick=6)]:
            with self.assertRaises(AssertionError):
                A.analyze(report, [dict(row, **update)], 0)
        with self.assertRaises(AssertionError):
            A.analyze(report, [dict(row, completed_tick=6), row], 0)
        report['metrics'][0]['visits'] *= 2
        with self.assertRaises(AssertionError):
            A.analyze(report, [row], 0)

    def test_plan_changes_only_neutral_demand_and_uses_new_seed_namespace(self):
        plan = V.plan()
        self.assertEqual(len(plan), 64)
        old_seeds = {r['seed'] for r in V.F.plan() if r['kind'] == 'held_out'}
        for item in plan:
            a, policies = V.arguments(dict(item, candidate=False))
            b, _ = V.arguments(dict(item, candidate=True))
            self.assertEqual(a[:-1], b[:-1])
            self.assertEqual(a[-1], 'none')
            self.assertEqual(b[-1], str(item['seat']))
            self.assertEqual(a[a.index('--admit-flag-costs')+1], str(item['seat']))
            self.assertEqual(policies[item['seat']], 'material_mission_v13')
            if item['kind'] == 'held_out':
                self.assertNotIn(item['seed'], old_seeds)
                self.assertEqual(a[a.index('--seat')+1], '0')


if __name__ == '__main__':
    unittest.main()
