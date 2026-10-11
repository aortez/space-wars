import copy
import importlib.util
import math
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    'threat', Path(__file__).parents[1] / 'analyze-transfer-threat.py')
T = importlib.util.module_from_spec(spec)
spec.loader.exec_module(T)


def motion(x=0., y=0., vx=0., vy=0., angle=0.):
    return dict(position=dict(x=x, y=y), velocity=dict(x=vx, y=vy), angle=angle)


class TransferThreatTests(unittest.TestCase):
    def test_opening_sign_and_common_motion_do_not_depend_on_world_velocity(self):
        own, enemy = motion(), motion(y=100., vy=-20.)
        g = T.relative_geometry(own, enemy)
        self.assertEqual((g['range'], g['opening_speed'], g['relative_speed']), (100., -20., 20.))
        own['velocity'] = dict(x=30., y=40.)
        enemy['velocity'] = dict(x=30., y=20.)
        self.assertEqual(T.relative_geometry(own, enemy), g)
        enemy['velocity']['y'] = 60.
        self.assertEqual(T.relative_geometry(own, enemy)['opening_speed'], 20.)

    def test_tangential_speed_is_not_mislabeled_as_closing(self):
        g = T.relative_geometry(motion(), motion(y=100., vx=60.))
        self.assertEqual(g['opening_speed'], 0.)
        self.assertEqual(g['relative_speed'], 60.)
        self.assertAlmostEqual(g['aim_error_radians'], math.atan2(20., 100.))

    def test_heading_is_measured_from_ship_nose_with_bounded_lead(self):
        self.assertEqual(T.relative_geometry(motion(), motion(y=600., vx=300.))['aim_error_radians'],
                         math.atan2(300., 600.))
        self.assertAlmostEqual(T.relative_geometry(motion(angle=math.pi / 2), motion(x=-100.))['aim_error_radians'], 0.)
        self.assertAlmostEqual(abs(T.relative_geometry(motion(), motion(y=-100.))['aim_error_radians']), math.pi)
        self.assertIsNone(T.relative_geometry(motion(), motion())['opening_speed'])

    def test_firing_explanations_preserve_visibility_readiness_aim_and_native_mode(self):
        row = dict(combat_goal='engage ship', laser_ready=True, cannon_ready=True,
                   target=dict(range=200., visible=True, ground_occluded=False, aim_error_radians=0.))
        self.assertEqual(T.weapon_gate_failures(row, 'cannon'), [])
        row['target']['aim_error_radians'] = .08
        self.assertEqual(T.weapon_gate_failures(row, 'cannon'), ['aim'])
        row['combat_goal'] = 'climb clear of ground'
        row['cannon_ready'] = False
        row['target'].update(range=230., ground_occluded=True)
        self.assertEqual(T.weapon_gate_failures(row, 'cannon'),
                         ['combat_controller_not_engaging', 'visibility', 'readiness', 'aim', 'range'])
        row['target'] = None
        self.assertEqual(T.weapon_gate_failures(row, 'laser'), ['target_unavailable'])

    def test_contact_requires_matching_damage_and_contact_tick_not_stale_cannon_history(self):
        sample = dict(pilots=[dict(tick=900)], damage=[dict(last_damage_tick=850,
                      last_contact_tick=850, last_source='cannon', last_contact_source='cannon',
                      last_ship_lost=True, last_contact_spawn_tick=100, last_damage_percent=31.)])
        self.assertEqual(T.contact_at_loss(sample, 0, 850)['age_seconds'], 12.5)
        for key, value in [('last_contact_tick', 800), ('last_source', 'laser'), ('last_ship_lost', False)]:
            stale = copy.deepcopy(sample)
            stale['damage'][0][key] = value
            self.assertIsNone(T.contact_at_loss(stale, 0, 850))
        sample['damage'][0]['last_contact_spawn_tick'] = 851
        with self.assertRaises(AssertionError):
            T.contact_at_loss(sample, 0, 850)

    def test_sparse_duplicate_or_wrong_seat_actions_cannot_prove_no_fire(self):
        rows = [dict(tick=t, seat=0, laser=False, cannon=False, goal='transfer') for t in range(10, 14)]
        self.assertEqual(T.complete_actions(rows, 0, 10, 14)['cannon_ticks'], 0)
        for bad in [rows[:-1], rows + rows[:1], [dict(r, seat=1) for r in rows]]:
            with self.assertRaises(AssertionError):
                T.complete_actions(bad, 0, 10, 14)

    def test_action_decoding_checks_identity_and_payload_instead_of_array_position(self):
        row = dict(seat=1, actions=[{'Scenario': dict(kind=T.WEAPON_ACTION, payload=[1, 0, 1])}, {}])
        self.assertEqual(T.weapon_action(row), dict(laser=False, cannon=True))
        for payload in ([0, 0, 1], [1, 2, 0], [1, 0]):
            row['actions'][0]['Scenario']['payload'] = payload
            with self.assertRaises(AssertionError):
                T.weapon_action(row)

    def test_launch_counter_bracket_includes_prestart_and_first_postend_samples(self):
        samples = [dict(pilots=[dict(tick=t)]) for t in (60, 120, 180, 240, 300)]
        bracket = T.bracketing_samples(samples, 0, 140, 220)
        self.assertEqual([s['pilots'][0]['tick'] for s in bracket], [120, 180, 240])

    def test_changed_retained_input_fails_before_analysis(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'evidence.json'
            path.write_text('{}')
            hashes = {path.name: T.digest(path)}
            self.assertEqual(T.checked_path(path.parent, path.name, hashes), path)
            path.write_text('{"changed":true}')
            with self.assertRaises(AssertionError):
                T.checked_path(path.parent, path.name, hashes)


if __name__ == '__main__':
    unittest.main()
