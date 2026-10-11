import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('footprint',Path(__file__).parents[1]/'probe-rebuild-footprint.py')
P=importlib.util.module_from_spec(spec);spec.loader.exec_module(P)


class FootprintProbeTest(unittest.TestCase):
    def test_probe_preserves_each_retained_direction_arrival_and_deadline(self):
        prior={'commands':{n:['old','--out','old','--rebuild-precise-arrival','true',
            '--rebuild-radial-placement',str(n=='precise_coarse').lower(),'--rebuild-replay-end','29421'] for n in P.MODES}}
        for name in P.MODES:
            cmd=P.command(prior,Path('/binary'),Path('/output'),name)
            self.assertEqual(cmd[0],'/binary');self.assertEqual(cmd[2],'/output')
            self.assertEqual(cmd[3:-2],prior['commands'][name][3:])
            self.assertEqual(cmd[-2:],['--rebuild-footprint-probe','true'])

    def test_reaudit_cannot_change_query_runtime_or_frozen_plan(self):
        old={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(old,dict(old,**{p:'repair'}),True)
        for p in set(old)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(old,dict(old,**{p:'repair'}),True)


if __name__=='__main__':unittest.main()
