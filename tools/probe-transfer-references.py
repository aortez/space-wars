#!/usr/bin/env python3
"""Audit all locally measured alternatives at used historical flag-shadow sources."""
import argparse
from collections import Counter
import gzip
import hashlib
import importlib.util
import json
import math
import struct
from pathlib import Path
import shutil
import subprocess

SPEC = importlib.util.spec_from_file_location('flags', Path(__file__).with_name('compare-flag-surveys.py'))
F = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(F)


def rows(path):
    with (gzip.open(path, 'rt') if path.suffix == '.gz' else path.open()) as stream:
        return [json.loads(line) for line in stream]


def plan(reference):
    study = json.loads((reference / 'summary.json').read_text())
    cases = []
    for condition in study['plan']:
        root = reference / condition['name']
        for report in rows(root / 'flag-value-shadow.jsonl'):
            if not any(a['used'] for a in report['admissions']):
                continue
            baseline = report['baseline']
            for candidate in report['augmented']['candidates']:
                if candidate['current'] or candidate['local'] is None:
                    continue
                assert candidate['unknown_reason'] in {
                    'transfer requires an unmodelled planet detour',
                    'moving body requires an unmodelled transfer detour'}
                source, destination = baseline['source_tick'], candidate['planet']
                cases.append(dict(name=f"{condition['name']}-t{source}-p{destination}",
                    condition=condition, source_tick=source, destination=destination,
                    seat={'player_1': 0, 'player_2': 1}[report['actor']],
                    transfer_source=baseline['transfer_source'], expected_reason=candidate['unknown_reason'],
                    candidate_kind='enemy' if candidate['observed_owner'] is not None else 'neutral'))
    return cases


def reference_prefixes(reference, cases):
    """Hash every original byte before each branch; retain only source rows."""
    expected = {}
    for name in dict.fromkeys(c['condition']['name'] for c in cases):
        ticks = {c['source_tick'] for c in cases if c['condition']['name'] == name}
        digest = hashlib.sha256()
        count = 0
        root = reference / name
        with gzip.open(root / 'trace.jsonl.gz', 'rb') as stream:
            for line in stream:
                r = json.loads(line)
                tick = r['tick']
                if tick > max(ticks):
                    break
                if tick in ticks:
                    key = (name, tick)
                    if key not in expected:
                        expected[key] = dict(prefix_sha256=digest.hexdigest(), prefix_rows=count, source_rows={})
                    expected[key]['source_rows'][r['seat']] = r
                digest.update(line)
                count += 1
        for tick in ticks:
            value = expected[(name, tick)]
            assert value['prefix_rows'] == 2 * tick and set(value['source_rows']) == {0, 1}
            value['evaluations'] = [r for r in rows(root / 'mission-evaluations.jsonl') if r['completed_tick'] < tick]
    return expected


def f32_identity(value):
    """Typed Rust f32 serialization and serde_json::Value use different decimals.
    Compare the represented f32 bits, never a floating-point tolerance.
    """
    if isinstance(value, float):
        return struct.pack('!f', value)
    if isinstance(value, dict):
        return {k: f32_identity(v) for k, v in value.items()}
    if isinstance(value, list):
        return [f32_identity(v) for v in value]
    return value


