"""LF-083 output mapping, loss, collection labelling on a fake bridge, and native export parity."""
import json
import math
from pathlib import Path
import random
import tempfile
import unittest

import torch
from tools.flight_ppo.bridge import Bridge
from . import fit, reference
from .model import FORMAT_VERSION, MAX_SPEED, NavigationModel, command_dicts, commands, tensor


def observation(goal=(0., 100.)):
    return {'self_goal': [goal[0] / 300., goal[1] / 300.] + [0.] * 33, 'cells': [[0., 0.]] * 1472,
            'rays': [[0., 0., 0., 0.]] * 128, 'nearest': [0.] * 6, 'objects': [[0.] * 16] * 32}


class FakeBridge:
    """One slot; every episode ends after three decisions. Records each call."""

    def __init__(self, refuse_policy=False):
        self.calls = []; self.decisions = 0; self.refuse_policy = refuse_policy
        self.contract = {'schema': {'ray_bearings': [(i + .5) * 2 * math.pi / 128 for i in range(128)], 'radial_edges': [10. * i for i in range(24)]}}

    def item(self, outcome=None):
        return {'inputs': observation(), 'progress': {'outcome': outcome, 'elapsed_ticks': 6 * self.decisions}}

    def call(self, op, **kw):
        self.calls.append((op, kw))
        if op == 'reset':
            self.decisions = 0; return {'items': [self.item()]}
        if op == 'step_policy' and self.refuse_policy:
            raise RuntimeError('invalid navigation command')
        if op in ('step_command', 'step_policy'):
            self.decisions += 1; return {'items': [self.item('deadline' if self.decisions == 3 else None)]}
        raise ValueError(op)


class FitTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.manual_seed(0)

    def test_commands_map_the_pair_through_tanh_and_the_speed_through_a_scaled_sigmoid(self):
        actor = torch.tensor([[0., 1., 0.], [-20., 20., 20.]])
        out = commands(actor)
        torch.testing.assert_close(out[0], torch.tensor([0., math.tanh(1.), 10.])); self.assertAlmostEqual(out[1, 0].item(), -1.); self.assertAlmostEqual(out[1, 2].item(), MAX_SPEED, places=4)

    def test_loss_vanishes_at_saturated_targets_and_heading_error_wraps(self):
        target = torch.tensor([[0., 1., 20.], [0., -1., 0.]])
        exact = torch.tensor([[0., 20., 30.], [0., -20., -30.]])
        self.assertLess(fit.loss_terms(exact, target).item(), 1e-6)
        angle, speed = fit.command_error(exact, target)
        self.assertLess(angle.max().item(), 1e-4); self.assertLess(speed.max().item(), 1e-4)
        # A heading just past the seam behind the ship is a small wrapped error, not nearly a full turn.
        behind = torch.tensor([[math.sin(-3.1), math.cos(-3.1), 0.]]); near_seam = torch.tensor([[math.atanh(math.sin(3.1)), math.atanh(math.cos(3.1)), -30.]])
        angle, _ = fit.command_error(near_seam, behind)
        self.assertAlmostEqual(angle.item(), 2 * math.pi - 6.2, places=3)

    def test_collection_labels_every_row_with_the_law_and_drives_as_asked(self):
        rows = [{'index': 0, 'family': 'clear', 'task': {}}, {'index': 1, 'family': 'altered', 'task': {}}]
        bridge = FakeBridge()
        x, y, meta, outcomes = fit.collect(bridge, rows, random.Random(1), 'law', {'rate': 0., 'max_decisions': 6})
        self.assertEqual(tuple(x.shape), (6, 4009)); self.assertEqual(outcomes, {'deadline': 2})
        self.assertEqual(meta[:, 0].tolist(), [0., 0., 0., 2., 2., 2.]); self.assertAlmostEqual(meta[0, 1].item(), 100.)
        expected, command, _ = fit.label(observation(), bridge.contract['schema']['ray_bearings'], bridge.contract['schema']['radial_edges'])
        torch.testing.assert_close(y[0], torch.tensor(expected)); self.assertEqual(expected[2], 20.)
        steps = [kw for op, kw in bridge.calls if op == 'step_command']
        self.assertEqual(len(steps), 6); self.assertTrue(all(s['commands'] == [command] and s['hold'] == fit.HOLD for s in steps))
        bridge = FakeBridge()
        fit.collect(bridge, rows[:1], random.Random(1), 'law', {'rate': 1., 'max_decisions': 1})
        bursts = [kw['commands'][0] for op, kw in bridge.calls if op == 'step_command']
        self.assertEqual(len(bursts), 3); self.assertTrue(all(0. <= b['speed'] <= MAX_SPEED and b != command for b in bursts))
        bridge = FakeBridge()
        x, y, _, outcomes = fit.collect(bridge, rows[:1], random.Random(1), 'policy')
        self.assertEqual([op for op, _ in bridge.calls], ['reset'] + ['step_policy'] * 3); torch.testing.assert_close(y[1], torch.tensor(expected))
        refused = FakeBridge(refuse_policy=True)
        x, y, _, outcomes = fit.collect(refused, rows, random.Random(1), 'policy')
        self.assertEqual(outcomes, {'invalid_command': 2}); self.assertEqual(len(x), 2)


