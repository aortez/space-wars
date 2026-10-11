import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('defense', Path(__file__).parents[1] / 'validate-acquisition-defense.py')
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)


def actions():
    return [dict(Scenario=dict(kind=1398079490, payload=[0] * 8)),
            dict(Scenario=dict(kind=1398079491, payload=[0, 0])),
            dict(Scenario=dict(kind=1398079493, payload=[0, 0, 0]))]


def fixture():
    ship = dict(position=dict(x=0., y=160.), velocity=dict(x=0., y=0.), angle=0., spin=0.)
    planet = dict(index=0, revision=1, radius=60., motion=dict(position=dict(x=0., y=0.)))
    pilot = dict(tick=100, owner='player_1', vehicle=0, location=dict(aboard=0), ship=ship,
                 ship_available=True, ship_form='ship', controls_armed=True, queries_ready=True,
                 landing=dict(phase='flying', supported_feet=0), planet=planet, gravity=dict(x=0., y=0.))
    target = dict(owner='player_2', ship_form='ship', health=100., visible=True, ground_occluded=False,
                  motion=dict(position=dict(x=0., y=280.), velocity=dict(x=0., y=0.), angle=0., spin=0.))
    combat = dict(recovery=dict(flight=dict(pilot=pilot, flight=dict(enabled=True, limits=dict(brake_acceleration=50.)))),
                  weapons=dict(last_hit_taken_tick=100, last_hit_source='laser'), target=target,
                  laser_available=True, cannon_ready=True)
    observation = dict(match_rules=True, local=dict(combat=combat), planets=[planet],
                       boundary=dict(center=dict(x=0., y=0.), radius=5000.))
    capture = dict(started_tick=90, goal='survey', site=None, failed_tick=None, completed_tick=None,
        objective_route=None, landing=dict(landed_tick=None, claimed_tick=None, boarded_tick=None),
        acquisition=dict(tick=100, planet=0, revision=1, selected_site=None, survey_rejected_by=None, reason='scan_deferred'))
    attempt = dict(started_tick=100, deadline_tick=820, observed_tick=100, finished_tick=None, reason=None,
        guidance='escape', planet=0, vehicle=0, opponent='player_2', hit_source='laser', capture=capture,
        native_actions=actions(), direction=dict(x=0., y=-1.), estimated_min_range=100., estimated_clearance=50.,
        range=120., opening_speed=None, source_clearance=100., clear_since=None, boundary=dict(active=False),
        controlled_ticks=1, laser_ticks=0, cannon_ticks=0)
    mission = dict(goal='disengage', capture=None, recovery=None, pursuit=None,
                   combat=dict(goal='engage ship', breaks=dict(active_until_tick=None)),
                   acquisition_defense=dict(attempts=1, separated=0, timed_out=0, last=attempt))
    trace = dict(tick=100, seat=0, observation=observation, mission=mission, actions=actions())
    return trace, attempt


def next_step(trace, attempt):
    before = copy.deepcopy(attempt)
    trace['tick'] += 1
    trace['observation']['local']['combat']['recovery']['flight']['pilot']['tick'] += 1
    attempt['observed_tick'] += 1
    attempt['controlled_ticks'] += 1
    attempt['opening_speed'] = 0.
    return before


