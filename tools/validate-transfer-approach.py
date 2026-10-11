#!/usr/bin/env python3
"""Frozen clear-entry geometry during committed post-escape travel."""
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

spec = importlib.util.spec_from_file_location('travel', Path(__file__).with_name('validate-escape-travel.py'))
T = importlib.util.module_from_spec(spec); spec.loader.exec_module(T)
E,I,H,C,F = T.E,T.I,T.H,T.C,T.F
PROFILE = 'clear_transfer_approach_v1'


def command(old, binary, root, enabled):
    cmd = list(old['command']); assert '--transfer-approach-seats' not in cmd
    cmd[0] = str(binary); cmd[cmd.index('--out')+1] = str(root)
    if enabled:
        seat = str(old['item']['seat'])
        assert cmd[cmd.index('--escape-travel-seats')+1] in [seat,'both']
        cmd += ['--transfer-approach-seats',seat]
    return cmd


def without_option(value):
    if isinstance(value,dict): return {k:without_option(v) for k,v in value.items() if k != 'transfer_approach'}
    if isinstance(value,list): return [without_option(v) for v in value]
    return value


def unchanged_state(old, new):
    roots = [C.P.root_of(r) for r in [old, new]]
    reports = [without_option(json.loads((r/'report.json').read_text())) for r in roots]
    C.P.M.T.report_parity(*reports, C.P.M.V.EXACT_REPORT_FIELDS +
        ['metrics','policy_configuration','cover_response','destination_retry','pursuit_health','actual_route_recovery','capture_escape','escape_travel'])
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


def point(center, bearing, radius):
    return tuple(c+b*radius for c,b in zip(center,bearing))


def near(a,b):
    assert math.dist(a,b)<.004,(a,b)


def clear(o, target, entry):
    return (all(math.dist(entry,E.vector(v['motion']['position'])) >= v['radius']+105.
                for v in o['planets'] if v['index'] != target)
            and (o['sun'] is None or math.dist(entry,E.vector(o['sun']['position'])) >= o['sun']['radius']+110.)
            and math.dist(entry,E.vector(o['boundary']['center'])) <= o['boundary']['radius']-20.)


def audit_step(row, previous):
    w=row['transfer_approach'];state=w['telemetry'];s=state['last'];o=w['observation']
    p=o['local']['combat']['recovery']['flight']['pilot'];tick=p['tick']
    assert row['pilot']=={k:v for k,v in p.items() if k!='sites'} and row['planets']==o['planets']
    travel=row['escape_travel']['telemetry']['last']
    assert o==row['escape_travel']['observation'] and travel['finished_tick'] is None
    assert p['controls_armed'] and p['queries_ready'] and p['ship_available']
    assert o['local']['combat']['recovery']['flight']['flight']['enabled']
    assert p['ship_form']=='ship' and isinstance(p['location'],dict) and 'aboard' in p['location']
    assert p['landing']['supported_feet']==0 and row['capture'] is None
    assert s['tick']==tick<travel['deadline_tick'] and s['vehicle']==p['vehicle']==travel['vehicle']
    for k in ['started_tick','deadline_tick','selected_tick','destination']:assert s[k]==travel[k],k
    assert row['mission']['target']==s['destination']
    target=next(v for v in o['planets'] if v['index']==s['destination'])
    center=E.vector(target['motion']['position']);ship=E.vector(p['ship']['position']);radius=target['radius']+85.
    offset=tuple(x-y for x,y in zip(ship,center));length=math.hypot(*offset)
    radial=tuple(x/length if length>0 else 0. for x in offset)
    ordinary=point(center,radial,radius);near(E.vector(s['ordinary_entry']),ordinary)
    prior=(previous or {}).get('last');checked=0;bearing=radial;entry=ordinary;alternate=False;reason='ordinary entry clear'
    if prior and prior['alternate'] and all(prior[k]==s[k] for k in ['started_tick','selected_tick','vehicle','destination']):
        checked+=1;old_entry=point(center,E.vector(prior['bearing']),radius)
        if clear(o,s['destination'],old_entry):
            entry=old_entry;bearing=E.vector(prior['bearing']);alternate=True;reason='retaining clear approach bearing'
    if not alternate:
        checked+=1
        if not clear(o,s['destination'],ordinary):
            candidates=[]
            for i in range(1,32):
                angle=math.tau*i/32.;c=math.cos(angle);sn=math.sin(angle)
                unit=(radial[0]*c-radial[1]*sn,radial[0]*sn+radial[1]*c)
                candidate=point(center,unit,radius);checked+=1
                if clear(o,s['destination'],candidate):candidates.append((math.dist(ship,candidate),unit,candidate))
            if candidates:
                _,bearing,entry=min(candidates,key=lambda v:v[0]);alternate=True;reason='selected clear approach bearing'
            else:reason='no clear entry; ordinary guidance retained'
    assert s['reason']==reason and s['alternate']==alternate and s['candidates_checked']==checked<=33
    near(E.vector(s['entry']),entry);near(E.vector(s['bearing']),bearing)
    guided=row['mission']['goal']=='transfer'
    assert s['used_for_transfer']==guided and row['mission']['goal'] in ['transfer','launch']
    assert not E.flight_controls(row)['interact']
    for k,increment in [('evaluated_ticks',1),('alternate_ticks',alternate),('guided_ticks',alternate and guided)]:
        assert state[k]==(previous or {}).get(k,0)+int(increment),k
    return state


