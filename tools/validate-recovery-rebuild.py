#!/usr/bin/env python3
"""Qualify shared recovery lifecycle progress on the two recorded native games."""
import argparse
from concurrent.futures import ProcessPoolExecutor
import copy
import importlib.util
from itertools import zip_longest
import json
from pathlib import Path
import shutil
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('clearance', Path(__file__).with_name('validate-acquisition-clearance.py'))
C = importlib.util.module_from_spec(spec)
spec.loader.exec_module(C)
B, I, P, D, ROOT = C.B, C.I, C.P, C.D, C.ROOT
PROFILE = 'recovery_native_rebuild_v1'
PRIOR = ROOT / 'target/acquisition-clearance/v1/summary.json'
CASES = ('known-lost-win-off', 'known-laser-off-win-off')
REBUILD_TICK = 21948
OWN_INPUTS = ('tools/validate-recovery-rebuild.py', 'tools/tests/test_recovery_rebuild.py',
              'docs/recovery-rebuild-plan.md', 'crates/spacewars-ai/src/recovery_task.rs',
              'crates/spacewars-ai/tests/surface_recovery.rs',
              'crates/spacewars-ai/src/recovery_pilot.rs',
              'scenarios/spacewars/src/surface_sortie/recovery.rs')


def inputs():
    return dict(C.inputs(), **{p: P.digest(ROOT / p) for p in OWN_INPUTS})


def jobs(sources, binary, out):
    result = []
    for name in CASES:
        prior = sources[name]
        cmd = list(prior['command'])
        assert len(cmd) % 2 == 1 and len(cmd[1::2]) == len(set(cmd[1::2]))
        assert prior['item']['seed'] == 11223442104665788832
        assert not prior['item']['defense'] and not prior['item']['clearance']
        cmd[0] = str(binary)
        cmd[cmd.index('--out') + 1] = str(out / 'raw' / name)
        result.append(dict(item=prior['item'], command=cmd))
    return result


def normalize_version(value):
    if isinstance(value, dict):
        return {k: ('recover_ship_v9' if k == 'task' and v == 'recover_ship_v10'
                    else normalize_version(v)) for k, v in value.items()}
    if isinstance(value, list):
        return [normalize_version(v) for v in value]
    return value


def pilot(row):
    return row['observation']['local']['combat']['recovery']['flight']['pilot']


def physical(row):
    p = pilot(row)
    return dict(pilot={k: p[k] for k in ('ship', 'actor', 'ship_form', 'ship_available',
                                       'ship_health', 'location', 'recovery')},
                health=row['observation']['match_context']['pilot_health'])


def point(row):
    p, m = pilot(row), row['mission']
    return dict(tick=row['tick'], seat=row['seat'], actions=row['actions'],
                pilot={k: p[k] for k in ('owner', 'vehicle', 'location', 'ship_form', 'ship_available',
                                        'controls_armed', 'queries_ready', 'recovery')},
                recovery=m['recovery'], completed_recoveries=m['completed_recoveries'])


def check_receipt(row, previous):
    p, m = pilot(row), row['mission']
    t, old = m['recovery'], previous['mission']['recovery']
    r = t['rebuild_boarding']
    assert r['started_tick'] == row['tick'] == p['tick']
    assert r['deadline_tick'] == row['tick'] + 90 * 60
    assert p['ship_available'] and p['ship_form'] == 'ship'
    assert r['native_rebuilds'] == p['recovery']['rebuilds'] > pilot(previous)['recovery']['rebuilds']
    assert p['vehicle'] == pilot(previous)['vehicle'] and p['owner'] == pilot(previous)['owner']
    assert old is not None and old.get('rebuild_boarding') is None
    assert old['status'] == 'blocked' or row['tick'] - old['started_tick'] > 120 * 60 + old['ground_budget_ticks']
    assert (r['previous_goal'], r['previous_reason']) == (old['goal'], old['reason'])
    for key in ('started_tick', 'landed_tick', 'exited_tick', 'relocations', 'relocation_surveys',
                'ground_budget_ticks', 'scuttle_attempts', 'scuttle_started_tick', 'scuttled_tick'):
        assert t[key] == old[key], f'rebuild reset {key}'
    assert t['site'] is None and t['relocation_site'] is None
    assert t['goal'] == 'board' and t['status'] == 'running'
    if not p['controls_armed']:
        assert t['ground'] is None
    return dict(seat=row['seat'], task_started_tick=t['started_tick'], receipt=r,
                ships_lost=p['recovery']['ships_lost'], scuttle_attempts=t['scuttle_attempts'],
                witness=point(row), completed_tick=None, blocked_tick=None)


