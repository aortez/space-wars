import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('approach',Path(__file__).parents[1]/'probe-threatened-approach.py')
A=importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)


def fixture():
    site=dict(id=dict(planet=0,bearing=0),vehicle_position=dict(x=0.,y=62.),normal=dict(x=0.,y=1.))
    p=dict(tick=10,owner='player_1',ship=dict(position=dict(x=0.,y=80.),velocity=dict(x=-40.,y=3.)),
        planet=dict(index=0,radius=60.,motion=dict(position=dict(x=0.,y=0.),velocity=dict(x=0.,y=3.),spin=.5)),
        sites=[site],site_query='survey')
    local=dict(combat=dict(recovery=dict(flight=dict(pilot=p)),target=dict(ground_occluded=False,
        motion=dict(position=dict(x=100.,y=80.)))),cover=[dict(site=site['id'],grounded=True,approach=True,departure=True)],sun=None)
    return local,site


class ThreatenedApproachTests(unittest.TestCase):
    def test_missing_cover_is_unknown_and_departure_does_not_prove_approach_cover(self):
        self.assertEqual(A.cover_state(None),'unknown')
        self.assertEqual(A.cover_state(dict(grounded=False,approach=False,departure=True)),'exposed')
        self.assertEqual(A.cover_state(dict(grounded=True,approach=True,departure=False)),'covered')

    def test_rotating_translating_planet_motion_is_removed_before_lateral_gate(self):
        local,site=fixture();g=A.geometry(local,site)
        self.assertAlmostEqual(g['lateral_speed'],0.)
        self.assertTrue(g['aligned'] and g['descent_gate'])
        bad=copy.deepcopy(local);bad['combat']['recovery']['flight']['pilot']['ship']['velocity']['x']=0.
        self.assertEqual(abs(A.geometry(bad,site)['lateral_speed']),40.)
        self.assertFalse(A.geometry(bad,site)['descent_gate'])

    def test_low_height_exception_requires_ground_cover_and_alignment(self):
        local,site=fixture();local['cover'][0]['approach']=False
        self.assertTrue(A.geometry(local,site)['descent_gate'])
        local['cover'][0]['grounded']=False
        self.assertFalse(A.geometry(local,site)['descent_gate'])
        local['cover']=[]
        self.assertIsNone(A.geometry(local,site)['cover'])
        self.assertFalse(A.geometry(local,site)['descent_gate'])
        local['combat']['target']['ground_occluded']=True
        self.assertTrue(A.geometry(local,site)['descent_gate'])
        site['vehicle_position']=dict(x=62.,y=0.)
        self.assertFalse(A.geometry(local,site)['descent_gate'])

    def test_unavailable_route_probe_is_not_zero_covered_routes(self):
        local,site=fixture()
        row=dict(world_tick=10,seat=0,observation=dict(local=local),route_batches=None,
            routes_unknown='diagnostic requires joint round-trip planning',jetpack=None,
            jetpack_unknown='no full observed site survey',choice=None,choice_unknown='no native selected site')
        result=A.audit_probe(row)
        self.assertEqual(result['counts']['approach_covered'],1)
        self.assertIsNone(result['counts']['covered_powered_round_trips'])
        self.assertFalse(result['sites'][0]['routes_measured'])

    def test_added_dense_rows_preserve_every_retained_state_and_action(self):
        with tempfile.TemporaryDirectory() as name:
            before,after=[Path(name)/n for n in ['before','after']]
            old=[dict(tick=0,seat=0,action=0),dict(tick=2,seat=0,action=1)]
            new=[old[0],dict(tick=1,seat=0,action=2),old[1]]
            dump=lambda path,values:path.write_text(''.join(json.dumps(r)+'\n' for r in values))
            dump(before,old);dump(after,new)
            self.assertEqual(A.sparse_parity(before,after),2)
            changed=copy.deepcopy(new);changed[-1]['action']=9;dump(after,changed)
            with self.assertRaises(AssertionError):A.sparse_parity(before,after)
            dump(after,new[:-1])
            with self.assertRaises(AssertionError):A.sparse_parity(before,after)

    def test_probe_command_preserves_the_frozen_health_setting_and_physics(self):
        old=dict(command=['old','--out','old-out','--pursuit-health-seats','0','--seconds','600'])
        cmd=A.command(old,'frozen','new-out')
        self.assertEqual(cmd[:7],['frozen','--out','new-out','--pursuit-health-seats','0','--seconds','600'])
        self.assertEqual(cmd[cmd.index('--probe-cover-ticks')+1],','.join(map(str,A.TICKS)))
        self.assertEqual(A.TICKS,[5490,5539,5540,13770,13800,13819,13820])


if __name__=='__main__':
    unittest.main()
