import copy
import importlib.util
from pathlib import Path
import unittest

from test_capture_execution import fixture

spec=importlib.util.spec_from_file_location('powered_mission',Path(__file__).parents[1]/'validate-powered-mission.py')
M=importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)


def publication(live=True):
    return dict(pilot=dict(tick=100,owner='player_1',planet=dict(index=1)),
        landing_objective=dict(planning='jetpack_round_trip',actor='player_1',tick=90 if live else 100,
            validated_tick=100 if live else None,objective=dict(planet=1,revision=0),sites=[],actual=None),
        objective_work='ready' if live else None,
        objective_evidence=dict(measurement_tick=90,publication=dict(retained_routes=1)) if live else None)


def visits():
    _,rows,_=fixture()
    visit=dict(planet=1,recorded=dict(departed_tick=106),physical=dict.fromkeys(['landed','exited','claimed','boarded','departed']))
    converted=[]
    for row in rows:
        p=copy.deepcopy(row['observation']['combat']['recovery']['flight']['pilot'])
        converted.append(dict(pilot=p,planets=[None,copy.deepcopy(p['planet'])],capture=None))
    return visit,converted


class PoweredMissionTests(unittest.TestCase):
    def test_all_pairs_share_commands_except_the_declared_route_option(self):
        plan=M.plan()
        self.assertEqual(len(plan),28)
        self.assertEqual(len({p['group'] for p in plan}),14)
        for group in sorted({p['group'] for p in plan}):
            arms=[p for p in plan if p['group']==group]
            self.assertEqual({a['powered'] for a in arms},{False,True})
            commands=[M.arguments(p) for p in arms]
            for command in commands:
                self.assertEqual(command[command.index('--objective-graph-budget')+1],'4')
                self.assertEqual(command[command.index('--objective-query-budget')+1],'384')
                i=command.index('--powered-capture-seats');del command[i:i+2]
            self.assertEqual(*commands)
        self.assertEqual(len({p['seed'] for p in plan if p['kind']=='armed'}),2)

    def test_live_and_native_evidence_have_distinct_clock_contracts(self):
        self.assertEqual(M.audit_publication(publication(),'jetpack_round_trip')['age'],10)
        self.assertEqual(M.audit_publication(publication(False),'jetpack_round_trip')['age'],0)
        for field,value in [('tick',-21),('validated_tick',99),('planning','joint_round_trip'),('actor','player_2')]:
            row=publication();row['landing_objective'][field]=value
            with self.assertRaises(AssertionError,msg=field): M.audit_publication(row,'jetpack_round_trip')
        row=publication();row['objective_work']='pending'
        with self.assertRaises(AssertionError): M.audit_publication(row,'jetpack_round_trip')
        row=publication(False);row['landing_objective']['tick']=99
        with self.assertRaises(AssertionError): M.audit_publication(row,'jetpack_round_trip')

    def test_powered_delivery_cannot_renew_an_expired_launch_window(self):
        row=publication()
        row['landing_objective']['sites']=[dict(crossing=dict(measured_tick=90,launch_until_tick=99,plan=dict(planet=1,revision=0)))]
        with self.assertRaises(AssertionError): M.audit_publication(row,'jetpack_round_trip')
        row['landing_objective']['sites'][0]['crossing']['launch_until_tick']=120
        self.assertEqual(M.audit_publication(row,'jetpack_round_trip')['powered'],1)

    def test_mission_departure_uses_the_target_frame_and_earned_capture(self):
        visit,rows=visits()
        rows[-1]['pilot']['planet']=dict(index=2)
        for row in rows: M.observe_visit(visit,row)
        self.assertEqual(visit['physical'],dict(landed=101,exited=102,claimed=104,boarded=105,departed=106))

    def test_unearned_claim_wrong_ship_and_grounded_departure_fail(self):
        for change in ['owner','capture_count','vehicle','clearance']:
            visit,rows=visits()
            if change=='owner':
                for row in rows: row['planets'][1]['claim']['owner']='player_2'
            elif change=='capture_count':
                for row in rows: row['planets'][1]['claim']['captures']=0
            elif change=='vehicle':
                for row in rows[5:]: row['pilot']['location']={'aboard':1}
            else: rows[-1]['pilot']['ship']['position']['y']=70
            with self.assertRaises(AssertionError,msg=change):
                for row in rows: M.observe_visit(visit,row)

    def test_recapture_does_not_erase_an_earlier_physical_claim(self):
        visit,rows=visits();rows[-1]['planets'][1]['claim']['owner']='player_2'
        for row in rows: M.observe_visit(visit,row)
        self.assertEqual(visit['physical']['claimed'],104)


if __name__=='__main__': unittest.main()
