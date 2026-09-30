import copy
from collections import Counter
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('surveyed_arrival', Path(__file__).resolve().parents[1] / 'compare-surveyed-arrivals.py')
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


def fixture(mode='measured'):
    planet = dict(index=2,revision=5,radius=10,motion=dict(position=dict(x=2,y=3),angle=.1),
        claim=dict(planet=2,owner=None,flag=None,stage_required_seconds=3,flag_interaction_range=4,captures=0,neutralizations=0))
    pilot = dict(owner='player_1',vehicle=0,spaceling=0,ship_available=True,ship_form='ship')
    observed = {(tick,0):dict(tick=tick,seat=0,observation=dict(planets=[copy.deepcopy(planet)],
        local=dict(combat=dict(recovery=dict(flight=dict(pilot=dict(pilot,tick=tick))))))) for tick in [10,11]}
    candidate = dict(id=dict(planet=2,bearing=33),status='measured',reason=None,
        measurement=dict(tick=10,planet=planet['motion'],revision=5,ship_form='ship',finding='measured'))
    evidence = dict(generation=10,candidates=[candidate])
    plan = dict(request=dict(generation=10))
    survey = [dict(tick=10,seat=0,plan=plan,allocation=dict(charged=dict(physics_queries=60)),evidence=[[0,evidence]])]
    retained = dict(tick=11,actor='player_1',vehicle=0,spaceling=0,episode_seed=42,
        groups=[dict(identity=M.R.neutral_identity(planet),observed_tick=11,survey=copy.deepcopy(evidence))])
    attached = copy.deepcopy(retained)
    if mode == 'empty': attached['groups'] = []
    actor = dict(seat=0,trigger=dict(tick=10,plan=plan,evidence=copy.deepcopy(evidence)),
        untriggered=False,unobserved=False,source=dict(seat=0,source_tick=11,attempted=True,rejected=None),
        retained_source=retained,attached_source=attached)
    return actor,observed,survey


def ledger_fixture():
    original = dict(tick=100,playing_charged_graph=4,allocation=dict(charged=dict(graph=58,physics_queries=0)))
    row = dict(queue=M.KEY,tick=100,playing_charged_graph=4,original_comparison_charged=dict(graph=58,physics_queries=0),
        remaining_before_comparison=dict(graph=2,physics_queries=0),
        allocation=dict(tick=1,allowance=dict(graph=2,physics_queries=0),charged=dict(graph=2,physics_queries=0),
            jobs=[dict(request=dict(actor=s,generation=1),charged=dict(graph=1,physics_queries=0),phase='Pending',age_ticks=1) for s in [0,1]]),
        actors=[dict(seat=s,rejected=None,state=dict(token=dict(actor=s,generation=1),phase='pending',validated_tick=100,
                source_tick=100,charged_graph=1,actor=f'player_{s+1}',destination=2,completed_tick=None,cancelled_tick=None,reason=None),ranked=False,candidates=[dict(charged_graph=1)]) for s in [0,1]])
    sources = {a['seat']:dict(source_tick=100,initial=dict(current_destination=2),final_state=copy.deepcopy(a['state']),rejected=None) for a in row['actors']}
    return row,original,sources


