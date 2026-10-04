#!/usr/bin/env python3
"""Frozen actual-hatch local-failure response against covered-handoff matches."""
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

spec = importlib.util.spec_from_file_location('actual', Path(__file__).with_name('probe-actual-landing.py'))
A = importlib.util.module_from_spec(spec); spec.loader.exec_module(A)
H, I, C, F = A.H, A.I, A.C, A.F
PROFILE = 'actual_local_failure_abort_v1'
REASON = 'actual hatch local attempt exhausted'


def f32(value):
    return struct.unpack('f', struct.pack('f', value))[0]


def native_local(p, point):
    x,y = map(f32, I.A.J.vector(point)); cx,cy = map(f32, I.A.J.vector(p['planet']['motion']['position']))
    x,y = f32(x-cx),f32(y-cy); angle = -f32(p['planet']['motion']['angle'])
    co,si = f32(math.cos(angle)),f32(math.sin(angle))
    return f32(f32(x*co)-f32(y*si)),f32(f32(x*si)+f32(y*co))


def command(old, binary, root, enabled):
    cmd = list(old['command']); assert '--actual-route-recovery-seats' not in cmd
    cmd[0] = str(binary); cmd[cmd.index('--out')+1] = str(root)
    if enabled: cmd += ['--actual-route-recovery-seats', str(old['item']['seat'])]
    return cmd


def without_option(value):
    if isinstance(value, dict):
        return {k:without_option(v) for k,v in value.items()
                if k not in ['actual_route_recovery', 'actual_local_failure', 'actual_failure_feedback']}
    if isinstance(value, list): return [without_option(v) for v in value]
    return value


def unchanged_state(old, new):
    roots = [C.P.root_of(r) for r in [old, new]]
    reports = [without_option(json.loads((r/'report.json').read_text())) for r in roots]
    C.P.M.T.report_parity(*reports, C.P.M.V.EXACT_REPORT_FIELDS +
        ['metrics','policy_configuration','cover_response','destination_retry','pursuit_health'])
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
    assert [{k:v for k,v in r['live_objective_planning']['telemetry'].items() if k not in ignored}
            for r in reports][0] == {k:v for k,v in reports[1]['live_objective_planning']['telemetry'].items() if k not in ignored}
    return dict(exact_streams_except_option_telemetry=streams,
        sensor_parity=C.P.M.C.audit_sensors(*roots), exact_allocation_ledger=True)


def audit_receipt(row, source):
    p, e = row['pilot'], row['objective_evidence']; a = e['actual_local_failure']; tick = p['tick']
    assert e['tick'] == tick and e['generation'] is not None
    assert e['request_tick'] == e['measurement_tick'] == source['pilot']['tick'] <= tick <= e['measurement_tick']+120
    assert e['invalidated_by'] is None and e['submission_deferred_by'] is None
    assert row['objective_work'] == 'pending' and row['landing_objective'] is None
    assert a['actor'] == p['owner'] and a['reason']
    assert p['queries_ready'] and p['ship_available'] and p['ship_form'] == 'ship'
    assert isinstance(p['location'], dict) and 'aboard' in p['location']
    assert p['landing']['phase'] == 'landed' and source['pilot']['landing']['phase'] == 'landed'
    for obj in [e['source_objective'], source['objective_evidence']['objective']]:
        assert all(obj[k] == e['objective'][k] for k in ['planet','revision','owner'])
        assert math.dist(I.A.J.vector(obj['position']), I.A.J.vector(e['objective']['position'])) <= .00201
        assert abs(obj['range'] - e['objective']['range']) <= .00011
    pose = a['pose']
    for pilot, tolerance in [(source['pilot'], .00005), (p, .00205)]:
        for key, value in [('vehicle', pilot['ship']['position']), ('exit', pilot['hatch'])]:
            assert math.dist(I.A.J.vector(pose[key]), native_local(pilot, value)) <= tolerance
        for old, new in zip(pose['boarding_hatches'], pilot['boarding_hatches']):
            assert (old is None) == (new is None)
            if old is not None: assert math.dist(I.A.J.vector(old), native_local(pilot,new)) <= tolerance
    source_p = source['pilot']
    assert abs(pose['angle']-f32(f32(source_p['ship']['angle'])-f32(source_p['planet']['motion']['angle']))) < .00001
    assert A.angular_change(source['pilot'], p) <= .0001001
    return a


def audit_abort(row):
    local, p = I.witness_observation(dict(row, initial_cover=row['actual_route_recovery']))
    cap, e, tick = row['capture'], row['objective_evidence'], p['tick']
    abort = cap['actual_route_recovery']['abort']
    assert p['controls_armed'] and p['transfer'] == 'ready'
    assert local['combat']['recovery']['flight']['flight']['enabled']
    assert cap['failed_tick'] == abort['tick'] == tick and cap['failure'] == REASON and cap['goal'] == 'blocked'
    assert cap['acquisition']['reason'] == 'actual_local_attempt_failed'
    assert cap['completed_tick'] is None and cap['landing']['claimed_tick'] is None
    assert abort['attempt'] == e['actual_local_failure'] and abort['objective'] == e['objective']
    assert all(abort[k] == e[k] for k in ['generation','request_tick','measurement_tick'])
    # The abort transition is neutral; it cannot press exit or fire a jetpack.
    controls = row['actions'][0]['Scenario']['payload']
    assert controls[:7] == [0]*7
    return abort


