"""Aligned-task coordinates, readout isolation, likelihood and stage gates."""
import copy
import math
from pathlib import Path
import tempfile
import time
import unittest
import torch
from torch.distributions import Normal
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.tests import equal_tree
from .tests import BRIDGE
from .collector import Collector
from .model import tensor
from .audit import inspect,HEADING
from .velocity import reward
from .velocity_audit import task_matches
from .aligned_audit import gate
from .aligned_run import restore
from . import aligned

PARENT=Path('data/training-experiments/learned-flight-motor-turn-v1-20261025/pilot')


def make_model(seed=29):
    state=torch.load(PARENT/f'seed-{seed}/checkpoint-0064.pt',weights_only=True)
    return aligned.ReadoutModel(state['contract']['schema'])


class AlignedTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1);torch.use_deterministic_algorithms(True)

    def test_matrices_equal_exposure_and_native_alignment(self):
        with Bridge(BRIDGE) as bridge:
            for dev,seed in [(False,5910001),(True,5910002)]:
                rows=aligned.candidates(seed,dev);self.assertEqual(len(rows),104 if dev else 48)
                self.assertEqual(len({r['task']['start']['seed'] for r in rows}),len(rows))
                if not dev:
                    for slot in range(4):self.assertTrue(all((r['task']['speed']>0)==(slot%2==0) for r in rows[slot::4]))
                for row in rows:
                    item=bridge.call('reset',tasks=[row['task']])['items'][0]
                    if not row['suite'].startswith('aligned'):continue
                    self.assertLess(item['errors']['heading'],1e-5);self.assertAlmostEqual(item['errors']['spin'],0.,places=6)
                    self.assertAlmostEqual(item['inputs'][2],row['task']['speed']/20,places=6)
                    self.assertTrue(task_matches(bridge.call('state')['traces'][0]['task'],row['task']))
            self.assertEqual([r['index'] for r in aligned.training_candidates(aligned.config(17,'joint',True))],[0,1,40,33])

    def test_arm_contract_diff_is_only_stopping_actor_weight(self):
        one=aligned.config(17,'joint');two=aligned.config(17,'movement_only')
        self.assertEqual(one.pop('arm'),'joint');self.assertEqual(two.pop('arm'),'movement_only')
        self.assertEqual(one.pop('stopping_actor_weight'),1);self.assertEqual(two.pop('stopping_actor_weight'),0)
        self.assertEqual(one,two)
        with self.assertRaises(ValueError):aligned.config(17,'unlisted')

    def test_declared_initialization_and_exact_steering_function(self):
        for seed in aligned.SEEDS:
            m=make_model(seed);parent=torch.load(PARENT/f'seed-{seed}/checkpoint-0064.pt',weights_only=True)
            original=make_model(seed);original.load_state_dict(parent['model']);rng=torch.get_rng_state().clone()
            aligned.initialize(m,PARENT/f'seed-{seed}/checkpoint-0064.pt',aligned.config(seed,'joint'))
            self.assertTrue(torch.equal(rng,torch.get_rng_state()));aligned.assert_fixed_steering(parent['model'],m.state_dict())
            x=torch.linspace(-2,2,18*7).reshape(7,18);actor,value=m(x);old_actor,old_value=original(x)
            self.assertTrue(torch.equal(actor[:,0],old_actor[:,0]) and torch.equal(value,old_value))
            torch.testing.assert_close(actor[:,1:].sigmoid(),torch.tensor([[.25,.1]]*7),atol=1e-7,rtol=0)

    def test_actuator_likelihood_excludes_deterministic_steering(self):
        m=make_model();x=torch.zeros(4,18);latent,z,logp,value=m.sample(x);actor,_=m(x)
        self.assertTrue(torch.equal(latent,actor[:,0]));manual=(Normal(actor[:,1:],m.log_std[1:].exp()).log_prob(z)-m.actuator_log_jacobian(z)).sum(-1)
        torch.testing.assert_close(logp,manual,rtol=0,atol=0)
        changed=actor.clone();changed[:,0]+=100
        self.assertTrue(torch.equal(m.log_prob(actor,latent,z),m.log_prob(changed,latent,z)))

    def test_group_advantages_do_not_depend_on_other_command_group(self):
        mask=torch.tensor([True,False]*8);raw=torch.arange(16,dtype=torch.float32);one=aligned.advantages(raw,mask)
        changed=raw.clone();changed[~mask]=torch.tensor([10.,-8.,4.,2.,-7.,-100.,19.,200.]);two=aligned.advantages(changed,mask)
        self.assertTrue(torch.equal(one[mask],two[mask]));self.assertFalse(torch.equal(one[~mask],two[~mask]))
        for group in [mask,~mask]:self.assertAlmostEqual(float(one[group].mean()),0,places=6)
        with self.assertRaises(ValueError):aligned.advantages(raw,torch.ones(16,dtype=torch.bool))

    def test_loss_keeps_movement_coefficient_and_removes_stopping_gradient(self):
        ratio=torch.tensor([1.1,.9,1.,1.],requires_grad=True);adv=torch.tensor([1.,-1.,.5,-.5]);mask=torch.tensor([True,True,False,False]);entropy=torch.zeros(4)
        gradients=[]
        for arm in aligned.ARMS:
            loss=aligned.actor_losses(ratio,adv,entropy,mask,aligned.config(17,arm))[0]
            gradients.append(torch.autograd.grad(loss,ratio)[0])
        self.assertTrue(torch.equal(gradients[0][:2],gradients[1][:2]))
        self.assertGreater(float(gradients[0][2:].abs().sum()),0);self.assertEqual(gradients[1][2:].count_nonzero(),0)

    def test_optimizer_cannot_change_steering_and_ignores_stopping_advantages(self):
        torch.manual_seed(29);original=make_model();aligned.initialize(original,PARENT/'seed-29/checkpoint-0064.pt',aligned.config(29,'joint'))
        x=torch.randn(256,4,18);x[:,:,2]=torch.tensor([.2,0.,.8,0.]);rows=[]
        with torch.no_grad():
            for state in x:
                latent,z,lp,v=original.sample(state);rows.append({'observations':state,'latent':latent,'actuator_latents':z,'logp':lp,'returns':v+.3,'advantages':torch.randn(4)})
        rollout={k:torch.stack([r[k] for r in rows]) for k in rows[0]};changed=copy.deepcopy(rollout);changed['advantages'][:,1::2]=torch.randn(256,2)*9
        rng=torch.get_rng_state().clone();outputs=[]
        for arm,data in [('movement_only',rollout),('movement_only',changed),('joint',changed)]:
            m=make_model();m.load_state_dict(original.state_dict());optimizer=torch.optim.Adam(m.parameters(),lr=.0003,eps=1e-5)
            torch.set_rng_state(rng);metrics=aligned.update(m,optimizer,data,aligned.config(29,arm));self.assertGreater(metrics['minibatches'],0)
            aligned.assert_fixed_steering(original.state_dict(),m.state_dict());self.assertFalse(equal_tree(original.state_dict(),m.state_dict()));outputs.append(copy.deepcopy(m.state_dict()))
        self.assertTrue(equal_tree(outputs[0],outputs[1]));self.assertFalse(equal_tree(outputs[1],outputs[2]))

    def test_collector_uses_velocity_reward_and_fixed_slot_groups(self):
        with tempfile.TemporaryDirectory() as directory,Bridge(BRIDGE) as bridge:
            c=aligned.config(29,'movement_only',True);m=make_model();aligned.initialize(m,PARENT/'seed-29/checkpoint-0064.pt',c)
            collector=Collector(bridge,aligned.training_candidates(c),Path(directory)/'traces');rollout=collector.collect(m,c,time.monotonic()+60)
            self.assertEqual(int((rollout['observations'][:,:,2]>0).sum()),512)
            for e,t in zip(collector.active,collector.state()['traces']):
                check=inspect(t,reward);self.assertAlmostEqual(check['reward'],e['reward'],places=9)
            flat={k:v.flatten(0,1) for k,v in rollout.items()}
            with torch.no_grad():actor=m(flat['observations'])[0]
            torch.testing.assert_close(m.log_prob(actor,flat['latent'],flat['actuator_latents']),flat['logp'],atol=2e-5,rtol=1e-5)

    def test_gate_requires_movement_gain_and_original_heading_retention(self):
        final=[{**r,'retained':True,'outcome':'success','first_success_tick':30 if r['family']=='hold' else 100} for r in aligned.candidates(5910002,True)]
        initial=copy.deepcopy(final)
        for r in initial:
            if r['suite']=='aligned_tracking':r.update(retained=False,outcome='deadline')
        parent=[r for r in final if r['suite'].startswith('legacy')];self.assertTrue(gate(initial,final,parent)['passed'])
        broken=copy.deepcopy(final);next(r for r in broken if r['suite']=='legacy_heading')['retained']=False
        self.assertTrue(gate(initial,broken,parent)['movement_learning_passed']);self.assertFalse(gate(initial,broken,parent)['passed'])
        self.assertFalse(gate(final,final,parent)['movement_learning_passed'])

    def test_resume_rejects_changed_arm_before_restoring_world(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'state.pt';torch.save({'contract':{'config':aligned.config(17,'joint')}},p)
            with self.assertRaisesRegex(ValueError,'mismatch'):restore(p,{'config':aligned.config(17,'movement_only')},None,Path(d)/'unused')
            self.assertFalse((Path(d)/'unused').exists())


if __name__=='__main__':unittest.main(verbosity=2)