class NativeTests(unittest.TestCase):
    """The built composed bridge and the LF-078 motor export must be present."""

    def test_a_v3_model_has_a_sense_head_the_native_loader_reads(self):
        with Bridge(reference.DEFAULT_BRIDGE) as bridge:
            model = NavigationModel(bridge.contract['schema'], contract=3)
            self.assertEqual(model.actor.out_features, 4); self.assertEqual(tuple(model.log_std.shape), (4,))
            with torch.no_grad():
                model.actor.bias[3] = 2.
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / 'v3.json'; model.export(path)
                record = json.loads(path.read_text())
                self.assertEqual(record['format_version'], 6); self.assertEqual(record['weights']['actor.weight']['shape'], [4, 64])
                self.assertEqual(bridge.call('load_navigation_policy', path=str(path))['contract'], 3)
            item = observation()
            native = bridge.call('infer_navigation', inputs=[item])['outputs'][0]
            with torch.no_grad():
                actor = model(tensor([{'inputs': item}]))[0][0]
            self.assertAlmostEqual(native['sense_latent'], actor[3].item(), places=4)
            self.assertEqual(native['command']['advisory'], [1., 0., 0., 0.])
            for a, b in zip(native['latents'], actor[:3].tolist()):
                self.assertAlmostEqual(a, b, places=4)
        rows = command_dicts(torch.tensor([[0., 1., 0., 2.], [0., 1., 0., -2.], [0., 1., 0., .3]]), max_speed=60.)
        self.assertEqual([r['advisory'] for r in rows], [[1., 0., 0., 0.], [-1., 0., 0., 0.], [0., 0., 0., 0.]])
        self.assertNotIn('advisory', command_dicts(torch.tensor([[0., 1., 0.]]))[0])
        self.assertEqual(tuple(commands(torch.zeros((2, 4))).shape), (2, 4))

    def test_a_v3_model_samples_its_speed_alone_and_refuses_the_full_sample(self):
        with Bridge(reference.DEFAULT_BRIDGE) as bridge:
            model = NavigationModel(bridge.contract['schema'], contract=3)
        with torch.no_grad():
            model.actor.bias[3] = 2.
        x = tensor([{'inputs': observation()}, {'inputs': observation(goal=(50., 0.))}])
        with torch.no_grad():
            actor, _ = model(x)
            latent, log_prob, value = model.sample_speed(x)
        # The pair and the sense keep their means; only the speed is drawn.
        self.assertEqual(tuple(latent.shape), (2, 4)); self.assertEqual(tuple(log_prob.shape), (2,)); self.assertEqual(tuple(value.shape), (2,))
        self.assertTrue(torch.equal(latent[:, :2], actor[:, :2])); self.assertTrue(torch.equal(latent[:, 3], actor[:, 3]))
        self.assertEqual([r['advisory'] for r in command_dicts(latent, max_speed=60.)], [[1., 0., 0., 0.]] * 2)
        with self.assertRaises(ValueError):
            model.sample(x)
        with self.assertRaises(ValueError):
            model.entropy(actor)

    def test_export_parity_and_policy_steps(self):
        torch.set_num_threads(1); torch.manual_seed(0)
        with Bridge(reference.DEFAULT_BRIDGE) as bridge:
            self.assertEqual(bridge.contract['navigation_format_version'], FORMAT_VERSION)
            bridge.call('load_motor_policy', path=str(Path(reference.DEFAULT_MOTOR).resolve()))
            rows = fit.static_rows(reference.task_sets(bridge, 3110001, 1, 128))
            # One row per static family; the generator's admission order
            # among families is not part of the contract.
            self.assertEqual(sorted(r['family'] for r in rows), sorted(fit.STATIC_FAMILIES))
            model = NavigationModel(bridge.contract['schema'])
            with torch.no_grad():
                model.actor.bias[1] = 1.
            with tempfile.TemporaryDirectory() as tmp:
                path = Path(tmp) / 'policy.json'; model.export(path)
                bridge.call('load_navigation_policy', path=str(path))
                for row in rows:
                    item = bridge.call('reset', tasks=[row['task']])['items'][0]
                    native = bridge.call('infer_navigation', inputs=[item['inputs']])['outputs'][0]
                    with torch.no_grad():
                        actor = model(torch.tensor([fit.flatten(item['inputs'])]))[0]
                    torch.testing.assert_close(torch.tensor([native['latents']]), actor, atol=1e-5, rtol=1e-5)
                    expected = commands(actor)[0]
                    for key, value in zip(('heading_sin', 'heading_cos', 'speed'), expected.tolist()):
                        self.assertAlmostEqual(native['command'][key], value, places=4)
                    result = bridge.call('step_policy', hold=fit.HOLD)
                    self.assertEqual(result['ticks'], [fit.HOLD]); self.assertEqual(result['items'][0]['progress']['elapsed_ticks'], fit.HOLD)
                    self.assertAlmostEqual(result['commands'][0]['speed'], native['command']['speed'], places=4)


if __name__ == '__main__':
    unittest.main()
