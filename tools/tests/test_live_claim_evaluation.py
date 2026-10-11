import copy
from collections import Counter
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('evaluation', Path(__file__).resolve().parents[1] / 'compare-live-claim.py')
E = importlib.util.module_from_spec(spec); spec.loader.exec_module(E)
spec = importlib.util.spec_from_file_location('claim_fixtures', Path(__file__).with_name('test_live_claim.py'))
F = importlib.util.module_from_spec(spec); spec.loader.exec_module(F)
spec = importlib.util.spec_from_file_location('strategy_fixtures', Path(__file__).with_name('test_recovery_strategies.py'))
S = importlib.util.module_from_spec(spec); spec.loader.exec_module(S)


def source(binary=Path('/source/binary')):
    runs = {}
    for item in E.X.cases(F.F.source()):
        runs[item['name']] = dict(item=item, command=[str(binary),
            *(v for pair in E.X.flags(item).items() for v in pair), '--out', '/source/'+item['name']])
    return dict(complete=True, profile=E.X.PROFILE, runtime_source_commit='runtime',
                inputs={'runtime.rs': 'unchanged'}, binary=dict(path=str(binary)), runs=runs)


def results():
    cases = E.cases(source())
    runs = {c['name']: dict(item=c, audited=True, retention={'matched': True},
                           players=[S.player(), S.player()]) for c in cases}
    pairs = []
    for group in dict.fromkeys(c['group'] for c in cases):
        items = {c['configuration']: c for c in cases if c['group'] == group}
        for a, b in E.X.CONTRASTS:
            item = items[a]
            pairs.append(dict(group=group, before_configuration=a, after_configuration=b,
                contrast=a+'->'+b, before=items[a]['name'], after=items[b]['name'],
                **{k:item[k] for k in ('stage','seat','opponent','interval','world_cluster','seed')},
                benefits=['earlier_or_additional_departure'], regressions=[], outcome_transition='draw->draw',
                first_control_difference={'tick':10}, adjusted_departures=dict(before=1,after=1),
                common_horizon_players={side:[dict(ships_lost=1,pilot_deaths=0,claims=1,
                    completed_departures=1,completed_recoveries=0,abandoned_visits=0) for _ in (0,1)]
                    for side in ('before','after')}))
    return runs,pairs


