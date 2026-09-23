"""Risk diagnostics preserve causal features, competing events and censoring."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

from test_landing_tail import snapshot

SPEC = importlib.util.spec_from_file_location('landing_risk',
    Path(__file__).resolve().parents[1] / 'inspect-landing-risk.py')
risk = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(risk)


def sample(tick, **changes):
    s = snapshot()
    s.update(tick=tick, capture_started_tick=0, seconds_since_landing_progress=7)
    s['phase'].update(name='descent', episode=0, age_seconds=tick / 60)
    s['contact'].update(phase='assisted', planet=0, matches_target=True, supported_feet=0)
    s.update(changes)
    return s


def trace(rows):
    """Rows are dense except for explicitly supplied gaps, as in LandingTrace."""
    result = {'samples': [], 'events': [], 'observations': len(rows), 'selection': {'planet': 0}}
    before = None
    def emit(s):
        if s is not None and (not result['samples'] or result['samples'][-1]['tick'] != s['tick']):
            result['samples'].append(s)
    for current in rows:
        e = risk.tail.transition(before, current)
        if e['tags']:
            emit(before)
            result['events'].append({'tick': current['tick'], **e})
        if e['tags'] or current['tick'] % 60 == 0:
            emit(current)
        before = current
    emit(before)
    return result


def dense(end=960):
    base = sample(0)
    rows = []
    for tick in range(end + 1):
        s = copy.deepcopy(base)
        s['tick'] = tick
        s['geometry']['distance'] = 30 - tick / 60
        rows.append(s)
    return rows


def increment(rows, at, amount=1):
    for s in rows:
        if s['tick'] >= at:
            s['native_counts']['replans'] += amount
            s['native_counts']['live_invalidations'] += amount


def landed(rows, at):
    for s in rows:
        if s['tick'] >= at:
            s['contact'].update(phase='landed', supported_feet=2)


def outcome(rows, tick=360, ending='abandoned', stop=2000):
    t = trace(rows)
    return risk.label(t, risk.timeline(t), {'stopped_tick': stop, 'ending': ending}, tick)


class RiskFeatureTests(unittest.TestCase):
    def test_features_do_not_change_with_future_events_or_landing(self):
        rows = dense()
        increment(rows, 200)
        t = trace(rows[:361])
        before = risk.features(t['samples'], risk.timeline(t), 0, 360)
        increment(rows, 361, 10)
        landed(rows, 370)
        t = trace(rows)
        after = risk.features(t['samples'], risk.timeline(t), 0, 360)
        self.assertEqual(before, after)
        self.assertEqual(after['recent']['counts']['prelanding_replan'], 1)
        self.assertFalse(after['physical_permissions'])

    def test_recent_history_is_unknown_when_short_reset_or_gapped(self):
        for kind in ('short', 'reset', 'capture', 'gap'):
            rows = dense(360)
            if kind == 'short': rows = rows[100:]
            if kind == 'gap': rows = rows[:200] + rows[250:]
            if kind == 'reset':
                increment(rows, 0)
                for s in rows[200:]: s['native_counts']['replans'] = 0
            if kind == 'capture':
                for s in rows[200:]: s['capture_started_tick'] = 200
            t = trace(rows)
            with self.subTest(kind=kind):
                f = risk.features(t['samples'], risk.timeline(t), 0, 360)
                self.assertFalse(f['recent']['complete'])
                self.assertIsNone(f['recent']['counts'])

    def test_history_window_and_current_event_are_inclusive_only_at_end(self):
        rows = dense(360)
        increment(rows, 60, 2)
        increment(rows, 360)
        t = trace(rows)
        f = risk.features(t['samples'], risk.timeline(t), 0, 360)
        self.assertEqual(f['recent']['counts']['prelanding_replan'], 1)

    def test_native_progress_clock_is_not_used_for_tactical_approach(self):
        rows = dense(360)
        for s in rows: s['phase']['name'] = 'approach'
        t = trace(rows)
        f = risk.features(t['samples'], risk.timeline(t), 0, 360)
        self.assertIsNone(f['native_progress_age_seconds'])
        self.assertAlmostEqual(f['one_second_endpoint_progress']['distance_closed'], 1)

    def test_progress_requires_both_endpoints_and_no_plan_change(self):
        for kind in ('gap', 'revision', 'phase', 'site', 'capture', 'replan', 'geometry', 'endpoint'):
            rows = dense(360)
            if kind == 'gap': rows = rows[:320] + rows[340:]
            if kind == 'endpoint': rows = rows[:300] + rows[301:]
            if kind == 'replan': increment(rows, 320)
            if kind == 'geometry': rows[300]['geometry'] = None
            for s in rows:
                if s['tick'] < 320: continue
                if kind == 'revision': s['revision'] += 1
                if kind == 'phase': s['phase']['episode'] += 1
                if kind == 'site': s['site']['bearing'] += 1
                if kind == 'capture': s['capture_started_tick'] = 320
            t = trace(rows)
            with self.subTest(kind=kind):
                f = risk.features(t['samples'], risk.timeline(t), 0, 360)
                self.assertIsNone(f['one_second_endpoint_progress'])

    def test_one_foot_is_eligible_but_past_physical_landing_is_not(self):
        rows = dense(360)
        rows[-1]['contact']['supported_feet'] = 1
        t = trace(rows)
        self.assertEqual(risk.features(t['samples'], risk.timeline(t), 0, 360)['status'], 'eligible')
        rows[200]['contact']['phase'] = 'landed'
        t = trace(rows)
        self.assertEqual(risk.features(t['samples'], risk.timeline(t), 0, 360)['status'],
                         'physical_landing_already_observed')

    def test_event_ledger_rejects_changed_counter_evidence_or_coverage(self):
        rows = dense(360)
        increment(rows, 200)
        for kind in ('delta', 'coverage', 'events'):
            t = trace(rows)
            if kind == 'delta': t['events'][-1]['counter_delta']['replans'] += 1
            if kind == 'coverage': t['observations'] += 1
            if kind == 'events': t['events'] = []
            with self.subTest(kind=kind), self.assertRaises(ValueError): risk.timeline(t)

    def test_unclassified_recent_replan_is_not_absence_of_prelanding_replans(self):
        rows = dense(360)
        for s in rows[190:210]: s['contact']['planet'] = 1
        increment(rows, 200)
        t = trace(rows)
        f = risk.features(t['samples'], risk.timeline(t), 0, 360)
        self.assertEqual(risk.strata(f)['recent_prelanding_replan'], 'unknown')
        self.assertEqual(risk.strata(f)['recent_live_invalidations'], 'yes')


class RiskEndpointTests(unittest.TestCase):
    def test_future_is_open_at_checkpoint_and_closed_at_horizon(self):
        for at, expected in ((360, False), (960, True), (961, False)):
            rows = dense(1000)
            increment(rows, at)
            with self.subTest(at=at): self.assertIs(outcome(rows)['interrupted'], expected)

    def test_first_landing_stops_exposure_before_grounded_replan(self):
        rows = dense()
        landed(rows, 500)
        increment(rows, 501)
        result = outcome(rows)
        self.assertEqual((result['status'], result['tick']), ('landed', 500))
        t = trace(rows)
        event = next(e for e in risk.timeline(t) if e['tick'] == 501)
        self.assertEqual(risk.replan_kind(event, 0), 'grounded_replan')

    def test_simultaneous_landing_and_replan_has_unknown_order(self):
        rows = dense()
        landed(rows, 500)
        increment(rows, 500)
        result = outcome(rows)
        self.assertEqual(result['status'], 'ambiguous_landing_replan')
        self.assertIsNone(result['interrupted'])

    def test_later_failed_ground_trip_does_not_erase_landing(self):
        rows = dense(500)
        landed(rows, 500)
        self.assertEqual(outcome(rows, ending='abandoned')['status'], 'landed')

    def test_short_failed_and_match_end_observations_are_not_negatives(self):
        for ending, status in (('ship_lost', 'attempt_ended'), ('abandoned', 'attempt_ended'),
                               ('match_end', 'censored_recording_end')):
            result = outcome(dense(500), ending=ending, stop=501)
            with self.subTest(ending=ending):
                self.assertEqual(result['status'], status)
                self.assertIsNone(result['interrupted'])

    def test_observed_interruption_before_failed_attempt_is_retained(self):
        rows = dense(500)
        increment(rows, 450)
        self.assertIs(outcome(rows, ending='ship_lost', stop=501)['interrupted'], True)

    def test_gap_crossing_horizon_censors_even_when_next_row_is_after_it(self):
        rows = dense(1000)
        rows = rows[:500] + rows[990:]
        result = outcome(rows)
        self.assertEqual((result['status'], result['tick']), ('censored_observation_gap', 500))
        self.assertIsNone(result['interrupted'])

    def test_reset_or_capture_change_censors_positive_counter_delta(self):
        for kind in ('reset', 'capture'):
            rows = dense()
            increment(rows, 500)
            for s in rows:
                if kind == 'reset': s['native_counts']['invalidations'] = int(s['tick'] < 500)
                if kind == 'capture' and s['tick'] >= 500: s['capture_started_tick'] = 500
            result = outcome(rows)
            with self.subTest(kind=kind):
                self.assertEqual(result['status'], 'censored_capture_or_counter_reset')
                self.assertIsNone(result['interrupted'])

    def test_acquisition_without_increment_is_not_a_native_replan(self):
        rows = dense()
        for s in rows[500:510]: s['site'] = None
        self.assertEqual(outcome(rows)['status'], 'horizon_clear')
        t = trace(rows)
        self.assertTrue(any('site_acquired' in e['tags'] for e in t['events']))
        self.assertFalse(any(risk.replan_kind(e, 0) for e in risk.timeline(t)))

    def test_unknown_frame_does_not_count_as_a_prelanding_or_grounded_event(self):
        rows = dense()
        for s in rows[500:]: s['contact'].update(planet=1, phase='landed')
        increment(rows, 501)
        result = outcome(rows)
        self.assertEqual(result['status'], 'censored_contact_frame_or_phase')
        t = trace(rows)
        e = next(e for e in risk.timeline(t) if e['tick'] == 501)
        self.assertEqual(risk.replan_kind(e, 0), 'unclassified_replan')

    def test_two_feet_without_physical_landing_remains_at_risk(self):
        rows = dense()
        for s in rows[400:]: s['contact']['supported_feet'] = 2
        increment(rows, 500)
        self.assertIs(outcome(rows)['interrupted'], True)

    def test_event_on_exclusive_attempt_boundary_does_not_become_positive(self):
        rows = dense(500)
        increment(rows, 500)
        result = outcome(rows, stop=500)
        self.assertEqual(result['status'], 'attempt_ended')
        self.assertIsNone(result['interrupted'])

    def test_forecast_outside_attempt_cannot_produce_a_future_label(self):
        for tick, stop in ((360, 360), (960, 900), (1000, 2000), (-1, 2000)):
            with self.subTest(tick=tick, stop=stop), self.assertRaises(ValueError):
                outcome(dense(), tick=tick, stop=stop)


class RiskAccountingTests(unittest.TestCase):
    def test_asteroid_stratum_uses_configuration_only(self):
        report = {'asteroids': {'settings': {'interval_seconds': 3}, 'terrain_edits': 999}}
        self.assertEqual(risk.asteroid_interval(report, 'normal'), 3)
        report['asteroids']['terrain_edits'] = 0
        self.assertEqual(risk.asteroid_interval(report, 'normal'), 3)
        self.assertIsNone(risk.asteroid_interval(report, 'controlled'))
        for bad in (None, True, -1, float('nan')):
            report['asteroids']['settings']['interval_seconds'] = bad
            with self.assertRaises(ValueError): risk.asteroid_interval(report, 'normal')

    def test_unknowns_stay_in_bounds_and_worlds_are_equal_weighted(self):
        rows = [{'seed': seed, 'outcome': {'status': status, 'interrupted': positive}}
                for seed, status, positive in [(1, 'interrupted', True), (1, 'interrupted', True),
                                               (1, 'attempt_ended', None), (2, 'landed', False)]]
        result = risk.statistics_for(rows)
        self.assertEqual(result['observed_fraction_bounds'], [0.5, 0.75])
        self.assertEqual(result['equal_world_fraction_bounds'], [1 / 3, 0.5])
        self.assertEqual(result['unknown'], 1)

    def test_source_hash_mismatch_is_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'source.json'
            path.write_text(json.dumps({'version': 1}))
            _, digest = risk.read_bound(path)
            risk.read_bound(path, digest)
            path.write_text(json.dumps({'version': 2}))
            with self.assertRaises(ValueError): risk.read_bound(path, digest)


if __name__ == '__main__':
    unittest.main()
