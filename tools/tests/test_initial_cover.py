import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('initial',Path(__file__).parents[1]/'validate-initial-cover.py')
I = importlib.util.module_from_spec(spec)
spec.loader.exec_module(I)


def fixture():
    site = dict(id=dict(planet=0,bearing=1),revision=0,
                vehicle_position=dict(x=0.,y=62.),normal=dict(x=0.,y=1.))
    p = dict(tick=12,owner='player_1',controls_armed=True,queries_ready=True,
        ship_available=True,ship_form='ship',location=dict(aboard=0),
        landing=dict(phase='flying',supported_feet=0),site_query='survey',sites=[site],
        ship=dict(position=dict(x=0.,y=80.),velocity=dict(x=0.,y=0.)),
        planet=dict(index=0,revision=0,radius=60.,motion=dict(position=dict(x=0.,y=0.),velocity=dict(x=0.,y=0.),spin=0.)))
    cover = [dict(site=site['id'],grounded=True,approach=True,departure=False)]
    local = dict(combat=dict(recovery=dict(flight=dict(pilot=p)),
        target=dict(ground_occluded=False,motion=dict(position=dict(x=100.,y=80.)))),
        cover=cover,sun=None,planet_orbit_omega=None,landing_objective=None,objective_work=None,objective_evidence=None)
    initial = dict(armed_tick=12,finished_tick=None,outcome=None,selected_site=None,
        selected_exposed=None,last_seed_tick=12,requests=1,last_request=dict(tick=12,site=site['id']))
    search = dict(planet=0,revision=0,seeded=True,pending=[site['id']])
    cap = dict(site=None,initial_cover=initial,cover_response=dict(failures=0,required_since=12,search=search),
        acquisition=dict(required_site=None,reason='cover_evidence_pending'),objective_route=None,solar=None)
    row = dict(pilot={k:v for k,v in p.items() if k!='sites'},seat=0,capture=cap,
        cover=cover,landing_objective=None,objective_work=None,objective_evidence=None,planets=[],match_context=None,
        initial_cover=dict(observation=dict(local=local,planets=[],match_context=None)))
    return row,local,site


class InitialCoverTests(unittest.TestCase):
    def test_early_request_binds_to_observed_covered_geometry_without_a_route(self):
        row,local,site=fixture()
        seed=I.audit_event(row,0,None)
        self.assertEqual(seed['ids'],[site['id']])
        self.assertIsNone(row['landing_objective'])

    def test_seed_rejects_missing_cover_and_solar_hazards(self):
        for mutation in range(3):
            row,local,site=fixture()
            if mutation==0:local['cover'].clear()
            elif mutation==1:local['cover'][0]['grounded']=False
            else:local['sun']=dict(position=dict(x=0.,y=0.),radius=100000.,heat_radius=100024.)
            with self.assertRaises(AssertionError):I.audit_event(row,0,None)

    def test_first_choice_requires_current_cover_even_for_a_previously_qualified_site(self):
        row,local,site=fixture();cap=row['capture'];initial=cap['initial_cover']
        initial.update(finished_tick=12,outcome='selected_site',selected_site=site['id'],
            selected_exposed=True,last_seed_tick=None,requests=0,last_request=None)
        cap.update(site=site['id']);cap['acquisition']['reason']='selected_site'
        I.audit_event(row,0,None)
        local['cover'].clear()
        with self.assertRaises(AssertionError):I.audit_event(row,0,None)

    def test_probe_advance_must_be_from_its_seed_and_consume_exactly_one_request(self):
        row,local,site=fixture();initial=row['capture']['initial_cover'];initial['last_seed_tick']=11
        initial['armed_tick']=11
        seed=dict(tick=11,ids=[site['id']],planet=0,revision=0)
        I.audit_event(row,0,seed)
        for bad in [None,dict(seed,ids=[]),dict(seed,revision=1)]:
            with self.assertRaises(AssertionError):I.audit_event(row,0,bad)
        with self.assertRaises(AssertionError):I.audit_event(row,1,seed)
        initial['requests']=9
        with self.assertRaises(AssertionError):I.audit_event(row,8,seed)

    def test_witness_cannot_substitute_a_different_consumed_observation(self):
        row,local,site=fixture()
        row['pilot']=copy.deepcopy(row['pilot']);row['pilot']['tick']+=1
        with self.assertRaises(AssertionError):I.witness_observation(row)

    def test_low_height_exception_is_local_and_removes_planet_motion(self):
        row,local,site=fixture();p=local['combat']['recovery']['flight']['pilot']
        local['cover'][0]['approach']=False
        p['planet']['motion']['spin']=.5;p['planet']['motion']['velocity']['y']=3.
        p['ship']['velocity']=dict(x=-40.,y=3.)
        self.assertTrue(I.qualifies(local,site))
        p['ship']['velocity']['x']=0.
        self.assertFalse(I.qualifies(local,site))
        p['ship']['position']=dict(x=0.,y=-80.);p['ship']['velocity']=dict(x=40.,y=3.)
        self.assertFalse(I.qualifies(local,site))

    def test_command_preserves_health_physics_and_only_adds_initial_option(self):
        old=dict(item=dict(seat=0),command=['old','--out','old-dir','--seconds','600',
            '--pursuit-health-seats','0','--cover-response-seats','0'])
        off=I.command(old,'binary','new-dir',False)
        on=I.command(old,'binary','new-dir',True)
        self.assertEqual(on,off+['--initial-cover-seats','0'])
        self.assertEqual(off[3:],old['command'][3:])
        self.assertEqual(old['command'][0],'old')

    def test_plan_keeps_cover_off_controls_and_health_regressions_separate(self):
        plan=I.plan()
        self.assertEqual(len(plan),17)
        self.assertEqual(sum(p['source']=='retained' for p in plan),14)
        self.assertEqual([p['item']['name'] for p in plan if p['source']=='health'],I.HEALTH_CASES)
        self.assertEqual([p['name'] for p in plan if not p['enabled']],
            ['shared-failure-cover-off-walking','shared-failure-cover-off-powered'])
        self.assertEqual(len({p['name'] for p in plan}),17)


if __name__ == '__main__':
    unittest.main()
