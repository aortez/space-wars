#!/usr/bin/env python3
"""Compare one live-claim stopping ablation against both current-runtime endpoints."""
import argparse
from concurrent.futures import ProcessPoolExecutor
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('factors', Path(__file__).with_name('diagnose-integration-factors.py'))
Q = importlib.util.module_from_spec(spec); spec.loader.exec_module(Q)
S, C, B, I, P, R, ROOT = Q.S, Q.C, Q.B, Q.I, Q.P, Q.R, Q.ROOT
PROFILE = 'live_claim_stopping_ablation_v1'
BASE = '08e730f1ed276db06aada75fef253bb0d20b16dd'
CONFIGS = ('ordinary', 'integrated', 'no-stop')
CONTRASTS = (('ordinary', 'integrated'), ('ordinary', 'no-stop'), ('integrated', 'no-stop'))
OWN = ('tools/diagnose-live-claim.py', 'tools/tests/test_live_claim.py', 'docs/live-claim-ablation-plan.md')


def inputs():
    paths = subprocess.check_output(['git', 'ls-files', 'crates', 'scenarios', 'vendor/rapier2d'],
                                    cwd=ROOT, text=True).splitlines()
    paths = [p for p in paths if p.endswith('.rs') or Path(p).name == 'Cargo.toml']
    return dict(Q.inputs(), **{p: P.digest(ROOT / p) for p in
                (*paths, 'Cargo.toml', 'Cargo.lock', '.cargo/config.toml', *OWN)})


def cases(source):
    result = []
    endpoints = {(c['group'], c['mask']): c for c in Q.cases(source)}
    for group in Q.GROUPS:
        for configuration in CONFIGS:
            original = endpoints[group, 0 if configuration == 'ordinary' else 7]
            result.append(dict(original, name=group + '-' + configuration,
                configuration=configuration, live_claim_stopping=configuration != 'no-stop',
                stage='selected_current_runtime_ablation'))
    return result


def flags(item):
    return dict(Q.flags(item), **{'--live-claim-stopping': str(item['live_claim_stopping']).lower()})


def jobs(source, binary, out):
    return [dict(item=item, command=[str(binary), *(v for pair in flags(item).items() for v in pair),
                                    '--out', str(out / 'raw' / item['name'])]) for item in cases(source)]


def check_configuration(report, item):
    Q.check_configuration(report, item)
    expected = [item['mask'] == 7 and item['live_claim_stopping'] and s == item['seat'] for s in (0, 1)]
    assert report.get('powered_capture', {}).get('live_claim_stopping_enabled_seats', [False, False]) == expected


def live_claim(row):
    p = R.pilot(row); claim = p['planet']['claim'] or {}; flag = claim.get('flag') or {}
    return (p['balanced'] and p['supported_planet'] == p['planet']['index']
            and claim.get('owner') is None and claim.get('claimant') == p['owner']
            and claim.get('phase') == 'raising' and claim.get('status') == 'raising'
            and flag.get('player') == p['owner'])


def audit_switch(rows, report, item):
    counts, ground_counts, claim_counts = [0, 0], [0, 0], [0, 0]
    expected = [not item['live_claim_stopping'] and s == item['seat'] for s in (0, 1)]
    for row in rows:
        s = row['seat']; m = row['mission']; counts[s] += 1
        assert m.get('live_claim_stopping_disabled', False) == expected[s]
        ground = (m['capture'] or {}).get('ground')
        if ground:
            ground_counts[s] += 1
            assert ground.get('live_claim_stopping_disabled', False) == expected[s]
            if item['mask'] == 7 and s == item['seat']:
                assert ground.get('continuous_walk') is True
            claim_counts[s] += int(live_claim(row))
    assert counts == [report['elapsed_ticks']] * 2
    assert [m.get('live_claim_stopping_disabled', False) for m in report['missions']] == expected
    return dict(disabled_seats=expected, dense_rows=counts, capture_ground_rows=ground_counts,
                supported_own_raise_rows=claim_counts)


