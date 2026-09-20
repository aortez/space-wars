"""Contracts for interpreting recorded trips without manufacturing calibration data."""
import copy
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / 'analyze-trip-costs.py'
SPEC = importlib.util.spec_from_file_location('trip_costs', SCRIPT)
costs = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = costs
SPEC.loader.exec_module(costs)


def leg(length=50, **changes):
    return {'failure': None, 'partial': False, 'length': length, 'start_node': 0,
            'reachable_nodes': 20, 'destination_nodes': 3, 'jumps': 0, 'flights': 0, **changes}


def row(tick, *, foot=False, claiming=False, revision=0, length=50, seat=0):
    owner = f'player_{seat + 1}'
    enemy = f'player_{2 - seat}'
    site = {'planet': 0, 'bearing': 12}
    route = {'site': site, 'outbound': leg(length), 'returning': leg(length), 'crossing': None}
    claim = {'owner': enemy, 'claimant': owner if claiming else None,
             'flag': {'player': enemy}, 'phase': 'lowering' if claiming else 'idle',
             'progress': 0.5 if claiming else 0.0, 'stage_required_seconds': 3.0,
             'status': 'lowering' if claiming else 'approach_flag'}
    pilot = {'tick': tick, 'owner': owner, 'location': 'on_foot' if foot else {'aboard': seat},
             'planet': {'index': 0, 'revision': revision, 'claim': claim},
             'ship': {'position': {'x': 0, 'y': 0}}, 'queries_ready': True,
             'ship_available': True, 'ship_form': 'ship'}
    local = {'combat': {'recovery': {'flight': {'pilot': pilot}},
                        'target': {'ground_occluded': False, 'motion': {'position': {'x': 100, 'y': 0}}}},
             'landing_objective': {'tick': tick - 1, 'validated_tick': tick,
                                   'objective': {'revision': revision}, 'sites': [copy.deepcopy(route)]}}
    return {'version': 1, 'seat': seat, 'tick': tick, 'observation': {'local': local},
            'mission': {'target': 0, 'capture': {'site': site, 'objective_route': route, 'ground': None}}}


def pilot(observation):
    return observation['observation']['local']['combat']['recovery']['flight']['pilot']


def visit(start=0, **changes):
    return {'planet': 0, 'selected_tick': start, 'arrived_tick': None, 'landed_tick': None,
            'claimed_tick': None, 'boarded_tick': None, 'departed_tick': None,
            'abandoned_tick': None, 'reason': None, **changes}


def report(visits, elapsed=100):
    return {'version': 2, 'seed': 42, 'elapsed_ticks': elapsed,
            'metrics': [{'visits': visits}, {'visits': []}],
            'missions': [{'policy': 'material_mission_v11'}, {'policy': 'material_mission_v10'}],
            'round': {'outcome': {'winner': 'player_1'}}}


def trip(*, physics_from=0, sparse=False):
    milestones = {k: [v, v] for k, v in {'selected': 0, 'arrived': 1, 'landed': 2,
                                       'claimed': 40, 'boarded': 60, 'departed': 80}.items()}
    result = costs.Trip(0, 0, 0, 80, 'completed', None, milestones, 'test', physics_from)
    for tick in (range(0, 80, 5) if sparse else range(80)):
        r = row(tick, foot=3 <= tick < 60, claiming=20 <= tick < 40)
        if tick >= 40:
            r['mission']['capture']['ground'] = {'destination': 'hatch', 'route': leg(50)}
        result.observe(r)
    return result


