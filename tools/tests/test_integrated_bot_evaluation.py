import copy
import gzip
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('integrated_evaluation', ROOT / 'tools/validate-integrated-bot.py')
M = importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)


def player():
    return dict(outcome='draw', completed_departures=1, claims=1, completed_recoveries=0,
                ships_lost=0, pilot_deaths=0, abandoned_visits=0, unfinished_visits=0,
                owned_planet_ticks=600, visits=[], progress=dict(eligible_ticks=300,
                    distance_observed_ticks=280, ticks_after_20s_without_progress=10,
                    longest_no_progress_ticks=20))


def complete_results():
    runs, comparisons = {}, []
    cases = M.P.plan()
    for i in range(0, len(cases), 2):
        pair = cases[i:i+2]
        for item in pair:
            runs[item['name']] = dict(players=[player(), player()])
        by_arm = {p['arm']: p for p in pair}
        item = pair[0]
        comparisons.append(dict(group=item['group'], stage=item['stage'], world=item['world'],
            seat=item['seat'], opponent=item['opponent'], interval=item['interval'], seed=item['seed'],
            control=by_arm['control']['name'], candidate=by_arm['candidate']['name'],
            first_control_difference=dict(tick=1), useful_completed_changes=[dict(kind='earlier')],
            missing_control_completions=[]))
    return runs, comparisons


class IntegratedEvaluationTests(unittest.TestCase):
    def test_legacy_wire_default_preserves_actor_and_freshness_checks(self):
        row = dict(pilot=dict(tick=1845, owner='player_2', planet=dict(index=1)),
                   landing_objective=dict(version=1, actor='player_2', tick=1845,
                       objective=dict(planet=1), sites=[], actual=None),
                   objective_work=None, objective_evidence=None)
        before = copy.deepcopy(row)
        self.assertEqual(M.audit_publication(row, 'legacy'), dict(age=0, powered=0))
        self.assertEqual(row, before)
        with self.assertRaises(KeyError):
            M.audit_publication(row, 'joint_round_trip')
        for key, value in (('actor', 'player_1'), ('tick', 1844), ('planning', 'joint_round_trip')):
            bad = copy.deepcopy(row)
            bad['landing_objective'][key] = value
            with self.assertRaises(AssertionError):
                M.audit_publication(bad, 'legacy')

    def test_frozen_commands_and_per_seat_identities_cannot_be_overridden(self):
        manifest = json.loads((ROOT / 'docs/data/integrated-bot-candidate-v1.json').read_text())
        with gzip.open(ROOT / manifest['archive']['path'], 'rt') as stream:
            plan = json.loads(json.load(stream)['documents']['comparison_plan']['text'])
        M.verify_plan(plan, manifest)
        for mutate in (
            lambda p: p['cases'][0]['command'].extend(['--seconds', '1']),
            lambda p: p['cases'][0]['command'].__setitem__(0, '/tmp/another-binary'),
            lambda p: p['cases'][0]['item'].update(seed=43),
            lambda p: p['cases'][0]['configuration_by_seat'].reverse(),
            lambda p: p['cases'].pop(),
        ):
            invalid = copy.deepcopy(plan)
            mutate(invalid)
            with self.assertRaises(ValueError):
                M.verify_plan(invalid, manifest)

    def test_stratum_regression_cannot_be_hidden_by_an_overall_tie(self):
        runs, pairs = complete_results()
        self.assertEqual(M.decision(runs, pairs)['decision'], 'advance_to_further_evaluation')
        fresh = [p for p in pairs if p['stage'] == 'held_out']
        worse = next(p for p in fresh if p['opponent'] == 10)
        better = next(p for p in fresh if p['opponent'] == 9)
        runs[worse['candidate']]['players'][worse['seat']]['outcome'] = 'loss'
        runs[better['candidate']]['players'][better['seat']]['outcome'] = 'win'
        result = M.decision(runs, pairs)
        self.assertEqual(result['fresh']['control']['points'], result['fresh']['candidate']['points'])
        self.assertIn('match_points_regressed:opponent:10', result['reasons'])
        self.assertEqual(result['decision'], 'retain')
        self.assertFalse(result['default_promotion'])

    def test_each_predeclared_safety_and_progress_gate_can_refuse_advancement(self):
        for key, change, expected in (
            ('completed_departures', -1, 'fewer_completed_departures'),
            ('ships_lost', 1, 'ships_lost_increased'),
            ('pilot_deaths', 1, 'pilot_deaths_increased'),
            ('worst', 1, 'worst_no_progress_ticks_increased'),
            ('late', 1, 'no_progress_fraction_increased'),
        ):
            runs, pairs = complete_results()
            pair = next(p for p in pairs if p['stage'] == 'held_out')
            subject = runs[pair['candidate']]['players'][pair['seat']]
            if key == 'worst':
                subject['progress']['longest_no_progress_ticks'] += change
            elif key == 'late':
                subject['progress']['ticks_after_20s_without_progress'] += change
            else:
                subject[key] += change
            self.assertIn(expected, M.decision(runs, pairs)['reasons'])

    def test_unchanged_play_unknown_progress_and_directed_failures_are_retained(self):
        runs, pairs = complete_results()
        for pair in pairs:
            pair['useful_completed_changes'] = []
        self.assertIn('no_useful_completed_change_in_fresh_pairs', M.decision(runs, pairs)['reasons'])
        runs, pairs = complete_results()
        for pair in pairs:
            runs[pair['candidate']]['players'][pair['seat']]['progress']['eligible_ticks'] = 0
        self.assertIn('unknown_no_progress_fraction', M.decision(runs, pairs)['reasons'])
        runs, pairs = complete_results()
        pairs[0]['missing_control_completions'] = [dict(planet=0)]
        self.assertIn('directed_completion_regressed:' + pairs[0]['group'],
                      M.decision(runs, pairs)['reasons'])

    def test_useful_completion_requires_an_executed_visit_after_changed_controls(self):
        a = dict(visits=[dict(planet=0, departed_tick=200), dict(planet=1, departed_tick=300)])
        b = dict(visits=[dict(planet=0, departed_tick=150), dict(planet=2, departed_tick=250),
                        dict(planet=1, departed_tick=None)])
        useful, missing = M.completion_changes(a, b, 100)
        self.assertEqual([p['kind'] for p in useful], ['earlier', 'additional'])
        self.assertEqual([p['planet'] for p in missing], [1])
        self.assertEqual(M.completion_changes(a, b, None)[0], [])
        self.assertEqual([p['planet'] for p in M.completion_changes(a, b, 175)[0]], [2])

    def test_first_changed_actions_do_not_confuse_diagnostic_identity_with_controls(self):
        with tempfile.TemporaryDirectory() as directory:
            paths = [Path(directory) / name for name in ('a.jsonl', 'b.jsonl')]
            rows = [dict(tick=t, seat=s, actions=[0]) for t in (0, 1) for s in (0, 1)]
            def write(path, records):
                path.write_text(''.join(json.dumps(r)+'\n' for r in records))
            write(paths[0], rows)
            other = [dict(r, policy='diagnostic-only') for r in rows]
            write(paths[1], other)
            self.assertIsNone(M.first_control_difference(*paths))
            other[2]['actions'] = [1]
            write(paths[1], other)
            result = M.first_control_difference(*paths)
            self.assertEqual((result['tick'], result['seat'], result['matched_rows']), (1, 0, 2))
            write(paths[1], rows[:-1])
            self.assertEqual(M.first_control_difference(*paths)['reason'], 'trace_length')


if __name__ == '__main__':
    unittest.main()
