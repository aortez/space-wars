"""Composition must preserve clocks, evidence, unknowns and comparison cohorts."""
import copy
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from test_landing_phases import row as phase_row, profile as phase_profile
from test_rolling_trip_estimates import profile as ground_profile
from test_trip_estimates import profile as category_profile
from test_trip_costs import pilot, report, visit
from test_walking_calibration import profile as walking_profile
from test_controlled_ground import report as controlled_report, completed_observation

SCRIPT = Path(__file__).resolve().parents[1] / 'compose-trip-estimates.py'
SPEC = importlib.util.spec_from_file_location('composition', SCRIPT)
model = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(model)


def row(tick=120, **kwargs):
    return phase_row(tick, length=25, **kwargs)


def profiles():
    walk = walking_profile()
    walk.update(version=1, model=model.walking.MODEL, runtime_revision='test',
        baseline_profile_sha256=None, runs=[], support={'minimum_samples': 6,
            'minimum_worlds': 3, 'minimum_reference_span_seconds': 4.0})
    ground = ground_profile()
    ground['cells'].update(category_profile('no_flag')['cells'])
    ground['cells']['no_flag/landing'] = copy.deepcopy(ground['cells']['walk/landing'])
    return [ground, phase_profile(), walk]


def state(r=None, p=None):
    r = r or row()
    state = model.ComposedTrip(model.frozen.snapshot(r, 0, 'mission_selection'), *(p or profiles()))
    return state, state.observe(r)


def controlled_row(tick=120, start=100, **kwargs):
    r = row(tick, **kwargs)
    return {'version': 1, 'scope': model.controlled.SCOPE, 'tick': tick, 'seat': 0,
        'capture_started_tick': start, 'observation': r['observation']['local'],
        'capture': r['mission']['capture'], 'controls': {}}


