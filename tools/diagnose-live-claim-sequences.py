#!/usr/bin/env python3
"""Replay four known live-claim cases with strictly observational impact traces."""
import argparse
import copy
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import tarfile
import traceback

spec = importlib.util.spec_from_file_location('evaluation', Path(__file__).with_name('compare-live-claim.py'))
E = importlib.util.module_from_spec(spec); spec.loader.exec_module(E)
X, Q, S, C, B, I, P, ROOT = E.X, E.Q, E.S, E.C, E.B, E.I, E.P, E.ROOT
D = C.A.D
PROFILE = 'live_claim_sequences_v1'
SOURCE = ROOT / 'target/live-claim-evaluation/v1/summary.json'
GROUPS = ('new-world1-p2-asteroids3', 'new-world3-p1-asteroids3')
NAMES = tuple(g + '-' + c for g in GROUPS for c in ('integrated', 'no-stop'))
OWN = ('tools/diagnose-live-claim-sequences.py', 'tools/tests/test_live_claim_sequences.py',
       'docs/live-claim-sequences-plan.md')
OBSERVER = {'--trace-impact': 'true', '--impact-start-tick': '0', '--impact-end-tick': '36001',
            '--impact-pod-control': 'bot', '--impact-control-from-tick': '0'}


def inputs():
    return dict(E.inputs(), **{p: P.digest(ROOT / p) for p in OWN})


def jobs(source, binary, out):
    result = []
    for name in NAMES:
        prior = source['runs'][name]
        cmd = list(prior['command'])
        assert len(cmd) % 2 == 1 and len(cmd[1::2]) == len(set(cmd[1::2]))
        assert not set(OBSERVER).intersection(cmd[1::2])
        cmd[0] = str(binary); cmd[cmd.index('--out') + 1] = str(out / 'raw' / name)
        cmd.extend(v for pair in OBSERVER.items() for v in pair)
        result.append(dict(item=dict(prior['item'], stage='selected_sequence_diagnosis'), command=cmd))
    return result


def verify(plan, out, reaudit=False):
    assert plan['profile'] == PROFILE
    current = inputs(); assert current.keys() == plan['inputs'].keys()
    changed = {p for p in current if current[p] != plan['inputs'][p]}
    assert not changed or reaudit and changed <= set(OWN[:2])
    assert P.digest(plan['source']['path']) == plan['source']['sha256']
    source = json.loads(Path(plan['source']['path']).read_text())
    assert source['complete'] and source['profile'] == E.PROFILE and source['inputs'] == E.inputs()
    assert plan['binary']['sha256'] == P.digest(plan['binary']['path']) == source['binary']['sha256']
    assert plan['runtime_source_commit'] == source['runtime_source_commit']
    assert plan['jobs'] == jobs(source, Path(plan['binary']['path']), out)
    assert plan['sources'] == {n: source['runs'][n] for n in NAMES}
    return source


def freeze(out):
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()
    source = json.loads(SOURCE.read_text())
    assert source['complete'] and source['inputs'] == E.inputs()
    out.mkdir(parents=True, exist_ok=False)
    for name in ('raw', 'logs', 'archives', 'inputs'):
        (out / name).mkdir()
    binary = out / 'surface_mission_soak'
    shutil.copy2(source['binary']['path'], binary); binary.chmod(0o555)
    plan = dict(profile=PROFILE, schema=1, inputs=inputs(),
        source=dict(path=str(SOURCE), sha256=P.digest(SOURCE)),
        runtime_source_commit=source['runtime_source_commit'],
        runner_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        binary=dict(path=str(binary), sha256=P.digest(binary)),
        sources={n: source['runs'][n] for n in NAMES}, jobs=jobs(source, binary, out),
        fresh_games=0, default_promotion=False)
    verify(plan, out); I.write(out / 'plan.json', plan)
    print('Frozen four observational replays; no runtime or control changes.', flush=True)


