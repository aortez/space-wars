import copy
import gzip
import importlib.util
import json
import math
from pathlib import Path
import tempfile
import unittest


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


A = module('arrival_replay', Path(__file__).parents[1]/'compare-arrival-replays.py')
S = module('arrival_fixture', Path(__file__).with_name('test_remote_arrival_screen.py'))
R = module('retention_fixture', Path(__file__).with_name('test_remote_retention.py'))
C = module('transfer_fixture', Path(__file__).with_name('test_transfer_calibration.py'))


def fixture(handoff=14, choice=15, predicted=16):
    screen,_,_,_,_ = S.fixture()
    planet = screen['source_planet']
    planet['claim'].update(planet=0,stage_required_seconds=3.,flag_interaction_range=4.,captures=0,neutralizations=0)
    screen['arrival']['planet']['claim'] = copy.deepcopy(planet['claim'])
    screen['arrival']['tick'] = predicted
    screen['sites'][0]['arrival_age_ticks'] = predicted-10
    sun = dict(position=dict(x=1000.,y=1000.),radius=10.,heat_radius=20.)
    screen['arrival']['sun'] = sun
    measure = screen['sites'][0]['source']['measurement']
    control = []
    for tick in range(11,max(predicted,choice)+2):
        current = copy.deepcopy(planet)
        current['motion'] = dict(position=dict(x=100.+2*tick,y=0.),velocity=dict(x=1.,y=2.),
                                 angle=math.pi/2,spin=.5)
        ship = dict(position=dict(x=120.+2*tick,y=100.),velocity=dict(x=3.,y=4.),angle=2.,spin=.6)
        site = A.project(measure,current['motion'])
        other = copy.deepcopy(site)
        other['id'] = dict(planet=0,bearing=1)
        pilot = dict(tick=tick,owner='player_1',vehicle=0,spaceling=1,ship_available=True,ship_form='ship',
                     ship_health=100,ship=ship,planet=current,sites=[site,other])
        local = dict(sun=sun,planet_orbit_omega=None,combat=dict(recovery=dict(flight=dict(pilot=pilot))))
        capture = None if tick < choice else dict(started_tick=handoff,site=other['id'],
            acquisition=dict(tick=tick,planet=0,selected_site=other['id']))
        control.append(dict(seat=0,tick=tick,observation=dict(planets=[current],local=local),mission=dict(capture=capture)))
    choice_row = next(r for r in control if r['tick'] == choice)
    assessments = [dict(site=s['id'],side=d,rejection=None,total_score=7. if s['id']['bearing']==0 else 1.,
                       solar=A.Q.solar_plan(choice_row['observation']['local'],s,d))
                   for s in A.P.pilot(choice_row)['sites'] for d in A.SIDES]
    native = dict(observation_tick=choice,report=dict(tick=choice,selected=assessments[2],assessments=assessments))
    predicted_local = copy.deepcopy(choice_row['observation']['local'])
    predicted_local['combat']['recovery']['flight']['pilot'].update(
        tick=predicted,planet=screen['arrival']['planet'],ship=screen['arrival']['ship'])
    for direction in screen['sites'][0]['directions']:
        direction['solar'] = A.Q.solar_plan(predicted_local,screen['sites'][0]['projected'],direction['side'])
    return dict(seat=0,source_tick=11,destination=0),screen,control,handoff,choice,native


