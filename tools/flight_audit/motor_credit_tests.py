"""Synthetic read-only LF-058 checks: no optimizer or physical environment."""
import copy
import math
from pathlib import Path
import tempfile
import unittest
import torch
from tools.flight_ppo.ppo import gae
from tools.flight_ppo.tests import equal_tree
from tools.flight_motor import persistent
from tools.flight_motor.model import MotorModel
from . import motor_credit as audit


def model():
    return MotorModel({'version':1,'features':list(range(18))})


def trace(complete=True):
    def item(t):
        return {'inputs':[float(t)]*18,'frame':{'velocity':{'x':-t/10,'y':0}},
                'errors':{'heading':0.,'spin':0.,'velocity':abs(10-t/10)},
                'progress':{'elapsed_ticks':t,'outcome':'deadline' if complete and t==65 else None}}
    return {'initial':item(0),'command':{'heading':math.pi/2,'speed':10},'task':{'deadline_ticks':65},
            'steps':[{'item':item(t),'action':{'steering':0.,'thrust':.8,'brake':0.}} for t in range(1,66)]}


class CreditTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)

    def test_gae_terminal_bootstrap_and_no_reset_leakage(self):
        r=torch.tensor([[-1.],[2.],[-3.]]);d=torch.tensor([[0.],[1.],[0.]])
        v=torch.tensor([[.2],[.3],[.4]]);last=torch.tensor([.5])
        a,target,delta=audit.rebuild_advantages(r,d,v,last,.5,.5)
        torch.testing.assert_close(delta[:,0],torch.tensor([-1.05,1.7,-3.15]))
        torch.testing.assert_close(a[:,0],torch.tensor([-.625,1.7,-3.15]))
        original=gae(r,d,v,last,.5,.5)
        self.assertTrue(torch.equal(a,original[0]) and torch.equal(target,original[1]))
        cf,_,_=audit.rebuild_advantages(r,d,v,last,.5,.5,torch.tensor([[0.],[-4.],[0.]]))
        torch.testing.assert_close(cf[:,0],torch.tensor([-1.125,-.3,-3.15]))
        r[2]=-1000
        changed=audit.rebuild_advantages(r,d,v,last,.5,.5)[0]
        self.assertTrue(torch.equal(changed[:2],a[:2]))

    def test_gae_exact_original_float_order(self):
        torch.manual_seed(5);r=torch.randn(256,4);v=torch.randn(256,4)
        d=(torch.rand(256,4)<.1).float();last=torch.randn(4)
        a,t,_=audit.rebuild_advantages(r,d,v,last,.99,.95)
        expected=gae(r,d,v,last,.99,.95)
        self.assertTrue(torch.equal(a,expected[0]) and torch.equal(t,expected[1]))

    def test_before_after_projection_reward_and_horizons(self):
        c=audit.episode_arrays(trace(),'track',7)
        torch.testing.assert_close(c['forward'],torch.arange(65)/10)
        self.assertLess(float(c['lateral'].abs().max()),1e-12)
        self.assertEqual(c['observations'][0,0],0);self.assertEqual(c['after_observations'][0,0],1)
        self.assertEqual(c['remaining'].tolist(),list(range(65,0,-1)))
        self.assertEqual(int(c['future_valid_30'].sum()),36);self.assertEqual(int(c['future_valid_60'].sum()),6)
        self.assertAlmostEqual(float(c['velocity_gain_30'][0]),3)
        self.assertAlmostEqual(float(c['observed_reward_30'][0]),sum(.99**k*(-((10-(k+1)/10)/5)**2/60) for k in range(30)),places=6)
        torch.testing.assert_close(c['reward'],c['cost_heading']+c['cost_velocity']+c['cost_spin']+c['terminal_reward'])

    def test_incomplete_tail_excluded_from_episode_return(self):
        self.assertTrue(audit.episode_arrays(trace(),'track',0)['episode_return_valid'].all())
        self.assertFalse(audit.episode_arrays(trace(False),'track',0)['episode_return_valid'].any())

    def test_masks_require_heading_spin_lateral_and_direction(self):
        c=audit.episode_arrays(trace(),'track',0)
        c['heading'][1]=.2;c['spin'][2]=.3;c['lateral'][3]=2;c['actions'][4,2]=.8
        c['forward'][5]=12;c['remaining'][6]=1
        g=audit.groups(c)
        self.assertTrue(g['helpful_push'][0]);self.assertFalse(g['helpful_push'][1:6].any())
        self.assertTrue(g['helpful_push_deadline_near'][6]);self.assertTrue(g['ready_overspeed'][5])
        self.assertFalse(g['helpful_brake'][5]);c['actions'][5,2]=.8
        self.assertTrue(audit.groups(c)['helpful_brake'][5])

    def test_summary_empty_groups_and_return_validity(self):
        c=audit.episode_arrays(trace(False),'track',0)
        for k in audit.METRICS:
            if k not in c:c[k]=torch.zeros(65)
        s=audit.summarize(c)
        self.assertEqual(s['stopping']['count'],0);self.assertNotIn('reward',s['stopping'])
        self.assertEqual(s['all']['complete_episode_return_count'],0)
        self.assertNotIn('observed_episode_return',s['all'])
        self.assertEqual(s['helpful_push']['count'],65)

    def test_conditional_bias_score_matches_autograd(self):
        m=model();x=torch.zeros(3,18);x[:2,2]=.5;previous=x.clone();previous[:,0]=1
        flat={'observations':x,'ar_previous_observations':previous,
              'ar_previous_latents':torch.tensor([[.1,-2.,.3]]*3),'ar_previous_valid':torch.tensor([1.,0.,1.]),
              'latent':torch.tensor([.4,.3,-.2]),'actuator_latents':torch.tensor([[1.,-1.],[.2,.5],[-3.,.4]])}
        for arm in audit.ARMS:
            config=persistent.config(17,arm);actor=m(x)[0];lp,score=audit.likelihood(m,actor,flat,config)
            gradient=torch.autograd.grad(lp.sum(),m.actor.bias)[0]
            torch.testing.assert_close(gradient,score.sum(0),atol=2e-6,rtol=1e-5)

    def test_exact_first_minibatch_replay_is_read_only(self):
        for arm in audit.ARMS:
            with self.subTest(arm=arm),tempfile.TemporaryDirectory() as d:
                torch.manual_seed(17);m=model();config=persistent.config(17,arm)
                x=torch.randn(4,18);x[:,2]=torch.tensor([.2,0.,.5,0.])
                before={'torch_rng':torch.get_rng_state().clone()};rows=[];ar=persistent.initial_state(4)
                with torch.no_grad():
                    for t in range(256):
                        actor,value=m(x);row={'observations':x.clone()}
                        if arm=='persistent':
                            previous=m(ar['previous_observations'])[0]
                            dist=persistent.distribution(m,actor,previous,ar['previous_latents'],ar['previous_valid'],x,config['motor_exploration'])
                            z=dist.sample();lp=persistent.log_prob(m,dist,z)
                            row.update(ar_previous_observations=ar['previous_observations'].clone(),ar_previous_latents=ar['previous_latents'].clone(),
                                       ar_previous_valid=ar['previous_valid'].clone(),ar_latents=z)
                            persistent.advance(ar,x,z,[t%40==39]*4);latent=z[:,0];actuators=z[:,1:]
                        else:latent,actuators,lp,value=m.sample(x)
                        row.update(latent=latent,actuator_latents=actuators,logp=lp,advantages=torch.tensor([-.5,.2,.8,-.7]),returns=value+1)
                        rows.append(row)
                rollout={k:torch.stack([r[k] for r in rows]) for k in rows[0]}
                with torch.random.fork_rng(devices=[]):expected_indices=torch.randperm(1024)[:256].tolist()
                weights=copy.deepcopy(m.state_dict());rng=torch.get_rng_state().clone()
                r=audit.first_minibatch_gradients(m,before,rollout,config,Path(d)/'grad.pt')
                self.assertEqual(r['minibatch_indices'],expected_indices)
                self.assertTrue(equal_tree(weights,m.state_dict()) and torch.equal(rng,torch.get_rng_state()))
                self.assertEqual(r['actor_bias_ascent']['weighted_value'],[0.,0.,0.])
                self.assertGreater(r['gradient_norms']['policy'],0);self.assertGreater(r['gradient_norms']['weighted_value'],0)

    def test_fixed_state_probe_deterministic_and_unmodified(self):
        m=model();x=torch.ones(7,18);saved=x.clone();rng=torch.get_rng_state().clone()
        self.assertEqual(audit.anchor_probe(m,x),audit.anchor_probe(m,x))
        self.assertTrue(torch.equal(saved,x) and torch.equal(rng,torch.get_rng_state()))


if __name__=='__main__':unittest.main(verbosity=2)
