import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('composition', Path(__file__).resolve().parents[1] / 'compose-capture-references.py')
C = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(C)
FIXTURE = importlib.util.spec_from_file_location('comparison_fixture', Path(__file__).with_name('test_transfer_comparison.py'))
F = importlib.util.module_from_spec(FIXTURE)
FIXTURE.loader.exec_module(F)


def fixture():
    r = F.fixture()[0]['sources'][0]['published']
    r['capture_costs'] = []
    for c in r['candidates']:
        site = dict(planet=c['destination'], bearing=0)
        e = dict(site=site, revision=1, observed_owner=None, radius=50, stage_seconds=3,
            flag_range=3, remote=True, tick=4, age_ticks=6, gravity=0,
            route_source_tick=None, route_validated_tick=None, route_objective=None, choice=None)
        full = dict(zip(C.PHASES, [10, 1, 2, 3, 4, 5]))
        c.update(source_capture=None, local_reference=dict(model='source_local_reference_v1', source_tick=10,
            destination=c['destination'], visit_tick=5, selected_site=None, observed_choice_tick=None,
            evidence=e, full=full, remaining=copy.deepcopy(full), elapsed_landing_ticks=0, unknown=None))
        travel = c['forecast']['handoff_seconds'] if c['forecast'] else None
        r['capture_costs'].append(dict(destination=c['destination'], travel_seconds=travel,
            local_seconds=25, known_components_seconds=25+travel if travel else None,
            remaining_trip_seconds=None, unknown='unmeasured'))
    return r


def control(tick):
    planet = dict(index=1, revision=1, radius=50, claim=dict(owner=None, flag=None,
        stage_required_seconds=3, flag_interaction_range=3), motion=dict(angle=0, position=dict(x=0,y=0)))
    p = dict(tick=tick, planet=copy.deepcopy(planet))
    capture = dict(site=dict(planet=1, bearing=0), goal='approach', failure=None, landing=dict(landed_tick=None))
    local = dict(objective_gravity=5, combat=dict(recovery=dict(flight=dict(pilot=p))))
    mission = dict(target=1, goal='capture', capture=capture, events=[dict(kind='selected', planet=1, tick=1)])
    return dict(seat=0, tick=tick, observation=dict(local=local, planets=[planet]), mission=mission)