def verify_plan(plan, out, reaudit=False):
    assert plan['profile'] == PROFILE and plan['base_commit'] == BASE
    current = inputs()
    assert set(current) == set(plan['inputs'])
    changed = {k for k in current if current[k] != plan['inputs'][k]}
    assert not changed or reaudit and changed <= set(OWN[:2])
    assert P.digest(plan['source']['path']) == plan['source']['sha256']
    source = json.loads(Path(plan['source']['path']).read_text())
    assert source['complete'] and source['profile'] == S.PROFILE
    assert plan['jobs'] == jobs(source, Path(plan['binary']['path']), out)
    assert P.digest(plan['binary']['path']) == plan['binary']['sha256']
    return source


def freeze(out, source_binary):
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()
    subprocess.run(['git', 'merge-base', '--is-ancestor', BASE, 'HEAD'], cwd=ROOT, check=True)
    source = json.loads(Q.SOURCE.read_text())
    assert source['complete'] and source['profile'] == S.PROFILE
    out.mkdir(parents=True, exist_ok=False)
    for name in ('raw', 'logs', 'archives'):
        (out / name).mkdir()
    binary = out / 'surface_mission_soak'
    before = P.digest(source_binary)
    shutil.copy2(source_binary, binary); binary.chmod(0o555)
    assert P.digest(binary) == before
    plan = dict(schema=1, profile=PROFILE, base_commit=BASE, inputs=inputs(),
        source=dict(path=str(Q.SOURCE), sha256=P.digest(Q.SOURCE)),
        runtime_source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        binary=dict(path=str(binary), sha256=before), jobs=jobs(source, binary, out),
        build_command=['cargo', '+1.89.0', 'build', '--release', '--locked', '-p', 'spacewars-ai',
                       '--example', 'surface_mission_soak', '--features', 'sensor-profile'],
        default_changes=False, fresh_games=0, historical_runtime_parity=False)
    verify_plan(plan, out)
    I.write(out / 'plan.json', plan)
    print('Frozen 12 complete current-runtime games; three configurations, four selected settings.', flush=True)


def run_case(job, out, reaudit):
    result = copy.deepcopy(job); name = job['item']['name']; root = S.L.root_of(job)
    log, metadata = out / 'logs' / (name + '.log'), out / (name + '-raw.json')
    try:
        if metadata.exists():
            assert reaudit, 'existing game requires explicit evidence reuse'
            old = json.loads(metadata.read_text())
            assert old['item'] == job['item'] and old['command'] == job['command']
            assert P.digest(log) == old['log_sha256']
            if not root.exists():
                B.unpack(json.loads((out / (name + '-result.json')).read_text())['archive'], root)
            result.update(hashes=old['hashes'], log_sha256=old['log_sha256'], reused_raw=True)
        else:
            assert not root.exists() and not log.exists(), 'partial games are never retried'
            with log.open('x') as stream:
                subprocess.run(job['command'], check=True, stdout=stream, stderr=stream, timeout=1800)
            result.update(hashes=I.raw_hashes(root), log_sha256=P.digest(log))
            I.write(metadata, result)
        assert all(P.digest(root / k) == v for k, v in result['hashes'].items())
        report = json.loads((root / 'report.json').read_text())
        check_configuration(report, job['item'])
        result['stopping_rule'] = audit_switch(Q.TRACE.rows(root / 'trace.jsonl'), report, job['item'])
        result.update(Q.analyze(root, job, root))
        result['defense'] = C.A.audit(root, job['item'], report)
        result['clearance'] = C.audit(root, job['item'], report)
        result['native'] = S.native_audit(root, report, job['item'])
        if (root / 'impact.jsonl').exists():
            result['impact'] = C.A.audit_impact(root, report)
        assert all(P.digest(root / k) == v for k, v in result['hashes'].items())
        result['audited'] = True
    except Exception:
        result['error'] = traceback.format_exc()
    I.write(out / (name + '-result.json'), result)
    print(name, 'audited' if result.get('audited') else result['error'], flush=True)
    return result


