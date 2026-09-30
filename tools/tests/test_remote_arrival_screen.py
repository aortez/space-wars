import copy
import importlib.util
import math
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location('remote_arrival', Path(__file__).parents[1]/'screen-remote-arrivals.py')
A = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(A)


def fixture():
    def v(x, y): return dict(x=x, y=y)
    def motion(x, y, angle=0., spin=0., velocity=None):
        return dict(position=v(x,y), velocity=velocity or v(0.,0.), angle=angle, spin=spin)
    source_motion = motion(0.,0.)
    planet = dict(index=0, motion=source_motion, radius=10., revision=1, claim=dict(owner=None, flag=None))
    site = dict(id=dict(planet=0,bearing=0), revision=1, local_position=v(0.,10.), position=v(0.,10.),
                normal=v(0.,1.), velocity=v(0.,0.), vehicle_position=v(0.,16.), hatch_position=v(0.,11.),
                boarding_hatches=[v(0.,11.),None], hatch_has_settling_margin=True)
    measurement = dict(tick=10, revision=1, planet=source_motion, ship_form='ship', opponent=None,
                       finding='measured', site=site, cover=None, climb_clear=True)
    candidate = dict(id=site['id'], status='stale', reason='historical', measurement=measurement)
    remote = dict(generation=9, candidates=[candidate])
    arrival_planet = copy.deepcopy(planet)
    arrival_planet['motion'] = motion(100.,0.,math.pi/2,.5,v(1.,2.))
    ship = motion(100.,100.)
    arrival = dict(tick=12,ship=ship,planet=arrival_planet,sun=None,planet_orbit_omega=None)
    projected = dict(site,position=v(90.,0.),normal=v(-1.,0.),vehicle_position=v(84.,0.),
                     hatch_position=v(89.,0.),boarding_hatches=[v(89.,0.),None],velocity=v(1.,-6.))
    screen = dict(model='remote_arrival_screen_v1',source_tick=11,destination=0,generation=9,
                  source_planet=planet,arrival=arrival,sites=[dict(source=candidate,arrival_age_ticks=2,
                  projected=projected,directions=[dict(side=d,solar=None,solar_clear=True) for d in [-1,1]],unknown=None)],
                  charged_graph=2,complete=True,unknown=None,
                  acquisition='requires fresh native survey; choice and duration unknown',
                  future_threat='unmodeled; source cover and opponent are historical only')
    source = dict(source_tick=11,seat=0,remote_source=remote,environment=dict(planets=[dict(orbit=None)]))
    forecast = dict(destination=0,source_capture=None,forecast=dict(end='kinematic_handoff',ticks=1,
                    samples=[dict(ship=ship,target=arrival_planet['motion'])]))
    row = dict(observation=dict(planets=[planet],local=dict(sun=None,combat=dict(recovery=dict(flight=dict(pilot={}))))))
    history = [dict(actor=0,tick=10,evidence=remote)]
    return screen,source,forecast,row,history


class RemoteArrivalAuditTests(unittest.TestCase):
    def test_projected_geometry_and_source_witness(self):
        self.assertEqual(A.audit_screen(*fixture())['directions'],2)

    def test_mutations_cannot_borrow_future_geometry_or_hide_work(self):
        for change in ['velocity','normal','hatch','age','witness','orbit','work','future_threat']:
            with self.subTest(change=change):
                args = fixture()
                screen,source,_,_,history = args
                projected = screen['sites'][0]['projected']
                if change == 'velocity': projected['velocity']['y'] = -3.
                elif change == 'normal': projected['normal']['x'] = 1.
                elif change == 'hatch': projected['boarding_hatches'][0] = None
                elif change == 'age': screen['sites'][0]['arrival_age_ticks'] = 1
                elif change == 'witness': history[0]['tick'] = 11
                elif change == 'orbit': source['environment']['planets'][0]['orbit'] = dict(rate=.5)
                elif change == 'work': screen['charged_graph'] = 1
                else: screen['future_threat'] = 'safe'
                with self.assertRaises(AssertionError): A.audit_screen(*args)

    def test_stripping_only_removes_screen_and_its_charges(self):
        report = dict(charged_graph=12,candidates=[dict(destination=0,forecast=dict(charged_graph=9),
                       remote_arrival=dict(charged_graph=2))])
        stripped = A.strip_screen(report)
        self.assertEqual(stripped,dict(charged_graph=10,candidates=[dict(destination=0,forecast=dict(charged_graph=9))]))
        self.assertIn('remote_arrival',report['candidates'][0])


if __name__ == '__main__': unittest.main()