class AcquisitionDefenseTests(unittest.TestCase):
    def test_persisted_seat_keys_normalize_without_hiding_count_changes(self):
        prior = dict.fromkeys(('players', 'allocation', 'continuation', 'prediction_outcomes', 'retry'), {})
        prior['route_summary'] = dict(publications={'0': 5, '1': 7})
        current = dict(prior, route_summary=dict(publications={0: 5, 1: 7}))
        A.retained_results(prior, current)
        current['route_summary']['publications'][1] = 8
        with self.assertRaises(AssertionError): A.retained_results(prior, current)

    def test_matrix_has_four_known_pairs_and_two_fresh_worlds(self):
        priors = {g: dict(record=dict(item=dict(seat=0), command=['bin', A.L.FLAG, '0'])) for g, _, _ in A.KNOWN}
        cases = A.cases(priors)
        self.assertEqual(len(cases), 24)
        self.assertEqual(len({c['name'] for c in cases}), 24)
        groups = {c['group'] for c in cases}
        self.assertEqual(len(groups), 12)
        for group in groups:
            pair = [c for c in cases if c['group'] == group]
            self.assertEqual({c['defense'] for c in pair}, {False, True})
        fresh = [c for c in cases if c['stage'] == 'held_out']
        self.assertEqual(len({c['seed'] for c in fresh}), 2)
        self.assertEqual({(c['world_cluster'], c['seat'], c['interval']) for c in fresh},
                         set(A.product(range(2), (0, 1), (0, 3))))
        self.assertTrue(all(c['arm'] == 'candidate' and c['laser'] for c in fresh))

    def test_commands_reject_duplicate_or_existing_options(self):
        old = ['old', '--seed', '42', '--out', 'old-root']
        self.assertEqual(A.command(old, 'new', 'new-root', 1, True),
                         ['new', '--seed', '42', '--out', 'new-root', A.FLAG, '1'])
        for bad in (old + ['--seed', '4'], old + [A.FLAG, 'none']):
            with self.assertRaises(AssertionError): A.command(bad, 'new', 'root', 0, True)

    def test_fresh_hit_and_uncommitted_current_receipt_are_required(self):
        for change in range(13):
            trace, attempt = fixture()
            o = trace['observation']; c = o['local']['combat']; p = c['recovery']['flight']['pilot']
            if change == 0: c['weapons']['last_hit_taken_tick'] = 99
            elif change == 1: c['weapons']['last_hit_source'] = attempt['hit_source'] = 'ground'
            elif change == 2: p['landing']['supported_feet'] = 1
            elif change == 3: p['location'] = 'on_foot'
            elif change == 4: c['target']['visible'] = False
            elif change == 5: c['target']['ground_occluded'] = True
            elif change == 6: attempt['capture']['site'] = 0
            elif change == 7: attempt['capture']['acquisition']['tick'] = 99
            elif change == 8: attempt['capture']['acquisition']['survey_rejected_by'] = 'stale'
            elif change == 9: attempt['capture']['acquisition']['reason'] = 'no_measured_candidates'
            elif change == 10: attempt['capture']['objective_route'] = {}
            elif change == 11: attempt['deadline_tick'] = 821
            else: p['queries_ready'] = False
            with self.subTest(change=change), self.assertRaises((AssertionError, TypeError)):
                A.audit_start(o, attempt, set())
        trace, attempt = fixture()
        A.audit_start(trace['observation'], attempt, set())
        with self.assertRaises(AssertionError): A.audit_start(trace['observation'], attempt, {90})

    def test_clock_and_source_are_immutable(self):
        for field in ('deadline_tick', 'planet', 'vehicle', 'hit_source', 'native_actions'):
            trace, attempt = fixture(); before = next_step(trace, attempt)
            A.audit_step(trace, attempt, before, set())
            attempt[field] = 'changed'
            with self.assertRaises(AssertionError): A.audit_step(trace, attempt, before, set())

    def test_missing_target_is_not_separation(self):
        trace, attempt = fixture(); before = next_step(trace, attempt)
        trace['observation']['local']['combat']['target'] = None
        attempt.update(range=None, opening_speed=None)
        A.audit_step(trace, attempt, before, set())
        attempt['clear_since'] = 101
        with self.assertRaises(AssertionError): A.audit_step(trace, attempt, before, set())

    def test_counter_changes_and_scheduled_break_fire_are_rejected(self):
        trace, attempt = fixture()
        A.audit_step(trace, attempt, None, set())
        attempt['controlled_ticks'] = 2
        with self.assertRaises(AssertionError): A.audit_step(trace, attempt, None, set())
        attempt.update(controlled_ticks=1, laser_ticks=1)
        trace['actions'][2]['Scenario']['payload'][1] = 1
        trace['mission']['combat']['breaks']['active_until_tick'] = 101
        with self.assertRaises(AssertionError): A.audit_step(trace, attempt, None, set())

    def test_boundary_requires_braking_and_open_wings(self):
        trace, attempt = fixture(); attempt['guidance'] = 'boundary'; attempt['boundary']['active'] = True
        with self.assertRaises(AssertionError): A.audit_step(trace, attempt, None, set())
        trace['actions'][0]['Scenario']['payload'][6] = 1
        A.audit_step(trace, attempt, None, set())
        trace['actions'][1]['Scenario']['payload'][1] = 1
        with self.assertRaises(AssertionError): A.audit_step(trace, attempt, None, set())

    def test_pair_prefix_requires_exact_source_capture_native_action_and_observation(self):
        trace, attempt = fixture(); before = A.strip(copy.deepcopy(trace))
        before['mission']['goal'] = 'capture'; before['mission']['capture'] = copy.deepcopy(attempt['capture'])
        self.assertEqual(A.prefix(iter([before]), iter([trace]), (100, 0))['tick'], 100)
        for key in ('capture', 'native_actions'):
            bad = copy.deepcopy(trace); bad['mission'][A.FIELD]['last'][key] = {}
            with self.assertRaises(AssertionError): A.prefix(iter([before]), iter([bad]), (100, 0))
        bad = copy.deepcopy(trace); bad['observation']['match_rules'] = False
        with self.assertRaises(AssertionError): A.prefix(iter([before]), iter([bad]), (100, 0))

    def test_unchanged_actions_do_not_hide_state_change_or_missing_rows(self):
        trace, _ = fixture(); before = A.strip(trace)
        self.assertIsNone(A.prefix(iter([before]), iter([trace]), None)['tick'])
        bad = copy.deepcopy(trace); bad['mission']['goal'] = 'other'
        with self.assertRaises(AssertionError): A.prefix(iter([before]), iter([bad]), None)
        with self.assertRaises(AssertionError): A.prefix(iter([before]), iter([]), None)

    def test_partial_impact_window_records_prior_losses_without_inventing_receipts(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(A.D, 'IMPACT_START', 1), patch.object(A.D, 'END', 4):
            root = Path(directory)
            traces, impacts = [], []
            for tick in range(3):
                for seat in (0, 1):
                    trace, _ = fixture()
                    trace.update(tick=tick, seat=seat, controls={})
                    p = trace['observation']['local']['combat']['recovery']['flight']['pilot']
                    p['recovery'] = dict(ships_lost=2 if seat == 0 else 0)
                    traces.append(trace)
                    if tick >= 1:
                        impacts.append(dict(tick=tick, seat=seat, overridden=False, controls={}, bot_controls={},
                            actions=trace['actions'], goal=trace['mission']['goal'], form=p['ship_form'], ship=p['ship'], location=p['location']))
            final = dict(final_tick=3, round={}, config=dict(seat=0, control='bot', control_from_tick=0, trace_start_tick=1, trace_end_tick=4))
            impacts.append(final)
            for name, rows in (('trace', traces), ('impact', impacts)):
                (root / (name + '.jsonl')).write_text(''.join(json.dumps(r) + '\n' for r in rows))
            report = dict(elapsed_ticks=3, round={}, final_pilots=[dict(recovery=dict(ships_lost=2)), dict(recovery=dict(ships_lost=0))])
            result = A.audit_impact(root, report)
            self.assertEqual(result['losses_before_window'], {0: 2, 1: 0})
            self.assertEqual(result['losses'], [])
            self.assertEqual(result['rows'], 4)


if __name__ == '__main__':
    unittest.main()
