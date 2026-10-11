import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('support',Path(__file__).parents[1]/'validate-rebuild-support-alignment.py')
P=importlib.util.module_from_spec(spec);spec.loader.exec_module(P)


class SupportAlignmentTest(unittest.TestCase):
    def test_pairs_change_only_support_gate_and_remove_diagnostic_requests(self):
        prior={'commands':{n:['old','--out','old','--rebuild-footprint-probe','true',
            '--rebuild-precise-arrival','true','--rebuild-radial-placement',str(n=='precise_coarse').lower()] for n in ('precise_handoff','precise_coarse')}}
        for case in ('handoff','coarse'):
            a=P.command(prior,Path('/binary'),Path('/raw'),'control_'+case)
            b=P.command(prior,Path('/binary'),Path('/raw'),'support_'+case)
            self.assertEqual(a[:-1],b[:-1]);self.assertEqual((a[-1],b[-1]),('false','true'))
            self.assertNotIn('--rebuild-footprint-probe',a)
            self.assertEqual(a[a.index('--rebuild-precise-arrival')+1],'true')

    def test_reaudit_cannot_change_shared_native_threshold_or_runtime(self):
        old={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(old,dict(old,**{p:'repair'}),True)
        for p in set(old)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(old,dict(old,**{p:'repair'}),True)

    def test_audit_rejects_admitted_bad_feet_and_cannot_count_them_as_valid(self):
        attempt={'rejection':None,'support_alignments':[.63595,.99959]}
        report={'tick':25373,'attempts':[attempt]}
        with self.assertRaises(AssertionError):P.check_report(report,True,23767)
        attempt['rejection']='landing_unsupported'
        self.assertEqual(P.check_report(report,True,23767),(1,1))
        attempt['support_alignments']=[P.MIN_ALIGNMENT,P.MIN_ALIGNMENT]
        with self.assertRaises(AssertionError):P.check_report(report,True,23767)
        attempt['rejection']=None
        self.assertEqual(P.check_report(report,True,23767),(1,0))
        with self.assertRaises(AssertionError):P.check_report(report,False,23767)
        attempt.pop('support_alignments')
        with self.assertRaises(AssertionError):P.check_report(report,True,23767)
        report['tick']=23767
        self.assertEqual(P.check_report(report,True,23767),(0,0))


if __name__=='__main__':unittest.main()
