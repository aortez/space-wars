import copy
import importlib.util
from pathlib import Path
import unittest
from test_capture_destinations import fixture as report_fixture

SPEC = importlib.util.spec_from_file_location('value_validation', Path(__file__).resolve().parents[1]/'validate-capture-value.py')
T = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(T)


def fixture():
    report = report_fixture()
    switch = dict(tick=22, source_tick=20, **{'from':0, 'to':1}, current_seconds=10.0, destination_seconds=4.0)
    report['missions'][0]['destination_planning'] = dict(switches=1, last_switch=switch)
    report['metrics'][0]['visits'] = [dict(planet=1, selected_tick=22, claimed_tick=60,
        arrived_tick=30, landed_tick=40, boarded_tick=80, departed_tick=100, abandoned_tick=None)]
    evaluation = dict(actor='player_1', source_tick=20, completed_tick=21, selected_tick=1,
        current_target=0, preferred_by_time=1, value_comparison=None,
        candidates=[dict(planet=0, total_seconds=10.0), dict(planet=1, total_seconds=4.0,
            travel_seconds=1.0, local=dict(landing=0.5, exit=0.1, outbound=0.3, claim=1.0,
                return_board=0.1, departure=1.0))])
    return report, evaluation


class ValueValidationTests(unittest.TestCase):
    def test_forecast_stays_at_its_source_and_is_joined_to_exact_switched_visit(self):
        report, evaluation = fixture()
        result = T.switch_predictions(report, [evaluation, copy.deepcopy(evaluation)])
        self.assertEqual(len(result['switches']), 1)
        switch = result['switches'][0]
        self.assertEqual(switch['actual_seconds_from_source'], 80/60)
        self.assertEqual(switch['error_seconds'], 4-80/60)
        self.assertEqual(switch['outcome'], 'completed')
        milestones = switch['milestone_references']['records']
        self.assertEqual(milestones[0]['predicted_seconds'], 1.0)
        self.assertEqual(milestones[0]['actual_seconds'], 10/60)
        self.assertAlmostEqual(milestones[-1]['error_seconds'], switch['error_seconds'])

    def test_wrong_source_identity_cost_or_visit_cannot_get_completion_credit(self):
        for mutation in range(7):
            report, evaluation = fixture()
            if mutation == 0: evaluation['source_tick'] = 19
            elif mutation == 1: evaluation['actor'] = 'player_2'
            elif mutation == 2: evaluation['completed_tick'] = 23
            elif mutation == 3: evaluation['candidates'][1]['total_seconds'] = 3.0
            elif mutation == 4: report['metrics'][0]['visits'][0]['selected_tick'] = 21
            elif mutation == 5: report['metrics'][0]['visits'].append(copy.deepcopy(report['metrics'][0]['visits'][0]))
            else: report['metrics'][0]['visits'][0]['departed_tick'] = 19
            with self.subTest(mutation=mutation), self.assertRaises((ValueError, AssertionError)):
                T.switch_predictions(report, [evaluation])

    def test_abandoned_and_unfinished_switches_remain_in_the_report(self):
        for abandoned in [None, 90]:
            report, evaluation = fixture()
            report['metrics'][0]['visits'][0].update(departed_tick=None, abandoned_tick=abandoned)
            r = T.switch_predictions(report, [evaluation])['switches'][0]
            self.assertEqual(r['outcome'], 'unfinished' if abandoned is None else 'abandoned')
            self.assertIsNone(r['error_seconds'])
            self.assertIsNone(r['actual_seconds_from_source'])
            self.assertIsNone(r['milestone_references']['records'][-1]['error_seconds'])

    def test_value_disagreement_is_not_an_accepted_switch(self):
        report, evaluation = fixture()
        report['missions'][0]['destination_planning'] = dict(switches=0, last_switch=None)
        evaluation['value_comparison'] = dict(preferred=0)
        r = T.switch_predictions(report, [evaluation, copy.deepcopy(evaluation)])
        self.assertEqual(r['switches'], [])
        self.assertEqual(len(r['first_value_time_disagreements']), 1)
        self.assertEqual(r['first_value_time_disagreements'][0]['value_preferred'], 0)

    def test_conflicting_copies_of_a_used_prediction_are_rejected(self):
        report, evaluation = fixture()
        changed = copy.deepcopy(evaluation)
        changed['candidates'][1]['total_seconds'] += 1
        with self.assertRaises(ValueError):
            T.switch_predictions(report, [evaluation, changed])

    def test_plan_keeps_all_predeclared_groups_seats_and_policies(self):
        plan = T.plan()
        self.assertEqual(len(plan), 91)
        self.assertEqual(len({r['name'] for r in plan}), len(plan))
        held = [r for r in plan if r['kind'] == 'held_out']
        self.assertEqual(len(held), 40)
        self.assertEqual(len({r['seed'] for r in held}), 4)
        for seed in {r['seed'] for r in held}:
            for interval in [0, 3]:
                self.assertEqual({(r['version'],r['seat']) for r in held if r['seed']==seed and r['interval']==interval},
                    {(10,None),(12,0),(12,1),(13,0),(13,1)})
        self.assertEqual(len([r for r in plan if r['kind']=='directed']), 48)


if __name__ == '__main__': unittest.main()
