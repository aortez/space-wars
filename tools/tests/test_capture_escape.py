import copy
import importlib.util
from pathlib import Path
import unittest
from test_actual_recovery import fixture as abort_fixture

spec = importlib.util.spec_from_file_location('escape',Path(__file__).parents[1]/'validate-capture-escape.py')
E = importlib.util.module_from_spec(spec); spec.loader.exec_module(E)


def sync(row):
    o = row['capture_escape']['observation']; p = o['local']['combat']['recovery']['flight']['pilot']
    row['pilot'] = {k:v for k,v in p.items() if k != 'sites'}
    row['planets'] = o['planets']
    return p


def fixture():
    abort_row,_ = abort_fixture()
    row = copy.deepcopy(abort_row); o = row.pop('actual_route_recovery')['observation']
    c = o['local']['combat']; p = c['recovery']['flight']['pilot']
    p.update(tick=201,vehicle=0,gravity=dict(x=0.,y=-18.))
    p['planet']['claim'] = dict(owner='player_2',flag_interaction_range=3.,
        flag=dict(player='player_2',position=dict(x=20.,y=80.)))
    c['recovery']['flight']['flight']['limits'] = dict(brake_acceleration=40.)
    c['target'].update(owner='player_2',ship_form='ship',health=100.,visible=True)
    c['target']['motion']['velocity'] = dict(x=0.,y=0.)
    c.update(laser_available=True,cannon_ready=True)
    o.update(match_rules=True,boundary=dict(center=dict(x=0.,y=0.),radius=5000.),planets=[p['planet']])
    a = dict(abort=copy.deepcopy(abort_row['capture']['actual_route_recovery']['abort']),
        started_tick=201,deadline_tick=920,observed_tick=201,finished_tick=None,reason=None,guidance='arming',
        vehicle=0,opponent='player_2',direction=dict(x=0.,y=1.),estimated_min_range=100.,estimated_clearance=20.,
        range=100.,opening_speed=None,source_clearance=20.,clear_since=None,
        boundary=dict(active=False),controlled_ticks=0,laser_ticks=0,cannon_ticks=0)
    row['capture_escape'] = dict(telemetry=dict(attempts=1,separated=0,timed_out=0,last=a),observation=o,pursuit=None,combat=None)
    row['capture'] = None; row['mission'] = dict(goal='select')
    row['actions'] = [dict(Scenario=dict(payload=[0]*8)),dict(Scenario=dict(payload=[0,0])),dict(Scenario=dict(payload=[0,0,0]))]
    sync(row)
    return row,abort_row


def flying(row, tick=202, position=None, covered=False, fire=False):
    row = copy.deepcopy(row); p = sync(row); w = row['capture_escape']; a = w['telemetry']['last']
    p.update(tick=tick)
    p['ship']['position'] = position or dict(x=0.,y=200.)
    p['landing'].update(phase='flying',supported_feet=0)
    c = w['observation']['local']['combat']; c['target']['ground_occluded'] = covered
    if fire:
        c['target']['motion']['position'] = dict(x=p['ship']['position']['x'],y=p['ship']['position']['y']+100.)
        row['actions'][2]['Scenario']['payload'] = [0,1,1]
        w['combat'] = dict(goal='engage ship')
    a.update(observed_tick=tick,guidance='escape',controlled_ticks=a['controlled_ticks']+1,
        laser_ticks=a['laser_ticks']+int(fire),cannon_ticks=a['cannon_ticks']+int(fire))
    measured,clear = E.separation(w['observation'],p,a); a.update(measured)
    a['clear_since'] = (a['clear_since'] if a['clear_since'] is not None else tick) if clear else None
    row['mission']['goal'] = 'disengage'
    sync(row)
    return row


