#!/usr/bin/env python3
"""Frozen relative closing-speed guidance during committed transfer."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import csv
import importlib.util
from itertools import zip_longest
import json
import math
from pathlib import Path
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('approach', Path(__file__).with_name('validate-transfer-approach.py'))
A = importlib.util.module_from_spec(spec); spec.loader.exec_module(A)
T,E,I,H,C,F = A.T,A.E,A.I,A.H,A.C,A.F
PROFILE = 'relative_transfer_speed_v1'


def command(old, binary, root, enabled):
    cmd = list(old['command']); assert '--transfer-speed-seats' not in cmd
    cmd[0] = str(binary); cmd[cmd.index('--out')+1] = str(root)
    if enabled:
        seat = str(old['item']['seat'])
        assert cmd[cmd.index('--transfer-approach-seats')+1] in [seat,'both']
        cmd += ['--transfer-speed-seats',seat]
    return cmd


def without_option(value):
    if isinstance(value,dict): return {k:without_option(v) for k,v in value.items() if k != 'transfer_speed'}
    if isinstance(value,list): return [without_option(v) for v in value]
    return value


def unchanged_state(old, new):
    roots = [C.P.root_of(r) for r in [old, new]]
    reports = [without_option(json.loads((r/'report.json').read_text())) for r in roots]
    C.P.M.T.report_parity(*reports, C.P.M.V.EXACT_REPORT_FIELDS +
        ['metrics','policy_configuration','cover_response','destination_retry','pursuit_health','actual_route_recovery','capture_escape','escape_travel','transfer_approach'])
    assert old['allocation'] == new['allocation']
    streams = C.P.M.V.EXACT_STREAMS + ['capture-evidence.jsonl']
    for name in streams:
        for a,b in zip_longest(*(F.rows(r/name) for r in roots)):
            assert a is not None and b is not None and without_option(a) == without_option(b), name
    ledgers = []
    for root in roots:
        with (root/'live-planning.csv').open() as f:
            ledgers.append([{k:v for k,v in row.items() if k != 'dispatch_ms'} for row in csv.DictReader(f)])
    assert ledgers[0] == ledgers[1]
    ignored = {'reused_ground','snapshot_total_ms','snapshot_max_ms','validation_total_ms','validation_max_ms'}
    telemetry = [{k:v for k,v in r['live_objective_planning']['telemetry'].items() if k not in ignored} for r in reports]
    assert telemetry[0] == telemetry[1]
    return dict(exact_streams_except_option_telemetry=streams,
        sensor_parity=C.P.M.C.audit_sensors(*roots), exact_allocation_ledger=True)


def dot(a,b):return sum(x*y for x,y in zip(a,b))


def near(a,b):
    if isinstance(a,(tuple,list)):assert math.dist(a,b)<.004,(a,b)
    else:assert abs(a-b)<.004,(a,b)


def limits(o,destination):
    f=o['local']['combat']['recovery']['flight'];p=f['pilot'];flight=f['flight']['limits']
    turn=math.pi/max(flight['turn_speed'],.1)+.6;result=[]
    for v in o['planets']:
        if v['index']==destination:continue
        offset=E.subtract(p['ship']['position'],v['motion']['position']);length=math.hypot(*offset)
        normal=tuple(x/length if length>0 else 0. for x in offset);clearance=length-v['radius']
        closing=max(0.,-dot(E.subtract(p['ship']['velocity'],v['motion']['velocity']),normal))
        deceleration=min(25.,max(5.,flight['brake_acceleration']*.5-max(0.,-dot(E.vector(p['gravity']),normal))))
        reserve=deceleration*turn;cap=math.sqrt(reserve*reserve+2*deceleration*max(clearance-70.,0.))-reserve
        result.append(dict(planet=v['index'],normal=dict(zip(['x','y'],normal)),clearance=clearance,
            closing_speed=closing,turn_seconds=turn,deceleration=deceleration,maximum_closing_speed=cap,
            minimum_world_normal_speed=dot(E.vector(v['motion']['velocity']),normal)-cap))
    return result


def project(desired, constraints):
    feasible=lambda v:all(dot(E.vector(c['normal']),E.vector(c['normal']))>.5 and dot(v,E.vector(c['normal']))>=c['minimum_world_normal_speed']-.001 for c in constraints)
    checked=1
    if feasible(desired):return desired,checked
    candidates=[]
    for i,a in enumerate(constraints):
        n=E.vector(a['normal']);minimum=a['minimum_world_normal_speed'];norm=dot(n,n)
        if norm<=.5:continue
        candidates.append(tuple(x+u*(minimum-dot(desired,n))/norm for x,u in zip(desired,n)))
        for b in constraints[:i]:
            m=E.vector(b['normal']);other=b['minimum_world_normal_speed'];det=n[0]*m[1]-n[1]*m[0]
            if abs(det)<=.0001:continue
            candidates.append(((minimum*m[1]-n[1]*other)/det,(n[0]*other-minimum*m[0])/det))
    checked+=len(candidates)
    admitted=[v for v in candidates if feasible(v) and math.dist(v,desired)<=55.]
    return (min(admitted,key=lambda v:math.dist(v,desired)) if admitted else None),checked


def audit_step(row,previous):
    w=row['transfer_speed'];state=w['telemetry'];s=state['last'];o=w['observation']
    f=o['local']['combat']['recovery']['flight'];p=f['pilot'];tick=p['tick']
    assert row['pilot']=={k:v for k,v in p.items() if k!='sites'} and row['planets']==o['planets']
    assert o==row['transfer_approach']['observation']==row['escape_travel']['observation']
    approach=row['transfer_approach']['telemetry']['last'];travel=row['escape_travel']['telemetry']['last']
    assert approach['tick']==s['tick']==tick and approach['used_for_transfer'] and row['mission']['goal']=='transfer'
    assert travel['finished_tick'] is None and tick<travel['deadline_tick']
    for k in ['started_tick','deadline_tick','selected_tick','vehicle','destination']:assert s[k]==approach[k]==travel[k],k
    assert p['controls_armed'] and p['queries_ready'] and p['ship_available'] and f['flight']['enabled']
    assert p['ship_form']=='ship' and p['vehicle']==s['vehicle'] and p['landing']['supported_feet']==0
    assert isinstance(p['location'],dict) and 'aboard' in p['location'] and row['capture'] is None
    target=next(v for v in o['planets'] if v['index']==s['destination'])
    waypoint=E.vector(approach['entry']);velocity=E.vector(target['motion']['velocity'])
    avoidance=w['avoidance']
    if avoidance:
        waypoint=E.vector(avoidance['waypoint']);obstacle=avoidance['obstacle']
        if obstacle=='sun':velocity=(0.,0.)
        else:velocity=E.vector(next(v for v in o['planets'] if v['index']==obstacle['planet'])['motion']['velocity'])
    delta=tuple(x-y for x,y in zip(waypoint,E.vector(p['ship']['position'])));length=math.hypot(*delta)
    desired=tuple(v+(x/length if length>0 else 0.)*min(length*.7,55.) for x,v in zip(delta,velocity))
    near(E.vector(s['desired_before']),desired)
    expected=limits(o,s['destination']);assert len(expected)==len(s['limits'])
    for a,b in zip(s['limits'],expected):
        assert a['planet']==b['planet']
        for k in b:
            if k=='planet':continue
            near(E.vector(a[k]) if k=='normal' else a[k],E.vector(b[k]) if k=='normal' else b[k])
    # Validate the serialized f32 constraints above, then solve their represented
    # half-planes independently without magnifying cross-language rounding.
    projected,checked=project(E.vector(s['desired_before']),s['limits'])
    assert s['candidates_checked']==checked<=1+len(expected)+len(expected)*(len(expected)-1)//2
    assert s['feasible']==(projected is not None)
    near(E.vector(s['desired_after']),projected if projected is not None else E.vector(s['desired_before']))
    limited=s['desired_after']!=s['desired_before'];assert s['limited']==limited
    force=projected is None or any(c['closing_speed']>c['maximum_closing_speed']+.001 for c in s['limits'])
    assert s['force_brake']==force
    controls=E.flight_controls(row);assert not controls['interact']
    if force:
        assert controls['brake'] and row['actions'][1]['Scenario']['payload'][1]==0
    for k,inc in [('evaluated_ticks',1),('limited_ticks',limited),('braking_ticks',force),('infeasible_ticks',projected is None)]:
        assert state[k]==(previous or {}).get(k,0)+int(inc),k
    return state


def audit_speed(root,item,enabled):
    report=json.loads((root/'report.json').read_text())
    assert report.get('transfer_speed')==(dict(profile=PROFILE,enabled_seats=[i==item['seat'] for i in range(2)]) if enabled else None)
    for seat,m in enumerate(report['missions']):assert ('transfer_speed' in m)==(enabled and seat==item['seat'])
    previous=None;witnesses=[];first=None
    for row in F.rows(root/'capture-evidence.jsonl'):
        w=row.get('transfer_speed')
        if w is None:
            if enabled and row['seat']==item['seat'] and 'transfer_approach' in row:
                assert not row['transfer_approach']['telemetry']['last']['used_for_transfer'],'missing speed decision'
            continue
        assert enabled and row['seat']==item['seat']
        previous=audit_step(row,previous);sample=previous['last'];witnesses.append(row)
        if first is None and (sample['limited'] or sample['force_brake']):first=sample['tick']
    final=report['missions'][item['seat']].get('transfer_speed')
    if enabled:assert final==(previous or dict(evaluated_ticks=0,limited_ticks=0,braking_ticks=0,infeasible_ticks=0,last=None))
    else:assert final is None
    path=root/'transfer-speed-witnesses.json';F.D.write(path,dict(schema=1,profile=PROFILE,enabled=enabled,rows=witnesses))
    return dict(telemetry=final,first_intervention_tick=first,witness_sha256=F.E.digest(path))


def analyze_run(entry,old,root,candidate,active,result):
    result.update(C.analyze(root,entry['item']))
    result['initial_cover']=I.audit_initial(root,entry['item'],entry['enabled'])
    result['handoff']=H.audit_handoffs(root,entry['item'],entry['enabled'])
    result['actual_recovery']=E.R.audit_recovery(root,entry['item'],entry['enabled'])
    result['capture_escape']=E.audit_escape(root,entry['item'],entry['enabled'])
    result['escape_travel']=T.audit_travel(root,entry['item'],entry['enabled'])
    result['transfer_approach']=A.audit_approach(root,entry['item'],entry['enabled'])
    result['transfer_speed']=audit_speed(root,entry['item'],active)
    if entry['source']=='health': result['pursuit_health']=I.H.audit_health(root,entry['item'])
    if candidate:
        result['comparison']=C.M.compare(C.P.root_of(old),root)
        difference=result['comparison']['first_control_difference'];first=result['transfer_speed']['first_intervention_tick']
        if first is None:result['unchanged_state']=unchanged_state(old,result)
        if difference:assert first is not None and difference['tick']>=first
        if not active:result['disabled_control_parity']=C.P.replay_parity(old,result)
    else:
        result['replay_parity']=C.P.replay_parity(old,result)
        assert result['initial_cover']==old['initial_cover']
        E.R.A.assert_handoff_parity(result['handoff'],old['handoff'])
        assert result['actual_recovery']==old['actual_recovery']
        assert result['capture_escape']==old['capture_escape']
        assert result['escape_travel']==old['escape_travel']
        assert result['transfer_approach']==old['transfer_approach']
        if entry['source']=='health':assert result['pursuit_health']==old['pursuit_health']
    result['hashes']={p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    return result


def run(entry,old,binary,out,candidate):
    root=out/(entry['name']+('-speed' if candidate else '-retained'));active=candidate and entry['enabled']
    cmd=command(old,binary,root,active);result=dict(item=old['item'],command=cmd)
    try:
        with (out/(root.name+'.log')).open('x') as log:subprocess.run(cmd,check=True,stdout=log,stderr=log,timeout=1800)
        analyze_run(entry,old,root,candidate,active,result)
    except Exception:result['error']=traceback.format_exc()
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior',type=Path,required=True);parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True);args=parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(),'freeze code, tests and plan first'
    prior=json.loads(args.prior.read_text());assert prior['complete'] and prior['plan']==I.plan()
    I.H.verify_inputs(prior);binary=args.binary.resolve(strict=True);args.out.mkdir(parents=True,exist_ok=False)
    result=dict(schema=1,complete=False,profile=PROFILE,
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),binary_sha256=F.E.digest(binary),
        prior_summary=dict(path=str(args.prior),sha256=F.E.digest(args.prior)),runner_sha256=F.E.digest(Path(__file__)),
        tools={name:F.E.digest(Path(__file__).with_name(name)) for name in set(prior['tools'])|{'validate-transfer-approach.py'}},
        plan=I.plan(),retention={},runs={})
    save=lambda:F.D.write(args.out/'summary.json',result);save()
    try:
        for candidate in [False,True]:
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures=[pool.submit(run,e,prior['runs'][e['name']],binary,args.out,candidate) for e in result['plan']]
                for entry,future in zip(result['plan'],futures):
                    new=future.result();(result['runs'] if candidate else result['retention'])[entry['name']]=new
                    save();assert 'error' not in new,(entry['name'],new.get('error'))
                    print(entry['name']+(': speed audited' if candidate else ': retained exactly'),flush=True)
        I.H.verify_inputs(prior)
        assert F.E.digest(args.prior)==result['prior_summary']['sha256'] and F.E.digest(binary)==result['binary_sha256']
        result['complete']=True
    except BaseException as error:result['error']=repr(error);raise
    finally:save()


if __name__=='__main__':main()
