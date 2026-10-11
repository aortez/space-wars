import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('refinement',Path(__file__).parents[1]/'validate-rebuild-refinement.py')
F=importlib.util.module_from_spec(spec);spec.loader.exec_module(F)


class RebuildRefinementTest(unittest.TestCase):
    def test_control_and_refined_commands_only_change_the_explicit_flag(self):
        prior={'commands':{'candidate':['old','--seed','17','--out','old','--rebuild-replay-end','29421']}}
        control=F.command(prior,Path('/bin'),Path('/raw'),False)
        refined=F.command(prior,Path('/bin'),Path('/raw'),True)
        self.assertEqual(control[:-1],refined[:-1])
        self.assertEqual(control[-2:],['--rebuild-refinement','false'])
        self.assertEqual(refined[-1],'true')
        self.assertEqual(control[1:7],['--seed','17','--out','/raw','--rebuild-replay-end','29421'])

    def test_resume_cannot_modify_the_controller_or_remove_a_frozen_file(self):
        old={'crates/spacewars-ai/src/recovery_task.rs':'runtime',F.OWN[0]:'audit'}
        F.check_inputs(old,dict(old,**{F.OWN[0]:'repaired'}),True)
        for new,resume in [({},True),(dict(old,**{F.OWN[0]:'repaired'}),False),
                           (dict(old,**{'crates/spacewars-ai/src/recovery_task.rs':'changed'}),True)]:
            with self.assertRaises(AssertionError):F.check_inputs(old,new,resume)


if __name__=='__main__':unittest.main()