class CompositionTests(unittest.TestCase):
    def test_each_leg_claim_and_elapsed_time_are_counted_once(self):
        s, result = state()
        # 2 elapsed + 11 landing + 3 exit + 8.25 outbound + 9 claim
        # + 8.25 return/board + 3 departure.
        self.assertEqual(result['total_seconds'], 44.5)
        self.assertEqual(result['remaining_seconds'], 42.5)
        self.assertEqual(result['phase_only_total_seconds'], 44)
        self.assertEqual(result['age_only_total_seconds'], 48)
        self.assertEqual(result['original_total_seconds'], 48)
        self.assertEqual(result['total_historical_envelope_seconds'], [40.5, 48.5])
        self.assertEqual(s.original['phases']['outbound']['estimate_seconds'], 8)
        self.assertFalse(result['physical_permissions'])

    def test_no_flag_and_other_categories_keep_their_existing_models(self):
        for category in ['no_flag', 'jump', 'crossing']:
            p = profiles()
            p[0]['cells'].update(category_profile(category)['cells'])
            snap = model.frozen.snapshot(row(no_flag=category == 'no_flag'), 0, 'local_choice')
            snap['category'] = category
            base = model.frozen.estimate(snap, p[0])
            self.assertEqual(model.ground_tail(base, p[2]), {n: base['phases'][n] for n in model.TAIL})
        _, result = state(row(no_flag=True))
        self.assertEqual(result['total_seconds'], result['phase_only_total_seconds'])
        self.assertEqual(result['ground_model'], model.frozen.MODEL)

    def test_affine_domain_can_gain_coverage_without_requiring_old_walking_domain(self):
        p = profiles()
        p[0]['cells']['walk/return_board']['reference_seconds_domain'] = [0, 4]
        _, result = state(p=p)
        self.assertIsNone(result['phase_only_total_seconds'])
        self.assertEqual(result['total_seconds'], 44.5)

    def test_short_or_long_walk_never_falls_back_to_the_old_model(self):
        for length in [1, 40]:
            _, result = state(phase_row(length=length))
            self.assertIsNotNone(result['phase_only_total_seconds'])
            self.assertIsNone(result['total_seconds'])
            self.assertEqual(result['status'], 'unknown')
            self.assertIn('reference_outside_training_domain', result['unknown_tail_phases']['outbound'])

    def test_missing_exit_claim_departure_or_landing_cannot_be_replaced_with_zero(self):
        for name in ['exit', 'claim', 'departure', 'landing']:
            p = profiles()
            if name == 'landing':
                p[1]['cells'] = {}
            else:
                del p[0]['cells']['walk/' + name]
            _, result = state(p=p)
            self.assertIsNone(result['total_seconds'])
            self.assertIsNone(result['total_historical_envelope_seconds'])

    def test_live_route_refresh_never_repairs_the_original_prediction(self):
        first = row()
        first['observation']['local']['landing_objective'] = None
        s, before = state(first)
        self.assertIsNone(before['total_seconds'])
        original = copy.deepcopy(s.record())
        after = s.observe(row(121))
        self.assertIsNotNone(after['total_seconds'])
        self.assertEqual(s.first_composed, original['first_composed_prediction'])
        self.assertEqual(s.original, original['original_prediction'])
        self.assertEqual(after['reference_acquired_tick'], 121)

    def test_stale_or_invalid_route_waits_for_current_evidence_and_keeps_phase_age(self):
        s, before = state()
        r = row(121)
        r['observation']['local']['objective_work'] = 'stale'
        self.assertIsNone(s.observe(r)['total_seconds'])
        r = row(122, revision=1)
        r['observation']['local']['landing_objective'] = None
        self.assertIsNone(s.observe(r)['total_seconds'])
        after = s.observe(row(123, revision=1))
        self.assertEqual(after['reference_acquired_tick'], 123)
        self.assertEqual(after['flight_phase']['age_seconds'], 3 / 60)
        self.assertEqual(after['budget']['first_time_limit_tick'], before['budget']['first_time_limit_tick'])

    def test_retry_retains_elapsed_time_deadline_and_original_costs(self):
        s, before = state()
        after = s.observe(row(121, retry=1))
        self.assertEqual(after['flight_phase']['context'], 'retry')
        self.assertEqual(after['plan_age_seconds'], 0)
        self.assertAlmostEqual(after['total_seconds'], before['total_seconds'] + 1 / 60)
        self.assertEqual(after['budget']['first_time_limit_tick'], before['budget']['first_time_limit_tick'])
        self.assertEqual(s.first_composed, before)

    def test_observation_gap_and_expired_capture_budget_stay_unknown(self):
        s, _ = state()
        self.assertIn('phase_entry_unobserved', s.observe(row(180))['unknown_reasons'])
        r = row(9061)
        r['mission']['capture']['goal_since'] = 9061
        result = s.observe(r)
        self.assertIn('capture_limit_reached', result['unknown_reasons'])
        self.assertIsNone(result['total_seconds'])

    def test_landing_stops_the_prelanding_forecast(self):
        s, _ = state()
        result = s.observe(row(121, phase='landed'))
        self.assertEqual(result['status'], 'landed')
        self.assertIsNone(result['total_seconds'])
        self.assertIsNone(s.observe(row(122)))

    def test_composition_does_not_mutate_profiles_or_rows(self):
        r, p = row(), profiles()
        before = copy.deepcopy([r, p])
        state(r, p)
        self.assertEqual([r, p], before)


