#!/usr/bin/env python3
"""Describe dense native arrival phases without changing or ranking bot controls."""
import argparse
import gzip
import hashlib
import json
import math
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pilot(row):
    return row['observation']['local']['combat']['recovery']['flight']['pilot']


def geometry(row):
    p = pilot(row)
    capture = row['mission']['capture']
    site = next(s for s in p['sites'] if s['id'] == capture['site'])
    assert site['revision'] == p['planet']['revision']
    ship, planet = p['ship'], p['planet']['motion']
    x, y = [ship['position'][k] - planet['position'][k] for k in ['x', 'y']]
    radius = math.hypot(x, y)
    assert radius > 0 and math.isfinite(radius)
    up, tangent = (x / radius, y / radius), (-y / radius, x / radius)
    vx = ship['velocity']['x'] - planet['velocity']['x'] + y * planet['spin']
    vy = ship['velocity']['y'] - planet['velocity']['y'] - x * planet['spin']
    sx, sy = [site['vehicle_position'][k] - planet['position'][k] for k in ['x', 'y']]
    normal = site['normal']
    dx, dy = [ship['position'][k] - site['vehicle_position'][k] for k in ['x', 'y']]
    return dict(tick=p['tick'], site=site['id'],
        signed_angle=math.atan2(x * sy - y * sx, x * sx + y * sy),
        tangent_speed=vx * tangent[0] + vy * tangent[1],
        orbital_bearing_rate=(x * vy - y * vx) / (radius * radius),
        radial_speed=vx * up[0] + vy * up[1], altitude=radius - p['planet']['radius'],
        height=dx * normal['x'] + dy * normal['y'],
        side_error=-(dx * normal['y'] - dy * normal['x']))


def analyze(root, item):
    trace = root / item['name'] / 'trace.jsonl.gz'
    start, end = item['switch']['tick'], item['visit']['departed_tick']
    rows = {}
    with gzip.open(trace, 'rt') as stream:
        for row in map(json.loads, stream):
            tick = pilot(row)['tick']
            if row['seat'] == item['seat'] and start <= tick <= end:
                assert tick not in rows, 'duplicate control tick'
                assert row['tick'] + 1 == tick, 'directed fixture world/control epoch changed'
                rows[tick] = row
    assert set(rows) == set(range(start, end + 1)), 'incomplete dense arrival window'
    phases = {}
    selected = []
    for row in rows.values():
        c = row['mission'].get('capture')
        if row['mission']['target'] != item['switch']['to'] or not c or c['site'] is None:
            continue
        assert c['circling_replans'] == 0 and c['landing']['landing_retries'] == 0
        phases.setdefault(c['goal'], row)
        selected.append(row)
    assert selected and all(r['mission']['capture']['site'] == selected[0]['mission']['capture']['site'] for r in selected)
    first = geometry(selected[0])
    approach, surface = [geometry(phases[p]) for p in ['approach', 'surface']]
    landed = item['visit']['landed_tick']
    assert first['tick'] <= approach['tick'] <= surface['tick'] <= landed <= end
    spans = dict(circling=approach['tick'] - first['tick'],
        approach=surface['tick'] - approach['tick'],
        landing=landed - surface['tick'], capture_and_departure=end - landed)
    assert sum(spans.values()) == end - first['tick']
    approach_geometry = [geometry(r) for t, r in rows.items() if approach['tick'] <= t < surface['tick']]
    p = pilot(selected[0])
    return dict(name=item['name'], version=item['version'], seat=item['seat'],
        target=item['switch']['to'], trace_sha256=digest(trace), dense_ticks=len(rows),
        first_choice=first, approach_entry=approach, surface_entry=surface,
        landed_tick=landed, departed_tick=end, phase_ticks=spans,
        maximum_approach_side_error=max(abs(g['side_error']) for g in approach_geometry),
        approach_ticks_outside_8_units=sum(abs(g['side_error']) > 8 for g in approach_geometry),
        arrival_motion=dict(ship=p['ship'], planet=p['planet']['motion']),
        orbital_bearing_rate_scope='rate of ship radial bearing around the rotating planet (not body spin); radians per second; positive is counterclockwise')


