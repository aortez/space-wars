"""Prediction-time boundaries, missing evidence, and held-out timing contracts."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from test_trip_costs import row as recorded_row, pilot, visit, report

SCRIPT = Path(__file__).resolve().parents[1] / 'estimate-trip-costs.py'
SPEC = importlib.util.spec_from_file_location('trip_estimates', SCRIPT)
model = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(model)


def row(tick=120, *, selected=0, no_flag=False, **kwargs):
    r = recorded_row(tick, **kwargs)
    p, m = pilot(r), r['mission']
    m.update(policy=model.POLICY, events=[{'tick': selected, 'kind': 'selected', 'planet': 0}])
    if tick >= selected + 60:
        m['events'].append({'tick': selected + 60, 'kind': 'arrived', 'planet': 0})
    m['capture']['landing'] = {'landed_tick': None}
    survey = r['observation']['local']['landing_objective']
    survey['actor'] = p['owner']
    survey['objective']['planet'] = 0
    if no_flag:
        p['planet']['claim'].update(owner=None, flag=None)
        m['capture']['objective_route'] = None
    return r


def profile(category='walk'):
    cells = {}
    for phase in model.PHASES:
        residual = phase == 'claim' or category != 'no_flag' and phase in ['outbound', 'return_board']
        cells[f'{category}/{phase}'] = {
            'mode': 'residual' if residual else 'absolute',
            'samples': [dict(seed=1), dict(seed=2)],
            'observed_seconds_envelope': [2, 4],
            'median_midpoint_seconds': 3,
            'reference_seconds_domain': [0, 20] if residual else None,
        }
    return {'version': 1, 'model': model.MODEL, 'runtime_revision': 'test',
            'cells': cells, 'runs': []}


def read_rows(rows):
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory)
        (path / 'trace.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in rows))
        return model.read_snapshots({'directory': '.', 'seats': [0]}, path)


class FrozenPredictionTests(unittest.TestCase):
    def test_first_choice_is_prefix_invariant_even_when_later_route_is_better(self):
        first = row(120)
        later = row(180, length=25)
        later['mission']['capture']['site']['bearing'] = 20
        prefix = read_rows([first])[0]
        all_rows = read_rows([first, later])[0]
        self.assertEqual(prefix['choice'], all_rows['choice'])
        self.assertEqual(model.estimate(prefix['choice'], profile()),
                         model.estimate(all_rows['choice'], profile()))
        self.assertEqual(all_rows['post_choice_changes'], [
            {'tick': 180, 'dependency': 'site'}, {'tick': 180, 'dependency': 'route'}])

    def test_missing_source_is_not_repaired_by_a_later_identical_route(self):
        first, later = row(120), row(121)
        first['observation']['local']['landing_objective'] = None
        observed = read_rows([first, later])[0]
        self.assertIn('route_source_unverified_at_choice', observed['choice']['missing'])
        self.assertIsNone(model.estimate(observed['choice'], profile())['total_seconds'])

    def test_remote_selection_stays_unknown_despite_later_local_choice(self):
        first, later = row(120), row(180)
        pilot(first)['planet']['index'] = 1
        first['mission']['capture'] = None
        a = read_rows([first, later])[0]
        selection = model.estimate(a['selection'], profile())
        self.assertIn('remote_ground_unmeasured', selection['missing'])
        self.assertIsNone(selection['total_seconds'])
        self.assertEqual(a['choice']['source_tick'], 180)

    def test_first_invalid_choice_is_not_silently_replaced(self):
        first, later = row(120), row(121)
        first['mission']['capture']['objective_route']['returning']['partial'] = True
        a = read_rows([first, later])[0]
        self.assertEqual(a['choice']['source_tick'], 120)
        self.assertIn('incomplete_round_trip', a['choice']['missing'])

    def test_landed_or_on_foot_first_observation_cannot_be_used_as_a_prediction(self):
        for foot, landed in [(True, None), (False, 119)]:
            r = row(120, foot=foot)
            r['mission']['capture']['landing']['landed_tick'] = landed
            self.assertIsNone(read_rows([r])[0]['choice'])

    def test_each_reselection_has_a_new_clock(self):
        attempts = read_rows([row(120), row(181, selected=180)])
        self.assertEqual([a['choice']['selected_tick'] for a in attempts], [0, 180])
        self.assertAlmostEqual(attempts[1]['choice']['elapsed_seconds'], 1 / 60)

    def test_future_mission_events_fail_closed(self):
        r = row()
        r['mission']['events'].append({'tick': 121, 'kind': 'departed', 'planet': 0})
        with self.assertRaisesRegex(ValueError, 'future event'):
            read_rows([r])

    def test_actor_and_tick_must_match_the_trace(self):
        for name, value in [('owner', 'player_2'), ('tick', 121)]:
            r = row()
            pilot(r)[name] = value
            with self.assertRaises(ValueError):
                read_rows([r])


class PhaseEstimateTests(unittest.TestCase):
    def test_elapsed_and_remaining_use_the_same_origin_without_double_counting(self):
        snap = model.snapshot(row(), 0, 'local_choice')
        result = model.estimate(snap, profile())
        self.assertEqual(snap['elapsed_transfer_seconds'], 1)
        self.assertEqual(snap['elapsed_local_approach_seconds'], 1)
        # 10 + 10 route references, 6 claim seconds, six 3-second residuals/allowances.
        self.assertEqual(result['remaining_seconds'], 44)
        self.assertEqual(result['total_seconds'], 46)
        self.assertEqual(result['total_historical_envelope_seconds'], [40, 52])

    def test_validation_does_not_reset_source_age_or_grant_permission(self):
        r = row()
        survey = r['observation']['local']['landing_objective']
        survey.update(tick=100, validated_tick=120)
        d = model.snapshot(r, 0, 'local_choice')['dependencies']
        self.assertEqual(d['route_source_tick'], 100)
        self.assertEqual(d['route_validated_tick'], 120)
        self.assertEqual(d['route_age_ticks'], 20)
        self.assertFalse(d['renewed_physical_validity'])

    def test_future_expired_unvalidated_and_wrong_actor_evidence_are_unknown(self):
        for change in [{'tick': 121}, {'validated_tick': 121},
                       {'tick': 0, 'validated_tick': 121}, {'validated_tick': None},
                       {'actor': 'player_2'}]:
            with self.subTest(change=change):
                r = row(121)
                r['observation']['local']['landing_objective'].update(change)
                # Future cases relative to observation 120.
                if change in [{'tick': 121}, {'validated_tick': 121}]:
                    r = row(120)
                    r['observation']['local']['landing_objective'].update(change)
                self.assertIsNone(model.estimate(model.snapshot(r, 0, 'local_choice'), profile())['total_seconds'])

    def test_changed_revision_prevents_using_a_complete_route(self):
        r = row()
        pilot(r)['planet']['revision'] = 1
        snap = model.snapshot(r, 0, 'local_choice')
        self.assertIn('changed_material', snap['missing'])
        self.assertIsNone(model.estimate(snap, profile())['total_seconds'])

    def test_missing_queries_also_block_empirical_no_flag_estimates(self):
        r = row(no_flag=True)
        pilot(r)['queries_ready'] = False
        result = model.estimate(model.snapshot(r, 0, 'local_choice'), profile('no_flag'))
        self.assertIsNone(result['total_seconds'])
        self.assertIn('local_queries_unavailable', result['missing'])

    def test_no_flag_uses_empirical_walk_costs_instead_of_zero(self):
        snap = model.snapshot(row(no_flag=True), 0, 'local_choice')
        result = model.estimate(snap, profile('no_flag'))
        self.assertIsNone(result['phases']['outbound']['reference_seconds'])
        self.assertEqual(result['phases']['outbound']['estimate_seconds'], 3)
        self.assertEqual(result['phases']['return_board']['estimate_seconds'], 3)
        self.assertEqual(result['total_seconds'], 23)

    def test_missing_phase_is_not_replaced_with_zero_in_the_total(self):
        frozen = profile()
        del frozen['cells']['walk/departure']
        result = model.estimate(model.snapshot(row(), 0, 'local_choice'), frozen)
        self.assertIsNone(result['total_seconds'])
        self.assertEqual(result['unknown_phases'], ['departure'])
        self.assertEqual(result['phases']['outbound']['estimate_seconds'], 13)

    def test_ground_reference_outside_sample_domain_is_explicitly_unknown(self):
        result = model.estimate(model.snapshot(row(length=150), 0, 'local_choice'), profile())
        self.assertIn('reference_outside_calibration_domain', result['phases']['outbound']['unknown_reasons'])
        self.assertIsNone(result['total_seconds'])

    def test_unknown_ship_geometry_is_never_safe_or_pilot_visibility(self):
        r = row()
        r['observation']['local']['combat']['target'] = None
        exposure = model.snapshot(r, 0, 'local_choice')['exposure']
        self.assertIsNone(exposure['ship_geometry_now'])
        self.assertEqual(exposure['future_ship_exposure'], 'unknown')
        self.assertEqual(exposure['pilot_visibility'], 'unmeasured')

    def test_estimator_does_not_mutate_snapshot_or_profile(self):
        snap, frozen = model.snapshot(row(), 0, 'local_choice'), profile()
        before = copy.deepcopy([snap, frozen])
        result = model.estimate(snap, frozen)
        result['dependencies']['site']['bearing'] = 100
        self.assertEqual([snap, frozen], before)


class CalibrationContracts(unittest.TestCase):
    def fixture(self):
        snap = model.snapshot(row(), 0, 'local_choice')
        bounds = {'selected': 0, 'arrived': 60, 'landed': 300,
                  'exited': 301, 'claim_started': 901, 'claimed': 1260,
                  'boarded': 1860, 'departed': 2040}
        trip = {'seat': 0, 'planet': 0, 'selected_tick': 0, 'stopped_tick': 2040,
                'ending': 'completed', 'reason': None,
                'milestones': {k: [v, v] for k, v in bounds.items()}, 'phases': {}}
        for name, begins, ends in model.costs.PHASES:
            duration = (bounds[ends] - bounds[begins]) / 60
            trip['phases'][name] = {'status': 'completed', 'physics_valid': True,
                                   'seconds_bounds': [duration, duration]}
        return snap, trip

    def test_landing_phase_begins_at_prediction_tick_not_arrival(self):
        snap, trip = self.fixture()
        self.assertEqual(model.actual_phases(trip, snap)['landing']['seconds_bounds'], [3, 3])
        self.assertEqual(trip['phases']['landing']['seconds_bounds'], [4, 4])

    def test_unfinished_and_sparse_phases_cannot_train_a_finished_duration(self):
        snap, trip = self.fixture()
        trip['phases']['departure']['status'] = 'censored'
        trip['phases']['outbound']['seconds_bounds'] = [10, 15]
        cells = {}
        model.add_samples(cells, {'label': 'train', 'seed': 1},
                          {'choice': snap, 'post_choice_changes': []}, trip)
        self.assertEqual(cells['walk/departure']['samples'], [])
        self.assertEqual(cells['walk/outbound']['samples'], [])
        self.assertIn('censored', cells['walk/departure']['excluded'][0]['reasons'])
        self.assertIn('uncertain_boundary', cells['walk/outbound']['excluded'][0]['reasons'])
        self.assertEqual(len(cells['walk/claim']['samples']), 1)

    def test_changed_dependencies_exclude_later_phases_but_preserve_earlier_evidence(self):
        snap, trip = self.fixture()
        cells = {}
        model.add_samples(cells, {'label': 'train', 'seed': 1},
                          {'choice': snap, 'post_choice_changes': [{'tick': 1000, 'dependency': 'revision'}]}, trip)
        self.assertEqual(len(cells['walk/outbound']['samples']), 1)
        self.assertEqual(cells['walk/return_board']['samples'], [])
        self.assertIn('changed_dependencies_after_choice', cells['walk/return_board']['excluded'][0]['reasons'])

    def test_censored_result_does_not_report_finished_error_or_envelope_coverage(self):
        snap, trip = self.fixture()
        trip['ending'] = 'abandoned'
        trip['phases']['total'].update(status='censored', seconds_bounds=[100, 100])
        actual = model.compare(trip, snap, model.estimate(snap, profile()))
        self.assertIsNone(actual['total_error_seconds_bounds'])
        self.assertIsNone(actual['total_historical_envelope_contains_actual_bounds'])
        self.assertTrue(actual['censored_elapsed_exceeds_historical_max'])

    def test_reset_telemetry_cannot_use_a_choice_after_the_actual_first_landing(self):
        snap, trip = self.fixture()
        trip['milestones']['landed'] = [119, 119]
        result = model.compare(trip, snap, model.estimate(snap, profile()))
        self.assertFalse(result['prediction_scope_valid'])
        self.assertIsNone(result['total_error_seconds_bounds'])
        self.assertIsNone(result['total_historical_envelope_contains_actual_bounds'])
        self.assertIsNone(result['censored_elapsed_exceeds_historical_max'])
        self.assertTrue(all(p['error_seconds_bounds'] is None for p in result['actual_phases'].values()))

    def test_training_and_evaluation_cannot_share_a_seed_or_recording(self):
        p = {'runs': [{'seed': 1, 'trace_sha256': 'a', 'report_sha256': 'b'}]}
        for r in [{'seed': 1, 'trace_sha256': 'c', 'report_sha256': 'd'},
                  {'seed': 2, 'trace_sha256': 'a', 'report_sha256': 'd'},
                  {'seed': 2, 'trace_sha256': 'c', 'report_sha256': 'b'}]:
            with self.assertRaises(ValueError):
                model.ensure_independent(p, r)
        model.ensure_independent(p, {'seed': 2, 'trace_sha256': 'c', 'report_sha256': 'd'})

    def test_error_summary_does_not_invent_exact_errors_from_sparse_boundaries(self):
        result = model.error_summary([[2, 2], [-10, 10], None, [-4, -4]])
        self.assertEqual(result['completed_exact_comparisons'], 2)
        self.assertEqual(result['median_absolute_error_seconds'], 3)
        self.assertEqual(result['signed_error_range_seconds'], [-4, 2])

    def test_cli_freezes_predictions_and_retains_attempts_without_a_choice(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            rows = [row(0), row(120)]
            rows[0]['mission']['capture'] = None
            rows[1]['mission']['capture'] = None
            (base / 'trace.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in rows))
            (base / 'report.json').write_text(json.dumps(report([visit()], elapsed=180)))
            (base / 'profile.json').write_text(json.dumps(profile()))
            manifest = {'version': 1, 'runtime_revision': 'test', 'runs': [
                {'label': 'probe', 'directory': '.', 'seats': [0], 'source_commit': 'test',
                 'physics_valid_from_tick': 0}]}
            (base / 'inputs.json').write_text(json.dumps(manifest))
            subprocess.run([sys.executable, str(SCRIPT), 'evaluate', '--manifest', str(base/'inputs.json'),
                            '--profile', str(base/'profile.json'), '--out', str(base/'out')],
                           check=True, capture_output=True, text=True)
            frozen = json.loads((base/'out/predictions.json').read_text())
            a = frozen['runs'][0]['attempts'][0]
            self.assertNotIn('actual', a)
            self.assertNotIn('post_choice_changes', a)
            self.assertIsNone(a['prediction'])
            self.assertIsNone(a['selection_prediction']['total_seconds'])
            evaluated = json.loads((base/'out/evaluation.json').read_text())
            self.assertEqual(evaluated['runs'][0]['attempts'][0]['recorded_attempt']['ending'], 'match_end')
            self.assertEqual(evaluated['summary']['recorded_attempts'], 1)
            self.assertEqual(evaluated['summary']['attempts_without_local_choice'], 1)


if __name__ == '__main__':
    unittest.main()
