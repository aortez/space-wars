"""LF-074 teacher law, synthetic coverage, supervised fit and export shape; no native physics."""
import json
import math
import random
import tempfile
from pathlib import Path
import unittest
import torch
from . import teacher as stage
from .model import ARCHITECTURE, MotorModel

SCHEMA = {'version': 1, 'kind': 'stationary-frame-heading-speed-v1', 'features': ['f%d' % i for i in range(18)],
          'heading': 'world radians; forward=(-sin(heading),cos(heading))',
          'speed': 'nonnegative COM speed along command heading in stationary world frame',
          'action_contract': 'continuous-flight-v2', 'hz': 60}


class TeacherTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)

    def test_batched_teacher_matches_scalar_reference(self):
        x = stage.synthetic_inputs(torch.Generator().manual_seed(3), 2000)
        for gains in [stage.GAINS, stage.GAINS_V3, stage.GAINS_V2, stage.GAINS_V1]:
            batched = stage.teacher(x, gains)
            for row, expected in zip(x.tolist(), batched.tolist()):
                scalar = stage.scalar_teacher(row, gains)
                for value, key in zip(expected, ['steering', 'thrust', 'brake']):
                    self.assertAlmostEqual(value, scalar[key], places=4)

    def test_pursuit_turns_the_nose_toward_a_large_lateral_error_and_not_a_small_one(self):
        def row(vx, vy, command=16.):
            return [0., 1., command / 20, vx / 20, vy / 20, vx / 20, vy / 20, 0., 0., 0., 1., 1., 1., 1., 1., 0., 0., 0.]
        # Lateral velocity toward body +x means the nose must turn left, which is a positive desired spin and negative steering.
        self.assertLess(stage.scalar_teacher(row(8., 12.))['steering'], 0.)
        self.assertEqual(stage.scalar_teacher(row(.5, 15.8))['steering'], 0.)  # Total error below pursuit_start: no pursuit.
        self.assertEqual(stage.scalar_teacher(row(8., 16.))['steering'], 0.)
        self.assertEqual(stage.scalar_teacher(row(8., 24.))['steering'], 0.)
        self.assertEqual(stage.scalar_teacher(row(8., 12.), stage.GAINS_V2)['steering'], 0.)
        self.assertEqual(stage.scalar_teacher(row(8., 0., command=0.))['steering'], 0.)

    def test_teacher_hand_cases(self):
        def row(delta=0., command=16., vx=0., vy=0.):
            return [math.sin(delta), math.cos(delta), command / 20, vx / 20, vy / 20, vx / 20, vy / 20, 0., 0., 0., 1., 1., 1., 1., 1., 0., 0., 0.]
        rest = stage.scalar_teacher(row())
        self.assertEqual((rest['thrust'], rest['brake'], rest['steering']), (1., 0., 0.))
        over = stage.scalar_teacher(row(vy=36.))
        self.assertEqual((over['thrust'], over['brake']), (0., 1.))
        near = stage.scalar_teacher(row(vy=17.))
        self.assertAlmostEqual(near['brake'], .1); self.assertEqual(near['thrust'], 0.)
        reverse = stage.scalar_teacher(row(vy=-16.))
        self.assertEqual((reverse['thrust'], reverse['brake']), (1., 1.))
        stopping = stage.scalar_teacher(row(command=0., vx=3., vy=4.))
        self.assertEqual(stopping['thrust'], 0.); self.assertAlmostEqual(stopping['brake'], .5)
        self.assertLess(stage.scalar_teacher(row(delta=.5))['steering'], 0.)
        self.assertEqual(stage.scalar_teacher(row(delta=3.))['steering'], -1.)
        lateral = stage.scalar_teacher(row(vx=3., vy=16.))
        self.assertAlmostEqual(lateral['brake'], .5); self.assertEqual(lateral['thrust'], 0.)
        self.assertEqual(stage.scalar_teacher(row(vx=.4, vy=16.))['brake'], 0.)
        self.assertEqual(stage.scalar_teacher(row(vx=3., vy=16.), stage.GAINS_V1)['brake'], 0.)
        self.assertEqual(stage.scalar_teacher(row(command=0., vx=3., vy=0.), stage.GAINS)['brake'], stage.scalar_teacher(row(command=0., vx=3., vy=0.), stage.GAINS_V1)['brake'])

    def test_synthetic_rows_are_valid_zero_gravity_open_wing_inputs(self):
        x = stage.synthetic_inputs(torch.Generator().manual_seed(1), 5000)
        self.assertEqual(x.shape, (5000, 18)); self.assertTrue(torch.isfinite(x).all())
        self.assertTrue((x[:, 2] >= 0).all()); self.assertTrue((x[:, 2] == 0).any()); self.assertTrue((x[:, 2] > .9).any())
        self.assertTrue((x[:, 8:10] == 0).all()); self.assertTrue((x[:, 10:15] == 1).all())
        self.assertTrue(((x[:, 0] ** 2 + x[:, 1] ** 2 - 1).abs() < 1e-5).all())

    def test_random_task_is_a_valid_motor_task(self):
        rng = random.Random(7)
        for _ in range(200):
            task = stage.random_task(rng); start = task['start']
            self.assertFalse(start['gravity']); self.assertEqual(start['planet_spin'], 0.); self.assertEqual(start['altitude'], 150.)
            self.assertLessEqual(math.hypot(start['relative_velocity']['x'], start['relative_velocity']['y']), 30. + 1e-6)
            self.assertTrue(0. <= task['speed'] <= 20.); self.assertLessEqual(abs(task['heading_delta']), math.pi / 2)
            self.assertGreater(task['retention_horizon_ticks'], task['deadline_ticks'])

    def test_fit_reduces_error_and_export_keeps_format_one(self):
        generator = torch.Generator().manual_seed(5); model = MotorModel(SCHEMA)
        x = stage.synthetic_inputs(generator, 20000); y = stage.teacher(x)
        with torch.no_grad():
            before = stage.loss_terms(model(x)[0], y).item()
        after = stage.fit(model, x, y, 1500, 1024, 1e-3, generator, log=lambda line: None)
        with torch.no_grad():
            self.assertLess(stage.loss_terms(model(x)[0], y).item(), .5 * before)
        self.assertLess(max(after), .25)
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'policy.json'; model.export(path); exported = json.loads(path.read_text())
        self.assertEqual((exported['format_version'], exported['architecture']), (1, ARCHITECTURE))
        self.assertEqual(sorted(exported['weights']), sorted(k for k in model.state_dict()))
        self.assertEqual(exported['weights']['actor.weight']['shape'], [3, 32])


if __name__ == '__main__':
    unittest.main()
