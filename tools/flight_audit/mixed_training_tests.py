"""Boundary/coverage tests for the frozen repeated-trial experiment criteria."""
import copy
import unittest
from pathlib import Path
import torch
from .mixed_training import SPEC, MIXED, aggregate, episode_seed, gates, paired, physical, read, trace_metrics, training_equal


def rows(success=lambda repeat, index: False):
    return [{'repeat': repeat, 'index': index, 'recipe': f'aligned-approach-v1/stratum-{index//4}',
             'execution_mode': MIXED, 'rng_seed': episode_seed(SPEC, 17, repeat, index),
             'outcome': 'success' if success(repeat, index) else 'deadline', 'ticks': 100,
             'initial_errors': {'position': 5., 'velocity': 0., 'spin': 0.},
             'final_errors': {'position': 2., 'velocity': .5, 'spin': .1},
             'collision': False, 'damage': False} for repeat in range(4) for index in range(16)]


class MixedTrainingTests(unittest.TestCase):
    def test_progression_requires_success_gain_coverage_repeats_and_safety(self):
        initial = rows()
        final = rows(lambda r, i: i % 4 < 3)
        self.assertTrue(gates(initial, initial, final, 17)['scoped_progression'])
        one_less = copy.deepcopy(final);one_less[0]['outcome'] = 'deadline'
        self.assertFalse(gates(initial, initial, one_less, 17)['scoped_progression'])
        no_far = rows(lambda r, i: i < 12)
        self.assertFalse(gates(initial, initial, no_far, 17)['scoped_progression'])
        uneven_repeats = rows(lambda r, i: r < 2 or i % 4 < 2)
        self.assertEqual(physical(uneven_repeats, 17)['successes'],48)
        self.assertFalse(gates(initial, initial, uneven_repeats, 17)['scoped_progression'])
        unsafe = copy.deepcopy(final);unsafe[0]['damage'] = True
        self.assertFalse(gates(initial, initial, unsafe, 17)['scoped_progression'])
        too_good_initial = rows(lambda r, i: i < 5)
        self.assertFalse(gates(too_good_initial, initial, final, 17)['scoped_progression'])

    def test_trial_identity_and_execution_guards(self):
        original = rows()
        for field, value in [('repeat', 5), ('index', 1), ('recipe', 'wrong'), ('rng_seed', 0),
                             ('execution_mode', 'deterministic-v1'), ('outcome', 'environment_error'), ('ticks', 0)]:
            changed = copy.deepcopy(original);changed[0][field] = value
            with self.assertRaises(ValueError):physical(changed, 17)
        with self.assertRaises(ValueError):physical(original[:-1], 17)
        changed = copy.deepcopy(original);changed[0]['final_errors']['position'] = float('nan')
        with self.assertRaises(ValueError):physical(changed, 17)

    def test_paired_gain_loss_is_directional_and_identity_checked(self):
        first, second = rows(lambda r, i: i < 4), rows(lambda r, i: 2 <= i < 6)
        self.assertEqual(paired(first,second),{'retained':8,'lost':8,'gained':8,'neither':40})
        second[0]['initial_errors']['position'] += 1
        with self.assertRaises(ValueError):paired(first,second)

    def test_aggregate_requires_declared_seeds_and_no_incomplete_runs(self):
        records = [{'seed':seed,'status':'completed','learning_evidence':True,'scoped_progression':i < 2,
                    'gain_from_128':8 if i < 2 else -8} for i,seed in enumerate([17,29,43])]
        self.assertTrue(aggregate(records)['scoped_progression'])
        self.assertTrue(aggregate(records)['duration_benefit'])
        records[-1]['gain_from_128'] = -9
        self.assertFalse(aggregate(records)['duration_benefit'])
        records[-1] = {'seed':43,'status':'incomplete_or_failed'}
        self.assertFalse(any(aggregate(records).values()))
        with self.assertRaises(ValueError):aggregate(records[:-1])

    def test_comparison_excludes_only_provenance_never_training_state(self):
        original = {'contract': {'updates':128},'model':{'weight':torch.tensor([1.])},'torch_rng':torch.tensor([2])}
        changed = copy.deepcopy(original);changed['contract']['updates'] = 256
        self.assertTrue(training_equal(changed, original))
        changed['torch_rng'][0] = 3
        self.assertFalse(training_equal(changed, original))

    def test_real_baseline_gate_and_success_trace_tolerances(self):
        root = Path('data/training-experiments/learned-flight-evaluation-integration-v1-20261023/seed-17/mixed')
        final = read(root/'results.json');initial = copy.deepcopy(final)
        for row in initial:row['outcome'] = 'deadline'
        result = gates(initial, final, final, 17)
        self.assertTrue(result['scoped_progression']);self.assertEqual(result['gain_from_128'],0)
        row = next(r for r in final if r['outcome'] == 'success')
        trace = read(root/f"repeat-{row['repeat']}/clear-{row['index']}.json")
        self.assertEqual(trace_metrics(trace)['maximum_dwell_ticks'],30)
        trace['steps'][-20]['frame']['errors']['spin'] = -.21
        with self.assertRaises(ValueError):trace_metrics(trace)


if __name__ == '__main__':
    unittest.main(verbosity=2)