class SurveyedArrivalAudit(unittest.TestCase):
    def test_pre_intent_refusal_needs_no_environment_and_cannot_hide_work(self):
        source = dict(environment=None,rejected='retained survey source unavailable',initial=None,
                      last_snapshot=None,last_snapshot_tick=None,published=None,published_state=None,final_state=None)
        M.audit_rejected(source)
        source['published'] = dict(ranked=True)
        with self.assertRaises(AssertionError): M.audit_rejected(source)

    def test_equal_partial_forecasts_are_preserved_and_mutations_rejected(self):
        forecast = dict(end=None,ticks=60,samples=[dict(after_ticks=0,x=0),dict(after_ticks=60,x=2)],
                        phases=[dict(start_tick=0,end_tick=60,goal='transfer')])
        self.assertEqual(M.forecast_pair(forecast,copy.deepcopy(forecast)),'partial_exact_equal')
        changed = copy.deepcopy(forecast); changed['samples'][1]['x'] = 3
        with self.assertRaises(AssertionError): M.forecast_pair(forecast,changed)
        longer = copy.deepcopy(forecast); longer['ticks'] = longer['phases'][0]['end_tick'] = 90
        self.assertEqual(M.forecast_pair(longer,forecast),'partial_exact_prefix')
        changed['ticks'] = 59
        with self.assertRaises(AssertionError): M.forecast_pair(longer,changed)

    def test_paired_arms_keep_actual_measurement_epoch_and_only_withhold_attachment(self):
        for mode in ['empty','measured']:
            report = M.audit_trigger(*fixture(mode),42,mode)
            self.assertEqual(report['source_age_ticks'],1)
            self.assertEqual(report['groups'],1)

    def test_first_negative_or_incomplete_attempt_cannot_be_replaced_by_later_success(self):
        for finding in ['no_landing','incomplete']:
            actor,observed,survey = fixture()
            for candidate in [survey[0]['evidence'][0][1]['candidates'][0],actor['trigger']['evidence']['candidates'][0],
                              actor['retained_source']['groups'][0]['survey']['candidates'][0],actor['attached_source']['groups'][0]['survey']['candidates'][0]]:
                candidate['status'] = candidate['measurement']['finding'] = finding
            self.assertEqual(M.audit_trigger(actor,observed,survey,42,'measured')['findings'],[finding])
            later = copy.deepcopy(survey[0]); later['tick'] = 40; survey.append(later)
            actor['trigger']['tick'] = 40
            with self.assertRaises(AssertionError): M.audit_trigger(actor,observed,survey,42,'measured')

    def test_rebasing_wrong_actor_epoch_generation_or_attachment_is_rejected(self):
        for mutation in ['source_tick','trigger','measurement','actor','vehicle','seed','generation','observed','status','attachment']:
            actor,observed,survey = fixture()
            if mutation == 'source_tick': actor['source']['source_tick'] = 12
            elif mutation == 'trigger': actor['trigger']['tick'] = 11
            elif mutation == 'measurement': actor['retained_source']['groups'][0]['survey']['candidates'][0]['measurement']['tick'] = 11
            elif mutation in ['actor','vehicle']: actor['retained_source'][mutation] = 99
            elif mutation == 'seed': actor['retained_source']['episode_seed'] = 43
            elif mutation == 'generation': actor['retained_source']['groups'][0]['survey']['generation'] = 11
            elif mutation == 'observed': actor['retained_source']['groups'][0]['observed_tick'] = 10
            elif mutation == 'status': actor['retained_source']['groups'][0]['survey']['candidates'][0]['status'] = 'stale'
            else: actor['attached_source']['groups'] = []
            with self.subTest(mutation=mutation),self.assertRaises(AssertionError):
                M.audit_trigger(actor,observed,survey,42,'measured')

    def test_changed_material_or_vehicle_cannot_keep_geometry(self):
        for mutation in ['material','vehicle']:
            actor,observed,survey = fixture()
            if mutation == 'material': observed[11,0]['observation']['planets'][0]['revision'] += 1
            else:
                M.A.P.pilot(observed[11,0])['vehicle'] = 1
                actor['retained_source']['vehicle'] = actor['attached_source']['vehicle'] = 1
            with self.assertRaises(AssertionError): M.audit_trigger(actor,observed,survey,42,'measured')
            actor['retained_source']['groups'] = actor['attached_source']['groups'] = []
            self.assertEqual(M.audit_trigger(actor,observed,survey,42,'measured')['groups'],0)

    def test_untriggered_sources_cannot_invent_attempts(self):
        actor,observed,_ = fixture()
        actor.update(trigger=None,source=None,retained_source=None,attached_source=None,untriggered=True)
        self.assertEqual(M.audit_trigger(actor,observed,[],42,'measured'),dict(triggered=False))
        actor['trigger'] = dict(tick=10)
        with self.assertRaises(AssertionError): M.audit_trigger(actor,observed,[],42,'measured')

    def test_two_actors_share_residual_after_playing_and_original_work(self):
        charges = Counter()
        row,original,sources = ledger_fixture()
        self.assertEqual(M.audit_allocation(row,original,charges,{},1,sources),64)
        self.assertEqual(charges,{0:1,1:1})

    def test_extra_allowance_physical_work_stale_validation_or_excess_age_is_rejected(self):
        for mutation in ['quota','charge','physical','playing','original','age','validation','actor','clock','replacement','missing']:
            row,original,sources = ledger_fixture()
            if mutation == 'quota': row['allocation']['allowance']['graph'] += 1
            elif mutation == 'charge': row['allocation']['charged']['graph'] += 1
            elif mutation == 'physical': row['allocation']['charged']['physics_queries'] = 1
            elif mutation == 'playing': row['playing_charged_graph'] = 3
            elif mutation == 'original': row['original_comparison_charged']['graph'] = 57
            elif mutation == 'age': row['actors'][0]['state']['source_tick'] = -21
            elif mutation == 'validation': row['actors'][0]['state']['validated_tick'] = 99
            elif mutation == 'clock': row['allocation']['tick'] = row['tick']
            elif mutation == 'replacement':
                row['actors'][0]['state']['token']['generation'] = 2
                row['allocation']['jobs'][0]['request']['generation'] = 2
            elif mutation == 'missing': row['actors'].pop()
            else: row['actors'][0]['state']['token']['actor'] = 1
            with self.subTest(mutation=mutation),self.assertRaises(AssertionError):
                M.audit_allocation(row,original,Counter(),{},1,sources)


if __name__ == '__main__': unittest.main()
