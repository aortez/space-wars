import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('pod_braking', Path(__file__).parents[1]/'validate-pod-braking.py')
P = importlib.util.module_from_spec(spec)
spec.loader.exec_module(P)


def fixture():
    return dict(motion=dict(center_of_mass=dict(x=6.,y=0.),velocity=dict(x=4.,y=0.),
        boundary_center=dict(x=0.,y=0.),boundary_radius=10.,spin=2.),
        planet=dict(motion=dict(position=dict(x=0.,y=0.),velocity=dict(x=0.,y=0.),spin=0.)),
        ship=dict(velocity=dict(x=4.,y=8.)),
        flight=dict(limits=dict(brake_acceleration=2.,turn_acceleration=3.)))


class PodBrakingTests(unittest.TestCase):
    def test_outward_motion_uses_center_of_mass_and_point_clearance(self):
        m = P.motion_metrics(fixture())
        self.assertEqual(m['speed'],4.)
        self.assertEqual(m['origin_velocity_difference'],8.)
        self.assertEqual(m['radial_clearance'],4.)
        self.assertEqual(m['distance_to_wall_along_velocity'],4.)
        self.assertEqual(m['ideal_braking_distance'],4.)
        self.assertEqual(m['spin_arrest_seconds'],0.5)

    def test_inward_tangent_center_and_stationary_rays(self):
        r = fixture()
        r['motion']['velocity']['x'] = -4.
        m = P.motion_metrics(r)
        self.assertEqual(m['distance_to_wall_along_velocity'],16.)
        self.assertEqual(m['ideal_outward_braking_distance'],0.)
        r['motion']['velocity'] = dict(x=0.,y=4.)
        self.assertEqual(P.motion_metrics(r)['distance_to_wall_along_velocity'],8.)
        r['motion']['center_of_mass'] = dict(x=0.,y=0.)
        self.assertEqual(P.motion_metrics(r)['distance_to_wall_along_velocity'],10.)
        r['motion']['velocity'] = dict(x=0.,y=0.)
        self.assertIsNone(P.motion_metrics(r)['distance_to_wall_along_velocity'])

    def test_frame_translation_and_rotation_are_evaluated_at_center_of_mass(self):
        r = fixture()
        r['planet']['motion'].update(velocity=dict(x=2.,y=3.),spin=2.)
        r['motion'].update(velocity=dict(x=2.,y=15.),spin=2.)
        m = P.motion_metrics(r)
        self.assertEqual(m['relative_speed'],0.)
        self.assertEqual(m['spin_arrest_seconds'],0.)
        translated = copy.deepcopy(r)
        for v in [translated['motion']['center_of_mass'],translated['motion']['boundary_center'],translated['planet']['motion']['position']]:
            v['x'] += 130.; v['y'] -= 25.
        self.assertEqual(P.motion_metrics(translated),m)

    def test_unknown_or_invalid_motion_does_not_become_safe_zero(self):
        r = fixture()
        r['motion'] = None
        self.assertIsNone(P.motion_metrics(r))
        r = fixture(); r['motion']['velocity']['x'] = float('nan')
        with self.assertRaises(AssertionError): P.motion_metrics(r)
        r = fixture(); r['flight']['limits']['brake_acceleration'] = 0.
        with self.assertRaises(AssertionError): P.motion_metrics(r)
        r = fixture(); r['motion']['center_of_mass']['x'] = 11.
        m = P.motion_metrics(r)
        self.assertEqual(m['radial_clearance'],-1.)
        self.assertIsNone(m['distance_to_wall_along_velocity'])

    def test_prefix_requires_every_seat_and_rejects_early_action_changes(self):
        with tempfile.TemporaryDirectory() as path:
            old,new = Path(path)/'old',Path(path)/'new'
            old.mkdir();new.mkdir()
            records = [dict(seat=s,pilot=dict(tick=t),actions=[0]) for t in range(3) for s in range(2)]
            def save(root,rs):
                (root/'capture-evidence.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in rs))
            save(old,records);save(new,records)
            self.assertEqual(P.compare_prefix(old,new,2),4)
            records[2]['actions'] = [1];save(new,records)
            with self.assertRaises(AssertionError): P.compare_prefix(old,new,2)
            save(new,records[:1])
            with self.assertRaises(AssertionError): P.compare_prefix(old,new,2)

    def test_physical_parity_keeps_state_and_length_but_omits_only_actions(self):
        with tempfile.TemporaryDirectory() as path:
            old,new = Path(path)/'old',Path(path)/'new'
            old.mkdir();new.mkdir()
            def save(root,rs):
                (root/'capture-evidence.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in rs))
            save(old,[dict(actions=[0],pilot=dict(tick=1,ship_form='escape_pod'))])
            save(new,[dict(actions=[1],pilot=dict(tick=1,ship_form='escape_pod'))])
            self.assertTrue(P.physical_parity(old,new))
            save(new,[dict(actions=[1],pilot=dict(tick=1,ship_form='ship'))])
            self.assertFalse(P.physical_parity(old,new))
            save(new,[])
            self.assertFalse(P.physical_parity(old,new))


if __name__ == '__main__':
    unittest.main()
