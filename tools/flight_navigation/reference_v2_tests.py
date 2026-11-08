"""The v3 navigation law's turn sense: the cases shared with the motor environment's rule, the reading from an observation and the advisory on the command."""
import json
import math
from pathlib import Path
import unittest

from . import reference_v2
from .fit_tests import FakeBridge, observation

CASES = Path(__file__).resolve().parents[2] / 'scenarios/spacewars/src/surface_sortie/flight_environment/turn_sense_cases.json'


class TurnSenseTests(unittest.TestCase):
    def test_the_rule_agrees_with_the_shared_cases(self):
        cases = json.loads(CASES.read_text())
        self.assertGreaterEqual(len(cases), 20)
        for case in cases:
            nearest = tuple(case['nearest']) if case['nearest'] is not None else None
            self.assertEqual(reference_v2.turn_sense(nearest, case['error'], case['spin'], case['previous_steering']), case['sense'], case)

    def test_the_reading_takes_the_nearest_surface_within_reach_the_spin_and_the_steering(self):
        inputs = observation()
        inputs['nearest'] = [1., 3. / 300., -3. / 300., 0., 0., 0.]
        inputs['self_goal'][10] = .5 / reference_v2.SPIN_SCALE; inputs['self_goal'][29] = -.95
        nearest, error, spin, steering = reference_v2.sense_reading(inputs, 1.5)
        self.assertAlmostEqual(nearest[0], 3.); self.assertAlmostEqual(nearest[1], math.pi / 2)
        self.assertEqual(error, 1.5); self.assertAlmostEqual(spin, .5); self.assertEqual(steering, -.95)
        # A surface eight units off is within the old reach of ten and beyond the hull.
        inputs['nearest'] = [1., 8. / 300., -8. / 300., 0., 0., 0.]
        self.assertIsNone(reference_v2.sense_reading(inputs, 1.5)[0])
        inputs['nearest'] = [0.] * 6
        self.assertIsNone(reference_v2.sense_reading(inputs, 1.5)[0])

    def test_the_v3_law_adds_the_advisory_to_its_command_and_the_v2_law_does_not(self):
        bridge = FakeBridge(); bearings = bridge.contract['schema']['ray_bearings']; edges = bridge.contract['schema']['radial_edges']
        # The goal to the left and the ground three units away on that side:
        # the shorter turn toward the goal carries the nose into it.
        inputs = observation(goal=(-100., 0.))
        inputs['nearest'] = [1., 3. / 300., -3. / 300., 0., 0., 0.]
        v2, _ = reference_v2.command(inputs, bearings, reference_v2.LAW, edges)
        self.assertNotIn('advisory', v2)
        v3, _ = reference_v2.command(inputs, bearings, reference_v2.LAW_V3, edges)
        self.assertEqual({k: v for k, v in v3.items() if k != 'advisory'}, v2)
        self.assertGreater(math.atan2(v3['heading_sin'], v3['heading_cos']), reference_v2.SENSE_GATE)
        self.assertEqual(v3['advisory'], [-1., 0., 0., 0.])
        inputs['nearest'] = [0.] * 6
        self.assertEqual(reference_v2.command(inputs, bearings, reference_v2.LAW_V3, edges)[0]['advisory'], [0., 0., 0., 0.])

    def test_the_grounded_family_starts_low_at_any_attitude_with_the_goal_up_the_radial(self):
        rows = reference_v2.grounded_sets(5, 24)
        self.assertEqual(len(rows), 24); self.assertEqual(rows, reference_v2.grounded_sets(5, 24))
        for row in rows:
            task = row['task']; start = task['start']
            self.assertEqual(row['family'], 'grounded'); self.assertTrue(task['contact_allowed'] and start['gravity'])
            self.assertEqual(start['altitude'], reference_v2.GROUNDED_ALTITUDE)
            up = (-math.sin(start['bearing']), math.cos(start['bearing']))
            fall = -(start['relative_velocity']['x'] * up[0] + start['relative_velocity']['y'] * up[1])
            self.assertTrue(any(abs(fall - f) < 1e-4 for f in reference_v2.GROUNDED_FALLS))
            self.assertAlmostEqual(task['goal_offset']['x'] * up[0] + task['goal_offset']['y'] * up[1], reference_v2.GROUNDED_CLIMB, places=4)
        self.assertGreater(len({round(r['task']['start']['heading_offset'], 1) for r in rows}), 12)


if __name__ == '__main__':
    unittest.main()
