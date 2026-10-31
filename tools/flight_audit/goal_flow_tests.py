"""Synthetic derivative units, saturation, family accounting and RNG neutrality."""
import copy
import json
from pathlib import Path
import unittest
import torch
from tools.flight_ppo.model import FlightModel, GOAL_SCALING
from tools.flight_ppo.tests import equal_tree
from tools.flight_ppo import exploration
from tools.flight_audit.goal_flow import layer_flow, layer_tape, first_gradients, goal_mask


class GoalFlowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
        cls.schema=json.loads(Path('data/training-experiments/learned-flight-goal-scaling-v1-20261024/pilot/schema/seed-17/policy-0000.json').read_text())['observation_schema']

    def model_inputs(self, rows=4):
        torch.manual_seed(39);m=FlightModel(self.schema).double();x=torch.randn(rows,4009,dtype=torch.float64)*.05
        x[:,3497:].reshape(-1,32,16)[:,:,0]=1
        return m,x

    def test_physical_goal_jacobian_matches_finite_difference(self):
        m,x=self.model_inputs();metrics,data=layer_flow(m,x);step=.01
        for axis in range(2):
            left=x.clone();right=x.clone();left[:,axis]-=step/300;right[:,axis]+=step/300
            difference=(layer_tape(m,right)['controls']-layer_tape(m,left)['controls'])/(2*step)
            torch.testing.assert_close(data['goal_jacobians']['controls'][:,:,axis],difference,atol=1e-10,rtol=1e-5)
        self.assertEqual(metrics['rows'],4)

    def test_reparameterized_physical_derivative_is_preserved(self):
        base,x=self.model_inputs();goal=FlightModel(self.schema,GOAL_SCALING).double();goal.load_state_dict(base.state_dict())
        with torch.no_grad():goal.query.weight[:,:2]/=32;goal.fusion1.weight[:,:2]/=32
        _,a=layer_flow(base,x);_,b=layer_flow(goal,x)
        for key in ['z1','h1','z2','h2','actor','controls']:
            torch.testing.assert_close(a['goal_jacobians'][key],b['goal_jacobians'][key],atol=1e-12,rtol=1e-10)

    def test_saturated_layer_damps_derivatives_without_changing_forward(self):
        m,x=self.model_inputs()
        with torch.no_grad():m.fusion1.bias.fill_(20)
        before=copy.deepcopy(m.state_dict());rng=torch.get_rng_state().clone();actor=m(x)[0].detach()
        metrics,data=layer_flow(m,x)
        self.assertEqual(metrics['tanh']['fusion1']['fraction_slope_at_most_0_01'],1.)
        self.assertLess(float(data['goal_jacobians']['controls'].norm()),1e-12)
        self.assertGreater(float(data['slope_one_control_jacobians']['both_slopes_one'].norm()),1e-7)
        self.assertTrue(equal_tree(before,m.state_dict()));self.assertTrue(torch.equal(rng,torch.get_rng_state()))
        self.assertTrue(torch.equal(actor,m(x)[0]))

    def test_first_minibatch_family_denominator_and_head_only_routing(self):
        m,x=self.model_inputs(16);m=m.float();x=x.float()
        with torch.no_grad():
            actor,value=m(x);dist=exploration.distribution(m,actor,actor,torch.zeros(16,3),torch.zeros(16),0.,.25)
            latent=dist.sample();lp=exploration.log_prob(m,dist,latent)
        batch={'observations':x.reshape(4,4,4009),'ar_previous_observations':torch.zeros(4,4,4009),
               'ar_previous_latents':torch.zeros(4,4,3),'ar_previous_valid':torch.zeros(4,4),'ar_latents':latent.reshape(4,4,3),
               'logp':lp.reshape(4,4),'advantages':torch.linspace(-2,3,16).reshape(4,4),'returns':(value+1).reshape(4,4)}
        config={'minibatch':16,'clip':.2,'value_weight':.5,'entropy_weight':.01,'grad_clip':.5,
                'exploration':{'kind':exploration.SCALED_KIND,'rho':0.,'std_multiplier':.25}}
        rng=torch.get_rng_state().clone();before=copy.deepcopy(m.state_dict())
        stats,vectors,values,normalized,pressure=first_gradients(m,batch,config,rng)
        self.assertTrue(torch.equal(rng,torch.get_rng_state()));self.assertTrue(equal_tree(before,m.state_dict()))
        self.assertEqual([v['rows'] for v in stats['families'].values()],[4]*4)
        torch.testing.assert_close(sum(vectors['policy_by_family'].values()),vectors['components']['policy'],atol=2e-6,rtol=2e-5)
        self.assertEqual(int(torch.count_nonzero(vectors['components']['weighted_critic'][goal_mask(m)])),0)
        self.assertGreater(float(vectors['components']['weighted_critic'].norm()),0)
        self.assertEqual(pressure.shape,(16,3));self.assertAlmostEqual(float(normalized.mean()),0,places=6)
        self.assertTrue(all(p.grad is None for p in m.parameters()))

    def test_goal_mask_excludes_velocity_and_other_weights(self):
        m,_=self.model_inputs();mask=goal_mask(m)
        self.assertEqual(int(mask.sum()),192)
        offset=0
        for name,p in m.named_parameters():
            part=mask[offset:offset+p.numel()].reshape(p.shape);offset+=p.numel()
            if name in ['query.weight','fusion1.weight']:
                self.assertTrue(part[:,:2].all());self.assertFalse(part[:,2:].any())
            else:self.assertFalse(part.any())


if __name__=='__main__':unittest.main(verbosity=2)
