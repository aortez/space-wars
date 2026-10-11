#!/usr/bin/env python3
"""Test a terrain-gap arrival predicate against four retained complete games."""
import argparse
import copy
import importlib.util
from itertools import zip_longest
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('sequences', Path(__file__).with_name('diagnose-live-claim-sequences.py'))
L = importlib.util.module_from_spec(spec); spec.loader.exec_module(L)
ROOT, P, I, B, D = L.ROOT, L.P, L.I, L.B, L.D
PROFILE = 'ground_gap_destination_arrival_v1'
SOURCE = ROOT/'target/live-claim-sequences/v1/summary.json'
TARGET = L.NAMES[1]
ORDER = (L.NAMES[0], L.NAMES[2], L.NAMES[3], TARGET)
RUNTIME = 'crates/spacewars-ai/src/jetpack_crossing.rs'
NEW = ('crates/spacewars-ai/src/jetpack_crossing_tests.rs',
       'crates/spacewars-ai/tests/fixtures/ground-gap-arrivals.json')
OWN = ('tools/validate-crossing-arrival.py','tools/tests/test_crossing_arrival.py',
       'docs/crossing-arrival-plan.md')


def inputs():
    return dict(L.inputs(), **{p:P.digest(ROOT/p) for p in (*NEW,*OWN)})


def jobs(source, binary, out):
    result = []
    for name in ORDER:
        prior = source['runs'][name]; command = list(prior['command'])
        assert len(command)%2 == 1 and len(set(command[1::2])) == len(command[1::2])
        command[0] = str(binary); command[command.index('--out')+1] = str(out/'raw'/name)
        item = dict(prior['item'],stage='selected_arrival_candidate',role='affected' if name==TARGET else 'retention')
        result.append(dict(item=item,command=command))
    return result


def verify(plan, out, reaudit=False):
    assert plan['profile'] == PROFILE
    current = inputs(); assert current.keys() == plan['inputs'].keys()
    changed = {p for p in current if current[p] != plan['inputs'][p]}
    assert not changed or reaudit and changed <= set(OWN[:2])
    assert P.digest(plan['source']['path']) == plan['source']['sha256']
    source = json.loads(Path(plan['source']['path']).read_text())
    assert source['complete'] and source['profile'] == L.PROFILE
    assert set(plan['inputs']) - set(source['inputs']) == set((*NEW,*OWN))
    assert not set(source['inputs']) - set(plan['inputs'])
    assert {p for p in source['inputs'] if source['inputs'][p] != plan['inputs'][p]} == {RUNTIME}
    assert plan['baseline_binary'] == source['binary']
    assert P.digest(plan['baseline_binary']['path']) == plan['baseline_binary']['sha256']
    assert P.digest(plan['binary']['path']) == plan['binary']['sha256'] != source['binary']['sha256']
    assert plan['jobs'] == jobs(source,Path(plan['binary']['path']),out)
    assert plan['sources'] == {n:source['runs'][n] for n in ORDER}
    return source


