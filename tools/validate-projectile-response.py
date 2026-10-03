#!/usr/bin/env python3
"""Frozen one-pulse braking/steering continuations of retained transfer cases."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('diagnostics', Path(__file__).with_name('validate-projectile-diagnostics.py'))
P = importlib.util.module_from_spec(spec)
spec.loader.exec_module(P)
S, digest, rows, root_of = P.S, P.digest, P.rows, P.root_of


def command(old, binary, root, mode):
    cmd = list(old['command'])
    assert '--probe-projectile-response' not in cmd and '--trace-projectiles' in cmd
    cmd[0] = str(binary)
    cmd[cmd.index('--out') + 1] = str(root)
    return cmd + ([] if mode == 'none' else ['--probe-projectile-response', mode])


def override(actions, mode):
    result = copy.deepcopy(actions)
    if mode == 'observe':
        return result
    control = result[0]['Scenario']['payload']
    assert len(control) == 8 and control[5] == 0
    if mode == 'brake':
        control[4], control[6] = 0, 1
    else:
        assert mode in ('left', 'right')
        control[:4] = list(struct.pack('<f', -1. if mode == 'left' else 1.))
        control[4] = 1 - control[6]
    result[1]['Scenario']['payload'][1] = 0
    return result


def ready(row, evidence):
    witness = evidence.get('escape_travel')
    if witness is None:
        return None
    o = witness['observation']; f = o['local']['combat']['recovery']['flight']; p = f['pilot']
    a = witness['telemetry']['last']
    if not (o['match_rules'] and not (o.get('match_context') or {}).get('finished', False)
            and row['goal'] == 'transfer' and not row['capture_active'] and not row['recovery_active']
            and a['finished_tick'] is None and a['observed_tick'] == p['tick'] < a['deadline_tick']
            and a['destination'] is not None and a['selected_tick'] is not None
            and row['target'] == a['destination'] and p['controls_armed'] and p['queries_ready']
            and f['flight']['enabled'] and p['ship_available'] and p['ship_form'] == 'ship'
            and p['vehicle'] == a['vehicle'] and isinstance(p['location'], dict)
            and p['landing']['phase'] == 'flying' and p['landing']['supported_feet'] == 0):
        return None
    return {k:a[k] for k in ['started_tick', 'deadline_tick', 'selected_tick', 'vehicle', 'destination']}


def threat(d):
    if d is None:
        return None
    possible = []
    for p in d['projectiles']:
        entry = P.linear_approach(p['relative_position'], p['relative_velocity'],
                                  d['observer_radius'] + p['collision_radius'])['entry_seconds']
        if entry is not None:
            possible.append(dict(id=p['id'], spawn_tick=p['spawn_tick'], entry_seconds=entry))
    return min(possible, key=lambda p:(p['entry_seconds'], p['id'])) if possible else None


def advance(previous, tick, key, selected, mode):
    expected = copy.deepcopy(previous)
    if expected is None and key is not None and selected is not None:
        expected = dict(source=key, started_tick=tick, end_tick=tick+30, threat=selected,
                        finished_tick=None, reason=None, applied_ticks=0)
    applied = False
    if expected is not None and expected['finished_tick'] is None:
        if tick >= expected['end_tick'] or key != expected['source']:
            expected['finished_tick'] = tick
            expected['reason'] = 'pulse complete' if tick >= expected['end_tick'] else 'native priority or transfer identity changed'
        elif mode != 'observe':
            applied = True
            expected['applied_ticks'] += 1
    return expected, applied


def source_ship_missing(pilot, vehicle):
    return not pilot['ship_available'] or pilot['ship_form'] != 'ship' or pilot['vehicle'] != vehicle


def audit_response(root, old_root, mode, seat, report):
    evidence = (r for r in rows(root/'capture-evidence.jsonl') if r['seat'] == seat)
    old = iter(rows(old_root/'capture-evidence.jsonl'))
    # Require both seats' complete consumed observations before intervention.
    probe = iter(rows(root/'projectile-response.jsonl'))
    diagnostic = (r for r in rows(root/'projectiles.jsonl') if r['seat'] == seat)
    previous = None; witnesses = []; first_change = None; changes = []; loss = None
    last_damage = None; prefix_rows = 0
    all_evidence = iter(rows(root/'capture-evidence.jsonl'))
    for tick in range(report['elapsed_ticks']):
        row = next(probe); e = next(evidence); d = next(diagnostic)
        assert row['tick'] == e['pilot']['tick'] == d['tick'] == tick and row['seat'] == seat
        assert row['mode'] == mode and row['actions'] == e['actions']
        assert row['goal'] == e['mission']['goal'] and row['target'] == e['mission']['target']
        assert row['capture_active'] == (e['capture'] is not None)
        key = ready(row, e)
        assert row['ready'] == key
        should_sample = key is not None and previous is None
        assert (row['diagnostic'] is not None) == should_sample
        if should_sample:
            assert row['diagnostic'] == d['diagnostic']
            assert row['observation'] == e['escape_travel']['observation']
        selected = threat(row['diagnostic'])
        expected, applied = advance(previous, tick, key, selected, mode)
        actual = row['attempt']
        if expected is not None:
            assert abs(actual['threat']['entry_seconds']-expected['threat']['entry_seconds']) < 1.e-7
            expected['threat']['entry_seconds'] = actual['threat']['entry_seconds']
        assert actual == expected and row['applied'] == applied
        assert row['actions'] == (override(row['original_actions'], mode) if applied else row['original_actions'])
        if row['actions'] != row['original_actions'] and first_change is None:
            first_change = tick
        if applied or (actual is not None and tick in [actual['started_tick'], actual['finished_tick']]):
            witnesses.append(dict(probe=row, evidence=e))
        if actual is not None:
            if row['damage'] != last_damage:
                changes.append(dict(tick=tick, damage=row['damage'], pilot=e['pilot'], mission=e['mission']))
            if loss is None and source_ship_missing(e['pilot'], actual['source']['vehicle']):
                loss = dict(tick=tick, damage=row['damage'], pilot=e['pilot'])
        last_damage = row['damage']
        for _ in range(2):
            new_row = next(all_evidence)
            if actual is None or tick <= actual['started_tick']:
                old_row = next(old)
                current = copy.deepcopy(new_row)
                if current['seat'] == seat:
                    current['actions'] = row['original_actions']
                assert current == old_row, ('prefix differs', tick, current['seat'])
                prefix_rows += 1
        previous = actual
    final = next(probe)
    assert final['final_tick'] == report['elapsed_ticks'] and next(probe, None) is None
    assert next(evidence, None) is None and next(diagnostic, None) is None
    expected_final = copy.deepcopy(previous)
    if expected_final is not None and expected_final['finished_tick'] is None:
        expected_final.update(finished_tick=report['elapsed_ticks'], reason='match ended')
    assert final['attempt'] == expected_final
    if loss is None and expected_final is not None and source_ship_missing(report['final_pilots'][seat], expected_final['source']['vehicle']):
        loss = dict(tick=report['elapsed_ticks'], damage=final['damage'], pilot=report['final_pilots'][seat])
    path = root/'projectile-response-witnesses.json'
    S.F.D.write(path, dict(schema=1, rows=witnesses, damage_changes=changes, first_ship_loss=loss, final=final))
    return dict(attempt=final['attempt'], first_action_change=first_change, exact_prefix_rows=prefix_rows,
                damage_changes=changes, first_ship_loss=loss, witness_sha256=digest(path))


def run(entry, old, binary, out):
    root = out/entry['key']; cmd = command(old, binary, root, entry['mode'])
    result = dict(item=old['item'], command=cmd)
    try:
        with (out/(entry['key']+'.log')).open('x') as log:
            subprocess.run(cmd, check=True, stdout=log, stderr=log, timeout=1800)
        result.update(S.C.analyze(root, old['item']))
        report = json.loads((root/'report.json').read_text())
        result['projectiles'] = P.audit_stream(root, report)
        if entry['mode'] == 'none':
            assert not (root/'projectile-response.jsonl').exists()
        else:
            result['response'] = audit_response(root, root_of(old), entry['mode'], old['item']['seat'], report)
        if entry['mode'] in ('none','observe') or result['response']['first_action_change'] is None:
            result['parity'] = S.C.P.replay_parity(old,result)
            assert digest(root/'projectiles.jsonl') == old['hashes']['projectiles.jsonl']
        result['hashes'] = {p.name:digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    except Exception:
        result['error'] = traceback.format_exc()
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior',type=Path,required=True);parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True);args=parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code, tests and plan first'
    prior=json.loads(args.prior.read_text());assert prior['complete']
    sources=[e for e in prior['plan'] if e['enabled']]
    assert len(sources)==4
    plan=[dict(key=f"{e['key']}-{mode}",source=e['key'],mode=mode) for mode in ['observe','brake','left','right'] for e in sources]
    plan.append(dict(key='speed-shared-armed-world1-p1-powered-disabled',source='speed-shared-armed-world1-p1-powered',mode='none'))
    for e in sources:
        old=prior['runs'][e['key']]
        for name,h in old['hashes'].items():assert digest(root_of(old)/name)==h
    binary=args.binary.resolve(strict=True);args.out.mkdir(parents=True,exist_ok=False)
    result=dict(schema=1,complete=False,plan=plan,runs={},source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
                prior=dict(path=str(args.prior),sha256=digest(args.prior)),binary=dict(path=str(binary),sha256=digest(binary)),runner_sha256=digest(__file__))
    save=lambda:S.F.D.write(args.out/'summary.json',result);save()
    try:
        with ThreadPoolExecutor(max_workers=2) as pool:
            futures=[pool.submit(run,e,prior['runs'][e['source']],binary,args.out) for e in plan]
            for e,future in zip(plan,futures):
                r=future.result();result['runs'][e['key']]=r;save()
                assert 'error' not in r,(e['key'],r.get('error'))
                print(e['key']+': audited',flush=True)
        assert digest(binary)==result['binary']['sha256'] and digest(args.prior)==result['prior']['sha256']
        result['complete']=True
    except BaseException as error:result['error']=repr(error);raise
    finally:save()


if __name__=='__main__':main()
