"""Distance cohorts and execution diagnostics cannot rewrite frozen forecasts."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import test_trip_estimates as fixtures
from test_trip_estimates import row, profile, read_rows
from test_trip_costs import pilot, leg

SPEC = importlib.util.spec_from_file_location('ground_validation',
    Path(__file__).resolve().parents[1] / 'validate-ground-costs.py')
ground = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ground)


def attempt():
    choice, trip = fixtures.CalibrationContracts().fixture()
    prediction = ground.frozen.estimate(choice, profile())
    return {'choice': choice, 'prediction': prediction, 'selection': {'seat': 0, 'planet': 0, 'selected_tick': 0},
            'recorded_attempt': trip, 'post_choice_changes': [],
            'actual': ground.frozen.compare(trip, choice, prediction)}


class CohortTests(unittest.TestCase):
    def test_long_threshold_applies_to_either_recorded_leg_in_seconds(self):
        c = attempt()['choice']
        c['references'].update(outbound=3.99, return_board=4.0)
        result = ground.classify(c, 4)
        self.assertEqual(result['group'], 'long/walk')
        self.assertEqual(result['long_legs'], ['return_board'])
        c['references']['return_board'] = 3.99
        self.assertEqual(ground.classify(c, 4)['group'], 'short/walk')

    def test_no_choice_no_flag_and_unmeasured_routes_are_distinct(self):
        self.assertEqual(ground.classify(None, 4)['group'], 'no_choice')
        c = ground.frozen.snapshot(row(no_flag=True), 0, 'local_choice')
        self.assertEqual(ground.classify(c, 4)['group'], 'no_flag')
        c['category'] = None
        self.assertEqual(ground.classify(c, 4)['group'], 'unmeasured_route')

    def test_jumping_and_crossing_do_not_become_walk_samples(self):
        c = attempt()['choice']
        for name in ['jump', 'crossing']:
            c['category'] = name
            self.assertEqual(ground.classify(c, 4)['group'], 'long/' + name)

    def test_later_route_changes_cannot_reclassify_first_choice(self):
        first, later = row(length=10), row(121, length=150)
        before, after = read_rows([first])[0], read_rows([first, later])[0]
        self.assertEqual(ground.classify(before['choice'], 4), ground.classify(after['choice'], 4))
        self.assertEqual(ground.classify(after['choice'], 4)['group'], 'short/walk')

    def test_missing_evidence_does_not_erase_a_long_route_from_the_cohort(self):
        a = attempt()
        a['choice']['missing'] = ['route_source_unverified_at_choice']
        result = ground.classify(a['choice'], 4)
        self.assertEqual(result['group'], 'long/walk')
        self.assertEqual(result['choice_evidence_missing'], ['route_source_unverified_at_choice'])

    def test_invalid_threshold_rejected(self):
        for value in [0, -1, float('nan'), float('inf'), True]:
            with self.assertRaises(ValueError):
                ground.classify(None, value)


class PhaseComparisonTests(unittest.TestCase):
    def test_ground_omits_landing_and_tail_starts_at_landing(self):
        a = attempt()
        r = ground.assess(a, 4)
        p = a['prediction']['phases']
        self.assertEqual(r['phases']['ground']['estimate_seconds'],
                         sum(p[n]['estimate_seconds'] for n in ['outbound', 'claim', 'return_board']))
        self.assertEqual(r['phases']['landed_to_departed']['actual']['seconds_bounds'], [29, 29])
        self.assertEqual(r['phases']['landed_to_departed']['estimate_seconds'],
                         sum(p[n]['estimate_seconds'] for n in ground.PHASES))

    def test_unknown_leg_is_not_replaced_by_zero_in_combined_cost(self):
        a = attempt()
        p = a['prediction']['phases']['outbound']
        p.update(estimate_seconds=None, historical_envelope_seconds=None,
                 unknown_reasons=['reference_outside_calibration_domain'])
        r = ground.assess(a, 4)
        self.assertIsNone(r['phases']['ground']['estimate_seconds'])
        self.assertIn('outbound:reference_outside_calibration_domain', r['phases']['ground']['unknown_reasons'])
        self.assertIsNotNone(r['phases']['return_board']['estimate_seconds'])

    def test_evidence_changes_are_cut_at_each_phase_end(self):
        a = attempt()
        a['post_choice_changes'] = [{'tick': 1000, 'dependency': 'revision'}]
        r = ground.assess(a, 4)
        self.assertFalse(r['phases']['outbound']['evidence_changes'])
        self.assertTrue(r['phases']['claim']['evidence_changes'])
        self.assertTrue(r['phases']['return_board']['evidence_changes'])

    def test_censored_ground_does_not_produce_a_completed_error(self):
        a = attempt()
        a['recorded_attempt']['ending'] = 'abandoned'
        a['recorded_attempt']['phases']['ground'].update(status='censored', seconds_bounds=[100, 100])
        r = ground.assess(a, 4)
        self.assertIsNone(r['phases']['ground']['error_seconds_bounds'])
        self.assertTrue(r['phases']['ground']['censored_elapsed_exceeds_historical_max'])
        s = ground.summarize([{'attempts': [r]}])['long/walk']
        self.assertEqual(s['endings'], {'abandoned': 1})
        self.assertEqual(s['phases']['ground']['completed_error']['completed_exact_comparisons'], 0)

    def test_missing_choice_preserves_actual_completed_phases(self):
        a = attempt()
        a.update(choice=None, prediction=None, actual=None)
        r = ground.assess(a, 4)
        self.assertEqual(r['classification']['group'], 'no_choice')
        self.assertEqual(r['phases']['ground']['actual']['status'], 'completed')
        self.assertIsNone(r['phases']['ground']['error_seconds_bounds'])

    def test_invalid_prediction_scope_or_physics_cannot_count_as_accuracy(self):
        for invalid in ['scope', 'physics']:
            a = attempt()
            if invalid == 'scope':
                a['actual']['prediction_scope_valid'] = False
            else:
                a['recorded_attempt']['phases']['outbound']['physics_valid'] = False
            self.assertIsNone(ground.assess(a, 4)['phases']['outbound']['error_seconds_bounds'])

    def test_sparse_duration_bounds_are_not_collapsed_to_exact_measurements(self):
        a = attempt()
        a['recorded_attempt']['phases']['outbound']['seconds_bounds'] = [10, 11]
        r = ground.assess(a, 4)
        p = r['phases']['outbound']
        self.assertEqual(p['error_seconds_bounds'], [2, 3])
        self.assertEqual(ground.phase_summary([p])['completed_error']['completed_exact_comparisons'], 0)

    def test_assessment_does_not_change_source_predictions(self):
        a = attempt()
        before = copy.deepcopy(a)
        ground.assess(a, 4)
        self.assertEqual(a, before)


class ExecutionTests(unittest.TestCase):
    def window(self):
        return ground.ExecutionWindow(ground.assess(attempt(), 4), 'outbound', 300, 400)

    def observation(self, tick=301):
        r = row(tick, foot=True)
        pilot(r).update(balanced=True, supported_planet=0)
        r['controls'] = {'turn': 1.0, 'thrust': False, 'brake': False}
        r['mission']['capture']['ground'] = {'goal': 'jump', 'destination': 'flag', 'route': leg(50),
            'revision': 0, 'continuous_walk': True, 'replans': 1, 'invalidations': 0}
        return r

    def test_goal_label_is_not_a_jump_command_and_route_is_posthoc(self):
        w = self.window()
        w.observe(self.observation())
        self.assertEqual(w.stats['goals'], {'jump': 1})
        self.assertEqual(w.stats['primary_held_ticks'], 0)
        self.assertEqual(w.stats['first_route']['nominal_seconds'], 10)
        self.assertEqual(w.stats['input_ticks']['full'], 1)

    def test_first_route_cannot_be_replaced_by_a_later_shorter_one(self):
        w = self.window()
        w.observe(self.observation())
        r = self.observation(302)
        r['mission']['capture']['ground']['route']['length'] = 1
        w.observe(r)
        self.assertEqual(w.stats['first_route']['route']['length'], 50)

    def test_wrong_target_and_aboard_ticks_do_not_count_as_walking(self):
        w = self.window()
        r = self.observation()
        r['mission']['target'] = 1
        w.observe(r)
        r = self.observation(302)
        pilot(r)['location'] = {'aboard': 0}
        w.observe(r)
        self.assertEqual(w.stats['sampled_ticks'], 2)
        self.assertEqual(w.stats['matching_on_foot_ticks'], 0)
        self.assertIsNone(w.stats['first_route'])

    def test_trace_mismatch_is_rejected_and_gaps_remain_visible(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            raw = (json.dumps(self.observation()) + '\n').encode()
            (base/'trace.jsonl').write_bytes(raw)
            run = {'attempts': [ground.assess(attempt(), 4)]}
            with self.assertRaisesRegex(ValueError, 'differs'):
                ground.add_execution({'directory': '.'}, base, run, 'wrong_hash')
            run = {'attempts': [ground.assess(attempt(), 4)]}
            ground.add_execution({'directory': '.'}, base, run, hashlib.sha256(raw).hexdigest())
            out = run['attempts'][0]['execution']['outbound']
            self.assertEqual(out['sampled_ticks'], 1)
            self.assertEqual(out['missing_ticks'], 599)
            self.assertFalse(out['dense'])


class CliTests(unittest.TestCase):
    def evaluation(self):
        a = attempt()
        unobserved = copy.deepcopy(a['recorded_attempt'])
        unobserved['selected_tick'] = 3000
        return {'version': 1, 'model': ground.frozen.MODEL, 'profile_sha256': 'frozen-profile',
            'manifest_sha256': 'original-manifest', 'runs': [{'label': 'example', 'seed': 1,
                'source_commit': 'runtime', 'report_sha256': 'report', 'trace_sha256': 'trace',
                'attempts': [a], 'attempts_without_observation': [unobserved]}]}

    def test_cli_preserves_unobserved_attempts_and_binds_its_source(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            source, out = base/'evaluation.json', base/'out'
            evaluation = self.evaluation()
            ground.frozen.write_json(source, evaluation)
            subprocess.run([sys.executable, ground.__file__, '--evaluation', str(source), '--out', str(out)],
                           check=True, capture_output=True)
            result = json.loads((out/'ground-validation.json').read_text())
            self.assertEqual(result['source_evaluation_sha256'], ground.costs.file_hash(source))
            self.assertEqual(result['profile_sha256'], 'frozen-profile')
            self.assertEqual(result['summary']['all']['attempts'], 1)
            self.assertEqual(result['unobserved_attempt_count'], 1)
            self.assertEqual(result['runs'][0]['unobserved_attempts'],
                             evaluation['runs'][0]['attempts_without_observation'])

    def test_cli_rejects_a_different_execution_manifest_before_reading_traces(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            source, manifest, out = base/'evaluation.json', base/'inputs.json', base/'out'
            ground.frozen.write_json(source, self.evaluation())
            ground.frozen.write_json(manifest, {'version': 1, 'runtime_revision': 'runtime', 'runs': [
                {'label': 'example', 'directory': 'unavailable', 'seats': [0],
                 'source_commit': 'runtime', 'physics_valid_from_tick': 0}]})
            result = subprocess.run([sys.executable, ground.__file__, '--evaluation', str(source),
                '--manifest', str(manifest), '--out', str(out)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('manifest differs from frozen evaluation input', result.stderr)
            self.assertFalse(out.exists())


if __name__ == '__main__':
    unittest.main()
