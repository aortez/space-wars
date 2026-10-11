"""Protect the fixed matrix and adverse-outcome reporting before execution."""
import importlib.util
from pathlib import Path
import unittest


TOOLS = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('heldout_audit', TOOLS / 'audit-terrain-recovery-heldout.py')
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class HeldoutTests(unittest.TestCase):
    def test_every_world_has_both_candidate_seats_and_one_control(self):
        matrix = audit.runner.matrix()
        settings = {item['setting'] for item in matrix}
        self.assertEqual(len(settings), 16)
        self.assertEqual(len(matrix), 48)
        self.assertEqual(len({item['case']['seed'] for item in matrix}), 2)
        for setting in settings:
            self.assertEqual([item['case']['candidate_seat'] for item in matrix
                              if item['setting'] == setting], [None, 0, 1])

    def test_improved_recovery_does_not_hide_a_worse_outcome(self):
        control = {'recoveries': 1, 'sorties': 3, 'owned_planets': 2, 'outcome_score': 1,
                   'blocked_ticks': 100, 'longest_block': 80, 'mission_blocked_ticks': 100,
                   'ships_lost': 1, 'death_tick': None}
        candidate = {**control, 'recoveries': 2, 'blocked_ticks': 0, 'longest_block': 0,
                     'mission_blocked_ticks': 0, 'sorties': 2, 'outcome_score': -1, 'death_tick': 500}
        self.assertEqual(audit.regression_flags(control, candidate),
                         ['less_sorties', 'less_outcome_score', 'new_pilot_death'])
        self.assertEqual(audit.regression_flags(control, control), [])


if __name__ == '__main__':
    unittest.main()
