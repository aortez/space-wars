#!/usr/bin/env python3
"""Frozen physical escape after an actual-hatch abort, on retained full matches."""
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
import struct
import traceback

spec = importlib.util.spec_from_file_location('recovery', Path(__file__).with_name('validate-actual-recovery.py'))
R = importlib.util.module_from_spec(spec); spec.loader.exec_module(R)
I, H, C, F = R.I, R.H, R.C, R.F
PROFILE = 'actual_capture_escape_v1'
vector = I.A.J.vector


def command(old, binary, root, enabled):
    cmd = list(old['command']); assert '--capture-escape-seats' not in cmd
    cmd[0] = str(binary); cmd[cmd.index('--out')+1] = str(root)
    if enabled:
        seat = str(old['item']['seat'])
        assert cmd[cmd.index('--actual-route-recovery-seats')+1] in [seat, 'both']
        cmd += ['--capture-escape-seats', seat]
    return cmd


def without_option(value):
    if isinstance(value, dict):
        return {k:without_option(v) for k,v in value.items() if k != 'capture_escape'}
    if isinstance(value, list): return [without_option(v) for v in value]
    return value


def unchanged_state(old, new):
    roots = [C.P.root_of(r) for r in [old, new]]
    reports = [without_option(json.loads((r/'report.json').read_text())) for r in roots]
    C.P.M.T.report_parity(*reports, C.P.M.V.EXACT_REPORT_FIELDS +
        ['metrics','policy_configuration','cover_response','destination_retry','pursuit_health','actual_route_recovery'])
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


def subtract(a, b): return tuple(x-y for x,y in zip(vector(a),vector(b)))
def dot(a, b): return sum(x*y for x,y in zip(a,b))
def unit(a):
    length = math.hypot(*a)
    return tuple(x/length for x in a) if length > 1e-8 else (0.,0.)


def stopping_clearance(o, p):
    boundary = o['boundary']; delta = subtract(p['ship']['position'],boundary['center'])
    outward = unit(delta); closing = max(0.,dot(vector(p['ship']['velocity']),outward))
    limits = o['local']['combat']['recovery']['flight']['flight']['limits']
    deceleration = max(5.,limits['brake_acceleration']*.5-max(0.,dot(vector(p['gravity']),outward)))
    return boundary['radius']-math.hypot(*delta)-20-closing*.6-closing*closing/(2*deceleration)


def separation(o, p, attempt):
    planet = next(v for v in o['planets'] if v['index'] == attempt['abort']['objective']['planet'])
    clearance = math.dist(vector(p['ship']['position']),vector(planet['motion']['position']))-planet['radius']
    c = o['local']['combat']; target = c['target']
    if not (target and target['owner'] == attempt['opponent'] and target['ship_form'] == 'ship' and target['health'] > 0):
        target = None
    distance = opening = None
    if target:
        delta = subtract(p['ship']['position'],target['motion']['position'])
        distance = math.hypot(*delta)
        opening = dot(subtract(p['ship']['velocity'],target['motion']['velocity']),unit(delta))
    clear = bool(p['controls_armed'] and p['queries_ready'] and c['recovery']['flight']['flight']['enabled']
        and p['landing']['supported_feet'] == 0 and clearance > 70 and stopping_clearance(o,p) > 20
        and target and (target['ground_occluded'] or distance >= 350 and opening >= 0))
    return dict(source_clearance=clearance, range=distance, opening_speed=opening), clear


def audit_weapons(row, local, p):
    c = local['combat']; laser,cannon = row['actions'][2]['Scenario']['payload'][1:]
    assert laser in [0,1] and cannon in [0,1]
    if laser or cannon:
        target = c['target']; assert target and target['visible'] and not target['ground_occluded']
        assert row['capture_escape']['combat']['goal'] == 'engage ship'
        delta = subtract(target['motion']['position'],p['ship']['position'])
        distance = math.hypot(*delta); lead = min(1.,distance/300.)
        aim = tuple(x+v*lead for x,v in zip(delta,subtract(target['motion']['velocity'],p['ship']['velocity'])))
        error = (math.atan2(-aim[0],aim[1])-p['ship']['angle']+math.pi)%(2*math.pi)-math.pi
        assert abs(error) < .08001
        assert not laser or c['laser_available'] and distance <= 250.001
        assert not cannon or c['cannon_ready'] and 19.999 <= distance <= 220.001
    return laser,cannon


