"""A conditional walking fit cannot absorb failures or later route knowledge."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

from test_controlled_ground import raw
from test_ground_cost_validation import attempt

SPEC = importlib.util.spec_from_file_location('walking_costs',
    Path(__file__).resolve().parents[1] / 'calibrate-walking-costs.py')
walking = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(walking)


def samples():
    return [{'seed': seed, 'reference_seconds': x, 'actual_seconds': 2 + 1.25 * x,
             'excluded_reasons': []} for seed in range(3) for x in [1, 7]]


def profile():
    return {'cells': {phase: walking.fit_cell(samples()) for phase in walking.LEGS}}


def audit():
    return {'route_modes': ['walk'], 'bad_route_ticks': 0, 'posture_recovery_ticks': 0,
        'primary_ticks': 0, 'ticks': 600, 'dense': True}


class FittingTests(unittest.TestCase):
    def test_recovers_known_overhead_and_scale_without_optimizing_on_validation(self):
        fit = walking.fit_cell(samples())['fits']['affine']
        self.assertAlmostEqual(fit['overhead_seconds'], 2)
        self.assertAlmostEqual(fit['reference_scale'], 1.25)
        self.assertAlmostEqual(fit['world_weighted_squared_error'], 0)

    def test_more_trials_of_one_world_do_not_increase_its_total_weight(self):
        s = samples()
        s[0]['actual_seconds'] += 3
        before = walking.linear_fit(s, True)
        after = walking.linear_fit(s + [r for r in s if r['seed'] == 0] * 5, True)
        for key in ['overhead_seconds', 'reference_scale', 'world_weighted_squared_error']:
            self.assertAlmostEqual(before[key], after[key])

    def test_negative_unconstrained_intercept_or_slope_uses_nonnegative_boundary(self):
        s = samples()
        for r in s:
            r['actual_seconds'] = 2 * r['reference_seconds'] - 1
        fit = walking.linear_fit(s, True)
        self.assertEqual(fit['overhead_seconds'], 0)
        for r in s:
            r['actual_seconds'] = 10 - r['reference_seconds']
        fit = walking.linear_fit(s, True)
        self.assertEqual(fit['reference_scale'], 0)

    def test_offset_only_is_an_explicit_comparator(self):
        fit = walking.linear_fit(samples(), False)
        self.assertEqual(fit['reference_scale'], 1)
        self.assertAlmostEqual(fit['overhead_seconds'], 3)

    def test_support_requires_worlds_sample_count_and_distance_span(self):
        for s in [samples()[:4], [dict(s, seed=0) for s in samples()],
                  [dict(s, reference_seconds=3) for s in samples()]]:
            self.assertFalse(walking.fit_cell(s)['fits'])

    def test_excluded_failure_is_retained_but_cannot_change_coefficients(self):
        s = samples() + [{'seed': 99, 'reference_seconds': 5, 'actual_seconds': 100,
                          'excluded_reasons': ['censored']}]
        cell = walking.fit_cell(s)
        self.assertEqual(cell['fits'], walking.fit_cell(samples())['fits'])
        self.assertEqual(len(cell['excluded']), 1)

    def test_invalid_measurements_are_rejected(self):
        for value in [-1, float('nan'), float('inf'), True]:
            s = samples()
            s[0]['actual_seconds'] = value
            with self.assertRaises(ValueError):
                walking.linear_fit(s, True)


class ForecastTests(unittest.TestCase):
    def forecast(self, choice=None):
        a = attempt()
        choice = copy.deepcopy(choice or a['choice'])
        choice['references'].update(outbound=4, return_board=5)
        return choice, a['prediction']

    def test_forecast_uses_only_original_references_and_preserves_claim(self):
        choice, base = self.forecast()
        original = copy.deepcopy((choice, base))
        result = walking.predict(choice, base, profile(), 'affine')
        self.assertEqual(result['outbound']['estimate_seconds'], 7)
        self.assertEqual(result['return_board']['estimate_seconds'], 8.25)
        self.assertEqual(result['ground']['estimate_seconds'], 15.25 + base['phases']['claim']['estimate_seconds'])
        self.assertEqual((choice, base), original)

    def test_missing_choice_invalid_evidence_and_powered_choices_stay_unknown(self):
        choice, base = self.forecast()
        self.assertIsNone(walking.predict(None, None, profile(), 'affine')['ground']['estimate_seconds'])
        for altered in [dict(choice, missing=['route_not_current_at_choice']), dict(choice, category='crossing')]:
            self.assertIsNone(walking.predict(altered, base, profile(), 'affine')['ground']['estimate_seconds'])

    def test_no_extrapolation_or_baseline_fallback_outside_training_domain(self):
        choice, base = self.forecast()
        for ref in [0, 7.01, float('nan')]:
            choice['references']['outbound'] = ref
            result = walking.predict(choice, base, profile(), 'affine')
            self.assertIn('reference_outside_training_domain', result['outbound']['unknown_reasons'])
            self.assertIsNone(result['ground']['estimate_seconds'])

    def test_sparse_profile_does_not_invent_a_fit(self):
        p = {'cells': {phase: walking.fit_cell([]) for phase in walking.LEGS}}
        choice, base = self.forecast()
        result = walking.predict(choice, base, p, 'affine')
        self.assertIn('insufficient_worlds', result['outbound']['unknown_reasons'])


class MeasurementTests(unittest.TestCase):
    def fixture(self):
        a = walking.ground.assess(attempt(), 4)
        return {'label': 'case', 'seed': 42, 'outcome': 'abandoned'}, a

    def test_completed_outbound_can_train_despite_later_return_failure(self):
        r, a = self.fixture()
        m = walking.measurement(r, a, 'outbound', audit())
        self.assertEqual(m['ending'], 'abandoned')
        self.assertFalse(m['excluded_reasons'])

    def test_censoring_changed_dependencies_and_posture_are_separate_exclusions(self):
        for fault, reason in [('censored', 'censored'), ('changed', 'changed_dependencies'), ('posture', 'posture_recovery'), ('primary', 'primary_input'), ('powered', 'execution_not_uninterrupted_walk'), ('sparse', 'execution_not_dense')]:
            r, a = self.fixture()
            d = audit()
            if fault == 'censored':
                a['phases']['outbound']['actual']['status'] = 'censored'
            elif fault == 'changed':
                a['phases']['outbound']['evidence_changes'] = [{'tick': 400, 'dependency': 'site'}]
            elif fault == 'posture':
                d['posture_recovery_ticks'] = 1
            elif fault == 'primary':
                d['primary_ticks'] = 1
            elif fault == 'powered':
                d['route_modes'].append('powered')
            else:
                d['dense'] = False
            self.assertIn(reason, walking.measurement(r, a, 'outbound', d)['excluded_reasons'])

    def test_later_powered_route_cannot_hide_behind_initial_walk_or_goal_name(self):
        r1, r2 = raw(301, foot=True), raw(302, foot=True)
        for r in [r1, r2]:
            r['posture'] = {'balance': 'Balanced'}
            r['capture']['ground'] = {'destination': 'flag', 'goal': 'jump',
                'route': {'flights': 0, 'jumps': 0, 'failure': None, 'partial': False}}
        r2['capture']['ground']['route']['flights'] = 1
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'trace.jsonl'
            content = ''.join(json.dumps(r) + '\n' for r in [r1, r2]).encode()
            path.write_bytes(content)
            a = {'execution': {'outbound': {'start_tick': 301, 'stop_tick': 303, 'dense': True, 'interval_ticks': 2}}}
            result = walking.execution_audit(path, a, hashlib.sha256(content).hexdigest())['outbound']
            self.assertEqual(result['route_modes'], ['powered', 'walk'])
            self.assertEqual(result['primary_ticks'], 0)
            with self.assertRaises(ValueError):
                walking.execution_audit(path, a, 'changed')

    def test_reused_world_or_recording_cannot_enter_validation(self):
        source = {'seed': 1, 'trace_sha256': 'trace', 'report_sha256': 'report'}
        p = {'runs': [source]}
        for altered in [dict(source, seed=2), dict(source, trace_sha256='new', report_sha256='new')]:
            with self.assertRaises(ValueError):
                walking.frozen.ensure_independent(p, altered)

    def test_changed_walks_stay_in_all_results_and_pairing_uses_common_coverage(self):
        run, a = self.fixture()
        run['attempts'] = [a]
        record = {'label': 'case', 'legs': {p: {'excluded_reasons': ['changed_dependencies']} for p in walking.LEGS}}
        pred = {'label': 'case', **{m: {p: {'estimate_seconds': 5} for p in walking.LEGS + ['ground']}
                                  for m in walking.VARIANTS}}
        pred['affine']['return_board']['estimate_seconds'] = None
        _, summary = walking.compare_forecasts([pred], [run], [record])
        self.assertEqual(summary['outbound']['all_completed']['affine']['completed_exact_comparisons'], 1)
        self.assertEqual(summary['outbound']['paired_clean_walk']['affine']['completed_exact_comparisons'], 0)
        self.assertEqual(summary['return_board']['all_completed']['baseline']['completed_exact_comparisons'], 1)
        self.assertEqual(summary['return_board']['paired_baseline_affine']['baseline']['completed_exact_comparisons'], 0)

    def test_manifest_and_initial_placement_mismatch_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            report = base / 'report.json'
            report.write_text(json.dumps({'offset': 0.6}))
            manifest = {'scope': walking.controlled.SCOPE, 'runtime_revision': 'runtime',
                'runs': [{'label': 'case', 'seed': 2, 'offset': 0.6, 'directory': '.'}]}
            source = base / 'inputs.json'
            source.write_text(json.dumps(manifest))
            evaluation = {'version': 1, 'model': 'controlled-ground-evaluation-v1', 'scope': walking.controlled.SCOPE,
                'manifest_sha256': walking.costs.file_hash(source), 'runs': [{'label': 'case', 'seed': 2,
                'source_commit': 'runtime', 'report_sha256': walking.costs.file_hash(report), 'attempts': []}]}
            data = base / 'evaluation.json'
            data.write_text(json.dumps(evaluation))
            walking.load_inputs(data, source)
            manifest['runs'][0]['offset'] = -0.6
            source.write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, 'manifest'):
                walking.load_inputs(data, source)
            evaluation['manifest_sha256'] = walking.costs.file_hash(source)
            data.write_text(json.dumps(evaluation))
            with self.assertRaisesRegex(ValueError, 'placement'):
                walking.load_inputs(data, source)


if __name__ == '__main__':
    unittest.main()