class CaptureEscapeTests(unittest.TestCase):
    def test_arm_binds_actual_abort_pose_and_original_clock(self):
        row,abort = fixture()
        self.assertEqual(E.audit_step(row,None,abort)['deadline_tick'],920)

    def test_wrong_source_or_unqualified_threat_cannot_arm(self):
        for change in range(9):
            row,abort = fixture(); w=row['capture_escape']; a=w['telemetry']['last']; p=sync(row)
            if change == 0: a['deadline_tick'] += 1
            elif change == 1: a['started_tick'] += 1
            elif change == 2: a['vehicle'] += 1
            elif change == 3: p['ship']['position']['x'] += .01
            elif change == 4: p['planet']['revision'] += 1
            elif change == 5: w['observation']['local']['combat']['target']['ground_occluded'] = True
            elif change == 6: w['observation']['local']['combat']['target']['owner'] = 'player_1'
            elif change == 7: row['actions'][0]['Scenario']['payload'][6] = 1
            else: a['abort']['measurement_tick'] = 201
            sync(row)
            with self.assertRaises(AssertionError): E.audit_step(row,None,abort)

    def test_controlled_tick_keeps_immutable_deadline_and_counts_once(self):
        arm,abort=fixture(); previous=copy.deepcopy(arm['capture_escape']['telemetry']['last'])
        row=flying(arm); E.audit_step(row,previous,abort)
        for change in range(5):
            bad=copy.deepcopy(row); a=bad['capture_escape']['telemetry']['last']
            if change == 0: a['deadline_tick'] += 1
            elif change == 1: a['direction']['x'] = .1
            elif change == 2: a['controlled_ticks'] += 1
            elif change == 3: bad['capture_escape']['pursuit'] = dict(started_tick=202)
            else: bad['actions'][0]['Scenario']['payload'][6] = 1
            with self.assertRaises(AssertionError): E.audit_step(bad,previous,abort)

    def test_separation_needs_sixty_ticks_and_current_cover_or_opening_range(self):
        row,abort=fixture()
        for tick in range(202,263):
            previous=copy.deepcopy(row['capture_escape']['telemetry']['last'])
            row=flying(row,tick,covered=True); a=row['capture_escape']['telemetry']['last']
            if tick == 262:
                a.update(finished_tick=tick,reason='separation established',guidance='finished',controlled_ticks=previous['controlled_ticks'])
            E.audit_step(row,previous,abort)
        self.assertEqual(a['clear_since'],202)
        for change in range(3):
            bad=copy.deepcopy(row); o=bad['capture_escape']['observation']
            if change == 0: o['local']['combat']['target']=None
            elif change == 1: o['local']['combat']['target']['ground_occluded']=False
            else: o['boundary']['radius']=230.
            with self.assertRaises(AssertionError): E.audit_step(bad,previous,abort)

    def test_defensive_shots_require_visibility_readiness_range_and_lead_alignment(self):
        row,abort=fixture(); previous=copy.deepcopy(row['capture_escape']['telemetry']['last'])
        row=flying(row,fire=True); E.audit_step(row,previous,abort)
        for change in range(4):
            bad=copy.deepcopy(row); c=bad['capture_escape']['observation']['local']['combat']
            if change == 0: c['target']['visible']=False
            elif change == 1: c['laser_available']=False
            elif change == 2: c['target']['motion']['position']['y'] += 200.
            else: c['target']['motion']['position']['x'] += 100.
            with self.assertRaises(AssertionError): E.audit_weapons(bad,bad['capture_escape']['observation']['local'],sync(bad))

    def test_command_preserves_old_recovery_and_adds_only_evaluated_seat(self):
        old=dict(item=dict(seat=1),command=['old','--out','old-dir','--actual-route-recovery-seats','1','--seconds','600'])
        off=E.command(old,'binary','new',False)
        self.assertEqual(off[3:],old['command'][3:])
        self.assertEqual(E.command(old,'binary','new',True),off+['--capture-escape-seats','1'])

    def test_parity_preserves_prior_failure_and_abort_evidence(self):
        arm,abort=fixture(); value=dict(arm=arm,abort=abort); original=copy.deepcopy(value)
        normalized=E.without_option(value)
        self.assertEqual(value,original)
        self.assertNotIn('capture_escape',normalized['arm'])
        self.assertEqual(normalized['abort'],abort)
        self.assertEqual(normalized['arm']['actions'],arm['actions'])


if __name__ == '__main__': unittest.main()
