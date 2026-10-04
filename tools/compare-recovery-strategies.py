#!/usr/bin/env python3
"""Compare frozen strategy configurations after the shared recovery correction."""
import argparse
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
import copy
import importlib.util
from itertools import product
import json
from pathlib import Path
import shutil
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('rebuild', Path(__file__).with_name('validate-recovery-rebuild.py'))
R = importlib.util.module_from_spec(spec)
spec.loader.exec_module(R)
C, B, I, P, D, ROOT = R.C, R.B, R.I, R.P, R.D, R.ROOT
L = B.L
PROFILE = 'recovery_corrected_strategies_v1'
NAMESPACE = PROFILE + ':held-out:2026-10'
CONFIGS = {
    'control': ('control', False, False),
    'control-laser': ('control', True, False),
    'control-defense': ('control', True, True),
    'candidate': ('candidate', False, False),
    'candidate-laser': ('candidate', True, False),
    'candidate-defense': ('candidate', True, True),
}
KNOWN = (
    ('known-recovery', 11223442104665788832, 0, 10, 0),
    ('known-rescue', 4180701290234409703, 0, 10, 3),
    ('known-boundary', 14699744800433948105, 1, 10, 3),
    ('known-laser-loss', 7362648228662683534, 1, 9, 0),
)
CONTRASTS = (
    ('control', 'control-laser', 'laser'),
    ('control', 'control-defense', 'combined'),
    ('control', 'candidate', 'integration'),
    ('control', 'candidate-laser', 'combined'),
    ('control', 'candidate-defense', 'combined'),
    ('control-laser', 'control-defense', 'defense'),
    ('candidate', 'candidate-laser', 'laser'),
    ('candidate-laser', 'candidate-defense', 'defense'),
    ('control-laser', 'candidate-laser', 'integration'),
    ('control-defense', 'candidate-defense', 'integration'),
)
SOURCES = {
    'recovery': ROOT / 'target/recovery-rebuild/v1/summary.json',
    'recovery_plan': ROOT / 'target/recovery-rebuild/v1/plan.json',
    'clearance': ROOT / 'target/acquisition-clearance/v1/summary.json',
    'laser': ROOT / 'target/pursuit-climb-laser/fresh-v1/summary.json',
}
OWN_INPUTS = ('tools/compare-recovery-strategies.py', 'tools/tests/test_recovery_strategies.py',
              'docs/recovery-strategies-plan.md')


def inputs():
    return dict(R.inputs(), **{p: P.digest(ROOT / p) for p in OWN_INPUTS})


def cases():
    groups = [dict(group=name, seed=seed, seat=seat, opponent=opponent, interval=interval,
                   stage='known_qualification', world_cluster=name)
              for name, seed, seat, opponent, interval in KNOWN]
    for world, seat, interval in product(range(2), (0, 1), (0, 3)):
        namespace = f'{NAMESPACE}:{world}'
        groups.append(dict(group=f'fresh-world{world}-p{seat+1}-asteroids{interval}', seed=P.seed(namespace),
                           seed_namespace=namespace, seat=seat, opponent=10, interval=interval,
                           stage='held_out', world_cluster=world))
    result = []
    for index, group in enumerate(groups):
        names = list(CONFIGS)
        names = names[index % 6:] + names[:index % 6]
        if index == 0:
            names = ['candidate', 'candidate-laser'] + [n for n in names if n not in ('candidate', 'candidate-laser')]
        for configuration in names:
            arm, laser, defense = CONFIGS[configuration]
            result.append(dict(group, name=group['group'] + '-' + configuration, configuration=configuration,
                               arm=arm, laser=laser, defense=defense, clearance=defense,
                               world='generated', seconds=600))
    return result


def flags(item):
    result = B.flags(item)
    result[C.A.FLAG] = str(item['seat']) if item['defense'] else 'none'
    result[C.FLAG] = str(item['seat']) if item['clearance'] else 'none'
    if item['group'] == 'known-rescue':
        result.update({'--trace-impact': 'true', '--impact-start-tick': '7600',
                       '--impact-end-tick': str(D.END), '--impact-pod-control': 'bot'})
    return result


def jobs(binary, out):
    return [dict(item=item, command=[str(binary), *(v for pair in flags(item).items() for v in pair),
                                    '--out', str(out / 'raw' / item['name'])]) for item in cases()]