class EvaluationTests(unittest.TestCase):
    def test_failures_missing_forecasts_and_ended_attempts_keep_separate_denominators(self):
        s, initial = state()
        trip = model.costs.build_trips(report([visit(arrived_tick=60, landed_tick=1500,
            claimed_tick=1900, boarded_tick=2000, departed_tick=2100)], elapsed=2200),
            {'seats': [0], 'physics_valid_from_tick': 0})[0].result()
        source = {'label': 'test', 'seed': 42, 'source_commit': 'test', 'trace_sha256': 'trace',
                  'report_sha256': 'report', 'trips': [trip]}
        later = {**initial, 'tick': 1020, 'status': 'unknown', 'total_seconds': None,
                 'unknown_reasons': ['unknown_surface_or_departure_cost']}
        result = model.evaluate_run(source, [s.record()], [initial, later])
        summary = model.summarize([result])
        self.assertEqual(summary['checkpoints']['0']['paired_completed'], 1)
        self.assertEqual(summary['checkpoints']['15']['coverage_lost'], 1)
        self.assertEqual(summary['checkpoints']['15']['paired_completed'], 0)
        self.assertEqual(summary['checkpoints']['30']['statuses'], {'already_landed': 1})
        self.assertEqual(summary['checkpoints']['60']['statuses'], {'attempt_ended': 1})
        trip['ending'] = 'abandoned'
        failed = model.summarize([model.evaluate_run(source, [s.record()], [initial])])
        self.assertEqual(failed['endings'], {'abandoned': 1})
        self.assertEqual(failed['checkpoints']['0']['numeric']['combined'], 1)
        self.assertEqual(failed['checkpoints']['0']['paired_completed'], 0)

    def test_controlled_setup_and_frame_exit_never_become_normal_mission_predictions(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            def replay(rows):
                trace = path / 'trace.jsonl'
                trace.write_text(''.join(json.dumps(r) + '\n' for r in rows))
                return model.replay_controlled({'directory': '.', 'seat': 0}, path,
                    lambda selection: model.ComposedTrip(selection, *profiles()), io.StringIO(), model.costs.file_hash(trace))
            self.assertEqual(replay([controlled_row(start=None)]), [])
            first, foreign = controlled_row(), controlled_row(121)
            foreign['observation']['combat']['recovery']['flight']['pilot']['planet']['index'] = 1
            records = replay([first, foreign, controlled_row(122)])
            self.assertEqual(records[0]['selection']['selected_tick'], 100)
            self.assertAlmostEqual(records[0]['first_composed_prediction']['total_seconds'], 44.5 - 100 / 60)
            self.assertEqual(records[0]['last_observed_tick'], 121)
            self.assertIsNone(replay([foreign, controlled_row(122)])[0]['original_prediction'])
            with self.assertRaisesRegex(ValueError, 'start changed'):
                replay([first, controlled_row(121, start=101)])

    def test_profile_hash_runtime_and_support_contracts_are_checked(self):
        p = profiles()
        p[1]['ground_profile_sha256'] = p[2]['baseline_profile_sha256'] = 'ground'
        model.validate_profiles({'runtime_revision': 'test'}, p, 'ground')
        for index, key, value in [(0, 'model', 'wrong'), (1, 'runtime_revision', 'old'),
                (1, 'ground_profile_sha256', 'other'), (1, 'min_attempts', 1),
                (2, 'baseline_profile_sha256', 'other'), (2, 'support', {})]:
            changed = copy.deepcopy(p)
            changed[index][key] = value
            with self.assertRaises(ValueError):
                model.validate_profiles({'runtime_revision': 'test'}, changed, 'ground')

    def test_cli_binds_recordings_checks_all_training_worlds_and_writes_predictions_first(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            (path / 'trace.jsonl').write_text(json.dumps(row()) + '\n')
            (path / 'report.json').write_text(json.dumps(report([visit()], elapsed=200)))
            config = {'label': 'test', 'directory': '.', 'seats': [0], 'source_commit': 'test', 'physics_valid_from_tick': 0}
            manifest = {'version': 1, 'runtime_revision': 'test', 'runs': [config]}
            model.frozen.write_json(path / 'manifest.json', manifest)
            p = profiles()
            model.frozen.write_json(path / 'ground.json', p[0])
            ground_hash = model.costs.file_hash(path / 'ground.json')
            p[1]['ground_profile_sha256'] = p[2]['baseline_profile_sha256'] = ground_hash
            for name, profile in zip(['phase', 'walking'], p[1:]):
                model.frozen.write_json(path / (name + '.json'), profile)
            output = io.StringIO()
            attempts = model.rolling.replay(config, path, p[0], output,
                trip_factory=lambda selection, _: model.phase.PhaseTrip(selection, *p[:2]))
            run = model.costs.analyze_run(config, path)
            evaluated = model.phase.evaluate_run(run, attempts, map(json.loads, output.getvalue().splitlines()))
            source = {'version': 1, 'model': model.phase.MODEL,
                'manifest_sha256': model.costs.file_hash(path / 'manifest.json'),
                'ground_profile_sha256': ground_hash, 'phase_profile_sha256': model.costs.file_hash(path / 'phase.json'),
                'runs': [evaluated]}
            model.frozen.write_json(path / 'source.json', source)
            command = [sys.executable, str(SCRIPT), '--manifest', str(path / 'manifest.json'),
                '--source-evaluation', str(path / 'source.json'), '--out', str(path / 'out')]
            for name in ['ground', 'phase', 'walking']:
                command += ['--' + name + '-profile', str(path / (name + '.json'))]
            process = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(process.returncode, 0, process.stderr)
            self.assertTrue((path / 'out' / 'evaluation.json').exists())
            process = subprocess.run(command + ['--walk-model', 'short-and-affine',
                '--out', str(path / 'regimes')], capture_output=True, text=True)
            self.assertEqual(process.returncode, 0, process.stderr)
            regime_result = json.loads((path / 'regimes' / 'evaluation.json').read_text())
            self.assertEqual(regime_result['model'], model.REGIME_MODEL)
            legs = regime_result['runs'][0]['attempts'][0]['checkpoints'][0]['walking_legs']
            self.assertEqual(legs['outbound']['regime'], 'moderate_affine')

            # Controlled evaluations have their own normalized outcome schema
            # and capture origin; exercise the real CLI adapter, not just rows.
            child = path / 'controlled'
            child.mkdir()
            raw = controlled_row()
            (child / 'trace.jsonl').write_text(json.dumps(raw) + '\n')
            (child / 'report.json').write_text(json.dumps(controlled_report()))
            cm = {'version': 1, 'scope': model.controlled.SCOPE, 'runtime_revision': 'test',
                  'runs': [{'label': 'trial', 'directory': '.', 'seat': 0, 'seed': 42}]}
            model.frozen.write_json(child / 'manifest.json', cm)
            cr = model.controlled.adapt(raw)
            cs = model.ComposedTrip(model.frozen.snapshot(cr, 100, 'mission_selection'), *p)
            cs.observe(cr)
            trip = model.controlled.make_trip(controlled_report(), completed_observation(), 0).result()
            csource = {'version': 1, 'model': 'controlled-ground-evaluation-v1', 'scope': model.controlled.SCOPE,
                'manifest_sha256': model.costs.file_hash(child / 'manifest.json'), 'profile_sha256': ground_hash,
                'runs': [{'label': 'trial', 'seed': 42, 'source_commit': 'test', 'outcome': 'completed',
                    'trace_sha256': model.costs.file_hash(child / 'trace.jsonl'),
                    'report_sha256': model.costs.file_hash(child / 'report.json'),
                    'attempts': [{'selection': cs.selection, 'prediction': cs.original, 'actual_trip': trip}]}]}
            model.frozen.write_json(child / 'source.json', csource)
            process = subprocess.run(command + ['--manifest', str(child / 'manifest.json'),
                '--source-evaluation', str(child / 'source.json'), '--out', str(child / 'out')], capture_output=True, text=True)
            self.assertEqual(process.returncode, 0, process.stderr)
            result = json.loads((child / 'out' / 'evaluation.json').read_text())
            self.assertEqual(result['summary']['trial_outcomes'], {'completed': 1})
            self.assertEqual(result['runs'][0]['attempts'][0]['selection']['selected_tick'], 100)

            for index, name in enumerate(['ground', 'phase', 'walking']):
                bad = copy.deepcopy(p)
                bad[index]['runs'] = [model.frozen.source_summary(run)]
                model.frozen.write_json(path / 'ground.json', bad[0])
                gh = model.costs.file_hash(path / 'ground.json')
                bad[1]['ground_profile_sha256'] = bad[2]['baseline_profile_sha256'] = gh
                for filename, profile in zip(['phase', 'walking'], bad[1:]):
                    model.frozen.write_json(path / (filename + '.json'), profile)
                source.update(ground_profile_sha256=gh, phase_profile_sha256=model.costs.file_hash(path / 'phase.json'))
                model.frozen.write_json(path / 'source.json', source)
                process = subprocess.run(command + ['--out', str(path / name)], capture_output=True, text=True)
                self.assertNotEqual(process.returncode, 0)
                self.assertIn('disjoint world seeds', process.stderr)
                self.assertTrue((path / name / 'predictions.json').exists())
                self.assertFalse((path / name / 'evaluation.json').exists())


if __name__ == '__main__':
    unittest.main()
