import copy
import importlib.util
from pathlib import Path
import struct
import unittest
from test_escape_travel import fixture

spec=importlib.util.spec_from_file_location('response',Path(__file__).parents[1]/'validate-projectile-response.py')
R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)


def actions(brake=False):
    return [{'Scenario':dict(kind=1398079490,payload=list(struct.pack('<f',.25))+[0,0,int(brake),0])},
            {'Scenario':dict(kind=1398079491,payload=[0,1])},
            {'Scenario':dict(kind=1398079493,payload=[0,1,1])}]


class ProjectileResponseTests(unittest.TestCase):
    def test_destroyed_unoccupied_ship_does_not_need_to_turn_into_a_pod(self):
        pilot=dict(ship_available=True,ship_form='ship',vehicle=0,location='on_foot')
        self.assertFalse(R.source_ship_missing(pilot,0))
        pilot['ship_available']=False
        self.assertTrue(R.source_ship_missing(pilot,0))

    def test_native_priorities_and_unready_physics_prevent_a_response(self):
        evidence,_=fixture()
        row=dict(goal='transfer',capture_active=False,recovery_active=False,target=1)
        self.assertIsNotNone(R.ready(row,evidence))
        for change in range(16):
            e=copy.deepcopy(evidence);r=copy.deepcopy(row)
            w=e['escape_travel'];o=w['observation'];f=o['local']['combat']['recovery']['flight'];p=f['pilot'];a=w['telemetry']['last']
            if change==0:r['goal']='avoid sun'
            elif change==1:r['capture_active']=True
            elif change==2:r['recovery_active']=True
            elif change==3:r['target']=2
            elif change==4:p['controls_armed']=False
            elif change==5:p['queries_ready']=False
            elif change==6:f['flight']['enabled']=False
            elif change==7:p['ship_available']=False
            elif change==8:p['ship_form']='pod'
            elif change==9:p['vehicle']=1
            elif change==10:p['landing']['supported_feet']=1
            elif change==11:a['observed_tick']-=1
            elif change==12:a['deadline_tick']=p['tick']
            elif change==13:a['finished_tick']=p['tick']
            elif change==14:o['match_context']=dict(finished=True)
            else:e['escape_travel']=None
            with self.subTest(change=change):self.assertIsNone(R.ready(r,e))

    def test_brake_and_steering_preserve_weapons_and_native_braking(self):
        for brake in (False,True):
            original=actions(brake)
            for mode in ('brake','left','right'):
                new=R.override(original,mode);p=new[0]['Scenario']['payload']
                self.assertEqual(new[2],original[2]);self.assertEqual(new[1]['Scenario']['payload'],[0,0])
                self.assertEqual(p[5],0)
                if mode=='brake':self.assertEqual(p[4:7],[0,0,1]);self.assertEqual(p[:4],original[0]['Scenario']['payload'][:4])
                else:self.assertEqual(p[4:7],[int(not brake),0,int(brake)])
            self.assertEqual(R.override(original,'observe'),original)

    def test_fixed_deadline_does_not_restart_when_threat_changes(self):
        key=dict(started_tick=100,deadline_tick=3700,selected_tick=100,vehicle=0,destination=2)
        a=None
        for tick in range(200,260):
            a,applied=R.advance(a,tick,key,dict(id=tick,spawn_tick=20,entry_seconds=.5),'left')
            self.assertEqual(applied,tick<230)
        self.assertEqual(a['applied_ticks'],30);self.assertEqual(a['finished_tick'],230)
        self.assertEqual(a['source']['deadline_tick'],3700);self.assertEqual(a['threat']['id'],200)

    def test_native_priority_cancels_without_resuming_later(self):
        key=dict(vehicle=0)
        a,_=R.advance(None,10,key,dict(id=1),'brake')
        a,applied=R.advance(a,11,None,dict(id=2),'brake')
        self.assertFalse(applied);self.assertEqual(a['reason'],'native priority or transfer identity changed')
        a,applied=R.advance(a,12,key,dict(id=3),'brake')
        self.assertFalse(applied);self.assertEqual(a['applied_ticks'],1)

    def test_first_threat_uses_entry_time_then_identity_without_owner_filter(self):
        def projectile(i,x,owner):
            return dict(id=i,spawn_tick=1,owner=owner,collision_radius=2.,relative_position=dict(x=x,y=0.),relative_velocity=dict(x=-50.,y=0.))
        d=dict(observer_radius=8.,projectiles=[projectile(2,100.,'player_2'),projectile(1,100.,'player_1'),projectile(3,120.,'player_2')])
        self.assertEqual(R.threat(d)['id'],1)
        d['projectiles'].reverse();self.assertEqual(R.threat(d)['id'],1)
        d['projectiles']=[projectile(3,120.,'player_2')];self.assertIsNone(R.threat(d))

    def test_command_only_adds_the_requested_laboratory_mode(self):
        old=dict(command=['old','--out','old-root','--trace-projectiles','true','--transfer-speed-seats','0'])
        off=R.command(old,'binary','root','none')
        self.assertEqual(off[3:],old['command'][3:])
        self.assertEqual(R.command(old,'binary','root','brake'),off+['--probe-projectile-response','brake'])


if __name__=='__main__':unittest.main()