def native_audit(root, report, item):
    previous, attempts, requests = {}, {}, []
    count = 0
    for row in D.rows(root / 'trace.jsonl'):
        tick, seat = row['tick'], row['seat']
        assert (tick, seat) == (count // 2, count % 2)
        count += 1
        p, m = R.pilot(row), row['mission']
        assert p['tick'] == tick and p['owner'] == f'player_{seat+1}'
        before = previous.get(seat)
        task = m['recovery']
        receipt = (task or {}).get('rebuild_boarding')
        if task:
            assert task['task'] == 'recover_ship_v10'
        if receipt:
            key = (seat, task['started_tick'])
            if key not in attempts:
                assert before is not None
                attempts[key] = R.check_receipt(row, before)
            a = attempts[key]
            assert receipt == a['receipt'] and task['scuttle_attempts'] == a['scuttle_attempts']
            assert p['recovery']['rebuilds'] >= receipt['native_rebuilds']
            if task['status'] == 'running':
                assert tick <= receipt['deadline_tick'] and p['ship_available'] and p['ship_form'] == 'ship'
            elif task['status'] == 'blocked' and a['blocked_tick'] is None:
                a.update(blocked_tick=tick, blocked_reason=task['reason'])
        if before:
            oldtask = before['mission']['recovery']
            oldreceipt = (oldtask or {}).get('rebuild_boarding')
            if oldreceipt:
                a = attempts[(seat, oldtask['started_tick'])]
                completed = m['completed_recoveries'] > before['mission']['completed_recoveries']
                if completed:
                    assert p['location'] == {'aboard': p['vehicle']} and p['ship_available'] and p['ship_form'] == 'ship'
                    assert p['recovery']['ships_lost'] == a['ships_lost']
                    assert tick <= a['receipt']['deadline_tick'] + 1
                    a.update(completed_tick=tick, completion=R.point(row))
                elif task is None or task['started_tick'] != oldtask['started_tick']:
                    assert p['recovery']['ships_lost'] > a['ships_lost']
                    a.update(ended_by_loss_tick=tick)
        if item['laser'] and seat == item['seat']:
            check = m[L.FIELD]['last']
            if check and check['tick'] == tick and check['decision'] == 'requested':
                requests.append(tick)
        previous[seat] = row
    assert count == report['elapsed_ticks'] * 2
    if item['laser']:
        assert len(requests) == report['missions'][item['seat']][L.FIELD]['requested_ticks']
    result = dict(rows=count, request_ticks=requests, attempts=list(attempts.values()))
    I.write(root / 'native-recovery-audit.json', result)
    return result


def departure_accounting(a, b, elapsed_a, elapsed_b):
    """Only never-started post-victory opportunities are exempt from the count gate."""
    horizon = min(elapsed_a, elapsed_b)
    players, endings = {'before': a, 'after': b}, {'before': elapsed_a, 'after': elapsed_b}
    exempt, common = {}, {}
    for side, other in (('before', 'after'), ('after', 'before')):
        completed = [v for v in players[side]['visits'] if v['departed_tick'] is not None]
        exempt[side] = [v for v in completed if endings[other] < endings[side]
                       and players[other]['outcome'] == 'win' and v['selected_tick'] > endings[other]]
        common[side] = [v for v in completed if v['departed_tick'] <= horizon]
    return dict(common_horizon=horizon, common_horizon_visits=common, post_victory_exempt_visits=exempt,
                adjusted_departures={side: players[side]['completed_departures'] - len(exempt[side])
                                     for side in players})


def compare(a, b, kind, reports):
    ar, br = L.root_of(a), L.root_of(b)
    ra, rb = reports[a['item']['name']], reports[b['item']['name']]
    assert ra['initial_world'] == rb['initial_world']
    difference = I.first_control_difference(ar / 'destination-behavior.jsonl', br / 'destination-behavior.jsonl')
    physical_keys = ('round', 'final_pilots', 'final_planets', 'final_audit', 'final_combat', 'asteroid_events', 'elapsed_ticks')
    same = all(ra[k] == rb[k] for k in physical_keys)
    if difference is None:
        assert same, 'physical changes without changed actions'
    else:
        assert difference['reason'] == 'actions', 'ending changed before a changed action'
    option_audit = None
    if kind == 'laser':
        option_audit = B.compare([a, b], {r['item']['name']: r for r in (a, b)})
    elif kind == 'defense':
        option_audit = C.compare(a, b)
    item = a['item']; seat = item['seat']
    x, y = a['players'][seat], b['players'][seat]
    accounting = departure_accounting(x, y, a['elapsed_ticks'], b['elapsed_ticks'])
    useful, missing = I.completion_changes(x, y, difference['tick'] if difference else None)
    useful = [v for v in useful if v['candidate'] not in accounting['post_victory_exempt_visits']['after']]
    benefits, regressions = C.changes(x, y)
    if useful:
        benefits.append('earlier_or_additional_departure')
    if difference is None:
        assert not benefits and not regressions and not useful
    return dict(group=item['group'], stage=item['stage'], seat=seat, opponent=item['opponent'],
                interval=item['interval'], world_cluster=item['world_cluster'], seed=item['seed'],
                contrast=a['item']['configuration'] + '->' + b['item']['configuration'], kind=kind,
                before=a['item']['name'], after=b['item']['name'], first_control_difference=difference,
                same_physical_outcomes=same, option_audit=option_audit,
                outcome_transition=x['outcome'] + '->' + y['outcome'], benefits=benefits,
                regressions=regressions, useful_completed_changes=useful,
                missing_before_completions=missing, **accounting)


def screen_contrast(runs, pairs):
    fresh = [p for p in pairs if p['stage'] == 'held_out']
    known = [p for p in pairs if p['stage'] == 'known_qualification']
    assert len(fresh) == 8 and len(known) == 4
    def totals(selected):
        return {side: I.totals([runs[p[side]]['players'][p['seat']] for p in selected])
                for side in ('before', 'after')}
    strata = {'overall': totals(fresh)}
    for key in ('seat', 'interval', 'world_cluster'):
        for value in sorted({p[key] for p in fresh}):
            strata[f'{key}:{value}'] = totals([p for p in fresh if p[key] == value])
    reasons = []
    if not any(p['benefits'] for p in fresh):
        reasons.append('no_useful_fresh_change')
    for key, values in strata.items():
        if values['after']['points'] < values['before']['points']:
            reasons.append('points_regressed:' + key)
    a, b = strata['overall']['before'], strata['overall']['after']
    for key in ('ships_lost', 'pilot_deaths', 'worst_no_progress_ticks'):
        if b[key] > a[key]:
            reasons.append(key + '_increased')
    adjusted = {side: sum(p['adjusted_departures'][side] for p in fresh) for side in ('before', 'after')}
    if adjusted['after'] < adjusted['before']:
        reasons.append('adjusted_departures_decreased')
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
    return dict(decision='retain' if reasons else 'advance_to_further_evaluation', reasons=reasons,
                strata=strata, known_totals=totals(known), adjusted_departures=adjusted,
                changed_fresh_pairs=sum(p['first_control_difference'] is not None for p in fresh),
                useful_fresh_pairs=sum(bool(p['benefits']) for p in fresh),
                fresh_outcomes=dict(Counter(p['outcome_transition'] for p in fresh)),
                regressions=[dict(group=p['group'], stage=p['stage'], changes=p['regressions'])
                             for p in pairs if p['regressions']])


def screen(runs, comparisons):
    expected = cases()
    assert set(runs) == {c['name'] for c in expected}
    assert all(runs[c['name']]['item'] == c for c in expected)
    assert len(comparisons) == 120
    keys = {(c['group'], a + '->' + b) for c in expected for a, b, _ in CONTRASTS}
    assert len(keys) == len(comparisons) == len({(p['group'], p['contrast']) for p in comparisons})
    assert keys == {(p['group'], p['contrast']) for p in comparisons}
    contrasts = {a + '->' + b: screen_contrast(runs, [p for p in comparisons if p['contrast'] == a + '->' + b])
                 for a, b, _ in CONTRASTS}
    requirements = {
        'control-laser': ['control->control-laser'],
        'control-defense': ['control->control-defense', 'control->control-laser', 'control-laser->control-defense'],
        'candidate': ['control->candidate'],
        'candidate-laser': ['control->candidate-laser', 'control->candidate', 'candidate->candidate-laser'],
        'candidate-defense': ['control->candidate-defense', 'control->candidate', 'candidate->candidate-laser',
                              'candidate-laser->candidate-defense'],
    }
    configurations = {name: dict(required_contrasts=edges,
        decision='advance_to_further_evaluation' if all(contrasts[k]['decision'] != 'retain' for k in edges) else 'retain')
        for name, edges in requirements.items()}
    return dict(contrasts=contrasts, configurations=configurations, default_promotion=False,
                independent_fresh_worlds=2, fresh_opponents=[10],
                decision='advance_to_further_evaluation' if any(
                    v['decision'] != 'retain' for v in configurations.values()) else 'retain')


def freeze(out):
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()
    sources = {k: json.loads(p.read_text()) for k, p in SOURCES.items()}
    prior = sources['recovery']
    assert prior['complete'] and prior['inputs'] == R.inputs()
    assert P.digest(prior['binary']['path']) == prior['binary']['sha256']
    fresh = {c['seed'] for c in cases() if c['stage'] == 'held_out'}
    old_seeds = {c['seed'] for c in P.plan()} | {c['seed'] for c in B.cases()}
    old_seeds |= {r['item']['seed'] for s in sources.values() if 'runs' in s for r in s['runs'].values()}
    old_seeds |= {r['item']['seed'] for r in json.loads(C.PRIOR.read_text())['runs'].values()}
    assert len(fresh) == 2 and not fresh & old_seeds
    out.mkdir(parents=True, exist_ok=False)
    for name in ('raw', 'logs', 'archives', 'inputs'):
        (out / name).mkdir()
    binary = out / 'surface_mission_soak'
    shutil.copy2(prior['binary']['path'], binary); binary.chmod(0o555)
    source_refs = {k: dict(path=str(p), sha256=P.digest(p)) for k, p in SOURCES.items()}
    source_refs['defense'] = dict(path=str(C.PRIOR), sha256=P.digest(C.PRIOR))
    plan = dict(schema=1, profile=PROFILE, namespace=NAMESPACE,
                runtime_source_commit=prior['source_commit'], runner_commit=subprocess.check_output(
                    ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                binary=dict(path=str(binary), sha256=P.digest(binary)), inputs=inputs(), sources=source_refs,
                jobs=jobs(binary, out), default_changes=False,
                replay_sources={'known-recovery-candidate': prior['runs']['known-laser-off-win-off'],
                                'known-recovery-candidate-laser': prior['runs']['known-lost-win-off']})
    I.write(out / 'plan.json', plan)
    print('Frozen 24 known and 48 fresh games; six configurations, two fresh worlds.', flush=True)


def verify_plan(plan, out, reaudit=False):
    assert plan['profile'] == PROFILE and plan['namespace'] == NAMESPACE
    current = inputs()
    assert set(current) == set(plan['inputs'])
    changed = {k for k in current if current[k] != plan['inputs'][k]}
    assert not changed or reaudit and changed <= set(OWN_INPUTS[:2])
    assert plan['jobs'] == jobs(Path(plan['binary']['path']), out)
    assert P.digest(plan['binary']['path']) == plan['binary']['sha256']
    assert all(P.digest(s['path']) == s['sha256'] for s in plan['sources'].values())
    prior = json.loads(Path(plan['sources']['recovery']['path']).read_text())
    assert plan['binary']['sha256'] == prior['binary']['sha256']
    assert plan['runtime_source_commit'] == prior['source_commit']
    assert plan['replay_sources'] == {'known-recovery-candidate': prior['runs']['known-laser-off-win-off'],
                                     'known-recovery-candidate-laser': prior['runs']['known-lost-win-off']}


def run_case(job, plan, out, reaudit):
    name = job['item']['name']; root = L.root_of(job)
    log, metadata = out / 'logs' / (name + '.log'), out / (name + '-raw.json')
    result = copy.deepcopy(job)
    try:
        if metadata.exists():
            assert reaudit, 'existing run requires explicit evidence reuse'
            old = json.loads(metadata.read_text())
            assert old['item'] == job['item'] and old['command'] == job['command']
            assert P.digest(log) == old['log_sha256']
            result.update(hashes=old['hashes'], log_sha256=old['log_sha256'], reused_raw=True)
            if not root.exists():
                record = json.loads((out / (name + '-result.json')).read_text())
                B.unpack(record['archive'], root)
        else:
            assert not root.exists()
            with log.open('x') as stream:
                subprocess.run(job['command'], stdout=stream, stderr=stream, check=True, timeout=1800)
            result.update(hashes=I.raw_hashes(root), log_sha256=P.digest(log))
            I.write(metadata, result)
        assert all(P.digest(root / k) == v for k, v in result['hashes'].items())
        result.update(I.analyze(root, job, root))
        report = json.loads((root / 'report.json').read_text())
        result['defense'] = C.A.audit(root, job['item'], report)
        result['clearance'] = C.audit(root, job['item'], report)
        result['native'] = native_audit(root, report, job['item'])
        result['laser'] = dict(request_ticks=result['native']['request_ticks'])
        if (root / 'impact.jsonl').exists():
            result['impact'] = C.A.audit_impact(root, report)
        if name in plan['replay_sources']:
            source = plan['replay_sources'][name]; oldroot = out / 'inputs' / name
            if not oldroot.exists():
                B.unpack(source['archive'], oldroot)
            assert I.raw_hashes(oldroot) == {k: v['sha256'] for k, v in source['archive']['files'].items()}
            result['retention'] = C.parity(source, result, oldroot)
            shutil.rmtree(oldroot)
        assert all(P.digest(root / k) == v for k, v in result['hashes'].items())
        result['audited'] = True
    except Exception:
        result['error'] = traceback.format_exc()
    I.write(out / (name + '-result.json'), result)
    print(name, 'audited' if result.get('audited') else result['error'], flush=True)
    return result


def pack_run(run, out, reaudit):
    root = L.root_of(run)
    assert all(P.digest(root / k) == v for k, v in run['hashes'].items())
    suffix = '-' + P.digest(__file__)[:12] if reaudit else ''
    run['archive'] = B.pack(root, out / 'archives' / (root.name + suffix + '.tar.gz'))
    I.write(out / (root.name + '-result.json'), run)
    return run


def execute(path, reaudit):
    plan = json.loads(path.read_text()); out = path.parent
    verify_plan(plan, out, reaudit)
    summary_path = out / 'summary.json'
    if summary_path.exists():
        assert reaudit
        backup = out / ('summary-before-reaudit-' + P.digest(summary_path)[:12] + '.json')
        assert not backup.exists(); shutil.copy2(summary_path, backup)
    summary = dict(schema=1, profile=PROFILE, complete=False, plan_sha256=P.digest(path),
                   runtime_source_commit=plan['runtime_source_commit'], runner_commit=plan['runner_commit'],
                   binary=plan['binary'], inputs=inputs(), reaudit=reaudit, runs={}, comparisons=[])
    I.write(summary_path, summary)
    groups = list(dict.fromkeys(j['item']['group'] for j in plan['jobs']))
    with ProcessPoolExecutor(max_workers=2) as pool:
        for group in groups:
            selected = [j for j in plan['jobs'] if j['item']['group'] == group]
            batches = [selected[:2], selected[2:]] if group == 'known-recovery' else [selected]
            for batch in batches:
                futures = [(j['item']['name'], pool.submit(run_case, j, plan, out, reaudit)) for j in batch]
                failed = []
                for name, future in futures:
                    result = future.result(); summary['runs'][name] = result
                    I.write(summary_path, summary)
                    if not result.get('audited'):
                        failed.append(name)
                assert not failed, f'invalid games retained: {failed}'
            by_config = {j['item']['configuration']: summary['runs'][j['item']['name']] for j in selected}
            reports = {r['item']['name']: json.loads((L.root_of(r) / 'report.json').read_text()) for r in by_config.values()}
            try:
                comparisons = [compare(by_config[a], by_config[b], kind, reports) for a, b, kind in CONTRASTS]
                I.write(out / (group + '-comparisons.json'), comparisons)
                summary['comparisons'].extend(comparisons)
            except Exception:
                summary['error'] = traceback.format_exc(); I.write(summary_path, summary)
                raise
            del reports
            futures = [pool.submit(pack_run, r, out, reaudit) for r in by_config.values()]
            for future in futures:
                run = future.result(); summary['runs'][run['item']['name']] = run
                I.write(summary_path, summary)
            if group == 'known-laser-loss':
                summary['qualification'] = dict(passed=True, games=24, parity_replays=2)
            I.write(summary_path, summary)
            print(group, 'compared and archived;', len(summary['runs']), '/ 72 games', flush=True)
    summary['screen'] = screen(summary['runs'], summary['comparisons'])
    summary['complete'] = True
    I.write(summary_path, summary)
    print('Complete:', summary['screen']['decision'], flush=True)


if __name__ == '__main__':
    assert __debug__, 'auditors require Python assertions enabled'
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    p = sub.add_parser('plan'); p.add_argument('--out', type=Path, required=True)
    p = sub.add_parser('run'); p.add_argument('--plan', type=Path, required=True); p.add_argument('--reaudit', action='store_true')
    args = parser.parse_args()
    if args.action == 'plan': freeze(args.out.resolve())
    else: execute(args.plan.resolve(), args.reaudit)
