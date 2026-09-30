import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('arrival_preference', Path(__file__).resolve().parents[1] / 'compare-arrival-preference.py')
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)
from test_arrival_local import fixture as local_fixture, pair_fixture


def fixture():
    screen, row = local_fixture()
    arrival = screen['arrival']
    arrival.update(ship=dict(position=dict(x=0,y=190)),sun=None)
    arrival['planet'].update(motion=dict(position=dict(x=0,y=0)),radius=100)
    screen['sites'][0]['projected']['vehicle_position'] = dict(x=0,y=106)
    screen['local_reference']['references'][0]['projected'] = copy.deepcopy(screen['sites'][0]['projected'])
    for d in screen['sites'][0]['directions']: d['solar'] = None
    value = dict(site=dict(planet=2,bearing=33),side=1,direction_order=0,approach_score=0)
    record = dict(site=value['site'],measurement_tick=10,arrival_tick=80,short_angle=0,eligible=[value],unknown=None)
    preference = dict(model='arrival_subset_preference_v1',source_tick=11,assessments=[record],preferred=value,
        unassessed_bearings=[b for b in range(64) if b != 33],charged_graph=1,complete=True,unknown=None,
        conditions='fresh committed v13; no required/rejected/cooldown site or objective; unexposed neutral ground',
        native_choice=None,native_choice_unknown='query readiness; retained subset only',acquisition_seconds=None)
    screen['site_preference'] = preference
    return dict(remote_arrival=screen,source_capture=None),row


class ArrivalPreferenceAudit(unittest.TestCase):
    def test_independent_score_provenance_and_unknowns(self):
        c,row = fixture()
        M.audit_preference(c,row)
        for field in ['site','measurement_tick','arrival_tick','short_angle','eligible','unknown']:
            changed = copy.deepcopy(c)
            changed['remote_arrival']['site_preference']['assessments'][0][field] = 'wrong'
            with self.subTest(field=field),self.assertRaises((AssertionError,TypeError)):
                M.audit_preference(changed,row)

    def test_cannot_claim_global_choice_time_or_complete_coverage(self):
        c,row = fixture()
        for field,value in [('native_choice',dict(site=33)),('acquisition_seconds',1/60),('unassessed_bearings',[]),('charged_graph',0),('preferred',None)]:
            changed = copy.deepcopy(c); changed['remote_arrival']['site_preference'][field] = value
            with self.subTest(field=field),self.assertRaises(AssertionError): M.audit_preference(changed,row)

    def test_no_sun_cannot_consider_the_opposite_direction(self):
        c,row = fixture(); r = c['remote_arrival']['site_preference']
        r['assessments'][0]['eligible'].append(dict(r['preferred'],side=-1,direction_order=1))
        with self.assertRaises(AssertionError): M.audit_preference(c,row)

    def test_partial_record_has_no_fabricated_choice_or_coverage(self):
        c,row = fixture(); r = c['remote_arrival']['site_preference']
        r.update(assessments=[],charged_graph=0,complete=False,preferred=None,unassessed_bearings=list(range(64)))
        M.audit_preference(c,row)
        r['complete'] = True
        with self.assertRaises(AssertionError): M.audit_preference(c,row)

    def test_active_capture_cannot_be_treated_as_fresh_selector(self):
        c,row = fixture(); c['source_capture'] = 'current_approach'
        with self.assertRaises(AssertionError): M.audit_preference(c,row)
        r = c['remote_arrival']['site_preference']
        r.update(assessments=[],charged_graph=0,preferred=None,unknown='source capture already active; fresh selector history unknown',unassessed_bearings=list(range(64)))
        M.audit_preference(c,row)

    def test_pair_removes_only_new_report_and_preserves_prior_reference(self):
        off,on = pair_fixture()
        for report in [off,on]:
            schedule = report['transfer_comparison'][M.KEY]
            if 'arrival_local_reference' in schedule:
                schedule['arrival_site_preference'] = schedule.pop('arrival_local_reference')
            source = schedule['actors'][0]['source']
            for field in ['initial','last_snapshot','published']:
                screen = source[field]['candidates'][0]['remote_arrival']
                # pair_fixture deliberately aliases its snapshots.
                if schedule.get('arrival_site_preference') and 'site_preference' not in screen:
                    screen['site_preference'] = screen.pop('local_reference')
                screen['local_reference'] = dict(unchanged=True)
        self.assertEqual(M.L.audit_pair(off,on,'site_preference','arrival_site_preference')['extra_graph'],1)
        on['transfer_comparison'][M.KEY]['actors'][0]['source']['last_snapshot']['candidates'][0]['remote_arrival']['local_reference'] = dict(unchanged=False)
        with self.assertRaises(AssertionError): M.L.audit_pair(off,on,'site_preference','arrival_site_preference')

    def test_retrospective_requires_complete_source_specific_material_and_native_domain(self):
        c,_ = fixture(); r = c['remote_arrival']['site_preference']; native = r['preferred']
        proof = dict(source_tick=11,seat=0,destination=2,sites=[dict(id=native['site'],measurement_tick=10,
            retrospective=dict(choice=dict(age_and_identity_valid=True)),same_frame_geometry_residual=dict(within_reconstruction_tolerance=True))])
        self.assertEqual(M.retrospective(r,0,2,native,proof,None,1/60)['classification'],'site_and_direction_match')
        for change in ['partial','missing','source','seat','destination','measurement','expired','geometry','domain']:
            r2,p2,domain = copy.deepcopy(r),copy.deepcopy(proof),None
            if change == 'partial': r2['complete'] = False
            elif change == 'missing': p2 = None
            elif change in ['source','seat','destination']: p2[dict(source='source_tick',seat='seat',destination='destination')[change]] += 1
            elif change == 'measurement': p2['sites'][0]['measurement_tick'] += 1
            elif change == 'expired': p2['sites'][0]['retrospective']['choice']['age_and_identity_valid'] = False
            elif change == 'geometry': p2['sites'][0]['same_frame_geometry_residual']['within_reconstruction_tolerance'] = False
            else: domain = 'source exposed'
            result = M.retrospective(r2,0,2,native,p2,domain,1/60)
            with self.subTest(change=change):
                self.assertEqual(result['classification'],'unknown')
                self.assertEqual(result['raw_relation'],'site_and_direction_match')
                self.assertTrue(result['unknown'])

    def test_direction_mismatch_is_not_excused_by_near_alignment(self):
        c,_ = fixture(); r = c['remote_arrival']['site_preference']
        native = dict(r['preferred'],side=-1)
        proof = dict(source_tick=11,seat=0,destination=2,sites=[dict(id=native['site'],measurement_tick=10,
            retrospective=dict(choice=dict(age_and_identity_valid=True)),same_frame_geometry_residual=dict(within_reconstruction_tolerance=True))])
        self.assertEqual(M.retrospective(r,0,2,native,proof,None,1/60)['classification'],'direction_mismatch')


if __name__ == '__main__': unittest.main()
