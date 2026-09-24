"""Locally supported flight costs retain independent evidence and causal guards."""
import copy
import importlib.util
from pathlib import Path
import unittest

from test_flight_progress import row, flight
from test_composed_trip_estimates import profiles
from test_landing_phases import row as phase_row
from test_trip_costs import pilot

SPEC = importlib.util.spec_from_file_location('approach_state',
    Path(__file__).resolve().parents[1] / 'estimate-approach-state.py')
model = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(model)


def record(progress=False):
    state = flight.FlightProgress()
    for tick in range(120, 181 if progress else 121):
        result = state.observe(row(tick, length=25))
    return result


def sample(seed=0, selected=0, approach=5, tail=3, progress=False, **changes):
    _, values, _ = model.state_vector(record(progress))
    return {'recording_sha256': str(seed), 'seed': seed, 'seat': 0, 'selected_tick': selected,
        'tick': 120, 'state': values, 'remaining_approach_seconds': approach,
        'post_approach_seconds_bounds': [tail, tail], **changes}


def lookup(samples=None, context='initial', progress=False):
    samples = samples or [sample(i, progress=progress) for i in range(3)]
    return model.StateLookup(model.make_profile('test',
        {context + ('/progress' if progress else '/entry'): samples}, []))


class StateLookupTests(unittest.TestCase):
    def test_approach_and_later_flight_are_counted_once(self):
        result = lookup().predict(record())
        self.assertEqual(result['approach_seconds'], 5)
        self.assertEqual(result['post_approach_flight_seconds'], 3)
        self.assertEqual(result['seconds'], 8)
        self.assertEqual(result['envelope_seconds'], [8, 8])
        self.assertEqual((result['support'], result['support_seeds']), (3, 3))

    def test_worlds_are_balanced_after_each_attempt_contributes_once(self):
        samples = [sample(0, selected=i, approach=100) for i in range(20)]
        samples += [sample(1, approach=5), sample(2, approach=7)]
        result = lookup(samples).predict(record())
        self.assertEqual(result['approach_seconds'], 7)
        repeated = lookup(samples + [copy.deepcopy(samples[0])] * 100).predict(record())
        self.assertEqual(repeated['seconds'], result['seconds'])
        self.assertEqual(repeated['support'], result['support'])

    def test_dense_samples_from_one_attempt_or_world_are_not_independent(self):
        for samples in [[sample(0)] * 20, [sample(0, selected=i) for i in range(20)],
                        [sample(0), sample(1)]]:
            result = lookup(samples).predict(record())
            self.assertIsNone(result['seconds'])
            self.assertEqual(result['reason'], 'insufficient_local_approach_support')

    def test_nearest_state_per_attempt_not_its_future_duration_selects_neighbor(self):
        samples = [sample(i) for i in range(3)]
        far = sample(0, approach=100)
        far['state']['height'] += 1
        samples.append(far)
        self.assertEqual(lookup(samples).predict(record())['seconds'], 8)

    def test_old_age_does_not_expire_physically_supported_progress(self):
        r = record(True)
        r['phase']['age_seconds'] = 100
        self.assertEqual(lookup(progress=True).predict(r)['seconds'], 8)
        r['phase']['age_seconds'] = None
        self.assertEqual(lookup(progress=True).predict(r)['reason'], 'phase_entry_unobserved')

    def test_entry_does_not_borrow_progress_or_retry_evidence(self):
        self.assertEqual(lookup(progress=True).predict(record())['reason'], 'no_approach_state_calibration')
        self.assertEqual(lookup(context='retry').predict(record())['reason'], 'no_approach_state_calibration')

    def test_unsupported_distance_velocity_gravity_and_progress_are_unknown(self):
        for feature in model.SCALES:
            r = record()
            r['geometry'][feature] += 100
            with self.subTest(feature=feature):
                self.assertEqual(lookup().predict(r)['reason'], 'approach_state_outside_training_domain')
        r = record(True)
        r['one_second_progress']['distance_closed'] = 100
        self.assertIsNone(lookup(progress=True).predict(r)['seconds'])

    def test_global_range_does_not_replace_local_support(self):
        samples = [sample(i) for i in range(6)]
        for i, s in enumerate(samples): s['state']['height'] += -30 if i < 3 else 30
        result = lookup(samples).predict(record())
        self.assertIsNone(result['seconds'])
        self.assertEqual(result['support'], 0)

    def test_missing_geometry_gravity_or_partial_history_cannot_make_an_eta(self):
        for change in ['geometry', 'gravity', 'progress']:
            r = record(True)
            if change == 'geometry': r['geometry'] = None
            if change == 'gravity': r['geometry']['gravity_normal'] = None
            if change == 'progress': r['one_second_progress'] = None
            with self.subTest(change=change):
                self.assertIsNone(lookup(progress=True).predict(r)['seconds'])

    def test_widened_domain_and_changed_rule_are_rejected(self):
        p = copy.deepcopy(lookup().profile)
        p['cells']['initial/entry']['domain']['height'][1] += 100
        with self.assertRaises(ValueError): model.StateLookup(p)
        p = copy.deepcopy(lookup().profile)
        p['rule']['minimum_worlds'] = 1
        with self.assertRaises(ValueError): model.StateLookup(p)

    def test_invalid_age_and_incomplete_profile_remain_invalid(self):
        for age in [-1, float('nan'), True]:
            r = record()
            r['phase']['age_seconds'] = age
            self.assertEqual(lookup().predict(r)['reason'], 'phase_entry_unobserved')
        p = copy.deepcopy(lookup().profile)
        del p['cells']['initial/entry']['domain']['right_speed']
        with self.assertRaises(ValueError): model.StateLookup(p)