def source_geometry(root, item, first):
    path = root / item['name'] / 'neutral-approach-inputs.jsonl.gz'
    with gzip.open(path, 'rt') as stream:
        row = next(r for r in map(json.loads, stream)
            if r['tick'] == item['switch']['source_tick'] and r['actor'] == f"player_{item['seat']+1}")
    measurement = next(c['measurement'] for c in row['evidence']['candidates'] if c['id'] == first['site'])
    evaluations = root / item['name'] / 'mission-evaluations.jsonl'
    with evaluations.open() as stream:
        report = next(r for r in map(json.loads, stream)
            if r['source_tick'] == row['tick'] and r['actor'] == row['actor'])
    candidate = next(c for c in report['candidates'] if c['planet'] == item['switch']['to'])
    observed = next(p for p in row['planets'] if p['index'] == item['switch']['to'])
    assert candidate['site'] == measurement['site']['id'] == first['site']
    assert candidate['evidence_tick'] == measurement['tick']
    assert candidate['revision'] == measurement['revision'] == measurement['site']['revision'] == observed['revision']
    assert candidate['total_seconds'] is not None
    assert measurement['tick'] < row['tick'] < first['tick']
    planet = observed['motion']
    old = measurement['planet']
    rotation = planet['angle'] - old['angle']
    x, y = [measurement['site']['vehicle_position'][k] - old['position'][k] for k in ['x', 'y']]
    vx, vy = x * math.cos(rotation) - y * math.sin(rotation), x * math.sin(rotation) + y * math.cos(rotation)
    sx, sy = [row['ship_position'][k] - planet['position'][k] for k in ['x', 'y']]
    return dict(measurement_tick=measurement['tick'], evaluation_source_tick=row['tick'],
        signed_angle=math.atan2(sx * vy - sy * vx, sx * vx + sy * vy),
        neutral_inputs_sha256=digest(path), evaluation_stream_sha256=digest(evaluations),
        scope='Historical measured vehicle position reprojected into the planet pose at evaluation; no future motion used.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    summary = json.loads((args.root / 'summary.json').read_text())
    assert summary['complete'] and len(summary['plan']) == len(summary['runs']) == 12
    assert all(r['physical_missions_and_evaluation_parity'] for r in summary['runs'])
    expected = {r['name']: r['trace_sha256'] for r in summary['runs']}
    results = [analyze(args.root, item) for item in summary['plan']]
    for item, result in zip(summary['plan'], results):
        if item['version'] == 16:
            result['evaluation_geometry'] = source_geometry(args.root, item, result['first_choice'])
        assert result['trace_sha256'] == expected[result['name']], 'trace differs from verified replay'
    for seat in [1, 2]:
        for bearing in ['0.8', '1.2']:
            group = [r for r in results if r['name'].startswith(f'destination-p{seat}-bearing{bearing}-')]
            assert {r['version'] for r in group} == {14, 15, 16}
            assert all(r['arrival_motion'] == group[0]['arrival_motion'] for r in group), 'different arrival state'
    result = dict(schema=1, source_commit=summary['source_commit'], binary_sha256=summary['binary_sha256'],
        runner_sha256=digest(Path(__file__)), replay_summary_sha256=digest(args.root / 'summary.json'),
        scope='All four changed directed neutral fixtures, v14/v15/v16. Dense native observations share the same ship and planet motion at first site selection. Phases use controller epochs, not the world-step trace label. Surface begins final landing control; it does not mean touchdown. No causal counterfactual or general match-strength claim.',
        cases=results)
    args.out.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')


if __name__ == '__main__':
    main()
