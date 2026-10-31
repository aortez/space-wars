"""LF-065 lambda isolation, real collector target routing and stopping invariance."""
import copy
from pathlib import Path
import tempfile
import time
import unittest
import torch
from tools.flight_ppo.ppo import gae
from tools.flight_ppo.tests import equal_tree
from . import gae_comparison as comparison
from . import movement, sustained, storage
from .collector import Collector
from .gae_audit import discounted_targets
from .model import MotorModel
from .movement_run import restore, identity

PARENT=Path('data/training-experiments/learned-flight-motor-turn-v1-20261025/pilot')


def make(seed, arm):
    c=comparison.config(seed,arm); path=PARENT/f'seed-{seed}/checkpoint-0064.pt'
    state=storage.load_torch(path); model=comparison.make_model(state['contract']['schema'],c)
    comparison.initialize(model,path,c)
    return model


class SyntheticBridge:
    """No physics: expose the collector's reward selection and sampling sequence."""
    def items(self):
        rows=[]
        for speed,previous in zip(self.speeds,self.previous):
            current=speed-2 if speed else 2
            x=[0.,1.,speed/20,0.,current/20,0.,current/20,0.,0.,0.,1.,1.,1.,1.,1.,*previous]
            rows.append({'inputs':x,'errors':{'heading':0.,'spin':0.,'velocity':2.},
                         'progress':{'outcome':None}})
        return rows

    def call(self, op, **kwargs):
        if op=='reset':
            self.speeds=[t['speed'] for t in kwargs['tasks']];self.previous=[[0.,0.,0.] for _ in self.speeds]
        elif op=='step':
            self.previous=[[a[k] for k in ['steering','thrust','brake']] for a in kwargs['actions']]
        else:raise ValueError(op)
        return {'items':self.items()}


class GaeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1);torch.use_deterministic_algorithms(True)

    def test_only_lambda_changes_and_original_control_recipe_matches(self):
        for seed in comparison.SEEDS:
            for pre in [False,True]:
                a=comparison.config(seed,'lambda095',pre);b=comparison.config(seed,'lambda1',pre)
                self.assertEqual(a.pop('lambda'),.95);self.assertEqual(b.pop('lambda'),1.)
                a.pop('arm');b.pop('arm');self.assertEqual(a,b)
                old=movement.config(seed,'residual',pre);new=comparison.config(seed,'lambda095',pre)
                for key in ['kind','arm']:old.pop(key);new.pop(key)
                for key in ['movement_actor_mode','movement_initialization']:old.pop(key)
                self.assertEqual(old,new)
                self.assertEqual(comparison.training_candidates(comparison.config(seed,'lambda095',pre)),
                                 comparison.training_candidates(comparison.config(seed,'lambda1',pre)))

    def test_same_initial_weights_rng_and_sampling_contract(self):
        for seed in comparison.SEEDS:
            torch.manual_seed(seed);a=make(seed,'lambda095');rng=torch.get_rng_state().clone()
            torch.manual_seed(seed);b=make(seed,'lambda1')
            self.assertTrue(equal_tree(a.state_dict(),b.state_dict()))
            self.assertTrue(torch.equal(rng,torch.get_rng_state()))
            self.assertEqual(sum(p.numel() for p in b.parameters()),5259)
            self.assertEqual(sum(p.numel() for p in b.parameters() if p.requires_grad),3493)
            self.assertEqual(comparison.architecture({}),sustained.ARCHITECTURE)

    def test_lambda_one_cancels_intermediate_values(self):
        torch.manual_seed(65);r=torch.randn(256,4);d=(torch.rand(256,4)<.04).float()
        v=torch.randn(256,4)*5;last=torch.randn(4)
        a,target=gae(r,d,v,last,.99,1.);expected=discounted_targets(r,d,last,.99)
        torch.testing.assert_close(target,expected,atol=2e-5,rtol=1e-5)
        _,changed=gae(r,d,v*3,last,.99,1.)
        torch.testing.assert_close(changed,expected,atol=3e-5,rtol=1e-5)
        self.assertFalse(torch.allclose(gae(r,d,v,last,.99,.95)[1],expected))

    def test_terminal_cuts_reset_and_last_bootstrap(self):
        r=torch.tensor([[1.],[2.],[100.]])
        d=torch.tensor([[0.],[1.],[0.]]);last=torch.tensor([10.])
        expected=discounted_targets(r,d,last,.5)
        self.assertTrue(torch.equal(expected,torch.tensor([[2.],[2.],[105.]])))
        target=gae(r,d,torch.tensor([[3.],[4.],[5.]]),last,.5,1.)[1]
        self.assertTrue(torch.equal(target,expected))

    def test_lambda095_original_recurrence(self):
        r=torch.tensor([[1.],[-2.],[3.]]);d=torch.tensor([[0.],[1.],[0.]])
        v=torch.tensor([[.1],[.2],[.3]]);last=torch.tensor([.4])
        adv,_=gae(r,d,v,last,.99,.95)
        expected=torch.tensor([[1+.99*.2-.1+.99*.95*(-2-.2)],[-2-.2],[3+.99*.4-.3]])
        torch.testing.assert_close(adv,expected)

    def collect_pair(self, directory):
        out=[]
        for arm in comparison.ARMS:
            torch.manual_seed(43);model=make(43,arm);c=comparison.config(43,arm,True)
            collector=Collector(SyntheticBridge(),comparison.training_candidates(c),Path(directory)/arm)
            rollout=collector.collect(model,c,time.monotonic()+30)
            out.append((model,collector,rollout,c))
        return out

    def test_collector_changes_targets_only_and_uses_velocity_reward(self):
        with tempfile.TemporaryDirectory() as d:
            pair=self.collect_pair(d);one=pair[0][2];two=pair[1][2]
            for key in ['observations','latent','actuator_latents','logp']:
                self.assertTrue(torch.equal(one[key],two[key]))
            self.assertFalse(torch.equal(one['advantages'],two['advantages']))
            self.assertFalse(torch.equal(one['returns'],two['returns']))
            model,collector,rollout,c=pair[1]
            with torch.no_grad():last=model(torch.tensor([i['inputs'] for i in collector.items]))[1]
            r=torch.full((256,4),-(2/5)**2/60)
            expected=discounted_targets(r,torch.zeros_like(r),last,c['gamma'])
            torch.testing.assert_close(rollout['returns'],expected,atol=1e-5,rtol=1e-5)

    def test_real_updates_preserve_stopping_with_both_target_recipes(self):
        with tempfile.TemporaryDirectory() as d:
            for model,collector,rollout,c in self.collect_pair(d):
                before=copy.deepcopy(model.state_dict())
                optimizer=torch.optim.Adam(model.parameters(),lr=c['learning_rate'],eps=1e-5)
                result=comparison.update(model,optimizer,rollout,c)
                self.assertGreater(result['minibatches'],0);self.assertFalse(equal_tree(before,model.state_dict()))
                comparison.assert_parent_actor(before,model.state_dict())
                x=torch.randn(1024,18);x[:,2]=0
                with torch.no_grad():
                    self.assertTrue(torch.equal(model(x)[0],MotorModel.forward(model,x)[0]))
                    self.assertTrue(torch.equal(model.distribution(model(x)[0],x).scale,
                                                model.log_std[1:].clamp(-5,2).exp().expand(1024,2)))

    def test_cross_lambda_resume_rejected_before_restore(self):
        with tempfile.TemporaryDirectory() as d:
            path=Path(d)/'state.pt.gz';storage.dump_torch(path,{'contract':{'config':comparison.config(17,'lambda095')}})
            with self.assertRaisesRegex(ValueError,'mismatch'):
                restore(path,{'config':comparison.config(17,'lambda1')},None,Path(d)/'unused',comparison)
            self.assertFalse((Path(d)/'unused').exists())

    def test_undeclared_arms_and_seeds_rejected(self):
        with self.assertRaises(ValueError):comparison.config(17,'lambda099')
        with self.assertRaises(ValueError):comparison.config(99,'lambda1')


if __name__=='__main__':unittest.main(verbosity=2)
