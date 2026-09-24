"""The expiry regime adds evidence without replacing supported forecasts."""
import copy
import unittest
from unittest.mock import Mock

from test_approach_state import model, lookup, row, profiles, pilot, phase_row


def trip(fitted=None, configured=None, first=None):
    first = first or row(length=25)
    return model.StateTrip(model.frozen.snapshot(first, 0, 'mission_selection'),
        *(configured or profiles()), fitted or lookup(progress=True),
        flight_model='duration-then-state')


def advance(state, start=120, end=480, **kwargs):
    for tick in range(start, end + 1):
        result = state.observe(row(tick, length=25, **kwargs))
    return result


class ExpiryCompositionTests(unittest.TestCase):
    def test_all_supported_forecasts_remain_exact_without_state_queries(self):
        fitted = Mock()
        state = trip(fitted)
        baseline = model.composed.ComposedTrip(state.selection, *profiles(), walk_model='short-and-affine')
        for tick in range(120, 480):
            r = row(tick, length=25)
            old, new = baseline.observe(r), state.observe(r)
            self.assertIsNotNone(old['total_seconds'])
            for field, value in old.items():
                if field != 'model':
                    self.assertEqual(new[field], value, (tick, field))
        fitted.predict.assert_not_called()
        self.assertEqual(new['model'], model.EXPIRY_MODEL)
        self.assertEqual(state.first_composed, baseline.first_composed)

    def test_expiry_boundary_uses_existing_progress_and_preserves_native_clock(self):
        state = trip()
        before = advance(state, end=479)
        first = copy.deepcopy(state.record())
        new = advance(state, start=480)
        self.assertEqual(before['flight_selection']['selected'], 'duration')
        self.assertIsNotNone(before['total_seconds'])
        self.assertEqual(new['flight_phase']['age_seconds'], 6)
        self.assertIsNone(new['duration_total_seconds'])
        self.assertEqual(new['flight_selection']['reason'], 'duration_support_expired')
        self.assertEqual(new['landing']['seconds'], 8)
        self.assertIsNotNone(new['total_seconds'])
        self.assertEqual(new['budget']['first_time_limit_tick'], before['budget']['first_time_limit_tick'])
        self.assertEqual(new['walking_legs'], before['walking_legs'])
        self.assertEqual(new['elapsed_seconds'], before['elapsed_seconds'] + 1 / 60)
        for name in ['first_state_prediction', 'first_composed_prediction', 'original_prediction']:
            self.assertEqual(state.record()[name], first[name])
        self.assertFalse(new['physical_permissions'])

    def test_one_surviving_attempt_is_expired_support_not_zero_time_remaining(self):
        p = profiles()
        p[1]['cells']['initial/approach']['samples'][1]['phase_seconds'] = 8
        result = advance(trip(configured=p))
        self.assertEqual(result['duration_landing']['support'], 1)
        self.assertEqual(result['landing']['support'], 3)
        self.assertIsNotNone(result['total_seconds'])

    def test_absent_or_never_supported_cells_do_not_qualify_as_expired(self):
        for change in ['absent', 'empty', 'one_attempt', 'repeated_attempt']:
            p, fitted = profiles(), Mock()
            cells = p[1]['cells']
            samples = cells['initial/approach']['samples']
            if change == 'absent': del cells['initial/approach']
            if change == 'empty': samples.clear()
            if change == 'one_attempt': samples.pop()
            if change == 'repeated_attempt': samples[:] = [samples[0]] * 20
            result = advance(trip(fitted, p))
            with self.subTest(change=change):
                self.assertIsNone(result['total_seconds'])
                self.assertEqual(result['flight_selection']['reason'], 'no_expired_duration_support')
                fitted.predict.assert_not_called()

    def test_failed_state_support_stays_unknown_and_keeps_duration_evidence(self):
        samples = lookup(progress=True).profile['cells']['initial/progress']['samples'][:2]
        fitted = model.StateLookup(model.make_profile('test', {'initial/progress': samples}, []))
        result = advance(trip(fitted))
        self.assertIsNone(result['total_seconds'])
        self.assertIn('insufficient_local_approach_support', result['unknown_reasons'])
        self.assertEqual(result['duration_unknown_reasons'], ['insufficient_surviving_phase_attempts'])

    def test_unobserved_entry_or_observation_gap_cannot_gain_a_forecast(self):
        for gap in [False, True]:
            fitted = Mock()
            state = trip(fitted)
            if gap: advance(state, end=479)
            result = state.observe(row(481, length=25))
            self.assertIsNone(result['total_seconds'])
            self.assertEqual(result['landing']['reason'], 'phase_entry_unobserved')
            fitted.predict.assert_not_called()

    def test_revision_refresh_requires_a_new_complete_progress_window(self):
        state = trip()
        advance(state, end=479)
        result = state.observe(row(480, length=25, revision=1))
        self.assertEqual(result['flight_phase']['age_seconds'], 6)
        self.assertIsNone(result['total_seconds'])
        self.assertEqual(result['landing']['reason'], 'approach_progress_window_missing')
        result = advance(state, start=481, end=540, revision=1)
        self.assertIsNotNone(result['total_seconds'])

    def test_retry_and_capture_restart_return_to_duration_without_borrowing_initial_state(self):
        for restart in ['retry', 'capture']:
            state = trip()
            advance(state)
            r = row(481, length=25, retry=int(restart == 'retry'))
            if restart == 'capture': r['mission']['capture'].update(started_tick=481, goal_since=481)
            result = state.observe(r)
            self.assertEqual(result['flight_phase']['context'], 'retry')
            self.assertEqual(result['flight_selection']['selected'], 'duration')
            self.assertIsNotNone(result['total_seconds'])
            for tick in range(482, 842):
                r = row(tick, length=25, retry=int(restart == 'retry'))
                if restart == 'capture': r['mission']['capture'].update(started_tick=481, goal_since=481)
                result = state.observe(r)
            self.assertIsNone(result['total_seconds'])
            self.assertEqual(result['landing']['reason'], 'no_approach_state_calibration')
            self.assertEqual(result['approach_state']['phase']['context'], 'retry')

    def test_missing_geometry_cannot_restore_an_expired_forecast(self):
        state = trip()
        advance(state, end=479)
        r = row(480, length=25)
        pilot(r)['sites'] = []
        result = state.observe(r)
        self.assertIsNone(result['total_seconds'])
        self.assertEqual(result['landing']['reason'], 'selected_site_unavailable_or_ambiguous')

    def test_ground_and_budget_guards_skip_state_lookup(self):
        for guard in ['ground', 'queries', 'retry_limit']:
            fitted = Mock()
            state = trip(fitted)
            retries = 4 if guard == 'retry_limit' else 0
            advance(state, end=479, retry=retries)
            r = row(480, length=25, retry=retries)
            if guard == 'ground': r['observation']['local']['objective_work'] = 'stale'
            if guard == 'queries': pilot(r)['queries_ready'] = False
            result = state.observe(r)
            with self.subTest(guard=guard):
                self.assertIsNone(result['total_seconds'])
                fitted.predict.assert_not_called()

    def test_expiry_cannot_extend_native_time_limit(self):
        p, fitted = profiles(), Mock()
        for sample in p[1]['cells']['initial/approach']['samples']:
            sample['phase_seconds'] = 1 / 60
        state = trip(fitted, p)
        for tick in [9000, 9001]:
            r = row(tick, length=25)
            r['mission']['capture'].update(started_tick=0, goal_since=9000)
            result = state.observe(r)
        self.assertIn('capture_limit_reached', result['unknown_reasons'])
        self.assertIsNone(result['total_seconds'])
        self.assertEqual(result['budget']['first_time_limit_tick'], 9001)
        self.assertEqual(result['flight_selection']['reason'], 'other_evidence_or_budget_guard')
        fitted.predict.assert_not_called()

    def test_other_flight_phases_and_terminal_stop_retain_duration_model(self):
        state = trip()
        advance(state)
        for tick, name in enumerate(['alignment', 'descent', 'settling', 'landed'], 481):
            r = row(tick, length=25)
            r['mission']['capture'] = phase_row(tick, phase=name, length=25)['mission']['capture']
            if name == 'settling': pilot(r)['landing']['supported_feet'] = 1
            result = state.observe(r)
            self.assertEqual(result['landing'], result['duration_landing'])
            self.assertEqual(result['total_seconds'], result['duration_total_seconds'])
            self.assertFalse(result['flight_selection']['state_lookup_attempted'])
        self.assertEqual(result['status'], 'landed')
        self.assertIsNone(state.observe(row(485, length=25)))

    def test_rule_must_be_explicit_and_known(self):
        with self.assertRaises(ValueError):
            model.StateTrip(model.frozen.snapshot(row(), 0, 'mission_selection'),
                            *profiles(), lookup(), flight_model='automatic')


if __name__ == '__main__':
    unittest.main()
