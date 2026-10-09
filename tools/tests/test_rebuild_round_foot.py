import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('round_foot',Path(__file__).parents[1]/'probe-rebuild-round-foot.py')
P=importlib.util.module_from_spec(spec);spec.loader.exec_module(P)


class RoundFootProbeTest(unittest.TestCase):
    def test_keeps_rejected_gate_off_and_retains_controls(self):
        prior={'commands':{n:['old','--out','old','--rebuild-support-alignment','false',
            '--rebuild-radial-placement',str(n=='control_coarse').lower(),'--rebuild-replay-end','29421'] for n in P.MODES}}
        for name in P.MODES:
            cmd=P.command(prior,Path('/binary'),Path('/output'),name)
            self.assertEqual(cmd[:3],['/binary','--out','/output'])
            self.assertEqual(cmd[3:-2],prior['commands'][name][3:])
            self.assertEqual(cmd[-2:],['--rebuild-round-foot-probe','true'])
            prior['commands'][name][4]='true'
            with self.assertRaises(AssertionError):P.command(prior,Path('/binary'),Path('/output'),name)

    def test_uncertain_queries_cannot_be_classified_as_supported_or_unsupported(self):
        def foot(alignment,status='Converged',surface=True):
            return dict(up_alignment=alignment,status=status,retained_surface=surface,
                supports_first_contact=alignment>=P.S.MIN_ALIGNMENT if status=='Converged' and surface else None)
        for a,b,expected in [(foot(.8),foot(.9),True),(foot(.6),foot(.9),False),
                (foot(P.S.MIN_ALIGNMENT),foot(.9),True),(None,foot(.9),None),
                (foot(.9,'Penetrating'),foot(.9),None),(foot(.9,'Failed'),foot(.9),None),
                (foot(.9,'OutOfIterations'),foot(.9),None),(foot(.9,surface=False),foot(.9),None)]:
            self.assertEqual(P.first_contact_prediction(dict(feet=[a,b]),P.S.MIN_ALIGNMENT),expected)

    def test_reaudit_cannot_change_runtime_or_plan(self):
        old={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(old,dict(old,**{p:'repair'}),True)
        for p in set(old)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(old,dict(old,**{p:'repair'}),True)


if __name__=='__main__':unittest.main()
