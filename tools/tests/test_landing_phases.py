"""Phase forecasts preserve causal clocks, retry scope and failed observations."""
import copy
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from test_rolling_trip_estimates import row as base_row, profile as ground_profile
from test_trip_costs import pilot, report, visit

SCRIPT = Path(__file__).resolve().parents[1] / 'estimate-landing-phases.py'
SPEC = importlib.util.spec_from_file_location('landing_phases', SCRIPT)
phases = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(phases)


def row(tick=120, phase='circling', retry=0, **kwargs):
    r = base_row(tick, **kwargs)
    c = r['mission']['capture']
    c.update(goal_since=120, replans=retry)
    c['landing']['goal_since'] = 120
    pilot(r)['landing']['planet'] = 0
    if phase == 'approach':
        c['goal'] = 'approach'
    elif phase in ['alignment', 'descent', 'settling']:
        c['goal'] = 'surface'
        c['landing']['goal'] = 'approach' if phase == 'alignment' else 'land'
        pilot(r)['landing']['supported_feet'] = int(phase == 'settling')
    elif phase == 'landed':
        c['landing']['landed_tick'] = tick
    return r


def sample(attempt=0, duration=6, landing=10, **kwargs):
    return {'label': 'training', 'recording_sha256': 'training_trace', 'seed': attempt, 'seat': 0, 'selected_tick': attempt,
            'phase_seconds': duration, 'landing_seconds_bounds': [landing, landing], **kwargs}


def profile():
    return {'version': 1, 'model': phases.MODEL, 'runtime_revision': 'test',
        'ground_profile_sha256': None, 'min_attempts': phases.MIN_ATTEMPTS, 'runs': [],
        'cells': {f'{context}/{name}': {'samples': [sample(0), sample(1, landing=12)], 'excluded': []}
                  for context in ['initial', 'retry']
                  for name in ['circling', 'approach', 'alignment', 'descent', 'settling']}}


def state(r=None):
    r = r or row()
    s = phases.PhaseTrip(phases.frozen.snapshot(r, 0, 'mission_selection'), ground_profile(), profile())
    return s, s.observe(r)


