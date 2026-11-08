"""LF-076 held-out task set, declared gates and arm selection; no native physics."""
import unittest
from . import polish


def result(family, outcome='success', first=100, post=.2, level=8, mode='forward'):
    return {'family': family, 'outcome': outcome, 'first_success_tick': first, 'max_post_acquisition_velocity_error': post, 'level': level, 'mode': mode}


class PolishTests(unittest.TestCase):
    def test_random_rows_are_deterministic_and_disjoint_from_fitting_seeds(self):
        a = polish.random_rows(); b = polish.random_rows()
        self.assertEqual(a, b); self.assertEqual(len(a), polish.RANDOM_TASKS)
        self.assertTrue(all(r['family'] == 'random' and r['task']['retention_horizon_ticks'] == 600 for r in a))
        self.assertNotEqual(polish.RANDOM_TASK_SEED, 1); self.assertNotEqual(polish.RANDOM_TASK_SEED, 2024)

    def test_measures_and_gates(self):
        speed16 = [result('track', first=60)] * 24 + [result('brake', first=150)] * 40 + [result('brake', level=0, first=30)] * 8 + [result('hold', first=30)] * 8 + [result('turn', first=100)] * 24
        aligned = [result('track')] * 104; held_out = [result('random')] * 150 + [result('random', 'deadline', None)] * 50
        initial = polish.measures(speed16, aligned, held_out)
        self.assertEqual((initial['tracking_cases'], initial['stopping_cases'], initial['random_passed'], initial['physical_failures']), (24, 80, 150, 0))
        better = dict(initial, mean_tracking_acquisition=55., mean_braking_acquisition=120., random_passed=151)
        self.assertTrue(polish.gates(initial, better)['passed'])
        self.assertFalse(polish.gates(initial, dict(better, max_post_acquisition_velocity_error=.7))['passed'])
        self.assertFalse(polish.gates(initial, dict(better, stopping_passed=79))['passed'])
        self.assertFalse(polish.gates(initial, dict(better, development_physical_failures=1))['passed'])
        # Terrain collisions on held-out random tasks are a navigation matter; only an increase fails the gate.
        collided = polish.measures(speed16, aligned, held_out[:-2] + [result('random', 'collision', None)] * 2)
        self.assertEqual((collided['random_physical_failures'], collided['random_passed']), (2, 150))
        self.assertTrue(polish.gates(collided, dict(better, random_physical_failures=2, random_passed=150))['passed'])
        self.assertFalse(polish.gates(collided, dict(better, random_physical_failures=3, random_passed=150))['passed'])
        with_lateral = polish.measures(speed16, aligned, held_out, [result('track', mode='left')] * 48 + [result('brake', 'deadline', None)] * 56)
        self.assertEqual((with_lateral['lateral_passed'], with_lateral['lateral_cases']), (48, 104))
        self.assertTrue(polish.gates(with_lateral, dict(with_lateral, mean_tracking_acquisition=55., mean_braking_acquisition=120.))['passed'])
        self.assertFalse(polish.gates(with_lateral, dict(with_lateral, mean_tracking_acquisition=55., mean_braking_acquisition=120., lateral_passed=47))['passed'])
        self.assertFalse(polish.gates(initial, dict(better, mean_tracking_acquisition=60.))['passed'])

    def test_selection_prefers_more_qualifying_seeds_then_faster_acquisition(self):
        self.assertIsNone(polish.select({'current': {'qualifying_seeds': 1, 'mean_tracking_acquisition': 50.}, 'hold6': {'qualifying_seeds': 1, 'mean_tracking_acquisition': 40.}}))
        self.assertEqual(polish.select({'current': {'qualifying_seeds': 2, 'mean_tracking_acquisition': 50.}, 'hold6': {'qualifying_seeds': 3, 'mean_tracking_acquisition': 60.}}), 'hold6')
        self.assertEqual(polish.select({'current': {'qualifying_seeds': 2, 'mean_tracking_acquisition': 50.}, 'hold6': {'qualifying_seeds': 2, 'mean_tracking_acquisition': 60.}}), 'current')


if __name__ == '__main__':
    unittest.main()
