#!/usr/bin/env python3
"""Frozen ownership-based pursuit hull comparison against retained matches."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import importlib.util
from itertools import zip_longest
import json
import math
from pathlib import Path
import subprocess

spec = importlib.util.spec_from_file_location('continuation', Path(__file__).with_name('validate-flight-continuation.py'))
C = importlib.util.module_from_spec(spec)
spec.loader.exec_module(C)
PROFILE = 'discretionary_pursuit_hull_v1'


def command(old, binary, root, enabled):
    result = list(old['command'])
    result[0] = str(binary)
    result[result.index('--out')+1] = str(root)
    if enabled:
        result += ['--pursuit-health-seats',str(old['item']['seat'])]
    return result


def audit_check(row, previous, previous_capture=None):
    evidence, p = row['pursuit_health'], row['pilot']
    h, target = evidence['telemetry'], evidence['target']
    check = h['last']
    assert p['controls_armed'] and p['ship_available'] and p['ship_form'] == 'ship'
    assert p['actor'] is None and isinstance(p['location'],dict) and 'aboard' in p['location']
    assert check['tick'] == p['tick'] and check['opponent'] == target['owner']
    assert target['visible'] and target['ship_form'] == 'ship' and target['health_fraction'] >= 0.5
    assert math.dist(C.M.J.vector(p['ship']['position']),C.M.J.vector(target['motion']['position'])) < 300.0001
    assert any((planet.get('claim') or {}).get('owner') == p['owner'] for planet in row['planets'])
    last_hit = evidence['last_hit_taken_tick']
    assert last_hit is None or p['tick']-last_hit >= 180
    # Evidence is recorded after intent: a declined pursuit can start a new
    # capture on this tick, but cannot interrupt a previously committed one.
    assert previous_capture is None
    if row.get('capture') is not None:
        assert check['decision'] != 'admitted'
        assert row['mission']['goal'] == 'capture'
    own, other = check['own_hull'], check['opponent_hull']
    assert own == p['ship_health'] and other == target['health']
    assert all(x is None or math.isfinite(x) for x in [own,other])
    valid = own is not None and other is not None and own > 0 and other > 0
    expected = ('admitted' if own >= other else 'weaker_hull') if valid else 'invalid_hull'
    assert check['decision'] == expected
    admitted = expected == 'admitted'
    assert h['checks'] == previous['checks']+1
    assert h['admitted'] == previous['admitted']+int(admitted)
    assert h['deferred'] == previous['deferred']+int(not admitted)
    assert h['checks'] == h['admitted']+h['deferred']
    return h


def audit_health(root, item):
    report = json.loads((root/'report.json').read_text())
    assert report['pursuit_health']['profile'] == PROFILE
    assert report['pursuit_health']['enabled_seats'] == [s==item['seat'] for s in range(2)]
    counters = dict(checks=0,admitted=0,deferred=0,last=None)
    checks, first_deferred, previous_capture = [], None, None
    for row in C.F.rows(root/'capture-evidence.jsonl'):
        if row['seat'] != item['seat']:
            assert 'pursuit_health' not in row
            continue
        committed_capture = previous_capture
        previous_capture = row.get('capture')
        if 'pursuit_health' not in row:
            continue
        counters = audit_check(row,counters,committed_capture)
        checks.append(row)
        if counters['last']['decision'] != 'admitted' and first_deferred is None:
            first_deferred = counters['last']['tick']
    for seat, mission in enumerate(report['missions']):
        assert mission.get('pursuit_health') == (counters if seat==item['seat'] else None)
    path = root/'pursuit-health-witnesses.json'
    C.F.D.write(path,dict(schema=1,profile=PROFILE,counters=counters,checks=checks))
    return dict(**counters,first_deferred_tick=first_deferred,
                witness_sha256=C.F.E.digest(path))


def without_gate(value):
    if isinstance(value,dict):
        return {k:without_gate(v) for k,v in value.items() if k!='pursuit_health'}
    if isinstance(value,list):
        return [without_gate(v) for v in value]
    return value


def unchanged_state_audit(old, new):
    roots = [C.P.root_of(r) for r in [old,new]]
    reports = [without_gate(json.loads((r/'report.json').read_text())) for r in roots]
    C.P.M.T.report_parity(*reports,C.P.M.V.EXACT_REPORT_FIELDS+['metrics','policy_configuration','cover_response','destination_retry'])
    assert old['allocation'] == new['allocation']
    streams = C.P.M.V.EXACT_STREAMS+['capture-evidence.jsonl']
    for name in streams:
        for a,b in zip_longest(*(C.F.rows(r/name) for r in roots)):
            assert a is not None and b is not None and without_gate(a) == without_gate(b), name
    return dict(exact_streams_except_gate_telemetry=streams,sensor_parity=C.P.M.C.audit_sensors(*roots),
                exact_allocation=True)


def verify_inputs(prior):
    for run in prior['runs'].values():
        for name,digest in run['hashes'].items():
            assert C.F.E.digest(C.P.root_of(run)/name) == digest


def run(old, binary, out, enabled):
    item = old['item']
    root = out/(item['name']+('-health' if enabled else '-retained'))
    cmd = command(old,binary,root,enabled)
    result = dict(command=cmd,item=item)
    try:
        with (out/(root.name+'.log')).open('x') as log:
            subprocess.run(cmd,check=True,stdout=log,stderr=log,timeout=1800)
        result.update(C.analyze(root,item))
        if enabled:
            result['pursuit_health'] = audit_health(root,item)
            result['hashes']['pursuit-health-witnesses.json'] = result['pursuit_health']['witness_sha256']
            result['comparison'] = C.M.compare(C.P.root_of(old),root)
            difference = result['comparison']['first_control_difference']
            if difference is None:
                result['unchanged_state'] = unchanged_state_audit(old,result)
            else:
                first = result['pursuit_health']['first_deferred_tick']
                assert first is not None and difference['tick'] >= first
        else:
            result['replay_parity'] = C.P.replay_parity(old,result)
    except Exception as error:
        result['error'] = repr(error)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior',type=Path,required=True)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code, tests and plan first'
    prior = json.loads(args.prior.read_text())
    assert prior['complete'] and prior['plan'] == C.P.plan()
    verify_inputs(prior)
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True,exist_ok=False)
    result = dict(schema=1,complete=False,profile=PROFILE,
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=C.F.E.digest(binary),prior_summary=dict(path=str(args.prior),sha256=C.F.E.digest(args.prior)),
        runner_sha256=C.F.E.digest(Path(__file__)),
        tools={name:C.F.E.digest(Path(__file__).with_name(name)) for name in [*prior['tools'],'validate-flight-continuation.py']},
        plan=C.P.plan(),retention={},runs={})
    save = lambda:C.F.D.write(args.out/'summary.json',result)
    save()
    try:
        for enabled in [False,True]:
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures = [pool.submit(run,prior['runs'][p['name']],binary,args.out,enabled) for p in result['plan']]
                for item,future in zip(result['plan'],futures):
                    new = future.result()
                    (result['runs'] if enabled else result['retention'])[item['name']] = new
                    save()
                    assert 'error' not in new, (item['name'],new.get('error'))
                    print(item['name']+(': health audited' if enabled else ': retained exactly'),flush=True)
        verify_inputs(prior)
        assert C.F.E.digest(args.prior) == result['prior_summary']['sha256']
        assert C.F.E.digest(binary) == result['binary_sha256']
        result['complete'] = True
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
