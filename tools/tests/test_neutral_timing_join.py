import copy
import importlib.util
from pathlib import Path
import unittest
import tempfile
from unittest.mock import patch


def load(name, path):
    spec=importlib.util.spec_from_file_location(name,path)
    module=importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


J=load('join',Path(__file__).resolve().parents[1]/'join-neutral-capture-timing.py')
N=load('neutral_fixture',Path(__file__).with_name('test_neutral_capture_timing.py'))
C=load('composition_fixture',Path(__file__).with_name('test_capture_composition.py'))


def source(sun=False):
    row,_,wrapper=N.fixture(sun)
    row['mission']['capture']['goal']='approach'
    J.P.pilot(row)['site_query']='survey'
    return row,wrapper['report']


def comparison():
    r=C.fixture()
    _,record=source()
    record['source_tick']=r['source_tick']
    for c in r['candidates']:
        c['neutral_timing']=dict(record=record if c['current'] else None,
            unknown=None if c['current'] else 'no actual first-choice timing at source')
    c=r['candidates'][0]
    c['source_capture']='current_approach'
    r['capture_costs'][0].update(travel_seconds=0.0,known_components_seconds=25)
    r['neutral_timing_costs']=[dict(destination=c['destination'],source_tick=r['source_tick'],
        travel_seconds=0.0 if c['current'] else None,
        source_tick_total_seconds=record['total_seconds'] if c['current'] else None,
        unknown=c['neutral_timing']['unknown']) for c in r['candidates']]
    return r


