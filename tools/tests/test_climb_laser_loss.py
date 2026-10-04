import copy
import importlib.util
from pathlib import Path
import unittest


spec = importlib.util.spec_from_file_location(
    'loss', Path(__file__).parents[1] / 'diagnose-climb-laser-loss.py')
L = importlib.util.module_from_spec(spec)
spec.loader.exec_module(L)


def joined_rows():
    pilot = dict(tick=12, owner='player_1', ship={'angle': 0.}, ship_form='escape_pod',
                 location={'aboard': 0}, controls_armed=True, recovery={'ships_lost': 1})
    controls = dict(turn=0., thrust=False, brake=True)
    trace = dict(tick=12, seat=0, actions=['native'], controls=controls, mission={'goal': 'recover'},
        observation=dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=pilot)))),
                         match_context=dict(pilot_health=[95., 100.])))
    row = dict(schema=1, tick=12, seat=0, actions=['native'], controls=dict(controls, interact=False),
        bot_controls=dict(controls, interact=False), overridden=False, goal='recover',
        ship=pilot['ship'], form='escape_pod', location=pilot['location'], controls_armed=True,
        vitals=dict(health=95.), recovery={'task': 'braking'})
    return row, trace


class ClimbLaserLossTests(unittest.TestCase):
    def test_replay_keeps_all_behavior_flags_and_existing_dense_window(self):
        old = dict(command=['binary', '--seed', '42', '--pursuit-climb-laser-seats', '0',
                            '--trace-start-tick', '0', '--trace-end-tick', '36001', '--out', 'old'])
        cmd = L.command(old, Path('new'))
        self.assertEqual(cmd[:-8], old['command'][:-1] + ['new'])
        self.assertEqual(cmd[-8:], ['--trace-impact', 'true', '--impact-start-tick', '0',
                                   '--impact-end-tick', '36001', '--impact-pod-control', 'bot'])

    def test_duplicate_flags_or_prior_control_override_cannot_be_shadowed(self):
        old = ['binary', '--trace-start-tick', '0', '--trace-end-tick', '36001', '--out', 'old']
        for extra in (['--impact-pod-control', 'brake'], ['--trace-impact', 'false'],
                      ['--impact-control-from-tick', '100'], ['--out', 'other']):
            with self.assertRaises(AssertionError):
                L.command(dict(command=old + extra), Path('new'))
        with self.assertRaises(AssertionError):
            L.command(dict(command=[s if s != '36001' else '100' for s in old]), Path('new'))

    def test_report_parity_preserves_dense_window_charged_work_and_outcomes(self):
        before = dict(dense_trace_ticks=[0, L.END], elapsed_ticks=12, winner=0,
                      planning=dict(queries=384, count=2, mean_ms=1.))
        after = copy.deepcopy(before)
        after['planning']['mean_ms'] = 10.
        L.compare_reports(before, after)
        for key, value in [('dense_trace_ticks', [0, 0]), ('elapsed_ticks', 13), ('winner', 1)]:
            with self.assertRaises(AssertionError):
                L.compare_reports(before, dict(after, **{key: value}))
        after['planning']['queries'] += 1
        with self.assertRaises(AssertionError):
            L.compare_reports(before, after)

    def test_impact_uses_actor_loss_counter_not_task_telemetry(self):
        row, trace = joined_rows()
        self.assertEqual(L.join_impact(row, trace, (12, 0)), 1)
        row['recovery'] = None
        self.assertEqual(L.join_impact(row, trace, (12, 0)), 1)

    def test_impact_rejects_wrong_clock_actor_actions_motion_and_overrides(self):
        row, trace = joined_rows()
        for key, value in [('tick', 13), ('seat', 1), ('actions', ['other']),
                           ('ship', {'angle': .1}), ('overridden', True),
                           ('vitals', {'health': 94.}), ('form', 'ship'),
                           ('location', 'on_foot'), ('controls_armed', False)]:
            with self.assertRaises(AssertionError, msg=key):
                L.join_impact(dict(row, **{key: value}), trace, (12, 0))
        changed = copy.deepcopy(row)
        changed['controls']['thrust'] = True
        with self.assertRaises(AssertionError):
            L.join_impact(changed, trace, (12, 0))

    def test_loss_receipt_requires_current_damage_and_single_counter_increment(self):
        row = dict(tick=12, damage=dict(last_damage_tick=12, last_ship_lost=True,
            last_source='cannon', last_damage_percent=20., last_contact_tick=12,
            last_contact_source='cannon', last_contact_spawn_tick=3))
        self.assertIsNone(L.loss_event(row, 0, 0))
        self.assertEqual(L.loss_event(row, 0, 1)['contact_age_ticks'], 9)
        for old, new in [(0, 2), (1, 0)]:
            with self.assertRaises(AssertionError):
                L.loss_event(row, old, new)
        row['damage']['last_damage_tick'] = 11
        with self.assertRaises(AssertionError):
            L.loss_event(row, 0, 1)

    def test_stale_or_mismatched_contact_cannot_supply_projectile_age(self):
        row = dict(tick=12, damage=dict(last_damage_tick=12, last_ship_lost=True,
            last_source='laser', last_damage_percent=20., last_contact_tick=12,
            last_contact_source='cannon', last_contact_spawn_tick=3))
        self.assertNotIn('spawn_tick', L.loss_event(row, 0, 1))
        row['damage'].update(last_source='cannon', last_contact_tick=11)
        self.assertNotIn('spawn_tick', L.loss_event(row, 0, 1))

    def test_first_differences_separate_damage_from_motion_and_keep_common_horizon(self):
        fields = ('action', 'non_laser_actions', 'controls', 'ship', 'actor', 'hull', 'form', 'location', 'recovery',
                  'pilot_health', 'supply', 'weapons', 'target_hull', 'target_form', 'mode', 'goal', 'pursuit')
        off = [dict(tick=t, seat=s, **dict.fromkeys(fields)) for t in range(4) for s in (0, 1)]
        on = copy.deepcopy(off[:6])
        on[2]['hull'] = 99.
        on[4]['hull'], on[4]['ship'] = 98., 'moved'
        result = L.differences(iter(off), iter(on))
        self.assertEqual(result['common_ticks_by_seat'], {0: 3, 1: 3})
        self.assertEqual(result['first']['p1:hull']['tick'], 1)
        self.assertEqual(result['first']['p1:ship']['tick'], 2)
        on[3]['seat'] = 0
        with self.assertRaises(AssertionError):
            L.differences(iter(off), iter(on))

    def test_only_laser_bit_is_removed_from_control_difference_check(self):
        actions = [{'Scenario': {'kind': 1398079490, 'payload': [1, 2, 3]}},
                   {'Scenario': {'kind': 1398079493, 'payload': [1, 1, 1]}}]
        result = L.without_laser(actions)
        self.assertEqual(result[0], actions[0])
        self.assertEqual(result[1]['Scenario']['payload'], [1, 0, 1])
        self.assertEqual(actions[1]['Scenario']['payload'], [1, 1, 1])


if __name__ == '__main__':
    unittest.main()
