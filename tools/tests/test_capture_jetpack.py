import copy
import importlib.util
from pathlib import Path
import unittest

from test_capture_topology import fixture as topology_fixture

spec = importlib.util.spec_from_file_location('capture_jetpack', Path(__file__).parents[1]/'probe-capture-jetpack.py')
J = importlib.util.module_from_spec(spec)
spec.loader.exec_module(J)


def measurement(route):
    cost = J.expected_cost(route)
    powered = route.get('crossing') is not None
    return dict(route=copy.deepcopy(route), cost=cost, work=dict(graph=50, physics_queries=30),
        forecasts=dict(started=int(powered), approved=int(powered), rejected={}),
        measurements=dict(finished_surveys=1, finished_candidates=1, successful_candidates=int(cost is not None),
            powered_candidates=int(powered and cost is not None), failures={} if cost is not None else {'disconnected': 1}))


def fixture(powered=False):
    row = topology_fixture()
    p = row['observation']['local']['combat']['recovery']['flight']['pilot']
    p['vehicle'] = 0
    p['planet']['motion'] = dict(position=dict(x=0, y=0), angle=0)
    for site in p['sites']:
        site['vehicle_position'] = dict(x=0, y=65)
    row['topology']['base']['nodes'] = [dict(id=n, position=dict(x=x, y=60)) for n, x in [(1,-8), (2,8)]]
    native = row['route_batches'][0]
    batch = copy.deepcopy(native)
    batch.update(planning='jetpack_round_trip', actual=None)
    if powered:
        walking = native['sites'][1]
        walking['outbound']['failure'] = 'disconnected'
        walking['returning'] = None
        walking['endpoint'] = None
        row['route_costs'][1]['cost'] = None
        row['topology']['sites'][1]['with_ship'] = dict(route=copy.deepcopy(walking), outbound_path=[], returning_path=None)
        route = batch['sites'][1]
        route['outbound'].update(length=16.0, flights=1)
        route['returning']['start_node'] = 2
        route['endpoint'] = dict(id=2)
        route['crossing'] = dict(version=2, measured_tick=31, launch_until_tick=151, nodes=[1,2],
            plan=dict(planet=1, revision=0, direction='Left', start=dict(x=-8,y=60), destination=dict(x=8,y=60),
                      cruise_radius=68, anchor=dict(Vehicle=dict(index=0,form='ship',position=dict(x=0,y=65),angle=0))),
            flights=[dict(seconds=3.0, burn_seconds=1.0, arrival_speed=1.0) for _ in range(2)])
    pairs = [dict(site=a['site'], walking=measurement(a), jetpack=measurement(b))
             for a,b in zip(native['sites'], batch['sites'])]
    row.update(jetpack=dict(equipped=True, batches=[batch], sites=pairs), jetpack_unknown=None, jetpack_ms=0.2)
    return row


class CaptureJetpackTests(unittest.TestCase):
    def test_valid_powered_round_trip_is_distinct_from_an_unchanged_walk(self):
        result = J.audit_sample(fixture(powered=True))['jetpack']
        self.assertEqual(result['outcomes'], {'unchanged_walk':1, 'powered_round_trip':1})
        self.assertEqual(result['newly_reachable_covered'], [dict(planet=1,bearing=1)])
        self.assertEqual(result['newly_reachable_covered_shortlisted'], [])

    def test_missing_equipment_preserves_walks_without_starting_forecasts(self):
        row = fixture()
        row['jetpack']['equipped'] = False
        self.assertEqual(J.audit_sample(row)['jetpack']['outcomes'], {'unchanged_walk':2})
        row = fixture(powered=True)
        row['jetpack']['equipped'] = False
        with self.assertRaises(AssertionError):
            J.audit_sample(row)

    def test_rejected_flight_and_unstarted_forecast_stay_distinct(self):
        for reason in [None, 'fuel_reserve']:
            row = fixture(powered=True)
            pair = row['jetpack']['sites'][1]
            pair['jetpack'] = copy.deepcopy(pair['walking'])
            row['jetpack']['batches'][0]['sites'][1] = copy.deepcopy(pair['walking']['route'])
            if reason:
                pair['jetpack']['forecasts'] = dict(started=1, approved=0, rejected={reason:1})
            result = J.audit_sample(row)['jetpack']
            expected = 'forecast_'+reason if reason else 'no_powered_forecast_started'
            self.assertEqual(result['outcomes'][expected], 1)
            self.assertEqual(result['newly_reachable_covered'], [])

    def test_stale_wrong_geometry_or_unsafe_flight_is_rejected(self):
        for change in ['tick','reserve','node','window','hull','version']:
            row = fixture(powered=True)
            c = row['jetpack']['sites'][1]['jetpack']['route']['crossing']
            if change == 'tick': c['measured_tick'] -= 1
            elif change == 'reserve': c['flights'][0]['burn_seconds'] = 3.0
            elif change == 'node': c['nodes'][0] = 3
            elif change == 'window': c['launch_until_tick'] += 1
            elif change == 'hull': c['plan']['anchor']['Vehicle']['position']['x'] = 5
            else: c['version'] = 1
            row['jetpack']['batches'][0]['sites'][1] = copy.deepcopy(row['jetpack']['sites'][1]['jetpack']['route'])
            with self.assertRaises(AssertionError, msg=change):
                J.audit_sample(row)

    def test_native_batch_identity_counts_and_costs_are_enforced(self):
        for change in ['missing','duplicate','actor','cost','counts','walking_parity']:
            row = fixture()
            if change == 'missing': row['jetpack']['batches'][0]['sites'].pop()
            elif change == 'duplicate': row['jetpack']['sites'].append(copy.deepcopy(row['jetpack']['sites'][0]))
            elif change == 'actor': row['jetpack']['batches'][0]['actor'] = 'player_2'
            elif change == 'cost': row['jetpack']['sites'][0]['jetpack']['cost'] += 1
            elif change == 'counts': row['jetpack']['sites'][0]['jetpack']['forecasts']['started'] = 1
            else: row['jetpack']['sites'][0]['walking']['route']['outbound']['length'] = 3
            with self.assertRaises(AssertionError, msg=change):
                J.audit_sample(row)

    def test_unavailable_measurement_is_not_zero_available_powered_routes(self):
        row = fixture()
        row.update(jetpack=None, jetpack_unknown='prospective survey unavailable')
        result = J.audit_sample(row)
        self.assertEqual(result['jetpack_unknown'], row['jetpack_unknown'])
        self.assertNotIn('jetpack', result)


if __name__ == '__main__':
    unittest.main()