class ArrivalReplayTests(unittest.TestCase):
    def test_native_retained_tie_uses_native_order_not_remote_request_order(self):
        args = fixture()
        screen,native = args[1],args[-1]['report']
        second = copy.deepcopy(screen['sites'][0])
        second['source']['id'] = second['source']['measurement']['site']['id'] = second['projected']['id'] = dict(planet=0,bearing=1)
        screen['sites'] = [second,screen['sites'][0]]
        for a in native['assessments']: a['total_score'] = 1.
        result = A.compare_screen(*args)
        self.assertEqual(result['native_best_retained'],native['assessments'][0])

    def test_three_clocks_and_source_evidence_remain_separate(self):
        args = fixture()
        original = copy.deepcopy(args)
        result = A.compare_screen(*args)
        self.assertEqual(args,original)
        self.assertEqual(result['actual_minus_predicted_transfer_ticks'],-2)
        self.assertEqual(result['actual_acquisition_ticks'],1)
        self.assertEqual(result['different_time_endpoint_differences']['planet']['position'],28.)
        self.assertEqual(result['same_predicted_tick_planet']['motion_difference']['position'],32.)
        site = result['sites'][0]
        self.assertEqual((site['measurement_tick'],site['source_age_ticks'],site['predicted_age_ticks']),(10,1,6))
        self.assertEqual(site['retrospective']['handoff']['age_ticks'],4)
        self.assertEqual(site['retrospective']['choice']['age_ticks'],5)
        self.assertEqual(site['same_frame_geometry_residual']['maximum'],0.)
        self.assertTrue(result['native_choice_outside_retained_set'])
        self.assertEqual(result['retained_score_above_native_choice'],6.)

    def test_wrong_source_seat_clock_and_incomplete_directions_are_rejected(self):
        for change in ['source','seat','gap','native_clock','age','frozen_frame','missing_direction','duplicate_direction']:
            with self.subTest(change=change):
                args = fixture()
                case,screen,control,_,_,native = args
                if change == 'source': screen['source_tick'] += 1
                elif change == 'seat': case['seat'] = 1
                elif change == 'gap': control.pop(1)
                elif change == 'native_clock': native['observation_tick'] += 1
                elif change == 'age': screen['sites'][0]['arrival_age_ticks'] -= 1
                elif change == 'frozen_frame': screen['sites'][0]['projected'] = copy.deepcopy(A.P.pilot(control[-1])['sites'][0])
                elif change == 'missing_direction': screen['sites'][0]['directions'].pop()
                else: native['report']['assessments'].append(copy.deepcopy(native['report']['assessments'][0]))
                with self.assertRaises(AssertionError): A.compare_screen(*args)

    def test_old_measurement_cannot_be_refreshed_without_original_dispatch(self):
        source,rows,history,seed = R.fixture()
        history = copy.deepcopy(history)
        source['source_tick'] = source['retained_source']['tick'] = 12
        rows[0,12] = copy.deepcopy(rows[0,11])
        group = source['retained_source']['groups'][0]
        group['observed_tick'] = 12
        group['survey']['candidates'][0]['measurement']['tick'] = 11
        with self.assertRaisesRegex(AssertionError,'original dispatch'):
            A.R.audit_memory(source,rows,history,seed)

    def test_transient_identity_changes_cannot_be_hidden_by_matching_endpoints(self):
        for change in ['owner','revision','captures','vehicle','pilot_owner','death']:
            with self.subTest(change=change):
                args = fixture()
                row = args[2][1]
                planet,pilot = row['observation']['planets'][0],A.P.pilot(row)
                if change == 'owner': planet['claim']['owner'] = 'player_2'
                elif change == 'revision': planet['revision'] += 1
                elif change == 'captures': planet['claim']['captures'] += 1
                elif change == 'vehicle': pilot['vehicle'] += 1
                elif change == 'pilot_owner': pilot['owner'] = 'player_2'
                else: row['observation']['match_context'] = dict(finished=False,pilots_alive=[False,True])
                result = A.compare_screen(*args)
                key = 'first_planet_change' if change in ['owner','revision','captures'] else 'first_actor_change'
                self.assertEqual(result['identity'][key],12)
                site = result['sites'][0]
                self.assertFalse(site['retrospective']['choice']['age_and_identity_valid'])
                self.assertEqual(site['same_frame_geometry_residual']['maximum'],0.)

    def test_actual_arrival_age_can_expire_a_valid_frozen_screen(self):
        result = A.compare_screen(*fixture(handoff=1810,choice=1811,predicted=1800))
        site = result['sites'][0]
        self.assertTrue(site['retrospective']['handoff']['age_and_identity_valid'])
        self.assertFalse(site['retrospective']['choice']['age_and_identity_valid'])
        self.assertEqual(site['retrospective']['choice']['age_ticks'],1801)
        self.assertEqual(site['same_frame_geometry_residual']['maximum'],0.)

    def test_missing_same_id_site_stays_unobserved(self):
        args = fixture()
        choice = next(r for r in args[2] if r['tick'] == args[4])
        A.P.pilot(choice)['sites'].pop(0)
        args[-1]['report']['assessments'] = args[-1]['report']['assessments'][2:]
        result = A.compare_screen(*args)
        site = result['sites'][0]
        self.assertEqual(site['native_site_status'],'absent_from_fresh_survey')
        self.assertIsNone(site['same_frame_geometry_residual'])
        self.assertIsNone(result['native_best_retained'])
        self.assertEqual([d['native_sign'] for d in site['direction_comparisons']],['not_observed']*2)

    def test_actual_frame_changes_are_measurements_not_silent_prediction_repairs(self):
        args = fixture()
        choice = next(r for r in args[2] if r['tick'] == args[4])
        native_site = A.P.pilot(choice)['sites'][0]
        native_site['position']['x'] += 3.
        native_site['boarding_hatches'][0] = None
        result = A.compare_screen(*args)
        residual = result['sites'][0]['same_frame_geometry_residual']
        self.assertEqual(residual['fields']['position'],3.)
        self.assertFalse(residual['hatch_availability_matches'])
        self.assertFalse(residual['within_reconstruction_tolerance'])

    def test_borderline_solar_sign_is_not_promoted_to_clear(self):
        plan = dict(approach_clearance=1.,parked_clearance=.005,departure_clearance=10.)
        self.assertEqual(A.solar_sign(plan),'unresolved')
        plan['parked_clearance'] = -.005
        self.assertEqual(A.solar_sign(plan),'unresolved')
        plan['parked_clearance'] = -.02
        self.assertEqual(A.solar_sign(plan),'unsafe')

    def test_capture_projection_derives_visit_and_selected_site_from_full_source(self):
        site = dict(id=dict(planet=2,bearing=31),position=dict(x=1.,y=2.))
        pilot = dict(tick=12,owner='player_1',vehicle=0,spaceling=1,location={'aboard':0},ship={},
                     ship_form='ship',ship_health=80.,transfers=0,planet={},sites=[site],queries_ready=True)
        mission = dict(policy='v13',goal='capture',target=2,replans=0,completed_sorties=0,capture={},
            events=[dict(kind='selected',planet=2,tick=5),dict(kind='selected',planet=1,tick=8),
                    dict(kind='arrived',planet=2,tick=11)],goal_since=11)
        raw = dict(source=dict(pilot=pilot,mission=mission,selected=dict(site=site['id'])),milestones=dict(choice=12))
        original = copy.deepcopy(raw)
        projected = A.capture_projection(raw)
        self.assertEqual(raw,original)
        self.assertEqual(projected['source']['mission']['visit_tick'],5)
        self.assertEqual(projected['source']['selected_site'],site)
        self.assertEqual(projected['source']['pilot']['ship_health'],80.)
        self.assertEqual(projected['milestones'],dict(choice=12))
        self.assertNotIn('queries_ready',projected['source']['pilot'])
        raw['source']['selected']['site'] = dict(planet=2,bearing=33)
        with self.assertRaises(StopIteration): A.capture_projection(raw)

    def test_followthrough_controls_are_bounded_at_transfer_without_hiding_boundary_corruption(self):
        with tempfile.TemporaryDirectory() as tmp:
            case,_,root = C.pair_fixture(Path(tmp))
            trace = A.T.rows(root/'transfer-probe.jsonl')
            rows = list(A.C.read_trace(root))
            suffix = copy.deepcopy(rows[-2:])
            for row in suffix:
                row['tick'] = A.P.pilot(row)['tick'] = 12
                row['actions'] = ['ordinary capture after handoff']
            rows += suffix
            def save():
                with gzip.open(root/'trace.jsonl.gz','wt') as stream:
                    stream.writelines(json.dumps(r)+'\n' for r in rows)
            save()
            with self.assertRaises(AssertionError): A.C.audit_controls(case,root,trace)
            self.assertEqual(A.C.audit_controls(case,root,trace,end_tick=11),2)
            rows[2]['actions'] = ['wrong boundary control']
            save()
            with self.assertRaises(AssertionError): A.C.audit_controls(case,root,trace,end_tick=11)


if __name__ == '__main__': unittest.main()
