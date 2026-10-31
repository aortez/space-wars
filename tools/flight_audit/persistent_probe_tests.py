"""Production AR law, reset isolation, physical progress and separate opportunity gates."""
import math
import unittest
import torch
from tools.flight_ppo import exploration
from tools.flight_ppo.model import FlightModel
from tools.flight_audit.persistent_probe import (SEEDS, SUITES, INDICES, REPEATS, AMPLITUDE,
    stream_seed, innovations, command, moments, pooled_moments, runs, movement, metrics, gates)
from tools.flight_audit.hold_robustness_tests import synthetic


class Model:
    log_std=torch.tensor([-.5,-.7,-.3])
    log_jacobian=staticmethod(FlightModel.log_jacobian)
    actuator_log_jacobian=staticmethod(FlightModel.actuator_log_jacobian)


class PersistentProbeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):torch.set_num_threads(1);torch.use_deterministic_algorithms(True)

    def test_stream_matrix_is_unique_paired_and_rng_neutral(self):
        keys=[stream_seed(s,t,r,i) for s in SEEDS for t in SUITES for r in range(REPEATS) for i in INDICES[t]]
        self.assertEqual(len(keys),240);self.assertEqual(len(set(keys)),240)
        rng=torch.get_rng_state().clone();a=innovations(17,'position',0,1,900)
        self.assertTrue(torch.equal(a,innovations(17,'position',0,1,900)))
        self.assertFalse(torch.equal(a,innovations(17,'position',1,1,900)))
        self.assertTrue(torch.equal(rng,torch.get_rng_state()))
        for suite,repeat,index in [('position',0,0),('hold',4,0),('unknown',0,1)]:
            with self.assertRaises(ValueError):stream_seed(17,suite,repeat,index)

    def test_first_sample_and_reset_have_full_marginal_scale(self):
        model=Model();actor=torch.tensor([[.1,-2.,.3]]);previous=torch.ones(1,3)*7;latent=torch.ones(1,3)*-5
        noise=innovations(17,'position',0,1,900)[:1];saved=torch.get_rng_state().clone()
        a=command(model,actor,previous,latent,torch.zeros(1),'independent',noise)
        b=command(model,actor,previous,latent,torch.zeros(1),'persistent',noise)
        self.assertTrue(torch.equal(a[1],b[1]));self.assertEqual(a[0],b[0]);self.assertEqual(a[2],b[2])
        self.assertTrue(torch.equal(a[1],actor+model.log_std.exp()*AMPLITUDE*noise))
        self.assertTrue(torch.equal(saved,torch.get_rng_state()))
        mean=command(model,actor,previous,latent,torch.ones(1),'mean',noise)
        self.assertTrue(torch.equal(mean[1],actor));self.assertIsNone(mean[2])
        with self.assertRaises(ValueError):command(model,actor,previous,latent,torch.ones(1),'rho-grid',noise)

    def test_conditional_sequence_matches_production_law_and_log_density(self):
        model=Model();noise=innovations(29,'hold',1,2,1800)[:32]
        previous_actor=torch.zeros(1,3);previous_latent=torch.zeros(1,3);valid=torch.zeros(1)
        actors=[];latents=[];probabilities=[]
        for t in range(32):
            actor=torch.tensor([[math.sin(t/4)*.2,-2.+t*.01,1.-t*.01]])
            commands,latent,logp=command(model,actor,previous_actor,previous_latent,valid,'persistent',noise[t:t+1])
            dist=exploration.distribution(model,actor,previous_actor,previous_latent,valid,.98,.25)
            self.assertTrue(torch.equal(latent,dist.loc+dist.scale*noise[t:t+1]))
            self.assertEqual(logp,float(exploration.log_prob(model,dist,latent)[0]))
            self.assertTrue(all(math.isfinite(v) for v in commands[0].values()))
            actors.append(actor[0]);latents.append(latent[0]);probabilities.append(logp)
            previous_actor=actor;previous_latent=latent;valid=torch.ones(1)
        actor=torch.stack(actors);latent=torch.stack(latents);rho=torch.full((32,1),.98);rho[0]=0
        center=actor+rho*torch.cat((torch.zeros(1,3),latent[:-1]-actor[:-1]))
        scale=model.log_std.exp()*.25*(1-rho.square()).sqrt()
        self.assertTrue(torch.equal(latent,center+scale*noise))
        actual=exploration.log_prob(model,torch.distributions.Normal(center,scale),latent)
        torch.testing.assert_close(actual,torch.tensor(probabilities),atol=1e-5,rtol=1e-5)

    def test_stationary_residual_scale_with_changing_actor_means(self):
        model=Model();std=model.log_std.exp()*AMPLITUDE
        for rho in [0.,.98]:
            generator=torch.Generator().manual_seed(591)
            previous_actor=torch.zeros(2048,3);previous_latent=torch.zeros(2048,3);valid=torch.zeros(2048);samples=[]
            for tick in range(256):
                actor=torch.full((2048,3),math.sin(tick/11)*.4)
                dist=exploration.distribution(model,actor,previous_actor,previous_latent,valid,rho,AMPLITUDE)
                latent=dist.loc+dist.scale*torch.randn(2048,3,generator=generator)
                samples.append(((latent-actor)/std)[:,0]);previous_actor=actor;previous_latent=latent;valid=torch.ones(2048)
            x=torch.stack(samples)
            self.assertLess(abs(float(x.mean())),.06)
            self.assertLess(abs(float(x.var(unbiased=False))-1),.06)
            self.assertLess(abs(float((x[:-1]*x[1:]).mean()/x.var(unbiased=False))-rho),.02)

    def test_lag_statistics_exclude_episode_boundaries_and_zero_breaks_runs(self):
        a=[1.,2.,3.];b=[-10.,-20.,-30.]
        m=pooled_moments([moments(a),moments(b)])
        self.assertEqual(m['pairs'],4);self.assertEqual(m['values'],6)
        left=torch.tensor([1.,2.,-10.,-20.],dtype=torch.float64);right=torch.tensor([2.,3.,-20.,-30.],dtype=torch.float64)
        self.assertAlmostEqual(m['lag1_correlation'],float(torch.corrcoef(torch.stack((left,right)))[0,1]))
        self.assertEqual(runs([1,1,0,1,-1,-1,-1]),{'runs':3,'ticks':6,'longest':3,'mean':2.})

    def frame(self,x,angle):
        return {'angle':angle,'target_position':{'x':8.,'y':0.},'center_of_mass':{'x':x,'y':0.},'errors':{'position':8.-x}}

    def test_useful_turn_requires_coincident_bearing_and_distance_progress(self):
        first=self.frame(0.,0.);turn=self.frame(0.,-math.pi/4);distance=self.frame(1.2,0.)
        separate=movement([first,turn,distance],'position')
        self.assertIsNone(separate['first_useful_turn_tick'])
        together=movement([first,self.frame(1.2,-math.pi/4)],'position')
        self.assertEqual(together['first_useful_turn_tick'],1);self.assertIsNone(together['first_radius_entry_tick'])
        wrapped=movement([self.frame(0.,math.pi-.01),self.frame(0.,-math.pi+.01)],'hold')
        self.assertAlmostEqual(wrapped['signed_turn_radians'],.02)

    def test_discounted_rewards_and_complete_hold_intervals(self):
        zero={'position':0.,'velocity':0.,'spin':0.}
        trace=synthetic(30,1800,[zero]*30)
        for frame in [trace['initial']]+[s['frame'] for s in trace['steps']]:frame['angle']=0.
        config={'gamma':.9995,'velocity_shaping_weight':.25,'deadline_distance_weight':4.,'regulation_weight':0.}
        m=metrics(trace,'hold',config)
        self.assertTrue(m['immediate_hold']);self.assertEqual(m['ticks'],30)
        expected=10*config['gamma']**29-.001*sum(config['gamma']**i for i in range(30))
        self.assertAlmostEqual(m['discounted_reward']['total'],expected)

    def records(self):
        result=[]
        for seed in SEEDS:
            for suite in SUITES:
                summary={}
                for condition,corr in [('independent',0.),('persistent',.98)]:
                    row={'steering_residual':{'lag1_correlation':corr},'successes':56,'immediate_holds':24,'physical_failures':0}
                    if suite=='hold':row['heading_strata']={str(i):{'episodes':16,'successes':14,'immediate_holds':6} for i in range(4)}
                    if suite=='position':
                        row['directions']={d:{'radius_entries':2 if condition=='persistent' and d in ['left','right'] else 0,
                                              'successes':1 if condition=='persistent' and d in ['left','right'] else 0} for d in ['forward','left','backward','right']}
                    summary[condition]=row
                result.append({'seed':seed,'suite':suite,'summary':summary})
        return result

    def test_training_candidate_needs_nonforward_exposure_and_holding_in_same_seeds(self):
        rows=self.records();self.assertTrue(gates(rows)['training_candidate'])
        for r in rows:
            if r['suite']=='position':
                for d in ['left','right']:r['summary']['persistent']['directions'][d]['successes']=0
        self.assertFalse(gates(rows)['training_candidate'])
        rows=self.records()
        for r in rows:
            if r['suite']=='hold':r['summary']['persistent']['successes']=47
        self.assertFalse(gates(rows)['training_candidate'])
        rows=self.records()
        for r in rows:
            if r['suite']=='hold':r['summary']['persistent']['immediate_holds']=15
        self.assertFalse(gates(rows)['training_candidate'])
        rows=self.records()
        for r in rows:
            if r['suite']=='hold' and r['seed']==43:r['summary']['persistent']['successes']=47
            if r['suite']=='position' and r['seed']==17:
                for d in ['left','right']:r['summary']['persistent']['directions'][d]['successes']=0
        self.assertFalse(gates(rows)['training_candidate'])  # Only seed 29 meets both.
        rows=self.records()
        for r in rows:
            if r['suite']=='hold':r['summary']['persistent']['heading_strata']['0']['successes']=7
        self.assertFalse(gates(rows)['training_candidate'])


if __name__=='__main__':unittest.main(verbosity=2)
