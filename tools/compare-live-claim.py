#!/usr/bin/env python3
"""Freeze a balanced fresh-world screen of the qualified live-claim ablation."""
import argparse
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
import importlib.util
from itertools import product
import json
from pathlib import Path
import shutil
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('claim', Path(__file__).with_name('diagnose-live-claim.py'))
X = importlib.util.module_from_spec(spec); spec.loader.exec_module(X)
Q, S, C, B, I, P, ROOT = X.Q, X.S, X.C, X.B, X.I, X.P, X.ROOT
PROFILE = 'live_claim_broader_v1'
NAMESPACE = PROFILE + ':held-out:2026-10'
SOURCE = ROOT / 'target/live-claim-ablation/v1/summary.json'
OWN = ('tools/compare-live-claim.py', 'tools/tests/test_live_claim_evaluation.py',
       'docs/live-claim-evaluation-plan.md')


def inputs():
    return dict(X.inputs(), **{p: P.digest(ROOT / p) for p in OWN})


def cases(source):
    result = []
    for group in Q.GROUPS:
        for config in X.CONFIGS:
            original = source['runs'][group + '-' + config]['item']
            result.append(dict(original, stage='known_qualification'))
    for index, (world, interval, seat) in enumerate(product(range(4), (0, 3), (0, 1))):
        namespace = f'{NAMESPACE}:{world}'
        group = f'new-world{world}-p{seat+1}-asteroids{interval}'
        order = X.CONFIGS[index % 3:] + X.CONFIGS[:index % 3]
        for config in order:
            candidate = config != 'ordinary'
            result.append(dict(name=group + '-' + config, group=group, configuration=config,
                stage='held_out', world='generated', world_cluster=world, seed=P.seed(namespace),
                seed_namespace=namespace, seat=seat, opponent=10, interval=interval, seconds=600,
                arm='candidate' if candidate else 'control', mask=7 if candidate else 0,
                live_claim_stopping=config != 'no-stop', laser=False, defense=False, clearance=False))
    return result


def jobs(source, binary, out):
    return [dict(item=item, command=[str(binary), *(v for pair in X.flags(item).items() for v in pair),
                                    '--out', str(out / 'raw' / item['name'])]) for item in cases(source)]


def collect_seeds(value):
    found = set()
    if isinstance(value, dict):
        if type(value.get('seed')) is int:
            found.add(value['seed'])
        for child in value.values():
            found.update(collect_seeds(child))
    elif isinstance(value, list):
        for child in value:
            found.update(collect_seeds(child))
    return found


def history_inventory():
    paths = set((ROOT / 'docs/data').glob('*.json'))
    for pattern in ('*/plan.json', '*/*/plan.json', '*/summary.json', '*/*/summary.json'):
        paths.update((ROOT / 'target').glob(pattern))
    seeds, files = set(), {}
    for path in sorted(paths):
        before = P.digest(path)
        seeds.update(collect_seeds(json.loads(path.read_text())))
        assert P.digest(path) == before
        files[str(path)] = before
    return dict(files=files, seeds=sorted(seeds))


def verify_plan(plan, out, reaudit=False):
    assert plan['profile'] == PROFILE and plan['namespace'] == NAMESPACE
    current = inputs()
    assert set(current) == set(plan['inputs'])
    changed = {k for k in current if current[k] != plan['inputs'][k]}
    assert not changed or reaudit and changed <= set(OWN[:2])
    assert P.digest(plan['source']['path']) == plan['source']['sha256']
    source = json.loads(Path(plan['source']['path']).read_text())
    assert source['complete'] and source['profile'] == X.PROFILE and source['inputs'] == X.inputs()
    assert plan['jobs'] == jobs(source, Path(plan['binary']['path']), out)
    assert P.digest(plan['binary']['path']) == plan['binary']['sha256'] == source['binary']['sha256']
    assert plan['runtime_source_commit'] == source['runtime_source_commit']
    assert plan['replay_sources'] == source['runs']
    assert all(P.digest(path) == digest for path, digest in plan['history']['files'].items())
    actual_seeds = set()
    for path in plan['history']['files']:
        actual_seeds.update(collect_seeds(json.loads(Path(path).read_text())))
    assert plan['history']['seeds'] == sorted(actual_seeds)
    fresh = {j['item']['seed'] for j in plan['jobs'] if j['item']['stage'] == 'held_out'}
    assert len(fresh) == 4 and not fresh.intersection(plan['history']['seeds'])
    for job in plan['jobs'][:12]:
        assert job['command'][1:-1] == source['runs'][job['item']['name']]['command'][1:-1]
    return source


