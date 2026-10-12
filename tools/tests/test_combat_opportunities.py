import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    'combat', Path(__file__).parents[1] / 'analyze-combat-opportunities.py')
A = importlib.util.module_from_spec(spec)
spec.loader.exec_module(A)


class CombatOpportunityTests(unittest.TestCase):
    def test_terminal_damage_is_included_and_truncated_trace_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / 'output'
            out.mkdir()
            damage = dict(last_contact_tick=None, last_damage_tick=None,
                          last_source=None, last_damage_percent=0)
            rows = [dict(seat=s, tick=0, target=None, aim=None, queries_ready=True,
                         laser_available=True, reason=None, combat_active=False,
                         controls_armed=True,
                         combat_goal='takeoff', goal='transfer', cannon=False,
                         laser=False, break_until=None, supply=None, damage=damage,
                         pilot_damage=None) for s in range(2)]
            summary = dict(audits_passed=True, end_tick=1, case={}, damage=[damage, damage],
                           brains=[dict(completed_sorties=0, completed_recoveries=0)]*2,
                           native_recovery=[dict(ships_lost=0, rebuilds=0, pods_lost=0)]*2,
                           combat=[dict(weapons={})]*2,
                           match=dict(outcome=dict(winner='player_2'), reason='pilot_death',
                                      pilots=[dict(death_tick=1, last_damage=dict(
                                          tick=1, cause='impact', amount=100)),
                                          dict(death_tick=None, last_damage=None)]))
            (out / 'summary.json').write_text(json.dumps(summary))
            (out / 'combat.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in rows))
            for name in ['actions.jsonl', 'laser-probes.jsonl']:
                (out / name).write_text('')
            report = A.analyze_case(out.parent)
            self.assertEqual(report['seats'][0]['recorded_damage_taken'], {'pilot_impact': 100})
            self.assertEqual(report['seats'][1]['recorded_damage_taken'], {})
            (out / 'combat.jsonl').write_text(json.dumps(rows[0])+'\n')
            with self.assertRaises(AssertionError):
                A.analyze_case(out.parent)

    def test_visibility_does_not_imply_an_eligible_shot(self):
        row = dict(target=dict(visible=True, ground_occluded=False),
                   aim=dict(direct_window=True), queries_ready=True, laser_available=True,
                   reason=None, combat_active=True, combat_goal='engage ship',
                   goal='hunt', cannon=False, break_until=None, tick=10)
        row['controls_armed'] = True
        self.assertEqual(A.opportunities(row), (True, True))
        self.assertEqual(A.opportunities({**row, 'goal': 'patrol', 'combat_active': False}),
                         (True, True))
        for change in [dict(cannon=True), dict(break_until=20), dict(controls_armed=False),
                       dict(reason='climbing for a firing pass'),
                       dict(combat_active=False, goal='capture')]:
            self.assertEqual(A.opportunities({**row, **change}), (True, False))
        for change in [dict(laser_available=False), dict(queries_ready=False),
                       dict(aim=dict(direct_window=False)),
                       dict(target=dict(visible=True, ground_occluded=True))]:
            self.assertEqual(A.opportunities({**row, **change}), (False, False))

    def test_first_change_rejects_extra_controls_or_earlier_physics_change(self):
        with tempfile.TemporaryDirectory() as directory:
            cases = []
            for name in ['control', 'candidate']:
                path = Path(directory) / name / 'output'
                path.mkdir(parents=True)
                cases.append(dict(directory=str(path.parent), case=dict(candidate_seat=1),
                                  end_tick=2, outcome=dict(winner='player_1')))
            actions = [{'Scenario': dict(kind=kind, payload=payload)} for kind, payload in
                       [(1, [0, 0]), (2, [0, 0]), (3, [0, 0, 0]),
                        (1, [1, 0]), (2, [1, 0]), (3, [1, 0, 0])]]
            old = dict(tick=0, actions=actions, scenario_before_step_fnv64='same')
            new = copy.deepcopy(old)
            new['actions'][5]['Scenario']['payload'][1] = 1

            def check(candidate):
                for case, row in zip(cases, [old, candidate]):
                    stream = [row, {**row, 'tick': 1}]
                    (Path(case['directory']) / 'output/actions.jsonl').write_text(
                        ''.join(json.dumps(r)+'\n' for r in stream))
                return A.first_change(*cases)

            self.assertEqual(check(new)['tick'], 0)
            bad = copy.deepcopy(new)
            bad['actions'][3]['Scenario']['payload'][1] = 1
            with self.assertRaises(AssertionError):
                check(bad)
            bad = copy.deepcopy(new)
            bad['actions'].append(copy.deepcopy(actions[0]))
            with self.assertRaises(AssertionError):
                check(bad)
            bad = {**new, 'scenario_before_step_fnv64': 'different'}
            with self.assertRaises(AssertionError):
                check(bad)
            check(new)
            candidate_path = Path(cases[1]['directory']) / 'output/actions.jsonl'
            candidate_path.write_text(json.dumps(new)+'\n')
            with self.assertRaisesRegex(AssertionError, 'incomplete action trace'):
                A.first_change(*cases)
            candidate_path.write_text((json.dumps(new)+'\n')*2)
            with self.assertRaisesRegex(AssertionError, 'unordered action row'):
                A.first_change(*cases)

    def test_missing_matrix_case_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'matrix.json').write_text('[{}, {}]')
            (root / 'results.json').write_text('[{"index":0,"case":{},"exit_code":0}]')
            with self.assertRaisesRegex(AssertionError, 'incomplete matrix'):
                A.analyze_root(root)


if __name__ == '__main__':
    unittest.main()
