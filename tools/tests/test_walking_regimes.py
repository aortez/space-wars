"""Walking regimes choose frozen support per leg, without repairing evidence."""
import copy
import math
import unittest

from test_composed_trip_estimates import model, profiles
from test_landing_phases import row


def tail(outbound, returning, p=None, missing=None, mode='short-and-affine'):
    p = p or profiles()
    snap = model.frozen.snapshot(row(), 0, 'local_choice')
    snap['references'].update(outbound=outbound, return_board=returning)
    snap['missing'] = missing or []
    anchor = model.frozen.estimate(snap, p[0])
    return model.ground_tail(anchor, p[2], mode), anchor


def state(r=None, p=None):
    r = r or row(length=2.5)
    s = model.ComposedTrip(model.frozen.snapshot(r, 0, 'mission_selection'),
                           *(p or profiles()), walk_model='short-and-affine')
    return s, s.observe(r)


class WalkingRegimeTests(unittest.TestCase):
    def test_short_costs_and_envelopes_are_exactly_the_supported_originals(self):
        result, base = tail(0, 0.5)
        for leg in model.walking.LEGS:
            self.assertEqual(result[leg]['regime'], 'short_empirical')
            self.assertEqual(result[leg]['model'], model.frozen.MODEL)
            for field in ['estimate_seconds', 'historical_envelope_seconds', 'unknown_reasons']:
                self.assertEqual(result[leg][field], base['phases'][leg][field])
        self.assertGreater(result['outbound']['estimate_seconds'], 0)

    def test_exact_minimum_belongs_to_affine_and_boundary_is_not_smoothed(self):
        before, _ = tail(math.nextafter(1, -math.inf), 0.5)
        at, _ = tail(1, 0.5)
        self.assertEqual(before['outbound']['regime'], 'short_empirical')
        self.assertAlmostEqual(before['outbound']['estimate_seconds'], 4)
        self.assertEqual(at['outbound']['regime'], 'moderate_affine')
        self.assertEqual(at['outbound']['estimate_seconds'], 3.25)

    def test_each_leg_has_its_own_boundary_and_can_use_a_different_model(self):
        p = profiles()
        p[2]['cells']['return_board']['reference_seconds_domain'][0] = 2
        result, _ = tail(1.5, 1.5, p)
        self.assertEqual(result['outbound']['regime'], 'moderate_affine')
        self.assertEqual(result['return_board']['regime'], 'short_empirical')
        self.assertEqual(result['outbound']['estimate_seconds'], 3.875)
        self.assertEqual(result['return_board']['estimate_seconds'], 4.5)

    def test_upper_boundary_is_inclusive_but_longer_legacy_support_cannot_fill_it(self):
        at, _ = tail(7, 7)
        outside, base = tail(math.nextafter(7, math.inf), 7)
        self.assertEqual(at['outbound']['estimate_seconds'], 10.75)
        self.assertIsNotNone(base['phases']['outbound']['estimate_seconds'])
        self.assertEqual(outside['outbound']['regime'], 'unsupported')
        self.assertIsNone(outside['outbound']['estimate_seconds'])

    def test_gap_below_affine_minimum_stays_unknown_when_legacy_lacks_support(self):
        p = profiles()
        p[0]['cells']['walk/outbound']['reference_seconds_domain'] = [0, 0.25]
        result, _ = tail(0.5, 4, p)
        self.assertEqual(result['outbound']['regime'], 'short_empirical')
        self.assertIsNone(result['outbound']['estimate_seconds'])
        self.assertIn('reference_outside_calibration_domain', result['outbound']['unknown_reasons'])

    def test_invalid_evidence_and_missing_legacy_cell_cannot_be_repaired(self):
        for missing in ['changed_material', 'route_not_current_at_choice', 'requires_aboard_full_ship']:
            result, _ = tail(0.5, 4, missing=[missing])
            self.assertTrue(all(result[n]['estimate_seconds'] is None for n in model.walking.LEGS))
        p = profiles()
        del p[0]['cells']['walk/return_board']
        result, _ = tail(4, 0.5, p)
        self.assertIsNone(result['return_board']['estimate_seconds'])

    def test_missing_fit_cannot_invent_a_boundary_or_enable_legacy_fallback(self):
        p = profiles()
        p[2]['cells']['outbound'] = model.walking.fit_cell([])
        result, base = tail(0.5, 4, p)
        self.assertIsNotNone(base['phases']['outbound']['estimate_seconds'])
        self.assertIsNone(result['outbound']['estimate_seconds'])
        self.assertEqual(result['outbound']['regime'], 'unsupported')

    def test_invalid_numeric_references_do_not_enter_a_regime(self):
        for ref in [None, -1, float('nan'), float('inf'), True]:
            result, _ = tail(ref, 0.5)
            self.assertEqual(result['outbound']['regime'], 'unsupported')
            self.assertIsNone(result['outbound']['estimate_seconds'])

    def test_validated_route_change_switches_regime_without_rewriting_first_forecast(self):
        s, first = state()
        self.assertEqual(first['total_seconds'], first['phase_only_total_seconds'])
        self.assertEqual(first['walking_legs']['outbound']['regime'], 'short_empirical')
        after = s.observe(row(121, length=25))
        self.assertIn('route', after['invalidated_by'])
        self.assertEqual(after['reference_acquired_tick'], 121)
        self.assertEqual(after['walking_legs']['outbound']['regime'], 'moderate_affine')
        self.assertEqual(s.first_composed, first)
        self.assertEqual(after['budget']['first_time_limit_tick'], first['budget']['first_time_limit_tick'])
        self.assertEqual(after['flight_phase']['age_seconds'], 1 / 60)
        self.assertFalse(after['physical_permissions'])

    def test_stale_references_and_retries_keep_existing_guards_and_clocks(self):
        s, first = state()
        stale = row(121, length=25)
        stale['observation']['local']['objective_work'] = 'stale'
        self.assertIsNone(s.observe(stale)['total_seconds'])
        after = s.observe(row(122, length=25, retry=1))
        self.assertEqual(after['flight_phase']['context'], 'retry')
        self.assertEqual(after['elapsed_seconds'], first['elapsed_seconds'] + 2 / 60)
        self.assertEqual(after['budget']['first_time_limit_tick'], first['budget']['first_time_limit_tick'])
        self.assertEqual(s.first_composed, first)

    def test_moderate_walk_and_nonwalking_models_are_unchanged(self):
        p = profiles()
        for category in ['walk', 'no_flag', 'jump', 'crossing']:
            snap = model.frozen.snapshot(row(length=25), 0, 'local_choice')
            snap['category'] = category
            anchor = model.frozen.estimate(snap, p[0])
            frozen_inputs = copy.deepcopy([anchor, p])
            strict = model.ground_tail(anchor, p[2])
            selected = model.ground_tail(anchor, p[2], 'short-and-affine')
            for value in selected.values():
                value.pop('regime', None)
            self.assertEqual(selected, strict)
            self.assertEqual([anchor, p], frozen_inputs)

    def test_mode_is_explicit_and_default_remains_the_strict_comparator(self):
        default, _ = tail(0.5, 0.5, mode='affine')
        self.assertIsNone(default['outbound']['estimate_seconds'])
        _, result = state()
        self.assertEqual(result['model'], model.REGIME_MODEL)
        with self.assertRaises(ValueError):
            tail(0.5, 0.5, mode='typo')

    def test_distinct_unstarted_setups_can_share_a_trace_without_duplicating_trials(self):
        run = {'trace_sha256': 'same_preparation', 'attempts': []}
        prediction = {'attempts': []}
        a = {'seed': 42, 'seat': 0, 'band': {'minimum': 2, 'maximum': 6, 'direction': -1}, 'offset': 0.6}
        b = {**a, 'band': {'minimum': 8, 'maximum': 20, 'direction': -1}}
        first = model.evaluation_identities(run, a, prediction, model.controlled.SCOPE)
        second = model.evaluation_identities(run, b, prediction, model.controlled.SCOPE)
        self.assertTrue(first.isdisjoint(second))
        self.assertEqual(first, model.evaluation_identities(run, dict(a, label='different_name'), prediction, model.controlled.SCOPE))
        # Once a capture exists, different band labels cannot legitimize a
        # repeated recording. Normal missions keep their original strict rule.
        for actual, forecast, scope in [([{}], [{}], model.controlled.SCOPE),
                                        ([], [], 'missions')]:
            r, p = {**run, 'attempts': actual}, {'attempts': forecast}
            self.assertEqual(model.evaluation_identities(r, a, p, scope),
                             model.evaluation_identities(r, b, p, scope))


if __name__ == '__main__':
    unittest.main()
