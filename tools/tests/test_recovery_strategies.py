import copy
from collections import Counter
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('strategies', Path(__file__).resolve().parents[1] / 'compare-recovery-strategies.py')
S = importlib.util.module_from_spec(spec)
spec.loader.exec_module(S)


def player(outcome='draw', visits=None):
    visits = visits if visits is not None else [dict(planet=0, selected_tick=1, departed_tick=50)]
    return dict(outcome=outcome, visits=visits, completed_departures=len(visits), claims=len(visits),
                completed_recoveries=0, ships_lost=1, pilot_deaths=0, death_tick=None,
                abandoned_visits=0, unfinished_visits=0, owned_planet_ticks=600,
                progress=dict(eligible_ticks=300, distance_observed_ticks=280,
                              ticks_after_20s_without_progress=10, longest_no_progress_ticks=20))


def complete_results():
    cases = S.cases()
    runs = {c['name']: dict(item=c, players=[player(), player()]) for c in cases}
    comparisons = []
    for group in dict.fromkeys(c['group'] for c in cases):
        by_config = {c['configuration']: c for c in cases if c['group'] == group}
        for a, b, kind in S.CONTRASTS:
            item = by_config[a]
            comparisons.append(dict(group=group, contrast=a + '->' + b, kind=kind,
                stage=item['stage'], seat=item['seat'], interval=item['interval'],
                world_cluster=item['world_cluster'], before=by_config[a]['name'], after=by_config[b]['name'],
                benefits=['earlier_or_additional_departure'], regressions=[], outcome_transition='draw->draw',
                first_control_difference=dict(tick=10), adjusted_departures=dict(before=1, after=1)))
    return runs, comparisons


