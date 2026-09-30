import copy
import importlib.util
import math
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('approach_surveys', Path(__file__).parents[1]/'compare-approach-surveys.py')
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)


def fixture():
    candidates = []
    for bearing, tick, point in [(0,6,dict(x=100,y=160)), (16,7,dict(x=40,y=100))]:
        site = dict(planet=0,bearing=bearing)
        candidates.append(dict(id=site, measurement=dict(tick=tick,revision=0,
            planet=dict(position=dict(x=100,y=100),angle=0),ship_form='ship',
            finding='measured',climb_clear=True,site=dict(id=site,revision=0,
                vehicle_position=point,boarding_hatches=[dict(x=0,y=0),None]))))
    row = dict(tick=10,actor='player_1',policy=A.POLICIES[1],queries_ready=True,
        ship_position=dict(x=-200,y=0),planets=[dict(index=0,revision=0,
            claim=dict(owner=None,flag=None,stage_required_seconds=3),
            motion=dict(position=dict(x=0,y=0),angle=math.pi/2))],
        target=0,selected_tick=1,
        evidence=dict(generation=5,candidates=candidates))
    report = dict(actor=row['actor'],policy=row['policy'],source_tick=10,current_target=0,selected_tick=1,candidates=[dict(
        planet=0,revision=0,observed_owner=None,site=dict(planet=0,bearing=0),evidence_tick=6,
        evidence_age_ticks=4,evidence_kind=A.NEUTRAL_KIND,local=dict(landing=17.866667,
            exit=1/60,outbound=4/60,claim=181/60,return_board=2/60,departure=226/60))])
    return row, report


def audit(rows, reports, ticks=None):
    return A.audit_approaches(rows,reports,start_tick=10,ticks=len(rows) if ticks is None else ticks,actors=['player_1'])


class ApproachSurveyAudit(unittest.TestCase):
    def test_plan_retains_all_directed_and_regression_controls_and_uses_fresh_worlds(self):
        plan = A.plan()
        self.assertEqual(len(plan),46)
        self.assertEqual(len({r['name'] for r in plan}),46)
        fresh = [r for r in plan if r['kind']=='held_out']
        self.assertEqual(len(fresh),12)
        self.assertEqual(len({r['seed'] for r in fresh}),2)
        self.assertFalse({r['seed'] for r in fresh} & {r['seed'] for r in A.H.plan()})
        for item in plan:
            args, policies = A.arguments(item)
            self.assertIn('--trace-neutral-approaches',args)
            if item['seat'] is not None:
                self.assertEqual(policies[item['seat']],item['version'])
                self.assertEqual(policies[1-item['seat']],
                    13 if item['kind']=='directed' else 10 if item['kind']=='regression' else 15)

    def test_original_source_ranking_reprojects_and_ignores_publication_order(self):
        for reverse in [False,True]:
            row, report = fixture()
            if reverse: row['evidence']['candidates'].reverse()
            result = audit([row],[report])
            self.assertEqual(result['ranked_references'],1)
            self.assertEqual(result['two_site_rankings'],1)

    def test_source_and_ranking_mutations_fail(self):
        for mutation in range(14):
            row, report = fixture()
            c = report['candidates'][0]
            if mutation==0: c.update(site=dict(planet=0,bearing=16),evidence_tick=7,evidence_age_ticks=3)
            if mutation==1: row['queries_ready']=False
            if mutation==2: c['evidence_age_ticks']=0
            if mutation==3: c['revision']=1
            if mutation==4: row['evidence']['generation']=7
            if mutation==5: row['tick']=report['source_tick']=1807
            if mutation==6: row['ship_position']=dict(x=0,y=0)
            if mutation==7: row['evidence']['candidates'][0]['measurement']['climb_clear']=False
            if mutation==8: row['evidence']['candidates'][0]['measurement']['site']['boarding_hatches']=[None,None]
            if mutation==9: c['local']['landing']=1
            if mutation==10: row['ship_position']['x']=1e30
            if mutation==11: row['evidence']['candidates'].append(copy.deepcopy(row['evidence']['candidates'][0]))
            if mutation==12: row['evidence']['candidates'].pop()
            if mutation==13: report['selected_tick']+=1
            with self.subTest(mutation=mutation), self.assertRaises((AssertionError,KeyError,StopIteration)):
                audit([row],[report])

    def test_retained_source_cannot_be_renewed_or_credited_to_future_publication(self):
        row, report = fixture()
        later = dict(row,tick=11,evidence=None)
        report['source_tick']=11
        report['candidates'][0]['evidence_age_ticks']=5
        result = audit([row,later],[report])
        self.assertEqual(result['retained_references_without_current_measurement'],1)
        with self.assertRaises(AssertionError): audit([later],[report])
        later = copy.deepcopy(row)
        later['tick']=11
        later['evidence']['candidates'][0]['measurement']['site']['vehicle_position']['x']+=1
        with self.assertRaises(AssertionError): audit([row,later],[report])
        with self.assertRaises(AssertionError): audit([row], [report])

    def test_negative_query_and_material_history_revoke_retained_numeric_costs(self):
        for mutation in range(7):
            row, report = fixture()
            later = copy.deepcopy(row)
            later['tick']=11
            if mutation==0:
                for c in later['evidence']['candidates']:
                    c['measurement'].update(tick=11,finding='no_landing',site=None)
            if mutation==1:
                row['ship_position']=dict(x=0,y=0)
                later['evidence']=None
            if mutation==2:
                later['evidence']=None
                later['planets'][0]['revision']=1
            if mutation==3:
                later['evidence']=None
                later['planets'][0]['claim']['owner']='player_2'
            if mutation==4: later['queries_ready']=False
            if mutation==5:
                later['evidence']['generation']=11
                for c in later['evidence']['candidates']: c['measurement']=None
            final = copy.deepcopy(row)
            final.update(tick=12,evidence=None)
            report['source_tick']=12
            report['candidates'][0]['evidence_age_ticks']=6
            history = [row,later,final] if mutation != 6 else [row,final]
            with self.subTest(mutation=mutation), self.assertRaises((AssertionError,KeyError,StopIteration)):
                audit(history,[report],ticks=3)

    def test_pending_new_generation_preserves_a_previously_admitted_source(self):
        row, report = fixture()
        # A new selected journey legitimately resets demand before thirty ticks.
        pending = copy.deepcopy(row)
        pending.update(tick=11,selected_tick=11)
        pending['evidence']['generation']=11
        for c in pending['evidence']['candidates']: c['measurement']=None
        report.update(source_tick=11,selected_tick=11)
        report['candidates'][0]['evidence_age_ticks']=5
        self.assertEqual(audit([row,pending],[report])['retained_references_without_current_measurement'],1)
        with self.assertRaises(AssertionError): audit([row],[],ticks=2)

    def test_handoff_audit_also_validates_v16_and_keeps_its_v15_default(self):
        spec = importlib.util.spec_from_file_location('handoff_tests', Path(__file__).with_name('test_landing_handoff.py'))
        tests = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(tests)
        rows, decisions = tests.fixture()
        for r in rows: r['mission']['policy']=A.POLICIES[1]
        decisions['switches'][0]['source_forecast']['policy']=A.POLICIES[1]
        self.assertEqual(A.H.audit_handoffs(rows,[],decisions,A.POLICIES)['references'][0]['completed_tick'],13)
        with self.assertRaises(KeyError): A.H.audit_handoffs(rows,[],decisions)


if __name__ == '__main__': unittest.main()
