import copy
import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('actual',Path(__file__).parents[1]/'probe-actual-landing.py')
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)


def probe():
    return dict(work=dict(graph=4,physics_queries=10),steps=14,max_steps=1_000_000,
        work_by_phase={'powered/direct_walk':dict(graph=3,physics_queries=10),
                       'powered/done':dict(graph=1,physics_queries=0)},
        transitions=[dict(phase='powered/direct_walk',steps=0,work=dict(graph=0,physics_queries=0)),
                     dict(phase='powered/done',steps=13,work=dict(graph=3,physics_queries=10))],
        stop='actual_positive',positive_actual=dict(site=None,
            outbound=dict(failure=None,partial=False),returning=dict(failure=None,partial=False)),
        complete_survey=None,final_phase='full/ground/0',source_measurements={},
        final_measurements={},final_flights={})


class ActualLandingProbeTests(unittest.TestCase):
    def test_work_sums_and_dispatch_lower_bound_are_independent_of_live_permissions(self):
        result=A.audit_work(probe())
        self.assertEqual(result['minimum_additional_dispatch_ticks'],1)
        self.assertEqual(result['stop'],'actual_positive')

    def test_wrong_totals_phase_charges_and_clocks_are_rejected(self):
        for mutation in range(4):
            p=probe()
            if mutation==0:p['work']['physics_queries']=9
            elif mutation==1:p['work_by_phase']['powered/done']['graph']=2
            elif mutation==2:p['transitions'][1]['steps']=14
            else:p['max_steps']=13
            with self.assertRaises(AssertionError):A.audit_work(p)

    def test_negative_partial_or_hypothetical_routes_are_not_actual_successes(self):
        for mutation in range(4):
            p=probe()
            if mutation==0:p['positive_actual']['outbound']['failure']='disconnected'
            elif mutation==1:p['positive_actual']['returning']['partial']=True
            elif mutation==2:p['positive_actual']['returning']=None
            else:p['positive_actual']['site']=dict(planet=0,bearing=1)
            with self.assertRaises(AssertionError):A.audit_work(p)

    def test_zero_limit_means_unknown_instead_of_negative(self):
        p=probe();p.update(work=dict(graph=0,physics_queries=0),steps=0,max_steps=0,
            work_by_phase={},transitions=p['transitions'][:1],stop='work_limit',positive_actual=None)
        self.assertEqual(A.audit_work(p)['stop'],'work_limit')
        p['stop']='complete_without_positive_actual'
        with self.assertRaises(AssertionError):A.audit_work(p)

    def test_command_preserves_the_entire_physical_configuration(self):
        old=dict(item=dict(seat=1),command=['frozen','--out','original','--covered-request-handoff-seats','1'])
        preserved=copy.deepcopy(old)
        off=A.command(old,'new','diagnostic',[])
        self.assertEqual(off,['new','--out','diagnostic',*old['command'][3:]])
        self.assertEqual(A.command(old,'new','diagnostic',[10,12]),off+
            ['--probe-actual-landing-seat','1','--probe-actual-landing-ticks','10,12'])
        self.assertEqual(old,preserved)

    def test_planet_rotation_is_removed_before_checking_hatch_angle(self):
        source=dict(ship=dict(angle=1.0),planet=dict(motion=dict(angle=.25)))
        together=dict(ship=dict(angle=1.5),planet=dict(motion=dict(angle=.75)))
        self.assertEqual(A.angular_change(source,together),0.)
        together['ship']['angle']+=.00021
        self.assertGreater(A.angular_change(source,together),.0001)


if __name__=='__main__':unittest.main()