def extract(record, root, names):
    """Extract only named regular members, bound by both archive and file hashes."""
    assert P.digest(record['path']) == record['sha256']
    root.mkdir(exist_ok=True)
    wanted = set(names); assert wanted <= record['files'].keys()
    found = set()
    with tarfile.open(record['path'], 'r|gz') as archive:
        for member in archive:
            if member.name not in wanted:
                continue
            assert member.isfile() and Path(member.name).name == member.name and member.name not in found
            path = root / member.name
            with archive.extractfile(member) as source, path.open('wb') as target:
                shutil.copyfileobj(source, target)
            assert P.digest(path) == record['files'][member.name]['sha256']
            assert path.stat().st_size == record['files'][member.name]['bytes']
            found.add(member.name)
            if found == wanted:
                break
    assert found == wanted


def original_hashes(hashes, prior):
    assert set(hashes) == set(prior) | {'impact.jsonl'} and 'impact.jsonl' not in prior
    assert all(hashes[k] == v for k, v in prior.items()
               if k not in ('report.json', 'sensors.jsonl', 'live-planning.csv'))
    return {k: v for k, v in hashes.items() if k != 'impact.jsonl'}


def observer_audit(impact_rows, trace_rows, report, observer_seat):
    traces = iter(trace_rows); count = 0; final = None
    initial, previous, losses = {}, {}, []
    for row in impact_rows:
        assert final is None, 'rows after native final record'
        assert row['schema'] == 1
        if 'final_tick' in row:
            final = row; continue
        trace = next(traces, None); assert trace is not None
        tick, seat = count // 2, count % 2
        assert (row['tick'], row['seat']) == (trace['tick'], trace['seat']) == (tick, seat)
        assert not row['overridden'] and row['controls'] == row['bot_controls']
        assert row['actions'] == trace['actions'] and row['goal'] == trace['mission']['goal']
        assert {k: row['controls'][k] for k in trace['controls']} == trace['controls']
        p = X.R.pilot(trace)
        assert p['tick'] == tick and p['owner'] == f'player_{seat+1}'
        assert (row['form'], row['ship'], row['location']) == (p['ship_form'], p['ship'], p['location'])
        assert row['vitals']['health'] == trace['observation']['match_context']['pilot_health'][seat]
        assert row['recovery'] == trace['mission']['recovery']
        assert row['flight'] == trace['observation']['local']['combat']['recovery']['flight']['flight']
        assert row['planet'] == dict(index=p['planet']['index'], motion=p['planet']['motion'])
        assert row['gravity'] == p['gravity']
        lost = (p.get('recovery') or {}).get('ships_lost', 0)
        initial.setdefault(seat, lost); old = previous.get(seat, lost)
        assert lost in (old, old + 1)
        if lost > old: losses.append(dict(seat=seat, receipt=D.loss_receipt(row, tick)))
        previous[seat] = lost; count += 1
    ticks = report['elapsed_ticks']
    assert count == ticks * 2 and next(traces, None) is None
    assert final is not None and final['final_tick'] == ticks and final['round'] == report['round']
    assert final['config'] == dict(seat=observer_seat, control='bot', control_from_tick=0,
                                 trace_start_tick=0, trace_end_tick=36001)
    for seat in (0, 1):
        lost = (report['final_pilots'][seat].get('recovery') or {}).get('ships_lost', 0)
        assert lost in (previous[seat], previous[seat] + 1)
        if lost > previous[seat]:
            losses.append(dict(seat=seat, receipt=D.loss_receipt(
                dict(tick=ticks, damage=final['damage'][seat]), ticks)))
        assert initial[seat] + sum(v['seat'] == seat for v in losses) == lost
    return dict(rows=count, overrides=0, initial_losses=initial, losses=losses, final=final)


def point(trace, impact):
    p, m = X.R.pilot(trace), trace['mission']
    cap, recovery, combat = m.get('capture') or {}, m.get('recovery') or {}, m.get('combat') or {}
    target = trace['observation']['local']['combat']['target']
    return dict(tick=trace['tick'], seat=trace['seat'], goal=m['goal'], target=m['target'],
        reason=m['reason'], capture_goal=cap.get('goal'), recovery_goal=recovery.get('goal'),
        recovery_status=recovery.get('status'), recovery_reason=recovery.get('reason'),
        combat_goal=combat.get('goal'), pursuit=m.get('pursuit'),
        ship_health=p['ship_health'], pilot_health=impact['vitals']['health'],
        form=p['ship_form'], location=p['location'], ships_lost=(p.get('recovery') or {}).get('ships_lost', 0),
        completed_departures=m['completed_sorties'], completed_recoveries=m['completed_recoveries'],
        controls=impact['controls'], weapons=D.T.weapon_action(trace), damage=impact['damage'],
        target_state={k: target[k] for k in ('health', 'ship_form', 'visible', 'ground_occluded')} if target else None,
        motion=impact['motion'])