def audit_recovery(root, item, enabled):
    report = json.loads((root/'report.json').read_text())
    assert report.get('actual_route_recovery') == (dict(profile=PROFILE,
        enabled_seats=[i == item['seat'] for i in range(2)]) if enabled else None)
    assert report['live_objective_planning'].get('actual_failure_feedback') == (
        dict(enabled_seats=[item['seat']]) if enabled else None)
    recent, receipts, aborts, witnesses, counts = {}, {}, {}, [], Counter()
    first_charge = {}
    with (root/'live-planning.csv').open() as f:
        for row in csv.DictReader(f):
            if row['task'] == 'landing_objective' and (int(row['graph']) or int(row['queries'])):
                first_charge.setdefault((int(row['actor']),int(row['generation'])),int(row['tick']))
    for row in F.rows(root/'capture-evidence.jsonl'):
        p = row['pilot']; tick, seat = p['tick'], row['seat']
        recent.setdefault(seat, {})[tick] = row; recent[seat].pop(tick-122, None)
        e = row.get('objective_evidence') or {}; attempt = e.get('actual_local_failure')
        cap = row.get('capture') or {}; recovery = cap.get('actual_route_recovery')
        assert (recovery is not None) == (bool(cap) and enabled and seat == item['seat'])
        if attempt:
            assert enabled and seat == item['seat']
            source = recent[seat][e['request_tick']]; audit_receipt(row, source)
            key = seat, e['generation']
            assert e['request_tick'] <= first_charge[key] < tick
            if key not in receipts:
                receipts[key] = dict(seat=seat, generation=e['generation'], first_tick=tick,
                    request_tick=e['request_tick'], attempt=attempt)
                witnesses.append(dict(kind='first_receipt', source=source, row=row))
                counts[attempt['reason']] += 1
            assert receipts[key]['attempt'] == attempt
        abort = (recovery or {}).get('abort')
        event = abort is not None and abort['tick'] == tick
        assert ('actual_route_recovery' in row) == event
        if event:
            audit_abort(row); key = seat, cap['started_tick']
            assert key not in aborts
            aborts[key] = dict(seat=seat, **abort)
            witnesses.append(dict(kind='abort', row=row))
    path = root/'actual-recovery-witnesses.json'
    F.D.write(path, dict(schema=1, profile=PROFILE, receipts=list(receipts.values()),
        aborts=list(aborts.values()), rows=witnesses))
    return dict(receipts=len(receipts), reasons=dict(counts), aborts=list(aborts.values()),
        first_abort_tick=min((a['tick'] for a in aborts.values()), default=None), witness_sha256=F.E.digest(path))


def run(entry, old, binary, out, candidate):
    root = out/(entry['name']+('-recovery' if candidate else '-retained'))
    active = candidate and entry['enabled']; cmd = command(old,binary,root,active)
    result = dict(item=old['item'], command=cmd)
    try:
        with (out/(root.name+'.log')).open('x') as log:
            subprocess.run(cmd, check=True, stdout=log, stderr=log, timeout=1800)
        result.update(C.analyze(root,entry['item']))
        result['initial_cover'] = I.audit_initial(root,entry['item'],entry['enabled'])
        result['handoff'] = H.audit_handoffs(root,entry['item'],entry['enabled'])
        result['actual_recovery'] = audit_recovery(root,entry['item'],active)
        if entry['source'] == 'health': result['pursuit_health'] = I.H.audit_health(root,entry['item'])
        if candidate:
            result['comparison'] = C.M.compare(C.P.root_of(old),root)
            difference = result['comparison']['first_control_difference']
            first = result['actual_recovery']['first_abort_tick']
            if first is None: result['unchanged_state'] = unchanged_state(old,result)
            if difference: assert first is not None and difference['tick'] >= first
            if not active: result['disabled_control_parity'] = C.P.replay_parity(old,result)
        else:
            result['replay_parity'] = C.P.replay_parity(old,result)
            assert result['initial_cover'] == old['initial_cover']
            A.assert_handoff_parity(result['handoff'],old['handoff'])
            if entry['source'] == 'health': assert result['pursuit_health'] == old['pursuit_health']
        result['hashes'] = {p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
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
        tools={name:F.E.digest(Path(__file__).with_name(name)) for name in
            set(prior['tools'])|{'validate-covered-handoff.py','probe-actual-landing.py'}},
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
                    print(entry['name']+(': recovery audited' if candidate else ': retained exactly'),flush=True)
        I.H.verify_inputs(prior)
        assert F.E.digest(args.prior) == result['prior_summary']['sha256'] and F.E.digest(binary) == result['binary_sha256']
        result['complete'] = True
    except BaseException as error: result['error'] = repr(error); raise
    finally: save()


if __name__ == '__main__': main()