def freeze(out):
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()
    source = json.loads(SOURCE.read_text())
    assert source['complete'] and source['profile'] == X.PROFILE and source['inputs'] == X.inputs()
    assert P.digest(source['binary']['path']) == source['binary']['sha256']
    history = history_inventory()
    out.mkdir(parents=True, exist_ok=False)
    for name in ('raw', 'logs', 'archives', 'inputs'):
        (out / name).mkdir()
    binary = out / 'surface_mission_soak'
    shutil.copy2(source['binary']['path'], binary); binary.chmod(0o555)
    plan = dict(schema=1, profile=PROFILE, namespace=NAMESPACE, inputs=inputs(), history=history,
        source=dict(path=str(SOURCE), sha256=P.digest(SOURCE)),
        runtime_source_commit=source['runtime_source_commit'],
        runner_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        binary=dict(path=str(binary), sha256=P.digest(binary)), jobs=jobs(source, binary, out),
        replay_sources=source['runs'], default_changes=False, independent_fresh_worlds=4,
        fresh_games=48, known_games=12)
    verify_plan(plan, out)
    I.write(out / 'plan.json', plan)
    print('Frozen 12 retention replays and 48 fresh games: four worlds, both seats, asteroids 0/3.', flush=True)


def run_case(job, plan, out, reaudit):
    # Keep process-pool entry points in this module; imported helpers run locally.
    result = X.run_case(job, out, reaudit)
    if not result.get('audited'):
        return result
    name = job['item']['name']
    try:
        if name in plan['replay_sources']:
            prior = plan['replay_sources'][name]; oldroot = out / 'inputs' / name
            if not oldroot.exists():
                B.unpack(prior['archive'], oldroot)
            result['retention'] = C.parity(prior, result, oldroot)
            assert result['stopping_rule'] == prior['stopping_rule']
            assert json.loads(json.dumps(result['native'])) == prior['native']
            shutil.rmtree(oldroot)
            print(name, 'exact retention verified', flush=True)
    except Exception:
        result.pop('audited', None)
        result['error'] = traceback.format_exc()
        print(name, result['error'], flush=True)
    I.write(out / (name + '-result.json'), result)
    return result


def horizon_player(player, timeline, seat, horizon, ending):
    assert 0 < horizon <= ending
    if horizon == ending:
        ships, recoveries, origin = player['ships_lost'], player['completed_recoveries'], 'native_final'
    else:
        events = [e for e in timeline if e['seat'] == seat and e['tick'] <= horizon]
        assert events
        state = events[-1]['state']
        ships, recoveries, origin = state['ships_lost'], state['completed_recoveries'], 'consumed_observation'
        assert type(ships) is int and ships >= 0
    return dict(ships_lost=ships, completed_recoveries=recoveries, counter_source=origin,
        pilot_deaths=int(player['death_tick'] is not None and player['death_tick'] <= horizon),
        claims=sum(v['claimed_tick'] is not None and v['claimed_tick'] <= horizon for v in player['visits']),
        completed_departures=sum(v['departed_tick'] is not None and v['departed_tick'] <= horizon for v in player['visits']),
        abandoned_visits=sum(v['abandoned_tick'] is not None and v['abandoned_tick'] <= horizon for v in player['visits']))


