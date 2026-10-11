#!/usr/bin/env python3
"""Frozen bounded post-escape travel commitment on retained complete matches."""
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

spec = importlib.util.spec_from_file_location('escape', Path(__file__).with_name('validate-capture-escape.py'))
E = importlib.util.module_from_spec(spec); spec.loader.exec_module(E)
I,H,C,F = E.I,E.H,E.C,E.F
PROFILE = 'escape_travel_commitment_v1'


def command(old, binary, root, enabled):
    cmd = list(old['command']); assert '--escape-travel-seats' not in cmd
    cmd[0] = str(binary); cmd[cmd.index('--out')+1] = str(root)
    if enabled:
        seat = str(old['item']['seat'])
        assert cmd[cmd.index('--capture-escape-seats')+1] in [seat,'both']
        cmd += ['--escape-travel-seats',seat]
    return cmd


def without_option(value):
    if isinstance(value,dict): return {k:without_option(v) for k,v in value.items() if k != 'escape_travel'}
    if isinstance(value,list): return [without_option(v) for v in value]
    return value


def unchanged_state(old, new):
    roots = [C.P.root_of(r) for r in [old, new]]
    reports = [without_option(json.loads((r/'report.json').read_text())) for r in roots]
    C.P.M.T.report_parity(*reports, C.P.M.V.EXACT_REPORT_FIELDS +
        ['metrics','policy_configuration','cover_response','destination_retry','pursuit_health','actual_route_recovery','capture_escape'])
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


def pursuit_reason(o, p):
    c=o['local']['combat']; target=c['target']
    if not target or not target['visible']: return None
    distance=math.dist(E.vector(target['motion']['position']),E.vector(p['ship']['position']))
    if (target['ship_form'] != 'ship' or target['health_fraction'] < .5) and distance < 400:
        return 'nearby vulnerable opponent'
    hit=c['weapons']['last_hit_taken_tick']
    if hit is not None and max(0,p['tick']-hit) < 180 and distance < 300:
        return 'responding to incoming fire'
    if distance < 300 and any((v.get('claim') or {}).get('owner') == p['owner'] for v in o['planets']):
        return 'nearby opponent after securing ground'
    return None


