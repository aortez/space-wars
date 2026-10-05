import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location(
    'climb', Path(__file__).parents[1] / 'validate-pursuit-climb-laser.py')
L = importlib.util.module_from_spec(spec)
spec.loader.exec_module(L)


def actions(laser=0):
    return [dict(Scenario=dict(kind=1398079490, payload=[0, 0, 0, 0, 0, 0, 0, 0])),
            dict(Scenario=dict(kind=1398079491, payload=[0, 0])),
            dict(Scenario=dict(kind=L.T.WEAPON_ACTION, payload=[0, laser, 0]))]


def fixture():
    motion = dict(position=dict(x=0., y=0.), velocity=dict(x=0., y=0.), angle=0., spin=0.)
    target_motion = copy.deepcopy(motion)
    target_motion['position']['y'] = 100.
    pilot = dict(tick=10, owner='player_1', ship=motion, controls_armed=True,
                 queries_ready=True, ship_available=True, ship_form='ship', location=dict(aboard=0))
    observation = dict(match_rules=True, local=dict(combat=dict(
        recovery=dict(flight=dict(pilot=pilot, flight=dict(enabled=True))),
        target=dict(motion=target_motion, visible=True, ground_occluded=False), laser_available=True)))
    gate = dict(checks=1, requested_ticks=1, last=dict(tick=10, source='mission', decision='requested',
        break_until_tick=None, distance=100., heading_error=0., native_actions=actions()))
    mission = dict(goal='hunt', pursuit=dict(started_tick=9), capture=None, recovery=None,
                   combat=None, reason='climbing for a firing pass', **{L.FIELD: gate})
    trace = dict(tick=10, seat=0, actions=actions(1), observation=observation, mission=mission)
    row = dict(seat=0, pilot=pilot, actions=trace['actions'], **{L.FIELD: dict(
        telemetry=gate, observation=observation, pursuit=mission['pursuit'], combat=None, reason=mission['reason'])})
    return row, trace, dict(checks=0, requested_ticks=0, last=None)


