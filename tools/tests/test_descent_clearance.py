"""Current descent evidence cannot replace ground permissions or future outcomes."""
import copy
import importlib.util
from pathlib import Path
import unittest

from test_flight_progress import row as flight_row, pilot
from test_composed_trip_estimates import profiles
from test_approach_state import lookup as approach_lookup

SPEC = importlib.util.spec_from_file_location('descent_clearance',
    Path(__file__).resolve().parents[1] / 'estimate-descent-clearance.py')
model = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(model)


def row(tick=120, **kwargs):
    r = flight_row(tick, length=25, **kwargs)
    c = r['mission']['capture']
    c['goal'] = 'surface'
    c['landing']['goal'] = 'land'
    pilot(r)['landing'].update(supported_feet=0, phase='assisted', foot_clearances=[6.0, 6.5],
        altitude=6.0, angle_degrees=1.0, descent_speed=1.7, lateral_speed=0.1,
        relative_spin=0.01, assist_strength=1.0)
    return r


def record(r=None):
    r = r or row()
    return model.diagnostic.snapshot(r, model.phase.PhaseClock().observe(r))


def sample(seed=0, selected=0, descent=4, tail=0.5, **changes):
    _, values, _ = model.state_vector(record())
    return {'recording_sha256': str(seed), 'seed': seed, 'seat': 0, 'selected_tick': selected,
        'tick': 120, 'state': values, 'remaining_descent_seconds': descent,
        'post_descent_seconds_bounds': [tail, tail], **changes}


def lookup(samples=None, context='initial'):
    return model.DescentLookup(model.make_profile('test',
        {context: samples or [sample(i) for i in range(3)]}, []))


