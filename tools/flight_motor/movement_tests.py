"""Matched movement initialization, protected stopping and native format 3."""
import copy
from pathlib import Path
import tempfile
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.motor_retention import parity
from . import movement,sustained,storage
from .model import MotorModel
from .movement_run import restore
from .tests import BRIDGE

PARENT=Path('data/training-experiments/learned-flight-motor-turn-v1-20261025/pilot')
OLD_BRIDGE='data/training-experiments/learned-flight-motor-sustained-v2-20261025/runtime/flight_motor_bridge'


def make(seed,arm):
    c=movement.config(seed,arm);p=PARENT/f'seed-{seed}/checkpoint-0064.pt';state=torch.load(p,weights_only=True)
    m=movement.make_model(state['contract']['schema'],c);movement.initialize(m,p,c)
    return m


class MovementTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1);torch.use_deterministic_algorithms(True)

    def test_matched_configs_tasks_and_budget(self):
        a=movement.config(29,'residual');b=movement.config(29,'movement')
        for k in ['arm','movement_actor_mode','movement_initialization','actor_scope','actuator_initialization']:a.pop(k);b.pop(k)
        self.assertEqual(a,b);self.assertEqual(a['movement_retention_ticks'],900)
        for pre in [False,True]:self.assertEqual(movement.training_candidates(movement.config(29,'residual',pre)),movement.training_candidates(movement.config(29,'movement',pre)))

    def test_initial_outputs_values_distribution_and_rng_match_exactly(self):
        for seed in movement.SEEDS:
            torch.manual_seed(seed);a=make(seed,'residual');rng=torch.get_rng_state().clone()
            torch.manual_seed(seed);b=make(seed,'movement');self.assertTrue(torch.equal(rng,torch.get_rng_state()))
            x=torch.randn(1024,18);x[:,2]=torch.arange(1024).remainder(21)/20
            with torch.no_grad():
                av,ac=a(x);bv,bc=b(x)
                self.assertTrue(torch.equal(av,bv));self.assertTrue(torch.equal(ac,bc))
                self.assertTrue(torch.equal(a.distribution(av,x).scale,b.distribution(bv,x).scale))
            self.assertEqual(sum(p.numel() for p in b.parameters()),5259)
            self.assertEqual(sum(p.numel() for p in b.parameters() if p.requires_grad),3493)

    def test_native_initial_equivalence_and_old_residual_compatibility(self):
        with tempfile.TemporaryDirectory() as d,Bridge(BRIDGE) as bridge,Bridge(OLD_BRIDGE) as old:
            for seed in movement.SEEDS:
                torch.manual_seed(seed);a=make(seed,'residual');torch.manual_seed(seed);b=make(seed,'movement')
                x=torch.randn(80,18);x[:,2]=torch.tensor([0.,.2,.6,.9]*20)
                pa=Path(d)/f'a-{seed}.json';pb=Path(d)/f'b-{seed}.json';a.export(pa);b.export(pb)
                bridge.call('load_policy',path=str(pa));old.call('load_policy',path=str(pa));original=bridge.call('infer',inputs=x.tolist())['outputs']
                self.assertEqual(original,old.call('infer',inputs=x.tolist())['outputs'])
                bridge.call('load_policy',path=str(pb));self.assertEqual(original,bridge.call('infer',inputs=x.tolist())['outputs']);parity(bridge,b,x.tolist())

    def test_native_trained_movement_path_and_zero_speed_protection(self):
        with tempfile.TemporaryDirectory() as d,Bridge(BRIDGE) as bridge:
            m=make(43,'movement');base=copy.deepcopy(m)
            with torch.no_grad():
                for name,p in m.named_parameters():
                    if name.startswith('residual'):p.add_(torch.randn_like(p)*.1)
            x=torch.randn(80,18);x[:,2]=torch.tensor([0.,.2,.6,.9]*20)
            self.assertTrue(torch.equal(m(x)[0][::4],base(x)[0][::4]))
            path=Path(d)/'changed.json';m.export(path);bridge.call('load_policy',path=str(path));parity(bridge,m,x.tolist())

    def test_real_ppo_update_preserves_original_stopping(self):
        torch.manual_seed(43);m=make(43,'movement');original=copy.deepcopy(m.state_dict());x=torch.randn(256,4,18);x[:,:,2]=torch.tensor([.2,0.,.8,0.]);rows=[]
        with torch.no_grad():
            for state in x:
                latent,z,lp,value=m.sample(state);rows.append({'observations':state,'latent':latent,'actuator_latents':z,'logp':lp,'returns':value+.5,'advantages':torch.randn(4)})
        rollout={k:torch.stack([r[k] for r in rows]) for k in rows[0]}
        optimizer=torch.optim.Adam(m.parameters(),lr=.0003,eps=1e-5);movement.update(m,optimizer,rollout,movement.config(43,'movement'))
        self.assertFalse(equal_tree(original,m.state_dict()));sustained.assert_parent_actor(original,m.state_dict())
        states=torch.randn(1024,18);states[:,2]=0
        self.assertTrue(torch.equal(m(states)[0],MotorModel.forward(m,states)[0]))

    def test_movement_path_has_gradient_and_stopping_has_none(self):
        m=make(29,'movement');x=torch.randn(8,18);x[:,2]=.9
        m(x)[0][:,1:].sum().backward();self.assertGreater(int(m.residual1.weight.grad[:,18:].count_nonzero()),0)
        m.zero_grad(set_to_none=True);x[:,2]=0;m(x)[0][:,1:].sum().backward()
        for name,p in m.named_parameters():
            if name.startswith('residual') and p.grad is not None:self.assertEqual(int(p.grad.count_nonzero()),0)

    def test_cross_arm_resume_rejected_before_environment_restore(self):
        with tempfile.TemporaryDirectory() as d:
            path=Path(d)/'state.pt.gz';storage.dump_torch(path,{'contract':{'config':movement.config(17,'residual')}})
            with self.assertRaisesRegex(ValueError,'mismatch'):restore(path,{'config':movement.config(17,'movement')},None,Path(d)/'unused')
            self.assertFalse((Path(d)/'unused').exists())


if __name__=='__main__':unittest.main(verbosity=2)
