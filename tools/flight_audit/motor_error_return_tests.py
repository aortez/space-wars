"""LF-071 saved-state mapping, cutoff credit and parameterized native branching."""
from pathlib import Path
import time
import unittest

import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from . import motor_action_return as base
from . import motor_error_return as probe
from .motor_action_return_tests import ConstantModel, FakeBridge, trace

PARENT = Path('data/training-experiments/learned-flight-motor-error-gradient-v1-20261025')


class ErrorReturnTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1); torch.use_deterministic_algorithms(True)

    def test_selection_keeps_missing_and_small_groups_explicit(self):
        self.assertEqual(probe.selected_rows(torch.zeros(4, dtype=torch.bool)), [])
        self.assertEqual(probe.selected_rows(torch.tensor([False, True])), [1])
        self.assertEqual(probe.selected_rows(torch.tensor([True, False, True])), [0, 2])
        self.assertEqual(probe.selected_rows(torch.ones(6, dtype=torch.bool)), [2, 4])

    def test_cutoff_is_included_without_duplicate_horizons(self):
        self.assertEqual(probe.horizons({'rollout_remaining': 73}), [1, 30, 73, 120, 256])
        self.assertEqual(probe.horizons({'rollout_remaining': 30}), [1, 30, 120, 256])

    def test_lambda_one_telescopes_and_differs_from_legacy_credit(self):
        t = trace(3); values = [-.5, -.4, .9, -.2]
        new = base.credits(t, 0, values, horizons=[1, 2, 3], gamma=.99, lam=1.)
        old = base.credits(t, 0, values, horizons=[1, 2, 3])
        for row in new:
            self.assertAlmostEqual(row['gae_first'], row['bootstrapped_return']-values[0], places=6)
        self.assertNotAlmostEqual(new[-1]['gae_first'], old[-1]['gae_first'], places=4)

    def test_custom_batch_removes_failed_branch_without_repeating_penalty(self):
        counts = {k: 0 for k in ['matched_initial_states', 'restore_ticks', 'neutral_setups',
                                  'probe_ticks', 'native_python_comparisons']}
        variants = [base.VARIANTS[0], base.VARIANTS[2]]
        rows = base.batch(FakeBridge(), ConstantModel(), trace(0), None, counts, time.monotonic()+10,
            horizons=[1, 4], variants=variants, lam=1.)
        self.assertEqual(counts['probe_ticks'], 5)
        self.assertEqual(counts['restore_ticks'], 1)
        self.assertEqual(counts['neutral_setups'], 3)
        self.assertEqual(rows[1]['metrics'][1]['performed_ticks'], 1)
        self.assertEqual(rows[1]['metrics'][1]['bootstrap'], 0.)
        self.assertEqual(rows[1]['metrics'][0]['observed_return'], rows[1]['metrics'][1]['observed_return'])
        self.assertEqual(rows[0]['metrics'][1]['performed_ticks'], 4)

    def test_all_frozen_intervals_map_exactly_and_keep_readiness_gaps(self):
        selected = {}; missing = []
        for seed in probe.SEEDS:
            source = PARENT/f'inputs/speed16/seed-{seed}'
            selected[seed] = 0
            for u in probe.UPDATES:
                b = probe.load(source/f'checkpoint-{u-1:04}.pt.gz')
                a = probe.load(source/f'checkpoint-{u:04}.pt.gz')
                r = probe.load(source/f'rollout-{u:04}.pt.gz')
                sources = probe.interval_sources(source, b, a, r)
                masks = probe.region.regions(r['observations'].flatten(0, 1))[1]
                for side in ['under', 'over']:
                    ids = probe.selected_rows(masks['ready_'+side])
                    if not ids: missing.append((seed, u, side))
                    for index in ids:
                        full, tick, _, _ = sources[index]
                        self.assertGreater(len(full['steps']), tick)
                        base.prefix(full, tick)
                    selected[seed] += len(ids)
        self.assertEqual(selected, {17: 8, 29: 16, 43: 16})
        self.assertIn((17, 16, 'under'), missing)
        self.assertIn((17, 16, 'over'), missing)
        self.assertIn((29, 86, 'over'), missing)

    def test_native_training_prefix_and_original_next_action_are_exact(self):
        source = PARENT/'inputs/speed16/seed-29'
        b = probe.load(source/'checkpoint-0015.pt.gz'); a = probe.load(source/'checkpoint-0016.pt.gz')
        r = probe.load(source/'rollout-0016.pt.gz')
        sources = probe.interval_sources(source, b, a, r)
        masks = probe.region.regions(r['observations'].flatten(0, 1))[1]
        index = probe.selected_rows(masks['ready_over'])[0]
        full, tick, _, _ = sources[index]
        before, after = base.prefix(full, tick), base.prefix(full, tick+1)
        with Bridge(PARENT/'runtime/flight_motor_bridge') as bridge:
            item = bridge.call('restore', traces=[before])['items'][0]
            expected = before['steps'][-1]['item'] if tick else before['initial']
            self.assertEqual(item, expected)
            got = bridge.call('step', actions=[full['steps'][tick]['action']])['items'][0]
            self.assertEqual(got, full['steps'][tick]['item'])
            self.assertEqual(bridge.call('state')['traces'][0], after)
            self.assertEqual(bridge.call('restore', traces=[after])['items'][0], got)
        atomic_json(Path('/tmp/motor-error-return-native-test.json'), {'new_ticks': 1,
            'replay_ticks': 2*tick+1, 'neutral_setups': 2, 'row': index, 'tick': tick, 'trace': after})


if __name__ == '__main__': unittest.main()
