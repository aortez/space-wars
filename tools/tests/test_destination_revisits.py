import copy
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('revisits',Path(__file__).parents[1]/'analyze-destination-revisits.py')
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)


def event(tick,kind,planet=None,reason=None):
    return dict(tick=tick,kind=kind,planet=planet,reason=reason)


def visit(planet,selected,abandoned=None,reason=None,departed=None):
    return dict(planet=planet,selected_tick=selected,abandoned_tick=abandoned,departed_tick=departed,
        reason=reason,arrived_tick=selected+1,landed_tick=None,claimed_tick=None,boarded_tick=None)


def fixture(return_tick=2500,reason='no qualified route'):
    a=visit(1,10,100,reason);b=visit(1,return_tick)
    events=[event(10,'selected',1),event(100,'replan',1,reason),
        event(101,'replan',None,'pausing travel for nearby opponent'),
        event(101,'pursuit_started',None,'nearby opponent'),
        event(1901,'pursuit_ended',None,'pursuit budget exhausted'),event(return_tick,'selected',1)]
    snapshot=dict(target=1,events=copy.deepcopy(events[:1]),capture=dict(failed_tick=99,started_tick=11,
        failure=reason,goal='blocked',site=None,acquisition=dict(planet=1,revision=2)))
    report=dict(metrics=[dict(visits=[a,b])],missions=[dict(events=events,target=1,capture=None,completed_sorties=0)],
        events=[dict(seat=0,telemetry=snapshot)],samples=[])
    return report,dict(switches=[])


def selected_source(row,kind='value_switch'):
    selection=row['selection'];visit=row['recorded']
    switch=dict(tick=selection['tick'],source_tick=selection['tick']-2,
        to=visit['planet'],**{'from':0},current_seconds=30,destination_seconds=40,
        value=dict(equivalent_seconds_saved=10) if kind=='value_switch' else None)
    current=dict(planet=0,total_seconds=30,evidence_tick=2300,route_validated_tick=None,revision=0)
    target=dict(planet=1,total_seconds=40,evidence_tick=2300,route_validated_tick=selection['tick']-1,revision=2)
    forecast=dict(source_tick=selection['tick']-2,completed_tick=selection['tick']-1,current_target=0,
        selected_tick=2000,combat_risk='unmodelled',comparison_reason='conditional',opponent_distance=320,
        candidates=[current,target])
    return dict(seat=0,switch=switch,source_forecast=forecast,actual_visit=visit)


class DestinationRevisitTests(unittest.TestCase):
    def test_pursuit_consumes_cooldown_without_erasing_original_failure(self):
        report,decisions=fixture()
        result=A.analyze(report,0,decisions);row=result['records'][0]
        self.assertTrue(row['deferred'])
        self.assertEqual(row['deferred_until'],1900)
        self.assertEqual(row['ticks_after_expiry'],600)
        self.assertEqual(row['confirmed_pursuit_overlap_ticks'],1799)
        self.assertTrue(row['pursuit_ended_after_expiry'])
        self.assertEqual(row['pursuits'][0]['duration_ticks'],1800)
        self.assertEqual(result['counts']['capture_failure_return_initial_picker'],1)

    def test_return_before_verified_expiry_is_rejected(self):
        report,decisions=fixture(1800)
        report['missions'][0]['events']=[e for e in report['missions'][0]['events'] if e['kind']!='pursuit_ended']
        with self.assertRaisesRegex(ValueError,'cooldown expired'):
            A.analyze(report,0,decisions)

    def test_expiry_is_inclusive_and_unfinished_pursuit_stays_censored(self):
        report,decisions=fixture(1900)
        report['missions'][0]['events']=[e for e in report['missions'][0]['events'] if e['kind']!='pursuit_ended']
        row=A.analyze(report,0,decisions)['records'][0]
        self.assertEqual(row['ticks_after_expiry'],0)
        self.assertIsNone(row['pursuits'][0]['end'])
        self.assertIsNone(row['pursuits'][0]['duration_ticks'])
        self.assertFalse(row['pursuit_ended_after_expiry'])

    def test_missing_failure_witness_does_not_invent_deferral(self):
        report,decisions=fixture()
        report['events']=[]
        row=A.analyze(report,0,decisions)['records'][0]
        self.assertIsNone(row['deferred'])
        self.assertIsNone(row['deferred_until'])
        self.assertIsNone(row['ticks_after_expiry'])

    def test_failed_capture_from_another_visit_cannot_classify_this_reason(self):
        report,decisions=fixture()
        report['events'][0]['telemetry']['events'][0]['tick']=9
        row=A.analyze(report,0,decisions)['records'][0]
        self.assertIsNone(row['failure_witness'])
        self.assertIsNone(row['deferred'])

    def test_recovery_and_frame_exit_do_not_gain_a_capture_cooldown(self):
        for reason in ['ship or surface recovery required','left destination approach frame']:
            report,decisions=fixture(reason=reason)
            row=A.analyze(report,0,decisions)['records'][0]
            self.assertFalse(row['deferred'])
            self.assertIsNone(row['deferred_until'])

    def test_no_return_is_a_finite_horizon_not_permanent_avoidance(self):
        report,decisions=fixture()
        report['metrics'][0]['visits'].pop()
        report['missions'][0]['events'].pop()
        result=A.analyze(report,0,decisions);row=result['records'][0]
        self.assertFalse(row['returned'])
        self.assertIn('within this run',row['return_unknown'])
        self.assertEqual(result['counts']['no_recorded_return'],1)

    def test_value_return_requires_exact_accepted_switch_and_keeps_new_evidence_distinct_from_cover(self):
        report,decisions=fixture()
        events=report['missions'][0]['events']
        events[-1]['reason']='better supported capture value'
        b=visit(0,2000,2500,'better supported capture value')
        events[-1:-1]=[event(2000,'selected',0),event(2500,'replan',0,'better supported capture value')]
        report['metrics'][0]['visits'].insert(1,b)
        verified=A.F.M.audit(report)['visits'][-1]
        with self.assertRaisesRegex(ValueError,'exactly one'):
            A.analyze(report,0,decisions)
        decisions['switches']=[selected_source(verified)]
        result=A.analyze(report,0,decisions);row=result['records'][0]
        self.assertEqual(row['selection']['kind'],'value_switch')
        self.assertTrue(row['selection']['nominal_destination_slower'])
        self.assertTrue(row['return_evidence_measured_after_failure'])
        self.assertTrue(row['same_measured_material_revision'])
        self.assertIn('unknown',row['return_exposure_resolved'])
        self.assertEqual(row['intervening_completed'],0)
        bad=copy.deepcopy(decisions)
        bad['switches'][0]['actual_visit']['selected_tick']+=1
        with self.assertRaisesRegex(ValueError,'another visit'):
            A.analyze(report,0,bad)

    def test_truncated_selection_history_is_retained_as_unverified(self):
        report,decisions=fixture()
        report['missions'][0]['events'].pop(0)
        result=A.analyze(report,0,decisions)
        self.assertEqual(result['counts']['visits'],2)
        self.assertEqual(result['counts']['unverified_visits'],1)
        self.assertEqual(len(result['unverified']),1)

    def test_pursuit_restart_without_end_cannot_double_count_overlap(self):
        events=[event(101,'pursuit_started'),event(102,'pursuit_started')]
        with self.assertRaisesRegex(ValueError,'without an ending'):
            A.pursuits(events,100)


if __name__=='__main__':unittest.main()