class DescentLookupTests(unittest.TestCase):
    def test_clearance_reference_residual_and_post_descent_count_once(self):
        result = lookup().predict(record())
        self.assertEqual(result['reference_seconds'], 3)
        self.assertEqual(result['descent_seconds'], 4)
        self.assertEqual(result['post_descent_flight_seconds'], 0.5)
        self.assertEqual(result['seconds'], 4.5)
        self.assertEqual(result['envelope_seconds'], [4.5, 4.5])

    def test_current_clearance_changes_reference_not_future_outcome(self):
        samples = [sample(i) for i in range(6)]
        for s in samples[3:]:
            s['state']['clearance'] = 8
            s['remaining_descent_seconds'] = 5
        r = record()
        r['contact']['foot_clearances'] = [7, 7.5]
        result = lookup(samples).predict(r)
        self.assertEqual(result['descent_seconds'], 4.5)
        self.assertEqual(result['seconds'], 5)
        # Changing unrelated native altitude cannot replace the valid foot rays.
        r['contact']['altitude'] = 26
        self.assertEqual(lookup(samples).predict(r), result)

    def test_nearest_state_selection_uses_motion_not_outcome(self):
        samples = [sample(i) for i in range(3)]
        distant = sample(0, descent=100)
        distant['state']['descent_speed'] += 1
        self.assertEqual(lookup(samples + [distant]).predict(record())['descent_seconds'], 4)

    def test_worlds_balanced_and_attempts_deduplicated(self):
        samples = [sample(0, selected=i, descent=100) for i in range(10)]
        samples += [sample(1, descent=4), sample(2, descent=6)]
        result = lookup(samples).predict(record())
        self.assertEqual(result['descent_seconds'], 6)
        repeated = lookup(samples + samples * 5).predict(record())
        self.assertEqual(repeated['seconds'], result['seconds'])
        self.assertEqual(repeated['support'], result['support'])

    def test_dense_single_attempt_or_single_world_does_not_supply_support(self):
        for samples in [[sample()] * 100, [sample(0, i) for i in range(10)], [sample(0), sample(1)]]:
            self.assertEqual(lookup(samples).predict(record())['reason'], 'insufficient_local_descent_support')

    def test_local_support_required_inside_global_domain(self):
        samples = [sample(i) for i in range(6)]
        for i, s in enumerate(samples):
            s['state']['descent_speed'] += -4 if i < 3 else 4
        self.assertEqual(lookup(samples).predict(record())['reason'], 'insufficient_local_descent_support')

    def test_all_motion_and_clearance_domains_are_bounded(self):
        for key in model.SCALES:
            r = record()
            if key == 'clearance': r['contact']['foot_clearances'] = [7, 7.5]
            elif key == 'foot_gap': r['contact']['foot_clearances'][1] += 1
            elif key.startswith('gravity'): r['geometry'][key] += 1
            elif key == 'assist_strength': r['contact'][key] -= 0.1
            else: r['contact'][key] += 1
            with self.subTest(key=key):
                self.assertEqual(lookup().predict(r)['reason'], 'descent_outside_training_domain')

    def test_invalid_rays_assist_frame_and_motion_are_explicit(self):
        for change in ['no_hit', 'negative', 'missing_ray', 'nan_ray', 'weak_assist', 'flying',
                       'contact', 'boolean_contact', 'frame', 'stale', 'motion']:
            r = record()
            if change == 'no_hit': r['contact']['foot_clearances'][1] = 26
            if change == 'negative': r['contact']['foot_clearances'][0] = -0.1
            if change == 'missing_ray': r['contact']['foot_clearances'] = None
            if change == 'nan_ray': r['contact']['foot_clearances'][0] = float('nan')
            if change == 'weak_assist': r['contact']['assist_strength'] = 0.49
            if change == 'flying': r['contact']['phase'] = 'flying'
            if change == 'contact': r['contact']['supported_feet'] = 1
            if change == 'boolean_contact': r['contact']['supported_feet'] = False
            if change == 'frame': r['contact']['matches_target'] = False
            if change == 'stale': r['geometry'] = None
            if change == 'motion': r['contact']['descent_speed'] = None
            with self.subTest(change=change): self.assertIsNone(lookup().predict(r)['seconds'])

    def test_phase_entry_and_retry_context_are_required(self):
        for age in [None, -1, True, float('nan')]:
            r = record()
            r['phase']['age_seconds'] = age
            self.assertEqual(lookup().predict(r)['reason'], 'phase_entry_unobserved')
        r = record()
        r['phase']['context'] = 'retry'
        self.assertEqual(lookup().predict(r)['reason'], 'no_descent_clearance_calibration')
        r['phase'].update(context='initial', name='alignment')
        self.assertEqual(lookup().predict(r)['reason'], 'not_descent')

    def test_tampered_rule_domain_or_duration_is_rejected(self):
        for change in ['rule', 'domain', 'negative_duration', 'missing_feature']:
            p = copy.deepcopy(lookup().profile)
            if change == 'rule': p['rule']['minimum_worlds'] = 1
            if change == 'domain': p['cells']['initial']['domain']['clearance'][1] += 1
            if change == 'negative_duration': p['cells']['initial']['samples'][0]['remaining_descent_seconds'] = -1
            if change == 'missing_feature': del p['cells']['initial']['samples'][0]['state']['descent_speed']
            with self.subTest(change=change), self.assertRaises(ValueError): model.DescentLookup(p)