def audit_step(row, previous, source):
    w=row['escape_travel']; o=w['observation']
    local,p=I.witness_observation(dict(row,initial_cover=w))
    a=w['telemetry']['last']; tick=p['tick']; controls=E.flight_controls(row)
    assert a['observed_tick'] == tick
    assert a['escape'] == source['capture_escape']['telemetry']['last']
    assert a['escape']['reason'] in ['escape deadline','separation established']
    assert a['escape']['finished_tick'] == a['started_tick'] == source['pilot']['tick']
    assert a['deadline_tick'] == a['started_tick']+3600
    assert a['vehicle'] == a['escape']['vehicle']
    assert a['escape']['abort']['attempt']['actor'] == p['owner']
    if previous is None:
        assert tick == a['started_tick'] and o['match_rules']
        assert p['controls_armed'] and p['queries_ready'] and p['ship_available']
        assert local['combat']['recovery']['flight']['flight']['enabled']
        assert p['ship_form'] == 'ship' and p['vehicle'] == a['vehicle']
        assert isinstance(p['location'],dict) and 'aboard' in p['location']
        assert p['landing']['phase'] == 'flying' and p['landing']['supported_feet'] == 0
        if a['destination'] is not None:
            assert a['selected_tick'] == tick and row['mission']['target'] == a['destination']
    else:
        assert previous['finished_tick'] is None and previous['observed_tick']+1 == tick
        for k in ['escape','started_tick','deadline_tick','vehicle','destination','selected_tick']:
            assert a[k] == previous[k], k
    old=previous or dict(deferred_pursuit_ticks=0,last_deferred_tick=None,last_deferred_reason=None,
                        travel_ticks=0,laser_ticks=0,cannon_ticks=0)
    increment=a['deferred_pursuit_ticks']-old['deferred_pursuit_ticks']
    assert increment in [0,1]
    if increment:
        assert a['last_deferred_tick'] == tick and a['last_deferred_reason'] == pursuit_reason(o,p)
        assert a['last_deferred_reason'] is not None and p['controls_armed']
        assert a['started_tick'] <= tick < a['deadline_tick'] and tick >= a['escape']['deadline_tick']
    else:
        assert a['last_deferred_tick'] == old['last_deferred_tick']
        assert a['last_deferred_reason'] == old['last_deferred_reason']
    guided=a['guidance'] in ['transfer','launch']
    laser=cannon=0
    if guided:
        assert a['finished_tick'] is None and row['mission']['goal'] == a['guidance']
        assert p['controls_armed'] and p['queries_ready'] and local['combat']['recovery']['flight']['flight']['enabled']
        assert not controls['interact'] and row['capture'] is None
        laser,cannon=E.audit_weapons(dict(row,capture_escape=w),local,p)
    for k,inc in [('travel_ticks',guided),('laser_ticks',laser),('cannon_ticks',cannon)]:
        assert a[k] == old[k]+int(inc), k
    if a['finished_tick'] is None:
        assert tick < a['deadline_tick'] and a['reason'] is None and w['pursuit'] is None
        assert p['vehicle'] == a['vehicle'] and p['ship_form'] == 'ship' and p['ship_available']
        assert isinstance(p['location'],dict) and 'aboard' in p['location']
        assert row['mission']['target'] == a['destination'] and a['destination'] is not None
        destination=next(v for v in o['planets'] if v['index'] == a['destination'])
        assert (destination.get('claim') or {}).get('owner') != p['owner']
        distance=math.dist(E.vector(p['ship']['position']),E.vector(destination['motion']['position']))
        assert abs(distance-a['distance']) < .003
        assert a['selected_tick'] == a['started_tick'] <= a['progress_tick'] <= tick
        assert tick-a['progress_tick'] <= 1200 and row['capture'] is None
        if not guided: assert a['guidance'] == 'higher_priority_control'
    else:
        assert a['finished_tick'] == tick and a['guidance'] == 'finished'
        reason=a['reason']; assert reason
        if reason == 'capture task started':
            assert row['capture'] is not None and row['mission']['goal'] == 'capture'
            assert p['queries_ready'] and p['planet']['index'] == row['mission']['target']
            assert math.dist(E.vector(p['ship']['position']),E.vector(p['planet']['motion']['position'])) < p['planet']['radius']+105.001
            assert math.hypot(*E.subtract(p['ship']['velocity'],p['planet']['motion']['velocity'])) < 18.001
            assert row['actions'][0]['Scenario']['payload'][:7] == [0]*7
            assert row['actions'][2]['Scenario']['payload'][1:] == [0,0]
        elif reason == 'transfer deadline': assert tick == a['deadline_tick']
        elif reason == 'transfer exhausted its progress budget': assert tick-a['progress_tick'] > 1200
        elif reason in ['recovery required','ship or surface recovery required']:
            assert not p['ship_available'] or p['ship_form'] != 'ship' or p['vehicle'] != a['vehicle'] or p['location'] == 'on_foot' or row['mission']['goal'] in ['recover','blocked']
        elif reason == 'destination unavailable or secured':
            assert not any(v['index'] == a['destination'] and (v.get('claim') or {}).get('owner') != p['owner'] for v in o['planets'])
        elif reason == 'match rules unavailable': assert not o['match_rules']
        elif reason == 'destination changed': assert row['mission']['target'] != a['destination']
        elif reason == 'no transfer selected': assert a['destination'] is None
        else: raise AssertionError(('unexpected end reason',reason))
    return a