def compare_group(runs):
    pairs = X.compare_group(runs)
    timelines = {r['item']['name']: json.loads((S.L.root_of(r) / 'factor-witness-audit.json').read_text())['timeline']
                 for r in runs.values()}
    by_name = {r['item']['name']: r for r in runs.values()}
    for pair in pairs:
        item = by_name[pair['before']]['item']
        pair.update(**{k: item[k] for k in ('stage', 'opponent', 'interval', 'world_cluster', 'seed')},
                    contrast=pair['before_configuration'] + '->' + pair['after_configuration'])
        pair['common_horizon_players'] = {side: [horizon_player(p, timelines[pair[side]], seat,
            pair['common_horizon'], by_name[pair[side]]['elapsed_ticks'])
            for seat, p in enumerate(by_name[pair[side]]['players'])] for side in ('before', 'after')}
        for side in ('before', 'after'):
            assert pair['common_horizon_players'][side][pair['seat']]['completed_departures'] == len(pair['common_horizon_visits'][side])
    return pairs


def screen_contrast(runs, pairs, require_fresh_benefit):
    fresh = [p for p in pairs if p['stage'] == 'held_out']
    known = [p for p in pairs if p['stage'] == 'known_qualification']
    assert len(fresh) == 16 and len(known) == 4
    def totals(selected):
        return {side: I.totals([runs[p[side]]['players'][p['seat']] for p in selected])
                for side in ('before', 'after')}
    strata = {'overall': totals(fresh)}
    for key in ('seat', 'interval', 'world_cluster'):
        for value in sorted({p[key] for p in fresh}):
            strata[f'{key}:{value}'] = totals([p for p in fresh if p[key] == value])
    reasons = []
    if require_fresh_benefit and not any(p['benefits'] for p in fresh):
        reasons.append('no_useful_fresh_change')
    for key, values in strata.items():
        if values['after']['points'] < values['before']['points']:
            reasons.append('points_regressed:' + key)
    a, b = strata['overall']['before'], strata['overall']['after']
    for key in ('ships_lost', 'pilot_deaths', 'worst_no_progress_ticks'):
        if b[key] > a[key]: reasons.append(key + '_increased')
    adjusted = {side: sum(p['adjusted_departures'][side] for p in fresh) for side in ('before', 'after')}
    if adjusted['after'] < adjusted['before']: reasons.append('adjusted_departures_decreased')
    if not a['eligible_ticks'] or not b['eligible_ticks']:
        reasons.append('unknown_no_progress_fraction')
    elif b['ticks_after_20s_without_progress'] * a['eligible_ticks'] > a['ticks_after_20s_without_progress'] * b['eligible_ticks']:
        reasons.append('no_progress_fraction_increased')
    for p in fresh:
        if 'earlier_pilot_death' in p['regressions']:
            reasons.append('earlier_pilot_death:' + p['group'])
    for p in known:
        if any(k in p['regressions'] for k in ('fewer_match_points', 'more_pilot_deaths', 'earlier_pilot_death')):
            reasons.append('known_regression:' + p['group'])
    common = {side: {key: sum(p['common_horizon_players'][side][p['seat']][key] for p in fresh)
               for key in ('ships_lost', 'pilot_deaths', 'claims', 'completed_departures', 'completed_recoveries', 'abandoned_visits')}
              for side in ('before', 'after')}
    return dict(decision='retain' if reasons else 'pass', reasons=reasons,
        requires_fresh_benefit=require_fresh_benefit, strata=strata, known_totals=totals(known),
        adjusted_departures=adjusted, common_horizon_totals=common,
        changed_fresh_pairs=sum(p['first_control_difference'] is not None for p in fresh),
        useful_fresh_pairs=sum(bool(p['benefits']) for p in fresh),
        fresh_outcomes=dict(Counter(p['outcome_transition'] for p in fresh)),
        regressions=[dict(group=p['group'], stage=p['stage'], changes=p['regressions']) for p in pairs if p['regressions']])