def flight_controls(row):
    # SurfaceSortieAction::encode: float turn, thrust, interact, brake, seat.
    payload = row['actions'][0]['Scenario']['payload']
    assert len(payload) == 8 and all(v in [0,1] for v in payload[4:7])
    return dict(horizontal=struct.unpack('<f',bytes(payload[:4]))[0],
        thrust=bool(payload[4]),interact=bool(payload[5]),brake=bool(payload[6]),seat=payload[7])


def audit_step(row, previous, abort_row):
    local,p = I.witness_observation(dict(row,initial_cover=row['capture_escape']))
    w = row['capture_escape']; a = w['telemetry']['last']; o = w['observation']; tick = p['tick']
    assert a['observed_tick'] == tick and a['started_tick'] == a['abort']['tick']+1
    assert a['deadline_tick'] == a['abort']['tick']+720
    assert a['abort'] == abort_row['capture']['actual_route_recovery']['abort']
    assert a['abort']['attempt']['actor'] == p['owner']
    payload = row['actions'][0]['Scenario']['payload']; controls = flight_controls(row)
    if previous is None:
        assert tick == a['started_tick'] and a['guidance'] == 'arming' and a['finished_tick'] is None
        assert p['vehicle'] == a['vehicle'] and p['controls_armed'] and p['queries_ready']
        assert p['ship_available'] and p['ship_form'] == 'ship' and isinstance(p['location'],dict) and 'aboard' in p['location']
        assert o['match_rules'] and local['combat']['recovery']['flight']['flight']['enabled']
        assert p['landing']['phase'] == 'landed' and not row['capture']
        target = local['combat']['target']
        assert target and target['owner'] == a['opponent'] and target['ship_form'] == 'ship' and target['health'] > 0
        assert target['visible'] and not target['ground_occluded']
        assert math.dist(vector(p['ship']['position']),vector(target['motion']['position'])) < 350
        for key,value in [('vehicle',p['ship']['position']),('exit',p['hatch'])]:
            assert math.dist(vector(a['abort']['attempt']['pose'][key]),R.native_local(p,value)) <= .00205
        for old,new in zip(a['abort']['attempt']['pose']['boarding_hatches'],p['boarding_hatches']):
            assert (old is None) == (new is None)
            if old is not None: assert math.dist(vector(old),R.native_local(p,new)) <= .00205
        assert R.A.angular_change(abort_row['pilot'],p) <= .0001001
        objective = a['abort']['objective']; claim = p['planet']['claim']
        assert (objective['planet'],objective['revision']) == (p['planet']['index'],p['planet']['revision'])
        assert claim['owner'] != p['owner'] and claim['flag']['player'] == objective['owner']
        assert math.dist(vector(objective['position']),R.native_local(p,claim['flag']['position'])) < .50001
        assert abs(objective['range']-(claim['flag_interaction_range']-.2)) < .01001
        assert abs(math.hypot(*vector(a['direction']))-1.) < .00001
        assert payload[:7] == [0]*7 and a['clear_since'] is None
        assert [a[k] for k in ['controlled_ticks','laser_ticks','cannon_ticks']] == [0,0,0]
    else:
        assert previous['finished_tick'] is None and tick == previous['observed_tick']+1
        for k in ['abort','started_tick','deadline_tick','vehicle','opponent','direction','estimated_min_range','estimated_clearance']:
            assert a[k] == previous[k], k
        if a['reason'] not in ['recovery required','surface task takes priority','escape deadline','source planet unavailable']:
            measured,clear = separation(o,p,a)
            for k,value in measured.items():
                assert (value is None) == (a[k] is None), k
                if value is not None: assert abs(value-a[k]) <= .003, (k,value,a[k])
            since = (previous['clear_since'] if previous['clear_since'] is not None else tick) if clear else None
            assert a['clear_since'] == since
            assert (a['reason'] == 'separation established') == (since is not None and tick-since >= 60)
        controlled = a['guidance'] in ['lift','climb','escape','boundary']
        if controlled:
            assert tick < a['deadline_tick'] and a['finished_tick'] is None
            assert row['mission']['goal'] == 'disengage' and not row['capture']
            assert p['vehicle'] == a['vehicle'] and p['ship_available'] and p['ship_form'] == 'ship'
            assert p['controls_armed'] and p['queries_ready'] and local['combat']['recovery']['flight']['flight']['enabled']
            assert isinstance(p['location'],dict) and 'aboard' in p['location']
            assert not controls['interact']
            if a['guidance'] == 'lift': assert p['landing']['supported_feet'] > 0
            if a['guidance'] == 'boundary': assert a['boundary']['active'] and controls['brake']
            if a['guidance'] in ['boundary','lift']: assert row['actions'][1]['Scenario']['payload'][1] == 0
            laser,cannon = audit_weapons(row,local,p)
        else:
            laser = cannon = 0
            if a['guidance'] in ['queries_unavailable','flight_disabled']:
                assert payload[:7] == [0]*7 and row['actions'][2]['Scenario']['payload'][1:] == [0,0]
        for k,increment in [('controlled_ticks',controlled),('laser_ticks',laser),('cannon_ticks',cannon)]:
            assert a[k] == previous[k]+int(increment), k
    if a['finished_tick'] is None:
        assert a['reason'] is None and tick < a['deadline_tick'] and w['pursuit'] is None
    else:
        assert a['finished_tick'] == tick and a['guidance'] == 'finished'
        if a['reason'] == 'escape deadline': assert tick == a['deadline_tick']
        elif a['reason'] == 'recovery required':
            assert not p['ship_available'] or p['ship_form'] != 'ship' or p['vehicle'] != a['vehicle'] or p['location'] == 'on_foot' or row['mission']['goal'] in ['recover','blocked']
        elif a['reason'] == 'surface task takes priority': assert not o['match_rules'] or row['capture']
        elif a['reason'] == 'source planet unavailable': assert not any(v['index'] == a['abort']['objective']['planet'] for v in o['planets'])
        else: assert a['reason'] == 'separation established'
    return a


