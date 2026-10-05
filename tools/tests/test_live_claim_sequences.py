import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('sequences', Path(__file__).resolve().parents[1] / 'diagnose-live-claim-sequences.py')
L = importlib.util.module_from_spec(spec); spec.loader.exec_module(L)


def source():
    runs = {name: dict(item=dict(name=name, seat=1 if 'world1' in name else 0),
        command=['original', '--seat', '0', '--seconds', '600', '--live-claim-stopping',
                 str(name.endswith('integrated')).lower(), '--out', '/original/'+name]) for name in L.NAMES}
    return dict(complete=True, profile=L.E.PROFILE, inputs={'runtime.rs':'same'},
        runtime_source_commit='runtime', binary=dict(path='original'), runs=runs)


def fixture():
    traces, impacts = [], []
    for tick in range(2):
        for seat in (0, 1):
            pilot=dict(tick=tick, owner=f'player_{seat+1}', ship_form='ship', ship={},
                location={'aboard':seat}, recovery=dict(ships_lost=0), planet=dict(index=0,motion={}),gravity={})
            flight=dict(limits=dict(brake_acceleration=40))
            trace=dict(tick=tick,seat=seat,actions=[seat,tick],controls={'brake':True},
                mission=dict(goal='capture',recovery=None),
                observation=dict(match_context=dict(pilot_health=[100,100]),
                    local=dict(combat=dict(recovery=dict(flight=dict(pilot=pilot,flight=flight))))))
            traces.append(trace)
            impacts.append(dict(schema=1,tick=tick,seat=seat,overridden=False,controls={'brake':True},
                bot_controls={'brake':True},actions=trace['actions'],goal='capture',form='ship',
                ship={},location=pilot['location'],vitals={'health':100},recovery=None,flight=flight,
                planet=dict(index=0,motion={}),gravity={},damage={}))
    final=dict(schema=1,final_tick=2,round={},damage=[{},{}],config=dict(seat=0,control='bot',
        control_from_tick=0,trace_start_tick=0,trace_end_tick=36001))
    impacts.append(final)
    report=dict(elapsed_ticks=2,round={},final_pilots=[dict(recovery=dict(ships_lost=0)) for _ in (0,1)])
    return impacts,traces,report


def receipt(tick):
    return dict(last_damage_tick=tick,last_ship_lost=True,last_source='cannon',last_damage_percent=10,
                last_contact_tick=None,last_contact_source=None)


