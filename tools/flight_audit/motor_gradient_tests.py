"""LF-068 numerical contracts; synthetic evidence, no native simulation."""
import copy
import unittest
import torch
from tools.flight_motor import aligned, sustained
from tools.flight_ppo.tests import equal_tree
from . import motor_gradient as audit
from . import motor_gradient_math as math
from . import motor_feedback as feedback
from .motor_feedback_tests import inputs


def fixture():
    torch.manual_seed(68)
    model = sustained.ResidualModel({'version': 1, 'features': list(range(18))})
    with torch.no_grad(): model.residual.weight.uniform_(-.1, .1)
    x = inputs()[:1].repeat(48, 1)
    x[:, 2] = torch.tensor([.2, 0, .5, 0, .8, 0] * 8)
    with torch.no_grad():
        latent, z, logp, value = model.sample(x)
    data = {'observations': x.reshape(8, 6, 18), 'latent': latent.reshape(8, 6),
            'actuator_latents': z.reshape(8, 6, 2), 'logp': logp.reshape(8, 6),
            'advantages': torch.randn(8, 6), 'returns': value.reshape(8, 6) + torch.randn(8, 6)}
    c = {'epochs': 2, 'minibatch': 12, 'clip': .2, 'value_weight': .5,
         'entropy_weight': .001, 'grad_clip': .5, 'target_kl': .03, 'stopping_actor_weight': 1.}
    return model, data, c


class GradientTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)

    def test_unequal_commands_keep_pooled_denominator(self):
        x = torch.zeros(5, 18); x[:, 2] = torch.tensor([.2, .2, .5, .8, 0])
        s = torch.tensor([1., 2, 3, 4, 100], requires_grad=True)
        pieces = math.command_losses(s, x)
        torch.testing.assert_close(torch.stack(pieces), torch.tensor([-.375, -.375, -.5]))
        self.assertEqual(float(sum(pieces).detach()), -1.25)

    def test_missing_command_and_zero_cosine_are_not_agreement(self):
        x = torch.zeros(4, 18); x[:, 2] = .2
        s = torch.ones(4, requires_grad=True)
        parts = math.command_losses(s, x)
        self.assertEqual(float(parts[1].detach()), 0)
        self.assertIsNone(math.cosine(torch.zeros(2), torch.ones(2)))
        self.assertAlmostEqual(math.cosine(torch.ones(2), -torch.ones(2)), -1.)

    def test_own_centering_preserves_common_scale(self):
        _, r, _ = fixture(); x = r['observations'].flatten(0, 1)
        raw = torch.arange(48.).reshape(8, 6).flatten()
        v = math.normalized_variants(raw, x)
        torch.testing.assert_close(v['pooled'], aligned.advantages(raw, x[:, 2] > 0))
        std = raw[x[:, 2] > 0].std(unbiased=False)
        for mask in math.masks_by_command(x):
            torch.testing.assert_close(v['own_center_common_scale'][mask], (raw[mask] - raw[mask].mean()) / std)

    def test_exact_replay_matches_unmodified_trainer_with_momentum(self):
        model, r, c = fixture(); optimizer = torch.optim.Adam(model.parameters(), lr=.0003, eps=1e-5)
        sustained.update(model, optimizer, r, c)
        other = copy.deepcopy(model); opt = torch.optim.Adam(other.parameters(), lr=.0003, eps=1e-5)
        opt.load_state_dict(copy.deepcopy(optimizer.state_dict()))
        rng = torch.get_rng_state().clone()
        expected = sustained.update(model, optimizer, r, c); expected_rng = torch.get_rng_state().clone()
        torch.set_rng_state(rng); actual, steps = audit.replay(other, opt, r, c)
        self.assertEqual(actual, expected)
        self.assertTrue(equal_tree(model.state_dict(), other.state_dict()))
        self.assertTrue(equal_tree(optimizer.state_dict(), opt.state_dict()))
        self.assertTrue(torch.equal(expected_rng, torch.get_rng_state()))
        self.assertGreater(float(steps[0]['allocation'][-2].norm()), 0)
        self.assertTrue(all(not s['gradients'][3].count_nonzero() for s in steps))

    def test_fresh_adam_allocation_matches_known_two_component_update(self):
        p = torch.nn.Parameter(torch.tensor([1., -2.])); opt = torch.optim.Adam([p], lr=.01, eps=1e-5)
        account = math.AdamAccounting(opt, [p]); g = torch.zeros(6, 2)
        g[0] = torch.tensor([2., -1.]); g[1] = torch.tensor([-1., 3.])
        before = p.detach().clone(); p.grad = g.sum(0); opt.step()
        allocated, _ = account.allocate(g, 1., p.detach() - before)
        torch.testing.assert_close(allocated.sum(0), (p.detach()-before).double(), atol=1e-12, rtol=1e-10)
        self.assertLess(float(allocated[0, 0]), 0)
        self.assertGreater(float(allocated[1, 0]), 0)
        self.assertEqual(float(allocated[-2].norm()), 0)

    def test_force_jacobian_matches_parameter_finite_difference(self):
        model, _, _ = fixture(); _, params = math.parameters(model)
        x = inputs(); value = feedback.acceleration(x, model(x)[0][:, 1:].sigmoid()).mean()
        jac = math.gradient(value, params)
        direction = torch.randn_like(jac); direction /= direction.norm()
        saved = [p.detach().clone() for p in params]
        def displaced(scale):
            offset = 0
            with torch.no_grad():
                for p, original in zip(params, saved):
                    p.copy_(original + scale * direction[offset:offset+p.numel()].reshape(p.shape)); offset += p.numel()
                return feedback.acceleration(x, model(x)[0][:, 1:].sigmoid()).mean()
        finite = (displaced(.01) - displaced(-.01)) / .02
        torch.testing.assert_close(finite, jac @ direction, atol=2e-4, rtol=.01)

    def test_zero_command_parameter_jacobian_and_distribution_protection(self):
        model, _, _ = fixture(); _, params = math.parameters(model); x = inputs(); x[:, 2] = 0
        actor, _ = model(x); dist = model.distribution(actor, x)
        jac = math.gradient(feedback.acceleration(x, actor[:, 1:].sigmoid()).sum(), params)
        self.assertEqual(int(jac.count_nonzero()), 0)
        with torch.no_grad():
            for n, p in model.named_parameters():
                if n.startswith('residual'): p.add_(torch.randn_like(p))
        new, _ = model(x)
        self.assertTrue(torch.equal(actor, new))
        self.assertTrue(torch.equal(dist.scale, model.distribution(new, x).scale))

    def test_sampling_reconstructs_rng_without_physics(self):
        model, r, _ = fixture(); rng = torch.get_rng_state().clone()
        with torch.no_grad(): original = [model.sample(x)[:3] for x in r['observations']]
        expected = torch.get_rng_state().clone(); torch.set_rng_state(rng)
        with torch.no_grad(): replay = [model.sample(x)[:3] for x in r['observations']]
        self.assertTrue(equal_tree(original, replay)); self.assertTrue(torch.equal(expected, torch.get_rng_state()))


if __name__ == '__main__':
    unittest.main(verbosity=2)
