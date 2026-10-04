#!/usr/bin/env python3
"""Test the existing powered landing forecast at the frozen topology observations."""
import argparse
from collections import Counter
import importlib.util
import json
import math
from pathlib import Path
import subprocess

spec = importlib.util.spec_from_file_location('topology', Path(__file__).with_name('probe-capture-topology.py'))
T = importlib.util.module_from_spec(spec)
spec.loader.exec_module(T)
P, V, C, F = T.P, T.V, T.C, T.F


def vector(v):
    return v['x'], v['y']


def audit_crossing(c, row, site):
    p = row['observation']['local']['combat']['recovery']['flight']['pilot']
    plan = c['plan']
    assert c['version'] == 2 and c['measured_tick'] == p['tick']
    assert 0 <= c['launch_until_tick']-c['measured_tick'] <= 120
    assert plan['planet'] == p['planet']['index'] and plan['revision'] == p['planet']['revision']
    assert plan['direction'] in ['Left', 'Right']
    points = [vector(plan[k]) for k in ['start', 'destination']]
    assert all(math.isfinite(v) for point in points for v in point)
    assert 1 < math.dist(*points) < 30
    assert max(math.hypot(*v) for v in points) < plan['cruise_radius'] < min(math.hypot(*v) for v in points)+20
    anchor = plan['anchor']['Vehicle']
    assert anchor['form'] == 'ship' and anchor['index'] == p['vehicle']
    assert math.isfinite(anchor['angle']) and all(math.isfinite(v) for v in vector(anchor['position']))
    physical = next(s for s in p['sites'] if s['id'] == site['site'])
    frame = p['planet']['motion']
    dx, dy = (physical['vehicle_position'][k]-frame['position'][k] for k in ['x', 'y'])
    co, si = math.cos(frame['angle']), math.sin(frame['angle'])
    assert math.dist(vector(anchor['position']), (dx*co+dy*si, -dx*si+dy*co)) < 0.001
    assert len(c['nodes']) == len(set(c['nodes'])) == 2
    nodes = {n['id']: n for n in row['topology']['base']['nodes'] if n['id'] not in site['removed_nodes']}
    for node, point in zip(c['nodes'], points):
        assert node in nodes and math.dist(vector(nodes[node]['position']), point) < 0.01
    assert len(c['flights']) == 2
    for flight in c['flights']:
        assert all(math.isfinite(v) for v in flight.values())
        assert 0 < flight['seconds'] <= 12.0 and 0 <= flight['arrival_speed'] <= 7
        assert 0 <= flight['burn_seconds'] <= (0.98-0.05)*3.0+1e-6


def expected_cost(route):
    crossing = route.get('crossing')
    legs = [route['outbound'], route['returning']]
    if any(r is None or r['failure'] is not None or r['partial'] or not math.isfinite(r['length'])
           or r['length'] < 0 or r['start_node'] is None or r['reachable_nodes'] == 0
           or r['destination_nodes'] == 0 or not 0 <= r['jumps'] <= 512
           or not 0 <= r['flights'] <= int(crossing is not None) for r in legs):
        return None
    cost = sum(r['length'] for r in legs)*7.6+sum(r['jumps'] for r in legs)*15
    if crossing:
        distance = math.dist(vector(crossing['plan']['start']), vector(crossing['plan']['destination']))
        extra = max((max(f['seconds'] for f in crossing['flights'])+4.0)*38-distance*7.6, 0)
        cost += sum(r['flights'] for r in legs)*extra
    return cost


