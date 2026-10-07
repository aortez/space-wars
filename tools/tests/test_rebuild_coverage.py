import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('coverage',Path(__file__).parents[1]/'probe-rebuild-coverage.py')
C=importlib.util.module_from_spec(spec);spec.loader.exec_module(C)


class RebuildCoverageTest(unittest.TestCase):
    def test_probe_preserves_replay_bounds_and_configuration(self):
        old=['binary','--seed','17','--out','old','--rebuild-replay-end','29421']
        cmd=C.command({'commands':{'candidate':old}},Path('/new'),Path('/raw'))
        self.assertEqual(cmd,['/new','--seed','17','--out','/raw','--rebuild-replay-end','29421','--rebuild-coverage-tick','24330'])
        self.assertEqual(old[0],'binary')

    def test_audit_repair_cannot_alter_runtime_or_remove_a_frozen_input(self):
        old={C.NATIVE:'native',C.OWN[0]:'audit'}
        C.check_inputs(old,dict(old,**{C.OWN[0]:'repair'}),True)
        for new,resume in [({C.NATIVE:'native'},True),(dict(old,**{C.NATIVE:'changed'}),True),
                           (dict(old,**{C.OWN[0]:'repair'}),False)]:
            with self.assertRaises(AssertionError):C.check_inputs(old,new,resume)

    def test_a_coverage_result_requires_the_retained_scene_and_read_only_receipt(self):
        for report in ({'tick':0},{'tick':24330,'seat':0},{'tick':24330,'seat':1,'physics_unchanged':False}):
            with self.assertRaises(AssertionError):C.analyze(report)


if __name__=='__main__':unittest.main()
