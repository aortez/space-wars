"""Rolling forecasts must not hide stale plans, censored tails or spent budgets."""
import copy
import importlib.util
import io
import json
import math
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from test_trip_estimates import row as base_row, profile as base_profile
from test_trip_costs import pilot, report, visit

SCRIPT = Path(__file__).resolve().parents[1] / 'roll-trip-estimates.py'
SPEC = importlib.util.spec_from_file_location('rolling_trip_estimates', SCRIPT)
rolling = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(rolling)


def row(tick=120, **kwargs):
    r = base_row(tick, **kwargs)
    p, c = pilot(r), r['mission']['capture']
    p['planet']['motion'] = {'position': {'x': 0, 'y': 0}, 'angle': 0}
    p['planet']['claim']['flag_interaction_range'] = 3
    if p['planet']['claim']['flag']:
        p['planet']['claim']['flag']['position'] = {'x': 20, 'y': 0}
    p['landing'] = {'supported_feet': 0, 'foot_clearances': [26, 26],
                    'descent_speed': 0, 'lateral_speed': 0}
    c.update(goal='seek_cover', replans=0, cover_replans=0, solar_replans=0,
             live_invalidations=0, circling_replans=0, circling_remaining=50,
             circling_progress_tick=tick, started_tick=60, failed_tick=None)
    c['landing'].update(goal='survey', last_progress_tick=tick, touchdown_adjustments=0)
    return r


def profile():
    p = base_profile()
    p['cells']['walk/landing'].update(samples=[{'seed': i, 'seconds_bounds': [x, x]}
        for i, x in enumerate([10, 20])], median_midpoint_seconds=15,
        observed_seconds_envelope=[10, 20])
    return p


def state(r=None):
    r = row() if r is None else r
    s = rolling.RollingTrip(rolling.frozen.snapshot(r, 0, 'mission_selection'), profile())
    return s, s.observe(r)


