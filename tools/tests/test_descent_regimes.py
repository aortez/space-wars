"""Coarse descent fallback preserves support without concealing invalid evidence."""
import copy
import unittest

from test_descent_clearance import model, row, lookup, pilot, profiles, approach_lookup, sample


def states(r=None, fitted=None):
    r = r or row()
    selection = model.frozen.snapshot(r, 0, 'mission_selection')
    args = (selection, *profiles(), approach_lookup(), fitted or lookup())
    return model.DescentTrip(*args), model.DescentRegimeTrip(*args), r


def ordinary_transient(r):
    pilot(r)['landing'].update(phase='flying', assist_strength=0.4)
    return r


class DescentRegimeTests(unittest.TestCase):
    def test_supported_clearance_is_exact_strict_forecast(self):
        strict, regime, r = states()
        a, b = strict.observe(r), regime.observe(r)
        self.assertEqual(b['descent_selection']['selected'], 'clearance')
        self.assertEqual(b['strict'], {k: a.get(k) for k in b['strict']})
        for key in b['comparator']: self.assertEqual(b.get(key), a.get(key))
        self.assertEqual(b['clearance_prediction'], a['landing'])
        self.assertEqual(regime.first_descent, strict.first_descent)

    def test_flying_and_weak_assist_retain_entire_supported_coarse_forecast(self):
        for native, strength in [('flying', 0.95), ('assisted', 0.4), ('flying', 0.0)]:
            strict, regime, r = states()
            pilot(r)['landing'].update(phase=native, assist_strength=strength)
            a, b = strict.observe(r), regime.observe(r)
            self.assertIsNone(a['total_seconds'])
            self.assertEqual(b['descent_selection']['selected'], 'duration')
            self.assertEqual(b['clearance_prediction']['reason'], 'descent_assist_unsupported')
            for key in b['comparator']: self.assertEqual(b[key], a['comparator'][key])

    def test_domain_and_local_support_misses_can_retain_coarse_duration(self):
        for kind in ['domain', 'local', 'cell']:
            fitted = lookup()
            if kind == 'local': fitted = lookup([sample(0), sample(1)])
            if kind == 'cell': fitted = lookup(context='retry')
            _, regime, r = states(fitted=fitted)
            if kind == 'domain': pilot(r)['landing']['descent_speed'] = 8
            b = regime.observe(r)
            self.assertEqual(b['descent_selection']['selected'], 'duration')
            self.assertEqual(b['total_seconds'], b['comparator']['total_seconds'])
            self.assertIsNone(b['strict']['total_seconds'])

    def test_bad_assist_values_or_native_phase_are_not_ordinary_transients(self):
        for native, strength in [('assisted', -0.1), ('assisted', 1.01), ('mystery', 0.4),
                                 ('settling', 0.4), ('flying', None), ('flying', True)]:
            _, regime, r = states()
            pilot(r)['landing'].update(phase=native, assist_strength=strength)
            b = regime.observe(r)
            self.assertIsNotNone(b['comparator']['total_seconds'])
            self.assertEqual(b['descent_selection']['selected'], 'unknown')
            self.assertIsNone(b['total_seconds'])

    def test_ray_contact_frame_and_motion_guards_never_fall_back(self):
        for kind in ['ray', 'negative_ray', 'missing_ray', 'contact', 'site', 'revision', 'motion', 'age']:
            _, regime, r = states()
            p = pilot(r)
            if kind == 'ray': p['landing']['foot_clearances'][0] = 26
            if kind == 'negative_ray': p['landing']['foot_clearances'][0] = -0.1
            if kind == 'missing_ray': p['landing']['foot_clearances'] = None
            if kind == 'contact': p['landing']['supported_feet'] = False
            if kind == 'site': p['sites'] = []
            if kind == 'revision': p['sites'][0]['revision'] += 1
            if kind == 'motion': p['landing']['descent_speed'] = None
            if kind == 'age': r['mission']['capture']['landing']['goal_since'] = 100
            b = regime.observe(r)
            with self.subTest(kind=kind):
                self.assertIsNone(b['total_seconds'])
                self.assertNotEqual(b['descent_selection']['selected'], 'duration')

    def test_missing_or_expired_duration_cannot_be_recreated(self):
        for kind in ['missing', 'expired']:
            _, regime, r = states()
            ordinary_transient(r)
            if kind == 'missing': regime.phase_profile['cells'].pop('initial/descent')
            if kind == 'expired':
                for tick in range(120, 481):
                    r = ordinary_transient(row(tick))
                    b = regime.observe(r)
            else:
                b = regime.observe(r)
            self.assertEqual(b['descent_selection']['reason'], 'coarse_duration_unavailable')
            self.assertIsNone(b['total_seconds'])

    def test_ground_and_budget_guards_survive_fallback(self):
        _, regime, r = states()
        regime.observe(ordinary_transient(r))
        r = ordinary_transient(row(121))
        r['observation']['local']['objective_work'] = 'stale'
        b = regime.observe(r)
        self.assertFalse(b['descent_lookup_attempted'])
        self.assertIsNone(b['total_seconds'])
        r = ordinary_transient(row(9061))
        r['mission']['capture']['landing']['goal_since'] = 9061
        b = regime.observe(r)
        self.assertIn('capture_limit_reached', b['unknown_reasons'])
        self.assertIsNone(b['total_seconds'])

    def test_unknown_ground_tail_cannot_be_hidden_by_duration(self):
        _, regime, r = states()
        del regime.profile['cells']['walk/exit']
        b = regime.observe(ordinary_transient(r))
        self.assertIn('unknown_surface_or_departure_cost', b['unknown_reasons'])
        self.assertEqual(b['descent_selection']['reason'], 'other_evidence_or_budget_guard')
        self.assertIsNone(b['total_seconds'])

    def test_selection_tracks_current_support_without_a_timer_or_latch(self):
        _, regime, r = states()
        selected = []
        for tick, transient in [(120, True), (121, False), (122, True), (123, False)]:
            r = row(tick)
            if transient: ordinary_transient(r)
            selected.append(regime.observe(r)['descent_selection']['selected'])
        self.assertEqual(selected, ['duration', 'clearance', 'duration', 'clearance'])

    def test_fallback_then_retry_preserves_all_first_forecasts_and_clock(self):
        strict, regime, r = states()
        ordinary_transient(r)
        strict.observe(r)
        a = regime.observe(r)
        first = copy.deepcopy(regime.record())
        r = row(121, retry=1)
        b = regime.observe(r)
        for key in ['first_regime_prediction', 'first_descent_prediction', 'first_state_prediction',
                    'first_composed_prediction', 'original_prediction']:
            self.assertEqual(regime.record()[key], first[key])
        self.assertEqual(regime.first_descent, strict.first_descent)
        self.assertEqual(b['flight_phase']['context'], 'retry')
        self.assertEqual(b['budget']['first_time_limit_tick'], a['budget']['first_time_limit_tick'])
        self.assertEqual(b['elapsed_seconds'], a['elapsed_seconds'] + 1 / 60)

    def test_restored_forecast_does_not_alias_retained_comparators(self):
        _, regime, r = states()
        b = regime.observe(ordinary_transient(r))
        saved = copy.deepcopy(regime.record())
        b['landing']['seconds'] = -99
        b['budget']['first_time_limit_tick'] = -99
        self.assertNotEqual(b['comparator']['landing']['seconds'], -99)
        self.assertNotEqual(b['comparator']['budget']['first_time_limit_tick'], -99)
        self.assertEqual(regime.record(), saved)

    def test_other_phases_and_terminal_results_are_exact(self):
        strict, regime, r = states()
        for tick, phase in enumerate(['approach', 'alignment', 'settling', 'landed'], 120):
            r = row(tick)
            c = r['mission']['capture']
            c['goal'] = 'approach' if phase == 'approach' else 'surface'
            c['landing']['goal'] = 'approach' if phase == 'alignment' else 'land'
            pilot(r)['landing']['supported_feet'] = int(phase == 'settling')
            if phase == 'landed': c['landing']['landed_tick'] = tick
            a, b = strict.observe(r), regime.observe(r)
            for key in b['comparator']: self.assertEqual(b.get(key), a.get(key))
            self.assertEqual(b['descent_selection']['selected'], 'comparator')
        self.assertIsNone(regime.observe(row(124)))


if __name__ == '__main__':
    unittest.main()
