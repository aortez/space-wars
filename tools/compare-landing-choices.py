#!/usr/bin/env python3
"""Explain the fixed acquisition corpus using the native landing selector."""
import argparse
from collections import Counter
import csv
import gzip
import hashlib
import importlib.util
import itertools
import json
import math
from pathlib import Path
import struct
import subprocess
import sys

SPEC = importlib.util.spec_from_file_location('acquisition', Path(__file__).with_name('probe-site-acquisition.py'))
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)
L, C, T, F = P.L, P.C, P.T, P.F

# Independent reconstruction rounds elementary arithmetic to f32. Transcendentals
# may differ across libm implementations: never use the tolerance to declare a
# near-zero clearance safe/unsafe. Record that sign as independently unresolved.
CLEARANCE_TOLERANCE = .01
SCORE_TOLERANCE = .002


def f32(v): return struct.unpack('!f', struct.pack('!f', v))[0]
def vec(v): return (f32(v['x']), f32(v['y']))
def add(a, b): return tuple(f32(x+y) for x, y in zip(a, b))
def sub(a, b): return tuple(f32(x-y) for x, y in zip(a, b))
def mul(a, s): return tuple(f32(x*s) for x in a)
def dot(a, b): return f32(f32(a[0]*b[0])+f32(a[1]*b[1]))
def length(a): return f32(math.sqrt(dot(a, a)))
def unit(a): return tuple(f32(x/length(a)) for x in a) if length(a) else a
def rotate(a, angle):
    s, c = f32(math.sin(angle)), f32(math.cos(angle))
    return (f32(f32(a[0]*c)-f32(a[1]*s)), f32(f32(a[0]*s)+f32(a[1]*c)))


def short_angle(p, site):
    center = vec(p['planet']['motion']['position'])
    up, direction = [unit(sub(vec(v), center)) for v in [p['ship']['position'], site['vehicle_position']]]
    return f32(math.atan2(f32(f32(up[0]*direction[1])-f32(up[1]*direction[0])), dot(up, direction)))


def directed(short, side):
    return short if abs(short) < f32(.2) or math.copysign(1, short) == side else f32(short + f32(side*f32(math.tau)))


def solar_plan(local, site, side, *, circling=True):
    """Rebuild sampled arc, parking and both departure corridors from raw geometry."""
    sun = local['sun']
    if sun is None: return None
    p = local['combat']['recovery']['flight']['pilot']
    motion, omega = p['planet']['motion'], local['planet_orbit_omega']
    center, sun_pos = vec(motion['position']), vec(sun['position'])
    safe = f32(max(f32(sun['radius']), f32(sun['heat_radius']))+10)
    up = unit(sub(vec(p['ship']['position']), center))
    offset, normal = sub(vec(site['vehicle_position']), center), vec(site['normal'])
    angle = directed(short_angle(p, site), side)
    radius = f32(f32(p['planet']['radius'])+60)
    travel = (f32(f32(abs(angle)*radius)/30) if circling else
              f32(length(sub(vec(p['ship']['position']), vec(site['vehicle_position'])))/12))

    def forecast(offset, seconds):
        position = (add(sun_pos, rotate(sub(center, sun_pos), f32(f32(omega)*seconds)))
                    if omega is not None else add(center, mul(vec(motion['velocity']), seconds)))
        return add(position, rotate(offset, f32(f32(motion['spin'])*seconds)))

    def clearance(a, b):
        delta = sub(b, a)
        t = max(0, min(1, f32(dot(sub(sun_pos, a), delta)/max(dot(delta, delta), f32(.0001)))))
        return f32(length(sub(sun_pos, add(a, mul(delta, t))))-safe)

    if circling:
        previous = forecast(mul(up, radius), 0)
        approach = clearance(vec(p['ship']['position']), previous)
        steps = max(math.ceil(f32(abs(angle)/f32(.08))), 1)
        for step in range(1, steps+1):
            fraction = f32(step/steps)
            point = forecast(mul(rotate(up, f32(angle*fraction)), radius), f32(travel*fraction))
            approach = min(approach, clearance(previous, point))
            previous = point
        approach = min(approach, clearance(previous, forecast(offset, travel)))
    else:
        approach = clearance(vec(p['ship']['position']), forecast(offset, travel))
    parked, departures = math.inf, [math.inf, math.inf]
    for step in range(13):
        seconds = f32(travel+f32(f32(25*step)/12))
        position = forecast(offset, seconds)
        parked = min(parked, f32(length(sub(position, sun_pos))-safe))
        lift = forecast(add(offset, mul(normal, 20)), seconds)
        for i, exit_side in enumerate([-1, 1]):
            exit_offset = add(add(offset, mul(normal, 92)), mul(mul((-normal[1], normal[0]), exit_side), 152))
            exit_position = forecast(exit_offset, seconds)
            departures[i] = min(departures[i], clearance(position, lift), clearance(lift, exit_position))
    return dict(forecast_tick=p['tick'], arrival_seconds=travel, surface_seconds=25, side=side,
                approach_clearance=approach, parked_clearance=parked, departure_clearance=max(departures),
                departure_side=-1 if departures[0] > departures[1] else 1,
                departure_corridors=departures)