PHASE = ('goal', 'target', 'reason', 'capture_goal', 'recovery_goal', 'recovery_status',
         'recovery_reason', 'combat_goal', 'form')


def sequence(root, report, evaluated_seat):
    phases, changes, pod_motion, motion_changes = {0: [], 1: []}, [], {0: [], 1: []}, []
    previous = {}; traces = iter(D.rows(root / 'trace.jsonl'))
    for row in D.rows(root / 'impact.jsonl'):
        if 'final_tick' in row: break
        trace = next(traces); seat = row['seat']; p = point(trace, row); old = previous.get(seat)
        signature = {k: p[k] for k in PHASE}
        if not phases[seat] or signature != phases[seat][-1]['state']:
            phases[seat].append(dict(start=p['tick'], end=p['tick'], state=signature, first=p, last=p,
                ticks=0, brake_ticks=0, thrust_ticks=0, laser_requests=0, cannon_requests=0))
        phase = phases[seat][-1]; assert phase['end'] == p['tick']
        phase.update(end=p['tick']+1, last=p, ticks=phase['ticks']+1,
            brake_ticks=phase['brake_ticks']+int(p['controls']['brake']),
            thrust_ticks=phase['thrust_ticks']+int(p['controls']['thrust']),
            laser_requests=phase['laser_requests']+int(p['weapons']['laser']),
            cannon_requests=phase['cannon_requests']+int(p['weapons']['cannon']))
        if old is None or any(p[k] != old[k] for k in ('damage', 'pilot_health', 'ships_lost')):
            changes.append(dict(before=old, after=p, impact=row))
        if row['form'] == 'escape_pod' and isinstance(row['location'], dict) and 'aboard' in row['location']:
            pod_motion[seat].append(dict(tick=p['tick'], metrics=D.B.motion_metrics(row),
                controls=p['controls'], health=p['pilot_health'], recovery=row['recovery']))
        if old and old['motion'] and p['motion']:
            a, b = old['motion'], p['motion']
            delta = math.hypot(*(b['velocity'][k]-a['velocity'][k] for k in ('x', 'y')))
            spin = abs(b['spin']-a['spin'])
            if delta >= 10 or spin >= 10:
                motion_changes.append(dict(seat=seat, tick=p['tick'], velocity_change=delta,
                    spin_change=spin, before=old, after=p))
        previous[seat] = p
    assert next(traces, None) is None
    result = dict(evaluated_seat=evaluated_seat, phases=phases, damage_changes=changes,
        pod_motion=pod_motion, large_motion_changes=motion_changes,
        pilot_damage_events=report['pilot_damage_events'], final_pilots=report['final_pilots'],
        final_missions=report['missions'], round=report['round'])
    path = root / 'sequence-diagnostics.json'; I.write(path, result)
    return dict(path=str(path), sha256=P.digest(path), evaluated_seat=evaluated_seat,
        phase_counts={s: len(v) for s, v in phases.items()},
        pod_rows={s: len(v) for s, v in pod_motion.items()}, large_motion_changes=len(motion_changes))


