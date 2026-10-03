#!/usr/bin/env python3
"""Frozen diagnosis of the post-capture pod impact; no policy or physics tuning."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
from itertools import zip_longest
import json
import math
from pathlib import Path
import struct
import subprocess

spec = importlib.util.spec_from_file_location('continuation', Path(__file__).with_name('validate-flight-continuation.py'))
C = importlib.util.module_from_spec(spec)
spec.loader.exec_module(C)
NAME = 'shared-armed-world1-p1-powered'
START, CONTROL_FROM, END = 13800, 15406, 36001


def digest(path):
    with path.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def write(path, value):
    with path.open('x') as f:
        f.write(json.dumps(value, indent=2, allow_nan=False) + '\n')


def rows(path):
    with path.open() as f:
        for line in f:
            yield json.loads(line)


def xy(v):
    result = v['x'], v['y']
    assert all(math.isfinite(x) for x in result)
    return result


def minus(a, b):
    return tuple(x-y for x,y in zip(a,b))


def dot(a, b):
    return sum(x*y for x,y in zip(a,b))


def motion_metrics(row):
    """Instantaneous geometry, not a promise about future gravity or contacts."""
    m = row['motion']
    if m is None:
        return None
    position, velocity = xy(m['center_of_mass']), xy(m['velocity'])
    offset = minus(position, xy(m['boundary_center']))
    radius, distance, speed = m['boundary_radius'], math.hypot(*offset), math.hypot(*velocity)
    assert math.isfinite(radius) and radius > 0
    projection = dot(offset, velocity)
    outward = projection/distance if distance else 0.0
    ray = None
    if speed > 0 and distance <= radius:
        # Positive ray/circle intersection. The actor is a point for this diagnostic.
        ray = (-projection + math.sqrt(max(0, projection**2 + speed**2*(radius**2-distance**2))))/speed
    planet = row['planet']['motion']
    delta = minus(position, xy(planet['position']))
    spin = planet['spin']
    assert math.isfinite(spin) and math.isfinite(m['spin'])
    frame_velocity = (planet['velocity']['x']-delta[1]*spin, planet['velocity']['y']+delta[0]*spin)
    limits = row['flight']['limits']
    brake, turn = limits['brake_acceleration'], limits['turn_acceleration']
    assert math.isfinite(brake) and brake > 0 and math.isfinite(turn) and turn > 0
    return dict(speed=speed, relative_speed=math.hypot(*minus(velocity, frame_velocity)),
        frame_speed=math.hypot(*frame_velocity), spin=m['spin'],
        origin_velocity_difference=math.hypot(*minus(xy(row['ship']['velocity']), velocity)),
        radial_clearance=radius-distance, outward_speed=outward,
        distance_to_wall_along_velocity=ray,
        ideal_braking_distance=speed**2/(2*brake),
        ideal_outward_braking_distance=max(0,outward)**2/(2*brake),
        spin_arrest_seconds=max(0,abs(m['spin']-spin)-0.5)/turn)


def audit_impact(root, mode, seat):
    data = list(rows(root/'impact.jsonl'))
    final = data.pop()
    assert final['schema'] == 1 and 'final_tick' in final
    assert final['config'] == dict(seat=seat, control=mode, control_from_tick=CONTROL_FROM,
                                 trace_start_tick=START, trace_end_tick=END)
    by_key = {(r['seat'],r['tick']):r for r in data}
    assert len(by_key) == len(data)
    end = min(END,final['final_tick'])
    assert set(by_key) == {(s,t) for s in range(2) for t in range(START,end)}
    witnesses, selected, previous, overrides = [], [], {}, 0
    for row in rows(root/'capture-evidence.jsonl'):
        p = row['pilot']
        key = row['seat'], p['tick']
        if key not in by_key:
            continue
        r = by_key[key]
        assert r['schema'] == 1
        assert r['ship'] == p['ship'] and r['form'] == p['ship_form'] and r['location'] == p['location']
        assert r['actions'] == row['actions'] and r['goal'] == row['mission']['goal']
        expected = mode != 'bot' and row['seat'] == seat and p['tick'] >= CONTROL_FROM and p['ship_form'] == 'escape_pod' and p['actor'] is None
        assert r['overridden'] == expected
        controls = r['controls']
        if expected:
            assert controls == dict(turn=0.0, thrust=False, brake=mode=='brake', interact=False)
            overrides += 1
        else:
            assert controls == r['bot_controls']
        action = r['actions'][0]['Scenario']
        assert action['kind'] == 0x53550002
        assert action['payload'] == list(struct.pack('<f', controls['turn'])) + [int(controls['thrust']),int(controls['interact']),int(controls['brake']),row['seat']]
        if row['seat'] != seat:
            continue
        metrics = motion_metrics(r)
        old = previous.get(seat)
        jump = math.hypot(*minus(xy(r['motion']['velocity']), xy(old['motion']['velocity']))) if old and r['motion'] and old['motion'] else None
        event = (old is not None and (r['form'] != old['form'] or r['damage']['debris_contacts'] != old['damage']['debris_contacts']))
        sample = dict(tick=p['tick'], metrics=metrics, velocity_change=jump, controls=controls,
                      form=r['form'], health=r['vitals']['health'], damage=r['damage'],
                      recovery=r['recovery'])
        selected.append(sample)
        if event or p['tick'] in [START,13941,13942,14090,14091,14880,14900,14901,15405,15406,end-1]:
            witnesses.append(dict(**sample, observation=r))
        previous[seat] = r
    assert len(selected) == end-START
    return dict(rows=len(data), overridden_ticks=overrides,
        post_hit_ticks=sum(r['tick']>=14091 for r in selected),
        post_hit_brake_ticks=sum(r['tick']>=14091 and r['controls']['brake'] for r in selected),
        post_hit_thrust_ticks=sum(r['tick']>=14091 and r['controls']['thrust'] for r in selected),
        final=final, witnesses=witnesses, samples=selected)


def compare_prefix(old, new, boundary):
    matched = 0
    left, right = rows(old/'capture-evidence.jsonl'), rows(new/'capture-evidence.jsonl')
    for a,b in zip(left,right):
        if a['pilot']['tick'] >= boundary:
            break
        assert a == b
        matched += 1
    assert matched == boundary*2
    return matched


def physical_parity(old, new):
    """Exact per-tick observed state and telemetry, with only actions removed."""
    a = rows(old/'capture-evidence.jsonl')
    b = rows(new/'capture-evidence.jsonl')
    for x,y in zip_longest(a,b):
        if x is None or y is None:
            return False
        x.pop('actions'); y.pop('actions')
        if x != y:
            return False
    return True


def command(prior, binary, root, mode):
    result = list(prior['command'])
    result[0] = str(binary)
    result[result.index('--out')+1] = str(root)
    if mode != 'disabled':
        result += ['--trace-impact','true','--impact-start-tick',str(START),'--impact-end-tick',str(END),
                   '--impact-pod-control',mode,'--impact-control-from-tick',str(CONTROL_FROM)]
    return result


def run(prior, binary, out, mode):
    root = out/mode
    cmd = command(prior,binary,root,mode)
    with (out/(mode+'.log')).open('x') as log:
        subprocess.run(cmd,check=True,stdout=log,stderr=log,timeout=1800)
    result = dict(command=cmd,item=prior['item'],**C.analyze(root,prior['item']))
    if mode in ['disabled','bot']:
        result['replay_parity'] = C.P.replay_parity(prior,result)
    else:
        result['matching_prefix_rows'] = compare_prefix(C.P.root_of(prior),root,CONTROL_FROM)
        result['physical_parity'] = physical_parity(C.P.root_of(prior),root)
        result['comparison'] = C.M.compare(C.P.root_of(prior),root)
    if mode != 'disabled':
        evidence = audit_impact(root,mode,prior['item']['seat'])
        write(root/'impact-analysis.json',evidence)
        result['impact'] = {k:v for k,v in evidence.items() if k not in ['samples','witnesses']}
        result['hashes']['impact.jsonl'] = digest(root/'impact.jsonl')
        result['hashes']['impact-analysis.json'] = digest(root/'impact-analysis.json')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior',type=Path,required=True)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code and plan first'
    s = json.loads(args.prior.read_text())
    assert s['complete']
    prior = s['runs'][NAME]
    for name,h in prior['hashes'].items():
        assert digest(C.P.root_of(prior)/name) == h
    args.out.mkdir(parents=True,exist_ok=False)
    binary = args.binary.resolve(strict=True)
    result = dict(schema=1,complete=False,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=digest(binary),runner_sha256=digest(Path(__file__)),
        prior_summary=dict(path=str(args.prior),sha256=digest(args.prior)),
        plan=dict(name=NAME,trace_start=START,control_from=CONTROL_FROM,trace_end=END,
                  modes=['disabled','bot','brake','coast']),runs={})
    try:
        with ThreadPoolExecutor(max_workers=2) as pool:
            futures = {m:pool.submit(run,prior,binary,args.out,m) for m in result['plan']['modes']}
            for mode,future in futures.items():
                result['runs'][mode] = future.result()
                print(mode,'audited',flush=True)
        assert digest(args.prior) == result['prior_summary']['sha256']
        assert digest(binary) == result['binary_sha256']
        for name,h in prior['hashes'].items():
            assert digest(C.P.root_of(prior)/name) == h
        result['complete'] = True
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        write(args.out/'summary.json',result)


if __name__ == '__main__':
    main()
