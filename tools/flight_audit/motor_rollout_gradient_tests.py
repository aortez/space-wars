"""LF-072 repeat isolation and fixed-state gradient contracts."""
import copy
from pathlib import Path
import tempfile
import time
import unittest

import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from . import motor_rollout_gradient as probe
from .motor_error_tests import fixture

PARENT=Path('data/training-experiments/learned-flight-motor-error-return-v1-20261025')


class RolloutGradientTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)

    def test_noise_seeds_are_unique_and_declared(self):
        seeds=[probe.noise_seed(s,u,r) for s in probe.SEEDS for u in probe.UPDATES for r in range(probe.REPEATS)]
        self.assertEqual(len(seeds),len(set(seeds)));self.assertEqual(len(seeds),240)
        with self.assertRaises(ValueError):probe.noise_seed(17,16,16)

    def test_model_load_preserves_global_rng_and_frozen_parent(self):
        state=probe.load(PARENT/'inputs/speed16/seed-29/checkpoint-0015.pt.gz')
        rng=torch.get_rng_state().clone();model=probe.model_from(state)
        self.assertTrue(torch.equal(rng,torch.get_rng_state()))
        self.assertTrue(equal_tree(model.state_dict(),state['model']))
        self.assertTrue(all(not p.requires_grad for n,p in model.named_parameters() if n.startswith('actor') or n=='log_std'))

    def test_gradient_projection_is_linear_and_missing_sources_zero(self):
        model,r,c=fixture();parent=copy.deepcopy(model)
        names=['rewards','tracking_rewards','terminal_rewards','elapsed_ticks','velocity_error_before','velocity_error_after','dones']
        evidence={k:torch.zeros(8,6) for k in names}
        fixed={'jacobians':torch.randn(3,3493)}
        before=copy.deepcopy(model.state_dict());rng=torch.get_rng_state().clone()
        result=probe.diagnostics(model,parent,r,evidence,fixed,c)
        self.assertTrue(equal_tree(before,model.state_dict()));self.assertTrue(torch.equal(rng,torch.get_rng_state()))
        expected=-fixed['jacobians'].double()@result['gradients']['pooled'].sum(0).double()
        torch.testing.assert_close(result['projections']['pooled'].sum(1),expected,atol=1e-6,rtol=1e-5)
        self.assertEqual(result['protected_stopping_rows'],24)
        r['observations'][:,:,7]=.4
        absent=probe.audit.full_gradients(model,r)
        self.assertFalse(absent['pooled'][:3].count_nonzero())

    def test_native_repeats_are_isolated_exact_and_noise_sensitive(self):
        state=probe.load(PARENT/'inputs/speed16/seed-29/checkpoint-0015.pt.gz')
        pristine=copy.deepcopy(state);model=probe.model_from(state)
        c={**state['contract']['config'],'rollout_ticks':4};rng=torch.get_rng_state().clone()
        work=[];results=[]
        with tempfile.TemporaryDirectory() as temporary,Bridge(PARENT/'runtime/flight_motor_bridge') as bridge:
            for i,seed in enumerate([720291600,720291600,720291601]):
                folder=Path(temporary)/str(i);folder.mkdir()
                result=probe.collect(bridge,model,state,folder,seed,time.monotonic()+30,config=c)
                results.append(result);work.append(result[-1])
            self.assertTrue(equal_tree(results[0],results[1]))
            self.assertFalse(torch.equal(results[0][0]['actuator_latents'],results[2][0]['actuator_latents']))
            self.assertFalse(torch.equal(results[0][0]['observations'],results[2][0]['observations']))
            for rollout,after,begin,end,_ in results:probe.verify_sampling(model,rollout,begin,end)
        self.assertTrue(equal_tree(state,pristine));self.assertTrue(equal_tree(model.state_dict(),state['model']))
        self.assertTrue(torch.equal(rng,torch.get_rng_state()))
        totals={k:sum(w[k] for w in work) for k in work[0]}
        self.assertEqual(totals['probe_ticks'],72)
        atomic_json(Path('/tmp/motor-rollout-gradient-native-test.json'),{'counts':totals,'same_seed_exact':True,
            'different_seed_changes_rollout':True,'source_weights_rng_unchanged':True})


if __name__=='__main__':unittest.main()
