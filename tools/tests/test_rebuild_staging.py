import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('staging',Path(__file__).parents[1]/'validate-rebuild-staging.py')
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)


class RebuildStagingTest(unittest.TestCase):
    def test_only_staging_switch_differs_and_existing_refinement_stays_enabled(self):
        prior={'commands':{'refined':['old','--out','old','--rebuild-refinement','true','--seed','17']}}
        control=S.command(prior,Path('/binary'),Path('/raw'),False)
        staged=S.command(prior,Path('/binary'),Path('/raw'),True)
        self.assertEqual(control[:-1],staged[:-1])
        self.assertEqual(control[-2:],['--rebuild-staging','false'])
        self.assertEqual(staged[-1],'true')
        self.assertEqual(staged[3:5],['--rebuild-refinement','true'])

    def test_audit_resume_cannot_change_new_runtime_modules(self):
        original={S.NEW[0]:'native',S.NEW[1]:'task',S.OWN[0]:'audit'}
        S.check_inputs(original,dict(original,**{S.OWN[0]:'repair'}),True)
        for p in S.NEW:
            with self.assertRaises(AssertionError):
                S.check_inputs(original,dict(original,**{p:'changed'}),True)
        with self.assertRaises(AssertionError):S.check_inputs(original,{S.OWN[0]:'audit'},True)


if __name__=='__main__':unittest.main()
