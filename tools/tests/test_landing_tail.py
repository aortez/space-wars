"""Landing diagnostics keep causal state, native retries and elapsed partitions distinct."""
import copy
import importlib.util
import io
import json
import math
from pathlib import Path
import tempfile
import unittest

from test_flight_progress import row, pilot

SPEC = importlib.util.spec_from_file_location('landing_tail',
    Path(__file__).resolve().parents[1] / 'inspect-landing-tail.py')
model = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(model)


def snapshot(r=None):
    r = r or row(length=25)
    pilot(r)['ship'].update(angle=0.5, spin=0.7)
    pilot(r)['landing'].update(phase='assisted', settled_seconds=0, relative_spin=0.2)
    return model.snapshot(r, model.phase.PhaseClock().observe(r))


def episode(start, end, name='approach', **kwargs):
    return {'entry_tick': start, 'end_tick': end, 'last_tick': end - 1, 'name': name, **kwargs}


class TailFeatureTests(unittest.TestCase):
    def test_site_heading_and_spin_use_the_moving_frame(self):
        s = snapshot()
        self.assertAlmostEqual(s['attitude']['heading_error_radians'], -0.5)
        self.assertAlmostEqual(s['attitude']['relative_spin'], 0.2)
        self.assertIsNone(s['seconds_since_landing_progress'])
        self.assertFalse(s['physical_permissions'])

    def test_rotation_and_translation_preserve_attitude(self):
        r = row(length=25)
        s = snapshot(r)
        p = pilot(r)
        for obj in [p['ship'], p['planet']['motion']]:
            for key in ['position', 'velocity']:
                v = obj[key]
                v['x'], v['y'] = -v['y'], v['x']
                if key == 'position': v['x'] += 500
        site = p['sites'][0]
        v = site['vehicle_position'];v['x'], v['y'] = -v['y'] + 500, v['x']
        v = site['normal'];v['x'], v['y'] = -v['y'], v['x']
        p['ship']['angle'] += math.pi / 2
        rotated = model.snapshot(r, model.phase.PhaseClock().observe(r))
        for key, value in s['attitude'].items():
            self.assertAlmostEqual(rotated['attitude'][key], value)

    def test_missing_stale_or_ambiguous_site_does_not_invent_attitude(self):
        for kind in ['missing', 'stale', 'ambiguous']:
            r = row(length=25)
            if kind == 'missing': pilot(r)['sites'] = []
            if kind == 'stale': pilot(r)['sites'][0]['revision'] += 1
            if kind == 'ambiguous': pilot(r)['sites'] *= 2
            s = snapshot(r)
            self.assertIsNone(s['attitude']['heading_error_radians'])
            self.assertIsNone(s['geometry'])

    def test_missing_pose_and_no_hit_rays_are_not_contact_evidence(self):
        r = row(length=25)
        p = pilot(r)
        p['ship'].update(angle=float('nan'), spin=None)
        p['landing']['supported_feet'] = True
        s = model.snapshot(r, model.phase.PhaseClock().observe(r))
        self.assertEqual(s['attitude'], {'heading_error_radians': None, 'relative_spin': None})
        self.assertTrue(s['contact']['ray_limit_or_no_hit'])
        self.assertIsNone(s['contact']['supported_feet'])

    def test_site_reacquisition_is_not_another_native_retry(self):
        before = snapshot()
        cleared = copy.deepcopy(before)
        cleared.update(tick=121, site=None)
        cleared['native_counts'].update(replans=1, live_invalidations=1, objective_replans=1)
        event = model.transition(before, cleared)
        self.assertIn('native_replan', event['tags'])
        self.assertIn('live_invalidations', event['tags'])
        self.assertEqual(event['counter_delta']['replans'], 1)
        acquired = copy.deepcopy(cleared)
        acquired.update(tick=122, site=before['site'])
        event = model.transition(cleared, acquired)
        self.assertIn('site_acquired', event['tags'])
        self.assertNotIn('native_replan', event['tags'])
        self.assertEqual(event['counter_delta']['replans'], 0)

    def test_gaps_capture_changes_and_counter_resets_are_explicit(self):
        before = snapshot()
        for kind in ['gap', 'capture', 'reset']:
            after = copy.deepcopy(before)
            after['tick'] = 122 if kind == 'gap' else 121
            if kind == 'capture': after['capture_started_tick'] = 121
            if kind == 'reset':
                before['native_counts']['replans'] = 2
                after['native_counts']['replans'] = 0
            event = model.transition(before, after)
            if kind == 'reset': self.assertIn('counter_reset', event['tags'])
            else: self.assertIsNone(event['counter_delta'])
            self.assertNotIn('native_replan', event['tags'])

    def test_later_observations_do_not_mutate_retained_features(self):
        state = model.LandingTrace(model.frozen.snapshot(row(), 0, 'mission_selection'))
        for tick in range(120, 181): state.observe(row(tick, length=25))
        prefix = json.dumps(state.samples, sort_keys=True)
        old = copy.deepcopy(state.samples)
        r = row(181, length=25)
        r['mission']['capture']['landing']['landed_tick'] = 181
        state.observe(r)
        self.assertEqual(json.dumps(state.samples[:len(old)], sort_keys=True), prefix)
        self.assertIsNone(state.observe(row(182, length=25)))


