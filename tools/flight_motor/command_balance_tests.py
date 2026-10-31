"""LF-067 scheduler isolation, six-slot targets and four-slot replay regression."""
import copy
from pathlib import Path
import tempfile
import time
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from . import command_balance as stage
from . import gae_comparison, sustained, storage
from .collector import Collector
from .gae_audit import discounted_targets
from .gae_tests import SyntheticBridge
from .model import MotorModel
from .movement_run import CompressedCollector, restore

PARENT = Path('data/training-experiments/learned-flight-motor-gae-v1-20261025')


def make(seed, arm):
    state = storage.load_torch(PARENT / f'inputs/seed-{seed}/checkpoint-0064.pt')
    model = stage.make_model(state['contract']['schema'], stage.config(seed, arm))
    stage.initialize(model, PARENT / f'inputs/seed-{seed}/checkpoint-0064.pt', stage.config(seed, arm))
    return model


class CommandBalanceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)

    def test_only_scheduler_differs_between_arms(self):
        for seed in stage.SEEDS:
            for pre in [False, True]:
                a = stage.config(seed, 'cyclic', pre); b = stage.config(seed, 'balanced', pre)
                for key in ['arm', 'collector_schedule']:
                    a.pop(key); b.pop(key)
                self.assertEqual(a, b)
                old = gae_comparison.config(seed, 'lambda1', pre)
                for key in ['kind', 'arm', 'updates']:
                    a.pop(key, None); old.pop(key)
                a.pop('collector_slots')
                self.assertEqual(a, old)

    def test_pools_partition_unchanged_tasks_and_preserve_stopping_schedule(self):
        c = stage.config(43, 'cyclic'); b = stage.config(43, 'balanced')
        original = sustained.training_candidates(sustained.config(43, 'sustain'))
        self.assertEqual(stage.training_candidates(c), original)
        cp = stage.collector_pools(c); bp = stage.collector_pools(b)
        self.assertEqual([cp[i] for i in [1, 3, 5]], [bp[i] for i in [1, 3, 5]])
        self.assertEqual([len(x) for x in bp], [8] * 6)
        self.assertEqual(sorted(r['index'] for pool in bp for r in pool), list(range(48)))
        for i, speed in zip([0, 2, 4], [4, 10, 16]):
            self.assertEqual({r['task']['speed'] for r in bp[i]}, {speed})
        self.assertEqual([cp[i][0]['task']['speed'] for i in [0, 2, 4]], [4, 4, 4])

    def test_preflights_are_paired_and_exercise_all_commands(self):
        one = stage.collector_pools(stage.config(17, 'cyclic', True))
        two = stage.collector_pools(stage.config(17, 'balanced', True))
        self.assertEqual(one, two)
        self.assertEqual([p[0]['task']['speed'] for p in one], [4, 0, 10, 0, 16, 0])

    def test_same_initial_model_and_rng(self):
        for seed in stage.SEEDS:
            torch.manual_seed(seed); a = make(seed, 'cyclic'); rng = torch.get_rng_state().clone()
            torch.manual_seed(seed); b = make(seed, 'balanced')
            self.assertTrue(equal_tree(a.state_dict(), b.state_dict()))
            self.assertTrue(torch.equal(rng, torch.get_rng_state()))

    def test_six_slot_collector_targets_and_balanced_exposure(self):
        with tempfile.TemporaryDirectory() as d:
            torch.manual_seed(43); c = stage.config(43, 'balanced'); model = make(43, 'balanced')
            collector = Collector(SyntheticBridge(), stage.training_candidates(c), Path(d) / 'traces', stage.collector_pools(c))
            rollout = collector.collect(model, c, time.monotonic() + 30)
            self.assertEqual(tuple(rollout['observations'].shape), (256, 6, 18))
            self.assertEqual(collector.steps, 1536)
            for command in [4, 10, 16]:
                self.assertEqual(int(((rollout['observations'][:, :, 2] * 20 - command).abs() < 1e-5).sum()), 256)
            with torch.no_grad():
                last = model(torch.tensor([i['inputs'] for i in collector.items]))[1]
            expected = discounted_targets(torch.full((256, 6), -(2/5)**2/60), torch.zeros(256, 6), last, .99)
            torch.testing.assert_close(rollout['returns'], expected, atol=1e-5, rtol=1e-5)

    def test_real_six_slot_update_preserves_original_stopping(self):
        with tempfile.TemporaryDirectory() as d:
            torch.manual_seed(43); c = stage.config(43, 'balanced'); model = make(43, 'balanced')
            collector = Collector(SyntheticBridge(), stage.training_candidates(c), Path(d) / 'traces', stage.collector_pools(c))
            rollout = collector.collect(model, c, time.monotonic() + 30); before = copy.deepcopy(model.state_dict())
            optimizer = torch.optim.Adam(model.parameters(), lr=c['learning_rate'], eps=1e-5)
            metrics = stage.update(model, optimizer, rollout, c)
            self.assertEqual(metrics['movement_rows'], 768); self.assertEqual(metrics['stopping_rows'], 768)
            stage.assert_parent_actor(before, model.state_dict())
            self.assertFalse(equal_tree(before, model.state_dict()))
            x = torch.randn(1536, 18); x[:, 2] = 0
            with torch.no_grad():
                self.assertTrue(torch.equal(model(x)[0], MotorModel.forward(model, x)[0]))

    def test_invalid_pool_partition_and_resume_rejected_before_mutation(self):
        with tempfile.TemporaryDirectory() as d:
            c = stage.config(17, 'balanced'); rows = stage.training_candidates(c); pools = stage.collector_pools(c)
            pools[0] = pools[2]
            with self.assertRaisesRegex(ValueError, 'partition'):
                Collector(None, rows, Path(d) / 'unused', pools)
            self.assertFalse((Path(d) / 'unused').exists())
            checkpoint = Path(d) / 'bad.pt.gz'
            storage.dump_torch(checkpoint, {'contract': {'config': c}, 'collector': {'pools': pools}})
            with self.assertRaisesRegex(ValueError, 'pools mismatch'):
                restore(checkpoint, {'config': c}, None, Path(d) / 'unused', stage)

    def test_undeclared_configs_rejected(self):
        with self.assertRaises(ValueError): stage.config(42, 'balanced')
        with self.assertRaises(ValueError): stage.config(43, 'random')

    def test_four_slot_native_replay_matches_LF065_update128(self):
        # Retain physical regression evidence separately from the scored experiment.
        evidence = Path('/tmp/motor-command-balance-tests'); evidence.mkdir(exist_ok=True)
        directory = Path(tempfile.mkdtemp(prefix='legacy-replay-', dir=evidence))
        run = PARENT / 'pilot/lambda1/seed-43'
        previous = storage.load_torch(run / 'checkpoint-0127.pt.gz')
        final = storage.load_torch(run / 'checkpoint-0128.pt.gz'); c = previous['contract']['config']
        with torch.random.fork_rng(devices=[]):
            model = gae_comparison.make_model(previous['contract']['schema'], c); model.load_state_dict(previous['model'])
            optimizer = torch.optim.Adam(model.parameters(), lr=c['learning_rate'], eps=1e-5); optimizer.load_state_dict(previous['optimizer'])
            with Bridge(PARENT / 'runtime/flight_motor_bridge') as bridge:
                collector = CompressedCollector.restore(bridge, previous['collector'], directory / 'traces')
                torch.set_rng_state(previous['torch_rng'])
                rollout = collector.collect(model, c, time.monotonic() + 60)
                storage.dump_torch(directory / 'rollout.pt.gz', rollout)
                self.assertTrue(equal_tree(rollout, storage.load_torch(run / 'rollout-0128.pt.gz')))
                metrics = sustained.update(model, optimizer, rollout, c)
                self.assertTrue(equal_tree(model.state_dict(), final['model']))
                self.assertTrue(equal_tree(optimizer.state_dict(), final['optimizer']))
                self.assertTrue(equal_tree(collector.state(), final['collector']))
                self.assertTrue(torch.equal(torch.get_rng_state(), final['torch_rng']))
                self.assertTrue(all(v == final['history'][-1][k] for k, v in metrics.items()))
                storage.dump_torch(directory / 'replayed-state.pt.gz', {'model': model.state_dict(), 'optimizer': optimizer.state_dict(),
                                  'collector': collector.state(), 'torch_rng': torch.get_rng_state(), 'metrics': metrics})
                atomic_json(directory / 'results.json', {'passed': True, 'source': str(run), 'training_ticks': 1024,
                    'optimizer_steps': metrics['minibatches'], 'restore_ticks': sum(len(t['steps']) for t in previous['collector']['traces']),
                    'neutral_setup_ticks': 4 + collector.started - previous['collector']['started']})


if __name__ == '__main__':
    unittest.main(verbosity=2)
