import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('arrival_local', Path(__file__).resolve().parents[1] / 'compare-arrival-local.py')
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


def fixture():
    planet = dict(index=2,claim=dict(planet=2,owner=None,flag=None,claimant=None,phase='idle',progress=0,
                                   stage_required_seconds=3,flag_interaction_range=2.8))
    pilot = dict(controls_armed=True,queries_ready=True,ship_available=True,ship_form='ship',ship_health=100,
                 vehicle=0,location=dict(aboard=0))
    row = dict(tick=11,mission=dict(policy='material_mission_v13'),
               observation=dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=pilot))))))
    site = dict(id=dict(planet=2,bearing=33),revision=4)
    report = dict(model='arrival_local_reference_v1',calibration='neutral_successful_trip_medians_v1',
                  policy='material_mission_v13',source_tick=11,charged_graph=1,complete=True,unknown=None,
                  acquisition_seconds=None,remaining_trip_seconds=None,phase_origin='unknown; no elapsed-time subtraction',
                  conditions='native selects this site; unexposed',solar_scope='not a solar safety certificate',future_threat='unknown',
                  references=[dict(site=site['id'],measurement_tick=10,arrival_tick=80,projected=copy.deepcopy(site),
                                   eligible_sides=[-1,1],phases=copy.deepcopy(M.N.PHASE_COSTS),
                                   conditional_seconds=sum(M.N.PHASE_COSTS.values()),unknown=None)])
    screen = dict(source_tick=11,unknown=None,complete=True,arrival=dict(tick=80,planet=planet),local_reference=report,
                  sites=[dict(source=dict(id=site['id'],measurement=dict(tick=10)),projected=site,unknown=None,
                              directions=[dict(side=s,solar_clear=True) for s in [-1,1]])])
    return screen,row


def pair_fixture():
    screen,_ = fixture()
    on = dict(charged_graph=80,candidates=[dict(remote_arrival=screen,forecast=dict(ticks=77),local_reference=dict(site=31))])
    off = M.without_reference(on)
    reports = []
    for mode,report,tick in [('off',off,40),('on',on,41)]:
        published = dict(phase='ready',charged_graph=report['charged_graph'],completed_tick=tick,validated_tick=tick)
        final = dict(published,phase='stale',validated_tick=None)
        source = dict(source_tick=11,initial=report,last_snapshot=report,published=report,published_state=published,final_state=final)
        schedule = dict(charged_graph=report['charged_graph'],actors=[dict(seat=0,source=source)])
        if mode == 'on': schedule['arrival_local_reference'] = True
        reports.append(dict(transfer_comparison={M.KEY:schedule}))
    return reports