class LiveClaimSequencesTests(unittest.TestCase):
    def test_commands_only_add_observation_and_keep_distinct_evaluated_and_harness_seats(self):
        prior=source();jobs=L.jobs(prior,Path('/binary'),Path('/out'))
        self.assertEqual([j['item']['name'] for j in jobs],list(L.NAMES))
        for job in jobs:
            name=job['item']['name'];cmd=job['command']
            self.assertEqual(cmd[1:len(prior['runs'][name]['command'])-1],prior['runs'][name]['command'][1:-1])
            self.assertEqual(dict(zip(cmd[-10::2],cmd[-9::2])),L.OBSERVER)
            self.assertEqual(cmd[cmd.index('--seat')+1],'0')
            self.assertEqual(job['item']['stage'],'selected_sequence_diagnosis')
        self.assertEqual([j['item']['seat'] for j in jobs],[1,1,0,0])

    def test_only_one_extra_raw_stream_is_allowed_and_original_actions_cannot_change(self):
        prior={'trace.jsonl':'actions','report.json':'old timing','sensors.jsonl':'old timing'}
        new={**prior,'report.json':'new timing','impact.jsonl':'observer'}
        self.assertEqual(set(L.original_hashes(new,prior)),set(prior))
        for changed in ({**new,'trace.jsonl':'changed'}, {**new,'extra.json':'unknown'},prior):
            with self.assertRaises(AssertionError):L.original_hashes(changed,prior)

    def test_observer_requires_full_dense_join_with_unchanged_controls(self):
        impacts,traces,report=fixture()
        result=L.observer_audit(impacts,traces,report,0)
        self.assertEqual(result['rows'],4);self.assertEqual(result['overrides'],0)
        self.assertEqual(result['losses'],[])
        mutations=[lambda rows:rows.pop(0),lambda rows:rows.pop(),
            lambda rows:rows.append(copy.deepcopy(rows[-1])),
            lambda rows:rows[0].update(overridden=True),lambda rows:rows[0].update(actions=[]),
            lambda rows:rows[0].update(bot_controls={}),lambda rows:rows[0].update(seat=1),
            lambda rows:rows[0]['vitals'].update(health=99),
            lambda rows:rows[0].update(flight={}),lambda rows:rows[0].update(recovery={})]
        for mutate in mutations:
            changed=copy.deepcopy(impacts);mutate(changed)
            with self.assertRaises(AssertionError):L.observer_audit(changed,traces,report,0)
        with self.assertRaises(AssertionError):L.observer_audit(impacts,traces,report,1)

    def test_native_final_step_loss_is_retained_and_requires_its_receipt(self):
        impacts,traces,report=fixture()
        report['final_pilots'][1]['recovery']['ships_lost']=1
        impacts[-1]['damage'][1]=receipt(2)
        result=L.observer_audit(impacts,traces,report,0)
        self.assertEqual(result['losses'],[dict(seat=1,receipt=dict(tick=2,source='cannon',amount=10))])
        impacts[-1]['damage'][1]['last_damage_tick']=1
        with self.assertRaises(AssertionError):L.observer_audit(impacts,traces,report,0)

    def test_midstream_loss_requires_receipt_and_cannot_skip_a_loss(self):
        impacts,traces,report=fixture()
        L.X.R.pilot(traces[3])['recovery']['ships_lost']=1
        impacts[3]['damage']=receipt(1);report['final_pilots'][1]['recovery']['ships_lost']=1
        self.assertEqual(L.observer_audit(impacts,traces,report,0)['losses'][0]['receipt']['tick'],1)
        L.X.R.pilot(traces[3])['recovery']['ships_lost']=2
        with self.assertRaises(AssertionError):L.observer_audit(impacts,traces,report,0)

    def test_selected_archive_members_are_verified_against_both_hashes(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);archive=root/'evidence.tar.gz';data=b'original report'
            with tarfile.open(archive,'w:gz') as stream:
                member=tarfile.TarInfo('report.json');member.size=len(data);stream.addfile(member,io.BytesIO(data))
            record=dict(path=str(archive),sha256=L.P.digest(archive),files={'report.json':dict(bytes=len(data),sha256=hashlib.sha256(data).hexdigest())})
            L.extract(record,root/'copy',('report.json',));self.assertEqual((root/'copy/report.json').read_bytes(),data)
            changed=copy.deepcopy(record);changed['files']['report.json']['sha256']='wrong'
            with self.assertRaises(AssertionError):L.extract(changed,root/'bad',('report.json',))
            changed=copy.deepcopy(record);changed['sha256']='wrong'
            with self.assertRaises(AssertionError):L.extract(changed,root/'bad',('report.json',))

    def test_frozen_source_runtime_and_commands_cannot_change_on_audit_resume(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);binary=root/'binary';binary.write_bytes(b'qualified')
            prior=source();prior['binary']=dict(path=str(binary),sha256=L.P.digest(binary))
            path=root/'source.json';path.write_text(json.dumps(prior))
            frozen={**prior['inputs'],L.OWN[0]:'runner',L.OWN[2]:'plan'}
            plan=dict(profile=L.PROFILE,inputs=frozen,binary=prior['binary'],runtime_source_commit='runtime',
                source=dict(path=str(path),sha256=L.P.digest(path)),sources=prior['runs'],jobs=L.jobs(prior,binary,root))
            with patch.object(L.E,'inputs',return_value=prior['inputs']),patch.object(L,'inputs',return_value=frozen):
                L.verify(plan,root)
                for change in (lambda p:p['jobs'][0]['command'].extend(['--override','true']),
                               lambda p:p.update(runtime_source_commit='other'),lambda p:p['sources'].clear()):
                    bad=copy.deepcopy(plan);change(bad)
                    with self.assertRaises(AssertionError):L.verify(bad,root,True)
            with patch.object(L.E,'inputs',return_value=prior['inputs']):
                for key in ('runtime.rs',L.OWN[2]):
                    with patch.object(L,'inputs',return_value={**frozen,key:'changed'}):
                        with self.assertRaises(AssertionError):L.verify(plan,root,True)
                with patch.object(L,'inputs',return_value={**frozen,L.OWN[0]:'fixed auditor'}):
                    with self.assertRaises(AssertionError):L.verify(plan,root)
                    L.verify(plan,root,True)

    def test_braking_geometry_uses_native_center_of_mass_not_vehicle_origin(self):
        v=lambda x,y:dict(x=x,y=y)
        row=dict(motion=dict(center_of_mass=v(0,0),velocity=v(10,0),boundary_center=v(0,0),
            boundary_radius=100,spin=0),ship=dict(velocity=v(10,50)),
            planet=dict(motion=dict(position=v(0,0),velocity=v(0,0),spin=0)),
            flight=dict(limits=dict(brake_acceleration=40,turn_acceleration=6)))
        metrics=L.D.B.motion_metrics(row)
        self.assertEqual(metrics['ideal_braking_distance'],1.25)
        self.assertEqual(metrics['distance_to_wall_along_velocity'],100)
        self.assertEqual(metrics['origin_velocity_difference'],50)
        row['ship']['velocity']=v(999,999)
        self.assertEqual(L.D.B.motion_metrics(row)['ideal_braking_distance'],1.25)


if __name__ == '__main__':unittest.main()