def audit_geometry(source, diagnostic):
    g = diagnostic['geometry']
    bodies = {b['index']: b for b in source['bodies']}
    target, frame = bodies[diagnostic['destination']], bodies[source['frame']]
    vec = lambda p: (p['x'], p['y'])
    add = lambda a, b: tuple(x + y for x, y in zip(a, b))
    sub = lambda a, b: tuple(x - y for x, y in zip(a, b))
    mul = lambda a, n: tuple(x * n for x in a)
    dot = lambda a, b: sum(x * y for x, y in zip(a, b))
    unit = lambda a: mul(a, 1 / math.hypot(*a))
    close = lambda a, b: math.dist(a, b) <= 0.002
    ship = source['ship']
    relative = sub(vec(ship['velocity']), vec(frame['velocity']))
    radial = sub(vec(ship['position']), vec(frame['position']))
    up = unit(radial)
    falling = max(-dot(relative, up), 0)
    climb = max(70 + falling * falling / 50 - (math.hypot(*radial) - frame['radius']), 0)
    launch = add(vec(ship['position']), mul(up, climb))
    entry = add(vec(target['position']), mul(unit(sub(launch, vec(target['position']))), target['radius'] + 85))
    assert g['leg'] in {'climb', 'transfer'}
    start, end = (vec(ship['position']), launch) if g['leg'] == 'climb' else (launch, entry)
    assert close(vec(g['from']), start) and close(vec(g['to']), end)
    assert (diagnostic['completed_stages'] is not None) == (g['leg'] == 'transfer')
    if g['check'] == 'boundary':
        assert g['body'] is None and diagnostic['reason'] == 'transfer requires unmodelled boundary guidance'
        center = vec(source['boundary']['center'])
        assert close(vec(g['obstacle_from']), center) and close(vec(g['obstacle_to']), center)
        assert math.isclose(g['threshold'], source['boundary']['radius'] - 65, abs_tol=0.0001)
        separation = max(math.dist(vec(g['from']), center), math.dist(vec(g['to']), center))
        assert math.isclose(g['separation'], separation, abs_tol=0.002) and separation > g['threshold']
        return
    if g['check'] == 'sun':
        body = source['sun']
        assert g['body'] is None and diagnostic['reason'] == 'transfer requires an unmodelled solar detour'
    else:
        assert g['check'] in {'static_planet', 'moving_planet'}
        assert g['body'] != (source['frame'] if g['leg'] == 'climb' else diagnostic['destination'])
        body = bodies[g['body']]
    assert math.isclose(g['threshold'], body['radius'] + 65, abs_tol=0.0001)
    assert close(vec(g['obstacle_from']), vec(body['position']))
    if g['check'] in {'static_planet', 'sun'}:
        if g['check'] == 'static_planet':
            assert diagnostic['reason'] == 'transfer requires an unmodelled planet detour'
        obstacle_end = vec(body['position'])
    else:
        assert diagnostic['reason'] == 'moving body requires an unmodelled transfer detour'
        assert g['leg'] == 'transfer'
        stages = diagnostic['completed_stages']
        seconds = sum(stages[k] for k in ['settle_seconds', 'turn_seconds', 'climb_seconds', 'cruise_seconds'])
        assert math.isfinite(seconds) and 0 <= seconds <= 30
        obstacle_end = add(vec(body['position']), mul(sub(vec(body['velocity']), vec(target['velocity'])), seconds))
    assert close(vec(g['obstacle_to']), obstacle_end)

    def point_segment(p, a, b):
        delta = sub(b, a)
        t = max(0, min(1, dot(sub(p, a), delta) / max(dot(delta, delta), 1e-30)))
        return math.dist(p, add(a, mul(delta, t)))
    a, b, c, d = vec(g['obstacle_from']), vec(g['obstacle_to']), vec(g['from']), vec(g['to'])
    cross = lambda a, b: a[0] * b[1] - a[1] * b[0]
    denominator = cross(sub(b, a), sub(d, c))
    intersects = abs(denominator) > 1e-7 and (0 <= cross(sub(c, a), sub(d, c)) / denominator <= 1
                                           and 0 <= cross(sub(c, a), sub(b, a)) / denominator <= 1)
    separation = 0 if intersects else min(point_segment(a, c, d), point_segment(b, c, d),
                                         point_segment(c, a, b), point_segment(d, a, b))
    # Independent double arithmetic compared to f32 geometry; never used for
    # source identity or to change the reference's acceptance.
    assert math.isclose(g['separation'], separation, abs_tol=0.002)
    assert g['separation'] < g['threshold']


