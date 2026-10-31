"""LF-064 return accounting, common noise, native replay and terminal handling."""
import copy
import math
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import patch

import torch
from torch.distributions import Normal
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.ppo import gae
from tools.flight_motor import storage
from . import motor_action_return as probe


def item(tick, speed=9., command=6., outcome=None):
    x = [0., 1., command / 20, 0., speed / 20, 0., speed / 20, 0., 0., 0., 1., 1., 1., 1., 1., 0., .5, .5]
    return {'inputs': x, 'errors': {'heading': 0., 'spin': 0., 'velocity': abs(speed-command)},
            'progress': {'elapsed_ticks': tick, 'outcome': outcome}}


def trace(n=3, terminal=None):
    return {'initial': item(0), 'steps': [{'action': {'steering': 0., 'thrust': .5, 'brake': .5},
             'item': item(t, 9-t, outcome=terminal if t == n else None)} for t in range(1, n+1)]}


class ConstantModel:
    def __call__(self, x): return torch.zeros(len(x), 3), torch.zeros(len(x))
    def distribution(self, actor, x): return Normal(actor[:, 1:], torch.ones(len(x), 2))


class FakeBridge:
    def call(self, op, **kw):
        if op == 'restore':
            self.traces = [copy.deepcopy(t) for t in kw['traces']]
            return {'items': [t['steps'][-1]['item'] if t['steps'] else t['initial'] for t in self.traces]}
        if op == 'infer':
            return {'outputs': [{'steering_latent': 0., 'thrust_logit': 0., 'brake_logit': 0., 'value': 0.,
                                 'action': {'steering': 0., 'thrust': .5, 'brake': .5}} for _ in kw['inputs']]}
        if op == 'step':
            for t, action in zip(self.traces, kw['actions']):
                previous = t['steps'][-1]['item'] if t['steps'] else t['initial']
                assert previous['progress']['outcome'] is None
                tick = len(t['steps']) + 1
                terminal = 'collision' if tick == 1 and action['thrust'] > .5 else None
                t['steps'].append({'action': action, 'item': item(tick, 9-tick, outcome=terminal)})
            return {'items': [t['steps'][-1]['item'] for t in self.traces]}
        if op == 'state': return {'traces': copy.deepcopy(self.traces)}
        raise ValueError(op)


class ActionReturnTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)

    def test_prefix_rejects_terminal_and_preserves_original(self):
        t = trace(3, 'collision'); saved = copy.deepcopy(t)
        self.assertEqual(len(probe.prefix(t, 2)['steps']), 2)
        with self.assertRaises(ValueError): probe.prefix(t, 3)
        with self.assertRaises(ValueError): probe.prefix(t, -1)
        self.assertEqual(t, saved)

    def test_command_projection_and_lateral_readiness(self):
        i = item(0); self.assertAlmostEqual(probe.speed_errors(i)[0], 3.)
        self.assertEqual(probe.speed_errors(i)[1], 0.)
        i['inputs'][0:2] = [1., 0.]; i['inputs'][3:5] = [-.45, .1]
        self.assertAlmostEqual(probe.speed_errors(i)[0], 3.)
        self.assertAlmostEqual(probe.speed_errors(i)[1], 2.)

    def test_common_noise_and_exact_intervention_duration(self):
        actor = torch.zeros(9, 3); sigma = torch.ones(9, 2) * .7
        noise = torch.tensor([[.3, -.2]]).expand(9, 2)
        saved = actor.clone(); base = sigma * noise
        for step in [0, 1, 29, 30, 119]:
            z = probe.perturb(actor, sigma, noise, probe.VARIANTS, step)
            for index, v in enumerate(probe.VARIANTS):
                expected = base[index].clone()
                if step < v['duration']: expected[v['channel']-1] += v['offset']
                self.assertTrue(torch.equal(z[index], expected))
        self.assertTrue(torch.equal(actor, saved))

    def test_credits_match_production_gae_and_discount(self):
        t = trace(3); values = [-.5, -.4, -.3, -.2]
        with patch.object(probe, 'HORIZONS', [1, 2, 3]): rows = probe.credits(t, 0, values)
        rewards = torch.tensor([[probe.reward(s['item'])['total']] for s in t['steps']])
        expected = gae(rewards, torch.zeros(3, 1), torch.tensor(values[:-1])[:, None], torch.tensor([values[-1]]), .99, .95)[0]
        self.assertEqual(rows[-1]['gae_first'], float(expected[0, 0]))
        self.assertAlmostEqual(rows[-1]['observed_return'], sum(.99**i*probe.reward(s['item'])['total'] for i, s in enumerate(t['steps'])))
        self.assertAlmostEqual(rows[-1]['bootstrap'], .99**3*values[-1])
        self.assertAlmostEqual(rows[0]['bootstrapped_return']-values[0], rows[0]['td_first'])

    def test_terminal_rewards_are_not_repeated_or_bootstrapped(self):
        t = trace(1, 'collision')
        rows = probe.credits(t, 0, [0., 900.])
        for row in rows:
            self.assertEqual(row['performed_ticks'], 1); self.assertEqual(row['bootstrap'], 0.)
            self.assertLess(row['observed_return'], -5.)
            self.assertEqual(row['observed_return'], rows[0]['observed_return'])
        self.assertAlmostEqual(rows[0]['gae_first'], rows[0]['observed_return'], places=6)

    def test_immediate_reward_and_critic_ranking_can_disagree(self):
        low = trace(1); high = copy.deepcopy(low); high['steps'][0]['item'] = item(1, 6.)
        with patch.object(probe, 'HORIZONS', [1]):
            a = probe.credits(low, 0, [0., 0.])[0]
            b = probe.credits(high, 0, [0., -1.])[0]
        self.assertGreater(b['observed_return'], a['observed_return'])
        self.assertLess(b['gae_first'], a['gae_first'])

    def test_batch_removes_terminals_and_counts_restoration(self):
        counts = {k: 0 for k in ['matched_initial_states', 'restore_ticks', 'neutral_setups',
                                  'probe_ticks', 'native_python_comparisons']}
        with patch.object(probe, 'HORIZONS', [1, 2, 3]):
            rows = probe.batch(FakeBridge(), ConstantModel(), trace(0), None, counts, time.monotonic()+10)
        self.assertEqual(len(rows), 9)
        self.assertEqual(sum(len(r['trace']['steps']) for r in rows), counts['probe_ticks'])
        self.assertEqual(counts['probe_ticks'], 23)
        self.assertEqual(counts['restore_ticks'], 7)
        self.assertEqual(counts['neutral_setups'], 16)
        self.assertEqual(sum(r['metrics'][-1]['terminal'] for r in rows), 2)

    def test_anchor_selection_is_earliest_ready_and_shared(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            for arm in probe.ARMS:
                for seed in probe.SEEDS:
                    folder = root / f'{arm}/seed-{seed}'; folder.mkdir(parents=True)
                    for base, speed in zip([0, 16, 32], [6, 12, 18]):
                        for bearing in [0, 2]:
                            for offset, factor in [(0, 0.), (4, .5), (8, 1.5)]:
                                t = {'initial': item(0, speed*factor, speed), 'steps': []}
                                if offset == 8:
                                    bad = item(1, speed, speed); bad['errors']['spin'] = .3
                                    good = item(2, speed+.1, speed)
                                    t['steps'] = [{'item': bad}, {'item': good}]
                                storage.dump_json(folder / f'case-{base+bearing+offset:02}.json.gz', t)
            anchors, _ = probe.select_anchors(root)
            self.assertEqual(len(anchors), 18)
            self.assertEqual([a['tick'] for a in anchors], [0, 2, 0]*6)
            self.assertTrue(all(a['source_seed'] == 17 and a['source_arm'] == 'residual' for a in anchors))

    def test_native_prefix_and_continuation_replay(self):
        archive = Path('data/training-experiments/learned-flight-motor-movement-v1-20261025')
        original = storage.read_json(archive / 'pilot/residual/seed-29/evaluation-final/case-20.json.gz')
        with Bridge(archive / 'runtime/flight_motor_bridge') as bridge:
            bridge.call('load_policy', path=str((archive / 'pilot/residual/seed-29/policy-0128.json').resolve()))
            bridge.call('reset', tasks=[original['task']])
            for _ in range(2): bridge.call('step_policy')
            first = bridge.call('state')['traces'][0]
            bridge.call('restore', traces=[first])
            for _ in range(2): bridge.call('step_policy')
            final = bridge.call('state')['traces'][0]
            self.assertEqual(final, probe.prefix(original, 4))
            bridge.call('restore', traces=[final])
            atomic_json(Path('/tmp/motor-action-return-native-test.json'),
                        {'new_ticks': 4, 'replay_ticks': 6, 'neutral_setups': 3, 'trace': final})


if __name__ == '__main__': unittest.main()
