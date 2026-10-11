import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('hold',Path(__file__).parents[1]/'validate-rebuild-footing-hold.py')
H=importlib.util.module_from_spec(spec);spec.loader.exec_module(H)


class FootingHoldTest(unittest.TestCase):
    def test_each_candidate_changes_only_holding_from_its_matched_control(self):
        common=['old','--out','old','--rebuild-refinement','true','--rebuild-staging','true',
            '--rebuild-staging-walk','true','--rebuild-staging-handoff']
        prior={'commands':{n:common+[str(n=='handoff').lower()] for n in ('walk','handoff')}}
        for base in ('walk','handoff'):
            old=H.command(prior,Path('/binary'),Path('/raw'),'control_'+base)
            new=H.command(prior,Path('/binary'),Path('/raw'),'hold_'+base)
            self.assertEqual(old[:-1],new[:-1])
            self.assertEqual(old[-2:],['--rebuild-footing-hold','false'])
            self.assertEqual(new[-2:],['--rebuild-footing-hold','true'])

    def test_audit_resume_cannot_change_runtime_or_bounds(self):
        original={p:'old' for p in (*H.NEW,*H.OWN,*H.W.OWN[:2])}
        for p in H.OWN[:2]:H.check_inputs(original,dict(original,**{p:'repair'}),True)
        for p in (*H.NEW,H.OWN[2],*H.W.OWN[:2]):
            with self.assertRaises(AssertionError):H.check_inputs(original,dict(original,**{p:'changed'}),True)
        with self.assertRaises(AssertionError):H.check_inputs(original,{},True)


if __name__=='__main__':unittest.main()
