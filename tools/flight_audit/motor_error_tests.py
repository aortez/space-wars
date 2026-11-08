"""LF-070 synthetic numerical contracts; no native simulation."""
import copy
from pathlib import Path
import tempfile
import unittest
import torch
from tools.flight_motor import aligned, sustained, storage
from tools.flight_ppo.tests import equal_tree
from . import motor_error_math as region
from . import motor_error_gradient as audit
from . import motor_gradient_math as math
from . import motor_feedback as feedback
from .motor_gradient_tests import fixture as old_fixture


def fixture():
    model, r, c = old_fixture()
    x = r['observations'].flatten(0, 1).clone(); x[:, 2] = torch.tensor([.8, 0]*24)
    x[:, 0] = 0; x[:, 1] = 1; x[:, 7] = 0
    x[:, 3:7] = 0
    speeds = torch.tensor([12, 0, 16, 0, 20, 0]*8)
    x = feedback.alter(x, x[:, 2]*20, speeds)
    x[24:, 7] = .2
    with torch.no_grad(): latent, z, logp, value = model.sample(x)
    r.update(observations=x.reshape(8, 6, 18), latent=latent.reshape(8, 6),
             actuator_latents=z.reshape(8, 6, 2), logp=logp.reshape(8, 6), returns=value.reshape(8, 6)+torch.randn(8, 6))
    return model, r, c


class ErrorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)

    def test_regions_partition_movement_and_keep_readiness_separate(self):
        _, r, _ = fixture(); x = r['observations'].flatten(0, 1)
        error, masks = region.regions(x)
        self.assertEqual([int(m.sum()) for m in masks.values()], [4]*6)
        self.assertTrue(torch.equal(torch.stack(list(masks.values())).sum(0), (x[:, 2]>0).long()))
        self.assertTrue((error[masks['ready_under']]< -1).all())
        y = x[:2].clone(); y[:, 2] = .8; y[:, 7] = 0
        y = feedback.alter(y, [16, 16], [15, 17])
        self.assertEqual(int(region.regions(y)[1]['ready_near'].sum()), 2)

    def test_shared_denominator_does_not_reweight_small_regions(self):
        _, r, _ = fixture(); x = r['observations'].flatten(0, 1)[:7]
        values = torch.arange(7., requires_grad=True)
        loss = sum(region.losses(values, x))
        torch.testing.assert_close(loss, -.5*values[x[:, 2]>0].mean())
        self.assertIsNone(math.cosine(torch.zeros(2), torch.ones(2)))

    def test_exact_region_replay_preserves_original_trainer_and_momentum(self):
        model, r, c = fixture(); opt = torch.optim.Adam(model.parameters(), lr=.0003, eps=1e-5)
        sustained.update(model, opt, r, c)
        other = copy.deepcopy(model); again = torch.optim.Adam(other.parameters(), lr=.0003, eps=1e-5)
        again.load_state_dict(copy.deepcopy(opt.state_dict())); rng = torch.get_rng_state().clone()
        expected = sustained.update(model, opt, r, c); expected_rng = torch.get_rng_state().clone()
        torch.set_rng_state(rng); actual, steps = audit.replay(other, again, r, c)
        self.assertEqual(actual, expected); self.assertTrue(equal_tree(model.state_dict(), other.state_dict()))
        self.assertTrue(equal_tree(opt.state_dict(), again.state_dict()))
        self.assertTrue(torch.equal(expected_rng, torch.get_rng_state()))
        self.assertEqual(tuple(steps[0]['allocation'].shape), (11, 3493))
        self.assertGreater(float(steps[0]['allocation'][-2].norm()), 0)

    def test_interval_reconstruction_cuts_completed_episode_and_reset(self):
        def item(code, tick, outcome=None):
            return {'inputs': [float(code)]+[0.]*17, 'errors': {'velocity': 2., 'heading': 0., 'spin': 0.},
                    'progress': {'elapsed_ticks': tick, 'outcome': outcome}}
        def trace(base, n, outcome=None):
            return {'initial': item(base, 0), 'steps': [{'item': item(base+t, t, outcome if t==n else None),
                    'action': {'thrust': .1, 'brake': .2}} for t in range(1, n+1)]}
        a, b = {'index': 0}, {'index': 1}
        before = {'collector': {'pools': [[a], [b]], 'active': [{'id': 1, 'ticks': 1}, {'id': 2, 'ticks': 0}], 'completed': []}}
        after = {'collector': {'completed': [{'id': 1, 'ticks': 2, 'candidate': a}],
            'active': [{'id': 3, 'ticks': 2, 'candidate': a}, {'id': 2, 'ticks': 3, 'candidate': b}],
            'traces': [trace(30, 2), trace(20, 3)]}}
        with tempfile.TemporaryDirectory() as d:
            p = Path(d); (p/'training-traces').mkdir()
            storage.dump_json(p/'training-traces/episode-000001.json.gz', trace(10, 2, 'collision'))
            evidence = region.reconstruct(p, before, after, 3)
        self.assertTrue(torch.equal(evidence['observations'][:, :, 0], torch.tensor([[11., 20.], [30., 21.], [31., 22.]])))
        self.assertEqual(evidence['dones'].tolist(), [[1., 0.], [0., 0.], [0., 0.]])
        self.assertEqual(float(evidence['terminal_rewards'][0, 0]), -5.)
        self.assertEqual(evidence['elapsed_ticks'][:, 0].tolist(), [2., 1., 2.])

    def test_region_full_gradients_sum_to_original_actor_gradient(self):
        model, r, _ = fixture(); _, params = math.parameters(model)
        f = {k: v.flatten(0, 1) for k, v in r.items()}; x = f['observations']; actor = model(x)[0]
        ratio = (model.log_prob(actor, f['latent'], f['actuator_latents'], x)-f['logp']).exp()
        adv = aligned.advantages(f['advantages'], x[:, 2]>0)
        expected = math.gradient(-.5*(ratio*adv)[x[:, 2]>0].mean(), params)
        torch.testing.assert_close(audit.full_gradients(model, r)['pooled'].sum(0), expected, atol=1e-6, rtol=1e-4)

    def test_independent_logit_credit_matches_local_force_difference(self):
        model, r, _ = fixture()
        names = ['rewards', 'tracking_rewards', 'terminal_rewards', 'elapsed_ticks', 'velocity_error_before', 'velocity_error_after', 'dones']
        credit = region.credit(model, r, {name: torch.zeros(8, 6) for name in names})
        x = r['observations'].flatten(0, 1)
        with torch.no_grad():
            actor = model(x)[0]; dist = model.distribution(actor, x)
            adv = aligned.advantages(r['advantages'].flatten(), x[:, 2]>0)
            direction = adv[:, None]*(r['actuator_latents'].flatten(0, 1)-actor[:, 1:])/dist.scale.square()
            lo = feedback.acceleration(x, (actor[:, 1:]-1e-3*direction).sigmoid())
            hi = feedback.acceleration(x, (actor[:, 1:]+1e-3*direction).sigmoid())
            expected = -region.regions(x)[0].sign()*(hi-lo)/.002
        torch.testing.assert_close(credit['fields']['independent_logit_correction'], expected, atol=.002, rtol=.01)

    def test_probe_jacobians_keep_zero_command_exact_and_missing_groups_explicit(self):
        model, r, _ = fixture()
        anchors = [{'inputs': r['observations'][0, 0].tolist()} for _ in range(54)]
        x, metadata = feedback.grid(anchors, [0, 16], [-6, -1, 0, 1, 6])
        result = audit.probes(model, x, metadata, r)
        self.assertEqual(tuple(result['jacobians'].shape), (29, 3493))
        for i, group in enumerate(result['groups']):
            if group['command']==0: self.assertFalse(result['jacobians'][i].count_nonzero())
        self.assertEqual(result['groups'][-1]['count'], 12)
        near = [g for g in result['groups'] if g['panel']=='saved_ready_near']
        self.assertTrue(all(g['count']==0 and g['force'] is None for g in near))


if __name__ == '__main__':
    unittest.main(verbosity=2)