class StateCompositionTests(unittest.TestCase):
    def state(self, r=None, fitted=None):
        r = r or row(length=25)
        state = model.StateTrip(model.frozen.snapshot(r, 0, 'mission_selection'),
                                *profiles(), fitted or lookup())
        return state, state.observe(r)

    def test_keeps_duration_comparator_original_ground_and_elapsed(self):
        r = row(length=25)
        baseline = model.composed.ComposedTrip(model.frozen.snapshot(r, 0, 'mission_selection'),
            *profiles(), walk_model='short-and-affine').observe(r)
        state, new = self.state(r)
        self.assertEqual(new['duration_total_seconds'], baseline['total_seconds'])
        self.assertEqual(new['total_seconds'], baseline['total_seconds'] + 8 - baseline['landing']['seconds'])
        for k in ['phase_only_total_seconds', 'walking_legs', 'elapsed_seconds', 'original_total_seconds', 'flight_phase']:
            self.assertEqual(new[k], baseline[k])
        self.assertEqual(state.first_composed, baseline)
        self.assertFalse(new['physical_permissions'])

    def test_other_phases_keep_exact_duration_estimates(self):
        for name in ['circling', 'alignment', 'descent']:
            r = row(length=25)
            r['mission']['capture'] = phase_row(phase=name, length=25)['mission']['capture']
            _, result = self.state(r)
            self.assertEqual(result['total_seconds'], result['duration_total_seconds'])
            self.assertEqual(result['landing'], result['duration_landing'])

    def test_first_forecasts_and_capture_deadline_survive_retry(self):
        state, before = self.state()
        original = copy.deepcopy(state.record())
        result = state.observe(row(121, length=25, retry=1))
        self.assertIsNone(result['total_seconds'])
        self.assertEqual(result['landing']['reason'], 'no_approach_state_calibration')
        self.assertEqual(result['budget']['first_time_limit_tick'], before['budget']['first_time_limit_tick'])
        self.assertEqual(state.first_state, original['first_state_prediction'])
        self.assertEqual(state.first_composed, original['first_composed_prediction'])
        self.assertEqual(state.original, original['original_prediction'])

    def test_capture_restart_within_same_mission_keeps_retry_context(self):
        state, before = self.state()
        r = row(121, length=25)
        r['mission']['capture'].update(started_tick=121, goal_since=121)
        result = state.observe(r)
        self.assertEqual(result['flight_phase']['context'], 'retry')
        self.assertEqual(result['approach_state']['phase'], result['flight_phase'])
        self.assertEqual(result['landing']['reason'], 'no_approach_state_calibration')
        self.assertIsNone(result['total_seconds'])
        self.assertEqual(result['elapsed_seconds'], before['elapsed_seconds'] + 1 / 60)

    def test_supported_progress_can_restore_expired_duration_forecast(self):
        state, _ = self.state(fitted=lookup(progress=True))
        for tick in range(121, 781): result = state.observe(row(tick, length=25))
        self.assertIsNone(result['duration_total_seconds'])
        self.assertIsNotNone(result['total_seconds'])
        self.assertEqual(result['flight_phase']['age_seconds'], 11)
        self.assertEqual(result['landing']['seconds'], 8)

    def test_stale_ground_and_spent_budget_cannot_be_repaired_by_flight(self):
        state, _ = self.state()
        r = row(121, length=25)
        r['observation']['local']['objective_work'] = 'stale'
        self.assertIsNone(state.observe(r)['total_seconds'])
        r = row(9061, length=25)
        r['mission']['capture']['goal_since'] = 9061
        result = state.observe(r)
        self.assertIn('capture_limit_reached', result['unknown_reasons'])
        self.assertIsNone(result['total_seconds'])

    def test_unsupported_approach_never_falls_back_to_numeric_baseline(self):
        r = row(length=25)
        pilot(r)['sites'] = []
        _, result = self.state(r)
        self.assertIsNotNone(result['duration_total_seconds'])
        self.assertIsNone(result['total_seconds'])
        self.assertIn('selected_site_unavailable_or_ambiguous', result['unknown_reasons'])

    def test_terminal_observation_stops_state_replay(self):
        state, _ = self.state()
        r = row(121, length=25)
        r['mission']['capture']['landing']['landed_tick'] = 121
        self.assertEqual(state.observe(r)['status'], 'landed')
        self.assertIsNone(state.observe(row(122, length=25)))