class RecoveryStrategyTests(unittest.TestCase):
    def test_matrix_is_balanced_and_fresh_seeds_are_separate(self):
        cases = S.cases()
        self.assertEqual(len(cases), 72)
        self.assertEqual(len({c['name'] for c in cases}), 72)
        fresh = [c for c in cases if c['stage'] == 'held_out']
        self.assertEqual(len(fresh), 48)
        counts = Counter((c['configuration'], c['seat'], c['interval'], c['opponent']) for c in fresh)
        self.assertEqual(set(counts.values()), {2})
        self.assertEqual(len(counts), 24)
        self.assertEqual([c['name'] for c in cases[:2]], ['known-recovery-candidate', 'known-recovery-candidate-laser'])
        seeds = {c['seed'] for c in fresh}
        prior = {c['seed'] for c in S.P.plan()} | {c['seed'] for c in S.B.cases()}
        prior |= {S.P.seed(f'{namespace}:{world}') for namespace in (S.C.NAMESPACE, S.C.A.NAMESPACE) for world in range(2)}
        self.assertEqual(len(seeds), 2)
        self.assertFalse(seeds & prior)

    def test_each_isolated_contrast_changes_only_its_declared_flags(self):
        for group in dict.fromkeys(c['group'] for c in S.cases()):
            by_config = {c['configuration']: S.flags(c) for c in S.cases() if c['group'] == group}
            for a, b, kind in S.CONTRASTS:
                if kind == 'combined': continue
                fa, fb = by_config[a], by_config[b]
                changed = {k for k in fa if fa[k] != fb[k]}
                expected = ({S.L.FLAG} if kind == 'laser' else
                            {S.C.FLAG, S.C.A.FLAG} if kind == 'defense' else
                            {'--active-flight-checks', *(f'--{name}' for name in S.P.SEAT_OPTIONS)})
                self.assertEqual(changed, expected, (group, a, b))
            for f in by_config.values():
                self.assertEqual((f['--seconds'], f['--require-finish'], f['--mode']), ('600', 'true', 'duel'))
                self.assertEqual(f['--impact-pod-control'] if '--trace-impact' in f else 'bot', 'bot')
                self.assertEqual(f['--objective-query-budget'], '384')

    def test_frozen_plan_rejects_provenance_and_case_mutations(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            binary = out / 'binary'; binary.write_bytes(b'qualified runtime')
            old = dict(source_commit='runtime-commit', binary=dict(sha256=S.P.digest(binary)),
                       runs={'known-laser-off-win-off': {'proof': 'off'}, 'known-lost-win-off': {'proof': 'on'}})
            source = out / 'source.json'; source.write_text(json.dumps(old))
            frozen_inputs = {'runtime.rs': 'runtime', S.OWN_INPUTS[0]: 'runner'}
            plan = dict(profile=S.PROFILE, namespace=S.NAMESPACE, inputs=frozen_inputs,
                runtime_source_commit=old['source_commit'], binary=dict(path=str(binary), sha256=S.P.digest(binary)),
                sources={'recovery': dict(path=str(source), sha256=S.P.digest(source))}, jobs=S.jobs(binary, out),
                replay_sources={'known-recovery-candidate': {'proof': 'off'}, 'known-recovery-candidate-laser': {'proof': 'on'}})
            with patch.object(S, 'inputs', return_value=frozen_inputs):
                S.verify_plan(plan, out)
                for mutation in (lambda p: p['jobs'].pop(), lambda p: p['jobs'][0]['command'].extend(['--seconds', '1']),
                                 lambda p: p.update(runtime_source_commit='different'),
                                 lambda p: p['jobs'][0]['item'].update(seed=3),
                                 lambda p: p['replay_sources'].clear(), lambda p: p['binary'].update(sha256='wrong')):
                    changed = copy.deepcopy(plan); mutation(changed)
                    with self.assertRaises(AssertionError): S.verify_plan(changed, out, True)
            with patch.object(S, 'inputs', return_value={**frozen_inputs, S.OWN_INPUTS[0]: 'new runner'}):
                with self.assertRaises(AssertionError): S.verify_plan(plan, out)
                S.verify_plan(plan, out, True)
            for changed in ({S.OWN_INPUTS[0]: 'runner'}, {**frozen_inputs, 'runtime.rs': 'changed'}):
                with patch.object(S, 'inputs', return_value=changed):
                    with self.assertRaises(AssertionError): S.verify_plan(plan, out, True)
            source.write_text('{}')
            with patch.object(S, 'inputs', return_value=frozen_inputs):
                with self.assertRaises(AssertionError): S.verify_plan(plan, out, True)

    def test_departure_exemption_requires_selection_after_an_earlier_victory(self):
        visits = [dict(selected_tick=1, departed_tick=50), dict(selected_tick=90, departed_tick=120),
                  dict(selected_tick=100, departed_tick=130), dict(selected_tick=101, departed_tick=140)]
        a, b = player('loss', visits), player('win')
        result = S.departure_accounting(a, b, 200, 100)
        self.assertEqual(result['adjusted_departures'], {'before': 3, 'after': 1})
        self.assertEqual(result['post_victory_exempt_visits']['before'], [visits[-1]])
        self.assertEqual(result['common_horizon_visits']['before'], [visits[0]])
        reverse = S.departure_accounting(b, a, 100, 200)
        self.assertEqual(reverse['adjusted_departures'], {'before': 1, 'after': 3})
        for ending in ('loss', 'draw'):
            result = S.departure_accounting(a, player(ending), 200, 100)
            self.assertEqual(result['adjusted_departures']['before'], 4)

    def test_missing_duplicate_or_modified_cases_cannot_pass(self):
        runs, pairs = complete_results()
        self.assertEqual(S.screen(runs, pairs)['decision'], 'advance_to_further_evaluation')
        with self.assertRaises(AssertionError): S.screen(runs, pairs[:-1])
        duplicate = copy.deepcopy(pairs); duplicate[-1] = copy.deepcopy(duplicate[0])
        with self.assertRaises(AssertionError): S.screen(runs, duplicate)
        runs[next(iter(runs))]['item']['seed'] += 1
        with self.assertRaises(AssertionError): S.screen(runs, pairs)

    def test_one_configurations_regression_is_not_hidden_by_another(self):
        runs, pairs = complete_results()
        bad = next(p for p in pairs if p['stage'] == 'held_out' and p['contrast'] == 'control->candidate-laser')
        runs[bad['after']]['players'][bad['seat']]['outcome'] = 'loss'
        result = S.screen(runs, pairs)
        self.assertEqual(result['configurations']['candidate-laser']['decision'], 'retain')
        self.assertEqual(result['configurations']['control-laser']['decision'], 'advance_to_further_evaluation')
        self.assertIn('points_regressed:overall', result['contrasts']['control->candidate-laser']['reasons'])

    def test_known_survival_failures_block_advancement_even_with_fresh_benefit(self):
        for regression in ('fewer_match_points', 'more_pilot_deaths', 'earlier_pilot_death'):
            runs, pairs = complete_results()
            pair = next(p for p in pairs if p['stage'] == 'known_qualification' and p['contrast'] == 'control->control-laser')
            pair['regressions'] = [regression]
            result = S.screen(runs, pairs)
            self.assertEqual(result['configurations']['control-laser']['decision'], 'retain')
            self.assertEqual(result['configurations']['control-defense']['decision'], 'retain')

    def test_inactive_fresh_option_cannot_advance(self):
        runs, pairs = complete_results()
        for p in pairs:
            if p['stage'] == 'held_out' and p['contrast'] == 'candidate-laser->candidate-defense':
                p['benefits'] = []; p['first_control_difference'] = None
        result = S.screen(runs, pairs)
        self.assertIn('no_useful_fresh_change', result['contrasts']['candidate-laser->candidate-defense']['reasons'])
        self.assertEqual(result['configurations']['candidate-defense']['decision'], 'retain')

    def test_earlier_observed_death_and_adjusted_completion_loss_are_retained(self):
        runs, pairs = complete_results()
        pair = next(p for p in pairs if p['stage'] == 'held_out' and p['contrast'] == 'control->control-laser')
        pair['regressions'] = ['earlier_pilot_death']
        pair['adjusted_departures']['after'] = 0
        reasons = S.screen(runs, pairs)['contrasts']['control->control-laser']['reasons']
        self.assertIn('adjusted_departures_decreased', reasons)
        self.assertTrue(any(r.startswith('earlier_pilot_death:') for r in reasons))

    def test_progress_coverage_and_worst_stall_remain_acceptance_checks(self):
        runs, pairs = complete_results()
        pair = next(p for p in pairs if p['stage'] == 'held_out' and p['contrast'] == 'control->control-laser')
        p = runs[pair['after']]['players'][pair['seat']]['progress']
        p.update(ticks_after_20s_without_progress=11, longest_no_progress_ticks=21)
        reasons = S.screen(runs, pairs)['contrasts']['control->control-laser']['reasons']
        self.assertIn('no_progress_fraction_increased', reasons)
        self.assertIn('worst_no_progress_ticks_increased', reasons)
        for row in runs.values():
            for player_ in row['players']: player_['progress']['eligible_ticks'] = 0
        reasons = S.screen(runs, pairs)['contrasts']['control->control-laser']['reasons']
        self.assertIn('unknown_no_progress_fraction', reasons)


if __name__ == '__main__':
    unittest.main()
