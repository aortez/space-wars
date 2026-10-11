import copy
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch

spec=importlib.util.spec_from_file_location('replay',Path(__file__).parents[1]/'validate-rebuild-replay.py')
R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)


class RebuildReplayTest(unittest.TestCase):
    def test_audit_resume_cannot_change_runtime_or_drop_inputs(self):
        expected={R.OWN[0]:'old',R.HARNESS[0]:'runtime'}
        R.check_inputs(expected,dict(expected,**{R.OWN[0]:'new'}),True)
        for current,reaudit in [(dict(expected,**{R.OWN[0]:'new'}),False),
                               (dict(expected,**{R.HARNESS[0]:'changed'}),True),({},True)]:
            with self.assertRaises(AssertionError):R.check_inputs(expected,current,reaudit)

    def test_prefix_reader_normalizes_json_plan_paths(self):
        seen=[]
        def rows(path):
            self.assertIsInstance(path,Path);seen.append(path);return iter([])
        with patch.object(R.D,'rows',side_effect=rows),self.assertRaises(AssertionError):
            R.audit_prefix(Path('/raw'),'/tape',{'prefix':{'failure':None}})
        self.assertIn(Path('/tape'),seen)

    def test_replay_preserves_source_configuration_and_adds_explicit_bounds(self):
        source={'runs':{R.S.A.TARGET:dict(command=['old','--seed','17','--out','old','--terrain-flight-forecast','true'])}}
        command=R.command(source,Path('/binary'),Path('/output'),Path('/tape'))
        self.assertEqual(command[:7],['/binary','--seed','17','--out','/output','--terrain-flight-forecast','true'])
        flags=dict(zip(command[1::2],command[2::2]))
        self.assertEqual(flags['--rebuild-replay-start'],'16820')
        self.assertEqual(flags['--rebuild-replay-handoff'],'23767')
        self.assertEqual(flags['--rebuild-replay-end'],'29421')
        self.assertEqual(len(flags),len(command[1::2]))

    def test_projection_keeps_motion_and_native_recovery_counters(self):
        original={'actor':{'x':3},'recovery':{'placement':None,'status':'ready','rebuilds':1}}
        metadata=copy.deepcopy(original);metadata['recovery']['placement']={'angle':3}
        self.assertEqual(R.project_pilot(original),R.project_pilot(metadata))
        for key,value in [('status','blocked'),('rebuilds',2)]:
            changed=copy.deepcopy(metadata);changed['recovery'][key]=value
            self.assertNotEqual(R.project_pilot(original),R.project_pilot(changed))
        metadata['actor']['x']=4
        self.assertNotEqual(R.project_pilot(original),R.project_pilot(metadata))


if __name__=='__main__':unittest.main()