class StateCalibrationTests(unittest.TestCase):
    def run_data(self):
        feature = record()
        episode = {**feature['phase'], 'last_tick': 179, 'end_tick': 180, 'closed_by': 'phase_change'}
        actual = {'milestones': {'landed': [240, 240]}, 'stopped_tick': 300, 'ending': 'completed'}
        attempt = {'selection': {'seat': 0, 'planet': 0, 'selected_tick': 0},
                   'phase_episodes': [episode], 'phase_breaks': [], 'actual': actual}
        return {'label': 'training', 'seed': 1, 'trace_sha256': 'trace', 'attempts': [attempt]}, feature

    def test_later_ground_failure_does_not_censor_completed_flight(self):
        run, feature = self.run_data()
        run['attempts'][0]['actual']['ending'] = 'abandoned'
        cells = {}
        model.add_training_run(cells, run, [feature])
        s = cells['initial/entry'][0]
        self.assertEqual(s['remaining_approach_seconds'], 1)
        self.assertEqual(s['post_approach_seconds_bounds'], [1, 1])

    def test_interrupted_censored_unobserved_or_unfinished_phase_is_excluded(self):
        for change in ['break', 'no_landing', 'unobserved', 'unfinished']:
            run, feature = self.run_data()
            a = run['attempts'][0]
            if change == 'break': a['phase_breaks'] = [{'tick': 200}]
            if change == 'no_landing': a['actual']['milestones'] = {}
            if change == 'unobserved': a['phase_episodes'][0]['entry_observed'] = False
            if change == 'unfinished': a['phase_episodes'][0]['closed_by'] = 'plan_restart'
            cells = {}
            with self.subTest(change=change):
                self.assertTrue(model.add_training_run(cells, run, [feature]))
                self.assertEqual(cells, {})


if __name__ == '__main__':
    unittest.main()
