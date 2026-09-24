"""Flight diagnostics preserve physical frames, causal history and missing data."""
import copy
import importlib.util
import io
import json
import math
from pathlib import Path
import tempfile
import unittest

from test_landing_phases import row as phase_row
from test_trip_costs import pilot

SCRIPT = Path(__file__).resolve().parents[1] / 'inspect-flight-progress.py'
SPEC = importlib.util.spec_from_file_location('flight_progress', SCRIPT)
flight = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(flight)


def vec(x, y):
    return {'x': x, 'y': y}


def row(tick=120, **kwargs):
    r = phase_row(tick, phase='approach', **kwargs)
    p = pilot(r)
    # A translating, spinning body; ship is 3 sideways and 4 above the target.
    p['planet']['motion'].update(position=vec(100, 200), velocity=vec(7, -2), spin=0.5)
    p['ship'].update(position=vec(103, 214), velocity=vec(-3, -4.5))
    p['gravity'] = vec(2, -6)
    p['sites'] = [{'id': copy.deepcopy(r['mission']['capture']['site']),
        'revision': p['planet']['revision'], 'vehicle_position': vec(100, 210),
        'normal': vec(0, 1)}]
    return r


class FlightGeometryTests(unittest.TestCase):
    def test_moving_rotating_frame_removes_orbit_and_spin_at_ship(self):
        result = flight.FlightProgress().observe(row())
        g = result['geometry']
        self.assertEqual(g['distance'], 5)
        self.assertEqual(g['height'], 4)
        self.assertEqual(g['side_error'], -3)
        self.assertEqual(g['normal_speed'], -4)
        self.assertEqual(g['right_speed'], -3)
        self.assertEqual(g['relative_speed'], 5)
        self.assertEqual(g['closing_speed'], 5)
        self.assertEqual(g['gravity_normal'], -6)
        self.assertFalse(result['physical_permissions'])

    def test_global_rotation_and_translation_do_not_change_features(self):
        r = row()
        p = pilot(r)
        def rotate(v, translate=False):
            x, y = v['x'], v['y']
            return vec(-y + (500 if translate else 0), x - (60 if translate else 0))
        for motion in [p['ship'], p['planet']['motion']]:
            motion['position'] = rotate(motion['position'], True)
            motion['velocity'] = rotate(motion['velocity'])
        p['sites'][0]['vehicle_position'] = rotate(p['sites'][0]['vehicle_position'], True)
        p['sites'][0]['normal'] = rotate(p['sites'][0]['normal'])
        p['gravity'] = rotate(p['gravity'])
        self.assertEqual(flight.FlightProgress().observe(r)['geometry'],
                         flight.FlightProgress().observe(row())['geometry'])

    def test_co_rotating_ship_has_zero_relative_velocity(self):
        r = row()
        pilot(r)['ship']['velocity'] = vec(0, -0.5)
        g = flight.FlightProgress().observe(r)['geometry']
        self.assertEqual(g['relative_speed'], 0)
        self.assertEqual(g['closing_speed'], 0)

    def test_landing_ray_altitude_is_not_used_as_site_height(self):
        r = row()
        pilot(r)['landing']['altitude'] = 26
        self.assertEqual(flight.FlightProgress().observe(r)['geometry']['height'], 4)

    def test_missing_or_invalid_geometry_is_unknown_without_zero_fallback(self):
        for field in ['velocity', 'normal', 'spin', 'position']:
            for bad in [None, float('nan'), True]:
                r = row()
                p = pilot(r)
                obj = p['sites'][0] if field == 'normal' else (
                    p['planet']['motion'] if field == 'spin' else p['ship'])
                obj[field] = bad
                with self.subTest(field=field, bad=bad):
                    result = flight.FlightProgress().observe(r)
                    self.assertIsNone(result['geometry'])
                    self.assertIsNotNone(result['unavailable_reason'])

    def test_site_evidence_must_be_current_unique_and_on_target(self):
        for change in ['dirty', 'absent', 'duplicate', 'stale', 'remote', 'pod', 'foot', 'landed']:
            r = row()
            p = pilot(r)
            if change == 'dirty': p['queries_ready'] = False
            if change == 'absent': p['sites'] = []
            if change == 'duplicate': p['sites'] *= 2
            if change == 'stale': p['sites'][0]['revision'] += 1
            if change == 'remote': p['planet']['index'] = 1
            if change == 'pod': p['ship_form'] = 'escape_pod'
            if change == 'foot': p['location'] = 'on_foot'
            if change == 'landed': r['mission']['capture']['landing']['landed_tick'] = r['tick']
            with self.subTest(change=change):
                self.assertIsNone(flight.FlightProgress().observe(r)['geometry'])

    def test_optional_gravity_and_exact_target_do_not_invent_values(self):
        r = row()
        p = pilot(r)
        p.pop('gravity')
        p['ship']['position'] = copy.deepcopy(p['sites'][0]['vehicle_position'])
        g = flight.FlightProgress().observe(r)['geometry']
        self.assertEqual(g['distance'], 0)
        self.assertIsNone(g['closing_speed'])
        self.assertIsNone(g['gravity_normal'])


