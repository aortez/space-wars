import copy
import importlib.util
from pathlib import Path
import unittest

from test_arrival_survey import fixture as row_fixture
from test_surveyed_arrival_comparison import fixture as trigger_fixture
from test_arrival_preference import fixture as preference_fixture

SPEC = importlib.util.spec_from_file_location('neighbors', Path(__file__).resolve().parents[1]/'compare-arrival-neighbors.py')
N = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(N)
S, M = N.M.S, N.M


def row_at(tick, index, finding='measured'):
    row,state,forecast,pilot = copy.deepcopy(row_fixture())
    ids = [dict(planet=2,bearing=b) for b in [0,63,1]]
    row['plan']['request']['candidates'] = ids+[None]
    row['tick'] = state['validated_tick'] = tick
    row['sampling_site'] = ids[index] if index < 3 else None
    result = row['evidence'][0][1]
    sample = copy.deepcopy(result['candidates'][0])
    sample.update(id=row['sampling_site'],status=finding,reason=None)
    sample['measurement'].update(tick=tick,finding=finding,site=dict(id=row['sampling_site']) if finding=='measured' else None)
    result['candidates'] = [sample if i==index else dict(id=v,status='pending',reason=None,measurement=None) for i,v in enumerate(ids)]
    return row,state,forecast,pilot


def collection(missing=0):
    actor,observed,original = trigger_fixture()
    frame = observed[10,0]
    for tick in range(12,72):
        observed[tick,0] = copy.deepcopy(frame)
        observed[tick,0]['tick'] = M.A.P.pilot(observed[tick,0])['tick'] = tick
    ids = [dict(planet=2,bearing=b) for b in [33,32,34]]
    plan = dict(token=dict(actor=0,generation=1),site=ids[0],
        request=dict(generation=10,candidates=ids+[None],sample_climb=True))
    survey = []
    candidates = [dict(id=v,status='pending',reason='sample not observed at measurement tick',measurement=None) for v in ids]
    for i in range(3-missing):
        event = copy.deepcopy(original[0]); event.update(tick=10+30*i,plan=copy.deepcopy(plan))
        sample = copy.deepcopy(event['evidence'][0][1]['candidates'][0])
        sample.update(id=ids[i],status='no_landing')
        sample['measurement'].update(tick=event['tick'],finding='no_landing')
        event['evidence'][0][1]['candidates'] = [sample if n==i else dict(id=v,status='pending',reason=None,measurement=None) for n,v in enumerate(ids)]
        candidates[i] = copy.deepcopy(sample)
        survey.append(event)
    if not survey:
        survey = [dict(original[0],plan=plan,evidence=[])]
    actor['trigger'] = dict(tick=10,plan=plan,evidence=copy.deepcopy(survey[0]['evidence'][0][1]) if survey[0]['evidence'] else None)
    actor['source']['source_tick'] = 71
    retained = actor['retained_source']; retained['tick'] = 71
    if missing == 3: retained['groups'] = []
    else:
        retained['groups'][0].update(observed_tick=survey[-1]['tick']+1,survey=dict(generation=10,candidates=candidates))
    actor['attached_source'] = copy.deepcopy(retained)
    return actor,observed,survey


