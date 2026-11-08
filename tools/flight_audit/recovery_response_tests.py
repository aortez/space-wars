"""Synthetic checks for goal isolation, comparison signs and trajectory interpretation."""
import copy
import math
import unittest
from tools.flight_audit.goal_response_tests import inputs
from tools.flight_audit.recovery_response import goal_grid, move_goal, local_goal, wrapped, response, trajectory, anchor_tick


class RecoveryResponseTests(unittest.TestCase):
    def test_only_goal_coordinates_change_without_aliasing(self):
        before = inputs(); saved = copy.deepcopy(before)
        after = move_goal(before, [24., -12.])
        self.assertEqual(before, saved)
        self.assertAlmostEqual(after['self_goal'][0], .08)
        self.assertAlmostEqual(after['self_goal'][1], -.04)
        after['self_goal'][:2] = before['self_goal'][:2]
        self.assertEqual(after, before)
        after['objects'][0][7] = 9
        self.assertEqual(before, saved)
        with self.assertRaises(ValueError): move_goal(before, [float('nan'), 0])

    def test_grid_covers_both_sides_near_radius_and_training_distance(self):
        grid = goal_grid()
        self.assertEqual(len(grid), 25)
        self.assertEqual(len({tuple(c['goal']) for c in grid}), 25)
        self.assertEqual(sum(c['direction'] == 'origin' for c in grid), 1)
        for d in [1.5, 3., 6., 8., 12., 24.]:
            self.assertEqual({tuple(c['goal']) for c in grid if c['distance'] == d}, {(d, 0), (-d, 0), (0, d), (0, -d)})

    def test_response_distinguishes_sign_from_size_and_fixed_bias(self):
        def rows(steer):
            return [{'case': {'kind': 'original'}, 'python_action': {'steering': .4, 'thrust': .2, 'brake': .6}}] + [
                {'case': c, 'python_action': {'steering': steer(c), 'thrust': .2, 'brake': .6}} for c in goal_grid()]
        constant = response(rows(lambda c: .4))
        self.assertFalse(constant['correct_right_left_sign'])
        self.assertEqual(constant['all_goal_spans'], dict(steering=0., thrust=0., brake=0.))
        tiny = response(rows(lambda c: .4 + c['goal'][0]*.001))
        self.assertTrue(tiny['correct_right_left_sign'])
        self.assertFalse(tiny['material_right_left_response'])
        wrong = response(rows(lambda c: -c['goal'][0]*.01))
        self.assertFalse(wrong['correct_right_left_sign'])
        self.assertTrue(wrong['material_right_left_response'])

    def test_ship_local_goal_and_wrapped_turn(self):
        frame = {'angle': math.pi/2, 'center_of_mass': {'x': 5, 'y': 7}, 'target_position': {'x': 2, 'y': 7}}
        x, y = local_goal(frame)
        self.assertAlmostEqual(x, 0); self.assertAlmostEqual(y, 3)
        self.assertAlmostEqual(wrapped(-math.pi+.1-(math.pi-.1)), .2)

    def test_radius_entry_is_not_a_settled_success(self):
        frame = {'angle': 0., 'center_of_mass': {'x': 0., 'y': 2.}, 'target_position': {'x': 0., 'y': 4.},
                 'errors': {'position': 2., 'velocity': 2., 'spin': 0.}}
        initial = copy.deepcopy(frame); initial['center_of_mass']['y'] = 0.; initial['errors']['position'] = 4.
        trace = {'version': 2, 'task': {'arrival': {'position': 3., 'velocity': 1., 'spin': 0.20000000298023224, 'dwell_ticks': 30}, 'deadline_ticks': 30}, 'initial': initial,
                 'steps': [{'frame': copy.deepcopy(frame), 'action': {'steering': 0., 'thrust': .1, 'brake': .6},
                            'evidence': dict(ship_lost=False, collision=False, damage=False, boundary_escape=False),
                            'progress': {'elapsed_ticks': i, 'dwell_ticks': 0, 'outcome': 'deadline' if i == 30 else None}} for i in range(1, 31)]}
        result = trajectory(trace)
        self.assertEqual(result['first_radius_entry_tick'], 1)
        self.assertEqual(result['physical']['outcome'], 'deadline')
        self.assertEqual(result['physical']['maximum_dwell'], 0)
        self.assertEqual(result['net_displacement_toward_initial_goal'], 2.)
        self.assertEqual(result['path_length'], 2.)
        self.assertEqual(anchor_tick(trace), 29)


if __name__ == '__main__': unittest.main(verbosity=2)
