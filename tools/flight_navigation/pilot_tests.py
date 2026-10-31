"""LF-084 likelihoods, decision rewards, the macro-step collector on a fake bridge, and the law anchor."""
import math
import random
import unittest

import torch
from torch.distributions import Normal, SigmoidTransform, TanhTransform, TransformedDistribution
from . import pilot
from .fit_tests import FakeBridge, observation
from .model import MAX_SPEED, NavigationModel, command_dicts
from .reference_tests import reference  # noqa: F401  The reference module must import cleanly for the labeller.

SCHEMA = {'version': 2, 'action_contract': 'continuous-flight-v2', 'radial_edges': [10. * i for i in range(24)]}


def item(distance, outcome=None, ticks=0):
    row = {'inputs': observation((0., distance)), 'progress': {'outcome': outcome, 'elapsed_ticks': ticks},
           'frame': {'errors': {'position': distance, 'velocity': 0., 'spin': 0.}}}
    return row


class StepBridge(FakeBridge):
    """Two slots: slot 0 ends in success at its second decision, slot 1 runs on; distances shrink by ten per decision."""

    def __init__(self):
        super().__init__(); self.distances = [50., 80.]; self.steps = [0, 0]

    def call(self, op, **kw):
        self.calls.append((op, kw))
        if op == 'reset':
            return {'items': [item(d) for d in self.distances]}
        if op == 'reset_at':
            self.steps[kw['index']] = 0; self.distances[kw['index']] = 50.
            return {'items': [item(d) for d in self.distances]}
        if op == 'step_command':
            items = []; ticks = []
            for i in range(2):
                self.steps[i] += 1; self.distances[i] -= 10.
                ended = i == 0 and self.steps[i] == 2
                items.append(item(self.distances[i], 'success' if ended else None, 6 * self.steps[i] - (2 if ended else 0)))
                ticks.append(4 if ended else 6)
            return {'items': items, 'ticks': ticks}
        raise ValueError(op)


class PilotTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.manual_seed(0)

    def test_log_probability_matches_the_squashed_transformed_distributions(self):
        model = NavigationModel(SCHEMA)
        actor = torch.tensor([[.2, .8, -.7], [-.4, -.1, .2]]); latent = torch.tensor([[.7, -1.3, 1.], [-.3, .4, -.7]])
        std = model.log_std.exp(); scale = actor[:, 2:3].sigmoid().clamp_min(.1)
        pair = TransformedDistribution(Normal(actor[:, :2], std[:2] * scale), [TanhTransform(cache_size=1)])
        speed = TransformedDistribution(Normal(actor[:, 2], std[2]), [SigmoidTransform()])
        expected = pair.log_prob(latent[:, :2].tanh()).sum(-1) + speed.log_prob(latent[:, 2].sigmoid())
        torch.testing.assert_close(model.log_prob(actor, latent), expected, atol=1e-6, rtol=1e-6)
        # Heading exploration follows the commanded speed: full when cruising, a tenth when stopping, and the speed deviation is unchanged.
        stopping, cruising = model.distributions(torch.tensor([[0., 3., -30.], [0., 3., 30.]])).stddev
        torch.testing.assert_close(stopping, torch.cat((std[:2] * .1, std[2:]))); torch.testing.assert_close(cruising, std)
        self.assertTrue(torch.isfinite(model.log_prob(actor, torch.tensor([[-40., 40., 80.], [40., -40., -80.]]))).all())
        self.assertTrue(torch.isfinite(model.entropy(actor)).all())
        x = torch.rand(3, 4009)
        with torch.no_grad():
            z, logp, value = model.sample(x)
        self.assertEqual(tuple(z.shape), (3, 3)); self.assertEqual(tuple(logp.shape), (3,)); self.assertEqual(tuple(value.shape), (3,))

    def test_speed_only_sampling_keeps_the_heading_at_its_mean(self):
        model = NavigationModel(SCHEMA); x = torch.rand(4, 4009)
        with torch.no_grad():
            actor, value = model(x); latent, logp, sampled_value = model.sample_speed(x)
        torch.testing.assert_close(latent[:, :2], actor[:, :2]); torch.testing.assert_close(sampled_value, value)
        self.assertFalse(torch.equal(latent[:, 2], actor[:, 2]))
        speed = TransformedDistribution(Normal(actor[:, 2], model.log_std.exp()[2]), [SigmoidTransform()])
        torch.testing.assert_close(logp, speed.log_prob(latent[:, 2].sigmoid()), atol=1e-6, rtol=1e-6)
        self.assertTrue(torch.isfinite(model.speed_entropy(actor)).all())
        c = pilot.config(17); self.assertIs(pilot.policy_terms(c), pilot.speed_terms)
        self.assertIs(pilot.policy_terms(pilot.config(17, heading_sampler='gaussian')), pilot.navigation_terms)
        with self.assertRaises(ValueError):
            pilot.config(17, heading_sampler='sticky')

    def test_command_dicts_keep_direction_and_floor_the_pair_norm(self):
        rows = command_dicts(torch.tensor([[0., 0., 0.], [.01, -.02, 30.], [3., 0., -30.]]))
        self.assertEqual(rows[0]['heading_sin'], 0.); self.assertAlmostEqual(rows[0]['heading_cos'], .125); self.assertAlmostEqual(rows[0]['speed'], 10.)
        self.assertAlmostEqual(math.hypot(rows[1]['heading_sin'], rows[1]['heading_cos']), .125); self.assertAlmostEqual(math.atan2(rows[1]['heading_sin'], rows[1]['heading_cos']), math.atan2(math.tanh(.01), math.tanh(-.02)))
        self.assertAlmostEqual(rows[1]['speed'], MAX_SPEED, places=4); self.assertGreater(rows[2]['heading_sin'], .99); self.assertLess(rows[2]['speed'], 1e-6)

    def test_decision_reward_discounts_within_the_hold_and_shaping_telescopes(self):
        spec = pilot.config(17)['reward']; g = .9
        r = pilot.decision_reward(item(50.), item(40.), 6, g, spec)
        self.assertAlmostEqual(r['time'], -.001 * sum(g ** t for t in range(6))); self.assertEqual(r['terminal'], 0.)
        self.assertAlmostEqual(r['potential'], g ** 6 * -math.tanh(40. / 60.) + math.tanh(50. / 60.))
        done = pilot.decision_reward(item(10.), item(2., 'success'), 4, g, spec)
        self.assertAlmostEqual(done['terminal'], 10. * g ** 3); self.assertAlmostEqual(done['potential'], math.tanh(10. / 60.))
        crash = pilot.decision_reward(item(30.), item(25., 'collision'), 6, g, spec)
        self.assertAlmostEqual(crash['terminal'], -10. * g ** 5)
        # Shaping over two held decisions telescopes to the discounted end potential minus the start potential.
        a = pilot.decision_reward(item(50.), item(40.), 6, g, spec)['potential']; b = pilot.decision_reward(item(40.), item(35.), 6, g, spec)['potential']
        self.assertAlmostEqual(a + g ** 6 * b, g ** 12 * -math.tanh(35. / 60.) + math.tanh(50. / 60.))
        with self.assertRaises(RuntimeError):
            pilot.decision_reward(item(1.), item(1., 'environment_error'), 1, g, spec)

    def test_collector_macro_steps_discount_by_ticks_reset_on_terminals_and_label_by_rate(self):
        c = pilot.config(17, anchor=1., label_rate=1.); model = NavigationModel(SCHEMA)
        with torch.no_grad():
            model.actor.bias[1] = 1.
        bridge = StepBridge(); labeller = lambda inputs: [0., 1., 5.]
        collector = pilot.NavigationCollector(bridge, [[{'family': 'clear', 'task': {}}], [{'family': 'altered', 'task': {}}]], c, labeller, random.Random(1))
        rollout = collector.collect(model, 3, c['lambda'])
        self.assertEqual(tuple(rollout['observations'].shape), (3, 2, 4009)); self.assertEqual(tuple(rollout['latent'].shape), (3, 2, 3))
        self.assertEqual(collector.outcomes, {'clear/success': 1}); self.assertEqual(collector.episodes, 1); self.assertEqual(collector.physical_failures, 0)
        self.assertEqual([op for op, _ in bridge.calls], ['reset', 'step_command', 'step_command', 'reset_at', 'step_command'])
        self.assertEqual(collector.ticks, 6 + 6 + 4 + 6 + 6 + 6); self.assertEqual(pilot.window_summary(collector.window)['mean_success_ticks'], 10.)
        self.assertTrue((rollout['labelled'] == 1.).all()); self.assertEqual(rollout['labels'][0, 0].tolist(), [0., 1., 5.])
        # Slot 0's second decision is terminal with four ticks: its advantage is reward minus value, with no bootstrap.
        g = c['gamma']; spec = c['reward']
        values = model(rollout['observations'][1])[1].detach()
        terminal_reward = pilot.decision_reward(item(40.), item(30., 'success'), 4, g, spec)['total']
        self.assertAlmostEqual(rollout['advantages'][1, 0].item(), terminal_reward - values[0].item(), places=4)
        # Slot 1 never ends: its last advantage bootstraps on the final value with the six-tick discount.
        final_value = model(torch.tensor([pilot.fit.flatten(observation((0., 50.)))]))[1].item()
        last = pilot.decision_reward(item(60.), item(50.), 6, g, spec)['total']
        self.assertAlmostEqual(rollout['advantages'][2, 1].item(), last + g ** 6 * final_value - values[1].item() * 0 - model(rollout['observations'][2])[1][1].item(), places=3)
        unlabelled = pilot.NavigationCollector(StepBridge(), [[{'family': 'clear', 'task': {}}], [{'family': 'altered', 'task': {}}]], pilot.config(17), None)
        self.assertEqual(unlabelled.collect(model, 2, 1.)['labelled'].sum().item(), 0.)

    def test_law_anchor_uses_labelled_rows_only_and_policy_terms_shapes(self):
        c = pilot.config(29, anchor=2., anchor_terms='command'); model = NavigationModel(SCHEMA)
        actor = torch.tensor([[0., 20., 30.], [0., 20., 30.], [20., 0., -30.]])
        flat = {'labels': torch.tensor([[0., 1., 20.], [1., 0., 0.], [0., 1., 20.]]), 'labelled': torch.tensor([1., 0., 1.]), 'latent': torch.zeros(3, 3)}
        # The update passes the actor rows of the minibatch, so the actor is indexed like the flat rollout.
        exact = pilot.law_anchor(model, actor[[0, 1]], flat, torch.tensor([0, 1]), c)
        self.assertLess(exact.item(), 1e-6)
        wrong = pilot.law_anchor(model, actor[[1, 2]], flat, torch.tensor([1, 2]), c)
        self.assertGreater(wrong.item(), 2.)
        self.assertEqual(pilot.law_anchor(model, actor[[1]], flat, torch.tensor([1]), c).item(), 0.)
        # The heading-only anchor ignores a wrong speed and charges a wrong heading.
        heading = pilot.config(29, anchor=2.)
        self.assertLess(pilot.law_anchor(model, torch.tensor([[0., 20., -30.]]), flat, torch.tensor([0]), heading).item(), 1e-6)
        self.assertGreater(pilot.law_anchor(model, actor[[2]], flat, torch.tensor([2]), heading).item(), 2.)
        logp, entropy = pilot.navigation_terms(model, actor[[0, 2]], flat, torch.tensor([0, 2]), c)
        self.assertEqual(tuple(logp.shape), (2,)); self.assertEqual(entropy.ndim, 0)
        with self.assertRaises(ValueError):
            pilot.config(5)
        self.assertEqual(pilot.config(17)['label_rate'], 0.); self.assertEqual(pilot.config(17, anchor=1.)['label_rate'], .25)
        self.assertEqual([len(p) for p in pilot.pools(list(range(35)), 16)], [3, 3, 3] + [2] * 13)


if __name__ == '__main__':
    unittest.main()
