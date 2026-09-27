import copy
import importlib.util
import json
import math
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('landing_choices', Path(__file__).resolve().parents[1] / 'compare-landing-choices.py')
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)


def fixture(sun=False, sun_position=None):
    reference = dict(planet=1, bearing=1)
    case = dict(seat=0, destination=1, candidate=dict(local_reference=dict(evidence=dict(site=reference))))
    p = dict(tick=11, owner='player_1', site_query='survey', ship=dict(position=dict(x=0, y=120)),
        planet=dict(index=1, revision=1, radius=60, claim=dict(flag=None),
                    motion=dict(position=dict(x=0, y=0), velocity=dict(x=0, y=0), spin=0)),
        sites=[dict(id=dict(planet=1, bearing=b), revision=1,
                    vehicle_position=dict(x=0 if b == 0 else 60, y=60 if b == 0 else 0),
                    normal=dict(x=0 if b == 0 else 1, y=1 if b == 0 else 0)) for b in range(2)])
    local = dict(combat=dict(recovery=dict(flight=dict(pilot=p)), target=None), cover=[],
        sun=dict(position=sun_position or dict(x=1000, y=1000), radius=20, heat_radius=44) if sun else None,
        planet_orbit_omega=None)
    checks = dict.fromkeys(['directions', 'eligible', 'previously_rejected', 'required_site',
        'route_absent', 'route_unusable', 'solar_cooldown', 'survey_unavailable',
        'unsafe_approach', 'unsafe_departure', 'unsafe_parking', 'unsafe_solar'], 0)
    assessments = []
    for i, site in enumerate(p['sites']):
        short = P.short_angle(p, site)
        preferred = -1 if short < 0 else 1
        for order, side in enumerate([preferred, -preferred] if sun else [preferred]):
            angle = P.directed(short, side) if sun else short
            score = P.f32(abs(angle)*120)
            solar = P.solar_plan(local, site, side)
            if solar: solar.pop('departure_corridors')
            assessments.append(dict(site=site['id'], revision=1, site_order=i, direction_order=order,
                side=side, short_angle=short, solar=solar, rejection=None,
                approach_score=score, cover_penalty=0, ground_score=0, total_score=score))
    checks['directions'] = checks['eligible'] = len(assessments)
    native = dict(reason='selected_site', tick=11, planet=1, revision=1, site_query='survey', selected_site=p['sites'][0]['id'],
        required_site=None, objective=None, checks=checks, sites_available=2)
    capture = dict(acquisition=native, site=native['selected_site'], started_tick=11,
                   replans=0, failed_tick=None, solar=assessments[0]['solar'])
    row = dict(seat=0, tick=11, observation=dict(local=local), mission=dict(capture=capture, goal='capture', target=1, recovery=None))
    r = dict(model='native_landing_choice_comparison_v1', tick=11, planet=1, revision=1,
        reference=reference, selected=assessments[0], reference_best=assessments[2 if sun else 1],
        classification='higher_reference_score', exposed=False, commit_descent=True,
        required_site=None, site_count=2, objective=None, checks=checks, assessments=assessments)
    choice = dict(reference=reference, observation_tick=11, actor='player_1', report=r,
                  unknown=None, assessment_ms=.1, physics_queries=0)
    # Detach JSON-like aliases so mutation tests alter only their named payload.
    return copy.deepcopy(case), copy.deepcopy(row), copy.deepcopy(choice)


