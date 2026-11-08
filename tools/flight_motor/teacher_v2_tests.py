"""The v7 planet-frame teacher: the thrust waits for the nose and the half turn keeps its direction, the v6 and v5 laws stay reachable, and under the v3 contract the turn sense is obeyed; no native physics."""
import math
import unittest
import torch
from . import teacher_v2 as stage


def row(delta=0., command=16., vx=0., vy=0., gx=0., gy=0., spin=0.):
    """A v2 motor input row: command heading delta, command speed, body-frame relative velocity, gravity, spin; open wings."""
    s = stage.SCALE; g = stage.GRAVITY_SCALE
    return [math.sin(delta), math.cos(delta), command / s, vx / s, vy / s, vx / s, vy / s, spin / 2., gx / g, gy / g, 1., 1., 1., 1., 1., 0., 0., 0.]


def row_v3(sense=0., **kwargs):
    """A v3 motor input row: the v2 row and the advisory vector, the turn sense first."""
    return row(**kwargs) + [sense, 0., 0., 0.]


class TeacherV2Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)

    def test_the_default_gains_are_the_v7_law_over_the_v6_and_v5_gains(self):
        self.assertEqual(stage.GAINS['thrust_gate'], (.35, .7)); self.assertEqual(stage.GAINS['half_turn'], .7)
        self.assertNotIn('thrust_gate', stage.GAINS_V6); self.assertNotIn('half_turn', stage.GAINS_V6)
        self.assertEqual(stage.GAINS_V6['brake_share'], 1.); self.assertEqual(stage.GAINS_V6['brake_share_speed'], 2.5)
        self.assertEqual(stage.GAINS_V6['attitude_forward'], 0.)
        self.assertNotIn('brake_share', stage.GAINS_V5)
        for key, value in stage.GAINS_V6.items():
            self.assertEqual(stage.GAINS[key], value)
        for key, value in stage.GAINS_V5.items():
            self.assertEqual(stage.GAINS_V6[key], value)

    def test_batched_teacher_matches_the_scalar_reference(self):
        rows = [row(), row(delta=1., command=40., vx=5., vy=20., gx=1., gy=-3.), row(command=0., vx=2., vy=-1., gy=-2.), row(delta=3., vx=30., vy=0.)]
        for gains in (stage.GAINS, stage.GAINS_V6, stage.GAINS_V5):
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
        v6 = stage.scalar_teacher_v2(fast, stage.GAINS_V6); v5 = stage.scalar_teacher_v2(fast, stage.GAINS_V5)
        self.assertEqual(v6['steering'], 0.); self.assertEqual(v6['brake'], 1.); self.assertEqual(v6['thrust'], 0.)
        self.assertEqual(v5['steering'], 1.); self.assertEqual(v5['brake'], 1.)
        # A small overspeed at a cruise behaves the same way with a proportional brake.
        cruise = row(command=50., vy=53.)
        self.assertEqual(stage.scalar_teacher_v2(cruise, stage.GAINS_V6)['steering'], 0.)
        self.assertAlmostEqual(stage.scalar_teacher_v2(cruise, stage.GAINS_V6)['brake'], .3, places=5)

    def test_the_share_fades_at_rest_so_a_command_behind_the_nose_still_turns_it(self):
        behind = row(delta=math.pi, command=10.)
        for gains in (stage.GAINS_V6, stage.GAINS_V5):
            self.assertEqual(abs(stage.scalar_teacher_v2(behind, gains)['steering']), 1.)
        # Crawling at one unit per second the share is partial, and the overspeed is still braked.
        crawl = row(command=1., vy=2.)
        v6 = stage.scalar_teacher_v2(crawl, stage.GAINS_V6)
        self.assertEqual(v6['steering'], 0.); self.assertGreater(v6['brake'], 0.)

    def test_a_speed_the_nose_exceeds_tilts_the_nose_instead_of_reversing_it(self):
        # Rising and drifting under gravity with the nose against it: the lateral
        # error tilts the nose; the forward overspeed is for the thrust cut and
        # gravity, so the v6 target stays within a right angle of the nose
        # where the v5 target pointed behind it.
        rising = row(command=1., vx=5., vy=5., gy=-2.)
        v6 = stage.scalar_teacher_v2(rising, stage.GAINS_V6)
        v5 = stage.scalar_teacher_v2(rising, stage.GAINS_V5)
        self.assertEqual(v6['thrust'], 0.)
        self.assertLess(abs(v6['steering']), abs(v5['steering']))
        self.assertEqual(abs(v5['steering']), 1.)

    def test_the_other_added_gains_are_inert_at_their_defaults(self):
        rows = torch.tensor([row(delta=.4, command=30., vx=4., vy=10., gx=.5, gy=-1.5), row(command=0., vx=1., vy=1., gy=-2.)], dtype=torch.float32)
        base = stage.teacher_v2(rows)
        for extra in ({'gravity_share': 0.}, {'attitude_relative': 0.}, {'attitude_floor': 0.}, {'thrust_soft': 0.}, {'stop_hold': 0.}, {'hold_floor': 0.}, {'attitude_ramp': (0., 0.)}):
            self.assertTrue(torch.equal(stage.teacher_v2(rows, dict(stage.GAINS, **extra)), base), extra)

    def test_the_contact_terms_are_inert_at_their_defaults(self):
        rows = torch.tensor([row(delta=1.5, command=57., gx=-16.), row(delta=3., command=20., vy=-3., gy=16.), row(command=0., vx=.3, vy=-.2, gy=-16.)],
                            dtype=torch.float32)
        base = stage.teacher_v2(rows, stage.GAINS_V6)
        for extra in ({'half_turn': 0.}, {'thrust_gate': None}, {'brake_floor': None}, {'thrust_lift': (.6, 1., 5., 10.)}):
            self.assertTrue(torch.equal(stage.teacher_v2(rows, dict(stage.GAINS_V6, **extra)), base), extra)

    def test_the_gate_holds_the_thrust_until_the_nose_comes_around(self):
        # On its side at rest under a fast climb command the v6 thrust is full
        # while the nose turns, the push that jams the hull against the ground.
        gated = dict(stage.GAINS_V6, thrust_gate=(.35, .7))
        side = row(delta=1.2, command=57., gx=-15., gy=-5.)
        self.assertEqual(stage.scalar_teacher_v2(side, stage.GAINS_V6)['thrust'], 1.)
        self.assertEqual(stage.scalar_teacher_v2(side, gated)['thrust'], 0.)
        self.assertEqual(stage.scalar_teacher_v2(side, gated)['steering'], stage.scalar_teacher_v2(side, stage.GAINS_V6)['steering'])
        # Once the nose points along its attitude vector the thrust is the v6 thrust.
        aligned = row(delta=0., command=57., gy=-16.)
        self.assertEqual(stage.scalar_teacher_v2(aligned, gated), stage.scalar_teacher_v2(aligned, stage.GAINS_V6))

    def test_the_lift_reopens_the_gate_for_a_slow_ship_whose_nose_points_up(self):
        gated = dict(stage.GAINS_V6, thrust_gate=(.35, .7))
        lifted = dict(gated, thrust_lift=(.6, 1., 5., 10.))
        # On its feet with the command off to one side, the gated thrust waits
        # for a turn the ground may block; lifted, it is the v6 thrust.
        feet = row(delta=1.2, command=20., gy=-16.)
        self.assertLess(stage.scalar_teacher_v2(feet, gated)['thrust'], stage.scalar_teacher_v2(feet, stage.GAINS_V6)['thrust'])
        self.assertEqual(stage.scalar_teacher_v2(feet, lifted), stage.scalar_teacher_v2(feet, stage.GAINS_V6))
        # On its side, or moving fast, the gate holds as before.
        side = row(delta=1.2, command=57., gx=-15., gy=-5.)
        fast = row(delta=1.2, command=57., vy=20., gy=-16.)
        for r in (side, fast):
            self.assertEqual(stage.scalar_teacher_v2(r, lifted), stage.scalar_teacher_v2(r, gated))

    def test_the_half_turn_keeps_the_turn_the_previous_steering_made(self):
        banded = dict(stage.GAINS_V6, half_turn=.7)
        for delta in (math.pi - .1, -(math.pi - .1)):
            shorter = stage.scalar_teacher_v2(row(delta=delta, command=20.), stage.GAINS_V6)['steering']
            self.assertEqual(abs(shorter), 1.)
            for previous in (-1., 1.):
                r = row(delta=delta, command=20.); r[15] = previous
                self.assertEqual(stage.scalar_teacher_v2(r, banded)['steering'], previous, (delta, previous))
            # A ship that was not turning takes the shorter turn at the bias's rate.
            self.assertAlmostEqual(stage.scalar_teacher_v2(row(delta=delta, command=20.), banded)['steering'], .5 * shorter, places=5)
        # Outside the band the previous steering changes nothing, nor inside
        # it when the ordinary turn is not saturated: hovering under gravity
        # with a crawl commanded behind, the nose stays against gravity.
        r = row(delta=1.5, command=20.); r[15] = 1.
        self.assertEqual(stage.scalar_teacher_v2(r, banded), stage.scalar_teacher_v2(r, stage.GAINS_V6))
        # Gravity across the nose and a drift put the attitude vector behind
        # the nose with the command 2.2 radians off, where the band still
        # holds the turn: the ordinary law's full turn is the same either way.
        drifting = row(delta=2.2, command=57., vx=12., vy=-2., gx=-12., gy=14.)
        self.assertEqual(stage.scalar_teacher_v2(drifting, stage.GAINS_V6)['steering'], -1.)
        for previous in (-1., 1.):
            r = list(drifting); r[15] = previous
            self.assertEqual(stage.scalar_teacher_v2(r, banded)['steering'], previous)
        hover = row(delta=math.pi - .1, command=1., gy=-16.); hover[15] = 1.
        self.assertLess(abs(stage.scalar_teacher_v2(hover, stage.GAINS_V6)['steering']), .2)
        self.assertEqual(stage.scalar_teacher_v2(hover, banded), stage.scalar_teacher_v2(hover, stage.GAINS_V6))

    def test_the_brake_floor_releases_the_brake_at_rest(self):
        floored = dict(stage.GAINS_V6, brake_floor=(.5, 2.))
        jitter = row(command=40., vy=-.3)
        self.assertEqual(stage.scalar_teacher_v2(jitter, stage.GAINS_V6)['brake'], 1.)
        self.assertEqual(stage.scalar_teacher_v2(jitter, floored)['brake'], 0.)
        falling = row(command=40., vy=-5.)
        self.assertEqual(stage.scalar_teacher_v2(falling, floored)['brake'], 1.)

    def test_the_contact_sets_are_reproducible_and_allow_contact(self):
        for make, count in ((stage.ground_tasks, 96), (stage.falling_tasks, 80), (stage.half_turn_tasks, 80),
                            (stage.drifting_turn_tasks, 64), (stage.tilted_climb_tasks, 48)):
            rows = make(1)
            self.assertEqual(len(rows), count)
            self.assertEqual(rows, make(1))
            self.assertTrue(all(r['task']['contact_allowed'] and r['task']['start']['gravity'] for r in rows))
            self.assertTrue(all(abs(r['task']['heading_delta']) <= math.pi for r in rows))
        self.assertTrue(all(r['settle'] for r in stage.ground_tasks(1)))
        self.assertFalse(any(r.get('settle') for r in stage.falling_tasks(1) + stage.half_turn_tasks(1)
                             + stage.drifting_turn_tasks(1) + stage.tilted_climb_tasks(1)))

    def test_the_sense_turns_the_long_way_with_the_brake_off_and_the_thrust_gated(self):
        # At rest with the command 1.5 radians to the left, the law turns left
        # by the shorter way; a sense of -1 turns right instead, at the limit,
        # with the brake off and the thrust behind its gate; a sense the
        # shorter way keeps the turn, and a free sense is the v2 law.
        v2 = stage.scalar_teacher_v2(row(delta=1.5))
        self.assertEqual(stage.scalar_teacher_v2(row_v3(delta=1.5)), v2)
        self.assertLess(v2['steering'], -.5)
        reversed_ = stage.scalar_teacher_v2(row_v3(delta=1.5, sense=-1.))
        self.assertAlmostEqual(reversed_['steering'], 1., places=5)
        self.assertEqual(reversed_['brake'], 0.); self.assertEqual(reversed_['thrust'], 0.)
        self.assertLess(stage.scalar_teacher_v2(row_v3(delta=1.5, sense=1.))['steering'], -.5)
        # The sense's turn slows into the attitude vector as the ordinary law
        # does: the shorter way under a sense is the ordinary steering.
        self.assertAlmostEqual(stage.scalar_teacher_v2(row_v3(delta=-.5, sense=-1.))['steering'],
                               stage.scalar_teacher_v2(row(delta=-.5))['steering'], places=5)
        # The long way round to a command 0.5 radians right, taken left, is a
        # saturated left turn.
        self.assertAlmostEqual(stage.scalar_teacher_v2(row_v3(delta=-.5, sense=1.))['steering'], -1., places=5)
        # Stopping commands obey the sense too.
        stopping = stage.scalar_teacher_v2(row_v3(delta=2., command=0., sense=-1.))
        self.assertAlmostEqual(stopping['steering'], 1., places=5); self.assertEqual(stopping['brake'], 0.)
        with self.assertRaises(ValueError):
            stage.teacher_v2(torch.tensor([row() + [0.]], dtype=torch.float32))

    def test_the_blocked_tasks_cross_the_ground_to_a_command_on_the_other_side(self):
        rows = stage.blocked_tasks(1)
        self.assertEqual(len(rows), 96); self.assertEqual(rows, stage.blocked_tasks(1))
        for r in rows:
            task = r['task']
            self.assertTrue(task['contact_allowed'] and task['start']['gravity'] and r['settle'])
            attitude = float(r['mode'][len('attitude'):])
            command = stage.wrap(task['heading_delta'] + attitude)
            self.assertLess(command * attitude, 0.)
            self.assertGreater(abs(attitude) + abs(command), math.pi)
            self.assertLessEqual(abs(command), 1.3)
        sets = stage.contact_sets(1, contract=3)
        self.assertIn('blocked', sets)
        self.assertTrue(all(r['task']['contract'] == 3 for rows in sets.values() for r in rows))
        plain = stage.contact_sets(1)
        self.assertNotIn('blocked', plain)
        self.assertTrue(all(r['task']['contract'] == 2 for rows in plain.values() for r in rows))
        rng = __import__('random').Random(3)
        kinds = {r['task']['heading_delta'] != 0. and r.get('settle', False) for r in (stage.random_contact_task(rng, 3) for _ in range(40))}
        self.assertEqual(kinds, {True, False})

    def test_synthetic_v3_rows_carry_a_sense_on_two_fifths_and_zero_spares(self):
        rows = stage.synthetic_inputs_v2(torch.Generator().manual_seed(1), 10000, contract=3)
        self.assertEqual(tuple(rows.shape), (10000, 22))
        sense = rows[:, stage.SENSE]
        self.assertTrue(set(sense.unique().tolist()) <= {-1., 0., 1.})
        self.assertAlmostEqual((sense != 0).float().mean().item(), .4, delta=.03)
        self.assertEqual(rows[:, stage.SENSE + 1:].abs().sum().item(), 0.)
        self.assertEqual(tuple(stage.synthetic_inputs_v2(torch.Generator().manual_seed(1), 100).shape), (100, 18))
        self.assertEqual(stage.under_contract([{'task': {'contract': 2}}], 3), [{'task': {'contract': 3}}])


if __name__ == '__main__':
    unittest.main()
