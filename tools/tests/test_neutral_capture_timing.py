import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('neutral_timing',Path(__file__).resolve().parents[1]/'compare-neutral-capture-timing.py')
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)
SPEC = importlib.util.spec_from_file_location('landing_fixture',Path(__file__).with_name('test_landing_choices.py'))
L = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(L)


def fixture(sun=False):
    _,row,choice = L.fixture(sun)
    o,m,p = row['observation'],row['mission'],P.P.pilot(row)
    p.update(version=1,vehicle=0,spaceling=0,controls_armed=True,queries_ready=True,ship_available=True,
             ship_form='ship',ship_health=100,location=dict(aboard=0),gravity=dict(x=0,y=-5))
    p['ship'].update(velocity=dict(x=0,y=0),angle=0,spin=0)
    p['planet']['motion']['angle'] = 0
    p['planet']['claim'].update(planet=1,owner=None,claimant=None,phase='idle',progress=0,stage_required_seconds=3,
                               flag_interaction_range=3,captures=0)
    for site in p['sites']:
        site.update(local_position=dict(x=0,y=60),position=dict(x=0,y=60),velocity=dict(x=0,y=0),
                    hatch_position=dict(x=0,y=62),boarding_hatches=[None,dict(x=0,y=62)])
    flight = o['local']['combat']['recovery']['flight']
    flight.update(version=2,flight=dict(version=1))
    o['local']['combat']['recovery']['version'] = 1
    o['local']['combat']['version'] = 2
    o['local']['version'] = 1
    o.update(version=1,planets=[copy.deepcopy(p['planet'])])
    m.update(policy='material_mission_v13',completed_sorties=0,events=[dict(kind='selected',planet=1,tick=3)])
    c = m['capture']
    c.update(policy='tactical_sortie_v11',failure=None,completed_tick=None,landing=dict(invalidations=0,
        landing_retries=0,landed_tick=None,claimed_tick=None,boarded_tick=None))
    c.update(dict.fromkeys(P.R.RETRIES,0))
    r = dict(model='neutral_capture_timing_v1',policy=m['policy'],actor=p['owner'],vehicle=0,spaceling=0,
             source_tick=11,visit_tick=3,started_tick=11,completed_sorties=0,attempt_counters=[0]*8,
             planet=copy.deepcopy(p['planet']),ship=copy.deepcopy(p['ship']),gravity=p['gravity'].copy(),
             site=copy.deepcopy(p['sites'][0]),selected=copy.deepcopy(choice['report']['selected']),
             opponent=None,opponent_distance=None,source_exposed=False,cover=None,
             phases=P.PHASE_COSTS.copy(),total_seconds=sum(P.PHASE_COSTS.values()),unknown=None)
    return row,choice,dict(report=r,unknown=None)


