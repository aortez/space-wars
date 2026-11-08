"""Archived-prefix, sampled-stream, model-identity and finding-rule checks."""
import copy
from pathlib import Path
import time
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from .duration_execution import aggregate, finding, frozen_export, read, validate_rows
from .execution import episode_seed, rollout
from .learning_gain import paired_innovations
from .steering import rollout as mixed_rollout

ROOT = Path('data/training-experiments/learned-flight-mixed-duration-v1-20261023')
OLD = Path('data/training-experiments/learned-flight-action-execution-v1-20261023')


def rows(count):
    return [{'repeat': repeat, 'index': index, 'rng_seed': episode_seed(17, repeat, index),
             'recipe': f'aligned-approach-v1/stratum-{index//4}',
             'outcome': 'success' if index < count else 'deadline'}
            for repeat in range(4) for index in range(16)]


class DurationExecutionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
        cls.bridge = Bridge(ROOT/'runtime/flight_ppo_bridge')
        cls.old = frozen_export(ROOT/'inputs/seed-17', 17, 128, cls.bridge)
        cls.new = frozen_export(ROOT/'seed-17', 17, 256, cls.bridge)
        batch = read(ROOT/'seed-17/development-admission.json')
        cls.candidate = next(r['candidate'] for r in batch['attempts'] if r['admitted'])

    @classmethod
    def tearDownClass(cls):
        cls.bridge.close()

    def test_export_tensor_and_declared_identity_guards(self):
        for model, directory, update in [(self.old, ROOT/'inputs/seed-17', 128), (self.new, ROOT/'seed-17', 256)]:
            state = torch.load(directory/f'checkpoint-{update:04}.pt', weights_only=True)
            self.assertTrue(all(torch.equal(v, state['model'][k]) for k, v in model.state_dict().items()))
            self.assertFalse(model.training);self.assertTrue(all(not p.requires_grad for p in model.parameters()))
        with self.assertRaises(ValueError):frozen_export(ROOT/'seed-17', 17, 64, self.bridge)
        with self.assertRaises(ValueError):frozen_export(ROOT/'seed-17', 29, 256, self.bridge)

    def test_archived_prefix_repeatability_and_paired_streams(self):
        candidate = self.candidate;rng = episode_seed(17, 0, candidate['index'])
        before = {k: v.clone() for k, v in self.new.state_dict().items()}
        caller_rng = torch.get_rng_state()
        with torch.random.fork_rng(devices=[]):
            a = rollout(self.bridge, self.old, candidate, 'sampled', rng, time.monotonic()+30, 32)
            b = rollout(self.bridge, self.old, candidate, 'sampled', rng, time.monotonic()+30, 32)
            c = rollout(self.bridge, self.new, candidate, 'sampled', rng, time.monotonic()+30, 32)
            d = rollout(self.bridge, self.new, candidate, 'sampled', rng, time.monotonic()+30, 32)
            e = mixed_rollout(self.bridge, self.new, candidate, rng, time.monotonic()+30, 32)
        self.assertTrue(torch.equal(caller_rng, torch.get_rng_state()))
        self.assertEqual(a, b);self.assertEqual(c, d)
        stem = f"sampled-r0-clear-{candidate['index']}.json.gz"
        self.assertEqual(a[0]['steps'], read(OLD/'seed-17/traces'/stem)['steps'][:32])
        self.assertEqual(a[1], read(OLD/'seed-17/policy'/stem)[:32])
        directory = ROOT/'seed-17/evaluation-final/repeat-0'
        self.assertEqual(e[0]['steps'], read(directory/f"clear-{candidate['index']}.json")['steps'][:32])
        self.assertEqual(e[1], read(directory/'policy'/f"clear-{candidate['index']}.json.gz")[:32])
        paired_innovations(a[1], c[1]);paired_innovations(c[1], e[1])
        corrupted = copy.deepcopy(c[1]);corrupted[0]['latent'][1] += .1
        with self.assertRaises(AssertionError):paired_innovations(a[1], corrupted)
        self.assertTrue(all(torch.equal(v, before[k]) for k, v in self.new.state_dict().items()))

    def test_direction_and_material_thresholds_are_separate(self):
        cells = {'sampled_128': rows(2), 'sampled_256': rows(4), 'mixed_128': rows(14), 'mixed_256': rows(10)}
        result = finding(cells)
        self.assertEqual(result['direction'], 'opposite_changes')
        self.assertTrue(result['material_opposing_change'])
        cells['sampled_256'] = rows(3)
        self.assertFalse(finding(cells)['material_opposing_change'])
        cells['sampled_256'] = rows(1)
        self.assertEqual(finding(cells)['direction'], 'both_declined')
        cells['sampled_256'] = rows(2)
        self.assertEqual(finding(cells)['direction'], 'sampled_unchanged_mixed_declined')
        cells['sampled_256'] = rows(2)
        for row in cells['sampled_256']:
            if row['repeat'] < 2 and row['index'] < 6:row['outcome'] = 'success'
        self.assertEqual(finding(cells)['sampled_delta'], 8)
        self.assertFalse(finding(cells)['material_opposing_change'])

    def test_incomplete_unpaired_and_single_seed_findings_cannot_pass(self):
        with self.assertRaises(ValueError):validate_rows(rows(2)[:-1])
        invalid = rows(2);invalid[0]['outcome'] = None
        with self.assertRaises(ValueError):validate_rows(invalid)
        cells = {'sampled_128': rows(2), 'sampled_256': rows(4), 'mixed_128': rows(14), 'mixed_256': rows(10)}
        cells['sampled_256'][0]['rng_seed'] += 1
        with self.assertRaises(AssertionError):finding(cells)
        record = {'material_opposing_change': True, 'direction': 'opposite_changes', 'sampled_delta': 8}
        records = [{'seed': seed, 'finding': dict(record, material_opposing_change=i == 0)} for i, seed in enumerate([17,29,43])]
        self.assertFalse(aggregate(records)['material_opposing_change'])
        records[1]['finding']['material_opposing_change'] = True
        self.assertTrue(aggregate(records)['material_opposing_change'])
        with self.assertRaises(ValueError):aggregate(records[:-1])


if __name__ == '__main__':
    unittest.main(verbosity=2)
