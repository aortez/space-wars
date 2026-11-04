"""Protected stopping, retained training, native inference and lossless evidence."""
import copy
import math
from pathlib import Path
import tempfile
import time
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.motor_retention import parity,inspect as inspect_retention
from .tests import BRIDGE
from .model import MotorModel
from .audit import inspect as inspect_training
from .velocity import reward
from .sustained_run import CompressedCollector,restore
from . import sustained,storage

PARENT=Path('data/training-experiments/learned-flight-motor-turn-v1-20261025/pilot')
OLD_BRIDGE='data/training-experiments/learned-flight-motor-aligned-v1-20261025/runtime/flight_motor_bridge'


def make(seed=29):
    state=torch.load(PARENT/f'seed-{seed}/checkpoint-0064.pt',weights_only=True)
    m=sustained.ResidualModel(state['contract']['schema']);sustained.initialize(m,PARENT/f'seed-{seed}/checkpoint-0064.pt',sustained.config(seed,'sustain'))
    base=MotorModel(m.schema);base.load_state_dict(state['model'])
    return m,base


class SustainedTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1);torch.use_deterministic_algorithms(True)

    def test_matched_arms_only_change_movement_horizon(self):
        a=sustained.config(17,'acquire');b=sustained.config(17,'sustain')
        a.pop('arm');b.pop('arm');self.assertIsNone(a.pop('movement_retention_ticks'));self.assertEqual(b.pop('movement_retention_ticks'),900);self.assertEqual(a,b)
        rows=sustained.training_candidates(sustained.config(17,'sustain'));old=sustained.training_candidates(sustained.config(17,'acquire'))
        for i,(r,o) in enumerate(zip(rows,old)):
            r=copy.deepcopy(r)
            if i%2==0:self.assertEqual(r['task'].pop('retention_horizon_ticks'),900)
            self.assertEqual(r,o)
        self.assertEqual([r['index'] for r in sustained.training_candidates(sustained.config(17,'sustain',True))],[0,1,40,33])

    def test_velocity_error_features_and_smooth_gate(self):
        x=torch.zeros(3,18);x[0,:5]=torch.tensor([.6,.8,.5,.2,-.1]);x[1,:5]=torch.tensor([0.,1.,.2,0.,.2]);x[2,2]=0
        torch.testing.assert_close(sustained.residual_features(x)[:,-2:],torch.tensor([[-.5,.5],[0.,0.],[0.,0.]]))
        torch.testing.assert_close(sustained.gate(x),torch.tensor([10/11,4/5,0.]))
        x[0,2]=-1
        with self.assertRaises(ValueError):sustained.gate(x)

    def test_initialization_preserves_all_parent_actor_weights(self):
        for seed in sustained.SEEDS:
            m,base=make(seed);sustained.assert_parent_actor(base.state_dict(),m.state_dict())
            self.assertEqual(sum(p.numel() for p in m.parameters()),5259);self.assertEqual(sum(p.numel() for p in m.parameters() if p.requires_grad),3493)
            x=torch.randn(8,18);x[:,2]=0
            self.assertTrue(torch.equal(m(x)[0],base(x)[0]));self.assertTrue(torch.equal(m(x)[1],base(x)[1]))
            self.assertTrue(torch.equal(m.distribution(m(x)[0],x).scale,base.log_std[1:].clamp(-5,2).exp().expand(8,2)))
            x[:,2]=.5;actual=m(x)[0];original=base(x)[0]
            self.assertTrue(torch.equal(actual[:,0],original[:,0]))
            torch.testing.assert_close(actual[:,1:]-original[:,1:],torch.tensor([[20/11,-20/11]]*8))

    def test_optimizer_changes_movement_without_changing_zero_speed_outputs(self):
        torch.manual_seed(43);m,base=make(43);x=torch.randn(256,4,18);x[:,:,2]=torch.tensor([.2,0.,.8,0.]);rows=[]
        with torch.no_grad():
            for state in x:
                latent,z,lp,v=m.sample(state);rows.append({'observations':state,'latent':latent,'actuator_latents':z,'logp':lp,'returns':v+.5,'advantages':torch.randn(4)})
        rollout={k:torch.stack([r[k] for r in rows]) for k in rows[0]};before=copy.deepcopy(m.state_dict())
        optimizer=torch.optim.Adam(m.parameters(),lr=.0003,eps=1e-5);metrics=sustained.update(m,optimizer,rollout,sustained.config(43,'sustain'))
        self.assertGreater(metrics['minibatches'],0);self.assertFalse(equal_tree(before,m.state_dict()));sustained.assert_parent_actor(base.state_dict(),m.state_dict())
        states=torch.randn(1024,18);states[:,2]=0
        self.assertTrue(torch.equal(m(states)[0],base(states)[0]))
        self.assertTrue(torch.equal(m.distribution(m(states)[0],states).scale,base.log_std[1:].clamp(-5,2).exp().expand(1024,2)))

    def test_stopping_loss_has_zero_residual_gradient(self):
        m,_=make();x=torch.randn(8,18);x[:,2]=0;actor,value=m(x);z=torch.ones(8,2)
        loss=-m.log_prob(actor,actor[:,0],z,x).mean()-.001*m.entropy(actor,x).mean();loss.backward()
        for name,p in m.named_parameters():
            if name.startswith('residual'):self.assertEqual(p.grad.count_nonzero(),0,name)

    def test_native_new_model_and_legacy_backward_compatibility(self):
        with tempfile.TemporaryDirectory() as d,Bridge(BRIDGE) as b,Bridge(OLD_BRIDGE) as old:
            m,base=make();x=torch.randn(12,18);x[:,2]=torch.tensor([0.,.2,.5,.8]*3)
            # Nonconstant residual exercises both hidden layers and derived features.
            with torch.no_grad():m.residual.weight.uniform_(-.2,.2)
            path=Path(d)/'model.json';m.export(path);b.call('load_policy',path=str(path.resolve()));parity(b,m,x.tolist())
            output=b.call('infer',inputs=x.tolist())['outputs'];parent=PARENT/'seed-29/policy-0064.json'
            b.call('load_policy',path=str(parent.resolve()));old.call('load_policy',path=str(parent.resolve()))
            legacy=b.call('infer',inputs=x.tolist())['outputs'];self.assertEqual(legacy,old.call('infer',inputs=x.tolist())['outputs'])
            for i in [0,4,8]:self.assertEqual(output[i]['action'],legacy[i]['action'])
            for i in range(12):self.assertEqual(output[i]['steering_latent'],legacy[i]['steering_latent'])

    def test_retention_training_delays_success_reward_until_full_horizon(self):
        task=copy.deepcopy(sustained.training_candidates(sustained.config(29,'sustain'))[0]['task'])
        h=task['start']['bearing']+task['start']['heading_offset'];task['start']['relative_velocity']={'x':-math.sin(h)*4,'y':math.cos(h)*4}
        with Bridge(BRIDGE) as b:
            b.call('reset',tasks=[task]);zero={'steering':0.,'thrust':0.,'brake':0.}
            for tick in range(1,901):
                item=b.call('step',actions=[zero])['items'][0]
                if tick<900:self.assertIsNone(item['progress']['outcome']);self.assertEqual(reward(item)['terminal'],0.)
                if tick==30:self.assertEqual(item['retention']['first_success_tick'],30)
            self.assertEqual(item['progress']['outcome'],'success');self.assertEqual(reward(item)['terminal'],1.)
            self.assertTrue(inspect_retention(b.call('state')['traces'][0])['retained'])

    def test_compressed_evidence_roundtrip_and_no_overwrite(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d);obj={'tensor':torch.arange(17),'nested':[(1,2),{'value':.25}],'rng':torch.get_rng_state()}
            storage.dump_torch(p/'state.pt.gz',obj);self.assertTrue(equal_tree(obj,storage.load_torch(p/'state.pt.gz')))
            storage.dump_json(p/'trace.json.gz',{'x':[1,2.,None]});self.assertEqual(storage.read_json(p/'trace.json.gz'),{'x':[1,2.,None]})
            with self.assertRaises(FileExistsError):storage.dump_torch(p/'state.pt.gz',{})

    def test_collector_keeps_sustained_movement_alive_across_rollouts(self):
        with tempfile.TemporaryDirectory() as d,Bridge(BRIDGE) as b:
            c=sustained.config(29,'sustain',True);m,_=make();collector=CompressedCollector(b,sustained.training_candidates(c),Path(d)/'traces')
            for _ in range(4):
                rollout=collector.collect(m,c,time.monotonic()+60);self.assertEqual(int((rollout['observations'][:,:,2]>0).sum()),512)
                flat={k:v.flatten(0,1) for k,v in rollout.items()}
                with torch.no_grad():actor=m(flat['observations'])[0];lp=m.log_prob(actor,flat['latent'],flat['actuator_latents'],flat['observations'])
                torch.testing.assert_close(lp,flat['logp'],atol=2e-5,rtol=1e-5)
            movement=[e for e in collector.completed if e['candidate']['task']['speed']>0];self.assertEqual(len(movement),2)
            for e in collector.completed:
                t=storage.read_json(collector.directory/f"episode-{e['id']:06}.json.gz")
                if e['candidate']['task']['speed']>0:self.assertEqual(e['ticks'],900);inspect_retention(t)
                else:inspect_training(t,reward)
                self.assertAlmostEqual(e['reward'],sum(reward(s['item'])['total'] for s in t['steps']),places=9)

    def test_cross_arm_checkpoint_resume_rejected_before_restore(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'state.pt.gz';storage.dump_torch(p,{'contract':{'config':sustained.config(17,'acquire')}})
            with self.assertRaisesRegex(ValueError,'mismatch'):restore(p,{'config':sustained.config(17,'sustain')},None,Path(d)/'unused')
            self.assertFalse((Path(d)/'unused').exists())


if __name__=='__main__':unittest.main(verbosity=2)
