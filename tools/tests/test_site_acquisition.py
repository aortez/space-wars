"""Acquisition evidence must not turn absent plans into landing failures."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from test_flight_progress import row as flight_row, pilot

SPEC = importlib.util.spec_from_file_location('site_acquisition',
    Path(__file__).resolve().parents[1] / 'inspect-site-acquisition.py')
model = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(model)


def row(tick=0):
    r = flight_row(tick=tick)
    p = pilot(r)
    p.update(controls_armed=True, site_query='survey')
    p['planet']['radius'] = 20
    r['mission']['goal'] = 'capture'
    r['mission']['capture'].update(site=None, started_tick=0)
    return r


def sample(tick=0, **changes):
    s = model.snapshot(row(tick), 0)
    s.update(changes)
    return s


def attempt(arrival=2, choice=5, stop=10):
    actual = {'selected_tick': 0, 'stopped_tick': stop, 'ending': 'abandoned',
              'reason': 'left destination approach frame',
              'milestones': {} if arrival is None else {'arrived': [arrival, arrival]}}
    return model.Attempt({'actual': actual, 'selection': {'seat': 0, 'planet': 0}},
                         {'first_choice_tick': choice})


class AcquisitionTests(unittest.TestCase):
    def test_choice_and_attempt_stop_are_exclusive_wait_boundaries(self):
        a = attempt()
        for t in range(6):
            a.observe(sample(t, chosen_site={'planet': 0, 'bearing': 1} if t == 5 else None))
        a.observe(sample(10, chosen_site={'planet': 0, 'bearing': 2}))
        result = a.result()
        self.assertEqual(result['observed_ticks'], 5)
        self.assertEqual(result['local_observed_ticks'], 3)
        self.assertEqual(result['missing_ticks'], 0)
        self.assertEqual(result['stop']['tick'], 10)
        self.assertEqual(result['choice']['tick'], 5)

    def test_stop_row_cannot_create_a_first_choice(self):
        a = attempt(choice=None, stop=3)
        for t in range(3): a.observe(sample(t))
        a.observe(sample(3, chosen_site={'planet': 0, 'bearing': 1}))
        self.assertEqual(a.result()['cohort'], 'no_choice/exact')
        self.assertIsNone(a.result()['choice'])
        self.assertEqual(a.result()['observed_ticks'], 3)

    def test_missing_arrival_is_not_local_failure_or_wait(self):
        a = attempt(arrival=None, choice=None, stop=3)
        for t in range(3): a.observe(sample(t))
        result = a.result()
        self.assertEqual(result['cohort'], 'no_choice/not_recorded')
        self.assertIsNone(result['local_prechoice_seconds'])
        self.assertEqual(result['evidence_state_ticks'], {'before_recorded_arrival': 3})

    def test_uncertain_arrival_is_retained_without_exact_duration(self):
        source = {'actual': {'selected_tick': 0, 'stopped_tick': 9, 'ending': 'match_end',
            'reason': None, 'milestones': {'arrived': [2, 4]}}, 'selection': {'seat': 0, 'planet': 0}}
        a = model.Attempt(source, {'first_choice_tick': None})
        self.assertEqual(a.result()['arrival_status'], 'uncertain')
        self.assertIsNone(a.result()['local_prechoice_seconds'])

    def test_gaps_and_counter_resets_do_not_invent_work(self):
        for change in ('gap', 'reset', 'capture'):
            a = attempt(arrival=0, choice=None, stop=5)
            x, y = sample(0), sample(2 if change == 'gap' else 1)
            x['native_counts']['replans'] = 4
            y['native_counts']['replans'] = 3 if change == 'reset' else 8
            if change == 'capture': y['capture_started_tick'] = 1
            a.observe(x); a.observe(y)
            self.assertEqual(a.result()['native_counter_increments'], {})
            self.assertEqual(a.result()['unknown_counter_intervals'], 1)
            self.assertEqual(a.result()['missing_ticks'], 3)

    def test_unselected_stale_replans_are_separate_counter_events(self):
        a = attempt(arrival=0, choice=None, stop=2)
        a.observe(sample(0))
        s = sample(1, objective_work='stale')
        for k in ('replans', 'objective_replans', 'live_invalidations'): s['native_counts'][k] = 1
        a.observe(s)
        result = a.result()
        self.assertEqual(result['native_counter_increments']['replans'], 1)
        self.assertFalse(result['counter_events'][0]['had_chosen_site'])

    def test_deferred_scan_is_not_empty_ground(self):
        s = sample(site_count=0, objective_work='pending')
        for kind in ('deferred', 'not_requested'):
            s['site_query'] = kind
            self.assertEqual(model.state(s, True), 'scan_' + kind)
        s['site_query'] = 'survey'
        self.assertEqual(model.state(s, True), 'measured_scan_empty')

    def test_route_absence_negative_and_unmatched_positive_stay_distinct(self):
        s = sample(site_count=3, objective_required=True, objective_work='pending', survey_age_current=False)
        self.assertEqual(model.state(s, True), 'candidates_without_current_routes')
        s.update(survey_age_current=True, objective_work='ready', complete_routes=0, matching_complete_routes=0)
        self.assertEqual(model.state(s, True), 'candidates_without_complete_round_trip')
        s.update(complete_routes=1)
        self.assertEqual(model.state(s, True), 'routes_without_matching_current_site')
        s.update(matching_complete_routes=1)
        self.assertEqual(model.state(s, True), 'candidates_with_recorded_round_trip')

    def test_route_failure_is_not_a_proof_the_entire_planet_is_unreachable(self):
        self.assertEqual(model.route_status({'outbound': {'failure': 'disconnected'}}), 'outbound_disconnected')
        self.assertEqual(model.route_status({'crossing': {}, 'outbound': {}}), 'powered_unclassified')

    def test_snapshot_does_not_consume_future_event_history(self):
        r = row()
        before = model.snapshot(r, 0)
        r['mission']['events'] = [{'kind': 'arrived', 'tick': 5000}, {'kind': 'departed', 'tick': 7000}]
        self.assertEqual(before, model.snapshot(r, 0))
        self.assertFalse(before['physical_permissions'])

    def test_bound_choice_actor_and_order_mismatches_are_rejected(self):
        with self.assertRaises(ValueError): attempt(choice=10, stop=10)
        with self.assertRaises(ValueError): attempt(arrival=5, choice=3)
        a = attempt()
        with self.assertRaises(ValueError): a.observe(sample(0, seat=1))
        with self.assertRaises(ValueError): a.observe(sample(1, chosen_site={'planet': 0}))
        a.observe(sample(0))
        with self.assertRaises(ValueError): a.observe(sample(0))
        with self.assertRaises(ValueError): a.result()

    def test_snapshot_checks_actor_clock(self):
        r = row()
        pilot(r)['tick'] = 8
        with self.assertRaises(ValueError): model.snapshot(r, 0)

    def test_malformed_counter_is_rejected(self):
        for value in (-1, True, 1.5):
            r = row()
            r['mission']['capture']['replans'] = value
            with self.assertRaises(ValueError): model.snapshot(r, 0)

    def test_controlled_origin_is_explicit_and_not_a_normal_arrival(self):
        r = row()
        raw = {'version': 1, 'scope': model.risk.tail.composed.controlled.SCOPE,
            'tick': 0, 'seat': 0, 'capture_started_tick': 0,
            'observation': r['observation']['local'], 'capture': r['mission']['capture'],
            'controls': {}}
        s = model.snapshot(model.adapt(raw, 'controlled'), 0)
        self.assertIsNone(s['mission_goal'])
        self.assertTrue(s['capture_present'])
        with self.assertRaises(ValueError): model.adapt(raw, 'normal')
        with self.assertRaises((ValueError, KeyError)): model.adapt(r, 'controlled')

    def test_failed_setup_runs_and_raw_source_hash_remain_bound(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            raw = root / 'trace.jsonl'
            r = row()
            controlled = {'version': 1, 'scope': model.risk.tail.composed.controlled.SCOPE,
                'tick': 0, 'seat': 0, 'capture_started_tick': None,
                'observation': r['observation']['local'], 'capture': None, 'controls': {}}
            raw.write_text(json.dumps(controlled) + '\n')
            digest = hashlib.sha256(raw.read_bytes()).hexdigest()
            derived = root / 'derived.json'
            derived.write_text(json.dumps({'trace_sha256': digest}))
            manifest = root / 'inputs.json'
            manifest.write_text('{"runtime_revision":"fixed"}')
            run = {'dataset': 'test', 'scope': 'controlled', 'label': 'setup', 'seed': 7,
                'asteroid_interval_seconds': None, 'trial_outcome': 'setup_unavailable',
                'unobserved_attempts': [], 'source_attempts': [], 'attempts': [],
                'provenance': {'report': str(root / 'report.json'), 'path': str(derived),
                    'sha256': hashlib.sha256(derived.read_bytes()).hexdigest()}}
            with patch.object(model.risk, 'load_runs', return_value=iter([run])):
                result = model.inspect(manifest, root / 'ok')
            self.assertEqual(result['runs'], 1)
            self.assertEqual(result['attempts'], 0)
            self.assertEqual(result['trial_outcomes'], {'setup_unavailable': 1})
            controlled['tick'] = 1
            raw.write_text(json.dumps(controlled) + '\n')
            with patch.object(model.risk, 'load_runs', return_value=iter([run])):
                with self.assertRaisesRegex(ValueError, 'raw recording hash'):
                    model.inspect(manifest, root / 'bad')


if __name__ == '__main__':
    unittest.main()