def audit_prefix(root, case, expected, probe):
    digest = hashlib.sha256()
    count = 0
    source_rows = {}
    with (root / 'trace.jsonl').open('rb') as stream:
        for line in stream:
            r = json.loads(line)
            if r['tick'] > case['source_tick']:
                break
            if r['tick'] < case['source_tick']:
                digest.update(line)
                count += 1
            else:
                source_rows[r['seat']] = r
    assert count == expected['prefix_rows'] == case['source_tick'] * 2
    assert digest.hexdigest() == expected['prefix_sha256'], 'changed frozen prefix'
    assert set(source_rows) == {0, 1}
    for seat in [0, 1]:
        assert source_rows[seat]['observation'] == expected['source_rows'][seat]['observation'], 'changed source observation'
        if seat != case['seat'] or not probe['source']['nomination']['accepted']:
            assert source_rows[seat] == expected['source_rows'][seat], 'refused/opponent source control changed'
    assert f32_identity(probe['source']['transfer_source']) == f32_identity(case['transfer_source']), 'wrong pinned reference'
    evaluations = [r for r in rows(root / 'mission-evaluations.jsonl') if r['completed_tick'] < case['source_tick']]
    assert evaluations == expected['evaluations'], 'changed original evaluator prefix'
    return dict(exact_prefix=True, prefix_rows=count, prefix_sha256=digest.hexdigest(),
                exact_source_observations=True, exact_transfer_source=True, exact_evaluation_prefix=True,
                evaluations=len(evaluations))


def audit_probe(case, probe, trace):
    assert probe['source_tick'] == case['source_tick'] and probe['destination'] == case['destination']
    source = probe['source']
    assert source is not None and source['tick'] == case['source_tick']
    d = source['diagnostic']
    assert d['destination'] == case['destination'] and d['reason'] == case['expected_reason']
    if 'expected_diagnostic' in case:
        assert f32_identity(d) == f32_identity(case['expected_diagnostic'])
    if d['reference'] is not None:
        assert d['reason'] is None and d['geometry'] is None
        assert d['reference'] == d['completed_stages']
        seconds = [d['reference'][k] for k in ['settle_seconds', 'turn_seconds', 'climb_seconds', 'cruise_seconds']]
        assert all(math.isfinite(t) and t >= 0 for t in seconds) and sum(seconds) <= 30.00001
    elif d['geometry'] is not None:
        audit_geometry(case['transfer_source'], d)
    else:
        assert d['reason'] in {'transfer frame unmeasured', 'transfer destination unmeasured',
                               'transfer control authority unmodelled', 'transfer exceeds short direct reference horizon'}
    outcome = probe['outcome']
    elapsed = outcome['tick'] - case['source_tick']
    assert outcome['elapsed_ticks'] == elapsed and 0 <= elapsed <= 3600
    assert [r['tick'] for r in trace] == list(range(case['source_tick'], outcome['tick'] + 1))
    assert all(r['terminal'] is None for r in trace[:-1]) and trace[-1]['terminal'] == outcome
    accepted = source['nomination']['accepted']
    assert accepted == (source['nomination']['reason'] is None)
    if not accepted:
        assert outcome['reason'] == 'nomination_refused' and elapsed == 0
    else:
        assert trace[0]['target'] == case['destination']
        def terminal_reason(row):
            if row['match_finished']: return 'match_finished'
            if not row['ship_available'] or row['health'] <= 0 or row['form'] != 'ship' or row['location'] == 'on_foot':
                return 'ship_or_pilot_lost'
            if row['recovery_active']: return 'recovery'
            if row['solver_contact'] or row['debris_contact']: return 'solver_or_debris_contact'
            if row['arrived']: return 'arrived'
            if row['target'] != case['destination']: return 'retargeted'
            if row['tick'] >= case['source_tick'] + 3600: return 'timeout'
            return None
        assert all(terminal_reason(r) is None for r in trace[:-1])
        last = trace[-1]
        assert terminal_reason(last) == outcome['reason']
        if outcome['reason'] == 'arrived':
            assert last['arrived'] and last['queries_ready']
            assert not last['solver_contact'] and not last['debris_contact']
            assert last['frame'] == case['destination']
            target = next(p for p in last['planets'] if p['index'] == case['destination'])
            distance = lambda a, b: sum((a[k] - b[k]) ** 2 for k in ['x', 'y']) ** 0.5
            assert distance(last['ship']['position'], target['motion']['position']) < target['radius'] + 105
            assert distance(last['ship']['velocity'], target['motion']['velocity']) < 18
        elif outcome['reason'] == 'solver_or_debris_contact':
            assert last['solver_contact'] or last['debris_contact']
        elif outcome['reason'] == 'retargeted':
            assert last['target'] != case['destination']
        elif outcome['reason'] == 'timeout':
            assert elapsed == 3600
        else:
            assert outcome['reason'] in {'recovery', 'ship_or_pilot_lost', 'match_finished'}
    avoidance = Counter(json.dumps(r['avoidance']['obstacle'], sort_keys=True) for r in trace if r['avoidance'])
    return dict(accepted=accepted, refusal=source['nomination']['reason'], outcome=outcome,
                diagnostic=d, avoidance_ticks=dict(avoidance), frame_changes=sum(
                    a['frame'] != b['frame'] for a, b in zip(trace, trace[1:])),
                health_at_source=trace[0]['health'], health_at_end=trace[-1]['health'])