class ArrivalNeighborsAudit(unittest.TestCase):
    def test_requested_mode_cannot_silently_fall_back_or_mix_collection_semantics(self):
        report = dict(transfer_comparison=dict(arrival_survey={},surveyed_arrival_comparison=dict(mode='measured')))
        N.audit_mode(report,'nearest')
        with self.assertRaises(AssertionError): N.audit_mode(report,'neighbors3')
        report['transfer_comparison']['arrival_survey']['pattern'] = 'neighbors3'
        with self.assertRaises(AssertionError): N.audit_mode(report,'neighbors3')
        report['transfer_comparison'][N.KEY]['arrival_collection'] = dict(model='neighbors3_window_v1',ticks=60)
        N.audit_mode(report,'neighbors3')
        with self.assertRaises(AssertionError): N.audit_mode(report,'nearest')
        report['transfer_comparison'][N.KEY]['mode'] = 'empty'
        with self.assertRaises(AssertionError): N.audit_mode(report,'neighbors3')

    def test_witnessed_continuity_refusal_reaches_the_physical_cursor(self):
        actor,observed,survey = collection(1)
        observed[20,0]['observation']['planets'][0]['revision'] += 1
        reasons = S.collection_disruptions(observed,survey,survey[0],38)
        last,cursors = {},{}
        S.audit_row(*row_at(8,0),384,[],last,cursors)
        cursors[0]['refusal'] = reasons[0]['reason']
        args = row_at(38,1)
        args[0].update(deferred=reasons[0]['reason'],allocation=None,evidence=[])
        self.assertEqual(S.audit_row(*args,384,[],last,cursors),(None,0))
        self.assertEqual(cursors[0]['count'],1)

    def test_gap_at_deadline_propagates_at_actual_freeze_but_later_changes_do_not(self):
        _,observed,survey = collection(1)
        observed[72,0] = copy.deepcopy(observed[71,0])
        observed[72,0]['tick'] = M.A.P.pilot(observed[72,0])['tick'] = 72
        observed[72,0]['observation']['planets'][0]['revision'] += 1
        self.assertEqual(S.collection_disruptions(observed,survey,survey[0],72),[])
        del observed[71,0]
        reasons = S.collection_disruptions(observed,survey,survey[0],72)
        self.assertEqual(reasons[0]['tick'],72)
        last,cursors = {},{}
        S.audit_row(*row_at(8,0),384,[],last,cursors)
        cursors[0]['refusal'] = reasons[0]['reason']
        args = row_at(72,1)
        args[0].update(deferred=reasons[0]['reason'],allocation=None,evidence=[])
        self.assertEqual(S.audit_row(*args,384,[],last,cursors),(None,0))

    def test_wrapped_request_and_individual_epochs_follow_unchanged_throttle(self):
        last,cursors = {},{}
        for index,tick in enumerate([8,38,68]):
            sample,used = S.audit_row(*row_at(tick,index),384,[],last,cursors)
            self.assertEqual((sample['tick'],used),(tick,7))
        self.assertEqual(cursors[0]['count'],3)
        args = row_at(69,3)
        args[0].update(deferred='collection window closed',allocation=None,evidence=[])
        self.assertEqual(S.audit_row(*args,384,[],last,cursors),(None,0))

    def test_charged_absence_and_negative_findings_spend_slots_without_retry(self):
        for absent in ['no_evidence','no_measurement']:
            last,cursors = {},{}
            args = row_at(8,0)
            if absent == 'no_evidence': args[0]['evidence'] = []
            else: args[0]['evidence'][0][1]['candidates'][0].update(status='pending',measurement=None)
            self.assertEqual(S.audit_row(*args,384,[],last,cursors),(None,7))
            self.assertEqual((last[0],cursors[0]['count']),(8,1))
            sample,used = S.audit_row(*row_at(38,1,'no_landing'),384,[],last,cursors)
            self.assertEqual((sample['finding'],used),('no_landing',7))

    def test_zero_fuel_and_delayed_second_sample_cannot_extend_window(self):
        last,cursors = {},{}
        S.audit_row(*row_at(8,0),384,[],last,cursors)
        args = row_at(38,1)
        args[0].update(deferred='query budget',allocation=None,evidence=[],remaining_before=dict(graph=0,physics_queries=0))
        S.audit_row(*args,0,[],last,cursors)
        self.assertEqual(cursors[0]['count'],1)
        S.audit_row(*row_at(39,1),384,[],last,cursors)
        args = row_at(69,2); args[0].update(deferred='collection window closed',allocation=None,evidence=[])
        S.audit_row(*args,384,[],last,cursors)
        self.assertEqual(cursors[0]['count'],2)

    def test_request_refusal_stays_latched_when_original_plan_returns(self):
        last,cursors = {},{}
        S.audit_row(*row_at(8,0),384,[],last,cursors)
        for tick in [38,39]:
            args = row_at(tick,1)
            if tick == 38: args[0]['plan']['token']['generation'] = args[1]['token']['generation'] = 2
            args[0].update(deferred='arrival shortlist request changed',allocation=None,evidence=[])
            S.audit_row(*args,384,[],last,cursors)
        self.assertEqual(cursors[0]['count'],1)

    def test_complete_partial_negative_and_absent_collection_preserve_original_samples(self):
        for missing in [0,1,2,3]:
            args = collection(missing)
            r = M.audit_trigger(*args,42,'measured',True)
            self.assertEqual(r['missing_slots'],missing)
            self.assertEqual(r['sample_ages'],[61,31,1][:3-missing])
            if missing in [1,2]:
                args[0]['retained_source']['groups'][0]['survey']['candidates'][-1]['reason'] = None
                with self.assertRaises(AssertionError): M.audit_trigger(*args,42,'measured',True)

    def test_retiming_retry_or_invented_geometry_is_rejected(self):
        for change in ['freeze','epoch','retry','missing_geometry','binding']:
            args = collection(1)
            actor,observed,survey = args
            if change == 'freeze': actor['source']['source_tick'] = 70
            elif change == 'epoch': survey[1]['evidence'][0][1]['candidates'][1]['measurement']['tick'] += 1
            elif change == 'retry': survey.append(copy.deepcopy(survey[0]))
            elif change == 'missing_geometry': actor['retained_source']['groups'][0]['survey']['candidates'][-1] = copy.deepcopy(actor['retained_source']['groups'][0]['survey']['candidates'][0])
            else: actor['retained_source']['vehicle'] += 1
            with self.subTest(change=change),self.assertRaises(AssertionError): M.audit_trigger(*args,42,'measured',True)

    def test_interruption_is_terminal_even_if_identity_returns_or_deadline_frame_is_missing(self):
        for change in ['material','vehicle','gap','deadline','request']:
            actor,observed,survey = collection(1)
            reason = 'arrival collection identity or observation continuity changed'
            if change == 'material': observed[20,0]['observation']['planets'][0]['revision'] += 1
            elif change == 'vehicle': M.A.P.pilot(observed[20,0])['vehicle'] += 1
            elif change == 'gap': del observed[20,0]
            elif change == 'deadline':
                observed[72,0] = observed.pop((71,0)); observed[72,0]['tick'] = M.A.P.pilot(observed[72,0])['tick'] = 72
                actor['source']['source_tick'] = 72
            else:
                event = copy.deepcopy(survey[0]); event.update(tick=20,allocation=None,evidence=[])
                event['plan']['token']['generation'] += 1
                survey.insert(1,event); reason = 'arrival shortlist request changed'
            actor['source'].update(rejected=reason,initial=None,last_snapshot=None,last_snapshot_tick=None,published=None,published_state=None,final_state=None)
            actor['attached_source'] = None
            result = M.audit_trigger(actor,observed,survey,42,'measured',True)
            self.assertEqual(result['rejected'],reason)
            actor['source']['rejected'] = None
            with self.assertRaises(AssertionError): M.audit_trigger(actor,observed,survey,42,'measured',True)

    def test_center_comparison_uses_same_frozen_frame_and_preserves_unknowns(self):
        candidate,_ = preference_fixture(); r = candidate['remote_arrival']['site_preference']
        center = copy.deepcopy(r['preferred']); center['approach_score'] = 10
        r['assessments'][0]['eligible'] = [center]
        better = dict(center,site=dict(planet=2,bearing=32),approach_score=3)
        r['assessments'].append(dict(r['assessments'][0],site=better['site'],eligible=[better]))
        r['preferred'] = better
        comparison = N.subset_comparison(r,center['site'],None,None)
        self.assertEqual(comparison['source_tick'],r['source_tick'])
        self.assertEqual(comparison['center_only_preference'],center)
        self.assertEqual(comparison['center_to_subset_relation'],'site_mismatch')
        r['assessments'][0]['eligible'] = []
        comparison = N.subset_comparison(r,center['site'],None,None)
        self.assertIsNone(comparison['center_only_preference'])
        self.assertEqual(comparison['center_to_subset_relation'],'unknown')

    def test_native_subset_relation_requires_all_joined_material_and_source_identity(self):
        candidate,_ = preference_fixture(); r = candidate['remote_arrival']['site_preference']
        center = r['preferred']['site']
        second = dict(r['assessments'][0],site=dict(planet=2,bearing=32),eligible=[],unknown='no eligible native arrival direction')
        r['assessments'].append(second)
        proof = dict(source_tick=r['source_tick'],seat=0,destination=2,native_best_retained=r['preferred'],
            sites=[dict(id=a['site'],measurement_tick=a['measurement_tick'],retrospective=dict(choice=dict(age_and_identity_valid=True)),
                same_frame_geometry_residual=dict(within_reconstruction_tolerance=True)) for a in r['assessments']])
        join = dict(seat=0,destination=2,native=r['preferred'],native_domain_unknown=None,observed_handoff_to_choice_seconds=1/60)
        self.assertEqual(N.subset_comparison(r,center,join,proof)['native_subset']['classification'],'site_and_direction_match')
        for change in ['expired_loser','wrong_source','partial','domain']:
            r2,p2,j2 = copy.deepcopy(r),copy.deepcopy(proof),copy.deepcopy(join)
            if change == 'expired_loser': p2['sites'][1]['retrospective']['choice']['age_and_identity_valid'] = False
            elif change == 'wrong_source': p2['source_tick'] += 1
            elif change == 'partial': r2['complete'] = False
            else: j2['native_domain_unknown'] = 'source exposed'
            result = N.subset_comparison(r2,center,j2,p2)['native_subset']
            self.assertEqual(result['classification'],'unknown')
            self.assertEqual(result['raw_relation'],'site_and_direction_match')


if __name__ == '__main__': unittest.main()
