import copy
import importlib.util
from pathlib import Path
import unittest
from test_projectile_response import actions

spec=importlib.util.spec_from_file_location('response',Path(__file__).parents[1]/'validate-projectile-response.py')
R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)


def fixture():
    vector=lambda x,y:dict(x=x,y=y)
    motion=dict(position=vector(0.,0.),velocity=vector(30.,0.),angle=0.,spin=0.)
    frame=dict(position=vector(0.,0.),velocity=vector(0.,0.),angle=0.,spin=0.)
    p=dict(ship=motion,planet=dict(motion=frame),landing=dict(assist_strength=0.))
    f=dict(pilot=p,flight=dict(sweep=0.,wings_closed=False,limits=dict(thrust_acceleration=45.,
           turn_speed=1.8,turn_acceleration=6.,brake_acceleration=40.,brake_gain=4.,cruise_speed=70.)))
    o=dict(local=dict(combat=dict(recovery=dict(flight=f))))
    d=dict(observer_radius=8.,unavailable_shells=0,shells_in_range=1,capacity=64,
           projectiles=[dict(id=1,collision_radius=2.,relative_position=vector(100.,0.),relative_velocity=vector(-50.,0.))])
    a=actions();a[1]['Scenario']['payload'][1]=0
    return o,d,a


class BrakeSelectionTests(unittest.TestCase):
    def choose(self,o,d,a):return R.B.select(o,d,a,R.P.linear_approach)

    def test_approaching_and_receding_motor_directions_are_distinguished(self):
        o,d,a=fixture();self.assertEqual(self.choose(o,d,a)['action'],'brake')
        d['projectiles'][0]['relative_position']['x']=-100.
        d['projectiles'][0]['relative_velocity']['x']=50.
        self.assertEqual(self.choose(o,d,a)['action'],'observe')

    def test_rotation_and_native_thrust_are_part_of_the_comparison(self):
        o,d,a=fixture();p=o['local']['combat']['recovery']['flight']['pilot']
        p['ship']['position']['y']=100.;p['ship']['velocity']['x']=-50.;p['planet']['motion']['spin']=1.
        self.assertEqual(self.choose(o,d,a)['action'],'brake')
        p['ship']['angle']=1.5707963267948966;a[0]['Scenario']['payload'][4]=1
        self.assertEqual(self.choose(o,d,a)['action'],'observe')

    def test_short_warning_and_incomplete_diagnostics_abstain(self):
        o,d,a=fixture();d['projectiles'][0]['relative_position']['x']=35.
        self.assertEqual(self.choose(o,d,a)['action'],'brake')
        d['projectiles'][0]['relative_position']['x']=34.
        self.assertEqual(self.choose(o,d,a)['reason'],'warning shorter than pulse')
        d['unavailable_shells']=1
        self.assertEqual(self.choose(o,d,a)['reason'],'incomplete projectile sample')

    def test_conflicting_warning_cannot_be_hidden_by_projectile_order(self):
        o,d,a=fixture();second=copy.deepcopy(d['projectiles'][0]);second['id']=2
        second['relative_position']['x']=-100.;second['relative_velocity']['x']=50.
        d['projectiles'].append(second);d['shells_in_range']=2
        for _ in range(2):
            self.assertEqual(self.choose(o,d,a)['action'],'observe');d['projectiles'].reverse()


if __name__=='__main__':unittest.main()