class LiveClaimEvaluationTests(unittest.TestCase):
    def test_matrix_balances_four_fresh_worlds_both_seats_and_asteroid_conditions(self):
        cases=E.cases(source()); self.assertEqual(len(cases),60)
        self.assertEqual(len({c['name'] for c in cases}),60)
        fresh=[c for c in cases if c['stage']=='held_out']
        self.assertEqual(len(fresh),48)
        counts=Counter((c['configuration'],c['seat'],c['interval']) for c in fresh)
        self.assertEqual(set(counts.values()),{4});self.assertEqual(len(counts),12)
        self.assertEqual(len({c['seed'] for c in fresh}),4)
        for world in range(4):
            selected=[c for c in fresh if c['world_cluster']==world]
            self.assertEqual({c['seed'] for c in selected},{E.P.seed(f'{E.NAMESPACE}:{world}')})
        orders=[]
        for g in dict.fromkeys(c['group'] for c in fresh):
            orders.append(tuple(c['configuration'] for c in fresh if c['group']==g))
        self.assertEqual(set(orders),{E.X.CONFIGS[i:]+E.X.CONFIGS[:i] for i in range(3)})

    def test_known_commands_replay_exactly_and_new_variant_changes_only_stopping(self):
        prior=source();cases=E.cases(prior)
        for item in cases[:12]:
            self.assertEqual([v for p in E.X.flags(item).items() for v in p],prior['runs'][item['name']]['command'][1:-2])
        for g in dict.fromkeys(c['group'] for c in cases):
            items={c['configuration']:c for c in cases if c['group']==g}
            a,b=[E.X.flags(items[c]) for c in ('integrated','no-stop')]
            self.assertEqual({k for k in a if a[k]!=b[k]},{'--live-claim-stopping'})
            for item in items.values():
                flags=E.X.flags(item)
                self.assertEqual((flags['--seconds'],flags['--require-finish']),('600','true'))
                self.assertEqual('--trace-impact' in flags,g=='known-rescue')

    def test_history_inventory_recurses_and_does_not_treat_booleans_as_seeds(self):
        self.assertEqual(E.collect_seeds({'seed':7,'nested':[{'seed':11},{'seed':True},{'seed':'12'}]}),{7,11})
        fresh={c['seed'] for c in E.cases(source()) if c['stage']=='held_out'}
        prior={c['seed'] for c in E.S.cases()}|{c['seed'] for c in E.P.plan()}
        self.assertFalse(fresh & prior)

    def test_freeze_rejects_runtime_source_inventory_case_and_binary_mutations(self):
        with tempfile.TemporaryDirectory() as directory:
            out=Path(directory);binary=out/'binary';binary.write_bytes(b'qualified binary')
            prior=source(binary);prior['binary']['sha256']=E.P.digest(binary)
            path=out/'source.json';path.write_text(json.dumps(prior))
            frozen={**prior['inputs'],E.OWN[0]:'runner',E.OWN[2]:'plan'}
            plan=dict(profile=E.PROFILE,namespace=E.NAMESPACE,inputs=frozen,
                source=dict(path=str(path),sha256=E.P.digest(path)),binary=prior['binary'],
                runtime_source_commit=prior['runtime_source_commit'],replay_sources=prior['runs'],
                jobs=E.jobs(prior,binary,out),history=dict(files={str(path):E.P.digest(path)},seeds=sorted(E.collect_seeds(prior))))
            with patch.object(E,'inputs',return_value=frozen),patch.object(E.X,'inputs',return_value=prior['inputs']):
                E.verify_plan(plan,out)
                for change in (lambda p:p['jobs'].pop(),lambda p:p['jobs'][12]['item'].update(seed=7),
                               lambda p:p['binary'].update(sha256='wrong'),lambda p:p['replay_sources'].clear(),
                               lambda p:p.update(runtime_source_commit='other'),lambda p:p['history']['seeds'].clear()):
                    changed=copy.deepcopy(plan);change(changed)
                    with self.assertRaises(AssertionError):E.verify_plan(changed,out,True)
            with patch.object(E.X,'inputs',return_value=prior['inputs']):
                with patch.object(E,'inputs',return_value={**frozen,E.OWN[0]:'fixed runner'}):
                    with self.assertRaises(AssertionError):E.verify_plan(plan,out)
                    E.verify_plan(plan,out,True)
                for key in ('runtime.rs',E.OWN[2]):
                    with patch.object(E,'inputs',return_value={**frozen,key:'changed'}):
                        with self.assertRaises(AssertionError):E.verify_plan(plan,out,True)

    def test_horizon_uses_final_physics_step_and_censors_later_counts(self):
        player=dict(ships_lost=2,completed_recoveries=1,death_tick=100,
            visits=[dict(claimed_tick=20,departed_tick=50,abandoned_tick=None),
                    dict(claimed_tick=None,departed_tick=None,abandoned_tick=90)])
        timeline=[dict(seat=0,tick=0,state=dict(ships_lost=0,completed_recoveries=0)),
                  dict(seat=0,tick=60,state=dict(ships_lost=1,completed_recoveries=0)),
                  dict(seat=1,tick=60,state=dict(ships_lost=9,completed_recoveries=9)),
                  dict(seat=0,tick=99,state=dict(ships_lost=1,completed_recoveries=1))]
        middle=E.horizon_player(player,timeline,0,60,100)
        self.assertEqual((middle['ships_lost'],middle['pilot_deaths'],middle['completed_departures']), (1,0,1))
        self.assertEqual(middle['abandoned_visits'],0)
        final=E.horizon_player(player,timeline,0,100,100)
        self.assertEqual((final['ships_lost'],final['pilot_deaths'],final['completed_recoveries']), (2,1,1))
        self.assertEqual(final['counter_source'],'native_final')
        self.assertEqual(final['abandoned_visits'],1)

    def test_missing_duplicate_mutated_or_unqualified_evidence_cannot_advance(self):
        runs,pairs=results()
        self.assertEqual(E.screen(runs,pairs,source())['decision'],'advance_to_normal_host_qualification')
        with self.assertRaises(AssertionError):E.screen(runs,pairs[:-1],source())
        dup=copy.deepcopy(pairs);dup[-1]=dup[0]
        with self.assertRaises(AssertionError):E.screen(runs,dup,source())
        bad=copy.deepcopy(pairs);bad[-1]['seat']=1-bad[-1]['seat']
        with self.assertRaises(AssertionError):E.screen(runs,bad,source())
        del runs[next(iter(runs))]['retention']
        with self.assertRaises(AssertionError):E.screen(runs,pairs,source())

    def test_secondary_can_preserve_fresh_games_but_primary_needs_fresh_benefit(self):
        runs,pairs=results()
        for p in pairs:
            if p['stage']=='held_out' and p['contrast']=='integrated->no-stop':
                p['benefits']=[];p['first_control_difference']=None
        result=E.screen(runs,pairs,source())
        self.assertEqual(result['decision'],'advance_to_normal_host_qualification')
        self.assertFalse(result['contrasts']['integrated->no-stop']['requires_fresh_benefit'])
        for p in pairs:
            if p['stage']=='held_out' and p['contrast']=='ordinary->no-stop':p['benefits']=[]
        result=E.screen(runs,pairs,source())
        self.assertEqual(result['decision'],'retain')
        self.assertIn('no_useful_fresh_change',result['contrasts']['ordinary->no-stop']['reasons'])
        self.assertFalse(result['default_promotion'])

    def test_one_world_loss_cannot_hide_behind_another_world_gain(self):
        runs,pairs=results()
        fresh=[p for p in pairs if p['stage']=='held_out' and p['contrast']=='ordinary->no-stop']
        bad=fresh[0];good=next(p for p in fresh if p['world_cluster']!=bad['world_cluster'] and p['seat']==bad['seat'] and p['interval']==bad['interval'])
        runs[bad['after']]['players'][bad['seat']]['outcome']='loss'
        runs[good['after']]['players'][good['seat']]['outcome']='win'
        result=E.screen(runs,pairs,source());reasons=result['contrasts']['ordinary->no-stop']['reasons']
        self.assertNotIn('points_regressed:overall',reasons)
        self.assertIn('points_regressed:world_cluster:0',reasons)
        self.assertEqual(result['decision'],'retain')

    def test_known_survival_costs_still_block_and_ship_cost_stays_reported(self):
        for regression in ('fewer_match_points','more_pilot_deaths','earlier_pilot_death'):
            runs,pairs=results();p=next(p for p in pairs if p['contrast']=='ordinary->no-stop' and p['stage']=='known_qualification')
            p['regressions']=[regression]
            self.assertEqual(E.screen(runs,pairs,source())['decision'],'retain')
        runs,pairs=results();p=next(p for p in pairs if p['contrast']=='ordinary->no-stop' and p['stage']=='known_qualification')
        p['regressions']=['more_ships_lost']
        result=E.screen(runs,pairs,source())
        self.assertEqual(result['decision'],'advance_to_normal_host_qualification')
        self.assertTrue(result['contrasts']['ordinary->no-stop']['regressions'])

    def test_fresh_losses_stalls_earlier_death_and_departures_are_separate_guards(self):
        runs,pairs=results();p=next(p for p in pairs if p['stage']=='held_out' and p['contrast']=='ordinary->no-stop')
        player=runs[p['after']]['players'][p['seat']]
        player.update(ships_lost=2,pilot_deaths=1)
        player['progress'].update(ticks_after_20s_without_progress=11,longest_no_progress_ticks=21)
        p['regressions']=['earlier_pilot_death'];p['adjusted_departures']['after']=0
        reasons=E.screen(runs,pairs,source())['contrasts']['ordinary->no-stop']['reasons']
        self.assertTrue({'ships_lost_increased','pilot_deaths_increased','worst_no_progress_ticks_increased',
                         'no_progress_fraction_increased','adjusted_departures_decreased'} <= set(reasons))
        self.assertTrue(any(r.startswith('earlier_pilot_death:') for r in reasons))
        for r in runs.values():
            for player in r['players']:player['progress']['eligible_ticks']=0
        self.assertIn('unknown_no_progress_fraction',E.screen(runs,pairs,source())['contrasts']['ordinary->no-stop']['reasons'])


if __name__ == '__main__':unittest.main()