def audit_recovery(root, oldroot, elapsed_ticks):
    previous, attempts, first = {}, {}, {}
    rows = prefix_rows = 0
    anchor = baseline_release = None
    for old, new in zip_longest(D.rows(oldroot / 'trace.jsonl'), D.rows(root / 'trace.jsonl')):
        if old and old['seat'] == 1 and old['tick'] > REBUILD_TICK and baseline_release is None:
            if old['mission']['recovery']['status'] != 'blocked':
                baseline_release = point(old)
        if new is None:
            continue
        tick, seat = new['tick'], new['seat']
        assert (tick, seat) == (rows // 2, rows % 2)
        rows += 1
        p, m = pilot(new), new['mission']
        assert p['tick'] == tick and p['owner'] == f'player_{seat+1}'
        if tick < REBUILD_TICK:
            assert old == normalize_version(new), f'pre-rebuild trace changed at {tick}/{seat}'
            prefix_rows += 1
        if old:
            assert (old['tick'], old['seat']) == (tick, seat)
            for key, a, b in (('actions', old['actions'], new['actions']),
                              ('physical', physical(old), physical(new))):
                if a != b and key not in first:
                    first[key] = dict(tick=tick, seat=seat, before=a, after=b)
        if tick == REBUILD_TICK and seat == 1:
            assert old['observation'] == new['observation'] and old['actions'] == new['actions']
            assert old['mission']['recovery']['status'] == 'blocked'
            assert old['mission']['recovery']['relocations'] == 4
            anchor = dict(before=point(old), after=point(new))
        task = m['recovery']
        receipt = (task or {}).get('rebuild_boarding')
        if receipt:
            key = (seat, task['started_tick'])
            if key not in attempts:
                attempts[key] = check_receipt(new, previous[seat])
            a = attempts[key]
            assert receipt == a['receipt'], 'boarding opportunity restarted'
            assert task['scuttle_attempts'] == a['scuttle_attempts'], 'boarding started another replacement'
            if task['status'] == 'running':
                assert tick <= receipt['deadline_tick']
                assert p['ship_available'] and p['ship_form'] == 'ship'
            elif task['status'] == 'blocked' and a['blocked_tick'] is None:
                a.update(blocked_tick=tick, blocked_reason=task['reason'])
        before = previous.get(seat)
        if before and m['completed_recoveries'] > before['mission']['completed_recoveries']:
            oldtask = before['mission']['recovery']
            if oldtask and oldtask.get('rebuild_boarding'):
                a = attempts[(seat, oldtask['started_tick'])]
                assert p['location'] == {'aboard': p['vehicle']} and p['ship_form'] == 'ship' and p['ship_available']
                assert tick <= a['receipt']['deadline_tick'] + 1
                assert p['recovery']['ships_lost'] == a['ships_lost']
                a.update(completed_tick=tick, completion=point(new))
        previous[seat] = new
    assert rows == elapsed_ticks * 2 and prefix_rows == REBUILD_TICK * 2
    assert anchor and baseline_release
    diagnosed = attempts[(1, 3220)]
    assert diagnosed['receipt']['started_tick'] == REBUILD_TICK
    assert diagnosed['receipt']['native_rebuilds'] == 1 and diagnosed['completed_tick'] is not None
    assert first['actions']['tick'] > REBUILD_TICK and first['physical']['tick'] > first['actions']['tick']
    result = dict(rows=rows, prefix_rows=prefix_rows, anchor=anchor, first_differences=first,
                  attempts=list(attempts.values()), baseline_release=baseline_release, qualified=True)
    I.write(root / 'rebuild-audit.json', result)
    return result


def freeze(out):
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()
    prior = json.loads(PRIOR.read_text())
    assert prior['complete'] and prior['qualification']['passed']
    sources = {name: prior['runs'][name] for name in CASES}
    out.mkdir(parents=True, exist_ok=False)
    for name in ('raw', 'logs', 'archives', 'inputs'):
        (out / name).mkdir()
    binary = out / 'surface_mission_soak'
    shutil.copy2(ROOT / 'target/release/examples/surface_mission_soak', binary)
    binary.chmod(0o555)
    plan = dict(schema=1, profile=PROFILE, source_commit=subprocess.check_output(
        ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        binary=dict(path=str(binary), sha256=P.digest(binary)), inputs=inputs(),
        prior_summary=dict(path=str(PRIOR), sha256=P.digest(PRIOR)), sources=sources,
        jobs=jobs(sources, binary, out), shared_recovery_fix=True, strategy_default_changes=False)
    I.write(out / 'plan.json', plan)
    print('Frozen two native games; shared recovery fix in both actors.', flush=True)


def run_case(job, plan, out, reaudit):
    name = job['item']['name']
    root, oldroot = out / 'raw' / name, out / 'inputs' / name
    log, metadata = out / 'logs' / (name + '.log'), out / (name + '-raw.json')
    result = copy.deepcopy(job)
    try:
        if reaudit:
            raw = json.loads(metadata.read_text())
            assert all(raw[k] == job[k] for k in ('item', 'command'))
            assert P.digest(log) == raw['log_sha256']
            if not root.exists():
                prior = json.loads((out / (name + '-result.json')).read_text())
                B.unpack(prior['archive'], root)
            result.update(hashes=raw['hashes'], log_sha256=raw['log_sha256'])
        else:
            assert not root.exists() and not metadata.exists()
            with log.open('x') as stream:
                subprocess.run(job['command'], stdout=stream, stderr=stream, check=True, timeout=1800)
            result.update(hashes=I.raw_hashes(root), log_sha256=P.digest(log))
            I.write(metadata, result)
        assert all(P.digest(root / k) == v for k, v in result['hashes'].items())
        result.update(I.analyze(root, job, root))
        report = json.loads((root / 'report.json').read_text())
        result['defense'] = C.A.audit(root, job['item'], report)
        result['clearance'] = C.audit(root, job['item'], report)
        source = plan['sources'][name]['archive']
        if not oldroot.exists():
            B.unpack(source, oldroot)
        assert I.raw_hashes(oldroot) == {k: v['sha256'] for k, v in source['files'].items()}
        result['recovery'] = audit_recovery(root, oldroot, report['elapsed_ticks'])
        assert all(P.digest(root / k) == v for k, v in result['hashes'].items())
        suffix = '-' + P.digest(__file__)[:12] if reaudit else ''
        result['archive'] = B.pack(root, out / 'archives' / (name + suffix + '.tar.gz'))
        # This extraction is disposable only after the original archive and
        # all its extracted member hashes have been verified above.
        shutil.rmtree(oldroot)
        result['audited'] = True
    except Exception:
        result['error'] = traceback.format_exc()
    I.write(out / (name + '-result.json'), result)
    print(name, 'passed' if result.get('audited') else result['error'], flush=True)
    return result


def execute(path, reaudit):
    plan = json.loads(path.read_text())
    out = path.parent
    assert plan['profile'] == PROFILE and P.digest(plan['binary']['path']) == plan['binary']['sha256']
    assert P.digest(PRIOR) == plan['prior_summary']['sha256']
    changed = {k for k, v in inputs().items() if plan['inputs'].get(k) != v}
    assert not changed or reaudit and changed <= set(OWN_INPUTS[:2]), 'frozen runtime/plan changed'
    assert plan['jobs'] == jobs(plan['sources'], Path(plan['binary']['path']), out)
    summary = dict(profile=PROFILE, plan_sha256=P.digest(path), source_commit=plan['source_commit'],
                   binary=plan['binary'], inputs=inputs(), reaudit=reaudit, runs={}, complete=False)
    summary_path = out / 'summary.json'
    if summary_path.exists():
        assert reaudit
        backup = out / ('summary-before-reaudit-' + P.digest(summary_path)[:12] + '.json')
        assert not backup.exists()
        shutil.copy2(summary_path, backup)
    with ProcessPoolExecutor(max_workers=2) as pool:
        futures = [(job['item']['name'], pool.submit(run_case, job, plan, out, reaudit)) for job in plan['jobs']]
        for name, future in futures:
            summary['runs'][name] = future.result()
            I.write(summary_path, summary)
    summary['complete'] = all(r.get('audited', False) for r in summary['runs'].values())
    I.write(summary_path, summary)
    assert summary['complete'], 'qualification failed; keep raw evidence for investigation'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    p = sub.add_parser('plan'); p.add_argument('--out', type=Path, required=True)
    p = sub.add_parser('run'); p.add_argument('--plan', type=Path, required=True)
    p.add_argument('--reaudit', action='store_true')
    args = parser.parse_args()
    if args.action == 'plan':
        freeze(args.out.resolve())
    else:
        execute(args.plan.resolve(), args.reaudit)