class PhaseClockTests(unittest.TestCase):
    def test_native_goals_and_contact_have_distinct_phases(self):
        clock = phases.PhaseClock()
        for tick, name in enumerate(['circling', 'approach', 'alignment', 'descent', 'settling'], 120):
            result = clock.observe(row(tick, phase=name))
            self.assertEqual(result['name'], name)
            self.assertEqual(result['age_seconds'], 0)
        self.assertEqual(clock.observe(row(125, phase='landed'))['name'], None)
        self.assertEqual(clock.episodes[-1]['closed_by'], 'landed')

    def test_contact_loss_restarts_descent_episode_without_resetting_capture(self):
        s, initial = state(row(120, phase='descent'))
        s.observe(row(121, phase='settling'))
        result = s.observe(row(122, phase='descent'))
        self.assertEqual(result['flight_phase']['episode'], 2)
        self.assertEqual(result['flight_phase']['age_seconds'], 0)
        self.assertEqual(result['plan_age_seconds'], 2 / 60)
        self.assertEqual(initial['budget']['first_time_limit_tick'], result['budget']['first_time_limit_tick'])

    def test_evidence_refresh_does_not_restart_phase_clock(self):
        s, _ = state(row(120, phase='descent'))
        result = s.observe(row(121, phase='descent', revision=1))
        self.assertEqual(result['invalidated_by'], ['revision'])
        self.assertEqual(result['flight_phase']['age_seconds'], 1 / 60)
        self.assertEqual(result['flight_phase']['context'], 'initial')

    def test_retry_changes_context_even_with_identical_site(self):
        s, initial = state()
        result = s.observe(row(121, retry=1))
        self.assertEqual(result['flight_phase']['context'], 'retry')
        self.assertEqual(result['flight_phase']['age_seconds'], 0)
        self.assertEqual(result['seconds_before_current_plan'], 1 / 60)
        self.assertEqual(result['budget']['first_time_limit_tick'], initial['budget']['first_time_limit_tick'])
        self.assertEqual(s.clock.breaks, [{'tick': 121, 'reason': 'plan_restart'}])

    def test_counter_already_nonzero_on_first_observation_is_retry(self):
        _, result = state(row(retry=2))
        self.assertEqual(result['flight_phase']['context'], 'retry')

    def test_site_change_without_native_retry_still_has_retry_context(self):
        clock = phases.PhaseClock()
        clock.observe(row())
        r = row(121)
        r['mission']['capture']['site']['bearing'] += 1
        self.assertEqual(clock.observe(r)['context'], 'retry')

    def test_gap_does_not_fabricate_phase_entry_or_restart_countdown(self):
        clock = phases.PhaseClock()
        clock.observe(row())
        self.assertIsNone(clock.observe(row(180))['age_seconds'])
        self.assertIsNone(clock.observe(row(181))['age_seconds'])
        self.assertEqual(clock.observe(row(182, phase='approach'))['age_seconds'], 0)
        self.assertEqual(clock.breaks, [{'tick': 180, 'reason': 'observation_gap'}])

    def test_first_midphase_sample_is_not_an_entry(self):
        clock = phases.PhaseClock()
        self.assertIsNone(clock.observe(row(140))['age_seconds'])
        clock = phases.PhaseClock()
        self.assertIsNone(clock.observe(row(120, phase='settling'))['age_seconds'])

    def test_returning_from_unavailable_phase_does_not_invent_an_entry(self):
        clock = phases.PhaseClock()
        clock.observe(row())
        r = row(121)
        pilot(r)['planet']['index'] = 1
        self.assertIsNone(clock.observe(r)['name'])
        self.assertIsNone(clock.observe(row(122))['age_seconds'])
        self.assertEqual(clock.observe(row(123, phase='approach'))['age_seconds'], 0)

    def test_unknown_phase_and_remote_planet_cannot_invent_progress(self):
        for change in ['remote', 'unknown_goal', 'on_foot']:
            r = row()
            if change == 'remote':
                pilot(r)['planet']['index'] = 1
            elif change == 'unknown_goal':
                r['mission']['capture']['goal'] = 'unrecognised'
            else:
                pilot(r)['location'] = 'on_foot'
            self.assertIsNone(phases.phase_name(r))

    def test_future_native_phase_clock_is_rejected(self):
        r = row()
        r['mission']['capture']['goal_since'] = 121
        with self.assertRaises(ValueError):
            phases.PhaseClock().observe(r)

    def test_contact_on_another_body_does_not_count_as_settling_on_target(self):
        r = row(phase='settling')
        pilot(r)['landing']['planet'] = 1
        self.assertIsNone(phases.phase_name(r))


