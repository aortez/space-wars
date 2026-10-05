import copy
import importlib.util
import math
from pathlib import Path
import unittest


spec = importlib.util.spec_from_file_location(
    'diagnosis', Path(__file__).parents[1] / 'diagnose-integrated-regression.py')
D = importlib.util.module_from_spec(spec)
spec.loader.exec_module(D)


def trace(tick, seat):
    return dict(tick=tick, seat=seat, observation=dict(local=dict(combat=dict(
        recovery=dict(flight=dict(pilot=dict(tick=tick, owner=f'player_{seat+1}')))))))


def firing_observation():
    motion = dict(position=dict(x=0., y=0.), velocity=dict(x=0., y=0.), angle=0.)
    pilot = dict(ship=motion, controls_armed=True, queries_ready=True, ship_available=True,
                 ship_form='ship', location=dict(aboard=0))
    target = dict(motion=copy.deepcopy(motion), visible=True, ground_occluded=False)
    target['motion']['position']['y'] = 100.
    combat = dict(target=target, laser_available=True, cannon_ready=True)
    return pilot, combat


class IntegratedRegressionTests(unittest.TestCase):
    def test_sparse_prior_is_exact_subset_and_new_trace_must_be_dense(self):
        dense = [trace(t, s) for t in range(3) for s in (0, 1)]
        prior = [dense[0], dense[3], dense[-1]]
        self.assertEqual(list(D.checked_trace(iter(dense), iter(prior), 3)), dense)
        for bad in (dense[:-1], dense[:2] + dense[3:], dense + dense[-1:], dense[::-1]):
            with self.assertRaises(AssertionError):
                list(D.checked_trace(iter(bad), iter(prior), 3))

    def test_changed_old_observation_or_actor_is_rejected(self):
        dense = [trace(0, s) for s in (0, 1)]
        changed = copy.deepcopy(dense)
        changed[0]['actions'] = ['changed']
        with self.assertRaises(AssertionError):
            list(D.checked_trace(iter(changed), iter(dense), 1))
        dense[1]['observation']['local']['combat']['recovery']['flight']['pilot']['owner'] = 'player_1'
        with self.assertRaises(AssertionError):
            list(D.checked_trace(iter(dense), iter(dense), 1))

    def test_only_timing_measurements_and_declared_trace_setting_can_change(self):
        before = dict(dense_trace_ticks=[0, 0], sensors=dict(count=2, mean_ms=1.),
                      outcome='win', health=80., elapsed_ticks=600,
                      planning=dict(queries=384, snapshot_total_ms=3.))
        after = copy.deepcopy(before)
        after['dense_trace_ticks'] = [0, D.END]
        after['sensors']['mean_ms'] = 99.
        after['planning']['snapshot_total_ms'] = 100.
        D.compare_reports(before, after)
        for key, value in [('outcome', 'loss'), ('health', 79.), ('elapsed_ticks', 601)]:
            bad = dict(after, **{key: value})
            with self.assertRaises(AssertionError):
                D.compare_reports(before, bad)
        after['sensors']['count'] = 3
        with self.assertRaises(AssertionError):
            D.compare_reports(before, after)

    def test_replay_preserves_every_behavior_flag_and_forbids_shadowing_diagnostics(self):
        prior = dict(command=['frozen-binary', '--seed', '42', '--trace', 'true', '--out', 'old'])
        command = D.command(prior, Path('new'))
        self.assertEqual(command[:7], ['frozen-binary', '--seed', '42', '--trace', 'true', '--out', 'new'])
        self.assertEqual(command[command.index('--impact-pod-control')+1], 'bot')
        for flag in ('--trace-end-tick', '--impact-pod-control'):
            with self.assertRaises(AssertionError):
                D.command(dict(command=prior['command'] + [flag, '0']), Path('new'))

    def test_current_aim_window_keeps_independent_safety_and_weapon_blockers(self):
        pilot, combat = firing_observation()
        self.assertEqual(D.gates(pilot, combat, 'cannon')['blockers'], [])
        pilot['controls_armed'] = False
        pilot['location'] = 'on_foot'
        combat['target']['visible'] = False
        combat['cannon_ready'] = False
        result = D.gates(pilot, combat, 'cannon')
        self.assertEqual(result['blockers'], ['controls_or_queries_unready', 'not_aboard_full_ship',
                                             'visibility', 'readiness'])
        combat['target'] = None
        self.assertEqual(D.gates(pilot, combat, 'laser')['blockers'], ['no_target'])

    def test_float_adjacent_boundaries_remain_indeterminate(self):
        pilot, combat = firing_observation()
        combat['target']['motion']['position']['y'] = 220.
        self.assertTrue(D.gates(pilot, combat, 'cannon')['uncertain'])
        combat['target']['motion']['position'] = dict(x=100.*math.sin(.08), y=100.*math.cos(.08))
        self.assertTrue(D.gates(pilot, combat, 'laser')['uncertain'])
        combat['target']['motion']['position'] = dict(x=0., y=100.)
        self.assertFalse(D.gates(pilot, combat, 'laser')['uncertain'])

    def test_loss_provenance_does_not_join_stale_contact_or_future_spawn(self):
        row = dict(tick=900, damage=dict(last_damage_tick=900, last_ship_lost=True,
                   last_source='cannon', last_damage_percent=20., last_contact_tick=900,
                   last_contact_source='cannon', last_contact_spawn_tick=880))
        self.assertEqual(D.loss_receipt(row, 900)['contact_age_ticks'], 20)
        row['damage']['last_contact_tick'] = 899
        self.assertNotIn('spawn_tick', D.loss_receipt(row, 900))
        row['damage']['last_contact_tick'] = 900
        row['damage']['last_contact_spawn_tick'] = 901
        with self.assertRaises(AssertionError):
            D.loss_receipt(row, 900)
        row['damage']['last_damage_tick'] = 899
        with self.assertRaises(AssertionError):
            D.loss_receipt(row, 900)

    def test_phase_counts_require_contiguous_ticks_and_keep_suppression_separate(self):
        def point(tick, mode):
            return dict(tick=tick, mode=mode, action=dict(laser=False, cannon=False),
                        gates={w: dict(blockers=[], uncertain=False) for w in ('laser', 'cannon')})
        points = [point(10, 'combat:flyby / weapons off'), point(11, 'combat:flyby / weapons off'),
                  point(12, 'mission:recover')]
        phases = D.phase_summary(points)
        self.assertEqual((phases[0]['start'], phases[0]['end']), (10, 12))
        self.assertEqual(phases[0]['counts']['laser_window'], 2)
        self.assertEqual(phases[0]['counts']['laser_requests'], 0)
        with self.assertRaises(AssertionError):
            D.phase_summary([points[0], dict(points[1], tick=13)])
        with self.assertRaises(AssertionError):
            D.phase_summary([points[0], dict(points[2], tick=13)])


if __name__ == '__main__':
    unittest.main()
