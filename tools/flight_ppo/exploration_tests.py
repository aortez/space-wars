"""Conditional density, temporal statistics and exact exploration restart."""
import json
import math
from pathlib import Path
import unittest
import torch
from torch.distributions import Normal
from . import exploration as ar
from .bridge import Bridge
from .collector import Collector
from .curriculum import generate
from .model import FlightModel
from . import tests

BRIDGE = tests.BRIDGE


class ExplorationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        cls.bridge = Bridge(BRIDGE)
        cls.schema = cls.bridge.contract["schema"]
        cls.pools = generate(cls.bridge, 4110001, 1)[1]

    @classmethod
    def tearDownClass(cls):
        cls.bridge.close()

    def test_independent_latent_marginals_and_nominal_runtime(self):
        model = FlightModel(self.schema)
        actor = torch.tensor([[.4, -.8, 1.2], [-.3, 0., 0.]])
        mean = ar.means(actor)
        torch.testing.assert_close(mean, actor)
        self.assertTrue(torch.equal(mean[:, 1:] >= 0, actor[:, 1:] >= 0))
        dist = ar.distribution(model, actor, actor, mean, torch.ones(2), .95)
        torch.testing.assert_close(dist.loc, mean)
        # Zero residual yields the same continuous controls as Rust evaluation.
        torch.testing.assert_close(dist.loc[:, 0].tanh(), actor[:, 0].tanh())

    def test_conditional_density_gradients_and_extreme_logits(self):
        model = FlightModel(self.schema)
        actor = torch.tensor([[.4, -.8, 1.2]], requires_grad=True)
        previous = torch.tensor([[.1, .3, -.2]], requires_grad=True)
        past = torch.tensor([[.7, -.4, 1.]])
        latent = torch.tensor([[.5, .6, -.2]])
        dist = ar.distribution(model, actor, previous, past, torch.ones(1), .95)
        center = ar.means(actor) + .95*(past-ar.means(previous))
        scale = model.log_std.exp() * math.sqrt(1-.95**2)
        expected = Normal(center, scale).log_prob(latent).sum(-1)-model.log_jacobian(latent[:, 0])-model.actuator_log_jacobian(latent[:,1:]).sum(-1)
        torch.testing.assert_close(ar.log_prob(model, dist, latent), expected)
        ar.log_prob(model, dist, latent).sum().backward()
        self.assertAlmostEqual(float(previous.grad[0,0]), -.95*float(actor.grad[0,0]), places=5)
        extreme = torch.tensor([[0., -80., 80.]], requires_grad=True)
        dist = ar.distribution(model, extreme, previous.detach(), past, torch.ones(1), .95)
        loss = ar.log_prob(model, dist, latent).sum()+ar.entropy(model, dist).sum()
        loss.backward()
        self.assertTrue(torch.isfinite(loss))
        self.assertTrue(torch.isfinite(extreme.grad).all())

    def test_stationary_latent_marginals_correlation_and_sign_persistence(self):
        stats = {}
        for rho in [0., .95]:
            torch.manual_seed(91)
            z = torch.randn(1024)
            samples=[]
            for _ in range(256):
                z = rho*z + math.sqrt(1-rho*rho)*torch.randn(1024)
                samples.append(z)
            x=torch.stack(samples)
            self.assertLess(abs(float(x.mean())), .03)
            self.assertLess(abs(float(x.var())-1), .04)
            correlation=float((x[:-1]*x[1:]).mean()/x.var())
            self.assertLess(abs(correlation-rho), .02)
            sign=x>=0
            fraction=float(sign.float().mean())
            self.assertLess(abs(fraction-.5), .02)
            stats[rho]=float((sign[1:]!=sign[:-1]).float().mean())
        self.assertLess(stats[.95], stats[0.]/3)

    def test_terminal_reset_and_rollout_density_reconstruction(self):
        config=json.loads((Path(__file__).parent/'clear-control-config.json').read_text())
        config.update(rollout_ticks=8, exploration={"kind":ar.KIND,"rho":.95})
        collector=Collector(self.bridge,self.pools)
        model=FlightModel(self.schema)
        rollout=collector.collect(model,config)
        flat={k:v.flatten(0,1) for k,v in rollout.items()}
        with torch.no_grad():
            actor=model(flat['observations'])[0]
            previous=model(flat['ar_previous_observations'])[0]
            dist=ar.distribution(model,actor,previous,flat['ar_previous_latents'],flat['ar_previous_valid'],.95)
            torch.testing.assert_close(ar.log_prob(model,dist,flat['ar_latents']),flat['logp'],atol=3e-6,rtol=3e-6)
        state=collector.exploration_state
        self.assertTrue(torch.equal(state['previous_valid'],torch.ones(4)))
        ar.advance(state,torch.ones(4,4009),torch.ones(4,3),[True,False,True,False])
        self.assertTrue(torch.equal(state['previous_valid'],torch.tensor([0.,1.,0.,1.])))
        self.assertEqual(float(state['previous_latents'][0].abs().sum()),0.)
        self.assertEqual(float(state['previous_observations'][2].abs().sum()),0.)
        actor=torch.randn(4,3)
        dist=ar.distribution(model,actor,torch.randn(4,3),state['previous_latents'],state['previous_valid'],.95)
        torch.testing.assert_close(dist.loc[0],ar.means(actor)[0])
        self.assertAlmostEqual(float(dist.scale[0,1].detach()),float(model.log_std[1].exp().detach()))

    def test_independent_and_persistent_fresh_process_resume(self):
        for rho in [0.,.95]:
            with self.subTest(rho=rho):
                config=json.loads((Path(__file__).parent/'clear-control-config.json').read_text())
                config.update(updates=2,rollout_ticks=8,epochs=2,minibatch=16,train_tasks_per_stratum=1,
                              exploration={"kind":ar.KIND,"rho":rho})
                tests.PolicyTests.check_continuation(self,config)


if __name__ == '__main__':
    unittest.main(verbosity=2)