def probe_args(case):
    return ['--world', 'generated', '--seed', str(case['condition']['seed']), '--mode', 'duel',
            '--match', 'true', '--seat', '0', '--asteroid-interval', str(case['condition']['interval']),
            '--p1-policy', f"material_mission_v{case['condition']['policies'][0]}",
            '--p2-policy', f"material_mission_v{case['condition']['policies'][1]}",
            '--trace', 'true', '--trace-start-tick', '0', '--trace-end-tick', '36002',
            '--survey-capture-flags', 'true', '--shadow-capture-flags', 'true',
            '--probe-transfer-seat', str(case['seat']), '--probe-transfer-tick', str(case['source_tick']),
            '--probe-transfer-destination', str(case['destination'])]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    cases = plan(opts.reference)
    assert len(cases) == 31 and len({(c['condition']['name'], c['source_tick']) for c in cases}) == 16
    opts.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=cases, runs={},
        scope='All 31 alternatives from 16 used historical flag comparisons, two worlds. One external nomination before source intent; 60-second maximum, controller commitment/recovery/solar/combat priorities retained. Stops at actual arrival, conservative solver/debris contact, loss/recovery, retarget, match end or timeout. Loss/contact precedes same-tick arrival; solver contacts include positive separation and do not prove impact. Interventions bypass evidence/value admission only. Refusals and interruptions are not numeric costs. Correlated engineering cases, not strength samples.',
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()),
        binary_sha256=F.digest(opts.binary), reference_summary_sha256=F.digest(opts.reference / 'summary.json'))
    F.D.write(opts.out / 'summary.json', result)  # full plan before any physics
    expected = reference_prefixes(opts.reference, cases)
    for c in cases:
        condition = c['condition']
        args = probe_args(c)
        run = F.D.run(opts.binary, opts.out, c['name'], args, c['seat'], seconds=c['source_tick'] // 60 + 61)
        root = opts.out / c['name']
        report = json.loads((root / 'report.json').read_text())
        probe = report['transfer_probe']
        run['probe'] = audit_probe(c, probe, rows(root / 'transfer-probe.jsonl'))
        run['parity'] = audit_prefix(root, c, expected[(condition['name'], c['source_tick'])], probe)
        run['trace_sha256'] = F.digest(root / 'trace.jsonl')
        run['probe_trace_sha256'] = F.digest(root / 'transfer-probe.jsonl')
        with (root / 'trace.jsonl').open('rb') as source, gzip.open(root / 'trace.jsonl.gz', 'wb', compresslevel=3) as dest:
            shutil.copyfileobj(source, dest)
        (root / 'trace.jsonl').unlink()
        result['runs'][c['name']] = run
        F.D.write(opts.out / 'summary.json', result)
        print(c['name'], run['probe']['outcome'], run['probe']['refusal'], flush=True)
    assert F.digest(opts.binary) == result['binary_sha256']
    result['outcomes'] = dict(Counter(r['probe']['outcome']['reason'] for r in result['runs'].values()))
    F.D.write(opts.out / 'summary.json', result)


if __name__ == '__main__':
    main()