def close(actual, expected, tolerance):
    assert isinstance(actual, (int, float)) and math.isfinite(actual)
    assert abs(actual-expected) <= tolerance, (actual, expected, tolerance)


def audit_choice(case, row, choice, *, allow_delayed_start=False):
    """Audit fresh neutral choices; flagged routes remain outside this fixed corpus."""
    reference = case['candidate']['local_reference']['evidence']['site']
    assert choice['reference'] == reference
    assert choice['physics_queries'] == 0 and math.isfinite(choice['assessment_ms']) and choice['assessment_ms'] >= 0
    if row is None:
        assert choice['observation_tick'] is None and choice['actor'] is None
        assert choice['report'] is None and choice['unknown'] == 'no observed site choice'
        assert choice['assessment_ms'] == 0
        return dict(classification='not_observed')
    p, local, capture = P.pilot(row), row['observation']['local'], row['mission']['capture']
    assert row['seat'] == case['seat'] and p['owner'] == f"player_{case['seat']+1}"
    assert choice['actor'] == p['owner'] and choice['observation_tick'] == row['tick'] == p['tick']
    if choice['report'] is None:
        assert isinstance(choice['unknown'], str) and choice['unknown']
        return dict(classification='unknown', reason=choice['unknown'])
    native = capture['acquisition']
    assert row['mission']['goal'] == 'capture' and row['mission']['target'] == case['destination']
    assert row['mission']['recovery'] is None
    assert native['reason'] == 'selected_site' and native['tick'] == row['tick']
    assert native['planet'] == p['planet']['index'] and native['revision'] == p['planet']['revision']
    assert native['site_query'] == p['site_query']
    assert native['selected_site'] == capture['site']
    assert capture['started_tick'] is not None and capture['started_tick'] <= row['tick']
    if not allow_delayed_start: assert capture['started_tick'] == row['tick']
    assert choice['unknown'] is None
    r = choice['report']
    assert r['model'] == 'native_landing_choice_comparison_v1'
    assert r['tick'] == p['tick'] and r['planet'] == p['planet']['index'] == case['destination']
    assert r['revision'] == p['planet']['revision'] and r['reference'] == reference
    assert r['commit_descent'] and r['required_site'] is None and native['required_site'] is None
    assert r['objective'] is None and native['objective'] is None and p['planet']['claim']['flag'] is None
    assert capture['replans'] == 0 and capture['failed_tick'] is None
    target = local['combat']['target']
    exposed = target is not None and not target['ground_occluded'] and length(sub(vec(target['motion']['position']), vec(p['ship']['position']))) < 300
    assert r['exposed'] == exposed
    assert r['site_count'] == native['sites_available'] == len(p['sites']) <= 64
    assert len({(s['id']['planet'], s['id']['bearing']) for s in p['sites']}) == len(p['sites'])
    checks = dict.fromkeys(native['checks'], 0)
    expected_order, ambiguity, departure_ambiguity, solar_error = [], [], [], 0.0
    for index, site in enumerate(p['sites']):
        assert site['id']['planet'] == r['planet'] and 0 <= site['id']['bearing'] < 64
        short = short_angle(p, site)
        preferred = -1 if short < 0 else 1
        for direction_order, side in enumerate([preferred, -preferred] if local['sun'] else [preferred]):
            expected_order.append((index, direction_order, side))
    assert [(v['site_order'], v['direction_order'], v['side']) for v in r['assessments']] == expected_order
    for a in r['assessments']:
        site = p['sites'][a['site_order']]
        assert a['site'] == site['id'] and a['revision'] == site['revision']
        short = short_angle(p, site)
        close(a['short_angle'], short, .000001)
        expected = solar_plan(local, site, a['side'])
        unsafe = False
        if expected is None:
            assert a['solar'] is None
        else:
            assert a['solar'] is not None
            for key in ['forecast_tick', 'side', 'surface_seconds']:
                assert a['solar'][key] == expected[key]
            close(a['solar']['arrival_seconds'], expected['arrival_seconds'], SCORE_TOLERANCE)
            assert a['solar']['departure_side'] in [-1, 1]
            left, right = expected['departure_corridors']
            if abs(left-right) <= 2*CLEARANCE_TOLERANCE:
                departure_ambiguity.append(dict(site=site['id'], side=a['side'],
                    corridor_clearances=[left, right]))
            else:
                assert a['solar']['departure_side'] == expected['departure_side']
            for component, counter in [('approach', 'unsafe_approach'), ('parked', 'unsafe_parking'), ('departure', 'unsafe_departure')]:
                key = component+'_clearance'
                value = a['solar'][key]
                close(value, expected[key], CLEARANCE_TOLERANCE)
                solar_error = max(solar_error, abs(value-expected[key]))
                if abs(expected[key]) <= CLEARANCE_TOLERANCE:
                    ambiguity.append(dict(site=site['id'], side=a['side'], component=component))
                else:
                    assert (value < 0) == (expected[key] < 0)
                checks[counter] += value < 0
                unsafe |= value < 0
        checks['directions'] += 1
        checks['unsafe_solar'] += unsafe
        assert a['rejection'] == ('unsafe_solar' if unsafe else None)
        if unsafe:
            assert all(a[k] is None for k in ['approach_score', 'cover_penalty', 'ground_score', 'total_score'])
        else:
            cover = next((v for v in local['cover'] if v['site'] == site['id']), None)
            penalty = 400 if exposed and not (cover and all(cover[k] for k in ['grounded', 'approach', 'departure'])) else 0
            angle = directed(short, a['side']) if expected is not None else short
            approach = f32(abs(angle)*f32(f32(p['planet']['radius'])+60))
            close(a['approach_score'], approach, SCORE_TOLERANCE)
            assert a['cover_penalty'] == penalty and a['ground_score'] == 0
            assert f32(a['total_score']) == f32(f32(f32(a['approach_score'])+penalty)+0)
            assert a['total_score'] >= 0
            checks['eligible'] += 1
    assert checks == r['checks'] == native['checks']
    eligible = [v for v in r['assessments'] if v['rejection'] is None]
    selected = min(eligible, key=lambda v: v['total_score'])
    assert r['selected'] == selected and selected['site'] == capture['site']
    assert T.f32_identity(selected['solar']) == T.f32_identity(capture['solar'])
    refs = [v for v in r['assessments'] if v['site'] == reference]
    viable = [v for v in refs if v['rejection'] is None]
    best = min(viable, key=lambda v: v['total_score']) if viable else None
    assert best == r['reference_best']
    classification = ('same_site' if selected['site'] == reference else
        ('native_order_tie' if best['total_score'] == selected['total_score'] else 'higher_reference_score') if best else
        'reference_rejected' if refs else 'reference_absent')
    assert r['classification'] == classification
    return dict(classification=classification, reference=reference, selected=selected['site'],
        selected_score=selected['total_score'], reference_score=best['total_score'] if best else None,
        reference_directions=[dict(side=v['side'], rejection=v['rejection'], solar=v['solar'],
            approach_score=v['approach_score'], cover_penalty=v['cover_penalty'], total_score=v['total_score']) for v in refs],
        assessments=len(r['assessments']), eligible=len(eligible), independent_solar_max_error=solar_error,
        independently_unresolved_clearance_signs=ambiguity,
        independently_unresolved_departure_sides=departure_ambiguity)


