"""Aligned-start recipe, physical orientation, gate isolation and exact resume."""
import copy
import json
import math
from pathlib import Path
import unittest
import torch
from . import tests
from .bridge import Bridge
from .curriculum import ALIGNED_RECIPE, ALIGNED_STRATA, aligned_candidate, generate
from .learning_metrics import seed_gates


class ApproachTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)
        cls.bridge = Bridge(tests.BRIDGE)

    @classmethod
    def tearDownClass(cls):
        cls.bridge.close()

    def test_sampling_bounds_and_partition_separation(self):
        for index in range(128):
            c = aligned_candidate(4210001, index)
            self.assertEqual(c, aligned_candidate(4210001, index))
            self.assertNotEqual(c['task'], aligned_candidate(4210002, index)['task'])
            t = c['task']; low, high = ALIGNED_STRATA[index % 4]
            distance = math.hypot(*t['goal_offset'].values())
            self.assertTrue(low <= distance <= high)
            self.assertEqual(t['start']['relative_velocity'], {'x': 0., 'y': 0.})
            self.assertEqual(t['start']['spin'], 0.)
            self.assertEqual(t['arrival'], {'position': 3., 'velocity': 1., 'spin': .2, 'dwell_ticks': 30})
            self.assertEqual(t['deadline_ticks'], 1800)

    def test_admission_quota_and_rejection_preserve_state(self):
        before = self.bridge.call('state')
        batch, pools = generate(self.bridge, 4210001, 2, recipe=ALIGNED_RECIPE)
        self.assertTrue(batch['complete']); self.assertEqual([len(p) for p in pools], [2]*4)
        self.assertEqual(before, self.bridge.call('state'))
        for pool in pools:
            c = copy.deepcopy(pool[0]); c['task']['start']['heading_offset'] += .1
            self.assertFalse(self.bridge.call('inspect', candidate=c)['admitted'])
        limited, _ = generate(self.bridge, 4210001, 2, max_rounds=1, recipe=ALIGNED_RECIPE)
        self.assertFalse(limited['complete'])
        with self.assertRaises(ValueError):generate(self.bridge, 1, 1, recipe='unknown')

    def test_real_forward_alignment_and_motion_are_not_trivial_success(self):
        _, pools = generate(self.bridge, 4210099, 1, recipe=ALIGNED_RECIPE)
        tasks = [p[0]['task'] for p in pools]
        initial = self.bridge.call('reset', tasks=tasks)['items']
        for item in initial:
            f = item['frame']; goal = f['target_position']; com = f['center_of_mass']
            dx, dy = goal['x']-com['x'], goal['y']-com['y']
            distance = math.hypot(dx, dy)
            self.assertAlmostEqual(dx/distance, -math.sin(f['angle']), delta=2e-5)
            self.assertAlmostEqual(dy/distance, math.cos(f['angle']), delta=2e-5)
            self.assertGreater(f['errors']['position'], 3.)
            self.assertIsNone(item['progress']['outcome'])
        for _ in range(120):
            stopped = self.bridge.call('step', actions=[{'steering': 0., 'thrust': 0., 'brake': 1.}]*4)['items']
        for a, b in zip(initial, stopped):
            self.assertIsNone(b['progress']['outcome'])
            self.assertAlmostEqual(a['frame']['errors']['position'], b['frame']['errors']['position'], delta=.01)
        self.bridge.call('reset', tasks=tasks)
        for _ in range(20):
            moved = self.bridge.call('step', actions=[{'steering': 0., 'thrust': 1., 'brake': 0.}]*4)['items']
        for a, b in zip(initial, moved):
            self.assertLess(b['frame']['errors']['position'], a['frame']['errors']['position'])
            self.assertGreater(b['frame']['errors']['velocity'], 0.)

    def test_gates_reject_mixed_recipes_and_preserve_coverage(self):
        initial = [{'recipe': f'{ALIGNED_RECIPE}/stratum-{i}', 'outcome': 'deadline'} for i in range(4) for _ in range(4)]
        final = copy.deepcopy(initial)
        for i, r in enumerate(final):r['outcome'] = 'success' if i % 4 < 3 else 'deadline'
        self.assertTrue(seed_gates(initial, final, ALIGNED_RECIPE)['basic_flight_gate'])
        with self.assertRaises(ValueError):seed_gates(initial, final)
        final[0]['recipe'] = 'clear-control-v1/stratum-0'
        with self.assertRaises(ValueError):seed_gates(initial, final, ALIGNED_RECIPE)

    def test_new_recipe_resumes_exactly(self):
        config = json.loads((Path(__file__).parent/'clear-control-config.json').read_text())
        config.update(task_recipe=ALIGNED_RECIPE, train_task_seed=4210001, development_task_seed=4210002,
                      heldout_task_seed=4210003, updates=2, rollout_ticks=8, epochs=2, minibatch=16,
                      train_tasks_per_stratum=1, deadline_distance_weight=4., velocity_shaping_weight=.25,
                      exploration={'kind':'continuous-gaussian-ar1-v2', 'rho':0.})
        tests.PolicyTests.check_continuation(self, config)


if __name__ == '__main__':unittest.main(verbosity=2)
