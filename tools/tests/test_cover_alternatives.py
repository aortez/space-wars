import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('cover_alternatives', Path(__file__).parents[1]/'probe-cover-alternatives.py')
P = importlib.util.module_from_spec(spec)
spec.loader.exec_module(P)


def fixture():
    ids = [dict(planet=1, bearing=i) for i in range(2)]
    leg = dict(failure=None, partial=False, length=0.0, start_node=1,
               reachable_nodes=2, destination_nodes=1, jumps=0, flights=0)
    routes = [dict(site=site, outbound=copy.deepcopy(leg), returning=copy.deepcopy(leg)) for site in ids]
    objective = dict(planet=1, revision=0, owner='player_2', position=dict(x=0.0, y=0.0), range=2.8)
    batch = dict(actor='player_1', tick=31, planning='joint_round_trip', version=1,
                 objective=objective, sites=routes)
    selected = dict(site=ids[0], revision=0, site_order=0, rejection=None,
                    approach_score=1.0, cover_penalty=4000.0, ground_score=0.0, total_score=4001.0)
    choice = dict(tick=31, planet=1, revision=0, selected=selected, site_count=2,
                  required_site=None, objective=objective, exposed=True, commit_descent=True,
                  checks=dict(eligible=1), assessments=[selected,
                    dict(site=ids[1], revision=0, site_order=1, rejection='route_absent')])
    acquisition = dict(reason='selected_site', tick=31, checks=choice['checks'], selected_site=ids[0],
                       sites_available=2, required_site=None, objective=objective)
    pilot = dict(owner='player_1', tick=31, site_query='survey', planet=dict(index=1, revision=0),
                 sites=[dict(id=site, revision=0) for site in ids], ship=dict(position=dict(x=0.0, y=0.0)))
    local = dict(combat=dict(recovery=dict(flight=dict(pilot=pilot)),
                            target=dict(ground_occluded=False, motion=dict(position=dict(x=100.0, y=0.0)))),
                 cover=[dict(site=site, grounded=bool(i), approach=bool(i), departure=False) for i,site in enumerate(ids)],
                 landing_objective=dict(batch, sites=routes[:1]))
    return dict(world_tick=31, loop_tick=30, seat=0, observation=dict(local=local),
                mission=dict(capture=dict(started_tick=1, site=ids[0], acquisition=acquisition)),
                choice=choice, choice_unknown=None, choice_ms=0.01, routes_ms=0.1,
                route_batches=[batch], routes_unknown=None,
                route_costs=[dict(site=site, cost=0.0) for site in ids])


class CoverAlternativesTests(unittest.TestCase):
    def test_measurable_covered_route_omitted_from_native_shortlist_is_distinct(self):
        result = P.audit_sample(fixture())
        self.assertEqual(result['counts']['native_eligible_covered'], 0)
        self.assertEqual(result['counts']['omitted_covered_round_trips'], 1)
        self.assertTrue(result['sites'][1]['approach_covered'])
        self.assertFalse(result['sites'][1]['full_cover'])

    def test_absent_diagnostic_routes_remain_unmeasured(self):
        row = fixture()
        row.update(route_batches=None, route_costs=[], routes_unknown='sensor unavailable')
        result = P.audit_sample(row)
        self.assertIsNone(result['sites'][1]['diagnostic_route_cost'])
        self.assertFalse(result['sites'][1]['diagnostic_route_measured'])
        self.assertIsNone(result['counts']['omitted_covered_round_trips'])
        self.assertIsNone(result['counts']['diagnostic_covered_round_trips'])

    def test_ground_cover_without_approach_is_not_an_approach_certificate(self):
        row = fixture()
        row['observation']['local']['cover'][1]['approach'] = False
        result = P.audit_sample(row)
        self.assertEqual(result['counts']['diagnostic_covered_round_trips'], 0)
        self.assertIsNotNone(result['sites'][1]['diagnostic_route_cost'])

    def test_unwitnessed_native_choice_stays_unknown(self):
        row = fixture()
        row.update(choice=None, choice_unknown='not a fresh choice')
        result = P.audit_sample(row)
        self.assertIsNone(result['counts']['native_eligible_covered'])
        self.assertIsNone(result['counts']['omitted_covered_round_trips'])
        self.assertIsNone(result['sites'][1]['native_eligible_directions'])
        self.assertEqual(result['counts']['diagnostic_covered_round_trips'], 1)

    def test_partial_one_way_and_invalid_routes_are_not_round_trips(self):
        route = fixture()['route_batches'][0]['sites'][0]
        self.assertEqual(P.route_cost(route), 0.0)
        for key, value in [('partial', True), ('failure', 'disconnected'),
                           ('length', -1.0), ('length', float('nan')), ('flights', 1)]:
            changed = copy.deepcopy(route)
            changed['returning'][key] = value
            self.assertIsNone(P.route_cost(changed), key)
        route['returning'] = None
        self.assertIsNone(P.route_cost(route))

    def test_native_clock_identity_and_full_measurement_are_enforced(self):
        for mutation in ['clock', 'actor', 'missing_route', 'duplicate_route', 'changed_native_route']:
            row = fixture()
            if mutation == 'clock':
                row['world_tick'] = row['loop_tick']
            elif mutation == 'actor':
                row['seat'] = 1
            elif mutation == 'missing_route':
                row['route_batches'][0]['sites'].pop()
            elif mutation == 'duplicate_route':
                row['route_batches'][0]['sites'][1] = row['route_batches'][0]['sites'][0]
            else:
                row['observation']['local']['landing_objective'] = copy.deepcopy(row['observation']['local']['landing_objective'])
                row['observation']['local']['landing_objective']['sites'][0]['outbound']['length'] = 5.0
            with self.assertRaises(AssertionError, msg=mutation):
                P.audit_sample(row)

    def test_native_ranking_and_cover_penalties_are_checked(self):
        for key in ['total_score', 'cover_penalty', 'ground_score']:
            row = fixture()
            row['choice']['selected'][key] += 1.0
            with self.assertRaises(AssertionError, msg=key):
                P.audit_sample(row)


if __name__ == '__main__':
    unittest.main()
