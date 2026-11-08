"""Versioned decoding, differing checkpoint scales and exact trained replay."""
import copy
import gzip
import json
from pathlib import Path
import tempfile
import time
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from .execution import episode_seed
from .steering import rollout,steering_command
from .learning_gain import PARENT,INITIAL,CONTRACT,load_checkpoint,validate_contract,paired_innovations
from .steering_report import paired_outcomes
from .execution_report import material_improvement


class LearningGainTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
        cls.bridge=Bridge(PARENT/'runtime/flight_ppo_bridge')
        cls.initial=load_checkpoint(INITIAL,17,0,cls.bridge)
        cls.trained=load_checkpoint(INITIAL,17,128,cls.bridge)
        admission=json.loads((PARENT/'inputs/seed-17/development-admission.json').read_text())
        cls.candidate=next(r['candidate'] for r in admission['attempts'] if r['admitted'])

    @classmethod
    def tearDownClass(cls):cls.bridge.close()

    def test_candidate_contract_rejects_changed_semantics(self):
        spec=validate_contract(CONTRACT)
        self.assertEqual(spec['decision_hz'],60)
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'contract.json'
            for key,value in [('correlation_rho',.5),('steering','hard-coded zero'),('version',2),('native_fallback',True)]:
                changed=copy.deepcopy(spec);changed[key]=value;path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):validate_contract(path)

    def test_pairing_uses_each_checkpoints_own_steering_scale(self):
        rows=[]
        for model,actor in [(self.initial,torch.tensor([[.1,0.,0.]])),(self.trained,torch.tensor([[-.2,-3.,2.]]))]:
            torch.manual_seed(33);_,latent=steering_command(model,actor)
            rows.append([{'actor':actor[0].tolist(),'latent':latent[0],'steering_std':float(model.log_std.clamp(-5,2).exp()[0])}])
        self.assertNotEqual(rows[0][0]['steering_std'],rows[1][0]['steering_std'])
        self.assertLess(paired_innovations(*rows)['maximum_abs_standardized_innovation_error'],2e-5)
        broken=copy.deepcopy(rows[1]);broken[0]['latent'][2]+=.1
        with self.assertRaises(AssertionError):paired_innovations(rows[0],broken)

    def test_real_initial_repeatability_and_exact_trained_replay(self):
        c=self.candidate;seed=episode_seed(17,0,c['index']);before={k:v.clone() for k,v in self.initial.state_dict().items()}
        a=rollout(self.bridge,self.initial,c,seed,time.monotonic()+30,32)
        b=rollout(self.bridge,self.initial,c,seed,time.monotonic()+30,32)
        other=rollout(self.bridge,self.initial,c,seed+1000,time.monotonic()+30,32)
        final=rollout(self.bridge,self.trained,c,seed,time.monotonic()+30,32)
        self.assertEqual(a,b);self.assertNotEqual(a[1],other[1])
        stem=f"mean_steering-r0-clear-{c['index']}.json.gz"
        with gzip.open(PARENT/'seed-17/policy'/stem,'rt') as f:old_rows=json.load(f)
        with gzip.open(PARENT/'seed-17/traces'/stem,'rt') as f:old_trace=json.load(f)
        self.assertEqual(final[0]['steps'],old_trace['steps'][:32]);self.assertEqual(final[1],old_rows[:32])
        self.assertEqual(a[0]['initial'],old_trace['initial']);paired_innovations(a[1],old_rows)
        self.assertTrue(all(torch.equal(v,before[k]) for k,v in self.initial.state_dict().items()))
        self.assertTrue(any(not torch.equal(v,self.trained.state_dict()[k]) for k,v in before.items()))

    def test_learning_gain_direction_and_paired_losses(self):
        initial=[{'repeat':r,'index':i,'rng_seed':r*100+i,'recipe':f'aligned-approach-v1/stratum-{i//4}','outcome':'success' if i==0 else 'deadline'} for r in range(4) for i in range(16)]
        trained=copy.deepcopy(initial)
        for e in trained:e['outcome']='success' if 1<=e['index']<=5 else 'deadline'
        self.assertEqual(paired_outcomes(initial,trained),{'lost_success':4,'gained_success':20,'neither_success':40})
        self.assertTrue(material_improvement(trained,4/64))
        self.assertFalse(material_improvement(initial,20/64))
        with self.assertRaises(ValueError):load_checkpoint(INITIAL,17,64,self.bridge)


if __name__=='__main__':unittest.main(verbosity=2)