class CaptureCompositionAudit(unittest.TestCase):
    def test_partial_forecast_keeps_phase_prefix_and_cannot_hide_terminal_tick(self):
        old = F.fixture()[0]
        old['sources'][0]['environment'] = dict(tick=10)
        f = old['sources'][0]['last_snapshot']['candidates'][1]['forecast']
        f['phases'] = [dict(start_tick=0, end_tick=2, goal='transfer')]
        new = copy.deepcopy(old)
        partial = new['sources'][0]['last_snapshot']['candidates'][1]['forecast']
        partial.update(ticks=1, charged_graph=1, end=None, handoff_seconds=None)
        partial['phases'][0]['end_tick'] = 1
        self.assertEqual(C.forecast_parity(old, new)['partial_exact_prefix'], 1)
        partial['phases'][0]['goal'] = 'launch'
        with self.assertRaises(AssertionError): C.forecast_parity(old, new)
        partial['phases'][0].update(goal='transfer', end_tick=2)
        partial.update(ticks=2, charged_graph=2)
        with self.assertRaises(AssertionError): C.forecast_parity(old, new)

    def test_stripping_route_provenance_cannot_hide_arbitrary_flagged_costs(self):
        r = fixture()
        r['candidates'] = r['candidates'][:1]
        c = r['candidates'][0]
        c['local_reference']['remaining'] = dict.fromkeys(C.PHASES, 123)
        c['local_reference']['full'] = dict(c['local_reference']['remaining'])
        e = c['local_reference']['evidence']
        e.update(tick=10, age_ticks=0, remote=False, observed_owner='player_2', gravity=5, flag_range=2.8)
        row = control(10)
        planet = row['observation']['planets'][0]
        planet['claim'].update(owner='player_2', flag=dict(player='player_2', position=dict(x=0,y=50)))
        local = row['observation']['local']
        p = local['combat']['recovery']['flight']['pilot']
        p['planet'] = copy.deepcopy(planet)
        p['sites'] = [dict(id=e['site'], boarding_hatches=[dict(x=0,y=50), None])]
        local['cover'] = [dict(site=e['site'], grounded=True, approach=True, departure=True)]
        source = dict(initial=r, seat=0, source_tick=10,
            final_state=dict(cancelled_tick=11, reason='source expired'))
        with self.assertRaisesRegex(AssertionError, 'route provenance'):
            C.audit_evidence(dict(sources=[source]), [row], [])

    def test_walking_calibration_requires_the_measured_complete_supported_route(self):
        leg = dict(failure=None, partial=False, start_node=0, reachable_nodes=2, destination_nodes=1,
            jumps=0, flights=0, length=20)
        site = dict(planet=1, bearing=0)
        survey = dict(sites=[dict(site=site, outbound=leg, returning=copy.deepcopy(leg))])
        result = C.walking_reference(survey, site, 3)
        self.assertAlmostEqual(result['outbound'], 4.15833333)
        self.assertAlmostEqual(result['return_board'], 4+2/60)
        for change in ['missing', 'partial', 'return', 'jump', 'flight', 'long', 'crossing']:
            s = copy.deepcopy(survey)
            if change == 'missing': s['sites'] = []
            elif change == 'partial': s['sites'][0]['outbound']['partial'] = True
            elif change == 'return': s['sites'][0]['returning'] = None
            elif change == 'jump': s['sites'][0]['outbound']['jumps'] = 1
            elif change == 'flight': s['sites'][0]['returning']['flights'] = 1
            elif change == 'long': s['sites'][0]['outbound']['length'] = 54
            else: s['sites'][0]['crossing'] = dict(valid=True)
            with self.subTest(change=change), self.assertRaises(AssertionError): C.walking_reference(s, site, 3)

    def test_choice_clock_is_derived_from_full_history_and_restarts_on_changed_context(self):
        normal = [control(t) for t in range(5, 10)]
        self.assertEqual(C.choice_clock(normal, 0, 9), 5)
        for change in ['gap', 'site', 'visit', 'material', 'gravity']:
            rows = copy.deepcopy(normal)
            if change == 'gap': rows.pop(2)
            elif change == 'site': rows[2]['mission']['capture']['site']['bearing'] = 1
            elif change == 'visit':
                for r in rows[2:]: r['mission']['events'][0]['tick'] = 7
            elif change == 'material':
                for r in rows[2:]: r['observation']['local']['combat']['recovery']['flight']['pilot']['planet']['revision'] = 2
            else:
                rows[1]['observation']['local']['objective_gravity'] = 5.009
                for r in rows[2:]: r['observation']['local']['objective_gravity'] = 5.018
            self.assertEqual(C.choice_clock(rows, 0, 9), 8 if change in ['gap', 'site'] else 7)

    def test_pending_and_ready_local_cancellation_has_an_independent_physical_cause(self):
        for ready in [False, True]:
            r = fixture()
            r['candidates'] = r['candidates'][:1]
            e = r['candidates'][0]['local_reference']['evidence']
            e['tick'] = 0
            source = dict(initial=r, seat=0, source_tick=1800,
                final_state=dict(cancelled_tick=1801, completed_tick=1800 if ready else None,
                    reason='local reference dependencies changed or expired'))
            rows = {(0, t): control(t) for t in range(1800, 1803)}
            C.audit_lifetime(source, rows)
            for cancelled in [1800, 1802]:
                bad = copy.deepcopy(source)
                bad['final_state']['cancelled_tick'] = cancelled
                with self.assertRaises(AssertionError): C.audit_lifetime(bad, rows)
            e.update(tick=1800, remote=False, gravity=5)
            rows[0,1800]['observation']['local']['objective_gravity'] = 5.009
            rows[0,1801]['observation']['local']['objective_gravity'] = 5.018
            C.audit_lifetime(source, rows)
            e['gravity'] = 5.009
            with self.assertRaises(AssertionError): C.audit_lifetime(source, rows)

    def test_unselected_site_never_becomes_whole_trip_and_explicit_current_choice_can(self):
        r = fixture()
        C.audit_composition(r)
        c, cost = r['candidates'][0], r['capture_costs'][0]
        c['source_capture'] = 'current_approach'
        local = c['local_reference']
        local.update(selected_site=local['evidence']['site'], observed_choice_tick=5, elapsed_landing_ticks=5)
        local['remaining']['landing'] -= 5/60
        value = sum(local['remaining'].values())
        cost.update(travel_seconds=0, local_seconds=value, known_components_seconds=value, remaining_trip_seconds=value, unknown=None)
        C.audit_composition(r)
        r['candidates'][0]['local_reference']['observed_choice_tick'] = None
        with self.assertRaises(AssertionError): C.audit_composition(r)

    def test_wrong_source_age_route_clock_arithmetic_and_missing_gap_are_rejected(self):
        for change in ['source', 'age', 'future', 'route', 'elapsed', 'phase', 'sum', 'whole', 'partial', 'entry']:
            r = fixture()
            c, total = r['candidates'][1], r['capture_costs'][1]
            local = c['local_reference']
            if change == 'source': local['source_tick'] += 1
            elif change == 'age': local['evidence']['age_ticks'] = 0
            elif change == 'future': local['evidence'].update(tick=11, age_ticks=-1)
            elif change == 'route': local['evidence'].update(route_objective=dict(planet=c['destination'], revision=1, owner=None, range=3), route_source_tick=1, route_validated_tick=11)
            elif change == 'elapsed': local['elapsed_landing_ticks'] = 1
            elif change == 'phase': local['remaining']['landing'] = -1
            elif change == 'sum': total['known_components_seconds'] += 1
            elif change == 'whole': total.update(remaining_trip_seconds=total['known_components_seconds'], unknown=None)
            elif change == 'partial': r['ranked'] = False
            else: c['source_capture'] = 'nominated_approach'
            with self.subTest(change=change), self.assertRaises(AssertionError): C.audit_composition(r)


if __name__ == '__main__': unittest.main()
