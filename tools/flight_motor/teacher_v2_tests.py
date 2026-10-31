"""The v6 planet-frame teacher: the brake's share leaves the attitude error, the v5 law stays reachable; no native physics."""
import math
import unittest
import torch
from . import teacher_v2 as stage


def row(delta=0., command=16., vx=0., vy=0., gx=0., gy=0., spin=0.):
    """A v2 motor input row: command heading delta, command speed, body-frame relative velocity, gravity, spin; open wings."""
    s = stage.SCALE; g = stage.GRAVITY_SCALE
    return [math.sin(delta), math.cos(delta), command / s, vx / s, vy / s, vx / s, vy / s, spin / 2., gx / g, gy / g, 1., 1., 1., 1., 1., 0., 0., 0.]


class TeacherV2Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)

    def test_the_default_gains_are_the_v6_law_over_the_v5_gains(self):
        self.assertEqual(stage.GAINS['brake_share'], 1.); self.assertEqual(stage.GAINS['brake_share_speed'], 2.5)
        self.assertEqual(stage.GAINS['attitude_forward'], 0.)
        self.assertNotIn('brake_share', stage.GAINS_V5)
        for key, value in stage.GAINS_V5.items():
            self.assertEqual(stage.GAINS[key], value)

    def test_batched_teacher_matches_the_scalar_reference(self):
        rows = [row(), row(delta=1., command=40., vx=5., vy=20., gx=1., gy=-3.), row(command=0., vx=2., vy=-1., gy=-2.), row(delta=3., vx=30., vy=0.)]
        for gains in (stage.GAINS, stage.GAINS_V5):
            batched = stage.teacher_v2(torch.tensor(rows, dtype=torch.float32), gains)
            for i, r in enumerate(rows):
                scalar = stage.scalar_teacher_v2(r, gains)
                for j, key in enumerate(('steering', 'thrust', 'brake')):
                    self.assertAlmostEqual(batched[i, j].item(), scalar[key], places=5)

    def test_an_overspeed_along_the_nose_brakes_without_turning_where_v5_reversed(self):
        # Flying straight ahead at 30 under a command of 10: the brake sheds the
        # excess, so the v6 nose holds, where the v5 attitude vector pointed
        # behind the nose and the steering saturated toward a half turn.
        fast = row(command=10., vy=30.)
        v6 = stage.scalar_teacher_v2(fast); v5 = stage.scalar_teacher_v2(fast, stage.GAINS_V5)
        self.assertEqual(v6['steering'], 0.); self.assertEqual(v6['brake'], 1.); self.assertEqual(v6['thrust'], 0.)
        self.assertEqual(v5['steering'], 1.); self.assertEqual(v5['brake'], 1.)
        # A small overspeed at a cruise behaves the same way with a proportional brake.
        cruise = row(command=50., vy=53.)
        self.assertEqual(stage.scalar_teacher_v2(cruise)['steering'], 0.)
        self.assertAlmostEqual(stage.scalar_teacher_v2(cruise)['brake'], .3, places=5)

    def test_the_share_fades_at_rest_so_a_command_behind_the_nose_still_turns_it(self):
        behind = row(delta=math.pi, command=10.)
        for gains in (stage.GAINS, stage.GAINS_V5):
            self.assertEqual(abs(stage.scalar_teacher_v2(behind, gains)['steering']), 1.)
        # Crawling at one unit per second the share is partial, and the overspeed is still braked.
        crawl = row(command=1., vy=2.)
        v6 = stage.scalar_teacher_v2(crawl)
        self.assertEqual(v6['steering'], 0.); self.assertGreater(v6['brake'], 0.)

    def test_a_speed_the_nose_exceeds_tilts_the_nose_instead_of_reversing_it(self):
        # Rising and drifting under gravity with the nose against it: the lateral
        # error tilts the nose; the forward overspeed is for the thrust cut and
        # gravity, so the v6 target stays within a right angle of the nose
        # where the v5 target pointed behind it.
        rising = row(command=1., vx=5., vy=5., gy=-2.)
        v6 = stage.scalar_teacher_v2(rising)
        v5 = stage.scalar_teacher_v2(rising, stage.GAINS_V5)
        self.assertEqual(v6['thrust'], 0.)
        self.assertLess(abs(v6['steering']), abs(v5['steering']))
        self.assertEqual(abs(v5['steering']), 1.)

    def test_the_other_added_gains_are_inert_at_their_defaults(self):
        rows = torch.tensor([row(delta=.4, command=30., vx=4., vy=10., gx=.5, gy=-1.5), row(command=0., vx=1., vy=1., gy=-2.)], dtype=torch.float32)
        base = stage.teacher_v2(rows)
        for extra in ({'gravity_share': 0.}, {'attitude_relative': 0.}, {'attitude_floor': 0.}, {'thrust_soft': 0.}, {'stop_hold': 0.}, {'hold_floor': 0.}, {'attitude_ramp': (0., 0.)}):
            self.assertTrue(torch.equal(stage.teacher_v2(rows, dict(stage.GAINS, **extra)), base), extra)


if __name__ == '__main__':
    unittest.main()