class DescentCompositionTests(unittest.TestCase):
    def states(self, r=None):
        r = r or row()
        selection = model.frozen.snapshot(r, 0, 'mission_selection')
        comparator = model.approach.StateTrip(selection, *profiles(), approach_lookup(), flight_model='duration-then-state')
        candidate = model.DescentTrip(selection, *profiles(), approach_lookup(), lookup())
        return comparator, candidate, r

    def test_ground_clocks_first_forecasts_and_comparator_remain_exact(self):
        old, new, r = self.states()
        a, b = old.observe(r), new.observe(r)
        self.assertEqual(b['comparator'], {k: a.get(k) for k in b['comparator']})
        self.assertEqual(b['total_seconds'], a['total_seconds'] + 4.5 - a['landing']['seconds'])
        for key in ['walking_legs', 'elapsed_seconds', 'original_total_seconds', 'flight_phase', 'flight_selection']:
            self.assertEqual(b[key], a[key])
        self.assertEqual(new.first_state, old.first_state)
        self.assertFalse(b['physical_permissions'])

    def test_other_phases_keep_exact_comparator(self):
        old, new, r = self.states()
        for tick, name in enumerate(['approach', 'alignment', 'settling'], 120):
            r['tick'] = pilot(r)['tick'] = tick
            c = r['mission']['capture']
            c['goal'] = 'approach' if name == 'approach' else 'surface'
            c['landing']['goal'] = 'approach' if name == 'alignment' else 'land'
            pilot(r)['landing']['supported_feet'] = int(name == 'settling')
            a, b = old.observe(r), new.observe(r)
            for key in b['comparator']: self.assertEqual(b[key], a.get(key))
            self.assertFalse(b['descent_lookup_attempted'])

    def test_missing_support_never_falls_back_to_numeric_duration(self):
        _, state, r = self.states()
        pilot(r)['landing']['foot_clearances'] = [26, 26]
        result = state.observe(r)
        self.assertIsNotNone(result['comparator']['total_seconds'])
        self.assertIsNone(result['total_seconds'])
        self.assertEqual(result['landing']['reason'], 'descent_foot_rays_unavailable')

    def test_ground_and_capture_budget_guards_survive_descent_candidate(self):
        _, state, r = self.states()
        state.observe(r)
        r = row(121)
        r['observation']['local']['objective_work'] = 'stale'
        result = state.observe(r)
        self.assertFalse(result['descent_lookup_attempted'])
        self.assertIsNone(result['total_seconds'])
        r = row(9061)
        r['mission']['capture']['landing']['goal_since'] = 9061
        result = state.observe(r)
        self.assertIn('capture_limit_reached', result['unknown_reasons'])
        self.assertIsNone(result['total_seconds'])

    def test_capture_restart_first_forecasts_and_terminal_scope(self):
        _, state, r = self.states()
        state.observe(r)
        first = copy.deepcopy(state.record())
        r = row(121)
        r['mission']['capture'].update(started_tick=121, goal_since=121)
        result = state.observe(r)
        self.assertEqual(result['descent_state']['phase']['context'], 'retry')
        self.assertIsNone(result['landing']['seconds'])
        self.assertEqual(state.first_descent, first['first_descent_prediction'])
        self.assertEqual(state.first_state, first['first_state_prediction'])
        r = row(122)
        r['mission']['capture']['landing']['landed_tick'] = 122
        self.assertEqual(state.observe(r)['status'], 'landed')
        self.assertIsNone(state.observe(row(123)))


class DescentCalibrationTests(unittest.TestCase):
    def data(self):
        feature = record()
        e = {**feature['phase'], 'last_tick': 359, 'end_tick': 360, 'closed_by': 'phase_change'}
        a = {'selection': {'seat': 0, 'planet': 0, 'selected_tick': 0}, 'phase_episodes': [e],
            'phase_breaks': [], 'actual': {'milestones': {'landed': [390, 390]}, 'stopped_tick': 500, 'ending': 'completed'}}
        trace = {k: copy.deepcopy(a[k]) for k in ['selection', 'phase_episodes', 'phase_breaks']}
        trace['samples'] = [{**copy.deepcopy(feature), 'tick': t} for t in range(120, 360)]
        return {'label': 'training', 'seed': 1, 'trace_sha256': 'trace', 'attempts': [a]}, {'attempts': [trace]}

    def test_training_cadence_and_later_ground_failure(self):
        run, traces = self.data()
        run['attempts'][0]['actual']['ending'] = 'abandoned'
        cells = {}
        model.add_training_run(cells, run, traces)
        self.assertEqual([s['tick'] for s in cells['initial']], [120, 180, 240, 300])
        self.assertEqual(cells['initial'][0]['remaining_descent_seconds'], 4)
        self.assertEqual(cells['initial'][0]['post_descent_seconds_bounds'], [0.5, 0.5])

    def test_failed_interrupted_or_unobserved_phase_is_retained_as_exclusion(self):
        for change in ['failure', 'break', 'unobserved']:
            run, traces = self.data()
            a, t = run['attempts'][0], traces['attempts'][0]
            if change == 'failure': a['actual']['milestones'] = {}
            if change == 'break': a['phase_breaks'] = t['phase_breaks'] = [{'tick': 361}]
            if change == 'unobserved': a['phase_episodes'][0]['entry_observed'] = t['phase_episodes'][0]['entry_observed'] = False
            cells = {}
            result = model.add_training_run(cells, run, traces)
            self.assertEqual(cells, {})
            self.assertTrue(result['episodes'])

    def test_mismatched_feature_lifecycle_is_rejected(self):
        run, traces = self.data()
        traces['attempts'][0]['phase_episodes'][0]['end_tick'] += 1
        with self.assertRaises(ValueError): model.add_training_run({}, run, traces)


if __name__ == '__main__':
    unittest.main()
