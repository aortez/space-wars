"""Velocity shaping isolation, terminal invariants and exact restart preflight."""
import json
import math
from pathlib import Path
import unittest
import torch
from .bridge import Bridge
from .collector import Collector
from .curriculum import generate
from .model import FlightModel
from .reward import potential, reward
from . import tests


class VelocityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        torch.set_num_threads(1)
        torch.use_deterministic_algorithms(True)
        cls.bridge = Bridge(tests.BRIDGE)
        cls.pools = generate(cls.bridge, 4110001, 1)[1]

    @classmethod
    def tearDownClass(cls):
        cls.bridge.close()

    def config(self, weight):
        config = json.loads((Path(__file__).parent/'clear-control-config.json').read_text())
        config.update(updates=2, rollout_ticks=8, epochs=2, minibatch=16,
                      train_tasks_per_stratum=1, velocity_shaping_weight=weight,
                      exploration={"kind":"continuous-gaussian-ar1-v2", "rho":0.0})
        return config

    def test_control_is_exact_legacy_formula_and_only_velocity_changes(self):
        for distance, speed, spin in [(0.,0.,0.), (20.,1.,.1), (80.,6.,.8), (300.,70.,3.)]:
            frame = {"errors":{"position":distance, "velocity":speed, "spin":spin}}
            legacy = -.5*math.tanh(distance/100)-.25*math.tanh(speed/20)-.25*math.tanh(spin)
            self.assertEqual(potential(frame), legacy)
            self.assertEqual(potential(frame, .25), legacy)
            self.assertAlmostEqual(potential(frame, 0)-legacy, .25*math.tanh(speed/20), places=14)
        for invalid in [-.1, float('nan'), float('inf')]:
            with self.assertRaises(ValueError):
                potential(frame, invalid)

    def test_both_rewards_telescope_and_preserve_terminal_and_time_terms(self):
        frames = [{"errors":{"position":d,"velocity":v,"spin":s}}
                  for d,v,s in [(80,0,0),(90,10,1),(40,15,.2),(2,.5,.1)]]
        outcomes = {"success":10., "deadline":-2., "collision":-10., "damage":-10.,
                    "ship_lost":-10., "boundary_escape":-10., "controller_error":-10.}
        gamma = .9995
        for weight in [.25, 0.]:
            for outcome, terminal in outcomes.items():
                terms = [reward({"frame":frames[i]}, {"frame":frames[i+1],
                         "progress":{"outcome":outcome if i==2 else None}}, gamma, weight) for i in range(3)]
                self.assertAlmostEqual(sum(gamma**i*t['potential'] for i,t in enumerate(terms)), -potential(frames[0],weight), places=14)
                self.assertEqual([t['time'] for t in terms], [-.001]*3)
                self.assertEqual([t['terminal'] for t in terms], [0.,0.,terminal])
            with self.assertRaises(RuntimeError):
                reward({"frame":frames[0]}, {"frame":frames[1],"progress":{"outcome":"environment_error"}}, gamma, weight)

    def test_collector_changes_reward_without_changing_first_rollout_actions(self):
        results=[]
        for weight in [.25, 0.]:
            torch.manual_seed(17)
            collector=Collector(self.bridge,self.pools)
            model=FlightModel(self.bridge.contract['schema'])
            rollout=collector.collect(model,self.config(weight))
            results.append((rollout,collector.state()))
        a,b=results
        self.assertEqual(a[1]['traces'],b[1]['traces'])
        for key in a[0]:
            if key not in ('advantages','returns'):
                self.assertTrue(torch.equal(a[0][key],b[0][key]),key)
        self.assertFalse(torch.equal(a[0]['advantages'],b[0]['advantages']))
        for ea,eb in zip(a[1]['active'],b[1]['active']):
            self.assertEqual(ea['reward']['terminal'],eb['reward']['terminal'])
            self.assertEqual(ea['reward']['time'],eb['reward']['time'])

    def test_both_weights_resume_exactly_in_fresh_processes(self):
        for weight in [.25,0.]:
            with self.subTest(weight=weight):
                tests.PolicyTests.check_continuation(self,self.config(weight))


if __name__ == '__main__':
    unittest.main(verbosity=2)
