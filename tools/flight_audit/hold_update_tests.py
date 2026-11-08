"""Synthetic checks; no trained policy, environment replay or optimizer continuation."""
import copy
import unittest
import torch
from torch import nn
from tools.flight_ppo import exploration
from tools.flight_ppo.model import FlightModel
from tools.flight_ppo.ppo import update
from tools.flight_ppo.reward import reward
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.signal import inspect_gradients
from tools.flight_audit.hold_update import VARIANTS, action_metrics, labels, probe, projections, row_data


class TinyModel(nn.Module):
    log_jacobian=staticmethod(FlightModel.log_jacobian)
    actuator_log_jacobian=staticmethod(FlightModel.actuator_log_jacobian)
    def __init__(self):
        super().__init__();self.shared=nn.Linear(4,4);self.actor=nn.Linear(4,3);self.critic=nn.Linear(4,1)
        self.log_std=nn.Parameter(torch.full((3,),-.5))
    def forward(self,x):
        h=self.shared(x).tanh();return self.actor(h),self.critic(h).squeeze(-1)


class HoldUpdateTests(unittest.TestCase):
    def setUp(self):torch.set_num_threads(1);torch.manual_seed(101)
    def setup_rollout(self):
        model=TinyModel();x=torch.randn(8,2,4)
        with torch.no_grad():
            actor,value=model(x.flatten(0,1));dist=exploration.distribution(model,actor,actor,torch.zeros_like(actor),torch.zeros(16),0.)
            z=dist.sample();lp=exploration.log_prob(model,dist,z)
        advantage=torch.linspace(-1,1,16).reshape(8,2)
        rollout={'observations':x,'ar_previous_observations':torch.zeros_like(x),
                 'ar_previous_latents':torch.zeros(8,2,3),'ar_previous_valid':torch.zeros(8,2),
                 'ar_latents':z.reshape(8,2,3),'logp':lp.reshape(8,2),'advantages':advantage,
                 'returns':value.reshape(8,2)+advantage}
        config={'exploration':{'kind':exploration.KIND,'rho':0.},'epochs':2,'minibatch':8,'clip':.2,'value_weight':.5,'entropy_weight':.01,'grad_clip':.5,'target_kl':.03}
        return model,rollout,config

    def test_continuous_entropy_pressure_on_low_and_high_actuator_means(self):
        model=TinyModel();means=torch.tensor([0.,-2.5,2.5],requires_grad=True)
        actor=means.expand(8192,3);dist=exploration.distribution(model,actor,actor,torch.zeros_like(actor),torch.zeros(8192),0.)
        loss=-.01*exploration.entropy(model,dist).mean();gradient=torch.autograd.grad(loss,means)[0]
        self.assertLess(float(gradient[1]),0);self.assertGreater(float(gradient[2]),0)
        self.assertTrue(torch.isfinite(gradient).all())

    def test_absolute_steering_probe_avoids_signed_cancellation_and_is_neutral(self):
        actor=torch.tensor([[1.,-2.,1.],[-1.,-2.,1.]])
        self.assertEqual(float(actor[:,0].mean()),0);self.assertGreater(float(action_metrics(actor)['mean_abs_steering']),.7)
        model=TinyModel();state=copy.deepcopy(model.state_dict());rng=torch.get_rng_state().clone();x=torch.zeros(16,4)
        self.assertEqual(probe(model,x),probe(model,x));self.assertTrue(equal_tree(model.state_dict(),state));self.assertTrue(torch.equal(rng,torch.get_rng_state()))

    def test_gradient_decomposition_and_observable_projection(self):
        model,rollout,config=self.setup_rollout();state=copy.deepcopy(model.state_dict());rng=torch.get_rng_state().clone()
        result,vectors,values,normalized,pressure=inspect_gradients(model,rollout,config,rng)
        projection,qs=projections(model,rollout['observations'].flatten(0,1),vectors)
        self.assertEqual(set(projection),{'mean_thrust','mean_brake','mean_abs_steering'})
        for name in projection:
            for term in ['policy','weighted_critic','weighted_entropy']:
                self.assertAlmostEqual(projection[name][term],-float(qs[name].dot(vectors[term])))
        self.assertEqual(tuple(pressure.shape),(16,3));self.assertAlmostEqual(float(normalized.mean()),0,places=6)
        self.assertTrue(equal_tree(state,model.state_dict()));self.assertGreater(result['all_parameters']['total_norm'],0)

    def test_matched_branches_preserve_batch_and_reproduce_full_update(self):
        model,rollout,config=self.setup_rollout();optimizer=torch.optim.Adam(model.parameters(),lr=.0003,eps=1e-5)
        initial=copy.deepcopy(model.state_dict());opt=copy.deepcopy(optimizer.state_dict());batch=copy.deepcopy(rollout);rng=torch.get_rng_state().clone();full=None
        for name,overrides in list(VARIANTS.items())+[('full_repeat',{})]:
            model.load_state_dict(initial);optimizer.load_state_dict(copy.deepcopy(opt));torch.set_rng_state(rng)
            metrics=update(model,optimizer,rollout,dict(config,**overrides))
            state={'model':copy.deepcopy(model.state_dict()),'optimizer':copy.deepcopy(optimizer.state_dict()),'rng':torch.get_rng_state(),'metrics':metrics}
            if name=='full':full=state
            if name=='full_repeat':self.assertTrue(equal_tree(state,full))
        self.assertTrue(equal_tree(rollout,batch))

    def test_continuous_labels_and_first_window_are_not_button_truthiness(self):
        row={'tick':29,'before':{'errors':{'position':0.,'velocity':.9,'spin':.1}},'after':{'frame':{'errors':{'position':.1,'velocity':1.1,'spin':.2}}},'action':{'steering':.1,'thrust':.03,'brake':.7}}
        flags=labels(row);self.assertTrue(flags['low_throttle']);self.assertFalse(flags['high_throttle'])
        self.assertTrue(flags['within_all_tolerances']);self.assertTrue(flags['speed_increase']);self.assertTrue(flags['first_30_ticks'])
        row['tick']=30;self.assertFalse(labels(row)['first_30_ticks'])

    def test_row_reward_preserves_graded_deadline_and_continuous_pressure(self):
        config={'gamma':.9995,'velocity_shaping_weight':.25,'deadline_distance_weight':4.}
        frame={'errors':{'position':10.,'velocity':2.,'spin':.1}}
        row={'slot':0,'episode':0,'tick':1799,'before':frame,'after':{'frame':frame,'progress':{'outcome':'deadline'}},'action':{'steering':.2,'thrust':.12,'brake':.7}}
        rollout={'advantages':torch.zeros(1,1),'returns':torch.ones(1,1),'ar_latents':torch.zeros(1,1,3)}
        rows,groups=row_data([row],rollout,torch.zeros(1),torch.ones(1),torch.tensor([[1.,2.,3.]]),config)
        self.assertAlmostEqual(rows[0]['reward']['terminal'],-2.-4.*7/37)
        self.assertEqual(rows[0]['score_pressure'],[1.,2.,3.]);self.assertNotIn('within_all_tolerances',groups)
        total=reward({'frame':frame},row['after'],.9995,.25,4.)['total'];self.assertEqual(rows[0]['reward']['total'],total)


if __name__=='__main__':unittest.main(verbosity=2)
