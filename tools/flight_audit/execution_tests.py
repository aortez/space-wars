"""Sampling-law equivalence, native replay, repeatability and diagnostic gates."""
import copy
import json
from pathlib import Path
import time
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo import exploration
from .execution import PARENT, command, episode_seed, load_frozen, rollout, metrics
from .execution_report import summarize, material_improvement


class ExecutionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
        cls.bridge=Bridge(PARENT/'runtime/flight_ppo_bridge')
        cls.model=load_frozen(PARENT,17,cls.bridge)
        cls.bridge.call('load_policy',path=str((PARENT/'seed-17/policy-0128.json').resolve()))
        batch=json.loads((PARENT/'seed-17/development-admission.json').read_text())
        cls.candidate=next(r['candidate'] for r in batch['attempts'] if r['admitted'])

    @classmethod
    def tearDownClass(cls):cls.bridge.close()

    def test_sampler_matches_training_distribution_and_threshold(self):
        actor=torch.tensor([[.2,-4.,1.],[-.3,-.01,.01]])
        torch.manual_seed(7001);actual,latent=command(self.model,actor,'sampled')
        torch.manual_seed(7001)
        dist=exploration.distribution(self.model,actor,actor+2,actor-3,torch.ones(2),0.)
        expected=dist.sample()
        self.assertEqual(latent,expected.tolist())
        p=torch.distributions.Normal(0.,1.).cdf(dist.loc[:,1:]/dist.scale[:,1:])
        torch.testing.assert_close(p,actor[:,1:].sigmoid(),atol=1e-6,rtol=1e-6)
        deterministic,_=command(self.model,actor,'deterministic')
        self.assertFalse(deterministic[0]['thrust']);self.assertTrue(deterministic[0]['brake'])
        for a,e in zip(actual,expected):
            self.assertEqual(a['thrust'],bool(e[1]>=0));self.assertEqual(a['brake'],bool(e[2]>=0))

    def test_native_prefix_exact_and_weights_frozen(self):
        before={k:v.clone() for k,v in self.model.state_dict().items()}
        trace,rows=rollout(self.bridge,self.model,self.candidate,'deterministic',1,time.monotonic()+30,32)
        parent=json.loads((PARENT/f"seed-17/evaluation-final/clear-{self.candidate['index']}.json").read_text())
        self.assertEqual(trace['initial'],parent['initial']);self.assertEqual(trace['steps'],parent['steps'][:32])
        result=metrics(self.candidate,trace,rows,'deterministic',-1,1)
        self.assertEqual(result['all_ticks']['executed_thrust_fraction'],0.)
        self.assertFalse(result['entered_goal_radius'])
        self.assertTrue(all(torch.equal(v,before[k]) for k,v in self.model.state_dict().items()))

    def test_sampled_repeatability_and_independent_episode_seeds(self):
        seed=episode_seed(17,0,self.candidate['index'])
        a=rollout(self.bridge,self.model,self.candidate,'sampled',seed,time.monotonic()+30,32)
        b=rollout(self.bridge,self.model,self.candidate,'sampled',seed,time.monotonic()+30,32)
        c=rollout(self.bridge,self.model,self.candidate,'sampled',seed+1000,time.monotonic()+30,32)
        self.assertEqual(a,b);self.assertNotEqual(a[1],c[1])
        seeds={episode_seed(l,r,i) for l in [17,29,43] for r in range(4) for i in range(512)}
        self.assertEqual(len(seeds),3*4*512)

    def test_arrival_finding_requires_multiple_models_and_repeats(self):
        def records(successes):
            return [{'repeat':rep,'recipe':f'aligned-approach-v1/stratum-{i//4}',
                     'outcome':'success' if i<n else 'deadline'} for rep,n in enumerate(successes) for i in range(16)]
        passed=material_improvement(records([4,4,4,4]),0.)
        self.assertTrue(passed)
        self.assertFalse(material_improvement(records([16,0,0,0]),0.))
        self.assertFalse(material_improvement(records([4,4,4,3]),0.))
        self.assertFalse(material_improvement(records([4,4,4,4]),.1))
        self.assertEqual(summarize(records([4,4,4,4]))['successes'],16)


if __name__=='__main__':unittest.main(verbosity=2)