def run_case(job, plan, out, reaudit):
    name = job['item']['name']; prior = plan['sources'][name]
    root, oldroot = out / 'raw' / name, out / 'inputs' / name
    log, metadata = out / 'logs' / (name + '.log'), out / (name + '-raw.json')
    result = copy.deepcopy(job)
    if metadata.exists():
        assert reaudit, 'existing games require explicit evidence reuse'
        raw = json.loads(metadata.read_text())
        assert raw['item'] == job['item'] and raw['command'] == job['command']
        assert P.digest(log) == raw['log_sha256']
        if not root.exists(): B.unpack(json.loads((out / (name + '-result.json')).read_text())['archive'], root)
        result.update(hashes=raw['hashes'], log_sha256=raw['log_sha256'], reused_raw=True)
    else:
        assert not root.exists() and not log.exists(), 'partial games are never retried'
        with log.open('x') as stream:
            subprocess.run(job['command'], check=True, stdout=stream, stderr=stream, timeout=1800)
        result.update(hashes=I.raw_hashes(root), log_sha256=P.digest(log)); I.write(metadata, result)
    assert all(P.digest(root / k) == v for k, v in result['hashes'].items())
    report = json.loads((root / 'report.json').read_text())
    X.check_configuration(report, job['item'])
    result['stopping_rule'] = X.audit_switch(Q.TRACE.rows(root / 'trace.jsonl'), report, job['item'])
    result.update(Q.analyze(root, job, root))
    result['defense'] = C.A.audit(root, job['item'], report)
    result['clearance'] = C.audit(root, job['item'], report)
    result['native'] = S.native_audit(root, report, job['item'])
    extract(prior['archive'], oldroot, ('report.json', 'sensors.jsonl', 'live-planning.csv'))
    projected = dict(result, hashes=original_hashes(result['hashes'], prior['hashes']))
    result['parity'] = C.parity(prior, projected, oldroot)
    assert result['stopping_rule'] == prior['stopping_rule']
    assert json.loads(json.dumps(result['native'])) == prior['native']
    observer_seat = int(job['command'][job['command'].index('--seat')+1])
    result['observer'] = observer_audit(D.rows(root/'impact.jsonl'), D.rows(root/'trace.jsonl'), report, observer_seat)
    result['sequence'] = sequence(root, report, job['item']['seat'])
    assert all(P.digest(root / k) == v for k, v in result['hashes'].items())
    result['audited'] = True
    suffix = '-' + P.digest(__file__)[:12] if reaudit else ''
    result['archive'] = B.pack(root, out / 'archives' / (name + suffix + '.tar.gz'))
    # Only our verified three-file extraction is disposable; original archives remain.
    shutil.rmtree(oldroot)
    I.write(out / (name + '-result.json'), result)
    print(name, 'original evidence matched; observer audited and archived', flush=True)
    return result


def execute(path, reaudit):
    plan = json.loads(path.read_text()); out = path.parent; verify(plan, out, reaudit)
    target = out / 'summary.json'
    if target.exists():
        assert reaudit
        backup = out / ('summary-before-reaudit-' + P.digest(target)[:12] + '.json')
        assert not backup.exists(); shutil.copy2(target, backup)
    summary = dict(schema=1, profile=PROFILE, complete=False, plan_sha256=P.digest(path), inputs=inputs(),
        binary=plan['binary'], runtime_source_commit=plan['runtime_source_commit'], runner_commit=plan['runner_commit'],
        reaudit=reaudit, fresh_games=0, default_promotion=False, runs={})
    I.write(target, summary)
    try:
        for job in plan['jobs']:
            summary['runs'][job['item']['name']] = run_case(job, plan, out, reaudit)
            I.write(target, summary)
        verify(plan, out, reaudit)
        assert tuple(summary['runs']) == NAMES and all(r['audited'] for r in summary['runs'].values())
        summary.update(complete=True, decision='diagnosis_only')
    except BaseException:
        summary['error'] = traceback.format_exc(); raise
    finally:
        I.write(target, summary)
    print('Complete: four exact observational replays; diagnosis only.', flush=True)


if __name__ == '__main__':
    assert __debug__, 'auditors require assertions enabled'
    parser = argparse.ArgumentParser(description=__doc__); sub = parser.add_subparsers(dest='action', required=True)
    p = sub.add_parser('plan'); p.add_argument('--out', type=Path, required=True)
    p = sub.add_parser('run'); p.add_argument('--plan', type=Path, required=True); p.add_argument('--reaudit', action='store_true')
    args = parser.parse_args()
    if args.action == 'plan': freeze(args.out.resolve())
    else: execute(args.plan.resolve(), args.reaudit)