def compare_group(runs):
    reports = {r['item']['name']: json.loads((S.L.root_of(r) / 'report.json').read_text()) for r in runs.values()}
    pairs = []
    for a, b in CONTRASTS:
        pair = Q.compare(runs[a], runs[b], reports)
        pair.update(before_configuration=a, after_configuration=b,
                    factor='live_claim_stopping' if a == 'integrated' else 'integration')
        pairs.append(pair)
    wanted = {r['item']['name']: set() for r in runs.values()}
    for pair in pairs:
        if (first := pair['first_control_difference']) is not None:
            for side in ('before', 'after'):
                wanted[pair[side]].add((first['tick'], first['seat']))
    evidence = {r['item']['name']: Q.witnesses(r, wanted[r['item']['name']]) for r in runs.values()}
    for pair in pairs:
        if (first := pair['first_control_difference']) is not None:
            key = f"{first['tick']}:{first['seat']}"
            a, b = [evidence[pair[side]][key]['current'] for side in ('before', 'after')]
            assert a['actions'] == first['control'] and b['actions'] == first['candidate']
            assert R.physical(a) == R.physical(b), 'physics changed before the first changed action'
            pair.update(first_change_physical_state_equal=True,
                first_change_observations_equal=a['observation'] == b['observation'],
                first_change_live_claim=[live_claim(row) for row in (a, b)],
                first_change_witnesses={side: evidence[pair[side]][key] for side in ('before', 'after')})
    return pairs


def diagnosis(runs, pairs, source):
    expected = {c['name']: c for c in cases(source)}
    assert set(runs) == set(expected) and all(r['item'] == expected[n] and r['audited'] for n, r in runs.items())
    assert len(pairs) == 12
    assert {(p['group'], p['before_configuration'], p['after_configuration']) for p in pairs} == {
        (g, a, b) for g in Q.GROUPS for a, b in CONTRASTS}
    primary = [p for p in pairs if p['factor'] == 'live_claim_stopping']
    return dict(decision='diagnosis_only', fresh_games=0, default_promotion=False,
        changed_settings=[p['group'] for p in primary if p['first_control_difference']],
        primary_outcomes={p['group']: p['outcome_transition'] for p in primary},
        historical_runtime_parity=False)


def pack(run, out, reaudit):
    # Process-pool entry points must belong to this importable main module.
    return Q.pack(run, out, reaudit)


def execute(path, reaudit):
    plan = json.loads(path.read_text()); out = path.parent
    source = verify_plan(plan, out, reaudit); target = out / 'summary.json'
    if target.exists():
        assert reaudit
        backup = out / ('summary-before-reaudit-' + P.digest(target)[:12] + '.json')
        assert not backup.exists(); shutil.copy2(target, backup)
    summary = dict(schema=1, profile=PROFILE, complete=False, plan_sha256=P.digest(path), inputs=inputs(),
        base_commit=BASE, runtime_source_commit=plan['runtime_source_commit'], binary=plan['binary'],
        reaudit=reaudit, runs={}, comparisons=[])
    I.write(target, summary)
    try:
        with ProcessPoolExecutor(max_workers=2) as pool:
            for group in Q.GROUPS:
                selected = [j for j in plan['jobs'] if j['item']['group'] == group]
                for batch in (selected[:2], selected[2:]):
                    futures = [pool.submit(run_case, j, out, reaudit) for j in batch]
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
                futures = [pool.submit(pack, r, out, reaudit) for r in runs.values()]
                for future in futures:
                    run = future.result(); summary['runs'][run['item']['name']] = run; I.write(target, summary)
                print(group, 'compared and archived;', len(summary['runs']), '/ 12 games', flush=True)
        verify_plan(plan, out, reaudit)
        summary['diagnosis'] = diagnosis(summary['runs'], summary['comparisons'], source)
        summary['complete'] = True
    except BaseException:
        summary['error'] = traceback.format_exc()
        raise
    finally:
        I.write(target, summary)
    print('Complete: diagnosis only; no promotion.', flush=True)


if __name__ == '__main__':
    assert __debug__, 'auditors require assertions enabled'
    parser = argparse.ArgumentParser(description=__doc__); sub = parser.add_subparsers(dest='action', required=True)
    p = sub.add_parser('plan'); p.add_argument('--out', type=Path, required=True); p.add_argument('--binary', type=Path, required=True)
    p = sub.add_parser('run'); p.add_argument('--plan', type=Path, required=True); p.add_argument('--reaudit', action='store_true')
    args = parser.parse_args()
    if args.action == 'plan': freeze(args.out.resolve(), args.binary.resolve(strict=True))
    else: execute(args.plan.resolve(), args.reaudit)
