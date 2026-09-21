"""Controlled starts must not masquerade as normal missions or repaired forecasts."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

from test_trip_estimates import row, profile

SPEC = importlib.util.spec_from_file_location('controlled_ground',
    Path(__file__).resolve().parents[1] / 'evaluate-controlled-ground.py')
controlled = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(controlled)


def raw(tick=120, start=100, **kwargs):
    r = row(tick, **kwargs)
    return {'version': 1, 'scope': controlled.SCOPE, 'tick': tick, 'seat': 0,
        'capture_started_tick': start, 'observation': r['observation']['local'],
        'capture': r['mission']['capture'], 'controls': {'turn': 0, 'thrust': False, 'brake': False}}


def observe(records):
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / 'trace.jsonl'
        path.write_text(''.join(json.dumps(r) + '\n' for r in records))
        return controlled.prefix(path, 0, profile())


def report():
    return {'capture_started_tick': 100, 'elapsed_ticks': 1800, 'seconds': 30,
        'capture': {'completed_tick': 1700, 'failed_tick': None, 'failure': None,
            'landing': {'landed_tick': 800, 'claimed_tick': 1200, 'boarded_tick': 1500}}}


def completed_observation():
    a = observe([raw()])
    a.update(native_milestones={'landed': 800, 'claimed': 1200, 'boarded': 1500}, mission_departed_tick=1650)
    return a


class ControlledPrefixTests(unittest.TestCase):
    def test_setup_rows_do_not_create_a_capture_attempt(self):
        result = observe([raw(start=None)])
        self.assertIsNone(result['selection'])
        self.assertIsNone(result['choice'])
        self.assertIsNone(result['prediction'])

    def test_capture_start_is_the_explicit_controlled_origin(self):
        result = observe([raw(99, start=None), raw()])
        self.assertEqual(result['selection']['selected_tick'], 100)
        self.assertEqual(result['choice']['elapsed_transfer_seconds'], 0)
        self.assertAlmostEqual(result['choice']['elapsed_seconds'], 20 / 60)

    def test_later_shorter_route_does_not_repair_the_first_forecast(self):
        first = observe([raw()])
        full = observe([raw(), raw(180, length=1)])
        self.assertEqual(first['choice'], full['choice'])
        self.assertEqual(first['prediction'], full['prediction'])
        self.assertTrue(full['post_choice_changes'])

    def test_missing_initial_route_evidence_stays_unknown(self):
        a = raw()
        a['observation']['landing_objective'] = None
        result = observe([a, raw(121)])
        self.assertIn('route_source_unverified_at_choice', result['choice']['missing'])
        self.assertIsNone(result['prediction']['remaining_seconds'])

    def test_wrong_scope_future_start_or_changed_start_is_rejected(self):
        a = raw()
        a['scope'] = 'missions'
        for records in [[a], [raw(start=121)], [raw(), raw(121, start=101)], [raw(), raw(121, start=None)]]:
            with self.assertRaises(ValueError):
                observe(records)

    def test_actor_order_and_postlanding_choice_are_checked(self):
        a = raw()
        a['seat'] = 1
        with self.assertRaises(ValueError):
            observe([a])
        with self.assertRaises(ValueError):
            observe([raw(), raw()])
        a = raw()
        a['capture']['landing']['landed_tick'] = 119
        self.assertIsNone(observe([a])['choice'])

    def test_leaving_target_censors_the_trial_and_foreign_choices_are_never_used(self):
        a = raw()
        a['capture']['site'] = None
        foreign = raw(121)
        foreign['observation']['combat']['recovery']['flight']['pilot']['planet']['index'] = 1
        result = observe([a, foreign, raw(122)])
        self.assertEqual(result['first_frame_change_tick'], 121)
        self.assertIsNone(result['choice'])
        t = controlled.make_trip(report(), result, 0)
        self.assertEqual((t.stop, t.ending), (121, 'frame_changed'))
        self.assertNotIn('claimed', t.milestones)

    def test_departure_uses_the_mission_threshold_not_tactical_hold_time(self):
        a = raw()
        p = a['observation']['combat']['recovery']['flight']['pilot']
        a['capture']['landing'].update(landed_tick=101, claimed_tick=110, boarded_tick=119)
        p['planet'].update(radius=60, motion={'position': {'x': 0, 'y': 0}})
        p['ship'] = {'position': {'x': 0, 'y': 131}}
        result = observe([a])
        self.assertEqual(result['mission_departed_tick'], 120)
        self.assertIsNone(result['choice'])

    def test_ship_loss_ends_the_capture_scope_without_becoming_a_timeout(self):
        a = raw(121)
        a['observation']['combat']['recovery']['flight']['pilot']['ship_available'] = False
        observed = observe([raw(), a, raw(122)])
        self.assertEqual(observed['first_ship_loss_tick'], 121)
        t = controlled.make_trip(report(), observed, 0)
        self.assertEqual((t.ending, t.stop), ('ship_lost', 121))


class ControlledActualTests(unittest.TestCase):
    def test_native_milestones_and_controller_completion_define_the_trip(self):
        t = controlled.make_trip(report(), completed_observation(), 0)
        self.assertEqual((t.start, t.stop, t.ending), (100, 1650, 'completed'))
        self.assertEqual(t.milestones['boarded'], [1500, 1500])
        self.assertEqual(t.milestones['departed'], [1650, 1650])

    def test_failure_is_censored_and_time_limit_does_not_become_success(self):
        r = report()
        r['capture'].update(completed_tick=None, failed_tick=1600, failure='blocked')
        a = completed_observation()
        a['mission_departed_tick'] = None
        t = controlled.make_trip(r, a, 0)
        self.assertEqual((t.stop, t.ending), (1601, 'abandoned'))
        self.assertEqual(t.result()['phases']['departure']['status'], 'censored')
        r['elapsed_ticks'] = 1601
        self.assertEqual(controlled.make_trip(r, a, 0).ending, 'abandoned')
        r['elapsed_ticks'] = 1800
        r['capture'].update(failed_tick=None, failure=None)
        self.assertEqual(controlled.make_trip(r, a, 0).ending, 'time_limit')

    def test_report_trace_disagreement_cannot_create_an_attempt_or_milestone(self):
        r = report()
        with self.assertRaises(ValueError):
            controlled.make_trip(r, observe([raw(start=None)]), 0)
        r['capture_started_tick'] = 101
        with self.assertRaises(ValueError):
            controlled.make_trip(r, observe([raw()]), 0)
        r = report()
        a = completed_observation()
        a['native_milestones']['landed'] = 1
        with self.assertRaises(ValueError):
            controlled.make_trip(r, a, 0)

    def test_unknown_setup_and_trace_hash_are_preserved(self):
        r = report()
        r['capture_started_tick'] = None
        self.assertIsNone(controlled.make_trip(r, observe([raw(start=None)]), 0))
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'trace.jsonl'
            content = (json.dumps(raw()) + '\n').encode()
            path.write_bytes(content)
            self.assertEqual(len(list(controlled.rows(path, hashlib.sha256(content).hexdigest()))), 1)
            with self.assertRaises(ValueError):
                list(controlled.rows(path, 'wrong'))


if __name__ == '__main__':
    unittest.main()