def without_wall_times(value):
    if isinstance(value, dict):
        return {k: without_wall_times(v) for k, v in value.items() if not k.endswith('_ms')}
    if isinstance(value, list): return [without_wall_times(v) for v in value]
    return value


def audit_sensors(before, after):
    count, digest = 0, hashlib.sha256()
    with (before/'sensors.jsonl').open() as a, (after/'sensors.jsonl').open() as b:
        for old, new in itertools.zip_longest(a, b):
            assert old is not None and new is not None, 'sensor row count changed'
            old, new = [without_wall_times(json.loads(v)) for v in [old, new]]
            assert old == new, 'sensor profile/call counts changed'
            digest.update((json.dumps(old, sort_keys=True, separators=(',', ':'))+'\n').encode())
            count += 1
    return dict(rows=count, work_sha256=digest.hexdigest())


def parity(before, after):
    count, size, digest = 0, 0, hashlib.sha256()
    with gzip.open(before/'trace.jsonl.gz', 'rb') as a, (after/'trace.jsonl').open('rb') as b:
        for old, new in itertools.zip_longest(a, b):
            assert old == new, 'full controller/observation trace changed'
            count += 1
            size += len(old)
            digest.update(old)
    for name in P.UPSTREAM+['transfer-probe.jsonl']:
        assert F.digest(before/name) == F.digest(after/name), name
    def live(root):
        with (root/'live-planning.csv').open() as stream:
            return [{k: v for k, v in r.items() if k != 'dispatch_ms'} for r in csv.DictReader(stream)]
    a, b = live(before), live(after)
    assert a == b
    assert P.evaluation_charges(before, a) == P.evaluation_charges(after, b)
    old, new = [json.loads((root/'report.json').read_text()) for root in [before, after]]
    assert old['transfer_probe'] == {k: v for k, v in new['transfer_probe'].items() if k != 'landing_choice'}
    assert F.D.same_physical_outcomes(old, new) and old['missions'] == new['missions']
    L.S.audit_upstream_parity(before, after)
    return dict(controller_rows=count, decompressed_bytes=size, trace_sha256=digest.hexdigest(),
                sensors=audit_sensors(before, after),
                physics=True, transfer=True, upstream=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True, help='completed acquisition corpus')
    parser.add_argument('--source-reference', type=Path, required=True, help='original source-local corpus')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    previous = json.loads((opts.reference/'summary.json').read_text())
    cases, denominator = P.plan(opts.source_reference)
    assert cases == previous['plan'] and denominator == previous['denominator']
    ordinary_plan = json.loads((opts.source_reference/'summary.json').read_text())['plan']
    assert len(ordinary_plan) == 13
    opts.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=cases, denominator=denominator, runs={}, ordinary_plan=ordinary_plan,
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()),
        binary_sha256=F.digest(opts.binary), reference_summary_sha256=F.digest(opts.reference/'summary.json'),
        source_summary_sha256=F.digest(opts.source_reference/'summary.json'),
        clearance_tolerance=CLEARANCE_TOLERANCE, score_tolerance=SCORE_TOLERANCE,
        scope='All20 fixed numeric neutral alternatives from62 candidates; historical sites never forced. Same native selector, ordinary controls after handoff,30s observation censor. Read-only diagnostic at fresh choice; scores are controller units, not seconds. Full trajectory parity against archived previous executable. No live policy, cost admission, calibration, deployment or strength claim.')
    save = lambda: F.D.write(opts.out/'summary.json', result)
    save()
    for case in cases:
        name = case['name']+'-on'
        before, root = opts.reference/name, opts.out/name
        for file, digest in previous['runs'][case['name']]['files'][name].items():
            assert F.digest(before/file) == digest
        bearing = case['candidate']['local_reference']['evidence']['site']['bearing']
        args = T.probe_args(case)+['--probe-transfer-pursuit', 'defer_new', '--bounded-acquisition-seats', 'none',
            '--probe-acquisition-seconds', str(P.HORIZON_SECONDS), '--probe-landing-reference-bearing', str(bearing)]
        run = F.D.run(opts.binary, opts.out, name, args, case['seat'], seconds=case['source_tick']//60+91)
        run['parity'] = parity(before, root)
        report = json.loads((root/'report.json').read_text())
        run['source'] = P.audit_source(case, opts.source_reference, root, report['transfer_probe'])
        control = [r for r in T.rows(root/'trace.jsonl') if r['seat'] == case['seat'] and r['tick'] >= case['source_tick']]
        run['acquisition'] = P.audit_acquisition(case, report, control)
        a = report['transfer_probe']['acquisition']
        chosen = a['outcome'] is not None and a['outcome']['reason'] == 'site_selected'
        run['choice'] = report['transfer_probe']['landing_choice']
        run['audit'] = audit_choice(case, control[-1] if chosen else None, run['choice'])
        run['trace_sha256'] = C.archive(root)
        run['files'] = {p.name: F.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
        result['runs'][case['name']] = run
        save()
        print(case['name'], run['audit']['classification'], flush=True)
    assert F.digest(opts.binary) == result['binary_sha256']
    result['results'] = dict(classifications=dict(Counter(r['audit']['classification'] for r in result['runs'].values())),
        controller_rows=sum(r['parity']['controller_rows'] for r in result['runs'].values()),
        physical_ticks=sum(r['elapsed_ticks'] for r in result['runs'].values()),
        assessments=sum(r['audit'].get('assessments', 0) for r in result['runs'].values()),
        independently_unresolved_clearance_signs=sum(len(r['audit'].get('independently_unresolved_clearance_signs', [])) for r in result['runs'].values()),
        independently_unresolved_departure_sides=sum(len(r['audit'].get('independently_unresolved_departure_sides', [])) for r in result['runs'].values()))
    save()
    # The core native selector was refactored. Re-run all13 ordinary source-local
    # cases too, with the diagnostic disabled and the existing strict audit.
    command = [sys.executable, str(Path(__file__).with_name('compose-capture-references.py')),
               '--reference', str(opts.source_reference), '--out', str(opts.out/'ordinary'),
               '--binary', str(opts.binary)]
    subprocess.run(command, check=True)
    ordinary = json.loads((opts.out/'ordinary'/'summary.json').read_text())
    assert ordinary['plan'] == ordinary_plan and ordinary['source_commit'] == result['source_commit']
    assert ordinary['binary_sha256'] == result['binary_sha256'] and not ordinary['source_dirty']
    result['ordinary'] = dict(command=command, summary_sha256=F.digest(opts.out/'ordinary'/'summary.json'),
        runs=len(ordinary['runs']), physical_ticks=sum(r['elapsed_ticks'] for r in ordinary['runs'].values()),
        results=ordinary['results'], sensors={name: audit_sensors(opts.source_reference/name, opts.out/'ordinary'/name)
                                            for name in ordinary['runs']})
    assert F.digest(opts.binary) == result['binary_sha256']
    save()


if __name__ == '__main__':
    main()