def audit_travel(root,item,enabled):
    report=json.loads((root/'report.json').read_text())
    assert report.get('escape_travel') == (dict(profile=PROFILE,enabled_seats=[i==item['seat'] for i in range(2)]) if enabled else None)
    for seat,m in enumerate(report['missions']):
        assert ('escape_travel' in m) == (enabled and seat==item['seat'])
    sources,attempts,witnesses,guidance={}, {}, [], Counter()
    for row in F.rows(root/'capture-evidence.jsonl'):
        seat,tick=row['seat'],row['pilot']['tick']
        if 'capture_escape' in row:
            escape=row['capture_escape']['telemetry']['last']
            if escape['finished_tick'] == tick: sources[seat,tick]=row
        if 'escape_travel' not in row:
            assert all(actor!=seat or a['finished_tick'] is not None for (actor,_),a in attempts.items()), 'missing active travel observation'
            continue
        assert enabled and seat==item['seat']
        a=row['escape_travel']['telemetry']['last'];key=seat,a['started_tick']
        attempts[key]=audit_step(row,attempts.get(key),sources[key])
        guidance[a['guidance']]+=1;witnesses.append(row)
    final=report['missions'][item['seat']].get('escape_travel')
    counts=dict(attempts=len(attempts),capture_handoffs=sum(a['reason']=='capture task started' for a in attempts.values()),
                timed_out=sum(a['reason']=='transfer deadline' for a in attempts.values()))
    if enabled:
        assert {k:final[k] for k in counts} == counts
        assert final['last'] == (list(attempts.values())[-1] if attempts else None)
    else: assert final is None
    path=root/'escape-travel-witnesses.json';F.D.write(path,dict(schema=1,profile=PROFILE,enabled=enabled,rows=witnesses))
    return dict(**counts,last_attempts=list(attempts.values()),guidance=dict(guidance),
        first_started_tick=min((a['started_tick'] for a in attempts.values()),default=None),witness_sha256=F.E.digest(path))


def analyze_run(entry,old,root,candidate,active,result):
    result.update(C.analyze(root,entry['item']))
    result['initial_cover']=I.audit_initial(root,entry['item'],entry['enabled'])
    result['handoff']=H.audit_handoffs(root,entry['item'],entry['enabled'])
    result['actual_recovery']=E.R.audit_recovery(root,entry['item'],entry['enabled'])
    result['capture_escape']=E.audit_escape(root,entry['item'],entry['enabled'])
    result['escape_travel']=audit_travel(root,entry['item'],active)
    if entry['source']=='health': result['pursuit_health']=I.H.audit_health(root,entry['item'])
    if candidate:
        result['comparison']=C.M.compare(C.P.root_of(old),root)
        difference=result['comparison']['first_control_difference'];first=result['escape_travel']['first_started_tick']
        if first is None:result['unchanged_state']=unchanged_state(old,result)
        if difference:assert first is not None and difference['tick']>=first
        if not active:result['disabled_control_parity']=C.P.replay_parity(old,result)
    else:
        result['replay_parity']=C.P.replay_parity(old,result)
        assert result['initial_cover']==old['initial_cover']
        E.R.A.assert_handoff_parity(result['handoff'],old['handoff'])
        assert result['actual_recovery']==old['actual_recovery']
        assert result['capture_escape']==old['capture_escape']
        if entry['source']=='health':assert result['pursuit_health']==old['pursuit_health']
    result['hashes']={p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    return result


def run(entry,old,binary,out,candidate):
    root=out/(entry['name']+('-travel' if candidate else '-retained'));active=candidate and entry['enabled']
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
        tools={name:F.E.digest(Path(__file__).with_name(name)) for name in set(prior['tools'])|{'validate-capture-escape.py'}},
        plan=I.plan(),retention={},runs={})
    save=lambda:F.D.write(args.out/'summary.json',result);save()
    try:
        for candidate in [False,True]:
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures=[pool.submit(run,e,prior['runs'][e['name']],binary,args.out,candidate) for e in result['plan']]
                for entry,future in zip(result['plan'],futures):
                    new=future.result();(result['runs'] if candidate else result['retention'])[entry['name']]=new
                    save();assert 'error' not in new,(entry['name'],new.get('error'))
                    print(entry['name']+(': travel audited' if candidate else ': retained exactly'),flush=True)
        I.H.verify_inputs(prior)
        assert F.E.digest(args.prior)==result['prior_summary']['sha256'] and F.E.digest(binary)==result['binary_sha256']
        result['complete']=True
    except BaseException as error:result['error']=repr(error);raise
    finally:save()


if __name__=='__main__':main()

