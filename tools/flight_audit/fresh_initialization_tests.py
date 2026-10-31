"""Initialization isolation and physical qualification for the paired experiment."""
import copy
import json
from pathlib import Path
import unittest
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.initialization import initialize_policy
from tools.flight_ppo.model import FlightModel
from tools.flight_ppo.tests import BRIDGE, equal_tree
from tools.flight_audit.fresh_initialization import assert_control_replay, benefit

PARENT = Path('data/training-experiments/learned-flight-bearing-curriculum-v1-20261025')


class FreshInitializationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)

    def test_standard_fresh_and_preserved_warm_share_initial_rng_and_goal_branch(self):
        with Bridge(BRIDGE) as bridge:
            for seed in [17, 29, 43]:
                config = json.loads((PARENT/f'configs/curriculum/{seed}.json').read_text())
                original = torch.load(PARENT/f'pilot/curriculum/seed-{seed}/checkpoint-0000.pt', weights_only=True)
                states = []; rngs = []
                for warm in [True, False]:
                    with torch.random.fork_rng(devices=[]):
                        torch.manual_seed(seed)
                        model = FlightModel(bridge.contract['schema'], config['input_scaling'], config['architecture'])
                        standard = copy.deepcopy(model.state_dict())
                        before = torch.get_rng_state().clone()
                        current = copy.deepcopy(config)
                        path = PARENT/f'inputs/seed-{seed}/policy.json' if warm else None
                        if not warm: del current['warm_start_sha256']
                        initialize_policy(model, current, path)
                        self.assertTrue(torch.equal(before, torch.get_rng_state()))
                        self.assertTrue(equal_tree(model.state_dict(), original['model'] if warm else standard))
                        if not warm: self.assertTrue(torch.equal(model.log_std, torch.full((3,), -.5)))
                        states.append(copy.deepcopy(model.state_dict())); rngs.append(torch.get_rng_state())
                self.assertTrue(torch.equal(rngs[0], rngs[1]))
                self.assertTrue(torch.equal(rngs[0], original['torch_rng']))
                self.assertFalse(equal_tree(states[0], states[1]))
                for key in states[0]:
                    if key.startswith('goal_'): self.assertTrue(torch.equal(states[0][key], states[1][key]))

    def test_warm_start_is_never_implicitly_loaded_or_silently_omitted(self):
        with self.assertRaisesRegex(ValueError, 'frozen policy hash'):
            initialize_policy(None, {}, 'unused.json')
        with self.assertRaisesRegex(ValueError, 'policy required'):
            initialize_policy(None, {'warm_start_sha256': 'expected'}, None)

    def records(self):
        return [{'seed': seed, 'arm': arm,
                 'stage': {'initial': {'successes': 0}, 'final': {'successes': 14 if arm == 'fresh' else 9, 'left': 7, 'right': 7, 'physical_failures': 0}},
                 'legacy_hold': {'final': {'immediate_holds': 16, 'immediate_holds_by_stratum': [4]*4, 'physical_failures': 0}},
                 'primary': {'initial': {'successes': 2 if arm == 'fresh' else 10}, 'final': {'successes': 11, 'physical_failures': 0}},
                 'all_position_training_cases_completed': True}
                for seed in [17, 29, 43] for arm in ['warm', 'fresh']]

    def test_weaker_fresh_baseline_never_lowers_broad_recovery_floor(self):
        rows = self.records(); self.assertTrue(benefit(rows)['passed'])
        for r in rows:
            if r['arm'] == 'fresh': r['primary']['final']['successes'] = 9
        self.assertFalse(benefit(rows)['passed'])
        rows = self.records()
        for r in rows:
            if r['arm'] == 'fresh': r['primary']['initial']['successes'] = 12
        self.assertFalse(benefit(rows)['passed'])

    def test_qualification_requires_both_signs_gain_and_holding_in_same_seeds(self):
        rows = self.records()
        for r in rows:
            if r['arm'] == 'fresh' and r['seed'] == 17: r['stage']['final']['left'] = 5
            if r['arm'] == 'fresh' and r['seed'] == 43: r['legacy_hold']['final']['immediate_holds_by_stratum'][0] = 1
        self.assertFalse(benefit(rows)['passed'])
        for field in ['initial', 'paired']:
            rows = self.records()
            for r in rows:
                if field == 'initial' and r['arm'] == 'fresh': r['stage']['initial']['successes'] = 13
                if field == 'paired' and r['arm'] == 'warm': r['stage']['final']['successes'] = 13
            self.assertFalse(benefit(rows)['passed'])

    def test_safety_and_full_training_exposure_remain_required(self):
        for suite in ['primary', 'stage', 'legacy_hold']:
            rows = self.records()
            for r in rows:
                if r['arm'] == 'fresh': r[suite]['final']['physical_failures'] = 1
            self.assertFalse(benefit(rows)['passed'])
        rows = self.records()
        for r in rows:
            if r['arm'] == 'warm': r['all_position_training_cases_completed'] = False
        self.assertFalse(benefit(rows)['passed'])

    def test_control_replay_allows_no_source_config_rng_or_weight_normalization(self):
        old = {'contract': {'python_sources': {'run.py': 'same'}, 'config': {'stage': 0}},
               'model': {'x': torch.tensor([1.])}, 'rng': torch.tensor([2])}
        assert_control_replay(copy.deepcopy(old), old)
        for section, key, value in [('contract', 'python_sources', {'run.py': 'changed'}),
                                     ('contract', 'config', {'stage': 1}), ('model', 'x', torch.tensor([2.]))]:
            new = copy.deepcopy(old); new[section][key] = value
            with self.assertRaises(AssertionError): assert_control_replay(new, old)
        new = copy.deepcopy(old); new['rng'][0] = 3
        with self.assertRaises(AssertionError): assert_control_replay(new, old)


if __name__ == '__main__': unittest.main(verbosity=2)