class ArrivalLocalAudit(unittest.TestCase):
    def test_same_site_reference_with_explicit_unknown_terms(self):
        screen,row = fixture()
        self.assertEqual(M.audit_reference(screen,row),dict(unknown=None,references=1,numeric=1))

    def test_rejects_wrong_site_epoch_projection_phase_clock_or_leaked_total(self):
        for field in ['site','measurement_tick','arrival_tick','projected','phases','conditional_seconds',
                      'acquisition_seconds','remaining_trip_seconds','eligible_sides','charged_graph']:
            screen,row = fixture()
            r = screen['local_reference']; ref = r['references'][0]
            if field == 'site': ref['site'] = dict(planet=2,bearing=31)
            elif field == 'projected': ref['projected']['revision'] += 1
            elif field == 'phases': ref[field]['landing'] = 999
            elif field in ['measurement_tick','arrival_tick','conditional_seconds']: ref[field] += 1
            elif field == 'eligible_sides': ref[field] = [1]
            else: r[field] = 9
            with self.subTest(field=field),self.assertRaises(AssertionError): M.audit_reference(screen,row)

    def test_claim_domain_mutations_cannot_keep_a_positive_reference(self):
        for key,value in [('planet',3),('owner','player_2'),('flag',{}),('claimant','player_2'),('phase','raising'),
                          ('progress',.1),('progress',float('nan')),('stage_required_seconds',4),
                          ('stage_required_seconds',float('inf')),('flag_interaction_range',0),('flag_interaction_range',float('nan'))]:
            screen,row = fixture(); screen['arrival']['planet']['claim'][key] = value
            with self.subTest(key=key,value=value),self.assertRaises(AssertionError): M.audit_reference(screen,row)
            ref = screen['local_reference']['references'][0]
            ref.update(unknown='claim state outside arrival-local reference domain',phases=None,conditional_seconds=None)
            self.assertEqual(M.audit_reference(screen,row)['numeric'],0)

    def test_screen_unknown_is_terminal_and_cannot_hide_numeric_work(self):
        screen,row = fixture(); r = screen['local_reference']
        screen['unknown'] = r['unknown'] = 'no source remote sites for destination'
        r.update(references=[],charged_graph=0)
        self.assertEqual(M.audit_reference(screen,row)['references'],0)
        r['charged_graph'] = 1
        with self.assertRaises(AssertionError): M.audit_reference(screen,row)

    def test_incomplete_reference_has_no_cost_until_charged(self):
        screen,row = fixture(); r = screen['local_reference']
        r.update(references=[],charged_graph=0,complete=False)
        self.assertEqual(M.audit_reference(screen,row)['numeric'],0)
        r['complete'] = True
        with self.assertRaises(AssertionError): M.audit_reference(screen,row)

    def test_solar_negative_and_single_direction_outcomes(self):
        for clear in [[False,True],[True,False],[False,False]]:
            screen,row = fixture(); ref = screen['local_reference']['references'][0]
            for d,passed in zip(screen['sites'][0]['directions'],clear): d['solar_clear'] = passed
            ref['eligible_sides'] = [s for s,passed in zip([-1,1],clear) if passed]
            if not any(clear): ref.update(phases=None,conditional_seconds=None,unknown='no eligible arrival solar direction')
            self.assertEqual(M.audit_reference(screen,row)['numeric'],int(any(clear)))

    def test_strip_only_removes_reference_and_its_own_charges(self):
        screen,_ = fixture(); screen['charged_graph'] = 2
        report = dict(charged_graph=80,candidates=[dict(remote_arrival=screen,local_reference=dict(site=31),forecast=dict(ticks=77))])
        stripped = M.without_reference(report)
        self.assertEqual(stripped['charged_graph'],79)
        self.assertEqual(stripped['candidates'][0]['remote_arrival']['charged_graph'],2)
        self.assertEqual(stripped['candidates'][0]['local_reference'],dict(site=31))
        self.assertIn('local_reference',report['candidates'][0]['remote_arrival'])

    def test_unsupported_policy_and_unavailable_ship_stay_unknown(self):
        for change in ['policy','ship']:
            screen,row = fixture(); r = screen['local_reference']
            if change == 'policy':
                r['policy'] = row['mission']['policy'] = 'material_mission_v12'
                reason = 'policy outside arrival-local reference domain'
            else:
                M.A.P.pilot(row)['ship_form'] = 'escape_pod'
                reason = 'source ship unavailable for arrival-local reference'
            with self.assertRaises(AssertionError): M.audit_reference(screen,row)
            r.update(references=[],charged_graph=0,complete=True,unknown=reason)
            self.assertEqual(M.audit_reference(screen,row)['unknown'],reason)

    def test_extra_work_may_delay_publication_with_current_validation(self):
        off,on = pair_fixture()
        result = M.audit_pair(off,on)
        self.assertEqual(result,dict(extra_graph=1,sources=[dict(source_tick=11,extra_graph=1,ready_off=40,ready_on=41)]))
        for field,value in [('validated_tick',40),('charged_graph',81)]:
            changed = copy.deepcopy(on)
            changed['transfer_comparison'][M.KEY]['actors'][0]['source']['published_state'][field] = value
            with self.subTest(field=field),self.assertRaises(AssertionError): M.audit_pair(off,changed)

    def test_pair_cannot_rewrite_old_local_evidence_or_forecast(self):
        for field in ['local_reference','forecast']:
            off,on = pair_fixture()
            on['transfer_comparison'][M.KEY]['actors'][0]['source']['last_snapshot']['candidates'][0][field] = dict(wrong=True)
            with self.subTest(field=field),self.assertRaises(AssertionError): M.audit_pair(off,on)

    def test_ranked_report_cannot_publish_an_unfinished_reference(self):
        screen,row = fixture(); r = screen['local_reference']
        r.update(references=[],charged_graph=0,complete=False)
        snapshot = dict(ranked=False,candidates=[dict(remote_arrival=screen)])
        M.audit_snapshot(snapshot,row)
        snapshot['ranked'] = True
        with self.assertRaises(AssertionError): M.audit_snapshot(snapshot,row)

    def test_reused_bearing_needs_original_sample_identity_age_and_geometry(self):
        screen,_ = fixture(); ref = screen['local_reference']['references'][0]
        original = dict(sites=[dict(id=ref['site'],measurement_tick=ref['measurement_tick'],
                        retrospective=dict(choice=dict(age_and_identity_valid=True)),
                        same_frame_geometry_residual=dict(within_reconstruction_tolerance=True))])
        self.assertIsNone(M.join_reason(ref,original))
        self.assertIsNotNone(M.join_reason(ref,None))
        for change in ['measurement','identity','geometry','absent']:
            joined = copy.deepcopy(original); site = joined['sites'][0]
            if change == 'measurement': site['measurement_tick'] += 1
            elif change == 'identity': site['retrospective']['choice']['age_and_identity_valid'] = False
            elif change == 'geometry': site['same_frame_geometry_residual']['within_reconstruction_tolerance'] = False
            else: site['same_frame_geometry_residual'] = None
            with self.subTest(change=change): self.assertIsNotNone(M.join_reason(ref,joined))


if __name__ == '__main__': unittest.main()
