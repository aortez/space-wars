import copy
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('probe',Path(__file__).parents[1]/'probe-rebuild-placement.py')
P=importlib.util.module_from_spec(spec);spec.loader.exec_module(P)


class PlacementProbeTest(unittest.TestCase):
    def test_native_precision_comparison_covers_nested_attempt_arrays(self):
        a=dict(attempts=[dict(center=dict(x=29.005197525024414,y=-15.964317321777344))])
        b=copy.deepcopy(a);b['attempts'][0]['center']['x']=29.005197525024418
        self.assertTrue(P.same_native(a,b))
        b['attempts'][0]['center']['x']+=.00001
        self.assertFalse(P.same_native(a,b))

    def test_commands_only_add_the_read_only_request(self):
        for name in P.MODES:
            old=['old','--out','old','--rebuild-radial-placement',str(name=='radial_handoff').lower()]
            cmd=P.command({'commands':{name:old}},Path('/binary'),Path('/work/raw')/name,name)
            self.assertEqual(cmd,['/binary','--out','/work/raw/'+name,*old[3:],
                '--rebuild-placement-probe','/work/requests/'+name+'.json'])
            self.assertEqual(old[0],'old')

    def test_audit_repairs_cannot_change_probe_runtime_or_inputs(self):
        original={p:'old' for p in P.CHANGED}
        for p in P.OWN[:2]:P.check_inputs(original,dict(original,**{p:'repair'}),True)
        for p in set(original)-set(P.OWN[:2]):
            with self.assertRaises(AssertionError):P.check_inputs(original,dict(original,**{p:'repair'}),True)
        with self.assertRaises(AssertionError):P.check_inputs(original,{},True)

    def test_matrix_requires_every_independent_combination_and_matching_baseline(self):
        request=dict(tick=10,preview_tick=5,planet=0,revision=3,bearing=7,expected_offset=-8)
        report=dict(tick=10,planet=0,revision=3,standing=dict(x=3,y=4),selected_offset=None,
            attempts=[dict(offset=-8,rejection='no_ground')])
        anchor=dict(request=request,point=dict(x=3,y=4),report=dict(tick=5,revision=3,selected_offset=-8))
        cases=[dict(map=m,position=p,direction=d,report=report,query_point=dict(x=3,y=4),query_up=dict(x=.6,y=.8),pose=None)
            for m in ('actual','preview_extent') for p in ('actual','preview') for d in ('contact','preview','radial')]
        probe=dict(tick=10,seat=1,physics_unchanged=True,preview_only=True,anchor=anchor,cases=cases,
            actual_point=dict(x=3,y=4),radial_enabled=False,baseline=report,native_recovery=dict(placement=report))
        self.assertTrue(P.analyze(probe,request,anchor)['fresh_native_match'])
        for bad in (dict(probe,cases=cases[:-1]),dict(probe,physics_unchanged=False),dict(probe,tick=11)):
            with self.assertRaises(AssertionError):P.analyze(bad,request,anchor)
        bad=copy.deepcopy(probe);bad['baseline']=dict(report,attempts=[dict(offset=-8,rejection='no_hatch_route')])
        with self.assertRaises(AssertionError):P.analyze(bad,request,anchor)


if __name__=='__main__':unittest.main()