class PursuitClimbLaserTests(unittest.TestCase):
    def test_only_the_laser_bit_may_change(self):
        L.audit_actions(actions(), actions(1), 0, True)
        L.audit_actions(actions(), actions(), 0, False)
        for index, offset in [(0, 1), (1, 1), (2, 2), (2, 0)]:
            bad = actions(1)
            bad[index]['Scenario']['payload'][offset] = 1
            with self.assertRaises(AssertionError):
                L.audit_actions(actions(), bad, 0, True)
        with self.assertRaises(AssertionError):
            L.audit_actions(actions(), actions(1), 0, False)

    def test_native_firing_check_uses_the_exact_consumed_observation(self):
        row, trace, previous = fixture()
        result = L.audit_check(row, trace, previous)
        self.assertEqual(result['requested_ticks'], 1)
        row[L.FIELD]['observation'] = copy.deepcopy(row[L.FIELD]['observation'])
        row[L.FIELD]['observation']['local']['combat']['laser_available'] = False
        with self.assertRaises(AssertionError):
            L.audit_check(row, trace, previous)

    def test_break_readiness_and_visibility_refusals_cannot_fire(self):
        for reason in ('scheduled_break', 'unready', 'occluded', 'no_target', 'unavailable'):
            row, trace, previous = fixture()
            c = trace['observation']['local']['combat']
            check = trace['mission'][L.FIELD]['last']
            if reason == 'scheduled_break':
                check['break_until_tick'] = 11
            elif reason == 'unready':
                c['laser_available'] = False
            elif reason == 'occluded':
                c['target']['ground_occluded'] = True
            elif reason == 'no_target':
                c['target'] = None
            else:
                c['recovery']['flight']['pilot']['ship_form'] = 'escape_pod'
            with self.assertRaises(AssertionError):
                L.audit_check(row, trace, previous)
            check.update(decision=reason, distance=None, heading_error=None)
            trace['mission'][L.FIELD]['requested_ticks'] = 0
            row['actions'] = trace['actions'] = actions()
            L.audit_check(row, trace, previous)

    def test_expired_break_is_not_a_permanent_weapons_ban(self):
        row, trace, previous = fixture()
        trace['mission'][L.FIELD]['last']['break_until_tick'] = 10
        L.audit_check(row, trace, previous)

    def test_reported_geometry_must_agree_and_bad_range_cannot_fire(self):
        row, trace, previous = fixture()
        trace['mission'][L.FIELD]['last']['distance'] = 249.
        with self.assertRaises(AssertionError):
            L.audit_check(row, trace, previous)
        trace['observation']['local']['combat']['target']['motion']['position']['y'] = 251.
        trace['mission'][L.FIELD]['last']['distance'] = 251.
        with self.assertRaises(AssertionError):
            L.audit_check(row, trace, previous)
        trace['mission'][L.FIELD]['last']['decision'] = 'aim_or_range'
        trace['mission'][L.FIELD]['requested_ticks'] = 0
        row['actions'] = trace['actions'] = actions()
        L.audit_check(row, trace, previous)

    def test_missing_checks_and_misidentified_sources_fail(self):
        for mutation in ('checks', 'requested_ticks', 'source'):
            row, trace, previous = fixture()
            gate = trace['mission'][L.FIELD]
            if mutation == 'source':
                gate['last']['source'] = 'unknown'
            else:
                gate[mutation] += 1
            with self.assertRaises(AssertionError):
                L.audit_check(row, trace, previous)

    def test_command_changes_only_binary_output_and_one_unique_option(self):
        prior = dict(item=dict(seat=0), command=['old-bin', '--seed', '42', '--out', 'old-root'])
        expected = ['new-bin', '--seed', '42', '--out', 'new-root', L.FLAG, '0']
        self.assertEqual(L.command(prior, Path('new-bin'), Path('new-root'), True), expected)
        self.assertEqual(L.command(prior, Path('new-bin'), Path('new-root'), False)[-1], 'none')
        prior['command'] += [L.FLAG, 'none']
        with self.assertRaises(AssertionError):
            L.command(prior, Path('new-bin'), Path('new-root'), True)

    def test_prefix_is_exact_through_the_first_changed_request(self):
        _, after, _ = fixture()
        before = L.without_option(after)
        before['actions'] = actions()
        result = L.compare_prefix(iter([before]), iter([after]), 10)
        self.assertEqual(result['tick'], 10)
        for bad in ('tick', 'native_action', 'observation'):
            changed = copy.deepcopy(after)
            if bad == 'tick':
                changed['mission'][L.FIELD]['last']['tick'] = 9
            elif bad == 'native_action':
                changed['mission'][L.FIELD]['last']['native_actions'][1]['Scenario']['payload'][1] = 1
            else:
                changed['observation']['match_rules'] = False
            with self.assertRaises(AssertionError):
                L.compare_prefix(iter([before]), iter([changed]), 10)

    def test_unchanged_actions_do_not_hide_earlier_state_changes_or_missing_rows(self):
        _, after, _ = fixture()
        after['actions'] = actions()
        before = L.without_option(after)
        self.assertIsNone(L.compare_prefix(iter([before]), iter([after]), None)['tick'])
        after['observation']['match_rules'] = False
        with self.assertRaises(AssertionError):
            L.compare_prefix(iter([before]), iter([after]), None)
        with self.assertRaises(AssertionError):
            L.compare_prefix(iter([before]), iter([]), None)

    def test_ship_losses_use_actor_counters_not_recovery_task_telemetry(self):
        for final_step_loss in (False, True):
            with self.subTest(final_step_loss=final_step_loss), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                traces, impacts = [], []
                loss_tick = 2 if final_step_loss else 1
                damage = dict(last_damage_tick=loss_tick, last_ship_lost=True, last_source='laser',
                              last_damage_percent=.1, last_contact_tick=None, last_contact_source=None)
                for tick in (0, 1):
                    for seat in (0, 1):
                        lost = int(seat == 1 and tick >= loss_tick)
                        _, trace, _ = fixture()
                        trace.update(tick=tick, seat=seat, controls={})
                        pilot = trace['observation']['local']['combat']['recovery']['flight']['pilot']
                        pilot.update(ship_form='escape_pod' if lost else 'ship', recovery=dict(ships_lost=lost))
                        traces.append(trace)
                        impacts.append(dict(tick=tick, seat=seat, overridden=False, controls={}, bot_controls={},
                            actions=trace['actions'], form=pilot['ship_form'], goal=trace['mission']['goal'],
                            recovery=dict(goal='land_pod') if lost else None, damage=damage))
                final = dict(final_tick=2, round={}, damage=[{}, damage], config=dict(
                    seat=0, control='bot', control_from_tick=0, trace_start_tick=0, trace_end_tick=3))
                impacts.append(final)
                for name, rows in [('trace', traces), ('impact', impacts)]:
                    (root / (name + '.jsonl')).write_text(''.join(json.dumps(r) + '\n' for r in rows))
                report = dict(elapsed_ticks=2, round={}, final_pilots=[dict(recovery=dict(ships_lost=n)) for n in (0, 1)])
                with patch.object(L.D, 'IMPACT_START', 0), patch.object(L.D, 'END', 3):
                    result = L.impact_audit(root, report)
                self.assertEqual(result['losses'][1][0]['receipt']['tick'], loss_tick)
                self.assertEqual(result['losses'][0], [])


if __name__ == '__main__':
    unittest.main()
