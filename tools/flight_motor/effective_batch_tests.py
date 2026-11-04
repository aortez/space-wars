"""LF-073 batching, return boundaries, mean gradients and protected control tests."""
import copy
from pathlib import Path
import tempfile
import time
import unittest
import torch
from tools.flight_ppo.tests import equal_tree
from tools.flight_ppo.ppo import gae
from tools.flight_audit.motor_gradient_math import gradient, parameters
from . import effective_batch as stage, single_speed, aligned
from .collector import Collector
from .gae_tests import SyntheticBridge
from .single_speed_tests import make
from .gae_audit import discounted_targets
from .movement_run import restore
from . import storage


class EffectiveBatchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)

    def test_matched_physics_and_explicit_optimizer_counts(self):
        for seed in stage.SEEDS:
            for pre in [False, True]:
                a, b = [stage.config(seed, arm, pre) for arm in stage.ARMS]
                self.assertEqual(a['updates']*a['rollout_ticks'], b['updates']*b['rollout_ticks'])
                self.assertEqual(a['updates'], 4*b['updates'])
                self.assertEqual(a['minibatch'], 256); self.assertEqual(b['minibatch'], 1024)
                self.assertEqual(a['epochs'], b['epochs']); self.assertEqual(a['epochs'], 4)
                old = single_speed.config(seed, 'speed16', pre)
                self.assertEqual({k for k in old if old[k] != a[k]}, {'arm'} if pre else {'arm', 'updates'})
                self.assertEqual(stage.collector_pools(a), stage.collector_pools(b))
        self.assertIs(stage.update, single_speed.update)

    def test_four_segments_equal_manual_unchanged_policy_collection(self):
        with tempfile.TemporaryDirectory() as d:
            results = []
            for manual in [False, True]:
                torch.manual_seed(29); c = stage.config(29, 'large', True); model = make(29, c)
                collector = Collector(SyntheticBridge(), stage.training_candidates(c), Path(d)/str(manual), stage.collector_pools(c))
                if manual:
                    rows = [collector.collect(model, {**c, 'rollout_ticks':256}, time.monotonic()+30) for _ in range(4)]
                    r = {k:torch.cat([v[k] for v in rows]) for k in rows[0]}
                else: r = stage.collect(collector, model, c, time.monotonic()+30)
                results.append((r, {k:getattr(collector,k) for k in ['pools','cursors','started','steps','completed','active','items']}, torch.get_rng_state()))
            self.assertTrue(equal_tree(results[0], results[1]))
            r = results[0][0]; self.assertEqual(r['observations'].shape, (1024, 6, 18))
            with torch.no_grad():
                values = torch.stack([model(x)[1] for x in r['observations']])
                last = model(torch.tensor([i['inputs'] for i in collector.items]))[1]
            reward = torch.full((1024, 6), -(2/5)**2/60); dones = torch.zeros_like(reward)
            for lo in range(0, 1024, 256):
                hi=lo+256; tail=values[hi] if hi<1024 else last
                expected = discounted_targets(reward[lo:hi], dones[lo:hi], tail, .99)
                torch.testing.assert_close(r['returns'][lo:hi], expected, atol=1e-5, rtol=1e-5)
            extended = gae(reward, dones, values, last, .99, 1.)[1]
            self.assertFalse(torch.allclose(extended, r['returns']))

    def test_larger_update_averages_before_step_and_preserves_stopping(self):
        with tempfile.TemporaryDirectory() as d:
            torch.manual_seed(43); c=stage.config(43, 'large', True); model=make(43,c)
            collector=Collector(SyntheticBridge(),stage.training_candidates(c),Path(d)/'traces',stage.collector_pools(c))
            r=stage.collect(collector,model,c,time.monotonic()+30)
            before=copy.deepcopy(model.state_dict()); opt=torch.optim.Adam(model.parameters(),lr=c['learning_rate'],eps=1e-5)
            sizes=[]
            handle=model.register_forward_pre_hook(lambda _, args:sizes.append(len(args[0])))
            metrics=stage.update(model,opt,r,c);handle.remove()
            self.assertEqual(metrics['minibatches'],24);self.assertEqual(sizes,[1024]*24)
            self.assertEqual({int(v['step']) for v in opt.state.values()}, {24})
            self.assertFalse(equal_tree(before,model.state_dict()));stage.assert_parent_actor(before,model.state_dict())
            x=r['observations'].flatten(0,1); x=x[x[:,2]==0]
            with torch.no_grad():final=model(x)[0];scale=model.distribution(final,x).scale
            model.load_state_dict(before)
            with torch.no_grad():
                self.assertTrue(torch.equal(final,model(x)[0]));self.assertTrue(torch.equal(scale,model.distribution(model(x)[0],x).scale))

    def test_global_group_means_equal_weighted_microbatch_gradient(self):
        torch.manual_seed(73);c=stage.config(29,'large');model=make(29,c)
        x=torch.randn(1024,18)*.1;x[:,1]=1;x[:,2]=0;x[:613,2]=.8
        with torch.no_grad():latent,z,old,_=model.sample(x)
        adv=aligned.advantages(torch.randn(1024),x[:,2]>0);params=parameters(model)[1]
        actor=model(x)[0];ratio=(model.log_prob(actor,latent,z,x)-old).exp()
        full=gradient(aligned.actor_losses(ratio,adv,torch.zeros_like(adv),x[:,2]>0,c)[0],params)
        summed=torch.zeros_like(full)
        for idx in torch.arange(1024).split(256):
            actor=model(x[idx])[0];ratio=(model.log_prob(actor,latent[idx],z[idx],x[idx])-old[idx]).exp()
            move=x[idx,2]>0; surrogate=ratio*adv[idx]
            loss=-.5*(surrogate[move].sum()/613+surrogate[~move].sum()/411)
            summed+=gradient(loss,params)
        torch.testing.assert_close(full,summed,atol=1e-6,rtol=1e-5)

    def test_gradient_partition_evidence_matches_full_group_weighting(self):
        from .effective_batch_analysis import partition_gradients
        with tempfile.TemporaryDirectory() as d:
            torch.manual_seed(29);c=stage.config(29,'large',True);model=make(29,c)
            collector=Collector(SyntheticBridge(),stage.training_candidates(c),Path(d)/'traces',stage.collector_pools(c))
            r=stage.collect(collector,model,c,time.monotonic()+30)
            rng=torch.get_rng_state().clone();state=copy.deepcopy(model.state_dict())
            result=partition_gradients(model,r,73)
            self.assertTrue(torch.equal(rng,torch.get_rng_state()));self.assertTrue(equal_tree(state,model.state_dict()))
            for size in ['256','1024']:
                counts=torch.tensor(result[size]['movement_counts'],dtype=torch.float32)
                weighted=(result[size]['gradients']*counts[:,None]).sum(0)/counts.sum()
                torch.testing.assert_close(weighted,result['6144']['gradients'][0],atol=1e-6,rtol=1e-4)

    def test_resume_rejects_cross_arm_before_restoration(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'checkpoint.pt.gz';storage.dump_torch(p,{'contract':{'config':stage.config(17,'current')}})
            with self.assertRaisesRegex(ValueError,'mismatch'):
                restore(p,{'config':stage.config(17,'large')},None,Path(d)/'unused',stage)
            self.assertFalse((Path(d)/'unused').exists())

    def test_invalid_config_rejected(self):
        with self.assertRaises(ValueError):stage.config(99,'large')
        with self.assertRaises(ValueError):stage.config(17,'undeclared')


if __name__=='__main__':unittest.main(verbosity=2)