class NeutralTimingAudit(unittest.TestCase):
    def test_selected_solar_requires_arrival_parking_direction_and_raw_witness(self):
        for field in ['arrival_seconds','surface_seconds','departure_side']:
            row,choice,_ = fixture(True)
            P.audit_selected(row,choice)
            a = choice['report']['selected']
            if field == 'departure_side':
                # This fixture has a safely separated best departure corridor.
                expected = P.Q.solar_plan(row['observation']['local'],P.P.pilot(row)['sites'][0],a['side'])
                self.assertGreater(abs(expected['departure_corridors'][0]-expected['departure_corridors'][1]),.02)
                a['solar'][field] *= -1
            else: a['solar'][field] += 100
            choice['report']['assessments'][0] = copy.deepcopy(a)
            row['mission']['capture']['solar'] = copy.deepcopy(a['solar'])
            with self.assertRaises(AssertionError): P.audit_selected(row,choice)
        row,choice,_ = fixture()
        row['mission']['capture']['solar'] = dict(forecast_tick=11)
        with self.assertRaises(AssertionError): P.audit_selected(row,choice)

    def test_overflowing_distance_remains_unknown_instead_of_crashing(self):
        row,choice,t = fixture()
        target = dict(ground_occluded=False,motion=copy.deepcopy(P.P.pilot(row)['ship']))
        target['motion']['position']['x'] = 2e38
        row['observation']['local']['combat']['target'] = target
        t['report'].update(opponent=copy.deepcopy(target),opponent_distance=None,
                          unknown='opponent geometry unavailable',phases=None,total_seconds=None)
        self.assertEqual(P.audit_timing(row,choice,t)['unknown'],'opponent geometry unavailable')

    def test_native_no_choice_binds_endpoint_clock_and_schema(self):
        row,_,_ = fixture()
        row['tick'] = 3600
        row['mission']['capture'] = None
        cap = dict(schema=1,horizon_ticks=7200,source=None,outcome=None,observed_rows=0,
                   last_observed_tick=None,touchdown=None,tactical_completed_tick=None,milestones=dict.fromkeys(P.R.NAMES))
        report = dict(elapsed_ticks=3600,round=dict(outcome=None),native_capture_probe=dict(schema=1,seat=0,
            choice_window_ticks=3600,landing_choice=None,capture_followthrough=cap,
            no_choice=dict(tick=3600,observed=True,reason='no_native_choice')))
        self.assertEqual(P.audit_trip(report,[row],True)['outcome'],'no_native_choice')
        for field in ['tick','observed','schema','seat','window','last','elapsed']:
            changed,r = copy.deepcopy(report),copy.deepcopy(row)
            probe = changed['native_capture_probe']
            if field == 'tick': probe['no_choice']['tick'] += 1
            elif field == 'observed': probe['no_choice']['observed'] = False
            elif field in ['schema','seat']: probe[field] += 1
            elif field == 'window': probe['choice_window_ticks'] += 1
            elif field == 'last': r['tick'] += 1
            else: changed['elapsed_ticks'] += 1
            with self.assertRaises(AssertionError): P.audit_trip(changed,[r],True)
        report['native_capture_probe']['no_choice'] = dict(tick=3599,observed=False,reason='match_finished')
        report['round']['outcome'] = dict(winner=1)
        report['elapsed_ticks'] = 3599
        row['tick'] = 3598
        self.assertEqual(P.audit_trip(report,[row],True)['outcome'],'match_finished')

    def test_witnessed_native_choice_cannot_lose_its_source(self):
        row,choice,_ = fixture()
        report = dict(native_capture_probe=dict(schema=1,seat=0,choice_window_ticks=3600,
            landing_choice=choice,no_choice=None,capture_followthrough=dict(schema=1,horizon_ticks=7200,source=None)))
        with self.assertRaisesRegex(AssertionError,'retain a capture source'): P.audit_trip(report,[row],True)

    def test_unsupported_source_keeps_zero_elapsed_and_no_physical_milestones(self):
        row,choice,_ = fixture()
        choice.update(reference=row['mission']['capture']['site'],report=None,unknown='native choice unavailable')
        cap = dict(schema=1,horizon_ticks=7200,source=dict(pilot=copy.deepcopy(P.P.pilot(row)),
            mission=copy.deepcopy(row['mission']),selected=None,choice_unknown=choice['unknown'],physics_queries=0,
            reference=None,reference_unknown='unknown',neutral_timing=dict(report=None,unknown=choice['unknown'])),
            observed_rows=1,last_observed_tick=11,touchdown=None,tactical_completed_tick=None,
            milestones=dict.fromkeys(P.R.NAMES)|dict(choice=11),
            outcome=dict(reason='unsupported_source',tick=11,elapsed_ticks=0,controller_observed=True))
        report = dict(elapsed_ticks=11,native_capture_probe=dict(schema=1,seat=0,choice_window_ticks=3600,
            no_choice=None,landing_choice=choice,capture_followthrough=cap))
        self.assertEqual(P.audit_trip(report,[row],True)['outcome'],'unsupported_source')
        for field in ['elapsed_ticks','report_tick','touchdown','tactical_completed_tick']:
            changed = copy.deepcopy(report)
            c = changed['native_capture_probe']['capture_followthrough']
            if field == 'elapsed_ticks': c['outcome'][field] = 1
            elif field == 'report_tick': changed['elapsed_ticks'] = 12
            else: c[field] = 11
            with self.assertRaises(AssertionError): P.audit_trip(changed,[row],True)

    def test_missing_or_false_cover_is_separate_from_supported_timing(self):
        row,choice,t = fixture()
        self.assertEqual(P.audit_timing(row,choice,t)['cover'],'missing')
        cover = dict(site=t['report']['site']['id'],grounded=False,approach=False,departure=False)
        row['observation']['local']['cover'] = [cover]
        t['report']['cover'] = copy.deepcopy(cover)
        self.assertEqual(P.audit_timing(row,choice,t)['cover'],'none')
        self.assertIsNotNone(P.audit_timing(row,choice,t)['prediction'])

    def test_wrong_actor_clock_visit_material_and_cost_are_detected(self):
        for mutation in ['actor','source_tick','visit_tick','revision','cost','hatch']:
            row,choice,t = fixture()
            r = t['report']
            if mutation == 'actor': r['actor'] = 'player_2'
            elif mutation in ['source_tick','visit_tick']: r[mutation] += 1
            elif mutation == 'revision': r['planet']['revision'] += 1
            elif mutation == 'hatch': r['site']['boarding_hatches'] = [None,None]
            else: r['phases']['landing'] += 1
            with self.assertRaises(AssertionError): P.audit_timing(row,choice,t)

    def test_numeric_estimate_cannot_ignore_versions_progress_or_missing_support(self):
        for mutation in ['version','progress','hatch','claim']:
            row,choice,t = fixture()
            p = P.P.pilot(row)
            if mutation == 'version': row['observation']['local']['combat']['version'] = 99
            elif mutation == 'progress': row['mission']['capture']['landing']['landed_tick'] = 11
            elif mutation == 'claim':
                p['planet']['claim']['claimant'] = 'player_2'
                row['observation']['planets'][0] = copy.deepcopy(p['planet'])
                t['report']['planet'] = copy.deepcopy(p['planet'])
            else:
                p['sites'][0]['boarding_hatches'] = [None,None]
                t['report']['site'] = copy.deepcopy(p['sites'][0])
            with self.assertRaises(AssertionError): P.audit_timing(row,choice,t)

    def test_exposed_choice_is_explicit_unknown_and_never_priced(self):
        row,choice,t = fixture()
        p = P.P.pilot(row)
        target = dict(ground_occluded=False,motion=copy.deepcopy(p['ship']))
        target['motion']['position']['x'] += 100
        row['observation']['local']['combat']['target'] = target
        choice['report']['exposed'] = True
        t['report'].update(opponent=copy.deepcopy(target),opponent_distance=100,source_exposed=True)
        with self.assertRaises(AssertionError): P.audit_timing(row,choice,t)
        t['report'].update(phases=None,total_seconds=None,unknown='source exposed to opponent')
        self.assertEqual(P.audit_timing(row,choice,t)['unknown'],'source exposed to opponent')

    def test_first_choice_is_not_filtered_by_admission_or_after_window(self):
        row,_,_ = fixture()
        later = copy.deepcopy(row)
        row['mission']['policy'] = 'unsupported'
        row['tick'] = row['mission']['capture']['acquisition']['tick'] = 3599
        later['tick'] = later['mission']['capture']['acquisition']['tick'] = 3600
        self.assertIs(P.first_choice([row,later]),row)
        self.assertIsNone(P.first_choice([later]))
        row['mission']['capture']['acquisition']['tick'] -= 1
        self.assertIsNone(P.first_choice([row,later]))

    def test_report_parity_removes_only_new_timing_and_wall_time(self):
        a = dict(missions=[dict(goal='capture')],policy=dict(mean_ms=1,count=2),
                 native_capture_probe=dict(capture_followthrough=dict(source=dict(reference=3))))
        b = copy.deepcopy(a)
        b['policy']['mean_ms'] = 5
        b['native_capture_probe']['capture_followthrough']['source']['neutral_timing'] = dict(phases=4)
        self.assertEqual(P.clean_report(a),P.clean_report(b))
        b['missions'][0]['goal'] = 'hunt'
        self.assertNotEqual(P.clean_report(a),P.clean_report(b))

    def test_predetermined_worlds_cover_both_seats_and_pressure_without_outcome_selection(self):
        plan = P.fresh_plan()
        self.assertEqual(len(plan),16)
        self.assertEqual(len({r['seed'] for r in plan}),4)
        for seed in {r['seed'] for r in plan}:
            self.assertEqual({(r['seat'],r['interval']) for r in plan if r['seed']==seed},{(0,0),(0,3),(1,0),(1,3)})


if __name__ == '__main__': unittest.main()
