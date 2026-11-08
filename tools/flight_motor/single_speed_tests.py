"""LF-069 task isolation, unchanged reward, strict gates and protected stopping."""
import copy
import math
from pathlib import Path
import tempfile
import time
import unittest
import torch
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.motor_feedback import direction
from tools.flight_audit.motor_feedback_tests import inputs
from . import single_speed as stage, command_balance, sustained, storage
from .aligned_audit import gate
from .collector import Collector
from .gae_audit import discounted_targets
from .gae_tests import SyntheticBridge
from .model import MotorModel
from .single_speed_audit import feedback_inputs

PARENT = Path('data/training-experiments/learned-flight-motor-command-balance-v1-20261025')


def make(seed, c):
    parent = PARENT/f'inputs/seed-{seed}/checkpoint-0064.pt'
    state = storage.load_torch(parent)
    model = stage.make_model(state['contract']['schema'], c)
    stage.initialize(model, parent, c)
    return model


class SingleSpeedTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)

    def test_only_task_identity_changes_in_training_recipe(self):
        for seed in stage.SEEDS:
            for pre in [False, True]:
                old = command_balance.config(seed, 'balanced', pre); new = stage.config(seed, 'speed16', pre)
                self.assertEqual({k for k in old if old[k] != new[k]}, {'kind', 'arm', 'collector_schedule'})
                self.assertEqual(old.keys(), new.keys())
        self.assertIs(stage.update, sustained.update)

    def test_pools_preserve_original_tasks_and_stopping_order(self):
        c = stage.config(29, 'speed16'); rows = stage.training_candidates(c); pools = stage.collector_pools(c)
        original = sustained.training_candidates({**c, 'preflight': False})
        self.assertEqual(rows, [r for r in original if r['task']['speed'] in [0, 16]])
        self.assertEqual([len(p) for p in pools], [3, 8, 3, 8, 2, 8])
        self.assertEqual(sorted(r['index'] for p in pools for r in p), sorted(r['index'] for r in rows))
        prior = command_balance.collector_pools(command_balance.config(29, 'balanced'))
        for i in [1, 3, 5]: self.assertEqual(pools[i], prior[i])
        for i in [0, 2, 4]: self.assertEqual({r['task']['speed'] for r in pools[i]}, {16})

    def test_preflight_exercises_both_sides_and_continuation(self):
        pools = stage.collector_pools(stage.config(17, 'speed16', True))
        self.assertEqual([len(p) for p in pools], [1]*6)
        self.assertEqual([pools[i][0]['mode'] for i in [0, 2, 4]], ['rest', 'fast', 'reverse'])
        self.assertTrue(all(pools[i][0]['task']['retention_horizon_ticks'] == 900 for i in [0, 2, 4]))

    def test_development_speed_modes_heldout_starts_and_unchanged_zero_tasks(self):
        rows = stage.candidates(5910002); old = sustained.candidates(5910002)
        moving = [r for r in rows if r['family'] == 'track']
        self.assertEqual(len(moving), 24)
        train_seeds = {r['task']['start']['seed'] for r in stage.training_candidates(stage.config(17, 'speed16'))}
        for a, b in zip(rows, old):
            if a['task']['speed'] == 0: self.assertEqual(a, b)
        for mode, forward in [('rest', 0), ('slow', 8), ('fast', 24), ('reverse', -16)]:
            selected = [r for r in moving if r['mode'] == mode]
            self.assertEqual(len(selected), 6)
            self.assertEqual({r['start_variant'] for r in selected}, {0, 1, 2})
            for row in selected:
                task = row['task']; start = task['start']; h = start['bearing'] + start['heading_offset']
                v = start['relative_velocity']
                self.assertEqual(task['speed'], 16); self.assertEqual(row['level'], 16)
                self.assertAlmostEqual(-math.sin(h)*v['x']+math.cos(h)*v['y'], forward)
                self.assertAlmostEqual(math.cos(h)*v['x']+math.sin(h)*v['y'], 0)
                self.assertNotIn(start['seed'], train_seeds)

    def test_initial_model_rng_match_previous_balanced_recipe(self):
        for seed in stage.SEEDS:
            torch.manual_seed(seed); one = make(seed, stage.config(seed, 'speed16')); rng = torch.get_rng_state().clone()
            torch.manual_seed(seed); two = make(seed, command_balance.config(seed, 'balanced'))
            self.assertTrue(equal_tree(one.state_dict(), two.state_dict()))
            self.assertTrue(torch.equal(rng, torch.get_rng_state()))

    def test_collector_reward_targets_and_actual_update_keep_stopping(self):
        with tempfile.TemporaryDirectory() as d:
            torch.manual_seed(43); c = stage.config(43, 'speed16'); model = make(43, c)
            collector = Collector(SyntheticBridge(), stage.training_candidates(c), Path(d)/'traces', stage.collector_pools(c))
            rollout = collector.collect(model, c, time.monotonic()+30)
            self.assertEqual(tuple(rollout['observations'].shape), (256, 6, 18))
            self.assertTrue(torch.equal(rollout['observations'][:, [0, 2, 4], 2], torch.full((256, 3), .8)))
            with torch.no_grad(): last = model(torch.tensor([i['inputs'] for i in collector.items]))[1]
            expected = discounted_targets(torch.full((256, 6), -(2/5)**2/60), torch.zeros(256, 6), last, .99)
            torch.testing.assert_close(rollout['returns'], expected, atol=1e-5, rtol=1e-5)
            before = copy.deepcopy(model.state_dict()); optimizer = torch.optim.Adam(model.parameters(), lr=c['learning_rate'], eps=1e-5)
            metrics = stage.update(model, optimizer, rollout, c)
            self.assertEqual(metrics['minibatches'], 24); self.assertEqual(metrics['stopping_rows'], 768)
            stage.assert_parent_actor(before, model.state_dict()); self.assertFalse(equal_tree(before, model.state_dict()))
            x = rollout['observations'].flatten(0, 1).clone(); x[:, 2] = 0
            with torch.no_grad():
                actor = model(x)[0]; self.assertTrue(torch.equal(actor, MotorModel.forward(model, x)[0]))
                self.assertTrue(torch.equal(model.distribution(actor, x).scale, model.log_std[1:].clamp(-5, 2).exp().expand(len(x), 2)))

    def test_feedback_offsets_preserve_fractional_labels(self):
        anchors = [{'index': i, 'inputs': row.tolist()} for i, row in enumerate(inputs())]
        x, meta = feedback_inputs(anchors)
        self.assertEqual(len(x), 30)
        self.assertEqual([m['error'] for m in meta[:5]], [-6, -.25, 0, .25, 6])
        forward = (x[:, 3:5]*20*direction(x)).sum(1)
        torch.testing.assert_close(forward, torch.tensor([m['command']+m['error'] for m in meta]), atol=3e-6, rtol=1e-5)

    def test_gate_requires_gain_each_mode_and_no_physical_failure(self):
        final = [{**r, 'retained': True, 'outcome': 'success', 'first_success_tick': 30} for r in stage.candidates(5910002)]
        initial = [{**r, 'retained': False} for r in final]; parent = [r for r in final if r['suite'].startswith('legacy')]
        self.assertTrue(gate(initial, final, parent)['passed'])
        fast = [i for i, r in enumerate(final) if r['family'] == 'track' and r['mode'] == 'fast']
        reduced = copy.deepcopy(final)
        for i in fast[:3]: reduced[i]['retained'] = False; reduced[i]['outcome'] = 'deadline'
        self.assertEqual(gate(initial, reduced, parent)['tracking_retained'], 21)
        self.assertFalse(gate(initial, reduced, parent)['passed'])
        self.assertFalse(gate(final, final, parent)['passed'])
        final[0]['outcome'] = 'collision'; final[0]['retained'] = False
        self.assertFalse(gate(initial, final, parent)['passed'])

    def test_undeclared_configs_and_nonpartitioned_pools_rejected(self):
        with self.assertRaises(ValueError): stage.config(42, 'speed16')
        with self.assertRaises(ValueError): stage.config(43, 'speed10')
        with tempfile.TemporaryDirectory() as d:
            c = stage.config(17, 'speed16'); pools = stage.collector_pools(c); pools[0] = pools[2]
            with self.assertRaisesRegex(ValueError, 'partition'):
                Collector(None, stage.training_candidates(c), Path(d)/'unused', pools)
            self.assertFalse((Path(d)/'unused').exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
