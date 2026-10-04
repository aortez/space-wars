import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('continuation', Path(__file__).parents[1]/'validate-flight-continuation.py')
M = importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)


class FlightContinuationTests(unittest.TestCase):
    def test_fixed_comparison_changes_only_continuation_flag(self):
        self.assertEqual(len(M.P.plan()), 14)
        self.assertEqual(len(M.retention_plan()), 7)
        self.assertEqual([p['name'] for p in M.retention_plan() if p['kind'] == 'armed'], ['shared-armed-world1-p1-powered'])
        for item in M.P.plan():
            self.assertEqual(M.arguments(item, False), M.R.arguments(item, True))
            self.assertEqual(M.arguments(item, True), M.arguments(item, False)+['--active-flight-checks', 'true'])

    def test_active_prediction_keeps_original_launch_clock_and_remaining_reserve(self):
        plan = dict(planet=0, revision=2, direction='Left', start=dict(x=0,y=60), destination=dict(x=10,y=60),
                    cruise_radius=70, anchor=dict(Vehicle=dict(index=0, form='ship', position=dict(x=5,y=64), angle=0)))
        launch = dict(version=2, measured_tick=100, launch_until_tick=220, plan=plan)
        c = dict(version=1, tick=300, charge=0.5, request=dict(launch=launch, launched_tick=180, plan=plan, phase='Cross'),
                 plan=plan, rejection=None, remaining=dict(seconds=5, burn_seconds=1, arrival_speed=3))
        self.assertTrue(M.audit_prediction(c, 300, 0.5))
        for mutate in [
            lambda a: a.update(tick=299),
            lambda a: a['request'].update(launched_tick=99),
            lambda a: a['request']['launch'].update(launch_until_tick=221),
            lambda a: a['remaining'].update(seconds=11),
            lambda a: a['remaining'].update(burn_seconds=1.36),
            lambda a: a['remaining'].update(arrival_speed=float('nan')),
            lambda a: a.update(charge=0.6),
            lambda a: a.update(rejection='world_clearance'),
            lambda a: a['request'].update(phase='Approach'),
        ]:
            invalid = copy.deepcopy(c)
            mutate(invalid)
            with self.assertRaises(AssertionError):
                M.audit_prediction(invalid, 300, 0.5)
        failed = copy.deepcopy(c)
        failed.update(rejection='world_clearance', remaining=None, plan=None)
        self.assertFalse(M.audit_prediction(failed, 300, 0.5))


if __name__ == '__main__':
    unittest.main()