class ScoreReferenceTests(unittest.TestCase):
    def test_walk_and_jump_reference_is_in_seconds(self):
        self.assertEqual(costs.leg_seconds(leg()), 10)
        self.assertAlmostEqual(costs.leg_seconds(leg(jumps=2)), 10 + 30 / 38)

    def test_crossing_replaces_chord_walk_time_and_uses_slower_direction(self):
        crossing = {'version': 2, 'flights': [{'seconds': 6}, {'seconds': 8}],
                    'plan': {'start': {'x': 0, 'y': 0}, 'destination': {'x': 15, 'y': 0}}}
        # 50 units include the 15-unit chord: 35/5 + 8 seconds flight + 4 recharge.
        self.assertEqual(costs.leg_seconds(leg(flights=1), crossing), 19)
        self.assertEqual(costs.leg_seconds(leg(flights=0), crossing), 10)
        crossing['flights'][0]['seconds'] = float('nan')
        self.assertIsNone(costs.leg_seconds(leg(), crossing))

    def test_unusable_routes_remain_unknown(self):
        for change in [{'failure': 'no_path'}, {'partial': True}, {'length': -1},
                       {'length': float('inf')}, {'length': True}, {'start_node': None},
                       {'reachable_nodes': 0}, {'destination_nodes': 0}, {'jumps': 513},
                       {'jumps': True}, {'flights': True}, {'flights': 1}, {'flights': 2}]:
            with self.subTest(change=change):
                self.assertIsNone(costs.leg_seconds(leg(**change)))

    def test_missing_flag_term_is_not_a_zero_duration_prediction(self):
        r = row(5)
        p = pilot(r)
        p['planet']['claim']['flag'] = None
        p['planet']['claim']['owner'] = None
        estimate = costs.selected_estimate(r)
        self.assertEqual(estimate['status'], 'no_existing_flag_term')
        self.assertEqual(estimate['ground_score_seconds'], 0)
        self.assertIsNone(estimate['outbound_seconds'])
        self.assertEqual(estimate['full_uninterrupted_claim_seconds'], 3)
        p['planet']['claim'] = None
        self.assertEqual(costs.selected_estimate(r)['status'], 'unavailable')
        self.assertIsNone(costs.selected_estimate(r)['ground_score_seconds'])

    def test_incomplete_round_trip_does_not_supply_a_selected_cost(self):
        r = row(5)
        r['mission']['capture']['objective_route']['returning'] = None
        estimate = costs.selected_estimate(r)
        self.assertEqual(estimate['status'], 'unavailable')
        self.assertIsNone(estimate['outbound_seconds'])
        self.assertIsNone(estimate['ground_score_seconds'])

    def test_enemy_and_own_claim_stages_are_separate_from_walking(self):
        r = row(5)
        self.assertEqual(costs.selected_estimate(r)['full_uninterrupted_claim_seconds'], 6)
        pilot(r)['planet']['claim']['flag']['player'] = 'player_1'
        self.assertEqual(costs.selected_estimate(r)['full_uninterrupted_claim_seconds'], 3)
        pilot(r)['planet']['claim']['owner'] = 'player_1'
        self.assertEqual(costs.selected_estimate(r)['full_uninterrupted_claim_seconds'], 0)

    def test_estimate_source_is_frozen_and_cannot_learn_from_after_landing(self):
        t = costs.Trip(0, 0, 0, 20, 'stopped', None, {'landed': [10, 10]}, 'test', 0)
        t.observe(row(2))
        t.observe(row(3))
        t.observe(row(6, length=40))
        changed = row(9, revision=7, length=40)
        changed['observation']['local']['landing_objective'] = None
        t.observe(changed)
        t.observe(row(12, length=5))
        self.assertEqual(t.first_estimate['outbound_seconds'], 10)
        self.assertEqual(t.landing_estimate['outbound_seconds'], 8)
        self.assertEqual(t.landing_estimate['source_tick'], 5)
        self.assertEqual(t.landing_estimate['revision'], 0)
        self.assertEqual(t.landing_estimate['observed_tick'], 6)
        self.assertEqual(t.landing_estimate['last_seen_tick'], 9)

    def test_unmatched_retained_route_has_unknown_source_revision(self):
        r = row(20)
        r['observation']['local']['landing_objective']['sites'] = []
        estimate = costs.selected_estimate(r)
        self.assertIsNone(estimate['source_tick'])
        self.assertIsNone(estimate['revision'])

    def test_later_identical_survey_cannot_become_an_earlier_routes_source(self):
        t = costs.Trip(0, 0, 0, 20, 'stopped', None, {'landed': [10, 10]}, 'test', 0)
        first = row(2)
        first['observation']['local']['landing_objective'] = None
        t.observe(first)
        t.observe(row(8))
        self.assertIsNone(t.landing_estimate['source_tick'])
        self.assertEqual(t.landing_estimate['observed_tick'], 2)