class NeutralTimingJoinAudit(unittest.TestCase):
    def test_no_choice_cannot_publish_or_retain_an_invented_source(self):
        keys=['initial','last_snapshot','published','published_state','final_state','environment','rejected','last_snapshot_tick']
        source=dict.fromkeys(keys)
        source.update(seat=1,source_tick=3601,attempted=False)
        report=dict(transfer_comparison=dict(sources=[source],submitted=0,completed=0,cancelled=0,charged_graph=0))
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            (root/'transfer-comparison-work.jsonl').write_text('')
            J.audit_schedule(root,report,[],None,[dict(seat=1,tick=3601)])
            for key in keys:
                changed=copy.deepcopy(report)
                changed['transfer_comparison']['sources'][0][key]='invented'
                with self.subTest(key=key),self.assertRaises(AssertionError):
                    J.audit_schedule(root,changed,[],None,[dict(seat=1,tick=3601)])

    def test_published_record_cannot_change_together_with_its_total_or_domain(self):
        for change in ['total','unknown_to_numeric']:
            initial=comparison()
            if change=='unknown_to_numeric':
                n=initial['candidates'][0]['neutral_timing']
                n['unknown']='source exposed to opponent'
                n['record'].update(unknown=n['unknown'],total_seconds=None,phases=None)
                initial['neutral_timing_costs'][0].update(source_tick_total_seconds=None,unknown=n['unknown'])
            source=dict(initial=initial,last_snapshot=copy.deepcopy(initial),published=copy.deepcopy(initial))
            J.audit_immutable_source(source)
            final=source['published']
            n=final['candidates'][0]['neutral_timing']
            n.update(unknown=None)
            n['record'].update(unknown=None,total_seconds=738.0,phases=dict.fromkeys(J.R.PHASES,123.0))
            final['neutral_timing_costs'][0].update(source_tick_total_seconds=738.0,unknown=None)
            J.audit_composition(final)
            with self.assertRaisesRegex(AssertionError,'source record changed'): J.audit_immutable_source(source)

    def test_current_predicate_allows_world_motion_and_one_clear_hatch(self):
        row,record=source()
        origin=copy.deepcopy(row)
        p=J.P.pilot(row)
        p['site_query']=dict(selected=record['selected']['site'])
        p['sites'][0]['position']['x']+=1
        p['gravity']['x']+=.1
        self.assertIsNone(J.current_reason(record,origin,row))

    def test_current_predicate_rejects_missing_evidence_retry_claim_and_exposure(self):
        for change in ['deferred','missing','hatch','retry','claim','exposed','solar']:
            row,record=source()
            origin=copy.deepcopy(row)
            p=J.P.pilot(row)
            if change=='deferred': p['site_query']=dict(deferred=dict(next_tick=12))
            elif change=='missing': p['sites']=[]
            elif change=='hatch': p['sites'][0]['boarding_hatches']=[None,None]
            elif change=='retry': row['mission']['capture']['landing']['landing_retries']+=1
            elif change=='claim': p['planet']['claim']['claimant']='player_2'
            elif change=='solar': row['observation']['local']['planet_orbit_omega']=.01
            else: row['observation']['local']['combat']['target']=dict(ground_occluded=False,motion=copy.deepcopy(p['ship']))
            with self.subTest(change=change): self.assertIsNotNone(J.current_reason(record,origin,row))

    def test_join_keeps_old_components_and_rejects_backfill_and_retiming(self):
        J.audit_composition(comparison())
        for change in ['hypothetical','source','clock','retime','travel','unknown']:
            r=comparison()
            if change=='hypothetical': r['candidates'][1]['neutral_timing']=copy.deepcopy(r['candidates'][0]['neutral_timing'])
            elif change=='source': r['candidates'][0]['neutral_timing']['record']['source_tick']+=1
            elif change=='clock': r['neutral_timing_costs'][0]['source_tick']+=1
            elif change=='retime': r['neutral_timing_costs'][0]['source_tick_total_seconds']-=1
            elif change=='travel': r['neutral_timing_costs'][0]['travel_seconds']=2
            else: r['candidates'][1]['neutral_timing']['unknown']=None
            with self.subTest(change=change),self.assertRaises(AssertionError): J.audit_composition(r)

    def test_pending_has_no_composed_total_and_unknown_does_not_blend_old_estimate(self):
        r=comparison()
        r.update(ranked=False,charged_graph=2,fastest_known_handoffs=[],capture_costs=[],neutral_timing_costs=[])
        J.audit_composition(r)
        r['neutral_timing_costs']=comparison()['neutral_timing_costs']
        with self.assertRaises(AssertionError): J.audit_composition(r)
        r=comparison()
        n=r['candidates'][0]['neutral_timing']
        n.update(unknown='source exposed to opponent')
        n['record'].update(unknown=n['unknown'],total_seconds=None,phases=None)
        r['neutral_timing_costs'][0].update(source_tick_total_seconds=None,unknown=n['unknown'])
        J.audit_composition(r)

    def test_terminal_observation_does_not_invent_a_dispatch(self):
        s,rows,up,_=C.F.fixture()
        with self.assertRaises(AssertionError): J.M.audit_schedule(s,rows[:2],up,12)
        J.M.audit_schedule(s,rows[:2],up,12,terminal_observed=True)
        s['sources'][0]['final_state']['reason']='source expired'
        with self.assertRaises(AssertionError): J.M.audit_schedule(s,rows[:2],up,12,terminal_observed=True)
        s['sources'][0]['final_state']['reason']='physical contact'
        s['sources'][0]['final_state']['charged_graph']+=1
        with self.assertRaises(AssertionError): J.M.audit_schedule(s,rows[:2],up,12,terminal_observed=True)

    def test_terminal_reason_needs_its_own_raw_witness(self):
        row,record=source()
        origin=copy.deepcopy(row)
        s=dict(source_tick=record['source_tick'],initial=comparison(),final_state=dict(cancelled_tick=row['tick'],reason='source expired'))
        with self.assertRaises(AssertionError): J.audit_terminal_reason(s,origin,row)
        s['final_state']['reason']='planet material or ownership changed'
        for p in row['observation']['planets']: p['claim']['neutralizations']=0
        origin=copy.deepcopy(row)
        with self.assertRaises(AssertionError): J.audit_terminal_reason(s,origin,row)
        row['observation']['planets'][0]['revision']+=1
        J.audit_terminal_reason(s,origin,row)

    def test_solar_environment_guard_precedes_inner_attempt_guard(self):
        row,record=source()
        origin=copy.deepcopy(row)
        row['observation']['local']['planet_orbit_omega']=.01
        row['mission']['completed_sorties']+=1
        self.assertEqual(J.current_reason(record,origin,row),'neutral solar environment changed')

    def test_near_zero_solar_cancellation_retains_uncertainty(self):
        row,record=source(True)
        origin=copy.deepcopy(row)
        reason='neutral current solar assessment unavailable or unsafe'
        for value in [-.02,-.005,.005,.02]:
            solar=dict(approach_clearance=value,parked_clearance=1,departure_clearance=1)
            with patch.object(J.Q,'solar_plan',return_value=solar):
                if value==.02:
                    with self.assertRaises(AssertionError): J.audit_neutral_cancellation(record,origin,row,reason)
                else:
                    self.assertEqual(J.audit_neutral_cancellation(record,origin,row,reason),
                        ['approach_clearance'] if abs(value)<.01 else [])

    def test_direct_solar_assessment_uses_direct_arrival_time(self):
        row,_,_=N.fixture(True)
        p=J.P.pilot(row)
        site=p['sites'][0]
        solar=J.Q.solar_plan(row['observation']['local'],site,1,circling=False)
        self.assertAlmostEqual(solar['arrival_seconds'],J.Q.length(J.Q.sub(J.Q.vec(p['ship']['position']),J.Q.vec(site['vehicle_position'])))/12,places=5)

    def test_compatibility_strip_does_not_hide_old_reference_changes(self):
        r=comparison()
        old=J.strip_join(r)
        r['neutral_timing_costs'][0]['source_tick_total_seconds']+=1
        self.assertEqual(old,J.strip_join(r))
        r['capture_costs'][0]['known_components_seconds']+=1
        self.assertNotEqual(old,J.strip_join(r))


if __name__=='__main__': unittest.main()
