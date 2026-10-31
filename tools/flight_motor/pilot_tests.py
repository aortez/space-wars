"""LF-075 collector accumulation, per-step discounting, GAE and reward variants on a fake bridge."""
import copy
import math
import unittest
import torch
from . import pilot
from .model import MotorModel
from .teacher_tests import SCHEMA


def item(velocity_error, outcome=None):
    return {'inputs': [0., 1., .8, 0., .5, 0., .5, 0., 0., 0., 1., 1., 1., 1., 1., 0., 0., 0.],
            'errors': {'heading': 0., 'velocity': velocity_error, 'spin': 0.}, 'progress': {'outcome': outcome}}


class FakeBridge:
    """Two slots; slot 0 ends its first episode after four ticks, slot 1 never ends."""

    def __init__(self):
        self.tick = 0; self.resets = []; self.steps = []

    def call(self, op, **kw):
        if op == 'reset':
            return {'items': [item(1.) for _ in kw['tasks']]}
        if op == 'reset_at':
            self.resets.append((self.tick, kw['index'])); return {'items': [item(1.), item(1.)]}
        if op == 'step':
            self.tick += 1; self.steps.append([dict(a) for a in kw['actions']])
            first = item(1., 'deadline' if self.tick == 4 else None)
            return {'items': [first, item(2.)]}
        raise ValueError(op)


class PilotTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True); torch.manual_seed(0)

    def test_steep_reward_adds_constant_slope_inside_the_band(self):
        current = [pilot.reward_total(item(e), 'current') for e in [.0, .5, 1.]]
        steep = [pilot.reward_total(item(e), 'steep') for e in [.0, .5, 1.]]
        self.assertAlmostEqual(steep[1] - current[1], -.5 / 5 / 60); self.assertAlmostEqual(steep[0], current[0])
        self.assertGreater(current[1] - current[2], 0.); self.assertGreater((steep[1] - steep[2]) / (current[1] - current[2]), 2.)

    def test_hold_accumulates_discounted_rewards_and_resets_on_termination(self):
        model = MotorModel(SCHEMA); bridge = FakeBridge(); pools = [[{'family': 'track', 'task': {}}], [{'family': 'brake', 'task': {}}]]
        collector = pilot.PilotCollector(bridge, pools, 'current', hold=3, gamma=.9)
        rollout = collector.collect(model, ticks=6, lam=1.)
        # Slot 1 holds twice for three ticks; slot 0 ends at tick four, then decides at ticks five and six.
        self.assertEqual(rollout['returns'].shape[0], 2 + 3)
        self.assertEqual(bridge.resets, [(4, 0)]); self.assertEqual(collector.episodes, 1); self.assertEqual(collector.outcomes, {'track/deadline': 1})
        per_tick = pilot.reward_total(item(2.), 'current')
        self.assertTrue(torch.isfinite(rollout['advantages']).all())
        self.assertEqual([len(s) for s in bridge.steps], [2] * 6)
        self.assertEqual(bridge.steps[0][1], bridge.steps[2][1]); self.assertNotEqual(bridge.steps[2][1], bridge.steps[3][1])

    def test_per_step_gamma_matches_hand_computation_without_termination(self):
        model = MotorModel(SCHEMA); bridge = FakeBridge(); bridge.tick = 100
        collector = pilot.PilotCollector(bridge, [[{'family': 'brake', 'task': {}}], [{'family': 'brake', 'task': {}}]], 'current', hold=2, gamma=.5)
        rollout = collector.collect(model, ticks=4, lam=1.)
        with torch.no_grad():
            value = model(rollout['observations'][:, 0])[1]; final = model(pilot.tensor(collector.items))[1]
        r0, r1 = pilot.reward_total(item(1.), 'current'), pilot.reward_total(item(2.), 'current')
        # Slot 1: two decisions, each worth r1 + .5 r1 with discount .25 to the next value.
        expected_last = r1 + .5 * r1 + .25 * final[1] - value[3]
        expected_first = r1 + .5 * r1 + .25 * (expected_last + value[3]) - value[2]
        self.assertAlmostEqual(rollout['advantages'][3, 0].item(), expected_last.item(), places=5)
        self.assertAlmostEqual(rollout['advantages'][2, 0].item(), expected_first.item(), places=5)
        self.assertAlmostEqual(rollout['advantages'][0, 0].item(), (r0 + .5 * r0 + .25 * (rollout['advantages'][1, 0] + value[1]) - value[0]).item(), places=5)

    def test_consecutive_windows_restart_holds_after_a_reset_shifts_their_phase(self):
        model = MotorModel(SCHEMA); bridge = FakeBridge()
        collector = pilot.PilotCollector(bridge, [[{'family': 'track', 'task': {}}], [{'family': 'brake', 'task': {}}]], 'current', hold=3, gamma=.9)
        first = collector.collect(model, ticks=6, lam=1.)
        second = collector.collect(model, ticks=6, lam=1.)
        # Slot 0 reset at tick four, so its hold was mid-flight at the window end; the next window must still step every slot.
        self.assertEqual(len(bridge.steps), 12); self.assertTrue(all(len(step) == 2 and None not in step for step in bridge.steps))
        self.assertEqual(second['returns'].shape[0], 2 + 2); self.assertTrue(torch.isfinite(first['advantages']).all())

    def test_deterministic_steering_uses_the_mean_and_actuator_likelihood(self):
        model = MotorModel(SCHEMA); bridge = FakeBridge(); bridge.tick = 100
        collector = pilot.PilotCollector(bridge, [[{'family': 'brake', 'task': {}}]] * 2, 'current', hold=1, gamma=.99, steering='deterministic')
        rollout = collector.collect(model, ticks=3, lam=1.)
        with torch.no_grad():
            actor = model(rollout['observations'][:, 0])[0]
        self.assertTrue(torch.allclose(rollout['latent'][:, 0], actor[:, 0]))
        flat = {k: v.flatten(0, 1) for k, v in rollout.items()}
        logp, entropy = pilot.actuator_terms(model, actor, flat, torch.arange(len(flat['logp'])), pilot.config(29, 'warm', 'current'))
        self.assertTrue(torch.allclose(logp, flat['logp'], atol=1e-5)); self.assertTrue(torch.isfinite(entropy))
        self.assertEqual(pilot.config(29, 'warm', 'current')['steering_sampler'], 'deterministic')
        self.assertEqual(pilot.config(29, 'fresh', 'current')['steering_sampler'], 'gaussian')

    def test_curriculum_stages_and_retargeting(self):
        c = pilot.config(29, 'fresh', 'current', stage_updates=21)
        self.assertEqual((c['updates'], c['curriculum']), (84, [[21, 4.], [21, 8.], [21, 12.], [21, 16.]]))
        self.assertEqual([pilot.stage_speed(c, i) for i in [0, 20, 21, 41, 42, 62, 63, 90]], [4., 4., 8., 8., 12., 12., 16., 16.])
        self.assertIsNone(pilot.stage_speed(pilot.config(29, 'fresh', 'current'), 5))
        track = {'family': 'track', 'level': 16, 'task': {'speed': 16, 'start': {'relative_velocity': {'x': 0., 'y': -16.}}}}
        brake = {'family': 'brake', 'level': 6, 'task': {'speed': 0., 'start': {'relative_velocity': {'x': 6., 'y': 0.}}}}
        pools = [[dict(track)], [dict(brake)]]; original = [[dict(track)], [dict(brake)]]
        pilot.retarget(pools, original, 4.)
        self.assertEqual(pools[0][0]['task']['speed'], 4.); self.assertEqual(pools[0][0]['task']['start']['relative_velocity'], {'x': 0., 'y': -4.})
        self.assertEqual(pools[1][0], brake); self.assertEqual(original[0][0]['task']['speed'], 16)
        pilot.retarget(pools, original, 16.); self.assertEqual(pools[0][0]['task']['start']['relative_velocity'], {'x': 0., 'y': -16.})

    def test_steering_anchor_is_small_on_the_law_and_large_off_it(self):
        from . import teacher
        c = pilot.config(29, 'warm', 'repeat', steering_anchor=10.); x = teacher.synthetic_inputs(torch.Generator().manual_seed(9), 512)
        model = MotorModel(SCHEMA); teacher.fit(model, x, teacher.teacher(x), 400, 256, 1e-3, torch.Generator().manual_seed(1), log=lambda line: None)
        flat = {'observations': x}; index = torch.arange(len(x))
        with torch.no_grad():
            on_law = pilot.steering_anchor(model, model(x)[0], flat, index, c).item()
            model.actor.bias[0] += 1.5
            off_law = pilot.steering_anchor(model, model(x)[0], flat, index, c).item()
        self.assertLess(on_law, off_law / 5); self.assertEqual(pilot.config(29, 'warm', 'repeat')['steering_anchor'], 0.)

    def test_matrix_pools_replace_only_the_movement_pools(self):
        from . import single_speed
        pools = single_speed.collector_pools(single_speed.config(29, 'speed16')); stopping = [copy.deepcopy(p) for p in pools if p[0]['family'] != 'track']
        matrix = pilot.matrix_pools(pools)
        movement = [p for p in matrix if p[0]['family'] == 'track']
        self.assertEqual(len(movement), 3); self.assertEqual(sum(len(p) for p in movement), 48)
        self.assertEqual(sorted({r['task']['speed'] for p in movement for r in p}), [4, 10, 16])
        self.assertEqual({r['mode'] for p in movement for r in p}, {'rest', 'slow', 'fast', 'reverse', 'left', 'right', 'forward_left', 'forward_right'})
        self.assertTrue(all(r['task']['retention_horizon_ticks'] == 900 for p in movement for r in p))
        self.assertEqual([p for p in matrix if p[0]['family'] != 'track'], stopping)
        self.assertEqual(pilot.config(29, 'warm', 'repeat', pools='matrix')['pools'], 'matrix')

    def test_config_cells_and_probe(self):
        self.assertEqual(pilot.config(29, 'warm', 'repeat')['hold'], 6); self.assertEqual(pilot.config(29, 'fresh', 'steep')['critic_warmup_updates'], 0)
        with self.assertRaises(ValueError):
            pilot.config(5, 'warm', 'current')
        with self.assertRaises(ValueError):
            pilot.config(29, 'fresh', 'current', init='hover')
        model = MotorModel(SCHEMA); result = pilot.probe(model)
        self.assertTrue(math.isfinite(result['slope_at_16']) and math.isfinite(result['bias_at_16']))
        self.assertEqual(pilot.probe(model, command=16.), pilot.probe(model, command=16., speeds=(15., 16., 17.)))
        self.assertNotEqual(pilot.probe(model, command=4.)['bias_at_16'], result['bias_at_16'])

    def test_coast_initialization_sets_actuator_biases_only_for_fresh_starts(self):
        c = pilot.config(29, 'fresh', 'current', init='coast'); model = pilot.make_model(SCHEMA, c, None)
        self.assertEqual(model.actor.bias[1].item(), -2.); self.assertEqual(model.actor.bias[2].item(), -2.)
        standard = pilot.make_model(SCHEMA, pilot.config(29, 'fresh', 'current'), None)
        self.assertEqual(standard.actor.bias[1].item(), -4.); self.assertEqual(standard.actor.bias[2].item(), 0.)
        self.assertGreater(pilot.probe(model, command=16.)['bias_at_16'], pilot.probe(standard, command=16.)['bias_at_16'])


if __name__ == '__main__':
    unittest.main()