class FlightHistoryTests(unittest.TestCase):
    def test_progress_uses_full_dense_second_in_moving_frame(self):
        state = flight.FlightProgress()
        for tick in range(120, 181):
            r = row(tick)
            # Whole world translates quickly while site-relative height closes.
            p = pilot(r)
            for point in [p['planet']['motion']['position'], p['ship']['position'],
                          p['sites'][0]['vehicle_position']]:
                point['x'] += 10 * (tick - 120)
            p['ship']['position']['y'] -= (tick - 120) / 60
            result = state.observe(r)
            if tick < 180:
                self.assertIsNone(result['one_second_progress'])
        progress = result['one_second_progress']
        self.assertEqual(progress['from_tick'], 120)
        self.assertAlmostEqual(progress['distance_closed'], 5 - math.sqrt(18))
        self.assertEqual(progress['height_reduced'], 1)
        self.assertEqual(progress['absolute_side_error_reduced'], 0)

    def test_gap_phase_retry_revision_and_missing_geometry_break_history(self):
        for change in ['gap', 'phase', 'retry', 'revision', 'missing', 'selection']:
            state = flight.FlightProgress()
            for tick in range(120, 180): state.observe(row(tick))
            r = row(181 if change == 'gap' else 180)
            if change == 'phase': r['mission']['capture']['goal'] = 'seek_cover'
            if change == 'retry': r['mission']['capture']['replans'] = 1
            if change == 'revision':
                pilot(r)['planet']['revision'] += 1
                pilot(r)['sites'][0]['revision'] += 1
            if change == 'missing': pilot(r)['sites'] = []
            if change == 'selection':
                r['mission']['events'].append({'kind': 'selected', 'tick': 180, 'planet': 0})
            result = state.observe(r)
            with self.subTest(change=change):
                self.assertIsNone(result['one_second_progress'])
                self.assertIsNotNone(result['history_reset'])
                if change == 'revision':
                    self.assertEqual(result['phase']['age_seconds'], 1)
                    self.assertEqual(result['phase']['context'], 'initial')
                if change == 'retry':
                    self.assertEqual(result['phase']['context'], 'retry')

    def test_history_is_bounded_and_cannot_be_changed_by_future_outcome(self):
        state = flight.FlightProgress()
        prefix = [state.observe(row(t)) for t in range(120, 181)]
        before = json.dumps(prefix, sort_keys=True)
        for t in range(181, 400): state.observe(row(t))
        r = row(400)
        r['mission']['capture']['failed_tick'] = 400
        state.observe(r)
        self.assertEqual(json.dumps(prefix, sort_keys=True), before)
        fresh = flight.FlightProgress()
        self.assertEqual(prefix, [fresh.observe(row(t)) for t in range(120, 181)])
        self.assertLessEqual(len(state.history), 61)

    def test_wrong_owner_future_clocks_and_reordered_observations_are_rejected(self):
        for change in ['owner', 'tick', 'start', 'event', 'counter']:
            r = row()
            if change == 'owner': pilot(r)['owner'] = 'player_2'
            if change == 'tick': pilot(r)['tick'] += 1
            if change == 'start': r['mission']['capture']['started_tick'] = 121
            if change == 'event': r['mission']['events'].append({'kind': 'selected', 'tick': 121, 'planet': 0})
            if change == 'counter': r['mission']['capture']['replans'] = -1
            with self.subTest(change=change), self.assertRaises(ValueError):
                flight.FlightProgress().observe(r)
        state = flight.FlightProgress()
        state.observe(row())
        with self.assertRaises(ValueError): state.observe(row())

    def test_sampled_replay_processes_dense_history_and_binds_raw_source(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'trace.jsonl'
            path.write_text(''.join(json.dumps(row(t)) + '\n' for t in range(120, 181)))
            output = io.StringIO()
            metadata = flight.replay(path, 0, output)
            records = [json.loads(line) for line in output.getvalue().splitlines()]
            self.assertEqual([r['tick'] for r in records], [120, 180])
            self.assertIsNotNone(records[-1]['one_second_progress'])
            self.assertEqual(metadata['observations'], 61)
            self.assertEqual(metadata['trace_sha256'], flight.costs.file_hash(path))

    def test_controlled_adapter_retains_separate_capture_scope(self):
        r = row()
        raw = {'version': 1, 'scope': flight.controlled.SCOPE, 'seat': 0, 'tick': 120,
            'capture_started_tick': 60, 'observation': r['observation']['local'],
            'capture': r['mission']['capture'], 'controls': {}}
        result = flight.FlightProgress().observe(flight.controlled.adapt(raw))
        self.assertEqual(result['scope'], 'controlled_ground_v1')
        self.assertEqual(result['selected_tick'], 60)
        self.assertEqual(result['capture_started_tick'], 60)


if __name__ == '__main__':
    unittest.main()
