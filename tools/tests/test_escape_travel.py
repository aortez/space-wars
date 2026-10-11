import copy
import importlib.util
import math
from pathlib import Path
import unittest
from test_capture_escape import fixture as escape_fixture

spec=importlib.util.spec_from_file_location('travel',Path(__file__).parents[1]/'validate-escape-travel.py')
T=importlib.util.module_from_spec(spec);spec.loader.exec_module(T)


def sync(row):
    o=row['escape_travel']['observation'];p=o['local']['combat']['recovery']['flight']['pilot']
    row['pilot']={k:v for k,v in p.items() if k!='sites'};row['planets']=o['planets']
    return p


def fixture():
    source,_=escape_fixture();o=source['capture_escape']['observation'];c=o['local']['combat']
    p=c['recovery']['flight']['pilot'];p['tick']=920;p['ship']['position']=dict(x=0.,y=200.)
    p['landing'].update(phase='flying',supported_feet=0)
    target=copy.deepcopy(p['planet']);target.update(index=1);target['motion']['position']=dict(x=1000.,y=0.)
    o['planets'].append(target);source['planets']=o['planets']
    c['target']['health_fraction']=1.;c['weapons']=dict(last_hit_taken_tick=920)
    e=source['capture_escape']['telemetry']['last'];e.update(observed_tick=920,finished_tick=920,reason='escape deadline',guidance='finished')
    source['pilot']={k:v for k,v in p.items() if k!='sites'}
    row=copy.deepcopy(source)
    a=dict(escape=copy.deepcopy(e),started_tick=920,deadline_tick=4520,observed_tick=920,
        finished_tick=None,reason=None,guidance='transfer',vehicle=0,destination=1,selected_tick=920,
        progress_tick=920,distance=math.hypot(1000.,200.),deferred_pursuit_ticks=1,
        last_deferred_tick=920,last_deferred_reason='responding to incoming fire',travel_ticks=1,laser_ticks=0,cannon_ticks=0)
    row['escape_travel']=dict(telemetry=dict(attempts=1,capture_handoffs=0,timed_out=0,last=a),
        observation=row['capture_escape']['observation'],combat=None,pursuit=None)
    row['mission'].update(goal='transfer',target=1)
    return row,source


def advance(row):
    row=copy.deepcopy(row);p=sync(row);p['tick']+=1
    a=row['escape_travel']['telemetry']['last'];a['observed_tick']=p['tick']
    a['deferred_pursuit_ticks']+=1;a['last_deferred_tick']=p['tick'];a['travel_ticks']+=1
    sync(row);return row


class EscapeTravelTests(unittest.TestCase):
    def test_fresh_completed_escape_binds_transfer_clock_destination_and_deferral(self):
        row,source=fixture();a=T.audit_step(row,None,source)
        self.assertEqual(a['deadline_tick'],4520)
        after=advance(row);T.audit_step(after,a,source)

    def test_moved_clock_source_actor_destination_or_transfer_input_is_rejected(self):
        for change in range(8):
            row,source=fixture();a=row['escape_travel']['telemetry']['last'];p=sync(row)
            if change==0:a['deadline_tick']+=1
            elif change==1:a['escape']['abort']['measurement_tick']+=1
            elif change==2:p['owner']='player_2'
            elif change==3:a['selected_tick']+=1
            elif change==4:p['vehicle']+=1
            elif change==5:row['mission']['target']=0
            elif change==6:row['actions'][0]['Scenario']['payload'][5]=1
            else:row['escape_travel']['pursuit']=dict(started_tick=920)
            sync(row)
            with self.assertRaises(AssertionError):T.audit_step(row,None,source)

    def test_a_deferral_needs_the_current_native_pursuit_reason(self):
        row,source=fixture();a=copy.deepcopy(row['escape_travel']['telemetry']['last'])
        row=advance(row);T.audit_step(row,a,source)
        for change in range(3):
            bad=copy.deepcopy(row);w=bad['escape_travel'];c=w['observation']['local']['combat']
            if change==0:c['target']['visible']=False
            elif change==1:w['telemetry']['last']['last_deferred_tick']-=1
            else:w['telemetry']['last']['last_deferred_reason']='nearby vulnerable opponent'
            with self.assertRaises(AssertionError):T.audit_step(bad,a,source)

    def test_timeout_does_not_count_another_travel_tick_or_extend_source_escape(self):
        row,source=fixture();previous=copy.deepcopy(row['escape_travel']['telemetry']['last'])
        previous['observed_tick']=4519;previous['progress_tick']=4519
        p=sync(row);p['tick']=4520;a=row['escape_travel']['telemetry']['last']
        a.update(observed_tick=4520,finished_tick=4520,reason='transfer deadline',guidance='finished',progress_tick=4519)
        sync(row);T.audit_step(row,previous,source)
        a['deadline_tick']+=1
        with self.assertRaises(AssertionError):T.audit_step(row,previous,source)

    def test_capture_handoff_needs_current_arrival_and_neutral_controls(self):
        row,source=fixture();previous=copy.deepcopy(row['escape_travel']['telemetry']['last'])
        p=sync(row);p['tick']=921;p['planet']=copy.deepcopy(row['planets'][1])
        p['ship']['position']=dict(x=1000.,y=150.)
        a=row['escape_travel']['telemetry']['last'];a.update(observed_tick=921,finished_tick=921,reason='capture task started',guidance='finished')
        row['capture']=dict(started_tick=921);row['mission']['goal']='capture';sync(row)
        T.audit_step(row,previous,source)
        row['actions'][0]['Scenario']['payload'][4]=1
        with self.assertRaises(AssertionError):T.audit_step(row,previous,source)

    def test_defensive_weapons_use_existing_visibility_readiness_and_aim_audit(self):
        row,source=fixture();w=row['escape_travel'];p=sync(row);c=w['observation']['local']['combat']
        c['target']['motion']['position']=dict(x=0.,y=300.)
        w['combat']=dict(goal='engage ship');a=w['telemetry']['last'];a.update(laser_ticks=1,cannon_ticks=1)
        row['actions'][2]['Scenario']['payload']=[0,1,1]
        T.audit_step(row,None,source)
        c['target']['ground_occluded']=True
        with self.assertRaises(AssertionError):T.audit_step(row,None,source)

    def test_command_and_parity_preserve_escape_recovery_and_the_evaluated_seat(self):
        old=dict(item=dict(seat=1),command=['old','--out','old-dir','--capture-escape-seats','1','--actual-route-recovery-seats','1'])
        off=T.command(old,'binary','new',False)
        self.assertEqual(off[3:],old['command'][3:])
        self.assertEqual(T.command(old,'binary','new',True),off+['--escape-travel-seats','1'])
        row,source=fixture();plain=T.without_option(row)
        self.assertNotIn('escape_travel',plain)
        self.assertEqual(plain['capture_escape'],row['capture_escape'])
        self.assertEqual(plain['actions'],row['actions'])


if __name__=='__main__':unittest.main()
