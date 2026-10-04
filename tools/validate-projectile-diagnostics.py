#!/usr/bin/env python3
"""Replay four retained transfer cases with read-only projectile diagnostics."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
import math
from pathlib import Path
import subprocess
import traceback


spec = importlib.util.spec_from_file_location('speed', Path(__file__).with_name('validate-transfer-speed.py'))
S = importlib.util.module_from_spec(spec)
spec.loader.exec_module(S)
rows, root_of = S.F.rows, S.C.P.root_of


def digest(path):
    return S.F.E.digest(Path(path))


def command(old, binary, root, enabled):
    cmd = list(old['command'])
    assert '--trace-projectiles' not in cmd
    cmd[0] = str(binary)
    cmd[cmd.index('--out') + 1] = str(root)
    return cmd + (['--trace-projectiles', 'true'] if enabled else [])


def vector(v):
    return v['x'], v['y']


def near(a, b):
    assert math.dist(vector(a), vector(b)) < .001, (a, b)


def audit_sample(row, pilot):
    assert row['schema'] == 1 and row['tick'] == pilot['tick']
    d = row['diagnostic']
    if d is None:
        assert not isinstance(pilot['location'], dict) or not pilot['ship_available']
        return
    assert d['version'] == 1 and d['tick'] == row['tick']
    assert d['actor'] == pilot['owner'] and d['vehicle'] == pilot['vehicle']
    assert d['ship_form'] == pilot['ship_form']
    assert d['range'] == 600. and d['capacity'] == 64 and d['observer_radius'] > 0.
    for key in ('position', 'velocity'):
        near(d['observer'][key], pilot['ship'][key])
    for key in ('angle', 'spin'):
        assert d['observer'][key] == pilot['ship'][key]
    shells = d['projectiles']
    assert len(shells) == min(d['shells_in_range'], 64)
    assert 0 <= d['unavailable_shells'] <= d['debris_scanned']
    assert d['shells_in_range'] + d['unavailable_shells'] <= d['debris_scanned']
    ids = set()
    previous = None
    for p in shells:
        assert p['id'] not in ids and p['id'] > 0 and 0 <= p['spawn_tick'] <= row['tick']
        ids.add(p['id'])
        assert p['radius'] > 0. and p['collision_radius'] > 0.
        for field, motion in [('relative_position', 'position'), ('relative_velocity', 'velocity')]:
            expected = {k: p['motion'][motion][k] - d['observer'][motion][k] for k in ('x', 'y')}
            near(p[field], expected)
        distance_squared = sum(v * v for v in vector(p['relative_position']))
        assert distance_squared <= 600. ** 2 + .1
        key = (distance_squared, p['id'])
        # f32 distance ties can differ slightly when recomputed in Python.
        if previous is not None:
            assert key[0] >= previous[0] - .1
            if key[0] == previous[0]:
                assert key[1] > previous[1]
        previous = key


def linear_approach(position, velocity, radius, horizon=2.):
    """Frozen straight-line screen, not a prediction of the physical outcome."""
    p, v = vector(position), vector(velocity)
    speed2 = sum(x * x for x in v)
    dot = sum(x * y for x, y in zip(p, v))
    distance = math.hypot(*p)
    closest_time = min(horizon, max(0., -dot / speed2)) if speed2 > 1.e-10 else 0.
    closest = math.hypot(*(x + closest_time * y for x, y in zip(p, v)))
    inside = distance <= radius
    discriminant = dot * dot - speed2 * (distance * distance - radius * radius)
    entry = None
    if inside:
        entry = 0.
    elif speed2 > 1.e-10 and dot < 0. and discriminant >= 0.:
        candidate = (-dot - math.sqrt(discriminant)) / speed2
        if 0. <= candidate <= horizon:
            entry = candidate
    return dict(range=distance, opening_speed=dot / distance if distance else None,
                closest_seconds=closest_time, closest_clearance=closest - radius,
                entry_seconds=entry)


def audit_stream(root, report):
    evidence = iter(rows(root / 'capture-evidence.jsonl'))
    count = 0
    maximum = dict(projectiles=0, debris_scanned=0, unavailable_shells=0, shells_in_range=0)
    for row in rows(root / 'projectiles.jsonl'):
        e = next(evidence, None)
        assert e is not None and row['seat'] == e['seat']
        audit_sample(row, e['pilot'])
        if row['diagnostic']:
            d = row['diagnostic']
            maximum['projectiles'] = max(maximum['projectiles'], len(d['projectiles']))
            for key in ('debris_scanned', 'unavailable_shells', 'shells_in_range'):
                maximum[key] = max(maximum[key], d[key])
        count += 1
    assert next(evidence, None) is None and count == report['elapsed_ticks'] * 2
    return dict(rows=count, maximum=maximum)


def run(entry, old, binary, out):
    root = out / entry['key']
    cmd = command(old, binary, root, entry['enabled'])
    result = dict(item=old['item'], command=cmd)
    try:
        with (out / (entry['key'] + '.log')).open('x') as log:
            subprocess.run(cmd, check=True, stdout=log, stderr=log, timeout=1800)
        result.update(S.C.analyze(root, old['item']))
        result['parity'] = S.C.P.replay_parity(old, result)
        reports = [json.loads((r / 'report.json').read_text()) for r in [root_of(old), root]]
        S.C.P.M.T.report_parity(*reports, ['pursuit_health', 'initial_cover', 'actual_route_recovery',
                                         'capture_escape', 'escape_travel', 'transfer_approach', 'transfer_speed'])
        # These streams carry every action and consumed observation; do not strip fields.
        if entry['enabled']:
            result['projectiles'] = audit_stream(root, reports[1])
        else:
            assert not (root / 'projectiles.jsonl').exists()
        result['hashes'] = {p.name: digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    except Exception:
        result['error'] = traceback.format_exc()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip(), 'freeze code, tests and plan first'
    speed = json.loads(args.prior.read_text())
    approach_path = Path(speed['prior_summary']['path'])
    assert digest(approach_path) == speed['prior_summary']['sha256']
    approach = json.loads(approach_path.read_text())
    assert speed['complete'] and approach['complete']
    names = ['shared-armed-world1-p1-powered', 'health-shared-armed-world1-p1-powered']
    plan = [dict(key=f'{side}-{name}', side=side, name=name, enabled=True)
            for side in ('approach', 'speed') for name in names]
    plan.append(dict(key='disabled-' + names[0], side='speed', name=names[0], enabled=False))
    sources = dict(approach=approach, speed=speed)
    for entry in plan:
        old = sources[entry['side']]['runs'][entry['name']]
        for name, expected in old['hashes'].items():
            assert digest(root_of(old) / name) == expected
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, plan=plan, runs={},
                  source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
                  binary=dict(path=str(binary), sha256=digest(binary)),
                  inputs={str(p): digest(p) for p in (args.prior, approach_path)},
                  runner_sha256=digest(__file__))
    save = lambda: S.F.D.write(args.out / 'summary.json', result)
    save()
    try:
        with ThreadPoolExecutor(max_workers=2) as pool:
            futures = [pool.submit(run, e, sources[e['side']]['runs'][e['name']], binary, args.out) for e in plan]
            for entry, future in zip(plan, futures):
                r = future.result()
                result['runs'][entry['key']] = r
                save()
                assert 'error' not in r, (entry['key'], r.get('error'))
                print(entry['key'] + ': exact replay and diagnostic audited', flush=True)
        assert digest(binary) == result['binary']['sha256']
        for path, expected in result['inputs'].items():
            assert digest(path) == expected
        result['complete'] = True
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