class TailAccountingTests(unittest.TestCase):
    def test_original_phase_schema_uses_manifest_and_per_run_runtime(self):
        config = {'runtime_revision': 'frozen', 'runs': [{}]}
        evaluation = {'model': model.phase.MODEL, 'manifest_sha256': 'hash',
                      'runs': [{'source_commit': 'frozen'}]}
        model.verify_evaluation(config, evaluation, 'frozen', 'hash')

    def test_compatibility_does_not_accept_unbound_or_mixed_runtime_sources(self):
        config = {'runtime_revision': 'frozen', 'runs': [{}]}
        for bad in ['manifest', 'run', 'unknown_schema']:
            evaluation = {'model': model.phase.MODEL, 'manifest_sha256': 'hash',
                          'runs': [{'source_commit': 'frozen'}]}
            if bad == 'manifest': evaluation['manifest_sha256'] = 'different'
            if bad == 'run': evaluation['runs'][0]['source_commit'] = 'different'
            if bad == 'unknown_schema': evaluation['model'] = 'unknown'
            with self.assertRaises(ValueError): model.verify_evaluation(config, evaluation, 'frozen', 'hash')

    def test_phase_partition_clips_boundaries_without_double_counting(self):
        spans = [episode(0, 120), episode(120, 180, 'alignment'), episode(180, 240, 'descent')]
        parts = model.partition(spans, 60, 210)
        self.assertEqual(parts, {'approach': 1, 'alignment': 1, 'descent': 0.5,
                                'unclassified_or_unobserved': 0})

    def test_missing_intervals_are_not_invented_phase_time(self):
        spans = [episode(0, 120, last_tick=59), episode(180, 240, 'descent')]
        parts = model.partition(spans, 0, 240)
        self.assertEqual(parts['approach'], 1)
        self.assertEqual(parts['unclassified_or_unobserved'], 2)
        with self.assertRaises(ValueError):
            model.partition([episode(0, 120), episode(60, 180)], 0, 180)

    def fixture(self, landing=None):
        actual = {'milestones': {'landed': landing} if landing else {}, 'stopped_tick': 400,
                  'ending': 'abandoned'}
        trace = {'samples': [{'tick': 300}], 'events': [],
            'phase_episodes': [episode(0, 120), episode(150, 240), episode(240, 300, 'settling')],
            'phase_breaks': [{'tick': 120, 'reason': 'plan_restart'}, {'tick': 150, 'reason': 'plan_restart'}]}
        return {'actual': actual}, trace

    def test_pre_break_and_final_segment_sum_without_calling_it_avoidable_time(self):
        a, t = self.fixture([300, 300])
        d = model.duration(a, t, 60)
        self.assertEqual(d['time_before_final_break_seconds'], 1.5)
        self.assertEqual(d['final_segment_seconds'], 2.5)
        self.assertAlmostEqual(sum(d['phase_seconds'].values()), d['observed_seconds'])
        # Later failure on the ground does not erase a physically completed landing.
        self.assertTrue(d['landed_exactly'])

    def test_uncertain_or_censored_landings_never_become_exact_tails(self):
        for landing in [None, [299, 301], [400, 400]]:
            a, t = self.fixture(landing)
            self.assertFalse(model.duration(a, t, 60)['landed_exactly'])

    def test_replay_uses_existing_lifecycle_and_checks_it_on_join(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory);(root/'run').mkdir()
            rows = [row(t, length=25) for t in range(120, 181)]
            rows[-1]['mission']['capture']['landing']['landed_tick'] = 180
            (root/'run/trace.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in rows))
            result = model.composed.rolling.replay({'seats': [0], 'directory': 'run'}, root,
                None, io.StringIO(), trip_factory=model.LandingTrace)[0]
            a = {k: copy.deepcopy(result[k]) for k in ['selection', 'phase_episodes', 'phase_breaks']}
            a['actual'] = {'milestones': {'landed': [180, 180]}, 'stopped_tick': 200, 'ending': 'completed'}
            self.assertTrue(model.join(a, result)['from_first_choice']['landed_exactly'])
            a['phase_breaks'].append({'tick': 140, 'reason': 'invented'})
            with self.assertRaises(ValueError): model.join(a, result)


if __name__ == '__main__':
    unittest.main()
