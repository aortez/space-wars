#!/usr/bin/env python3
"""Freeze, run and audit one observational replay with bounded physical forks."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('arrival', Path(__file__).with_name('validate-crossing-arrival.py'))
A = importlib.util.module_from_spec(spec); spec.loader.exec_module(A)
L, ROOT, P, I, B, D = A.L, A.ROOT, A.P, A.I, A.B, A.D
SOURCE = ROOT/'target/crossing-arrival/v1/summary.json'
PROFILE = 'native_high_ledge_probe_v1'
SPEC = '1:22575,22965:276:282'
CHANGED = {'scenarios/spacewars/src/surface_sortie/jetpack.rs',
           'crates/spacewars-ai/examples/surface_mission_soak.rs'}
NEW = ('scenarios/spacewars/src/surface_sortie/jetpack/diagnostics.rs',
       'crates/spacewars-ai/examples/support/high_ledge_probe.rs',
       'crates/spacewars-ai/tests/fixtures/recovery-route-stall.json')
OWN = ('tools/probe-high-ledge.py','tools/tests/test_high_ledge_probe.py','docs/high-ledge-probe-plan.md')
MOVEMENT = 0x53550002


def inputs():
    return dict(A.inputs(), **{p:P.digest(ROOT/p) for p in (*NEW,*OWN)})


def command(source, binary, out):
    cmd = list(source['runs'][A.TARGET]['command'])
    assert '--probe-high-ledge' not in cmd
    cmd[0] = str(binary)
    cmd[cmd.index('--out')+1] = str(out/'raw'/A.TARGET)
    return cmd + ['--probe-high-ledge', SPEC]


def verify(plan, out, reaudit=False):
    assert plan['profile'] == PROFILE
    current = inputs()
    assert current.keys() == plan['inputs'].keys()
    changed = {p for p in current if current[p] != plan['inputs'][p]}
    assert not changed or reaudit and changed <= set(OWN[:2])
    assert P.digest(SOURCE) == plan['source_sha256']
    source = json.loads(SOURCE.read_text())
    assert source['complete'] and source['profile'] == A.PROFILE
    assert {p for p in source['inputs'] if source['inputs'][p] != plan['inputs'][p]} == CHANGED
    assert plan['command'] == command(source, Path(plan['binary']['path']), out)
    assert P.digest(plan['binary']['path']) == plan['binary']['sha256']
    assert P.digest(source['binary']['path']) == source['binary']['sha256']
    assert plan['baseline_binary'] == source['binary']
    return source


def freeze(out, binary):
    assert not subprocess.check_output(['git','status','--porcelain'], cwd=ROOT, text=True).strip()
    out.mkdir(parents=True, exist_ok=False)
    for name in ('raw','logs','archives','inputs'): (out/name).mkdir()
    target = out/'surface_mission_soak'; shutil.copy2(binary, target); target.chmod(0o555)
    source = json.loads(SOURCE.read_text())
    plan = dict(schema=1, profile=PROFILE, inputs=inputs(), source_sha256=P.digest(SOURCE),
        runtime_source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        binary=dict(path=str(target),sha256=P.digest(target)), baseline_binary=source['binary'],
        command=command(source,target,out), fresh_games=0, main_replays=1,
        fork_sources=[22575,22965], ground_deadline=23678, default_promotion=False)
    verify(plan,out); I.write(out/'plan.json',plan)
    print('Frozen one unchanged main game, two control forks and at most two flight attempts.',flush=True)


def first_candidate(candidates):
    assert [(c['margin'],c['height']) for c in candidates] == [(m,h) for m in range(8) for h in (3,5,7)]
    for c in candidates:
        plan = c['plan']; assert plan['anchor'] == {'GroundGap':{'from':276,'to':282}}
        assert all(math.isfinite(v) for p in ('start','destination') for v in plan[p].values())
        assert plan['planet'] == 0 and plan['revision'] == 21
        assert not c['ordinary_height_allowed']
        assert isinstance(c['corridor_clear'], bool)
    return next((c for c in candidates if c['corridor_clear']), None)


def movement(action, seat):
    a = action.get('Scenario') or {}
    return a.get('kind') == MOVEMENT and len(a.get('payload',[])) == 8 and a['payload'][-1] == seat


def controls_only(original, trial, seat):
    assert len(original) == len(trial)
    replaced = 0
    for a,b in zip(original,trial):
        if movement(a,seat):
            assert movement(b,seat); replaced += 1
        else: assert a == b, 'fork changed a non-movement input'
    assert replaced == 1


def audit_flight(root, source, tape):
    execution = source['execution']; rows = list(D.rows(root/f"high-ledge-flight-{source['tick']}.jsonl"))
    assert rows and rows[0]['observation']['tick'] == source['tick']
    lowest = 1.0; launch = None
    for index,row in enumerate(rows):
        o = row['observation']; tick = source['tick']+index
        assert o['tick'] == tick <= source['deadline']
        if row['applied']:
            assert o['stop'] is None and tick < source['deadline']
            controls_only(tape[tick]['actions'],row['actions'],1)
        else: assert index == len(rows)-1 and o['stop'] == execution['reason']
        if 'jetpack' in o:
            charge = o['jetpack']['charge']; assert math.isfinite(charge) and 0 <= charge <= 1
            lowest = min(lowest,charge)
            if row['applied']: assert charge >= .05
            ground = o['ground']
            if ground['goal'] == 'jetpack_lift' and row['applied']:
                control = next(a['Scenario']['payload'] for a in row['actions'] if movement(a,1))
                if control[4] and launch is None:
                    launch = tick; assert charge >= .98
        if launch is not None: assert tick-launch <= 720
    assert execution['launched_tick'] == launch
    assert execution['last_tick'] == rows[-1]['observation']['tick']
    assert execution['lowest_charge'] == lowest
    assert execution['last'] == rows[-1]['observation']
    assert rows[0]['observation']['pilot'] == tape[source['tick']]['native']['pilots'][1]
    success = execution['reason'] == 'crossing_complete'
    if success:
        last = execution['last']; p = last['pilot']; c = last['ground']['crossing']
        assert last['tick'] < source['deadline'] and launch is not None and last['tick']-launch < 720
        assert lowest >= .05 and c['goal'] == 'Complete' and c['completed_tick'] == last['tick']
        assert c['plan']['anchor'] == {'GroundGap':{'from':276,'to':282}}
        assert p['supported_planet'] == 0 and p['balanced']
        assert A.footing(p,c['plan'])['distance'] < 1
        m=p['planet']['motion']; a=p['actor']; offset={k:a['position'][k]-m['position'][k] for k in ('x','y')}
        velocity=dict(x=m['velocity']['x']-m['spin']*offset['y'],y=m['velocity']['y']+m['spin']*offset['x'])
        assert math.hypot(*(a['velocity'][k]-velocity[k] for k in ('x','y'))) < 1
    return dict(rows=len(rows),success=success,reason=execution['reason'],
                launch_tick=launch,last_tick=execution['last_tick'],lowest_charge=lowest)


def probe_audit(root):
    probe = json.loads((root/'high-ledge-probe.json').read_text())
    assert probe['seat'] == 1 and probe['ticks'] == [22575,22965] and probe['gap'] == [276,282]
    assert probe['flight_limit_ticks'] == 720 and math.isclose(probe['landing_reserve'],.05,abs_tol=1e-8)
    tape_rows=list(D.rows(root/'high-ledge-reference.jsonl')); tape={r['tick']:r for r in tape_rows}
    assert len(tape)==len(tape_rows) and list(tape)==list(range(22575,23679))
    paired={}
    for row in D.rows(root/'trace.jsonl'):
        if row['tick'] not in tape: continue
        p=L.X.R.pilot(row); expected=tape[row['tick']]['native']['pilots'][row['seat']]
        assert expected == {k:p[k] for k in expected}
        paired.setdefault(row['tick'],[]).extend(row['actions'])
    assert all(paired[t] == tape[t]['actions'] for t in tape)
    fixture=json.loads((ROOT/'crates/spacewars-ai/tests/fixtures/recovery-route-stall.json').read_text())
    sources={s['tick']:s for s in fixture['samples']}; result=[]
    for s in probe['results']:
        old=sources[s['tick']]; o=s['observation']
        assert o['ground']==old['ground'] and o['jetpack']==old['jetpack'] and o['flight']['pilot']==old['pilot']
        assert s['deadline']==23678 and s['control_exact_ticks']==23678-s['tick']+1
        selected=first_candidate(s['candidates']); assert s['selected']==selected
        assert (s['execution'] is None)==(selected is None)
        flight=audit_flight(root,s,tape) if selected is not None else None
        result.append(dict(tick=s['tick'],clear_candidates=sum(c['corridor_clear'] for c in s['candidates']),
            control_exact_ticks=s['control_exact_ticks'],flight=flight))
    assert [r['tick'] for r in result]==[22575,22965]
    return dict(results=result,reference_ticks=len(tape),
        successful_local_flights=sum(r['flight'] is not None and r['flight']['success'] for r in result),
        default_promotion=False)


def execute(path, reaudit):
    plan=json.loads(path.read_text()); out=path.parent; source=verify(plan,out,reaudit)
    prior=source['runs'][A.TARGET]; root=out/'raw'/A.TARGET; log=out/'logs/game.log'
    marker=out/'recording.json'
    if marker.exists():
        assert reaudit
        recorded=json.loads(marker.read_text()); assert recorded['command']==plan['command']
        assert P.digest(log)==recorded['log_sha256']
        assert all(P.digest(root/p)==sha for p,sha in recorded['hashes'].items())
    else:
        assert not reaudit and not root.exists() and not log.exists(), 'never retry a partial game'
        with log.open('x') as stream: subprocess.run(plan['command'],stdout=stream,stderr=stream,check=True,timeout=1800)
        recorded=dict(command=plan['command'],hashes=I.raw_hashes(root),log_sha256=P.digest(log)); I.write(marker,recorded)
    summary=dict(schema=1,profile=PROFILE,complete=False,plan_sha256=P.digest(path),reaudit=reaudit,
        inputs=inputs(),runtime_source_commit=plan['runtime_source_commit'],binary=plan['binary'],fresh_games=0)
    try:
        assert set(prior['hashes']) <= set(recorded['hashes'])
        assert all(n.startswith('high-ledge-') for n in set(recorded['hashes'])-set(prior['hashes']))
        run=dict(item=prior['item'],command=plan['command'],hashes={k:recorded['hashes'][k] for k in prior['hashes']})
        run.update(L.Q.analyze(root,run,root))
        old=out/'inputs'/A.TARGET; old.mkdir(exist_ok=True)
        L.extract(prior['archive'],old,('report.json','sensors.jsonl','live-planning.csv'))
        summary['parity']=L.C.parity(prior,run,old)
        report=json.loads((root/'report.json').read_text())
        summary['observer']=L.observer_audit(D.rows(root/'impact.jsonl'),D.rows(root/'trace.jsonl'),report,report['seat'])
        summary['probe']=probe_audit(root)
        summary['probe_evidence']=json.loads((root/'high-ledge-probe.json').read_text())
        summary['report']=report
        verify(plan,out,reaudit)
        summary['archive']=B.pack(root,out/'archives'/('replay-'+P.digest(__file__)[:12]+'.tar.gz'))
        summary['complete']=True
    except BaseException:
        summary['error']=traceback.format_exc(); raise
    finally: I.write(out/'summary.json',summary)
    print('Audited replay and physical probes:',summary['probe'],flush=True)


if __name__=='__main__':
    assert __debug__
    parser=argparse.ArgumentParser(description=__doc__); sub=parser.add_subparsers(dest='action',required=True)
    p=sub.add_parser('plan'); p.add_argument('--out',type=Path,required=True); p.add_argument('--binary',type=Path,required=True)
    p=sub.add_parser('run'); p.add_argument('--plan',type=Path,required=True); p.add_argument('--reaudit',action='store_true')
    args=parser.parse_args()
    if args.action=='plan': freeze(args.out.resolve(),args.binary.resolve())
    else: execute(args.plan.resolve(),args.reaudit)
