"""Endpoint probabilities retain failed/censored evidence and guarded coverage."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from test_landing_risk import dense, trace

SPEC = importlib.util.spec_from_file_location('landing_probability',
    Path(__file__).resolve().parents[1] / 'estimate-landing-risk.py')
model = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(model)


def row(i=0, status='landed', recent='no', phase='descent', world=None, checkpoint=15):
    f = {'status': 'eligible', 'phase': {'name': phase}, 'site': {'planet': 0, 'bearing': 1},
         'contact': {'planet': 0, 'phase': 'assisted'}, 'physical_permissions': False,
         'recent': {'complete': recent != 'unknown', 'window_seconds': 5,
                    'counts': None if recent == 'unknown' else {'prelanding_replan': int(recent == 'yes')}}}
    return {'dataset': 'training', 'label': 'run', 'scope': 'normal', 'asteroid_interval_seconds': 0,
        'seed': i % 4 if world is None else world, 'seat': 0, 'planet': 0, 'selected_tick': i,
        'checkpoint_seconds': checkpoint, 'tick': 1000 + i, 'features': f,
        'actual_ending': 'abandoned', 'actual_reason': None,
        'outcome': {'status': status, 'interrupted': None if status == 'attempt_ended' or status.startswith('censored_')
                     else status == 'interrupted'}}


def profile(rows):
    return {'version': 1, 'model': model.MODEL, 'rule': copy.deepcopy(model.RULE), 'cells': model.fit(rows),
            'runtime_revision': 'frozen', 'training_worlds': [0, 1, 2, 3], 'training_traces': ['training-trace']}


def scored(record, phase, recent):
    return {**record, 'predictions': {v: {'status': 'numeric', 'used': v, 'probabilities': p}
            for v, p in [('phase', phase), ('phase_recent', recent)]}}


class ProbabilityFitTests(unittest.TestCase):
    def test_worlds_have_equal_weight_and_uniform_prior_is_explicit(self):
        rows = [row(i, status='interrupted', world=0) for i in range(9)]
        rows += [row(i, world=i - 8) for i in range(9, 12)]
        c = model.fit_cell(rows)
        self.assertEqual(c['status'], 'supported')
        self.assertEqual(c['probabilities'], {'interrupted': .25, 'landed': .65,
                                            'horizon_clear': .05, 'attempt_ended': .05})
        self.assertEqual(c['empirical_censoring_bounds']['interrupted'], [.25, .25])

    def test_attempt_end_is_its_own_endpoint_and_censoring_is_not_a_negative(self):
        rows = [row(i, status='attempt_ended') for i in range(12)]
        c = model.fit_cell(rows)
        self.assertAlmostEqual(c['probabilities']['attempt_ended'], .85)
        self.assertEqual(c['classified'], 12)
        rows += [row(12, status='censored_recording_end')]
        c = model.fit_cell(rows)
        self.assertEqual(c['classified'], 12)
        self.assertEqual(c['censored'], 1)
        self.assertAlmostEqual(c['probabilities']['attempt_ended'], .85)
        self.assertEqual(c['outcomes']['censored_recording_end'], 1)

    def test_minimum_attempts_worlds_and_censoring_guard_support(self):
        for rows, reason in [([row(i) for i in range(11)], 'too_few_classified_attempts'),
            ([row(i, world=0) for i in range(12)], 'too_few_classified_worlds'),
            ([row(i) for i in range(12)] + [row(i, status='censored_recording_end') for i in (12, 13)], 'too_much_censoring')]:
            with self.subTest(reason=reason):
                c = model.fit_cell(rows)
                self.assertEqual(c['reason'], reason)
                self.assertIsNone(c['probabilities'])

    def test_duplicate_attempt_at_same_checkpoint_cannot_inflate_support(self):
        rows = [row(i) for i in range(12)]
        duplicate = copy.deepcopy(rows[0]); duplicate['tick'] += 60
        with self.assertRaises(ValueError): model.fit(rows + [duplicate])

    def test_checkpoint_scope_and_phase_cells_stay_separate(self):
        rows = [row(i) for i in range(12)]
        rows += [row(i, checkpoint=0, phase='approach') for i in range(12)]
        controlled = [row(i, phase='circling') for i in range(12)]
        for r in controlled: r.update(dataset='controlled', scope='controlled')
        cells = model.fit(rows + controlled)
        self.assertIn('normal/15/descent', cells)
        self.assertIn('normal/0/approach', cells)
        self.assertIn('controlled/15/circling', cells)
        self.assertNotIn('normal/15/approach', cells)


class ProbabilityForecastTests(unittest.TestCase):
    def test_supported_refinement_and_guarded_fallback_preserve_parent(self):
        rows = [row(i, status='interrupted', recent='yes') for i in range(12)]
        rows += [row(i) for i in range(12, 24)]
        p = profile(rows)
        a, b = (model.predict(p, row(recent='yes'), v) for v in model.VARIANTS)
        self.assertGreater(b['probabilities']['interrupted'], a['probabilities']['interrupted'])
        self.assertEqual(b['used'], 'phase_recent')
        missing = model.predict(p, row(recent='unknown'), 'phase_recent')
        self.assertEqual(missing['probabilities'], a['probabilities'])
        self.assertEqual(missing['support'], a['support'])
        self.assertEqual(missing['reason'], 'recent_history_unknown_keep_phase')

    def test_sparse_child_uses_exact_parent_and_missing_parent_stays_unknown(self):
        p = profile([row(i) for i in range(12)] + [row(12, recent='yes')])
        a, b = (model.predict(p, row(recent='yes'), v) for v in model.VARIANTS)
        self.assertEqual(b['probabilities'], a['probabilities'])
        self.assertEqual(b['reason'], 'recent_support_unavailable_keep_phase')
        self.assertEqual(model.predict(p, row(phase='settling'), 'phase_recent')['status'], 'unknown')

    def test_future_labels_and_returned_mutations_do_not_change_forecast(self):
        p = profile([row(i) for i in range(12)])
        r = row(); before = model.predict(p, r, 'phase_recent')
        r.update(outcome={'status': 'interrupted'}, actual_ending='ship_lost', actual_reason='future')
        self.assertEqual(before, model.predict(p, r, 'phase_recent'))
        before['probabilities']['landed'] = 0
        self.assertGreater(model.predict(p, r, 'phase_recent')['probabilities']['landed'], 0)
        self.assertFalse(model.predict(p, r, 'phase_recent')['physical_permissions'])

    def test_invalid_current_contact_plan_and_history_do_not_get_a_fallback(self):
        p = profile([row(i) for i in range(12)])
        for kind in ('frame', 'landed', 'site', 'permission', 'counter', 'clock'):
            r = row()
            if kind == 'frame': r['features']['contact']['planet'] = 1
            if kind == 'landed': r['features']['contact']['phase'] = 'landed'
            if kind == 'site': r['features']['site'] = None
            if kind == 'permission': r['features']['physical_permissions'] = True
            if kind == 'counter': r['features']['recent']['counts']['prelanding_replan'] = True
            if kind == 'clock': r['tick'] = -1
            with self.subTest(kind=kind):
                self.assertEqual(model.predict(p, r, 'phase_recent')['status'], 'unknown')

    def test_later_checkpoints_and_no_choice_are_not_silently_extrapolated(self):
        p = profile([row(i) for i in range(12)])
        self.assertEqual(model.predict(p, row(checkpoint=30), 'phase')['reason'], 'scope_or_checkpoint_not_calibrated')
        r = row(); r['features'] = {'status': 'no_observed_choice'}
        self.assertEqual(model.predict(p, r, 'phase')['reason'], 'no_observed_choice')

    def test_world_recording_and_runtime_overlap_are_rejected(self):
        p = profile([row(i) for i in range(12)])
        for bad in ('world', 'trace', 'runtime', 'rule'):
            source = {'runtime_revision': 'frozen', 'worlds': [99], 'traces': ['new-trace']}
            candidate = copy.deepcopy(p)
            if bad == 'world': source['worlds'] = [0]
            if bad == 'trace': source['traces'] = ['training-trace']
            if bad == 'runtime': source['runtime_revision'] = 'other'
            if bad == 'rule': candidate['rule']['horizon_seconds'] = 20
            with self.subTest(bad=bad), self.assertRaises(ValueError): model.validate_holdout(candidate, source)


class ProbabilityEvaluationTests(unittest.TestCase):
    def test_proper_scores_distinguish_ending_from_landing_and_warning_all_the_time(self):
        good = {'interrupted': .1, 'landed': .7, 'horizon_clear': .1, 'attempt_ended': .1}
        warn = {'interrupted': .7, 'landed': .1, 'horizon_clear': .1, 'attempt_ended': .1}
        self.assertLess(model.score(good, 'landed')['brier'], model.score(warn, 'landed')['brier'])
        self.assertGreater(model.score(good, 'attempt_ended')['brier'], model.score(good, 'landed')['brier'])
        self.assertAlmostEqual(model.score(good, 'landed')['brier'], .12)

    def test_censored_score_bounds_keep_unresolved_rows_visible(self):
        p = {'interrupted': .1, 'landed': .7, 'horizon_clear': .1, 'attempt_ended': .1}
        rows = [scored(row(), p, p), scored(row(1, status='censored_recording_end'), p, p)]
        m = model.metrics(rows, 'phase')
        self.assertEqual((m['numeric'], m['classified'], m['censored']), (2, 1, 1))
        self.assertAlmostEqual(m['all_numeric_brier_bounds'][0], .12)
        self.assertGreater(m['all_numeric_brier_bounds'][1], .12)

    def test_auc_handles_ties_and_missing_outcome_classes(self):
        self.assertEqual(model.auc([(.1, True), (.1, False)]), .5)
        self.assertEqual(model.auc([(.9, True), (.1, False)]), 1)
        self.assertIsNone(model.auc([(.9, True)]))

    def test_paired_world_differences_do_not_weight_busy_worlds_more(self):
        a = {'interrupted': .1, 'landed': .7, 'horizon_clear': .1, 'attempt_ended': .1}
        b = {'interrupted': .7, 'landed': .1, 'horizon_clear': .1, 'attempt_ended': .1}
        rows = [scored(row(i, world=0), a, b) for i in range(9)]
        rows += [scored(row(9, world=1, status='interrupted'), a, b)]
        c = model.comparison(rows)
        self.assertAlmostEqual(c['paired_equal_world_score_difference']['brier'], 0)
        self.assertGreater(c['models']['phase_recent']['mean_scores']['brier'], c['models']['phase']['mean_scores']['brier'])

    def test_outcome_binding_rejects_changed_features(self):
        causal = {k: v for k, v in row().items() if k not in model.OUTCOME_FIELDS}
        f = {'records': [causal]}
        with tempfile.TemporaryDirectory() as folder:
            p = Path(folder) / 'outcomes.json'
            d = {'model': model.risk.MODEL, 'features_sha256': 'hash', 'records': [row()]}
            p.write_text(json.dumps(d)); model.bound_outcomes(p, f, 'hash')
            d['records'][0]['features']['phase']['name'] = 'approach'
            p.write_text(json.dumps(d))
            with self.assertRaises(ValueError): model.bound_outcomes(p, f, 'hash')

    def test_feature_only_export_does_not_call_the_outcome_labeller(self):
        t = trace(dense(960)); t.update(first_choice_tick=0, selection={'seat': 0, 'planet': 0, 'selected_tick': 0})
        run = {'dataset': 'fixture', 'scope': 'normal', 'label': 'one', 'seed': 99, 'asteroid_interval_seconds': 0,
            'attempts': [t], 'source_attempts': [{'actual': {'ending': 'abandoned', 'stopped_tick': 961}}],
            'trial_outcome': None, 'unobserved_attempts': [], 'provenance': {}}
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); manifest = root / 'input.json'
            manifest.write_text(json.dumps({'version': 1, 'runtime_revision': 'frozen', 'datasets': [{}]}))
            with mock.patch.object(model.risk, 'load_runs', return_value=iter([run])), \
                 mock.patch.object(model.risk, 'label', side_effect=AssertionError('future labels read')):
                model.risk.inspect(manifest, root / 'features', features_only=True)
            self.assertTrue((root / 'features/features.json').exists())
            self.assertTrue((root / 'features/provenance.json').exists())
            self.assertFalse((root / 'features/outcomes.json').exists())


if __name__ == '__main__':
    unittest.main()
