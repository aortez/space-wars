import copy
import importlib.util
import json
from pathlib import Path
import unittest
import tempfile

from test_cover_alternatives import fixture as cover_fixture

spec = importlib.util.spec_from_file_location('capture_topology', Path(__file__).parents[1]/'probe-capture-topology.py')
T = importlib.util.module_from_spec(spec)
spec.loader.exec_module(T)


def fixture():
    row = cover_fixture()
    row.update(choice=None, choice_unknown='no fresh choice', topology_unknown=None, topology_ms=0.1)
    sites = []
    for route in row['route_batches'][0]['sites']:
        route['endpoint'] = dict(id=1)
        trip = dict(route=copy.deepcopy(route), outbound_path=[1], returning_path=[1])
        sites.append(dict(site=route['site'], removed_nodes=[], removed_edges=[],
                          with_ship=trip, without_ship=copy.deepcopy(trip)))
    row['topology'] = dict(objective=row['route_batches'][0]['objective'], gravity=9.8,
        base=dict(version=1, actor='player_1', planet=1, revision=0, tick=31,
            nodes=[dict(id=n) for n in [1, 2]],
            edges=[dict(**{'from': a, 'to': b}, kind='walk', length=1.0) for a,b in [(1,2), (2,1)]],
            rejected=[dict(id=n, reason='steep_floor') for n in range(512) if n not in [1,2]]), sites=sites)
    return row


class CaptureTopologyTests(unittest.TestCase):
    def test_native_routes_paths_and_base_components_are_bound_to_observation(self):
        result = T.audit_topology(fixture())
        self.assertEqual(result['base_graph']['components'], [[1, 2]])
        self.assertEqual(result['counts']['covered_round_trips_without_own_ship'], 1)
        self.assertEqual(result['counts']['hull_breaks_round_trip'], 0)

    def test_hull_counterfactual_is_kept_separate_from_native_access(self):
        row = fixture()
        native = row['route_batches'][0]['sites'][1]
        native['outbound']['failure'] = 'disconnected'
        native['returning'] = None
        native['endpoint'] = None
        row['route_costs'][1]['cost'] = None
        site = row['topology']['sites'][1]
        site['with_ship'] = dict(route=copy.deepcopy(native), outbound_path=[], returning_path=None)
        result = T.audit_topology(row)
        self.assertEqual(result['counts']['diagnostic_covered_round_trips'], 0)
        self.assertEqual(result['counts']['covered_round_trips_without_own_ship'], 1)
        self.assertEqual(result['counts']['hull_breaks_round_trip'], 1)

    def test_one_way_edges_are_not_a_strong_component(self):
        self.assertEqual(T.components({1, 2, 3}, [{'from': 1, 'to': 2}, {'from': 2, 'to': 1},
                                                {'from': 2, 'to': 3}]), [[1, 2], [3]])

    def test_inconsistent_graph_delta_and_native_route_are_rejected(self):
        for change in ['missing_edge_removal', 'bad_edge_index', 'duplicate_node', 'native_route', 'clock']:
            row = fixture()
            site = row['topology']['sites'][0]
            if change == 'missing_edge_removal':
                site['removed_nodes'] = [2]
            elif change == 'bad_edge_index':
                site['removed_edges'] = [2]
            elif change == 'duplicate_node':
                site['removed_nodes'] = [2, 2]
            elif change == 'native_route':
                site['with_ship']['route']['outbound']['length'] = 1.0
            else:
                row['topology']['base']['tick'] += 1
            with self.assertRaises(AssertionError, msg=change):
                T.audit_topology(row)

    def test_path_must_follow_measured_directed_edges_and_length(self):
        row = fixture()
        trip = row['topology']['sites'][0]['without_ship']
        for path in [[1, 3], [1, 2], [1, 1]]:
            trip['outbound_path'] = path
            with self.assertRaises((AssertionError, KeyError)):
                T.audit_paths(trip, {1, 2, 3}, row['topology']['base']['edges'])

    def test_unavailable_topology_remains_unknown(self):
        row = fixture()
        row.update(topology=None, topology_unknown='no full observed site survey')
        result = T.audit_topology(row)
        self.assertEqual(result['topology_unknown'], row['topology_unknown'])
        self.assertNotIn('base_graph', result)

    def test_optional_report_fields_require_matching_presence(self):
        T.report_parity({}, {}, ['cover_response'])
        T.report_parity(dict(cover_response=None), dict(cover_response=None), ['cover_response'])
        with self.assertRaises(AssertionError):
            T.report_parity({}, dict(cover_response=None), ['cover_response'])

    def test_retained_finished_search_does_not_count_later_selected_site_samples(self):
        row = fixture()
        route = row['route_batches'][0]['sites'][0]
        p = row['observation']['local']['combat']['recovery']['flight']['pilot']
        p['site_query'] = dict(selected=route['site'])
        row['mission']['capture'].update(failure=None, cover_response=dict(search=dict(
            started_tick=10, finished_tick=31, seeded=True)))
        later = copy.deepcopy(row)
        later['observation']['local']['combat']['recovery']['flight']['pilot']['tick'] = 61
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'trace.jsonl').write_text('\n'.join(json.dumps(r) for r in [row, later]))
            history = T.search_history(root, 0)
        self.assertEqual([p['world_tick'] for p in history[0]['probes']], [31])


if __name__ == '__main__':
    unittest.main()