class ConditionalDurationTests(unittest.TestCase):
    def predict(self, samples, age=0, context='initial'):
        p = {'cells': {'initial/descent': {'samples': samples}}}
        return phases.remaining(p, {'name': 'descent', 'context': context, 'age_seconds': age})

    def test_conditioning_uses_phase_exit_not_eventual_landing(self):
        result = self.predict([sample(0, duration=2, landing=20), sample(1), sample(2)], age=2)
        self.assertEqual(result['support'], 2)
        self.assertEqual(result['seconds'], 8)
        self.assertIsNone(self.predict([sample(0), sample(1)], age=6)['seconds'])

    def test_retry_never_borrows_initial_evidence(self):
        result = self.predict([sample(0), sample(1)], context='retry')
        self.assertEqual(result['reason'], 'no_phase_calibration')
        self.assertIsNone(result['seconds'])

    def test_repeated_episodes_do_not_overweight_one_attempt(self):
        result = self.predict([sample(0, landing=100)] * 100 + [sample(1), sample(2)])
        self.assertEqual(result['seconds'], 10)
        self.assertEqual(result['support'], 3)
        self.assertEqual(result['support_episodes'], 102)
        self.assertIsNone(self.predict([sample(0)] * 100)['seconds'])
        self.assertIsNone(self.predict([sample(0), sample(0, label='duplicate_alias')])['seconds'])

    def test_uncertain_landing_interval_is_kept(self):
        result = self.predict([sample(0, landing_seconds_bounds=[10, 11]), sample(1, landing_seconds_bounds=[12, 13])], age=1)
        self.assertEqual(result['envelope_seconds'], [9, 12])
        self.assertEqual(result['seconds'], 10.5)

    def test_inconsistent_duration_and_invalid_age_fail(self):
        with self.assertRaises(ValueError):
            self.predict([sample(duration=20, landing=10)])
        for age in [-1, float('nan')]:
            with self.assertRaises(ValueError):
                self.predict([sample()], age=age)

    def test_baselines_and_earlier_forecasts_are_preserved(self):
        s, result = state()
        baseline = phases.rolling.RollingTrip(s.selection, ground_profile()).observe(row())
        self.assertEqual(result['age_only_total_seconds'], baseline['total_seconds'])
        saved = copy.deepcopy(result)
        original = copy.deepcopy(s.original)
        s.observe(row(121, retry=1, revision=1))
        self.assertEqual(result, saved)
        self.assertEqual(s.original, original)

    def test_unknown_ground_tail_and_unavailable_queries_remain_unknown(self):
        s, _ = state()
        r = row(121)
        pilot(r)['queries_ready'] = False
        result = s.observe(r)
        self.assertIsNone(result['phase_landing_seconds'])
        self.assertIsNone(result['age_only_total_seconds'])
        self.assertIn('local_queries_unavailable', result['unknown_reasons'])
        s, _ = state()
        s.anchor['phases']['outbound']['estimate_seconds'] = None
        result = s.observe(row(121))
        self.assertIsNotNone(result['phase_landing_seconds'])
        self.assertIsNone(result['total_seconds'])
        self.assertIn('unknown_surface_or_departure_cost', result['unknown_reasons'])

    def test_numeric_phase_history_cannot_override_native_capture_deadline(self):
        r = row(9000, phase='approach')
        r['mission']['capture'].update(started_tick=0, goal_since=9000)
        s, before = state(r)
        self.assertIsNotNone(before['phase_landing_seconds'])
        r = copy.deepcopy(r)
        r['tick'] = pilot(r)['tick'] = 9001
        after = s.observe(r)
        self.assertIn('capture_limit_reached', after['unknown_reasons'])
        self.assertIsNone(after['phase_landing_seconds'])
        self.assertIsNone(after['total_seconds'])
        self.assertEqual(after['budget']['first_time_limit_tick'], 9001)


