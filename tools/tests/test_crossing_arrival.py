import copy
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('arrival',Path(__file__).resolve().parents[1]/'validate-crossing-arrival.py')
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)


def source():
    return dict(complete=True,profile=A.L.PROFILE,inputs={A.RUNTIME:'old','physics.rs':'same'},
        binary=dict(path='old',sha256='baseline'),runs={n:dict(item=dict(name=n,seat=1 if 'world1' in n else 0),
            command=['old','--seat','0','--out','/old/'+n,'--trace-impact','true']) for n in A.ORDER})


class CrossingArrivalTests(unittest.TestCase):
    def test_jobs_change_only_binary_and_output_and_preserve_control_order(self):
        s=source();jobs=A.jobs(s,Path('/new'),Path('/out'))
        self.assertEqual([j['item']['name'] for j in jobs],list(A.ORDER))
        self.assertEqual([j['item']['role'] for j in jobs],['retention']*3+['affected'])
        for j in jobs:
            n=j['item']['name'];a=dict(zip(s['runs'][n]['command'][1::2],s['runs'][n]['command'][2::2]))
            b=dict(zip(j['command'][1::2],j['command'][2::2]));a.pop('--out');b.pop('--out')
            self.assertEqual(a,b)

    def test_native_fixture_provenance_and_all_four_arrival_distances(self):
        f=json.loads((A.ROOT/A.NEW[1]).read_text());bundle=A.ROOT/f['source_bundle']
        self.assertEqual(A.P.digest(bundle),f['source_sha256'])
        with gzip.open(bundle,'rt') as stream:b=json.load(stream)
        rows=b['derived_diagnosis']['runs'][f['source_run']]['recovery_trace_witnesses']
        for sample in f['samples']:
            row=next(r for r in rows if r['tick']==sample['tick']);o=row['observation']['local']['combat']['recovery']
            self.assertEqual(sample['pilot'],{k:o['flight']['pilot'][k] for k in sample['pilot']})
            self.assertEqual(sample['plan'],row['mission']['recovery']['ground']['crossing']['plan'])
            self.assertEqual(sample['jetpack'],{k:o['jetpack'][k] for k in sample['jetpack']})
            self.assertEqual(A.footing(sample['pilot'],sample['plan'])['distance']<1,sample['expected_arrival'])

    def test_prefix_requires_original_observation_and_the_known_false_completion(self):
        f=json.loads((A.ROOT/A.NEW[1]).read_text())['samples'][3]
        g=dict(started_tick=18278,goal='survey',jetpack_crossings=4,
               crossing=dict(goal='Complete',plan=f['plan']))
        a=dict(tick=19963,seat=1,actions=['old'],mission=dict(recovery=dict(ground=g)),
               observation=dict(local=dict(combat=dict(recovery=dict(flight=dict(pilot=f['pilot']))))))
        b=copy.deepcopy(a);b['actions']=['new'];b['mission']['recovery']['ground']['crossing']['goal']='Descend'
        b['mission']['recovery']['ground']['jetpack_crossings']=3
        earlier=dict(tick=19963,seat=0,actions=[])
        result=A.prefix([earlier,a],[earlier,b]);self.assertEqual(result['exact_prefix_rows'],1)
        changed=copy.deepcopy(b);changed['observation']['local']['combat']['recovery']['flight']['pilot']['balanced']=False
        with self.assertRaises(AssertionError):A.prefix([a],[changed])
        with self.assertRaises(AssertionError):A.prefix([dict(a,tick=19962)],[dict(b,tick=19962)])

    def test_ground_audit_checks_both_capture_and_recovery_completion_footing(self):
        samples=json.loads((A.ROOT/A.NEW[1]).read_text())['samples']
        def row(seat,sample):
            scope=('capture','recovery')[seat]
            ground=dict(started_tick=0,goal='survey',last_progress_tick=0,
                crossing=dict(started_tick=0,completed_tick=0,goal='Complete',plan=sample['plan']))
            return dict(tick=0,seat=seat,mission={scope:dict(status='running',ground=ground)},
                observation=dict(local=dict(combat=dict(recovery=dict(
                    flight=dict(pilot=sample['pilot']),jetpack=dict(charge=1))))))
        result=A.ground_audit([row(0,samples[0]),row(1,samples[1])],1)
        self.assertEqual({a['scope'] for a in result['attempts'].values()},{'capture','recovery'})
        self.assertEqual(sum(a['completed_tick']==0 for a in result['attempts'].values()),2)
        for seat in (0,1):
            changed=[row(0,samples[0]),row(1,samples[1])];changed[seat]=row(seat,samples[3])
            with self.assertRaises(AssertionError):A.ground_audit(changed,1)
        with self.assertRaises(AssertionError):A.ground_audit([row(0,samples[0])],1)

    def test_replay_freeze_allows_only_the_single_runtime_change_and_auditor_repairs(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);old=root/'old';old.write_bytes(b'old');new=root/'new';new.write_bytes(b'new')
            s=source();s['binary']=dict(path=str(old),sha256=A.P.digest(old));path=root/'source.json';path.write_text(json.dumps(s))
            frozen={**s['inputs'],A.RUNTIME:'new',**{p:p for p in (*A.NEW,*A.OWN)}}
            plan=dict(profile=A.PROFILE,inputs=frozen,source=dict(path=str(path),sha256=A.P.digest(path)),
                baseline_binary=s['binary'],binary=dict(path=str(new),sha256=A.P.digest(new)),sources=s['runs'],
                jobs=A.jobs(s,new,root))
            with patch.object(A,'inputs',return_value=frozen):A.verify(plan,root)
            for key in (A.RUNTIME,A.NEW[1],A.OWN[2],'physics.rs'):
                with patch.object(A,'inputs',return_value={**frozen,key:'changed'}):
                    with self.assertRaises(AssertionError):A.verify(plan,root,True)
            with patch.object(A,'inputs',return_value={**frozen,A.OWN[0]:'audit fix'}):
                with self.assertRaises(AssertionError):A.verify(plan,root)
                A.verify(plan,root,True)

    def test_corrected_counter_alone_is_not_a_recovery_improvement(self):
        old=dict(completed_recoveries=0,pilot_deaths=0,outcome='win',ships_lost=1)
        s={A.TARGET:dict(players=[{},old])}
        for changed,expected in (({},False),({'completed_recoveries':1},True),
            ({'completed_recoveries':1,'outcome':'loss'},False),
            ({'completed_recoveries':1,'ships_lost':2},False),
            ({'completed_recoveries':1,'pilot_deaths':1},False)):
            r={A.TARGET:dict(players=[{},{**old,**changed}])}
            result=A.decision(r,s)
            self.assertEqual(result['decision']=='advance_to_broader_validation',expected)
            self.assertFalse(result['default_promotion'])


if __name__=='__main__':unittest.main()
