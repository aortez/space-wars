"""Check the steering intervention and pairing against actual archived sampling."""
import copy
import gzip
import json
import time
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo import exploration
from .execution import command,episode_seed,load_frozen,rollout as full_rollout
from .steering import PARENT,steering_command,innovation_error,rollout
from .steering_report import paired_outcomes
from .execution_report import material_improvement


class SteeringTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
        cls.bridge=Bridge(PARENT/'runtime/flight_ppo_bridge')
        cls.model=load_frozen(PARENT/'inputs',17,cls.bridge)
        admission=json.loads((PARENT/'inputs/seed-17/development-admission.json').read_text())
        cls.candidate=next(r['candidate'] for r in admission['attempts'] if r['admitted'])

    @classmethod
    def tearDownClass(cls):cls.bridge.close()

    def test_only_executed_steering_changes_and_rng_consumption_matches(self):
        actor=torch.tensor([[.2,-3.,1.],[-.3,-.4,.1]])
        torch.manual_seed(11);a,z=command(self.model,actor,'sampled');rng=torch.get_rng_state()
        torch.manual_seed(11);b,w=steering_command(self.model,actor)
        self.assertEqual(z,w);self.assertTrue(torch.equal(rng,torch.get_rng_state()))
        for i,(left,right) in enumerate(zip(a,b)):
            self.assertEqual(left['thrust'],right['thrust']);self.assertEqual(left['brake'],right['brake'])
            self.assertEqual(right['steering'],float(actor[i,0].tanh()))
            self.assertNotEqual(left['steering'],right['steering'])

    def test_innovations_stay_paired_when_state_dependent_means_change(self):
        rows=[]
        for actor in [torch.tensor([[.2,-3.,1.]]),torch.tensor([[-.4,-1.,-.5]])]:
            torch.manual_seed(22);_,z=steering_command(self.model,actor)
            rows.append([{'actor':actor[0].tolist(),'latent':z[0]}])
        self.assertLess(innovation_error(self.model,*rows)['maximum_abs_standardized_innovation_error'],2e-5)
        broken=copy.deepcopy(rows[1]);broken[0]['latent'][1]+=.1
        with self.assertRaises(AssertionError):innovation_error(self.model,rows[0],broken)

    def test_real_replay_is_repeatable_paired_and_weights_unchanged(self):
        c=self.candidate;seed=episode_seed(17,0,c['index']);before={k:v.clone() for k,v in self.model.state_dict().items()}
        a=rollout(self.bridge,self.model,c,seed,time.monotonic()+30,32)
        b=rollout(self.bridge,self.model,c,seed,time.monotonic()+30,32)
        other=rollout(self.bridge,self.model,c,seed+1000,time.monotonic()+30,32)
        self.assertEqual(a,b);self.assertNotEqual(a[1],other[1])
        full=full_rollout(self.bridge,self.model,c,'sampled',seed,time.monotonic()+30,32)
        stem=f"sampled-r0-clear-{c['index']}.json.gz"
        with gzip.open(PARENT/'seed-17/policy'/stem,'rt') as f:old_rows=json.load(f)
        with gzip.open(PARENT/'seed-17/traces'/stem,'rt') as f:old_trace=json.load(f)
        self.assertEqual(full[0]['steps'],old_trace['steps'][:32]);self.assertEqual(full[1],old_rows[:32])
        self.assertEqual(a[0]['initial'],old_trace['initial'])
        innovation_error(self.model,a[1],old_rows)
        self.assertTrue(all(torch.equal(v,before[k]) for k,v in self.model.state_dict().items()))

    def test_paired_outcomes_and_gain_threshold(self):
        base=[{'repeat':r,'index':i,'rng_seed':r*100+i,'recipe':f'aligned-approach-v1/stratum-{i//4}','outcome':'success' if i==0 else 'deadline'} for r in range(4) for i in range(16)]
        changed=copy.deepcopy(base)
        for r in changed:r['outcome']='success' if 1<=r['index']<=5 else 'deadline'
        self.assertEqual(paired_outcomes(base,changed),{'lost_success':4,'gained_success':20,'neither_success':40})
        self.assertTrue(material_improvement(changed,4/64))
        changed[-12]['outcome']='deadline'
        self.assertFalse(material_improvement(changed,4/64))
        with self.assertRaises(AssertionError):paired_outcomes(base,changed[:-1])


if __name__=='__main__':unittest.main(verbosity=2)
