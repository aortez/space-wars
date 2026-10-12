import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    'clearance', Path(__file__).parents[1] / 'analyze-combat-clearance.py')
C = importlib.util.module_from_spec(spec)
spec.loader.exec_module(C)


def handoff(tick=0):
    return dict(tick=tick, seat=1, hunt_clearance_handoff=None,
                before=dict(goal='hunt', climb=0, pursuit=dict(started_tick=0)),
                goal='hunt', reason=None, pursuit=None, pursuit_climb=None,
                mission_events=[],
                geometry=[dict(planet=0, clearance=110, radial_speed=10),
                          dict(planet=1, clearance=160, radial_speed=10)],
                opponent_range=200, ship_health=100, weapons={})


class ClearanceQualificationTests(unittest.TestCase):
    def test_release_classification_covers_range_geometry_and_real_mission_changes(self):
        row = handoff()
        self.assertTrue(C.clearance_event(row)['premature_release'])
        row['pursuit_climb'] = 0
        self.assertTrue(C.clearance_event(row)['retained_handoff'])
        row['pursuit_climb'] = 1  # Replacing an unfinished anchor also loses safety memory.
        self.assertTrue(C.clearance_event(row)['premature_release'])
        for changes, reason in [
            (dict(goal='transfer'), 'different_mission'),
            (dict(opponent_range=None), 'opponent_unavailable'),
            (dict(opponent_range=300.01), 'opponent_left_climb_range'),
            (dict(geometry=[]), 'anchor_unavailable'),
            (dict(geometry=[dict(planet=0, clearance=140.01, radial_speed=-3.99)]),
             'normal_clearance'),
        ]:
            event = C.clearance_event(handoff() | changes)
            self.assertEqual(event['release_reason'], reason)
            self.assertFalse(event['premature_release'])
        for clearance, speed in [(140, 0), (141, -4)]:
            row = handoff() | dict(geometry=[dict(planet=0, clearance=clearance, radial_speed=speed)])
            self.assertTrue(C.clearance_event(row)['premature_release'])
        row = handoff()
        row['geometry'][0]['clearance'] = 150
        row['geometry'][1]['clearance'] = 139
        self.assertTrue(C.clearance_event(row)['premature_release'])

    def test_profile_pairing_rejects_confounds(self):
        old = dict(seed=1, candidate_seat=1, direct_laser_window=False,
                   hunt_clearance_handoff=False, probe_laser=False)
        new = old | dict(hunt_clearance_handoff=True)
        C.A.validate_pair(old, new, 'clearance-handoff')
        for bad in [new | dict(seed=2), new | dict(probe_laser=True),
                    new | dict(direct_laser_window=True), old]:
            with self.assertRaises(AssertionError):
                C.A.validate_pair(old, bad, 'clearance-handoff')
        with self.assertRaisesRegex(AssertionError, 'mixed experimental'):
            C.A.validate_pair(old | dict(direct_laser_window=True),
                              new | dict(direct_laser_window=True), 'clearance-handoff')

    def test_controller_normalization_does_not_hide_other_changes(self):
        text = ('    hunt_clearance_handoff: Some(\n'
                f'        "{C.PROFILE}",\n    ),\n    pursuit_climb: Some(2),\n')
        expected = '    hunt_clearance_handoff: None,\n    pursuit_climb: Some(2),\n'
        self.assertEqual(C.normalize_controllers(text), expected)
        self.assertNotEqual(C.normalize_controllers(text.replace('Some(2)', 'Some(3)')), expected)
        for bad in [text * 2, text.replace(C.PROFILE, 'wrong'), expected]:
            with self.assertRaises(AssertionError):
                C.normalize_controllers(bad)

    def test_first_controller_change_can_precede_first_encoded_input_change(self):
        with tempfile.TemporaryDirectory() as directory:
            cases = []
            for name, enabled in [('control', False), ('candidate', True)]:
                path = Path(directory) / name / 'output'
                path.mkdir(parents=True)
                cases.append(dict(directory=str(path.parent), case=dict(candidate_seat=1),
                                  end_tick=2, outcome=dict(winner='player_1')))
                snapshot = dict(brains=[{}, {}], world='same')
                debug = '    hunt_clearance_handoff: None,\n'
                if enabled:
                    snapshot['brains'][1]['hunt_clearance_handoff'] = C.PROFILE
                    debug = ('    hunt_clearance_handoff: Some(\n'
                             f'        "{C.PROFILE}",\n    ),\n')
                (path / 'initial.json').write_text(json.dumps(snapshot))
                (path / 'initial-controllers.txt').write_text(debug)
                for seat in [1, 2]:
                    (path / f'initial-frame-p{seat}.json').write_text('{}')
                combat, actions = [], []
                for tick in range(2):
                    other = handoff(tick) | dict(seat=0)
                    evaluated = handoff(tick)
                    if enabled:
                        evaluated.update(pursuit_climb=0, hunt_clearance_handoff=C.PROFILE)
                    combat.extend([other, evaluated])
                    action = dict(tick=tick, actions=[0]*6, scenario_before_step_fnv64='same',
                                  normalized_decision_fnv64='new' if enabled else 'old')
                    if tick == 1 and enabled:
                        action['actions'][3] = 1
                    actions.append(action)
                for filename, stream in [('combat', combat), ('actions', actions)]:
                    (path / f'{filename}.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in stream))
            first = C.A.first_change(*cases, 'clearance-handoff')
            self.assertEqual(first['tick'], 1)
            result = C.audit_prefix(*cases, first)
            self.assertEqual(result['first_controller_change']['tick'], 0)
            self.assertEqual(result['first_normalized_decision_change'], 0)
            path = Path(cases[1]['directory']) / 'output'
            rows = list(C.A.rows(path / 'combat.jsonl'))
            rows[1]['before']['climb'] = 1
            (path / 'combat.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in rows))
            with self.assertRaises(AssertionError):
                C.audit_prefix(*cases, first)
            actions = list(C.A.rows(path / 'actions.jsonl'))
            actions[0]['actions'][0] = 1
            (path / 'actions.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in actions))
            with self.assertRaisesRegex(AssertionError, 'opponent changed first'):
                C.A.first_change(*cases, 'clearance-handoff')


if __name__ == '__main__':
    unittest.main()