class LandingChoiceAudit(unittest.TestCase):
    def test_optional_delayed_first_choice_still_requires_current_native_tick(self):
        case,row,choice = fixture()
        row['mission']['capture']['started_tick'] -= 5
        with self.assertRaises(AssertionError): P.audit_choice(case,row,choice)
        P.audit_choice(case,row,choice,allow_delayed_start=True)
        row['mission']['capture']['acquisition']['tick'] -= 1
        with self.assertRaises(AssertionError): P.audit_choice(case,row,choice,allow_delayed_start=True)

    def test_sensor_parity_ignores_time_but_preserves_counts_and_actor_clock(self):
        with tempfile.TemporaryDirectory() as folder:
            a, b = Path(folder)/'a', Path(folder)/'b'
            a.mkdir()
            b.mkdir()
            original = dict(tick=1, seat=0, profile=dict(counters=dict(queries=3),
                            stages=dict(survey=dict(calls=2, inclusive_ms=.2, exclusive_ms=.1))))
            (a/'sensors.jsonl').write_text(json.dumps(original)+'\n')
            def write(row): (b/'sensors.jsonl').write_text(json.dumps(row)+'\n')
            changed = copy.deepcopy(original)
            changed['profile']['stages']['survey']['inclusive_ms'] = 10
            write(changed)
            self.assertEqual(P.audit_sensors(a, b)['rows'], 1)
            for mutation in ['tick', 'seat', 'calls', 'queries']:
                changed = copy.deepcopy(original)
                if mutation in ['tick', 'seat']: changed[mutation] += 1
                elif mutation == 'calls': changed['profile']['stages']['survey']['calls'] += 1
                else: changed['profile']['counters']['queries'] += 1
                write(changed)
                with self.assertRaises(AssertionError): P.audit_sensors(a, b)

    def test_neutral_comparison_binds_rank_and_solar_geometry(self):
        for sun in [False, True]:
            case, row, choice = fixture(sun)
            result = P.audit_choice(case, row, choice)
            self.assertEqual(result['classification'], 'higher_reference_score')
            self.assertEqual(result['assessments'], 4 if sun else 2)
            self.assertFalse(result['independently_unresolved_clearance_signs'])

    def test_closest_tie_retains_preferred_direction(self):
        case, row, choice = fixture(True)
        r = choice['report']
        self.assertEqual(r['assessments'][0]['total_score'], r['assessments'][1]['total_score'])
        r['selected'] = r['assessments'][1]
        with self.assertRaises(AssertionError): P.audit_choice(case, row, choice)

    def test_missing_or_reordered_opposite_direction_is_detected(self):
        for mutation in ['drop', 'swap', 'side']:
            case, row, choice = fixture(True)
            rows = choice['report']['assessments']
            if mutation == 'drop': rows.pop()
            elif mutation == 'swap': rows[0], rows[1] = rows[1], rows[0]
            else: rows[1]['side'] = rows[0]['side']
            with self.assertRaises(AssertionError): P.audit_choice(case, row, choice)

    def test_forged_score_cover_and_solar_cause_are_detected(self):
        for mutation in ['score', 'cover', 'solar', 'cause', 'nan']:
            case, row, choice = fixture(True)
            a = choice['report']['assessments'][-1]
            if mutation == 'score': a['total_score'] += 10
            elif mutation == 'cover': a['cover_penalty'] = 400
            elif mutation == 'solar': a['solar']['approach_clearance'] = -1
            elif mutation == 'cause': a['rejection'] = 'unsafe_solar'
            else: a['short_angle'] = math.nan
            with self.assertRaises(AssertionError): P.audit_choice(case, row, choice)

    def test_reference_departure_side_is_checked_outside_a_numerical_tie(self):
        case, row, choice = fixture(True)
        # The non-winning site has different left/right departure clearance.
        r = choice['report']
        index = r['reference_best']['site_order']
        direction = r['reference_best']['direction_order']
        a = next(v for v in r['assessments'] if v['site_order'] == index and v['direction_order'] == direction)
        raw = P.solar_plan(row['observation']['local'], P.P.pilot(row)['sites'][index], a['side'])
        self.assertGreater(abs(raw['departure_corridors'][0]-raw['departure_corridors'][1]), 2*P.CLEARANCE_TOLERANCE)
        a['solar']['departure_side'] *= -1
        r['reference_best'] = a
        with self.assertRaises(AssertionError): P.audit_choice(case, row, choice)

    def test_true_departure_corridor_ties_are_explicitly_unresolved(self):
        case, row, choice = fixture(True, dict(x=0, y=-1000))
        result = P.audit_choice(case, row, choice)
        self.assertTrue(result['independently_unresolved_departure_sides'])
        for entry in result['independently_unresolved_departure_sides']:
            self.assertLessEqual(abs(entry['corridor_clearances'][0]-entry['corridor_clearances'][1]), 2*P.CLEARANCE_TOLERANCE)

    def test_reference_clock_actor_and_native_counts_are_bound(self):
        for mutation in ['reference', 'clock', 'actor', 'counts', 'site', 'geometry']:
            case, row, choice = fixture()
            if mutation == 'reference': choice['reference']['bearing'] = 8
            elif mutation == 'clock': choice['observation_tick'] += 1
            elif mutation == 'actor': choice['actor'] = 'player_2'
            elif mutation == 'counts': choice['report']['checks']['eligible'] += 1
            elif mutation == 'site': choice['report']['assessments'][1]['revision'] += 1
            else: P.P.pilot(row)['sites'][1]['vehicle_position'] = dict(x=-60, y=0)
            with self.assertRaises(AssertionError): P.audit_choice(case, row, choice)

    def test_unknowns_are_retained_without_inventing_a_comparison(self):
        case, row, choice = fixture()
        choice.update(report=None, unknown='native choice could not be reproduced')
        self.assertEqual(P.audit_choice(case, row, choice)['classification'], 'unknown')
        choice.update(observation_tick=None, actor=None, unknown='no observed site choice', assessment_ms=0)
        self.assertEqual(P.audit_choice(case, None, choice)['classification'], 'not_observed')
        choice['observation_tick'] = 11
        with self.assertRaises(AssertionError): P.audit_choice(case, None, choice)

    def test_wrong_claimed_classification_is_detected(self):
        case, row, choice = fixture()
        choice['report']['classification'] = 'reference_rejected'
        with self.assertRaises(AssertionError): P.audit_choice(case, row, choice)

    def test_independent_geometry_finds_safe_long_arc_and_unsafe_short_arc(self):
        _, row, _ = fixture(True)
        local, p = row['observation']['local'], P.P.pilot(row)
        local['sun'] = dict(position=dict(x=0, y=0), radius=200, heat_radius=224)
        p['planet']['motion']['position'] = dict(x=320, y=0)
        degrees = math.pi/180
        p['ship']['position'] = dict(x=320+120*math.cos(100*degrees), y=120*math.sin(100*degrees))
        site = p['sites'][0]
        site['normal'] = dict(x=math.cos(-100*degrees), y=math.sin(-100*degrees))
        site['vehicle_position'] = dict(x=320+60*site['normal']['x'], y=60*site['normal']['y'])
        a, b = [P.solar_plan(local, site, side) for side in [1, -1]]
        self.assertLess(a['approach_clearance'], 0)
        self.assertTrue(all(b[k] > 0 for k in ['approach_clearance', 'parked_clearance', 'departure_clearance']))
        self.assertGreater(b['arrival_seconds'], a['arrival_seconds'])


if __name__ == '__main__':
    unittest.main()
