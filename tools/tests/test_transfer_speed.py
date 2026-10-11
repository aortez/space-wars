import copy
import importlib.util
import math
from pathlib import Path
import unittest
from test_transfer_approach import fixture as approach_fixture, sync

spec=importlib.util.spec_from_file_location('speed',Path(__file__).parents[1]/'validate-transfer-speed.py')
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)


def fixture(limited=False):
    row=approach_fixture(limited);o=row['transfer_approach']['observation'];f=o['local']['combat']['recovery']['flight'];p=f['pilot']
    f['flight']['limits']['turn_speed']=1.8;p['gravity']=dict(x=0.,y=0.)
    p['ship']['velocity']=dict(x=60. if limited else 0.,y=-10. if limited else 0.)
    a=row['transfer_approach']['telemetry']['last'];delta=S.E.subtract(a['entry'],p['ship']['position']);length=math.hypot(*delta)
    before=tuple(x/length*min(length*.7,55.) for x in delta)
    constraints=S.limits(o,1);c=constraints[0];normal=S.E.vector(c['normal']);minimum=c['minimum_world_normal_speed']
    excess=minimum-S.dot(before,normal)
    after=tuple(v+n*excess/S.dot(normal,normal) for v,n in zip(before,normal)) if limited else before
    vec=lambda v:dict(zip(['x','y'],v))
    sample=dict(tick=920,started_tick=920,deadline_tick=4520,selected_tick=920,vehicle=0,destination=1,
        desired_before=vec(before),desired_after=vec(after),limited=limited,force_brake=limited,feasible=True,
        candidates_checked=2 if limited else 1,limits=constraints)
    row['transfer_speed']=dict(observation=o,avoidance=None,telemetry=dict(evaluated_ticks=1,limited_ticks=int(limited),
        braking_ticks=int(limited),infeasible_ticks=0,last=sample))
    row['actions'][0]['Scenario']['payload'][6]=int(limited);sync(row)
    return row


class TransferSpeedTests(unittest.TestCase):
    def test_current_relative_limit_and_native_brake_are_audited(self):
        for limited in [False,True]:
            row=fixture(limited);state=S.audit_step(row,None)
            self.assertEqual(state['limited_ticks'],int(limited))
            self.assertEqual(state['last']['deadline_tick'],4520)

    def test_corrupt_clock_destination_velocity_or_limit_is_rejected(self):
        for key in ['started_tick','deadline_tick','selected_tick','vehicle','destination','desired_before','desired_after','maximum_closing_speed','normal','candidates_checked']:
            row=fixture(True);s=row['transfer_speed']['telemetry']['last']
            if key in ['desired_before','desired_after']:s[key]['x']+=1.
            elif key=='normal':s['limits'][0][key]['x']+=.1
            elif key=='maximum_closing_speed':s['limits'][0][key]+=1.
            else:s[key]+=1
            with self.assertRaises(AssertionError):S.audit_step(row,None)

    def test_forced_braking_requires_the_actual_brake_byte_and_open_wings(self):
        for change in range(3):
            row=fixture(True)
            if change==0:row['actions'][0]['Scenario']['payload'][6]=0
            elif change==1:row['actions'][1]['Scenario']['payload'][1]=1
            else:row['actions'][0]['Scenario']['payload'][5]=1
            with self.assertRaises(AssertionError):S.audit_step(row,None)

    def test_moving_planet_gravity_and_turn_authority_are_consumed_inputs(self):
        for change in range(3):
            row=fixture(True);o=row['transfer_speed']['observation'];f=o['local']['combat']['recovery']['flight']
            if change==0:o['planets'][0]['motion']['velocity']['x']+=20.
            elif change==1:f['pilot']['gravity']['y']=-10.
            else:f['flight']['limits']['turn_speed']=.7
            sync(row)
            with self.assertRaises(AssertionError):S.audit_step(row,None)

    def test_two_body_projection_and_infeasible_fallback_remain_bounded(self):
        bound=lambda x,y,v:dict(normal=dict(x=x,y=y),minimum_world_normal_speed=v)
        self.assertEqual(S.project((-20.,-20.),[bound(1.,0.,5.),bound(0.,1.,8.)]),((5.,8.),4))
        self.assertIsNone(S.project((0.,0.),[bound(1.,0.,20.),bound(-1.,0.,20.)])[0])
        self.assertIsNone(S.project((0.,0.),[bound(1.,0.,100.)])[0])

    def test_finished_or_unready_travel_cannot_supply_speed_decisions(self):
        for change in range(5):
            row=fixture();p=sync(row)
            if change==0:p['queries_ready']=False
            elif change==1:p['controls_armed']=False
            elif change==2:row['escape_travel']['telemetry']['last']['finished_tick']=920
            elif change==3:row['mission']['goal']='launch'
            else:row['transfer_approach']['telemetry']['last']['used_for_transfer']=False
            sync(row)
            with self.assertRaises(AssertionError):S.audit_step(row,None)

    def test_configuration_and_parity_preserve_all_earlier_evidence(self):
        old=dict(item=dict(seat=1),command=['old','--out','old-dir','--transfer-approach-seats','1','--escape-travel-seats','1'])
        off=S.command(old,'binary','new',False)
        self.assertEqual(off[3:],old['command'][3:])
        self.assertEqual(S.command(old,'binary','new',True),off+['--transfer-speed-seats','1'])
        row=fixture(True);plain=S.without_option(row)
        self.assertNotIn('transfer_speed',plain)
        self.assertEqual(plain['transfer_approach'],row['transfer_approach'])
        self.assertEqual(plain['escape_travel'],row['escape_travel'])
        self.assertEqual(plain['actions'],row['actions'])


if __name__=='__main__':unittest.main()