class RollingStateTests(unittest.TestCase):
    def test_first_estimate_matches_original_and_preserves_source_age(self):
        s, out = state()
        self.assertEqual(out['total_seconds'], s.original['total_seconds'])
        self.assertEqual(out['route_source_tick'], 119)
        next_row = row(121)
        result = s.observe(next_row)
        self.assertEqual(result['reference_acquired_tick'], 120)
        self.assertEqual(result['route_age_ticks'], 2)
        self.assertEqual(result['plan_observed_tick'], 120)
        self.assertAlmostEqual(result['total_seconds'], out['total_seconds'])
        self.assertFalse(result['physical_permissions'])

    def test_material_change_expires_old_forecast_until_new_evidence_arrives(self):
        s, first = state()
        original = copy.deepcopy(s.original)
        r = row(121, revision=1)
        r['mission']['capture']['site'] = None
        result = s.observe(r)
        self.assertIsNone(result['total_seconds'])
        self.assertIn('revision', result['invalidated_by'])
        r = row(122, revision=1)
        r['observation']['local']['landing_objective'] = None
        self.assertIsNone(s.observe(r)['total_seconds'])
        result = s.observe(row(123, revision=1))
        self.assertEqual(result['reference_acquired_tick'], 123)
        self.assertEqual(result['plan_observed_tick'], 122)
        self.assertAlmostEqual(result['seconds_before_current_plan'], 2 / 60)
        self.assertEqual(result['budget']['capture_started_tick'], first['budget']['capture_started_tick'])
        self.assertEqual(s.original, original)

    def test_route_change_with_same_site_also_expires_reference(self):
        s, _ = state()
        r = row(121, length=40)
        r['observation']['local']['landing_objective'] = None
        out = s.observe(r)
        self.assertEqual(out['invalidated_by'], ['route'])
        self.assertIsNone(out['remaining_seconds'])
        self.assertEqual(out['plan_observed_tick'], 120)
        self.assertEqual(out['reference_generation'], 1)
        self.assertEqual(out['plan_generation'], 0)

    def test_material_refresh_during_descent_preserves_actual_approach_progress(self):
        s, _ = state()
        r = row(121, revision=1)
        r['mission']['capture']['goal'] = 'surface'
        r['mission']['capture']['landing']['goal'] = 'land'
        result = s.observe(r)
        self.assertEqual(result['invalidated_by'], ['revision'])
        self.assertEqual(result['reference_acquired_tick'], 121)
        self.assertEqual(result['plan_observed_tick'], 120)
        self.assertAlmostEqual(result['plan_age_seconds'], 1 / 60)
        self.assertEqual(result['seconds_before_current_plan'], 0)
        self.assertEqual(result['plan_generation'], 0)

    def test_native_retry_with_identical_site_and_route_starts_new_plan_not_new_budget(self):
        s, before = state()
        r = row(121)
        r['mission']['capture']['replans'] = 1
        after = s.observe(r)
        self.assertIn('native_replans', after['invalidated_by'])
        self.assertEqual(after['plan_age_seconds'], 0)
        self.assertGreater(after['elapsed_seconds'], before['elapsed_seconds'])
        self.assertEqual(after['budget']['first_time_limit_tick'], before['budget']['first_time_limit_tick'])
        self.assertEqual(after['budget']['retries_remaining']['ordinary'], 3)

    def test_progress_phase_changes_do_not_restart_the_countdown(self):
        s, _ = state()
        r = row(121)
        r['mission']['capture']['goal'] = 'surface'
        r['mission']['capture']['landing']['goal'] = 'land'
        out = s.observe(r)
        self.assertEqual(out['plan_observed_tick'], 120)
        self.assertEqual(out['progress']['landing_goal'], 'land')
        self.assertEqual(out['invalidated_by'], [])

    def test_stale_native_evidence_cannot_reacquire_itself_on_the_same_tick(self):
        s, _ = state()
        r = row(121)
        r['observation']['local']['objective_work'] = 'stale'
        out = s.observe(r)
        self.assertIn('native_objective_evidence_stale', out['invalidated_by'])
        self.assertIsNone(out['total_seconds'])
        r = row(122)
        r['observation']['local']['landing_objective'] = None
        self.assertIsNone(s.observe(r)['total_seconds'])
        self.assertEqual(s.observe(row(123))['reference_acquired_tick'], 123)

    def test_unknown_queries_suspend_but_do_not_reset_plan_age(self):
        s, _ = state()
        r = row(121)
        pilot(r)['queries_ready'] = False
        self.assertIn('local_queries_unavailable', s.observe(r)['unknown_reasons'])
        out = s.observe(row(122))
        self.assertEqual(out['plan_observed_tick'], 120)
        self.assertEqual(out['reference_acquired_tick'], 120)

    def test_gap_invalidates_and_does_not_imply_continuity_through_missing_ticks(self):
        s, _ = state()
        r = row(180)
        r['observation']['local']['landing_objective'] = None
        out = s.observe(r)
        self.assertIn('observation_gap', out['invalidated_by'])
        self.assertIsNone(out['total_seconds'])
        self.assertEqual(out['elapsed_seconds'], 3)
        self.assertEqual(out['plan_observed_tick'], 120)

    def test_gap_with_reacquired_evidence_cannot_reset_the_unchanged_plan_clock(self):
        s, _ = state()
        out = s.observe(row(180))
        self.assertEqual(out['reference_acquired_tick'], 180)
        self.assertEqual(out['plan_age_seconds'], 1)
        self.assertEqual(out['seconds_before_current_plan'], 0)

    def test_rotating_planet_does_not_move_the_flag_in_its_material_frame(self):
        s, _ = state()
        r = row(121)
        p = pilot(r)
        p['planet']['motion'] = {'position': {'x': 40, 'y': 50}, 'angle': math.pi / 2}
        p['planet']['claim']['flag']['position'] = {'x': 40, 'y': 70}
        self.assertEqual(s.observe(r)['invalidated_by'], [])

    def test_small_cumulative_flag_moves_cannot_refresh_the_dependency_origin(self):
        s, _ = state()
        a, b = row(121), row(122)
        pilot(a)['planet']['claim']['flag']['position']['x'] += 0.3
        pilot(b)['planet']['claim']['flag']['position']['x'] += 0.6
        self.assertEqual(s.observe(a)['invalidated_by'], [])
        self.assertIn('objective', s.observe(b)['invalidated_by'])

    def test_landing_ends_the_prelanding_model_even_if_telemetry_later_resets(self):
        s, _ = state()
        r = row(121)
        r['mission']['capture']['landing']['landed_tick'] = 121
        out = s.observe(r)
        self.assertEqual(out['status'], 'landed')
        self.assertIsNone(out['remaining_seconds'])
        self.assertIsNone(s.observe(row(122)))

    def test_native_clock_reset_is_visible_alongside_the_original_attempt_age(self):
        s, _ = state()
        r = row(121)
        r['mission']['capture']['started_tick'] = 121
        out = s.observe(r)
        self.assertTrue(out['budget']['native_clock_changed'])
        self.assertEqual(out['budget']['first_observed_capture_started_tick'], 60)
        self.assertAlmostEqual(out['elapsed_seconds'], 121 / 60)

    def test_append_only_observations_cannot_mutate_previous_forecasts(self):
        s, original = state()
        saved = copy.deepcopy(original)
        s.observe(row(121, revision=1, length=20))
        self.assertEqual(original, saved)


