import copy
import importlib.util
from pathlib import Path
import unittest
from test_initial_cover import fixture as initial_fixture

spec = importlib.util.spec_from_file_location('recovery',Path(__file__).parents[1]/'validate-actual-recovery.py')
R = importlib.util.module_from_spec(spec); spec.loader.exec_module(R)


def fixture():
    row,local,_ = initial_fixture()
    p = local['combat']['recovery']['flight']['pilot']
    p.update(tick=200,transfer='ready',hatch=dict(x=1.,y=80.),boarding_hatches=[dict(x=1.,y=80.),None])
    p['ship']['angle'] = 0.; p['planet']['motion']['angle'] = 0.
    p['landing'].update(phase='landed',supported_feet=2)
    local['combat']['recovery']['flight']['flight'] = dict(enabled=True)
    row['pilot'] = {k:v for k,v in p.items() if k!='sites'}
    objective = dict(planet=0,revision=0,owner='player_2',position=dict(x=20.,y=80.),range=2.8)
    attempt = dict(actor='player_1',reason='arrival_window',pose=dict(
        vehicle=dict(x=0.,y=80.),angle=0.,exit=p['hatch'],boarding_hatches=p['boarding_hatches']))
    e = dict(tick=200,objective=objective,source_objective=objective,generation=29,
        request_tick=180,measurement_tick=180,invalidated_by=None,submission_deferred_by=None,
        actual_local_failure=attempt)
    row['objective_evidence'] = local['objective_evidence'] = e
    row['objective_work'] = local['objective_work'] = 'pending'
    abort = dict(tick=200,generation=29,request_tick=180,measurement_tick=180,objective=objective,attempt=attempt)
    row['capture'].update(failed_tick=200,completed_tick=None,failure=R.REASON,goal='blocked',
        landing=dict(claimed_tick=None),actual_route_recovery=dict(abort=abort))
    row['capture']['acquisition']['reason'] = 'actual_local_attempt_failed'
    row['actual_route_recovery'] = row.pop('initial_cover')
    row['actions'] = [dict(Scenario=dict(payload=[0]*8))]
    source = copy.deepcopy(row); source['pilot']['tick'] = 180
    return row,source


class ActualRecoveryTests(unittest.TestCase):
    def test_completed_attempt_binds_source_pose_clock_and_neutral_abort(self):
        row,source = fixture()
        self.assertEqual(R.audit_receipt(row,source)['reason'],'arrival_window')
        self.assertEqual(R.audit_abort(row)['tick'],200)

    def test_moved_or_old_receipts_do_not_bind(self):
        for change in range(8):
            row,source = fixture(); e = row['objective_evidence']
            if change == 0: e['request_tick'] = 181
            elif change == 1: e['invalidated_by'] = 'hatch_moved'
            elif change == 2: e['actual_local_failure']['actor'] = 'player_2'
            elif change == 3: e['actual_local_failure']['pose']['angle'] = .01
            elif change == 4: row['pilot']['ship']['position']['x'] += .01
            elif change == 5: e['source_objective'] = dict(e['source_objective'],revision=1)
            elif change == 6: e['tick'] -= 1
            else: row['pilot']['tick'] = e['tick'] = 301
            with self.assertRaises(AssertionError): R.audit_receipt(row,source)

    def test_aborting_cannot_press_transfer_or_renew_its_measurement(self):
        for change in range(3):
            row,_ = fixture()
            if change == 0: row['actions'][0]['Scenario']['payload'][6] = 1
            elif change == 1: row['capture']['actual_route_recovery']['abort']['measurement_tick'] = 200
            else: row['capture']['failed_tick'] = 201
            with self.assertRaises(AssertionError): R.audit_abort(row)

    def test_command_only_adds_the_evaluated_seat_option(self):
        old = dict(item=dict(seat=1),command=['old','--out','old-dir','--covered-request-handoff-seats','1'])
        off = R.command(old,'frozen','new',False)
        self.assertEqual(off[3:],old['command'][3:])
        self.assertEqual(R.command(old,'frozen','new',True),off+['--actual-route-recovery-seats','1'])

    def test_parity_strips_only_the_new_optional_records(self):
        row,_ = fixture(); preserved = copy.deepcopy(row)
        plain = R.without_option(row)
        self.assertEqual(row,preserved)
        self.assertNotIn('actual_local_failure',plain['objective_evidence'])
        self.assertNotIn('actual_route_recovery',plain['capture'])
        self.assertEqual(plain['actions'],row['actions'])
        self.assertEqual(plain['capture']['failure'],R.REASON)


if __name__ == '__main__': unittest.main()
