"""Paired innovations, amplitude semantics and separate holding/recovery evidence."""
import copy
import unittest
import torch
from tools.flight_ppo.execution import sampled_command
from tools.flight_audit.noise_probe import innovations,stream_seed,command,metrics,gates,SEEDS,SUITES,FAMILIES
from tools.flight_audit.hold_robustness_tests import synthetic

class NoiseProbeTests(unittest.TestCase):
    def test_unique_stream_keys_and_rng_isolation(self):
        keys=[stream_seed(s,t,r,i) for s in SEEDS for t in SUITES for r in range(4) for i in range(16)]
        self.assertEqual(len(set(keys)),384)
        before=torch.get_rng_state().clone()
        a=innovations(17,'recovery',0,1,900);b=innovations(17,'recovery',0,1,900)
        self.assertTrue(torch.equal(a,b));self.assertTrue(torch.equal(before,torch.get_rng_state()))
        self.assertFalse(torch.equal(a,innovations(17,'recovery',1,1,900)))
        with self.assertRaises(ValueError):stream_seed(17,'recovery',4,0)

    def test_shared_noise_scaled_around_unchanged_means(self):
        actor=torch.tensor([[.2,-2.,.1]]);std=torch.tensor([.4,.6,.5]);noise=innovations(17,'recovery',0,1,900)[:1]
        saved=actor.clone();deviation=std.clone()
        for scale in [0.,.25,.5,1.]:
            actions,latent=command(actor,std,scale,noise)
            torch.testing.assert_close(latent,actor+std*scale*noise,rtol=0,atol=0)
            self.assertTrue(-1<=actions[0]['steering']<=1 and 0<=actions[0]['thrust']<=1 and 0<=actions[0]['brake']<=1)
        self.assertTrue(torch.equal(actor,saved) and torch.equal(std,deviation))

    def test_unit_scale_matches_existing_independent_normal_law(self):
        class Model:log_std=torch.tensor([-.5,-.7,-.3])
        actor=torch.tensor([[.2,-2.,.1]])
        with torch.random.fork_rng(devices=[]):
            torch.manual_seed(731);noise=torch.randn(1,3)
            torch.manual_seed(731);old,latent=sampled_command(Model(),actor)
        new,value=command(actor,Model.log_std.exp(),1.,noise)
        torch.testing.assert_close(value,torch.tensor(latent),atol=1e-7,rtol=1e-7)
        for k in new[0]:self.assertAlmostEqual(new[0][k],old[0][k],places=6)

    def test_bad_amplitude_and_latent_rejected(self):
        actor=torch.zeros(1,3);std=torch.ones(3);noise=torch.ones(1,3)
        for scale in [-1.,.1,2.,float('nan')]:
            with self.assertRaises(ValueError):command(actor,std,scale,noise)
        with self.assertRaises(ValueError):command(actor,-std,1.,noise)
        with self.assertRaises(ValueError):command(actor,std,1.,noise*float('inf'))

    def test_holding_effect_cannot_be_reported_as_position_recovery(self):
        records=[]
        for seed in SEEDS:
            for suite in SUITES:
                full={'immediate_holds':0,'successes':0,'by_family':dict.fromkeys(FAMILIES,0),'physical_failures':0}
                half={'immediate_holds':64,'successes':48,'by_family':{'settled':16,'position':0,'velocity':16,'spin':16},'physical_failures':0}
                records.append({'seed':seed,'suite':suite,'summary':{'full':full,'half':half,'quarter':copy.deepcopy(half)}})
        _,g=gates(records)
        for scale in ['half','quarter']:
            self.assertTrue(g[scale]['holding_noise_effect']);self.assertFalse(g[scale]['position_noise_effect']);self.assertFalse(g[scale]['broad_recovery'])
        for r in records:
            if r['suite']=='recovery':
                for scale in ['half','quarter']:r['summary'][scale]['by_family']['position']=12;r['summary'][scale]['successes']=60
        _,g=gates(records);self.assertTrue(g['half']['position_noise_effect'] and g['half']['broad_recovery'])

    def test_complete_interval_and_eligibility_reconstruction(self):
        zero={'position':0.,'velocity':0.,'spin':0.}
        t=synthetic(30,900,[zero]*31,dict(zero,position=4.))
        m=metrics(t);self.assertEqual(m['eligible_before'],30);self.assertFalse(m['immediate_hold']);self.assertEqual(m['maximum_dwell'],30)
        bad=copy.deepcopy(t);bad['steps'][-1]['progress']['dwell_ticks']=31
        with self.assertRaises(ValueError):metrics(bad)

if __name__=='__main__':unittest.main()
