"""Vector noise scaling, exact parent law, reset isolation, coverage and joint gates."""
import copy
import math
import unittest
import torch
from tools.flight_ppo import exploration
from tools.flight_ppo.model import FlightModel
from tools.flight_audit import propulsion_probe as probe
from tools.flight_audit.persistent_probe import command as original_command
from tools.flight_audit import persistent_probe_tests as parent_tests
from tools.flight_audit.persistent_probe_tests import Model
from tools.flight_audit.hold_robustness_tests import synthetic


class PropulsionProbeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1);torch.use_deterministic_algorithms(True)

    def test_quarter_baseline_exact_production_with_changing_means(self):
        model=Model();noise=probe.innovations(17,'position',0,1,900)[:32]
        previous_actor=torch.zeros(1,3);previous_latent=torch.zeros(1,3);valid=torch.zeros(1)
        for t in range(32):
            actor=torch.tensor([[math.sin(t),-2.+t*.01,.5-t*.02]])
            old=original_command(model,actor,previous_actor,previous_latent,valid,'persistent',noise[t:t+1])
            new=probe.command(model,actor,previous_actor,previous_latent,valid,'quarter',noise[t:t+1])
            self.assertEqual(old[0],new[0]);self.assertTrue(torch.equal(old[1],new[1]));self.assertEqual(old[2],new[2])
            previous_actor=actor;previous_latent=new[1];valid=torch.ones(1)

    def test_vector_conditional_law_density_and_first_steering(self):
        model=Model();noise=probe.innovations(29,'hold',2,7,1800)[:32]
        actors=[];latents=[];logps=[]
        previous_actor=torch.zeros(1,3);previous_latent=torch.zeros(1,3);valid=torch.zeros(1)
        for t in range(32):
            actor=torch.tensor([[math.cos(t)*.2,-3.+t*.01,.5-t*.02]])
            base=exploration.distribution(model,actor,previous_actor,previous_latent,valid,.98,.25)
            commands,latent,logp=probe.command(model,actor,previous_actor,previous_latent,valid,'propulsion',noise[t:t+1])
            expected=base.loc+base.scale*torch.tensor([1.,8.,8.])*noise[t:t+1]
            self.assertTrue(torch.equal(expected,latent))
            if t==0:
                quarter=probe.command(model,actor,previous_actor,previous_latent,valid,'quarter',noise[t:t+1])
                self.assertEqual(commands[0]['steering'],quarter[0][0]['steering'])
            self.assertTrue(-1<=commands[0]['steering']<=1)
            self.assertTrue(all(0<=commands[0][k]<=1 for k in ['thrust','brake']))
            actors.append(actor[0]);latents.append(latent[0]);logps.append(logp)
            previous_actor=actor;previous_latent=latent;valid=torch.ones(1)
        actor=torch.stack(actors);latent=torch.stack(latents);rho=torch.full((32,1),.98);rho[0]=0
        center=actor+rho*torch.cat((torch.zeros(1,3),latent[:-1]-actor[:-1]))
        scale=model.log_std.exp()*torch.tensor([.25,2.,2.])*(1-rho.square()).sqrt()
        self.assertTrue(torch.equal(latent,center+scale*noise))
        logp=exploration.log_prob(model,torch.distributions.Normal(center,scale),latent)
        torch.testing.assert_close(logp,torch.tensor(logps),atol=1e-5,rtol=1e-5)

    def test_reset_ignores_history_uses_full_marginal_scale_and_preserves_rng(self):
        model=Model();actor=torch.tensor([[.1,-3.,.6]]);noise=torch.tensor([[.5,1.,-1.]])
        rng=torch.get_rng_state().clone()
        for condition in probe.RHOS:
            result=probe.command(model,actor,torch.ones(1,3)*12,torch.ones(1,3)*-9,torch.zeros(1),condition,noise)
            scale=model.log_std.exp()*.25*torch.tensor(probe.CHANNEL_FACTORS[condition])
            self.assertTrue(torch.equal(result[1],actor+scale*noise))
        self.assertTrue(torch.equal(rng,torch.get_rng_state()))
        with self.assertRaises(ValueError):probe.conditional(model,actor,actor,actor,torch.zeros(1),'candidate-grid')
        with self.assertRaises(ValueError):probe.command(model,actor,actor,actor,torch.zeros(1),'propulsion',noise*float('nan'))
        mean=probe.command(model,actor,actor,actor,torch.ones(1),'mean',noise)
        self.assertTrue(torch.equal(mean[1],actor));self.assertIsNone(mean[2])

    def test_stationary_scale_and_persistence_for_every_channel(self):
        model=Model();generator=torch.Generator().manual_seed(651)
        previous_actor=torch.zeros(2048,3);previous_latent=torch.zeros(2048,3);valid=torch.zeros(2048);samples=[]
        scale=model.log_std.exp()*torch.tensor([.25,2.,2.])
        for tick in range(256):
            actor=torch.full((2048,3),math.sin(tick/11)*.4)
            dist=probe.conditional(model,actor,previous_actor,previous_latent,valid,'propulsion')
            latent=dist.loc+dist.scale*torch.randn(2048,3,generator=generator)
            samples.append((latent-actor)/scale);previous_actor=actor;previous_latent=latent;valid=torch.ones(2048)
        x=torch.stack(samples)
        for k in range(3):
            self.assertLess(abs(float(x[:,:,k].mean())),.06)
            self.assertLess(abs(float(x[:,:,k].var(unbiased=False))-1),.06)
            self.assertLess(abs(float((x[:-1,:,k]*x[1:,:,k]).mean()/x[:,:,k].var(unbiased=False))-.98),.02)

    def frame(self,angle=0.,velocity=1.,goal=True):
        return {'angle':angle,'center_of_mass':{'x':0.,'y':0.},'target_position':{'x':0.,'y':8. if goal else 0.},
                'velocity':{'x':0.,'y':velocity},'target_velocity':{'x':0.,'y':0.}}

    def test_coverage_uses_before_action_bearing_and_projected_velocity(self):
        command={'steering':0.,'thrust':.25,'brake':.25}
        frames=[self.frame(math.pi),self.frame(velocity=-1),self.frame(),self.frame()]
        trace={'initial':self.frame(),'steps':[{'action':dict(command),'frame':f} for f in frames]}
        trace['steps'][3]['action']['brake']=.25001
        c=probe.coverage(trace,'position')
        self.assertEqual(c['joint_ticks'],3);self.assertEqual(c['joint_longest_run'],3);self.assertEqual(c['first_joint_tick'],0)
        self.assertEqual(c['aligned_joint_ticks'],2);self.assertEqual(c['approaching_aligned_joint_ticks'],1)

    def test_zero_goal_has_no_alignment_and_hold_has_no_bearing(self):
        command={'steering':0.,'thrust':.5,'brake':0.}
        trace={'initial':self.frame(goal=False),'steps':[{'action':command,'frame':self.frame(goal=False)}]}
        c=probe.coverage(trace,'position');self.assertEqual(c['joint_ticks'],1);self.assertEqual(c['aligned_joint_ticks'],0)
        self.assertNotIn('aligned_joint_ticks',probe.coverage(trace,'hold'))
        trace['steps'][0]['action']['thrust']=.24999
        self.assertIsNone(probe.coverage(trace,'position')['first_joint_tick'])

    def test_group_weights_action_means_by_ticks_and_keeps_channel_statistics(self):
        zero={'position':0.,'velocity':0.,'spin':0.};trace=synthetic(30,1800,[zero]*30)
        for frame in [trace['initial']]+[s['frame'] for s in trace['steps']]:frame['angle']=0.
        for step in trace['steps']:step['action']={'steering':0.,'thrust':.5,'brake':.1}
        config={'gamma':.9995,'velocity_shaping_weight':.25,'deadline_distance_weight':4.,'regulation_weight':0.}
        row=probe.metrics(trace,'hold',config);row.update(index=0,condition='propulsion',repeat=0)
        actor=torch.zeros(30,3);latent=torch.arange(90,dtype=torch.float32).reshape(30,3)*.001
        row.update(probe.residual_metrics(actor,latent,torch.ones(3),'propulsion'))
        other=copy.deepcopy(row);other['ticks']=60;other['propulsion_coverage']['ticks']=60
        other['propulsion_coverage']['controls']['thrust']['sum']=6.
        g=probe.group([row,other],'hold')
        self.assertAlmostEqual(g['propulsion_coverage']['controls']['thrust']['mean'],21/90)
        self.assertEqual(g['channel_residuals']['brake']['pairs'],58)

    def test_joint_gate_requires_directional_gain_hold_retention_and_same_seeds(self):
        def records():
            rows=parent_tests.PersistentProbeTests().records()
            for r in rows:
                r['summary']={'quarter':r['summary']['independent'],'propulsion':r['summary']['persistent']}
                for v in r['summary'].values():v['channel_residuals']={k:{'lag1_correlation':.98} for k in ['steering','thrust','brake']}
            return rows
        rows=records();self.assertTrue(probe.gates(rows)['training_candidate'])
        for r in rows:
            if r['suite']=='position':
                for d in ['left','right']:r['summary']['propulsion']['directions'][d]['successes']=0
        self.assertFalse(probe.gates(rows)['training_candidate'])
        rows=records()
        for r in rows:
            if r['suite']=='position' and r['seed']==17:
                for d in ['left','right']:r['summary']['propulsion']['directions'][d]['successes']=0
            if r['suite']=='hold' and r['seed']==43:r['summary']['propulsion']['heading_strata']['0']['successes']=7
        self.assertFalse(probe.gates(rows)['training_candidate'])
        rows=records()
        for r in rows:r['summary']['propulsion']['channel_residuals']['brake']['lag1_correlation']=.8
        self.assertFalse(probe.gates(rows)['training_candidate'])


if __name__=='__main__':unittest.main(verbosity=2)