class MeasurementTests(unittest.TestCase):
    def test_dense_boundaries_and_sparse_bounds(self):
        dense, sparse = trip().result(), trip(sparse=True).result()
        self.assertEqual(dense['milestones']['exited'], [3, 3])
        self.assertEqual(dense['milestones']['claim_started'], [20, 20])
        self.assertEqual(sparse['milestones']['exited'], [2, 5])
        self.assertEqual(sparse['milestones']['claim_started'], [2, 20])
        self.assertEqual(dense['phases']['outbound']['seconds_bounds'], [17 / 60] * 2)
        self.assertEqual(sparse['phases']['outbound']['seconds_bounds'], [0, 18 / 60])
        self.assertTrue(all(c['measured_seconds'] is None for c in sparse['comparisons']))

    def test_later_false_samples_cannot_exclude_a_claim_hidden_in_a_gap(self):
        samples = [costs.sample(row(t, foot=True, claiming=t == 20), 0) for t in [0, 1, 10, 19, 20]]
        self.assertEqual(costs.first_boundary(samples, 'claiming', 0, 30), [2, 20])

    def test_later_reclaim_cannot_supply_missing_start_of_an_earlier_claim(self):
        t = trip()
        for s in t.samples:
            s.claiming = s.tick == 65
        result = t.result()
        self.assertNotIn('claim_started', result['milestones'])
        self.assertEqual(result['phases']['outbound']['status'], 'boundary_unobserved')

    def test_missing_targets_and_queries_are_unknown_not_cover(self):
        samples = []
        for tick in range(4):
            r = row(tick, foot=True)
            if tick == 1:
                r['observation']['local']['combat']['target']['ground_occluded'] = True
            if tick == 2:
                r['observation']['local']['combat']['target'] = None
            if tick == 3:
                pilot(r)['queries_ready'] = False
            samples.append(costs.sample(r, 0))
        result = costs.exposure(samples, 0, 10)
        self.assertEqual(result['ship_proxy_seconds_bounds'], [1 / 60, 9 / 60])
        self.assertEqual(result['missing_ticks'], 6)
        self.assertEqual(result['ship_proxy_unknown_samples'], 2)
        self.assertEqual(result['on_foot_samples'], 4)
        self.assertEqual(result['pilot_visibility'], 'not_measured')

    def test_claim_on_other_planet_is_not_this_trips_claim(self):
        r = row(10, foot=True, claiming=True)
        pilot(r)['planet']['index'] = 1
        s = costs.sample(r, 0)
        self.assertFalse(s.exited)
        self.assertFalse(s.claiming)

    def test_phase_using_older_physics_is_excluded_but_new_return_is_usable(self):
        result = trip(physics_from=40).result()
        out = next(c for c in result['comparisons'] if c['phase'] == 'outbound')
        back = next(c for c in result['comparisons'] if c['estimate_origin'] == 'fresh_return_route')
        self.assertIn('older_physics_in_phase', out['excluded_reasons'])
        self.assertIsNone(out['measured_seconds'])
        self.assertEqual(back['excluded_reasons'], [])
        self.assertEqual(back['measured_seconds'], 20 / 60)

    def test_material_edit_excludes_stale_cost_from_calibration(self):
        t = trip()
        t.samples[45].revision = 1
        result = t.result()
        back = next(c for c in result['comparisons'] if c['estimate_origin'] == 'fresh_return_route')
        self.assertIn('changed_or_unverified_ground', back['excluded_reasons'])
        self.assertIsNone(back['ratio'])

    def test_missing_claim_start_is_unknown_boundary_not_endless_outbound(self):
        t = trip()
        for s in t.samples:
            s.claiming = False
        result = t.result()
        self.assertEqual(result['phases']['outbound']['status'], 'boundary_unobserved')
        self.assertEqual(result['phases']['outbound']['seconds_bounds'], [0, 37 / 60])
        self.assertEqual(result['phases']['claim']['status'], 'not_observed')
        out = next(c for c in result['comparisons'] if c['phase'] == 'outbound')
        self.assertIsNone(out['ratio'])

    def test_unfinished_attempt_is_censored_and_has_no_landing_comparison(self):
        t = costs.Trip(0, 0, 0, 30, 'abandoned', 'damage', {'selected': [0, 0]}, 'test', 0)
        t.observe(row(0))
        result = t.result()
        self.assertEqual(result['phases']['total']['status'], 'censored')
        self.assertEqual(result['phases']['total']['seconds_bounds'], [0.5, 0.5])
        self.assertEqual(result['comparisons'], [])
        self.assertIsNone(result['latest_retained_before_landing'])


