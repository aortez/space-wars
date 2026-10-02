import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('execution', Path(__file__).parents[1]/'probe-capture-execution.py')
E = importlib.util.module_from_spec(spec)
spec.loader.exec_module(E)


def fixture():
    initial = dict(tick=100,owner='player_1',location={'aboard':0},vehicle=0,transfers=0,
        ship_form='ship',ship_available=True,ship={},actor=None,
        landing=dict(phase='flying',planet=1,supported_feet=0),
        planet=dict(index=1,claim=dict(owner='player_2',captures=0,neutralizations=0)))
    milestones = dict.fromkeys(['landed','exited','neutralized','claimed','boarded'])
    rows = []
    for i in range(7):
        p = copy.deepcopy(initial)
        p['tick'] += i
        if 1 <= i <= 5: p['landing'].update(phase='landed',supported_feet=2)
        if i >= 2: p['transfers'] = 1
        if 2 <= i <= 4: p['location'] = 'on_foot'
        if i >= 3: p['planet']['claim'].update(owner=None,neutralizations=1)
        if i >= 4: p['planet']['claim'].update(owner='player_1',captures=1)
        if i >= 5: p['transfers'] = 2
        if 1 <= i <= 5: milestones[['landed','exited','neutralized','claimed','boarded'][i-1]] = p['tick']
        telemetry = dict(goal='complete' if i == 6 else 'surface',completed_tick=106 if i == 6 else None,
            failed_tick=None,failure=None,site=None)
        r = {k:copy.deepcopy(p[k]) for k in ['tick','location','vehicle','transfers','ship_form','ship_available','ship','actor','landing']}
        r.update(planet=1,claim=p['planet']['claim'],match_context=None,goal=telemetry['goal'],ground_goal=None,
            crossing=None,charge=None,burn_seconds=None,milestones=copy.deepcopy(milestones),
            stop='controller_completed' if i == 6 else None,
            actions=None if i == 6 else [{},{},{'Scenario':dict(kind=0x53550005,payload=[0,0,0])}],
            telemetry=telemetry,observation=dict(combat=dict(recovery=dict(flight=dict(pilot=p))),landing_objective=None))
        rows.append(r)
    arm = dict(name='walking',planning='joint_round_trip',start_tick=100,end_tick=106,
        stop='controller_completed',telemetry=rows[-1]['telemetry'],milestones=copy.deepcopy(milestones),
        launches=[],crossing_completions=[],lowest_charge=1.0,burn_seconds=None,physics_ok=True,
        audits=[dict(tick=t,audit=dict(occupied_cells=100,removed_cells=0,max_speed=20,issues=[])) for t in [100,106]])
    return arm,rows,initial


class CaptureExecutionTests(unittest.TestCase):
    def test_complete_requires_physical_capture_boarding_and_departure(self):
        arm,rows,initial = fixture()
        self.assertEqual(E.audit_arm(arm,rows,initial,10800)['milestones']['claimed'],104)

    def test_gaps_wrong_source_and_unearned_milestones_fail(self):
        for change in ['gap','source','owner','capture_count','transfer','depart']:
            arm,rows,initial = fixture()
            if change == 'gap': rows.pop(2)
            elif change == 'source': initial['vehicle'] = 1
            elif change == 'owner': rows[4]['claim']['owner'] = 'player_2'
            elif change == 'capture_count': rows[4]['claim']['captures'] = 0
            elif change == 'transfer': rows[5]['transfers'] = 1
            elif change == 'depart': rows[-1]['landing']['supported_feet'] = 2
            with self.assertRaises(AssertionError,msg=change): E.audit_arm(arm,rows,initial,10800)

    def test_truncation_or_fabricated_success_is_not_completion(self):
        for change in ['truncated','timeout','telemetry','weapons']:
            arm,rows,initial = fixture()
            if change == 'truncated': rows.pop()
            elif change == 'timeout': arm['stop'] = rows[-1]['stop'] = 'horizon'
            elif change == 'telemetry': arm['telemetry']['completed_tick'] = 105
            else: rows[2]['actions'][2]['Scenario']['payload'][1] = 1
            with self.assertRaises(AssertionError,msg=change): E.audit_arm(arm,rows,initial,10800)

    def test_physics_conservation_and_end_audit_are_required(self):
        for change in ['cells','speed','issues','missing']:
            arm,rows,initial = fixture()
            if change == 'cells': arm['audits'][-1]['audit']['removed_cells'] += 1
            elif change == 'speed': arm['audits'][-1]['audit']['max_speed'] = 500
            elif change == 'issues': arm['audits'][-1]['audit']['issues'] = ['bad support']
            else: arm['audits'].pop()
            with self.assertRaises(AssertionError,msg=change): E.audit_arm(arm,rows,initial,10800)

    def test_horizon_is_reported_without_promoting_partial_progress(self):
        arm,rows,initial = fixture()
        arm['stop'] = rows[-1]['stop'] = 'horizon'
        arm['telemetry']['completed_tick'] = None
        self.assertEqual(E.audit_arm(arm,rows,initial,6)['stop'],'horizon')

    def test_launch_requires_charge_recent_forecast_and_same_physical_corridor(self):
        plan = dict(planet=1,revision=0,direction='Left',start=dict(x=0,y=60),destination=dict(x=10,y=60),cruise_radius=65,
            anchor=dict(Vehicle=dict(index=0,form='ship',position=dict(x=5,y=60),angle=0)))
        launch = dict(tick=100,equipment=dict(charge=1),latest_forecast=dict(version=2,measured_tick=90,launch_until_tick=210,plan=plan),
            ground=dict(crossing=dict(plan=copy.deepcopy(plan))))
        E.audit_launch(launch)
        # A newly measured corridor can differ slightly as the parked hull settles.
        launch['ground']['crossing']['plan']['start']['x'] += 0.2
        E.audit_launch(launch)
        for change in ['charge','stale','future','anchor','endpoint','revision']:
            row = copy.deepcopy(launch)
            if change == 'charge': row['equipment']['charge'] = 0.97
            elif change == 'stale': row['tick'] = 211
            elif change == 'future': row['tick'] = 89
            elif change == 'anchor': row['ground']['crossing']['plan']['anchor']['Vehicle']['index'] = 1
            elif change == 'endpoint': row['ground']['crossing']['plan']['start']['x'] += 1
            else: row['ground']['crossing']['plan']['revision'] += 1
            with self.assertRaises(AssertionError,msg=change): E.audit_launch(row)


if __name__ == '__main__': unittest.main()
