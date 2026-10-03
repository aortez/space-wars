import copy
import importlib.util
import math
from pathlib import Path
import unittest
from test_escape_travel import fixture as travel_fixture

spec=importlib.util.spec_from_file_location('approach',Path(__file__).parents[1]/'validate-transfer-approach.py')
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)


def sync(row):
    o=row['transfer_approach']['observation'];p=o['local']['combat']['recovery']['flight']['pilot']
    row['escape_travel']['observation']=o
    row['pilot']={k:v for k,v in p.items() if k!='sites'};row['planets']=o['planets']
    return p


def fixture(alternate=False):
    row,_=travel_fixture();o=row['escape_travel']['observation'];p=o['local']['combat']['recovery']['flight']['pilot']
    p['ship']['position']=dict(x=-220.,y=100.)
    o['planets'][0]['motion']['position']=dict(x=-120. if alternate else -1500.,y=0.)
    o['planets'][0]['radius']=40.
    o['planets'][1]['motion']['position']=dict(x=0.,y=0.);o['planets'][1]['radius']=40.
    p['planet']=o['planets'][0]
    o['sun']=None;o['boundary']=dict(center=dict(x=0.,y=0.),radius=5000.)
    radial=(-220/math.hypot(220,100),100/math.hypot(220,100))
    bearing=(-0.16170794234937613,0.9868386602586721) if alternate else radial
    vec=lambda v:dict(zip(['x','y'],v))
    sample=dict(tick=920,started_tick=920,deadline_tick=4520,selected_tick=920,vehicle=0,destination=1,
        ordinary_entry=vec(tuple(x*125 for x in radial)),entry=vec(tuple(x*125 for x in bearing)),bearing=vec(bearing),
        alternate=alternate,used_for_transfer=True,candidates_checked=32 if alternate else 1,
        reason='selected clear approach bearing' if alternate else 'ordinary entry clear')
    row['transfer_approach']=dict(observation=o,telemetry=dict(evaluated_ticks=1,alternate_ticks=int(alternate),guided_ticks=int(alternate),last=sample))
    sync(row);return row


class TransferApproachTests(unittest.TestCase):
    def test_clear_and_obstructed_entries_preserve_the_travel_binding(self):
        for alternate in [False,True]:
            row=fixture(alternate);state=A.audit_step(row,None)
            self.assertEqual(state['last']['alternate'],alternate)
            self.assertEqual(state['last']['deadline_tick'],4520)

    def test_clock_destination_vehicle_actor_and_entry_corruption_are_rejected(self):
        for key in ['deadline_tick','started_tick','selected_tick','destination','vehicle','entry','bearing','candidates_checked']:
            row=fixture(True);s=row['transfer_approach']['telemetry']['last']
            if key in ['entry','bearing']:s[key]['x']+=1.
            else:s[key]+=1
            with self.assertRaises(AssertionError):A.audit_step(row,None)

    def test_retained_bearing_moves_with_destination_and_revalidates_other_bodies(self):
        row=fixture(True);old=copy.deepcopy(A.audit_step(row,None));w=row['transfer_approach'];s=w['telemetry']['last']
        p=sync(row);p['tick']=921;s['tick']=921
        w['observation']['planets'][1]['motion']['position']['x']+=2.
        center=(2.,0.);ship=(-220.,100.);d=math.dist(ship,center)
        s['ordinary_entry']=dict(x=2.+125*(-222/d),y=125*100/d);s['entry']['x']+=2.
        s.update(reason='retaining clear approach bearing',candidates_checked=1)
        for k in ['evaluated_ticks','alternate_ticks','guided_ticks']:w['telemetry'][k]+=1
        sync(row);A.audit_step(row,old)
        w['observation']['planets'][0]['motion']['position']=copy.deepcopy(s['entry'])
        sync(row)
        with self.assertRaises(AssertionError):A.audit_step(row,old)

    def test_infeasible_geometry_keeps_the_ordinary_entry(self):
        row=fixture();w=row['transfer_approach'];w['observation']['boundary']['radius']=100.
        s=w['telemetry']['last'];s.update(reason='no clear entry; ordinary guidance retained',candidates_checked=32)
        A.audit_step(row,None)
        s['alternate']=True
        with self.assertRaises(AssertionError):A.audit_step(row,None)

    def test_climb_does_not_count_as_alternate_transfer_guidance(self):
        row=fixture(True);row['mission']['goal']='launch';w=row['transfer_approach'];w['telemetry']['guided_ticks']=0
        w['telemetry']['last']['used_for_transfer']=False
        A.audit_step(row,None)
        w['telemetry']['guided_ticks']=1
        with self.assertRaises(AssertionError):A.audit_step(row,None)

    def test_unready_or_finished_travel_cannot_supply_geometry_or_interaction(self):
        for change in range(6):
            row=fixture();p=sync(row)
            if change==0:p['controls_armed']=False
            elif change==1:p['queries_ready']=False
            elif change==2:p['landing']['supported_feet']=1
            elif change==3:row['escape_travel']['telemetry']['last']['finished_tick']=920
            elif change==4:row['actions'][0]['Scenario']['payload'][5]=1
            else:row['transfer_approach']['observation']['local']['combat']['recovery']['flight']['flight']['enabled']=False
            sync(row)
            with self.assertRaises(AssertionError):A.audit_step(row,None)

    def test_command_and_telemetry_stripping_preserve_prior_travel_evidence(self):
        old=dict(item=dict(seat=1),command=['old','--out','old-dir','--escape-travel-seats','1','--capture-escape-seats','1'])
        off=A.command(old,'binary','new',False)
        self.assertEqual(off[3:],old['command'][3:])
        self.assertEqual(A.command(old,'binary','new',True),off+['--transfer-approach-seats','1'])
        row=fixture(True);plain=A.without_option(row)
        self.assertNotIn('transfer_approach',plain)
        self.assertEqual(plain['escape_travel'],row['escape_travel'])
        self.assertEqual(plain['actions'],row['actions'])


if __name__=='__main__':unittest.main()