class AttemptAndFileTests(unittest.TestCase):
    def test_reselection_cannot_borrow_later_or_same_tick_milestones(self):
        r = report([visit(0, arrived_tick=3, abandoned_tick=10, claimed_tick=10,
                          boarded_tick=22, departed_tick=25), visit(10, claimed_tick=20)])
        first, second = costs.build_trips(r, {'physics_valid_from_tick': 0})
        self.assertEqual(first.stop, 10)
        self.assertNotIn('claimed', first.milestones)
        self.assertEqual(first.discarded, {'claimed': 10, 'boarded': 22, 'departed': 25})
        self.assertEqual(second.milestones['claimed'], [20, 20])

    def test_departure_stops_visit_before_later_unrelated_abandonment(self):
        r = report([visit(0, claimed_tick=15, boarded_tick=16, departed_tick=20, abandoned_tick=30)])
        t, = costs.build_trips(r, {'physics_valid_from_tick': 0})
        self.assertEqual((t.stop, t.ending, t.reason), (20, 'completed', None))
        self.assertEqual(t.milestones['departed'], [20, 20])

    def test_physics_provenance_and_order_are_required(self):
        for config in [{'physics_valid_from_tick': -1}, {'physics_valid_from_tick': True}]:
            with self.assertRaises(ValueError):
                costs.build_trips(report([visit()]), config)
        with self.assertRaises(ValueError):
            costs.build_trips(report([visit(20), visit(10)]), {'physics_valid_from_tick': 0})

    def test_continuation_uses_explicit_trial_not_resumed_mission_history(self):
        r = report([visit(0, claimed_tick=95)], elapsed=100)
        r['successor_continuation'] = {'actor': 1, 'trial': {
            'source_tick': 10, 'stopped_tick': 90, 'stop_reason': 'capture trip completed',
            'proposal': {'approach': {'site': {'planet': 2}}},
            'sortie': {'arrived_tick': 19, 'surface_started_tick': 20, 'landed_tick': 30,
                       'exited_tick': 31, 'claimed_tick': 50, 'boarded_tick': 70, 'departed_tick': 90}}}
        t, = costs.build_trips(r, {'physics_valid_from_tick': 50, 'scope': 'continuation'})
        self.assertEqual((t.seat, t.planet, t.start, t.stop), (1, 2, 10, 90))
        self.assertEqual(t.milestones['arrived'], [20, 20])
        self.assertEqual(t.milestones['claimed'], [50, 50])
        self.assertEqual(t.physics_from, 50)

    def test_cli_and_invalid_trace_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'report.json').write_text(json.dumps(report([visit()])))
            trace = root / 'trace.jsonl'
            trace.write_text(json.dumps(row(0)) + '\n')
            config = {'label': 'fixture', 'directory': '.', 'source_commit': 'test',
                      'physics_valid_from_tick': 0}
            manifest = root / 'inputs.json'
            manifest.write_text(json.dumps({'version': 1, 'runs': [config]}))
            subprocess.run([sys.executable, str(SCRIPT), '--manifest', str(manifest),
                            '--out', str(root / 'out')], check=True, capture_output=True, text=True)
            result = json.loads((root / 'out/calibration.json').read_text())
            self.assertEqual(result['runs'][0]['trace_sha256'], costs.file_hash(trace))
            self.assertIn('fixture / P1 / 0', (root / 'out/calibration.md').read_text())
            for rows in [[row(0), row(0)], [row(2), row(1)], [row(1, seat=1)]]:
                if len(rows) == 1:
                    pilot(rows[0])['owner'] = 'player_1'
                trace.write_text(''.join(json.dumps(r) + '\n' for r in rows))
                with self.assertRaises(ValueError):
                    costs.analyze_run(config, root)


if __name__ == '__main__':
    unittest.main()