def audit_escape(root, item, enabled):
    report = json.loads((root/'report.json').read_text())
    assert report.get('capture_escape') == (dict(profile=PROFILE,
        enabled_seats=[i == item['seat'] for i in range(2)]) if enabled else None)
    for seat,mission in enumerate(report['missions']):
        assert ('capture_escape' in mission) == (enabled and seat == item['seat'])
    aborts, attempts, witnesses, guidance = {}, {}, [], Counter()
    for row in F.rows(root/'capture-evidence.jsonl'):
        seat,tick = row['seat'],row['pilot']['tick']
        if 'actual_route_recovery' in row: aborts[seat,tick] = row
        if 'capture_escape' not in row:
            for (actor,_),previous in attempts.items():
                assert actor != seat or previous['finished_tick'] is not None, 'missing active observation'
            continue
        assert enabled and seat == item['seat']
        a = row['capture_escape']['telemetry']['last']; key = seat,a['started_tick']
        attempts[key] = audit_step(row,attempts.get(key),aborts[seat,a['abort']['tick']])
        guidance[a['guidance']] += 1; witnesses.append(row)
    final = report['missions'][item['seat']].get('capture_escape')
    expected = dict(attempts=len(attempts),separated=sum(a['reason']=='separation established' for a in attempts.values()),
        timed_out=sum(a['reason']=='escape deadline' for a in attempts.values()))
    if enabled:
        assert {k:final[k] for k in expected} == expected
        assert final['last'] == (list(attempts.values())[-1] if attempts else None)
    else: assert final is None
    for row in F.rows(root/'trace.jsonl'):
        mission = row['mission']; escape = mission.get('capture_escape')
        if escape and escape['last']:
            attempt = escape['last']
            if attempt['started_tick'] <= row['tick'] < attempt['deadline_tick']:
                assert mission['pursuit'] is None, 'new pursuit before the original deadline'
    path = root/'capture-escape-witnesses.json'
    F.D.write(path,dict(schema=1,profile=PROFILE,enabled=enabled,rows=witnesses))
    return dict(**expected, last_attempts=list(attempts.values()),guidance=dict(guidance),
        first_started_tick=min((a['started_tick'] for a in attempts.values()),default=None),witness_sha256=F.E.digest(path))