class DurationAndBudgetTests(unittest.TestCase):
    def test_elapsed_successful_samples_are_removed_without_creating_zero_countdown(self):
        cell = profile()['cells']['walk/landing']
        out = rolling.remaining_landing(cell, 12)
        self.assertEqual(out['seconds'], 8)
        self.assertEqual(out['support'], 1)
        out = rolling.remaining_landing(cell, 20)
        self.assertIsNone(out['seconds'])
        self.assertEqual(out['reason'], 'beyond_historical_landing_duration')

    def test_overlapping_measurement_interval_is_preserved(self):
        cell = {'mode': 'absolute', 'samples': [{'seconds_bounds': [10, 11]}]}
        out = rolling.remaining_landing(cell, 10.5)
        self.assertEqual(out['envelope_seconds'], [0, 0.5])
        self.assertEqual(out['seconds'], 0.25)

    def test_time_limit_uses_native_strict_boundary_and_is_not_completion_probability(self):
        c = row()['mission']['capture']
        c['started_tick'] = 0
        before = rolling.budgets(c, 9000)
        self.assertFalse(before['time_limit_reached'])
        self.assertEqual(before['seconds_until_time_limit'], 1 / 60)
        self.assertTrue(rolling.budgets(c, 9001)['time_limit_reached'])
        self.assertEqual(before['completion_probability'], 'unknown')

    def test_retry_budget_separates_cover_solar_and_live_cancellations(self):
        c = row()['mission']['capture']
        c.update(replans=17, cover_replans=3, solar_replans=2, live_invalidations=10)
        result = rolling.budgets(c, 120)
        self.assertEqual(result['retries_remaining'], {'ordinary': 2, 'cover': 5, 'solar': 6})
        self.assertFalse(result['retry_limit_reached'])
        c['replans'] += 2
        self.assertTrue(rolling.budgets(c, 120)['retry_limit_reached'])

    def test_invalid_future_clocks_and_inconsistent_counters_are_rejected(self):
        for changes in [{'started_tick': 121}, {'cover_replans': 1}]:
            c = row()['mission']['capture']
            c.update(changes)
            with self.assertRaises(ValueError):
                rolling.budgets(c, 120)

    def test_elapsed_history_becomes_unknown_while_global_deadline_keeps_approaching(self):
        s, _ = state()
        for tick in range(121, 1321):
            result = s.observe(row(tick))
        self.assertIsNone(result['total_seconds'])
        self.assertIn('beyond_historical_landing_duration', result['unknown_reasons'])
        self.assertEqual(result['plan_age_seconds'], 20)
        self.assertEqual(result['elapsed_seconds'], 22)
        self.assertEqual(result['budget']['first_time_limit_tick'], 9061)


class ReplayContracts(unittest.TestCase):
    def test_same_planet_reselection_retires_old_attempt(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            rows = [row(120), row(121, selected=121)]
            (path/'trace.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in rows))
            output = io.StringIO()
            attempts = rolling.replay({'directory': '.', 'seats': [0]}, path, profile(), output)
            updates = list(map(json.loads, output.getvalue().splitlines()))
            self.assertEqual(len(attempts), 2)
            self.assertEqual(updates[1]['status'], 'attempt_left')
            self.assertEqual(updates[2]['selected_tick'], 121)

    def test_evaluation_pairs_fixed_horizons_and_retains_unknowns_and_ended_attempts(self):
        s, initial = state()
        original = s.original
        r = report([visit(arrived_tick=60, landed_tick=1500,
                          claimed_tick=1900, boarded_tick=2000, departed_tick=2100)], elapsed=2200)
        config = {'seats': [0], 'physics_valid_from_tick': 0}
        t = rolling.costs.build_trips(r, config)[0].result()
        run = {'label': 'example', 'seed': 5, 'source_commit': 'test',
               'trace_sha256': 'trace', 'report_sha256': 'report', 'trips': [t]}
        later = {**copy.deepcopy(initial), 'tick': 1020, 'status': 'unknown',
                 'unknown_reasons': ['awaiting_current_plan_evidence'], 'total_seconds': None}
        result = rolling.evaluate_run(run, [{'selection': s.selection, 'original_prediction': original}], [initial, later])
        checks = result['attempts'][0]['checkpoints']
        self.assertEqual([c['status'] for c in checks], ['estimate', 'unknown', 'already_landed', 'attempt_ended', 'attempt_ended'])
        summary = rolling.summarize([result])
        self.assertEqual(summary['0']['paired_completed'], 1)
        self.assertEqual(summary['15']['paired_completed'], 0)
        self.assertEqual(summary['15']['statuses']['unknown'], 1)

    def test_cli_writes_prediction_stream_before_outcomes_and_refuses_training_world(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            (path/'trace.jsonl').write_text(json.dumps(row())+'\n')
            (path/'report.json').write_text(json.dumps(report([visit()], elapsed=200)))
            p = profile()
            p['runs'] = [{'seed': 42, 'trace_sha256': 'not_this_trace', 'report_sha256': 'not_this_report'}]
            (path/'profile.json').write_text(json.dumps(p))
            manifest = {'version': 1, 'runtime_revision': 'test', 'runs': [
                {'label': 'test', 'directory': '.', 'seats': [0], 'source_commit': 'test', 'physics_valid_from_tick': 0}]}
            (path/'manifest.json').write_text(json.dumps(manifest))
            result = subprocess.run([sys.executable, str(SCRIPT), '--manifest', str(path/'manifest.json'),
                '--profile', str(path/'profile.json'), '--out', str(path/'out')], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('disjoint world seeds', result.stderr)
            prediction = json.loads((path/'out/predictions.json').read_text())
            self.assertNotIn('actual', prediction['runs'][0]['attempts'][0])
            self.assertFalse((path/'out/evaluation.json').exists())


if __name__ == '__main__':
    unittest.main()