def audit_measurement(m):
    route, f, measurements = m['route'], m['forecasts'], m['measurements']
    cost = expected_cost(route)
    if cost is None:
        assert m['cost'] is None
    else:
        assert m['cost'] is not None and math.isfinite(m['cost'])
        assert math.isclose(m['cost'], cost, rel_tol=1e-5, abs_tol=1e-4)
        assert route.get('endpoint') is not None
    assert all(isinstance(v, int) and v > 0 for v in m['work'].values())
    assert 0 <= f['started'] <= 1 and f['started'] == f['approved']+sum(f['rejected'].values())
    assert all(isinstance(v, int) and v > 0 for v in f['rejected'].values())
    assert f['approved'] == int(route.get('crossing') is not None)
    assert measurements['finished_surveys'] == measurements['finished_candidates'] == 1
    assert measurements['successful_candidates'] == int(cost is not None)
    assert measurements['powered_candidates'] == int(cost is not None and route.get('crossing') is not None)
    assert sum(measurements['failures'].values()) == int(cost is None)


def audit_sample(row):
    result = T.audit_topology(row)
    diagnostic = row['jetpack']
    if diagnostic is None:
        assert row['jetpack_unknown']
        result['jetpack_unknown'] = row['jetpack_unknown']
        return result
    assert row['jetpack_unknown'] is None and row['topology'] is not None
    assert math.isfinite(row['jetpack_ms']) and row['jetpack_ms'] >= 0
    p = row['observation']['local']['combat']['recovery']['flight']['pilot']
    walking = {P.identity(r['site']): r for b in row['route_batches'] for r in b['sites']}
    powered = {}
    assert 1 <= len(diagnostic['batches']) <= 8
    for batch in diagnostic['batches']:
        assert batch['version'] == 1 and batch['planning'] == 'jetpack_round_trip'
        assert batch['actor'] == p['owner'] and batch['tick'] == p['tick']
        assert batch['objective'] == row['topology']['objective'] and batch['actual'] is None
        assert 1 <= len(batch['sites']) <= 8
        for route in batch['sites']:
            key = P.identity(route['site'])
            assert key not in powered
            powered[key] = route
    pairs = {P.identity(s['site']): s for s in diagnostic['sites']}
    assert len(pairs) == len(diagnostic['sites']) and pairs.keys() == powered.keys() == walking.keys()
    topology = {P.identity(s['site']): s for s in row['topology']['sites']}
    sites, reasons = [], Counter()
    for base in result['sites']:
        key = P.identity(base['site'])
        pair = pairs[key]
        walk, flight = pair['walking'], pair['jetpack']
        assert walk['route'] == walking[key] and flight['route'] == powered[key]
        for m in [walk, flight]:
            audit_measurement(m)
        assert walk['forecasts']['started'] == 0 and walk['route'].get('crossing') is None
        if not diagnostic['equipped']:
            assert walk['route'] == flight['route'] and flight['forecasts']['started'] == 0
        if flight['route'].get('crossing'):
            assert diagnostic['equipped']
            audit_crossing(flight['route']['crossing'], row, topology[key])
        if walk['cost'] is not None:
            assert walk['route'] == flight['route'] and flight['forecasts']['started'] == 0
            outcome = 'unchanged_walk'
        elif flight['cost'] is not None:
            assert flight['route'].get('crossing') is not None
            assert flight['route']['outbound']['flights']+flight['route']['returning']['flights'] > 0
            outcome = 'powered_round_trip'
        elif flight['forecasts']['rejected']:
            outcome = 'forecast_'+next(iter(flight['forecasts']['rejected']))
        else:
            outcome = 'no_powered_forecast_started'
        reasons[outcome] += 1
        sites.append(dict(site=base['site'], approach_covered=base['approach_covered'],
            native_shortlist=base['native_route_measured'], outcome=outcome,
            walking_cost=walk['cost'], powered_cost=flight['cost'],
            walking_work=walk['work'], powered_work=flight['work'],
            forecasts=flight['forecasts'], crossing=flight['route'].get('crossing')))
    result['jetpack'] = dict(equipped=diagnostic['equipped'], outcomes=dict(reasons), sites=sites,
        newly_reachable_covered=[s['site'] for s in sites if s['approach_covered'] and s['outcome'] == 'powered_round_trip'],
        newly_reachable_covered_shortlisted=[s['site'] for s in sites if s['approach_covered'] and s['native_shortlist'] and s['outcome'] == 'powered_round_trip'])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--study', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip(), 'freeze probe and plan first'
    source = args.study/'summary.json'
    previous = json.loads(source.read_text())
    assert previous['complete']
    assert previous['plan'] == [list(case) for case in T.CASES]
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, plan=previous['plan'], runs={},
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_summary_sha256=F.E.digest(source), binary_sha256=F.E.digest(binary),
        runner_sha256=F.E.digest(Path(__file__)),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [T, P, V, C, F, V.A, F.E, F.D]},
        scope='Three unchanged recorded replays and seven frozen observations. Existing native powered forecasts and paired single-site jobs are measured offline, with the actual proposed hull and unchanged thresholds. This supplies no physical execution, future cover, strength or live-budget guarantee.')
    save = lambda: F.D.write(args.out/'summary.json', result)
    save()
    try:
        for label, _, ticks in T.CASES:
            old = previous['runs'][label]
            command = list(old['command'])
            old_root = Path(command[command.index('--out')+1])
            for filename, digest in old['hashes'].items():
                assert F.E.digest(old_root/filename) == digest, (label, filename)
            root = args.out/label
            command[0] = str(binary)
            command[command.index('--out')+1] = str(root)
            command += ['--probe-cover-jetpack', 'true']
            result['runs'][label] = dict(command=command, source_path=str(old_root))
            save()
            print(label, flush=True)
            with (args.out/(label+'.log')).open('w') as log:
                subprocess.run(command, check=True, stdout=log, stderr=log, timeout=900)
            before, after = [json.loads((r/'report.json').read_text()) for r in [old_root, root]]
            assert after['physics_ok']
            fields = V.EXACT_REPORT_FIELDS+['metrics', 'policy_configuration', 'cover_response', 'destination_retry']
            T.report_parity(before, after, fields)
            streams = {}
            for filename in V.EXACT_STREAMS:
                digest = F.E.digest(root/filename)
                assert digest == F.E.digest(old_root/filename), (label, filename)
                streams[filename] = digest
            sensors = C.audit_sensors(old_root, root)
            allocation = F.allocation_audit(root)
            assert allocation == F.allocation_audit(old_root)
            prior, probe = [json.loads((r/'cover-probe.json').read_text()) for r in [old_root, root]]
            assert probe['jetpack_model'] == 'existing_powered_landing_probe_v1'
            assert probe['requested_world_ticks'] == ticks and not probe['unreached_world_ticks']
            assert [r['world_tick'] for r in probe['rows']] == ticks
            samples = []
            for before, row in zip(prior['rows'], probe['rows']):
                existing = {k: v for k, v in row.items() if not k.startswith('jetpack')}
                assert C.without_wall_times(before) == C.without_wall_times(existing), 'existing diagnostic changed'
                samples.append(audit_sample(row))
            assert len(samples) == len(ticks)
            trace = {r['observation']['local']['combat']['recovery']['flight']['pilot']['tick']: r
                     for r in F.rows(root/'trace.jsonl') if r['seat'] == 0}
            for row in probe['rows']:
                witness = trace[row['world_tick']]
                assert row['observation'] == witness['observation'] and row['mission'] == witness['mission']
                assert row['loop_tick'] == witness['tick']
            result['runs'][label].update(samples=samples,
                unchanged_stream_sha256=streams, unchanged_report_fields=fields,
                sensor_parity=sensors, allocation=allocation,
                hashes={p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()},
                timings=[{k: r[k] for k in ['world_tick', 'jetpack_ms', 'jetpack_profile']} for r in probe['rows']])
            save()
        assert F.E.digest(binary) == result['binary_sha256']
        result['complete'] = True
        save()
    except Exception as error:
        result['error'] = repr(error)
        save()
        raise


if __name__ == '__main__':
    main()
