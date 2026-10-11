import copy
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('radial',Path(__file__).parents[1]/'validate-rebuild-radial-placement.py')
F=importlib.util.module_from_spec(spec);spec.loader.exec_module(F)


class RadialPlacementTest(unittest.TestCase):
    def test_paired_commands_change_only_the_orientation_flag(self):
        prior={'commands':{base:['old','--out','old','--rebuild-contact-frame','true',
            '--rebuild-contact-probe','true','--retained-mode',base] for base,_ in F.MODES.values()}}
        for base in ('handoff','recheck_walk','recheck_handoff'):
            control=F.command(prior,Path('/binary'),Path('/raw'),'control_'+base)
            radial=F.command(prior,Path('/binary'),Path('/raw'),'radial_'+base)
            self.assertEqual(control[:-1],radial[:-1])
            self.assertEqual(control[-2:],['--rebuild-radial-placement','false'])
            self.assertEqual(radial[-2:],['--rebuild-radial-placement','true'])

    def test_audit_resume_cannot_change_runtime_or_bounds(self):
        original={p:'old' for p in (*F.CHANGED,*F.F.OWN)}
        for p in F.OWN[:2]:F.check_inputs(original,dict(original,**{p:'repair'}),True)
        for p in set(original)-set(F.OWN[:2]):
            with self.assertRaises(AssertionError):F.check_inputs(original,dict(original,**{p:'changed'}),True)
        with self.assertRaises(AssertionError):F.check_inputs(original,{},True)

    def test_direction_audit_rejects_facet_and_unnormalized_directions(self):
        report=dict(tick=10,standing=dict(x=3,y=4),radial_up=dict(x=.6,y=.8),selected_offset=None,attempts=[])
        self.assertLess(F.check_direction(report,True,5),.00001)
        for bad in (dict(x=0,y=1),dict(x=3,y=4),dict(x=float('nan'),y=.8)):
            with self.assertRaises(AssertionError):F.check_direction(dict(report,radial_up=bad),True,5)
        old={k:v for k,v in report.items() if k!='radial_up'}
        self.assertIsNone(F.check_direction(old,False,5))
        self.assertIsNone(F.check_direction(dict(old,tick=5),True,5))
        with self.assertRaises(AssertionError):F.check_direction(report,False,5)

    def test_direction_audit_preserves_landing_hatch_and_selection_requirements(self):
        attempt=dict(offset=-11,rejection=None,settling_angle_degrees=10,route=dict(failure=None,length=12))
        report=dict(tick=10,standing=dict(x=3,y=4),radial_up=dict(x=.6,y=.8),selected_offset=-11,attempts=[attempt])
        F.check_direction(report,True,5)
        for key,value in (('settling_angle_degrees',20),('route',dict(failure='blocked',length=12)),
                          ('route',dict(failure=None,length=25)),('rejection','no_ground')):
            changed=copy.deepcopy(report);changed['attempts'][0][key]=value
            with self.assertRaises(AssertionError):F.check_direction(changed,True,5)
        with self.assertRaises(AssertionError):F.check_direction(dict(report,selected_offset=8),True,5)


if __name__=='__main__':unittest.main()
