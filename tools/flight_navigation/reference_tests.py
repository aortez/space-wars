"""Navigation reference geometry: ray headings, clearance cones, speed schedule and command direction."""
import math
import unittest
from . import reference


class ReferenceTests(unittest.TestCase):
    def test_ray_headings_follow_the_motor_convention(self):
        self.assertAlmostEqual(reference.ray_heading(math.pi / 2), 0.)          # Forward ray.
        self.assertAlmostEqual(reference.ray_heading(0.), -math.pi / 2)         # Body +x is to the right, a negative heading.
        self.assertAlmostEqual(reference.ray_heading(math.pi), math.pi / 2)     # Body -x is to the left.

    def test_clearance_is_the_corridor_free_distance(self):
        bearings = [0., math.pi / 2, math.pi, math.pi / 2 + math.radians(10.)]
        # A hit straight ahead at 150, one to the right at 30 (outside a 9-unit corridor), none behind-left, and one 10 degrees left of ahead at 40.
        rays = [[1., .1, 0., 0.], [1., .5, 0., 0.], [0., 0., 0., 0.], [1., 40. / 300., 0., 0.]]
        forward, left = reference.clearance_by_heading(rays, bearings, [0., math.pi / 2], 9.)
        self.assertAlmostEqual(forward, 40. * math.cos(math.radians(10.)), places=3); self.assertAlmostEqual(left, 300.)
        self.assertAlmostEqual(reference.clearance_by_heading(rays, bearings, [0.], 5.)[0], 150.)

    def test_command_points_at_a_clear_goal_and_slows_near_it(self):
        bearings = [i * 2 * math.pi / 128 for i in range(128)]; clear = [[0., 0., 0., 0.]] * 128
        far = {'self_goal': [0., 100. / 300.] + [0.] * 33, 'rays': clear}
        action, info = reference.command(far, bearings)
        self.assertAlmostEqual(action['heading_sin'], 0., places=5); self.assertAlmostEqual(action['heading_cos'], 1., places=5)
        self.assertEqual(action['speed'], 20.)
        near = {'self_goal': [0., 1. / 300.] + [0.] * 33, 'rays': clear}
        self.assertEqual(reference.command(near, bearings)[0]['speed'], 0.)
        beside = {'self_goal': [5. / 300., 0.] + [0.] * 33, 'rays': clear}
        turned, _ = reference.command(beside, bearings)
        self.assertLess(turned['heading_sin'], -.9)  # Outside the stop radius the nose still turns toward a goal beside the ship.
        self.assertAlmostEqual(turned['speed'], .6 * 3.)
        inside = {'self_goal': [2. / 300., 0.] + [0.] * 33, 'rays': clear}
        held, _ = reference.command(inside, bearings)
        self.assertAlmostEqual(held['heading_sin'], 0.); self.assertAlmostEqual(held['heading_cos'], 1.); self.assertEqual(held['speed'], 0.)
        right = {'self_goal': [50. / 300., 0.] + [0.] * 33, 'rays': clear}
        self.assertLess(reference.command(right, bearings)[0]['heading_sin'], 0.)

    def test_command_steers_around_a_wall_ahead(self):
        bearings = [i * 2 * math.pi / 128 for i in range(128)]
        rays = [[1., 15. / 300., 0., 0.] if abs(reference.ray_heading(b)) < math.radians(25.) else [0., 0., 0., 0.] for b in bearings]
        action, info = reference.command({'self_goal': [0., 100. / 300.] + [0.] * 33, 'rays': rays}, bearings)
        self.assertGreater(abs(math.atan2(action['heading_sin'], action['heading_cos'])), math.radians(20.))
        self.assertGreater(action['speed'], 0.)

    def test_zero_gravity_copy_leaves_geometry_untouched(self):
        candidate = {'family': 'clear', 'recipe': 'r', 'task': {'start': {'gravity': True, 'planet_spin': .015, 'seed': 3}, 'obstacles': [{'x': 1}]}}
        row = reference.stationary_zero_gravity(candidate)
        self.assertFalse(row['task']['start']['gravity']); self.assertEqual(row['task']['start']['planet_spin'], 0.)
        self.assertEqual(row['task']['obstacles'], candidate['task']['obstacles']); self.assertTrue(candidate['task']['start']['gravity'])


if __name__ == '__main__':
    unittest.main()
