import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('landing_handoff', Path(__file__).parents[1]/'compare-landing-handoff.py')
H = importlib.util.module_from_spec(spec)
spec.loader.exec_module(H)


def fixture():
    site = dict(planet=0, bearing=2)
    handoff = dict(site=site, source_tick=9, evidence_tick=8, switch_tick=10,
        started_tick=None, accepted_tick=None, landed_tick=None, completed_tick=None,
        invalidated_tick=None, reason=None)
    decisions = dict(switches=[dict(seat=0, switch=dict(tick=10,to=0),
        source_forecast=dict(policy='material_mission_v15',source_tick=9,
            candidates=[dict(planet=0,revision=0,site=site,evidence_tick=8)]),
        actual_visit=dict(arrived_tick=10,departed_tick=13))])
    rows = []
    for tick in range(10,14):
        handoff[['started_tick','accepted_tick','landed_tick','completed_tick'][tick-10]] = tick
        p = dict(tick=tick, owner='player_1', queries_ready=True,
            planet=dict(index=0,revision=0), sites=[dict(id=site, revision=0,
                boarding_hatches=[dict(x=0,y=0),None],vehicle_position=dict(x=0,y=0))],
            ship=dict(position=dict(x=0,y=0)), landing=dict(phase='landed'), transfer='ready')
        capture = dict(site=site,goal='surface',landing=dict(landed_tick=12),
            acquisition=dict(tick=11,planet=0,revision=0,required_site=site,selected_site=site,
                reason='selected_site',checks=dict(eligible=1)))
        rows.append(dict(tick=tick,seat=0,handoff=copy.deepcopy(handoff),visit_tick=10,
            observation=dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=p))))),
            mission=dict(policy='material_mission_v15',capture=capture,
                destination_planning=dict(landing_handoff=copy.deepcopy(handoff)))))
    return rows, decisions


class LandingHandoffAudit(unittest.TestCase):
    def test_plan_keeps_all_directed_cases_and_declares_fresh_smoke_worlds(self):
        plan = H.plan()
        self.assertEqual(len(plan),46)
        self.assertEqual(len({r['name'] for r in plan}),46)
        self.assertEqual(sum(r['kind']=='directed' for r in plan),32)
        fresh = [r for r in plan if r['kind']=='held_out']
        self.assertEqual(len(fresh),12)
        self.assertEqual(len({r['seed'] for r in fresh}),2)
        for item in plan:
            _, policies = H.arguments(item)
            if item['seat'] is not None:
                self.assertEqual(policies[item['seat']],item['version'])
                self.assertEqual(policies[1-item['seat']],
                    13 if item['kind']=='directed' else 10 if item['kind']=='regression' else 14)

    def test_completed_handoff_has_source_and_native_milestones(self):
        rows, decisions = fixture()
        result = H.audit_handoffs(rows,[],decisions)
        self.assertEqual(result['references'][0]['completed_tick'],13)

    def test_missing_source_and_false_native_milestones_fail(self):
        for mutation in range(8):
            rows, decisions = fixture()
            if mutation==0: rows.pop(0)
            if mutation==1: rows[1]['handoff']['site']=dict(planet=0,bearing=3)
            if mutation==2: rows[1]['mission']['capture']['acquisition']['checks']['eligible']=0
            if mutation==3: rows[2]['observation']['local']['combat']['recovery']['flight']['pilot']['transfer']='too_far'
            if mutation==4: rows[3]['handoff']['invalidated_tick']=13
            if mutation==5: rows[3]['handoff']['accepted_tick']=12
            if mutation==6: rows.pop(1)
            if mutation==7: rows.pop(2)
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                H.audit_handoffs(rows,[],decisions)

    def test_refused_cost_requires_fresh_native_site_instead_of_remote_readmission(self):
        rows, decisions = fixture()
        rows=rows[:3]
        rows[2]['handoff'].update(landed_tick=None,invalidated_tick=12,reason='native rejection')
        rows[2]['mission']['destination_planning']['landing_handoff']=copy.deepcopy(rows[2]['handoff'])
        fallback=dict(planet=0,bearing=3)
        rows[2]['mission']['capture']['site']=fallback
        evaluation=dict(policy='material_mission_v15',actor='player_1',selected_tick=10,
            source_tick=12,completed_tick=12,current_target=0,candidates=[dict(current=True,total_seconds=10,
            evidence_tick=12,evidence_kind='current local measurement',site=fallback)])
        self.assertEqual(H.audit_handoffs(rows,[evaluation],decisions)['fresh_native_replacement_reports'],1)
        for field, value in [('evidence_tick',11),('evidence_kind',H.S.FLAG_KIND),('site',dict(planet=0,bearing=2))]:
            bad=copy.deepcopy(evaluation)
            bad['candidates'][0][field]=value
            with self.subTest(field=field), self.assertRaises(AssertionError):
                H.audit_handoffs(rows,[bad],decisions)
        evaluation['candidates'][0]['total_seconds']=None
        self.assertEqual(H.audit_handoffs(rows,[evaluation],decisions)['refused_current_reports_unknown'],1)
        evaluation['source_tick']=11
        with self.assertRaises(AssertionError): H.audit_handoffs(rows,[evaluation],decisions)

    def test_native_milestones_join_the_original_site_epoch_and_arrival(self):
        for mutation in range(9):
            rows, decisions = fixture()
            acquisition=rows[1]['mission']['capture']['acquisition']
            if mutation==0: acquisition['tick']-=1
            if mutation==1: acquisition['planet']=1
            if mutation==2: acquisition['required_site']=dict(planet=0,bearing=3)
            if mutation==3: acquisition['selected_site']=dict(planet=0,bearing=3)
            if mutation==4:
                p=rows[1]['observation']['local']['combat']['recovery']['flight']['pilot']
                p['planet']['revision']=1
                for site in p['sites']: site['revision']=1
                acquisition['revision']=1
            if mutation==5: rows[2]['mission']['capture']['site']=dict(planet=0,bearing=3)
            if mutation==6: decisions['switches'][0]['actual_visit']['arrived_tick']=9
            if mutation==7:
                decisions['switches'][0]['switch']['tick']=0
                decisions['switches'][0]['source_forecast']['source_tick']=0
                decisions['switches'][0]['source_forecast']['candidates'][0]['evidence_tick']=0
                decisions['switches'][0]['actual_visit']['arrived_tick']=0
                for row in rows:
                    row['handoff'].update(source_tick=0,evidence_tick=0,switch_tick=0,started_tick=0)
                    for f in ['accepted_tick','landed_tick','completed_tick']:
                        if row['handoff'][f] is not None: row['handoff'][f]+=119
                    row['mission']['destination_planning']['landing_handoff']=copy.deepcopy(row['handoff'])
                    row['tick']=0 if row['tick']==10 else row['tick']+119
                    row['observation']['local']['combat']['recovery']['flight']['pilot']['tick']=row['tick']
                acquisition['tick']=130
                decisions['switches'][0]['actual_visit']['departed_tick']=132
            if mutation==8:
                p=rows[2]['observation']['local']['combat']['recovery']['flight']['pilot']
                p['planet']['revision']=1
                for site in p['sites']: site['revision']=1
            with self.subTest(mutation=mutation), self.assertRaises(AssertionError):
                H.audit_handoffs(rows,[],decisions)


if __name__ == '__main__': unittest.main()
