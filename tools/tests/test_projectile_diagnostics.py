import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    'projectiles', Path(__file__).parents[1] / 'validate-projectile-diagnostics.py')
P = importlib.util.module_from_spec(spec)
spec.loader.exec_module(P)
analysis_spec = importlib.util.spec_from_file_location(
    'projectile_analysis', Path(__file__).parents[1] / 'analyze-projectile-diagnostics.py')
A = importlib.util.module_from_spec(analysis_spec)
analysis_spec.loader.exec_module(A)


def vec(x=0., y=0.):
    return dict(x=x, y=y)


def fixture():
    motion = dict(position=vec(10., 20.), velocity=vec(3., 4.), angle=.5, spin=.2)
    pilot = dict(tick=10, owner='player_1', vehicle=0, ship_form='ship', ship=motion,
                 location=dict(aboard=0), ship_available=True)
    projectile = dict(id=100001, owner='player_2', spawn_tick=4, radius=2., collision_radius=3.,
                      motion=dict(position=vec(60., 20.), velocity=vec(-17., 4.), angle=0., spin=1.),
                      relative_position=vec(50., 0.), relative_velocity=vec(-20., 0.))
    row = dict(schema=1, tick=10, seat=0, diagnostic=dict(version=1, tick=10, actor='player_1', vehicle=0,
               ship_form='ship', observer=motion, observer_radius=9., range=600., capacity=64,
               debris_scanned=3, shells_in_range=1, unavailable_shells=0, projectiles=[projectile]))
    return row, pilot


class ProjectileDiagnosticsTests(unittest.TestCase):
    def test_current_physical_frame_and_relative_values_are_required(self):
        row, pilot = fixture()
        P.audit_sample(row, pilot)
        for key in ('relative_position', 'relative_velocity'):
            changed = copy.deepcopy(row)
            changed['diagnostic']['projectiles'][0][key]['x'] += 1.
            with self.assertRaises(AssertionError):
                P.audit_sample(changed, pilot)

    def test_identity_age_bounds_and_missing_observer_cannot_be_silently_accepted(self):
        row, pilot = fixture()
        for field, value in [('id', 0), ('spawn_tick', 11), ('collision_radius', 0.)]:
            changed = copy.deepcopy(row)
            changed['diagnostic']['projectiles'][0][field] = value
            with self.assertRaises(AssertionError):
                P.audit_sample(changed, pilot)
        row['diagnostic'] = None
        with self.assertRaises(AssertionError):
            P.audit_sample(row, pilot)
        pilot['location'] = 'on_foot'
        P.audit_sample(row, pilot)

    def test_linear_entry_uses_relative_speed_and_combined_radius(self):
        result = P.linear_approach(vec(100., 0.), vec(-50., 0.), 10.)
        self.assertAlmostEqual(result['entry_seconds'], 1.8)
        self.assertEqual(result['opening_speed'], -50.)
        self.assertEqual(result['closest_seconds'], 2.)
        self.assertEqual(result['closest_clearance'], -10.)

    def test_receding_tangential_and_distant_passes_are_not_collision_predictions(self):
        for position, velocity in [(vec(100., 0.), vec(50., 0.)),
                                   (vec(100., 0.), vec(0., 100.)),
                                   (vec(100., 30.), vec(-100., 0.)),
                                   (vec(300., 0.), vec(-50., 0.)),
                                   (vec(100., 0.), vec())]:
            self.assertIsNone(P.linear_approach(position, velocity, 10.)['entry_seconds'])
        self.assertEqual(P.linear_approach(vec(5., 0.), vec(), 10.)['entry_seconds'], 0.)

    def test_command_preserves_policy_configuration_and_disables_by_omission(self):
        old = dict(command=['old', '--out', 'old-root', '--transfer-speed-seats', '0'])
        self.assertEqual(P.command(old, 'binary', 'new-root', False),
                         ['binary', '--out', 'new-root', '--transfer-speed-seats', '0'])
        self.assertEqual(P.command(old, 'binary', 'new-root', True)[-2:], ['--trace-projectiles', 'true'])

    def test_screen_episodes_split_on_a_miss_or_missing_observation(self):
        rows = [dict(tick=t, entry=entry) for t, entry in [(10, True), (11, True),
                                                         (12, False), (13, True), (15, True)]]
        self.assertEqual(A.episodes(rows, lambda r: r['entry']),
                         [dict(first_tick=10, last_tick=11, ticks=2),
                          dict(first_tick=13, last_tick=13, ticks=1),
                          dict(first_tick=15, last_tick=15, ticks=1)])


if __name__ == '__main__':
    unittest.main()