def audit_approach(root,item,enabled):
    report=json.loads((root/'report.json').read_text())
    assert report.get('transfer_approach')==(dict(profile=PROFILE,enabled_seats=[i==item['seat'] for i in range(2)]) if enabled else None)
    for seat,m in enumerate(report['missions']):assert ('transfer_approach' in m)==(enabled and seat==item['seat'])
    previous=None;witnesses=[];reasons=Counter();first=None
    for row in F.rows(root/'capture-evidence.jsonl'):
        w=row.get('transfer_approach')
        if w is None:
            if enabled and row['seat']==item['seat'] and 'escape_travel' in row:
                travel=row['escape_travel']['telemetry']['last'];p=row['pilot']
                if travel['guidance'] in ['transfer','launch'] and p['landing']['supported_feet']==0:
                    raise AssertionError(('missing approach observation',p['tick']))
            continue
        assert enabled and row['seat']==item['seat']
        previous=audit_step(row,previous);sample=previous['last'];witnesses.append(row);reasons[sample['reason']]+=1
        if first is None and sample['alternate'] and sample['used_for_transfer']:first=sample['tick']
    final=report['missions'][item['seat']].get('transfer_approach')
    if enabled:assert final==(previous or dict(evaluated_ticks=0,alternate_ticks=0,guided_ticks=0,last=None))
    else:assert final is None
    path=root/'transfer-approach-witnesses.json';F.D.write(path,dict(schema=1,profile=PROFILE,enabled=enabled,rows=witnesses))
    return dict(telemetry=final,reasons=dict(reasons),first_guided_alternate_tick=first,witness_sha256=F.E.digest(path))


def analyze_run(entry,old,root,candidate,active,result):
    result.update(C.analyze(root,entry['item']))
    result['initial_cover']=I.audit_initial(root,entry['item'],entry['enabled'])
    result['handoff']=H.audit_handoffs(root,entry['item'],entry['enabled'])
    result['actual_recovery']=E.R.audit_recovery(root,entry['item'],entry['enabled'])
    result['capture_escape']=E.audit_escape(root,entry['item'],entry['enabled'])
    result['escape_travel']=T.audit_travel(root,entry['item'],entry['enabled'])
    result['transfer_approach']=audit_approach(root,entry['item'],active)
    if entry['source']=='health': result['pursuit_health']=I.H.audit_health(root,entry['item'])
    if candidate:
        result['comparison']=C.M.compare(C.P.root_of(old),root)
        difference=result['comparison']['first_control_difference'];first=result['transfer_approach']['first_guided_alternate_tick']
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
        if entry['source']=='health':assert result['pursuit_health']==old['pursuit_health']
    result['hashes']={p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    return result


def run(entry,old,binary,out,candidate):
    root=out/(entry['name']+('-approach' if candidate else '-retained'));active=candidate and entry['enabled']
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
        tools={name:F.E.digest(Path(__file__).with_name(name)) for name in set(prior['tools'])|{'validate-escape-travel.py'}},
        plan=I.plan(),retention={},runs={})
    save=lambda:F.D.write(args.out/'summary.json',result);save()
    try:
        for candidate in [False,True]:
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures=[pool.submit(run,e,prior['runs'][e['name']],binary,args.out,candidate) for e in result['plan']]
                for entry,future in zip(result['plan'],futures):
                    new=future.result();(result['runs'] if candidate else result['retention'])[entry['name']]=new
                    save();assert 'error' not in new,(entry['name'],new.get('error'))
                    print(entry['name']+(': approach audited' if candidate else ': retained exactly'),flush=True)
        I.H.verify_inputs(prior)
        assert F.E.digest(args.prior)==result['prior_summary']['sha256'] and F.E.digest(binary)==result['binary_sha256']
        result['complete']=True
    except BaseException as error:result['error']=repr(error);raise
    finally:save()


if __name__=='__main__':main()
