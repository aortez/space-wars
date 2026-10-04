import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('health',Path(__file__).parents[1]/'validate-pursuit-health.py')
H = importlib.util.module_from_spec(spec)
spec.loader.exec_module(H)


def row():
    return dict(seat=0,pilot=dict(tick=200,owner='player_1',ship_form='ship',ship_available=True,
        controls_armed=True,actor=None,location=dict(aboard=0),ship_health=54.,ship=dict(position=dict(x=0.,y=0.))),
        planets=[dict(claim=dict(owner='player_1'))],capture=None,
        pursuit_health=dict(telemetry=dict(checks=1,admitted=0,deferred=1,
            last=dict(tick=200,opponent='player_2',own_hull=54.,opponent_hull=94.,decision='weaker_hull')),
            target=dict(owner='player_2',visible=True,ship_form='ship',health=94.,health_fraction=.94,
                        motion=dict(position=dict(x=100.,y=0.))),last_hit_taken_tick=None))


class PursuitHealthTests(unittest.TestCase):
    def test_current_discretionary_denial_is_bound_to_observed_hull_and_counters(self):
        r = row()
        previous = dict(checks=0,admitted=0,deferred=0)
        self.assertEqual(H.audit_check(r,previous),r['pursuit_health']['telemetry'])
        for field,value in [('own_hull',95.),('opponent_hull',20.),('tick',199),('decision','admitted')]:
            bad = copy.deepcopy(r)
            bad['pursuit_health']['telemetry']['last'][field] = value
            with self.assertRaises(AssertionError): H.audit_check(bad,previous)
        bad = copy.deepcopy(r);bad['pursuit_health']['telemetry']['checks'] = 2
        with self.assertRaises(AssertionError): H.audit_check(bad,previous)

    def test_defensive_vulnerable_hidden_and_unowned_contexts_cannot_be_counted(self):
        previous = dict(checks=0,admitted=0,deferred=0)
        for kind in range(4):
            r = row()
            if kind==0:r['pursuit_health']['last_hit_taken_tick']=199
            if kind==1:r['pursuit_health']['target']['health_fraction']=.49
            if kind==2:r['pursuit_health']['target']['visible']=False
            if kind==3:r['planets'][0]['claim']['owner']=None
            with self.assertRaises(AssertionError): H.audit_check(r,previous)

    def test_declined_pursuit_can_start_a_new_capture_but_cannot_interrupt_one(self):
        r = row();previous=dict(checks=0,admitted=0,deferred=0)
        r['capture']=dict(acquisition_wait=None);r['mission']=dict(goal='capture')
        H.audit_check(r,previous)
        with self.assertRaises(AssertionError): H.audit_check(r,previous,r['capture'])
        committed=r['capture'];r['capture']=None
        with self.assertRaises(AssertionError): H.audit_check(r,previous,committed)

    def test_equal_hull_admission_and_expired_incoming_fire_are_valid(self):
        r = row();r['pilot']['ship_health']=94.
        h=r['pursuit_health']['telemetry'];h.update(admitted=1,deferred=0)
        h['last'].update(own_hull=94.,decision='admitted')
        r['pursuit_health']['last_hit_taken_tick']=20
        self.assertEqual(H.audit_check(r,dict(checks=0,admitted=0,deferred=0)),h)

    def test_each_case_changes_only_the_selected_seats_explicit_option(self):
        plan=H.C.P.plan()
        self.assertEqual(len(plan),14)
        self.assertEqual(sum(p['kind']=='armed' for p in plan),8)
        for item in plan:
            old=dict(item=item,command=['old','--out','old-out','--active-flight-checks','true'])
            disabled=H.command(old,'new','new-out',False)
            enabled=H.command(old,'new','new-out',True)
            self.assertEqual(disabled,['new','--out','new-out','--active-flight-checks','true'])
            self.assertEqual(enabled,disabled+['--pursuit-health-seats',str(item['seat'])])

    def test_comparison_omits_only_gate_telemetry_and_keeps_controls_and_hull(self):
        a=dict(actions=[1],pilot=dict(ship_health=54),mission=dict(goal='transfer'))
        b=copy.deepcopy(a);b['pursuit_health']={};b['mission']['pursuit_health']={}
        self.assertEqual(H.without_gate(b),a)
        b['actions']=[0]
        self.assertNotEqual(H.without_gate(b),a)
        b['actions']=[1];b['pilot']['ship_health']=55
        self.assertNotEqual(H.without_gate(b),a)


if __name__ == '__main__':
    unittest.main()
