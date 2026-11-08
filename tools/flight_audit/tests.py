"""Validate audit arithmetic and one exact archived rollout without updates."""
import json
import math
from pathlib import Path
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import identity,load
from tools.flight_ppo.tests import equal_tree
from .signal import RecordingBridge,heading_cos,reward_components,gradient_summary,inspect_gradients,inspect_rows

PARENT=Path('data/training-experiments/learned-flight-credit-v1-20261023')


class AuditTests(unittest.TestCase):
    def test_heading_convention_and_zero_distance(self):
        f={'angle':0.,'center_of_mass':{'x':0.,'y':0.},'target_position':{'x':0.,'y':1.}}
        self.assertAlmostEqual(heading_cos(f),1.)
        f['angle']=math.pi;self.assertAlmostEqual(heading_cos(f),-1.)
        f['target_position']={'x':0.,'y':0.};self.assertEqual(heading_cos(f),0.)

    def test_reward_decomposition_with_terminal_potential_reset(self):
        before={'errors':{'position':20.,'velocity':3.,'spin':.2}}
        after={'frame':{'errors':{'position':19.,'velocity':4.,'spin':.3}},'progress':{'outcome':None}}
        parts=reward_components(before,after,.9995,.25)
        self.assertGreater(parts['position'],0.)
        self.assertLess(parts['velocity'],0.);self.assertLess(parts['spin'],0.)
        after['progress']['outcome']='collision'
        parts=reward_components(before,after,.9995,.25)
        self.assertEqual(parts['terminal'],-10.)
        for key in ['position','velocity','spin']:self.assertGreater(parts[key],0.)

    def test_gradient_conflict_and_zero_vectors(self):
        result=gradient_summary(torch.tensor([1.,0.]),torch.tensor([-2.,0.]),torch.zeros(2))
        self.assertEqual(result['policy_critic_cosine'],-1.)
        self.assertEqual(result['critic_to_policy_norm_ratio'],2.)
        self.assertEqual(result['policy_total_cosine'],-1.)
        self.assertIsNone(gradient_summary(torch.zeros(2),torch.zeros(2),torch.zeros(2))['policy_critic_cosine'])

    def test_real_rollout_reconstruction_and_gradient_probe_do_not_update_model(self):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
        run=PARENT/'control/seed-17';config=json.loads((run/'contract.json').read_text())['config']
        with Bridge(PARENT/'runtime/flight_ppo_bridge') as raw:
            bridge=RecordingBridge(raw)
            model,optimizer,collector,_,_=load(run/'checkpoint-0000.pt',identity(config,raw),bridge)
            weights={k:v.clone() for k,v in model.state_dict().items()}
            bridge.collector=collector
            rollout=collector.collect(model,config)
            rng=torch.get_rng_state();state=collector.state()
            expected=torch.load(run/'checkpoint-0001.pt',weights_only=True)
            self.assertTrue(equal_tree(state,expected['collector']))
            report,vectors,values,normalized,pressure=inspect_gradients(model,rollout,config,rng)
            groups,rows=inspect_rows(bridge.rows,rollout,values,normalized,pressure,config)
            self.assertEqual(len(rows),1024)
            self.assertEqual(sum(groups[x]['count'] for x in ['thrust_only','brake_only','both','neither']),1024)
            self.assertTrue(equal_tree(weights,model.state_dict()))
            self.assertFalse(optimizer.state)
            self.assertLess(report['maximum_abs_logp_error'],5e-6)
            self.assertLess(abs(float(normalized.mean())),1e-6)
            self.assertEqual(bridge.ticks,1024)


if __name__=='__main__':unittest.main(verbosity=2)