def screen(runs, pairs, source):
    expected = {c['name']: c for c in cases(source)}
    assert set(runs) == set(expected) and all(r['item'] == expected[n] and r['audited'] for n, r in runs.items())
    assert len(pairs) == 60
    assert {(p['group'], p['contrast']) for p in pairs} == {
        (c['group'], a + '->' + b) for c in expected.values() for a, b in X.CONTRASTS}
    for pair in pairs:
        item = expected[pair['before']]
        assert pair['before'] == pair['group'] + '-' + pair['before_configuration']
        assert pair['after'] == pair['group'] + '-' + pair['after_configuration']
        assert pair['contrast'] == pair['before_configuration'] + '->' + pair['after_configuration']
        assert all(pair[k] == item[k] for k in ('stage', 'seat', 'opponent', 'interval', 'world_cluster', 'seed'))
    assert all(r.get('retention') for r in runs.values() if r['item']['stage'] == 'known_qualification')
    contrasts = {a + '->' + b: screen_contrast(runs, [p for p in pairs if p['contrast'] == a + '->' + b],
                                             a == 'ordinary') for a, b in X.CONTRASTS}
    required = ('ordinary->no-stop', 'integrated->no-stop')
    return dict(contrasts=contrasts, required_contrasts=list(required), default_promotion=False,
        independent_fresh_worlds=4, fresh_opponents=[10], fresh_games=48,
        decision='advance_to_normal_host_qualification' if all(contrasts[k]['decision'] == 'pass' for k in required) else 'retain')


def pack(run, out, reaudit):
    root = S.L.root_of(run)
    assert all(P.digest(root / k) == v for k, v in run['hashes'].items())
    suffix = '-' + P.digest(__file__)[:12] if reaudit else ''
    run['archive'] = B.pack(root, out / 'archives' / (root.name + suffix + '.tar.gz'))
    I.write(out / (root.name + '-result.json'), run)
    return run


def execute(path, reaudit):
    plan = json.loads(path.read_text()); out = path.parent
    source = verify_plan(plan, out, reaudit); target = out / 'summary.json'
    if target.exists():
        assert reaudit
        backup = out / ('summary-before-reaudit-' + P.digest(target)[:12] + '.json')
        assert not backup.exists(); shutil.copy2(target, backup)
    summary = dict(schema=1, profile=PROFILE, complete=False, plan_sha256=P.digest(path), inputs=inputs(),
        runtime_source_commit=plan['runtime_source_commit'], runner_commit=plan['runner_commit'],
        binary=plan['binary'], reaudit=reaudit, runs={}, comparisons=[])
    I.write(target, summary)
    try:
        with ProcessPoolExecutor(max_workers=2) as pool:
            for group in dict.fromkeys(j['item']['group'] for j in plan['jobs']):
                selected = [j for j in plan['jobs'] if j['item']['group'] == group]
                for batch in (selected[:2], selected[2:]):
                    futures = [pool.submit(run_case, j, plan, out, reaudit) for j in batch]
                    failed = []
                    for future in futures:
                        run = future.result(); summary['runs'][run['item']['name']] = run
                        I.write(target, summary)
                        if not run.get('audited'): failed.append(run['item']['name'])
                    assert not failed, f'invalid games retained: {failed}'
                runs = {j['item']['configuration']: summary['runs'][j['item']['name']] for j in selected}
                pairs = compare_group(runs)
                I.write(out / (group + '-comparisons.json'), pairs)
                summary['comparisons'].extend(pairs); I.write(target, summary)
                for future in [pool.submit(pack, r, out, reaudit) for r in runs.values()]:
                    run = future.result(); summary['runs'][run['item']['name']] = run; I.write(target, summary)
                if len(summary['runs']) == 12:
                    summary['qualification'] = dict(passed=True, exact_replays=12)
                    I.write(target, summary)
                print(group, 'compared and archived;', len(summary['runs']), '/ 60 games', flush=True)
        verify_plan(plan, out, reaudit)
        summary['screen'] = screen(summary['runs'], summary['comparisons'], source)
        summary['complete'] = True
    except BaseException:
        summary['error'] = traceback.format_exc()
        raise
    finally:
        I.write(target, summary)
    print('Complete:', summary['screen']['decision'], flush=True)


if __name__ == '__main__':
    assert __debug__, 'auditors require assertions enabled'
    parser = argparse.ArgumentParser(description=__doc__); sub = parser.add_subparsers(dest='action', required=True)
    p = sub.add_parser('plan'); p.add_argument('--out', type=Path, required=True)
    p = sub.add_parser('run'); p.add_argument('--plan', type=Path, required=True); p.add_argument('--reaudit', action='store_true')
    args = parser.parse_args()
    if args.action == 'plan': freeze(args.out.resolve())
    else: execute(args.plan.resolve(), args.reaudit)
