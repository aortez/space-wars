import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('walk',Path(__file__).parents[1]/'validate-rebuild-staging-walk.py')
W=importlib.util.module_from_spec(spec);spec.loader.exec_module(W)


class StagingWalkTest(unittest.TestCase):
    def test_three_modes_change_only_the_two_execution_switches(self):
        prior={'commands':{'staged':['old','--out','old','--rebuild-refinement','true','--rebuild-staging','true']}}
        commands={n:W.command(prior,Path('/binary'),Path('/raw'),n) for n in W.MODES}
        self.assertEqual(len(commands),3)
        for name,(walk,handoff) in W.MODES.items():
            self.assertEqual(commands[name][:-4],commands['control'][:-4])
            self.assertEqual(commands[name][-4:],['--rebuild-staging-walk',str(walk).lower(),
                '--rebuild-staging-handoff',str(handoff).lower()])

    def test_audit_repair_cannot_change_runtime_or_plan(self):
        original={p:'original' for p in (*W.NEW,*W.OWN)}
        for path in W.OWN[:2]:
            W.check_inputs(original,dict(original,**{path:'repaired'}),True)
        for path in (*W.NEW,W.OWN[2]):
            with self.assertRaises(AssertionError):
                W.check_inputs(original,dict(original,**{path:'changed'}),True)
        with self.assertRaises(AssertionError):W.check_inputs(original,{},True)


if __name__=='__main__':unittest.main()
