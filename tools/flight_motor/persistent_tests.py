"""Conditional likelihood, residual lifecycle and matched motor comparison checks."""
import copy
from pathlib import Path
import random
import subprocess
import sys
import tempfile
import time
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from . import persistent, velocity
from .model import MotorModel
from .collector import Collector
from .tests import BRIDGE
from .run import identity, restore

PARENT=Path('data/training-experiments/learned-flight-motor-turn-v1-20261025/pilot')


class PersistentTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1);torch.use_deterministic_algorithms(True)

    def test_only_declared_exploration_changes_between_main_arms(self):
        for seed in [17,29,43]:
            control=persistent.config(seed,'control');experimental=persistent.config(seed,'persistent')
            self.assertEqual(experimental.pop('motor_exploration'),persistent.SPEC)
            self.assertEqual(experimental.pop('arm'),'persistent');self.assertEqual(control.pop('arm'),'control')
            self.assertEqual(control,experimental)
            self.assertEqual(persistent.training_candidates(control),velocity.candidates(5510001))
            pre=persistent.training_candidates(persistent.config(seed,'persistent',True))
            self.assertEqual([r['index'] for r in pre],[0,24,40,56])
            self.assertEqual([r['task']['speed'] for r in pre],[0.,4.,10.,16.])
        with self.assertRaises(ValueError):persistent.config(17,'unlisted')

    def setup_distribution(self):
        with Bridge(BRIDGE) as b:model=MotorModel(b.contract['schema'])
        actor=torch.tensor([[.2,-3.,.4]]*3,requires_grad=True)
        previous=torch.tensor([[-.1,-2.,.7]]*3,requires_grad=True)
        last=torch.tensor([[.8,1.,-2.]]*3);x=torch.zeros(3,18);x[:2,2]=.5;valid=torch.tensor([1.,0.,1.])
        dist=persistent.distribution(model,actor,previous,last,valid,x,persistent.SPEC)
        return model,actor,previous,last,x,valid,dist

    def test_conditional_center_variance_and_stationary_start(self):
        model,actor,previous,last,x,valid,dist=self.setup_distribution()
        rho=torch.tensor([[0.,.95,.95],[0.,0.,0.],[0.,0.,0.]])
        multiplier=torch.tensor([[1.,8.,8.],[1.,8.,8.],[1.,1.,1.]])
        torch.testing.assert_close(dist.loc,actor+rho*(last-previous),rtol=0,atol=0)
        torch.testing.assert_close(dist.scale,model.log_std.exp()*multiplier*(1-rho.square()).sqrt(),rtol=0,atol=0)
        bad=copy.deepcopy(persistent.SPEC);bad['movement_multiplier']=9
        with self.assertRaises(ValueError):persistent.distribution(model,actor,previous,last,valid,x,bad)

    def test_transformed_density_and_gradient_through_previous_policy_mean(self):
        model,actor,previous,last,x,valid,dist=self.setup_distribution()
        sample=torch.tensor([[.3,2.,-1.],[.1,30.,-30.],[-.2,-2.,.1]])
        actual=persistent.log_prob(model,dist,sample)
        manual=dist.log_prob(sample).sum(-1)-torch.log1p(-sample[:,0].tanh().square())
        manual-=(-torch.nn.functional.softplus(-sample[:,1:])-torch.nn.functional.softplus(sample[:,1:])).sum(-1)
        torch.testing.assert_close(actual,manual,atol=1e-5,rtol=1e-6)
        (-actual.sum()).backward();rho=torch.tensor([[0.,.95,.95],[0.,0.,0.],[0.,0.,0.]])
        torch.testing.assert_close(previous.grad,-rho*actor.grad,rtol=0,atol=0)
        self.assertTrue(torch.isfinite(model.log_std.grad).all());self.assertGreater(float(model.log_std.grad.abs().sum()),0)
        self.assertTrue(torch.isfinite(persistent.entropy(model,dist)).all())

    def test_zero_command_has_original_independent_likelihood(self):
        model,actor,previous,last,x,valid,_=self.setup_distribution();x[:,2]=0
        dist=persistent.distribution(model,actor,previous,last,valid,x,persistent.SPEC)
        sample=torch.tensor([[.3,2.,-1.],[.1,3.,-3.],[-.2,-2.,.1]])
        torch.testing.assert_close(persistent.log_prob(model,dist,sample),model.log_prob(actor,sample[:,0],sample[:,1:]),atol=1e-5,rtol=1e-6)

    def test_reset_erases_residual_and_invalid_state_is_rejected(self):
        state=persistent.initial_state(4);x=torch.ones(4,18);latent=torch.ones(4,3)
        persistent.advance(state,x,latent,[False,True,False,True])
        active=[{'ticks':1},{'ticks':0},{'ticks':9},{'ticks':0}];persistent.validate_state(state,active)
        self.assertEqual(state['previous_valid'].tolist(),[1.,0.,1.,0.])
        self.assertEqual(state['previous_latents'][1].count_nonzero(),0)
        bad=copy.deepcopy(state);bad['previous_observations'][1,0]=1
        with self.assertRaisesRegex(ValueError,'leaked'):persistent.validate_state(bad,active)
        bad=copy.deepcopy(state);bad['previous_valid'][0]=0
        with self.assertRaisesRegex(ValueError,'cursor'):persistent.validate_state(bad,active)

    def test_real_collector_log_prob_recomputes_before_update_and_resets(self):
        with tempfile.TemporaryDirectory() as d,Bridge(BRIDGE) as b:
            c=persistent.config(29,'persistent',True);model=MotorModel(b.contract['schema'])
            velocity.initialize(model,PARENT/'seed-29/checkpoint-0064.pt',c)
            collector=Collector(b,persistent.training_candidates(c),Path(d)/'traces')
            for _ in range(2):
                rollout=collector.collect(model,c,time.monotonic()+60);flat={k:v.flatten(0,1) for k,v in rollout.items()}
                with torch.no_grad():
                    actor=model(flat['observations'])[0];previous=model(flat['ar_previous_observations'])[0]
                    dist=persistent.distribution(model,actor,previous,flat['ar_previous_latents'],flat['ar_previous_valid'],flat['observations'],c['motor_exploration'])
                    current=persistent.log_prob(model,dist,flat['ar_latents'])
                torch.testing.assert_close(current,flat['logp'],rtol=0,atol=2e-5)
                self.assertEqual(set(flat['observations'][:,2].tolist()),{0.,float(torch.tensor(.2)),.5,float(torch.tensor(.8))})
            self.assertGreaterEqual(len(collector.completed),4)
            persistent.validate_state(collector.exploration,collector.active)

    def test_both_preflight_arms_resume_exactly_and_cannot_cross_contracts(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)
            for arm in persistent.ARMS:
                c=persistent.config(17,arm,True);atomic_json(p/f'{arm}.json',c)
                command=[sys.executable,'-m','tools.flight_motor.run','--bridge',BRIDGE,'--config',str(p/f'{arm}.json'),'--skip-evaluation']
                subprocess.run(command+['--out',str(p/arm),'--initialize-from',str(PARENT/'seed-17/checkpoint-0064.pt')],check=True,capture_output=True,timeout=90)
                subprocess.run(command+['--out',str(p/f'{arm}-again'),'--resume',str(p/arm/'checkpoint-0001.pt')],check=True,capture_output=True,timeout=90)
                for name in ['checkpoint-0002.pt','rollout-0002.pt']:
                    self.assertTrue(equal_tree(torch.load(p/arm/name,weights_only=True),torch.load(p/f'{arm}-again'/name,weights_only=True)))
            with Bridge(BRIDGE) as b:
                with self.assertRaisesRegex(ValueError,'mismatch'):restore(p/'control/checkpoint-0002.pt',identity(persistent.config(17,'persistent',True),b),b,p/'unused')
            self.assertFalse((p/'unused').exists())
            zero=torch.load(p/'persistent/checkpoint-0000.pt',weights_only=True)
            self.assertEqual(zero['collector']['exploration']['previous_valid'].count_nonzero(),0)
            self.assertEqual(zero['optimizer']['state'],{})


if __name__=='__main__':unittest.main(verbosity=2)