class CalibrationAndReplayTests(unittest.TestCase):
    def test_interrupted_initial_episode_excluded_but_successful_retry_retained(self):
        s, _ = state()
        s.observe(row(121, retry=1))
        s.observe(row(122, phase='descent', retry=1))
        s.observe(row(123, phase='landed', retry=1))
        trip = {'milestones': {'landed': [123, 123]}, 'stopped_tick': 200, 'ending': 'completed'}
        cells = {}
        phases.add_samples(cells, {'label': 'run', 'seed': 0, 'trace_sha256': 'trace'}, s.record(), trip)
        self.assertEqual(len(cells['initial/circling']['samples']), 0)
        self.assertIn('interrupted_before_landing', cells['initial/circling']['excluded'][0]['reasons'])
        self.assertEqual(cells['retry/circling']['samples'][0]['landing_seconds_bounds'], [2 / 60] * 2)
        self.assertEqual(cells['retry/descent']['samples'][0]['phase_seconds'], 1 / 60)

    def test_failure_and_trace_end_are_censored_not_duration_samples(self):
        s, _ = state()
        cells = {}
        trip = {'milestones': {}, 'stopped_tick': 200, 'ending': 'abandoned'}
        phases.add_samples(cells, {'label': 'run', 'seed': 0, 'trace_sha256': 'trace'}, s.record(), trip)
        cell = cells['initial/circling']
        self.assertFalse(cell['samples'])
        self.assertIn('landing_not_completed_in_scope', cell['excluded'][0]['reasons'])
        self.assertIn('phase_not_completed', cell['excluded'][0]['reasons'])

    def test_gap_excludes_prior_phase_even_if_later_landing_completes(self):
        s, _ = state()
        s.observe(row(121, phase='descent'))
        s.observe(row(150, phase='descent'))
        s.observe(row(151, phase='landed'))
        trip = {'milestones': {'landed': [151, 151]}, 'stopped_tick': 200, 'ending': 'completed'}
        cells = {}
        phases.add_samples(cells, {'label': 'run', 'seed': 0, 'trace_sha256': 'trace'}, s.record(), trip)
        self.assertFalse(cells['initial/circling']['samples'])
        self.assertFalse(cells['initial/descent']['samples'])

    def test_replay_emits_contact_changes_and_reselection_retires_state(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            rows = [row(120, phase='descent'), row(121, phase='settling'), row(122, selected=122)]
            (path/'trace.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in rows))
            output = io.StringIO()
            attempts = phases.rolling.replay({'directory': '.', 'seats': [0]}, path, ground_profile(), output,
                trip_factory=lambda selection, ground: phases.PhaseTrip(selection, ground, profile()))
            updates = list(map(json.loads, output.getvalue().splitlines()))
            self.assertEqual(len(attempts), 2)
            self.assertEqual(updates[1]['flight_phase']['name'], 'settling')
            self.assertEqual(updates[2]['status'], 'attempt_left')
            self.assertEqual(updates[3]['selected_tick'], 122)

    def test_summary_pairs_models_and_keeps_unknowns_by_retry_context(self):
        s, initial = state()
        r = report([visit(arrived_tick=60, landed_tick=1500, claimed_tick=1900,
                          boarded_tick=2000, departed_tick=2100)], elapsed=2200)
        trip = phases.costs.build_trips(r, {'seats': [0], 'physics_valid_from_tick': 0})[0].result()
        run = {'label': 'example', 'seed': 5, 'source_commit': 'test',
               'trace_sha256': 'trace', 'report_sha256': 'report', 'trips': [trip]}
        later = {**copy.deepcopy(initial), 'tick': 1020, 'status': 'unknown', 'total_seconds': None,
                 'phase_landing_seconds': None, 'unknown_reasons': ['no_phase_calibration']}
        later['flight_phase']['context'] = 'retry'
        result = phases.evaluate_run(run, [s.record()], [initial, later])
        summary = phases.summarize([result])
        self.assertEqual(summary['0']['paired_completed'], 1)
        self.assertEqual(summary['15']['paired_completed'], 0)
        self.assertEqual(summary['15']['by_context']['retry']['statuses']['unknown'], 1)
        self.assertEqual(summary['30']['statuses']['already_landed'], 1)

    def test_cli_forecasts_precede_outcome_read_and_both_training_profiles_are_checked(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            (path/'trace.jsonl').write_text(json.dumps(row())+'\n')
            (path/'ground.json').write_text(json.dumps(ground_profile()))
            p = profile()
            p['ground_profile_sha256'] = phases.costs.file_hash(path/'ground.json')
            p['runs'] = [{'seed': 42, 'trace_sha256': 'different', 'report_sha256': 'different'}]
            (path/'phase.json').write_text(json.dumps(p))
            manifest = {'version': 1, 'runtime_revision': 'test', 'runs': [
                {'label': 'test', 'directory': '.', 'seats': [0], 'source_commit': 'test', 'physics_valid_from_tick': 0}]}
            (path/'manifest.json').write_text(json.dumps(manifest))
            cmd = [sys.executable, str(SCRIPT), 'evaluate', '--manifest', str(path/'manifest.json'),
                '--ground-profile', str(path/'ground.json'), '--phase-profile', str(path/'phase.json'), '--out', str(path/'out')]
            result = subprocess.run(cmd, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)  # No report exists yet.
            self.assertTrue((path/'out/predictions.json').exists())
            self.assertFalse((path/'out/evaluation.json').exists())
            (path/'report.json').write_text(json.dumps(report([visit()], elapsed=200)))
            result = subprocess.run(cmd, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('disjoint world seeds', result.stderr)


if __name__ == '__main__':
    unittest.main()