def analyze_run(entry, old, root, candidate, active, result):
    result.update(C.analyze(root,entry['item']))
    result['initial_cover'] = I.audit_initial(root,entry['item'],entry['enabled'])
    result['handoff'] = H.audit_handoffs(root,entry['item'],entry['enabled'])
    result['actual_recovery'] = R.audit_recovery(root,entry['item'],entry['enabled'])
    result['capture_escape'] = audit_escape(root,entry['item'],active)
    if entry['source'] == 'health': result['pursuit_health'] = I.H.audit_health(root,entry['item'])
    if candidate:
        result['comparison'] = C.M.compare(C.P.root_of(old),root)
        difference = result['comparison']['first_control_difference']
        first = result['capture_escape']['first_started_tick']
        if first is None: result['unchanged_state'] = unchanged_state(old,result)
        if difference: assert first is not None and difference['tick'] > first
        if not active: result['disabled_control_parity'] = C.P.replay_parity(old,result)
    else:
        result['replay_parity'] = C.P.replay_parity(old,result)
        assert result['initial_cover'] == old['initial_cover']
        R.A.assert_handoff_parity(result['handoff'],old['handoff'])
        assert result['actual_recovery'] == old['actual_recovery']
        if entry['source'] == 'health': assert result['pursuit_health'] == old['pursuit_health']
    result['hashes'] = {p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    return result


def run(entry, old, binary, out, candidate):
    root = out/(entry['name']+('-escape' if candidate else '-retained'))
    active = candidate and entry['enabled']; cmd = command(old,binary,root,active)
    result = dict(item=old['item'],command=cmd)
    try:
        with (out/(root.name+'.log')).open('x') as log:
            subprocess.run(cmd,check=True,stdout=log,stderr=log,timeout=1800)
        analyze_run(entry,old,root,candidate,active,result)
    except Exception: result['error'] = traceback.format_exc()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior',type=Path,required=True)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code, tests and plan first'
    prior = json.loads(args.prior.read_text()); assert prior['complete'] and prior['plan'] == I.plan()
    I.H.verify_inputs(prior)
    binary = args.binary.resolve(strict=True); args.out.mkdir(parents=True,exist_ok=False)
    result = dict(schema=1,complete=False,profile=PROFILE,
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.E.digest(binary),prior_summary=dict(path=str(args.prior),sha256=F.E.digest(args.prior)),
        runner_sha256=F.E.digest(Path(__file__)),
        tools={name:F.E.digest(Path(__file__).with_name(name)) for name in set(prior['tools'])|{'validate-actual-recovery.py'}},
        plan=I.plan(),retention={},runs={})
    save = lambda:F.D.write(args.out/'summary.json',result)
    save()
    try:
        for candidate in [False,True]:
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures = [pool.submit(run,e,prior['runs'][e['name']],binary,args.out,candidate) for e in result['plan']]
                for entry,future in zip(result['plan'],futures):
                    new = future.result(); (result['runs'] if candidate else result['retention'])[entry['name']] = new
                    save(); assert 'error' not in new, (entry['name'],new.get('error'))
                    print(entry['name']+(': escape audited' if candidate else ': retained exactly'),flush=True)
        I.H.verify_inputs(prior)
        assert F.E.digest(args.prior) == result['prior_summary']['sha256'] and F.E.digest(binary) == result['binary_sha256']
        result['complete'] = True
    except BaseException as error: result['error'] = repr(error); raise
    finally: save()


if __name__ == '__main__': main()