def freeze(out, binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    source = json.loads(SOURCE.read_text()); out.mkdir(parents=True,exist_ok=False)
    for name in ('raw','logs','archives','inputs'): (out/name).mkdir()
    copied = out/'surface_mission_soak'; shutil.copy2(binary,copied); copied.chmod(0o555)
    plan = dict(schema=1,profile=PROFILE,inputs=inputs(),fresh_games=0,default_promotion=False,
        runtime_source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        source=dict(path=str(SOURCE),sha256=P.digest(SOURCE)),baseline_binary=source['binary'],
        binary=dict(path=str(copied),sha256=P.digest(copied)),sources={n:source['runs'][n] for n in ORDER},
        jobs=jobs(source,copied,out),build_command=['cargo','+1.89.0','build','--release','--locked',
            '-p','spacewars-ai','--example','surface_mission_soak','--features','sensor-profile'])
    verify(plan,out); I.write(out/'plan.json',plan)
    print('Frozen three exact controls and one affected full-game candidate.',flush=True)


def footing(pilot, plan):
    a, m, up = pilot['actor'], pilot['planet']['motion'], pilot['actor_up']
    x, y = [a['position'][k]-.45*up[k]-m['position'][k] for k in ('x','y')]
    c, s = math.cos(m['angle']), math.sin(m['angle'])
    local = dict(x=x*c+y*s,y=-x*s+y*c)
    return dict(local_foot=local,distance=math.hypot(*(local[k]-plan['destination'][k] for k in ('x','y'))))


def ground_audit(rows, ticks):
    attempts, witnesses, count = {}, [], 0
    for row in rows:
        tick,seat = row['tick'],row['seat']; assert (tick,seat) == (count//2,count%2); count += 1
        p = L.X.R.pilot(row)
        for scope in ('capture','recovery'):
            task = row['mission'].get(scope) or {}; g = task.get('ground') or {}; crossing = g.get('crossing')
            if not crossing: continue
            key = f"{seat}:{scope}:{g['started_tick']}:{crossing['started_tick']}"
            state = [g['goal'],crossing['goal'],task.get('status')]
            if key not in attempts:
                attempts[key] = dict(seat=seat,scope=scope,ground_started=g['started_tick'],
                    started=crossing['started_tick'],first_plan=crossing['plan'],transitions=[],launch_tick=None,
                    completed_tick=None,lowest_charge=1,completion=None)
            a = attempts[key]
            if not a['transitions'] or a['transitions'][-1]['state'] != state:
                a['transitions'].append(dict(tick=tick,state=state)); witnesses.append(row)
            charge = row['observation']['local']['combat']['recovery']['jetpack']['charge']
            assert math.isfinite(charge) and 0 <= charge <= 1
            a.update(last_tick=tick,last_plan=crossing['plan'],lowest_charge=min(a['lowest_charge'],charge),
                final_reason=g.get('reason'),last_progress_tick=g['last_progress_tick'])
            if crossing['goal'] == 'Lift' and a['launch_tick'] is None: a['launch_tick'] = tick
            if crossing['completed_tick'] == tick:
                assert a['completed_tick'] is None and crossing['goal'] == 'Complete'
                geometry = footing(p,crossing['plan'])
                if 'GroundGap' in crossing['plan']['anchor']:
                    assert geometry['distance'] < 1, 'terrain gap completed away from destination footing'
                    assert p['balanced'] and p['supported_planet'] == crossing['plan']['planet']
                a.update(completed_tick=tick,completion=dict(geometry=geometry,observation=p,ground=g))
    assert count == ticks*2
    return dict(rows=count,scopes=['capture','recovery'],attempts=attempts,witnesses=witnesses)


def prefix(before, after):
    first, action, equal, rows = None,None,0,0
    for a,b in zip_longest(before,after):
        if a is None or b is None: break
        assert (a['tick'],a['seat']) == (b['tick'],b['seat']); rows += 1
        if first is None:
            if a == b: equal += 1
            else:
                assert (b['tick'],b['seat']) == (19963,1)
                assert a['observation'] == b['observation']
                ag,bg = (r['mission']['recovery']['ground'] for r in (a,b))
                assert ag['crossing']['goal'] == 'Complete' and bg['crossing']['goal'] == 'Descend'
                assert ag['jetpack_crossings'] == 4 and bg['jetpack_crossings'] == 3
                assert footing(L.X.R.pilot(b),bg['crossing']['plan'])['distance'] > 3
                first = dict(tick=b['tick'],seat=b['seat'],before=a,after=b)
        if action is None and a['actions'] != b['actions']:
            assert first is not None
            action = dict(tick=b['tick'],seat=b['seat'],before=a['actions'],after=b['actions'])
    assert first is not None and action is not None
    return dict(exact_prefix_rows=equal,common_rows=rows,first_change=first,first_action_change=action)


def run_case(job, plan, out, reaudit):
    name = job['item']['name']; prior = plan['sources'][name]
    root,oldroot = out/'raw'/name,out/'inputs'/name
    log,metadata = out/'logs'/(name+'.log'),out/(name+'-raw.json')
    result = copy.deepcopy(job)
    if metadata.exists():
        assert reaudit
        old = json.loads(metadata.read_text()); assert old['command'] == job['command'] and old['item'] == job['item']
        assert P.digest(log) == old['log_sha256']
        if not root.exists(): B.unpack(json.loads((out/(name+'-result.json')).read_text())['archive'],root)
        result.update(hashes=old['hashes'],log_sha256=old['log_sha256'],reused_raw=True)
    else:
        assert not root.exists() and not log.exists(), 'partial games are never retried'
        with log.open('x') as stream:
            subprocess.run(job['command'],stdout=stream,stderr=stream,check=True,timeout=1800)
        result.update(hashes=I.raw_hashes(root),log_sha256=P.digest(log)); I.write(metadata,result)
    assert set(result['hashes']) == set(prior['hashes'])
    assert all(P.digest(root/k) == v for k,v in result['hashes'].items())
    report = json.loads((root/'report.json').read_text()); L.X.check_configuration(report,job['item'])
    result['stopping_rule'] = L.X.audit_switch(D.rows(root/'trace.jsonl'),report,job['item'])
    result.update(L.Q.analyze(root,job,root))
    result['defense'] = L.C.A.audit(root,job['item'],report)
    result['clearance'] = L.C.audit(root,job['item'],report)
    result['native'] = L.S.native_audit(root,report,job['item'])
    result['observer'] = L.observer_audit(D.rows(root/'impact.jsonl'),D.rows(root/'trace.jsonl'),report,report['seat'])
    ground = ground_audit(D.rows(root/'trace.jsonl'),report['elapsed_ticks']); I.write(root/'ground-arrival-audit.json',ground)
    result['ground'] = dict(sha256=P.digest(root/'ground-arrival-audit.json'),rows=ground['rows'],
        attempts=len(ground['attempts']),completed=sum(a['completed_tick'] is not None for a in ground['attempts'].values()))
    wanted = ('report.json','sensors.jsonl','live-planning.csv') if name != TARGET else ('trace.jsonl',)
    L.extract(prior['archive'],oldroot,wanted)
    if name != TARGET:
        result['parity'] = L.C.parity(prior,result,oldroot)
        assert result['stopping_rule'] == prior['stopping_rule']
        assert json.loads(json.dumps(result['native'])) == prior['native']
    else:
        comparison = prefix(D.rows(oldroot/'trace.jsonl'),D.rows(root/'trace.jsonl'))
        I.write(root/'arrival-prefix.json',comparison)
        result['prefix'] = dict(sha256=P.digest(root/'arrival-prefix.json'),
            exact_prefix_rows=comparison['exact_prefix_rows'],common_rows=comparison['common_rows'],
            first_tick=comparison['first_change']['tick'],first_action_change=comparison['first_action_change'])
    result['sequence'] = L.sequence(root,report,job['item']['seat'])
    assert all(P.digest(root/k) == v for k,v in result['hashes'].items())
    result['audited'] = True
    suffix = '-'+P.digest(__file__)[:12] if reaudit else ''
    result['archive'] = B.pack(root,out/'archives'/(name+suffix+'.tar.gz'))
    shutil.rmtree(oldroot); I.write(out/(name+'-result.json'),result)
    print(name,'audited and archived',flush=True)
    return result


def decision(runs, sources):
    old,new = sources[TARGET]['players'][1],runs[TARGET]['players'][1]
    # A counter correction alone cannot qualify as successful recovery.
    progress = new['completed_recoveries'] > old['completed_recoveries']
    survival = new['pilot_deaths'] <= old['pilot_deaths'] and new['outcome'] == old['outcome']
    losses = new['ships_lost'] <= old['ships_lost']
    return dict(completed_recovery_improved=progress,survival_retained=survival,ship_losses_retained=losses,
        before=old,after=new,decision='advance_to_broader_validation' if progress and survival and losses
        else 'correctness_only_recovery_unproven',default_promotion=False)


def execute(path, reaudit):
    out = path.parent; plan = json.loads(path.read_text()); verify(plan,out,reaudit)
    target = out/'summary.json'
    if target.exists():
        assert reaudit
        backup = out/('summary-before-reaudit-'+P.digest(target)[:12]+'.json')
        assert not backup.exists(); shutil.copy2(target,backup)
    summary = dict(schema=1,profile=PROFILE,complete=False,inputs=inputs(),plan_sha256=P.digest(path),
        binary=plan['binary'],runtime_source_commit=plan['runtime_source_commit'],reaudit=reaudit,runs={})
    I.write(target,summary)
    try:
        for job in plan['jobs']:
            summary['runs'][job['item']['name']] = run_case(job,plan,out,reaudit); I.write(target,summary)
        verify(plan,out,reaudit)
        summary.update(complete=True,screen=decision(summary['runs'],plan['sources']))
    except BaseException:
        summary['error'] = traceback.format_exc(); raise
    finally: I.write(target,summary)
    print('Complete:',summary['screen']['decision'],flush=True)


if __name__ == '__main__':
    assert __debug__
    parser = argparse.ArgumentParser(description=__doc__); sub = parser.add_subparsers(dest='action',required=True)
    p = sub.add_parser('plan'); p.add_argument('--out',type=Path,required=True); p.add_argument('--binary',type=Path,required=True)
    p = sub.add_parser('run'); p.add_argument('--plan',type=Path,required=True); p.add_argument('--reaudit',action='store_true')
    args = parser.parse_args()
    if args.action == 'plan': freeze(args.out.resolve(),args.binary.resolve())
    else: execute(args.plan.resolve(),args.reaudit)
