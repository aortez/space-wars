#!/usr/bin/env python3
"""Replay recorded cover failures and distinguish ranking from missing route evidence."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import subprocess


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


V = module('visits', 'validate-visit-metrics.py')
C = module('choices', 'compare-landing-choices.py')
F = V.F
CASES = [
    ('directed-failure', 'regressions', 'value-destination-p1-bearing-0.8-candidate', 0, [3270, 3930, 4080]),
    ('directed-control', 'regressions', 'value-destination-p1-bearing0.8-candidate', 0, [3510]),
    ('recorded-asteroids', 'regressions', 'world3-asteroids3-p2-candidate', 1, [2475, 2895, 9383, 10875]),
    ('fresh-p1', 'runs', 'world3-asteroids3-p1', 0, [9750]),
    ('fresh-p2', 'runs', 'world3-asteroids3-p2', 1, [13365]),
]


def identity(site):
    return site['planet'], site['bearing']


def route_cost(route):
    """JointRoundTrip only; missing/one-way/partial routes are not access."""
    assert 'crossing' not in route
    legs = [route['outbound'], route['returning']]
    if any(r is None or r['failure'] is not None or r['partial']
           or not math.isfinite(r['length']) or r['length'] < 0
           or r['start_node'] is None or r['reachable_nodes'] == 0
           or r['destination_nodes'] == 0 or r['jumps'] > 512 or r['flights'] != 0 for r in legs):
        return None
    return C.f32(C.f32(C.f32(sum(r['length'] for r in legs)) * C.f32(7.6))
                 + C.f32(sum(r['jumps'] for r in legs) * 15))


def audit_sample(row):
    o = row['observation']['local']
    p = o['combat']['recovery']['flight']['pilot']
    tick = p['tick']
    assert row['world_tick'] == tick and p['owner'] == f"player_{row['seat']+1}"
    assert 0 <= tick - row['loop_tick'] <= 1
    sites = {identity(s['id']): s for s in p['sites']}
    cover = {identity(c['site']): c for c in o['cover']}
    assert len(sites) == len(p['sites']) <= 64 and len(cover) == len(o['cover']) <= 64
    assert all(s['id']['planet'] == p['planet']['index'] for s in sites.values())
    for key in ['choice_ms', 'routes_ms']:
        assert math.isfinite(row[key]) and row[key] >= 0
    batches = row['route_batches']
    costs, routes = {}, {}
    if batches is None:
        assert row['routes_unknown'] and not row['route_costs']
    else:
        assert row['routes_unknown'] is None and p['site_query'] == 'survey'
        assert 1 <= len(batches) <= 8
        assert len(row['route_costs']) == len(sites)
        costs = {identity(r['site']): r['cost'] for r in row['route_costs']}
        for batch in batches:
            assert batch['actor'] == p['owner'] and batch['tick'] == tick
            assert batch['planning'] == 'joint_round_trip' and batch['version'] == 1
            assert 1 <= len(batch['sites']) <= 8
            assert batch['objective'] == batches[0]['objective']
            for route in batch['sites']:
                key = identity(route['site'])
                assert key not in routes
                routes[key] = route
                expected = route_cost(route)
                assert costs[key] == expected
        assert routes.keys() == costs.keys() == sites.keys(), 'incomplete all-site measurement'
        if o['landing_objective'] is not None:
            native = o['landing_objective']
            assert native['tick'] == tick and native['objective'] == batches[0]['objective']
            for route in native['sites']:
                assert routes[identity(route['site'])] == route, 'diagnostic changed a native route'

    choice = row['choice']
    if choice is not None:
        assert row['choice_unknown'] is None
        capture = row['mission']['capture']
        a = capture['acquisition']
        assert a['reason'] == 'selected_site' and a['tick'] == choice['tick'] == tick
        assert a['checks'] == choice['checks'] and a['selected_site'] == choice['selected']['site'] == capture['site']
        assert choice['site_count'] == a['sites_available'] == len(sites)
        assert choice['required_site'] == a['required_site']
        assert choice['objective'] == a['objective']
        assert choice['planet'] == p['planet']['index'] and choice['revision'] == p['planet']['revision']
        target = o['combat']['target']
        exposed = bool(target and not target['ground_occluded'] and
                       C.length(C.sub(C.vec(target['motion']['position']), C.vec(p['ship']['position']))) < 300)
        assert choice['exposed'] == exposed and choice['commit_descent']
        eligible = []
        for assessment in choice['assessments']:
            site = p['sites'][assessment['site_order']]
            assert assessment['site'] == site['id'] and assessment['revision'] == site['revision']
            if assessment['rejection'] is not None:
                continue
            eligible.append(assessment)
            c = cover.get(identity(site['id']))
            penalty = 0 if not exposed or c and all(c[k] for k in ['grounded', 'approach', 'departure']) else 400
            penalty *= 10 if choice['objective'] is not None else 1
            assert assessment['cover_penalty'] == penalty
            if choice['objective'] is None:
                assert assessment['ground_score'] == 0
            elif costs:
                assert assessment['ground_score'] == costs[identity(site['id'])]
            score = C.f32(C.f32(assessment['approach_score'] + penalty) + assessment['ground_score'])
            assert assessment['total_score'] == score
        assert len(eligible) == a['checks']['eligible']
        assert choice['selected'] == min(eligible, key=lambda v: v['total_score'])
    else:
        assert row['choice_unknown']

    records = []
    native_routes = {identity(r['site']): r for r in (o['landing_objective'] or {}).get('sites', [])}
    for key, site in sites.items():
        c = cover.get(key)
        assessments = [a for a in choice['assessments'] if identity(a['site']) == key] if choice else []
        native_eligible = [a for a in assessments if a['rejection'] is None]
        records.append(dict(site=site['id'], cover=c,
            approach_covered=None if c is None else c['grounded'] and c['approach'],
            full_cover=None if c is None else all(c[k] for k in ['grounded', 'approach', 'departure']),
            native_route_measured=key in native_routes,
            diagnostic_route_measured=key in routes, diagnostic_route_cost=costs.get(key),
            diagnostic_outbound_failure=routes[key]['outbound']['failure'] if key in routes else None,
            diagnostic_return_failure=(routes[key]['returning'] or {}).get('failure') if key in routes else None,
            native_eligible_directions=len(native_eligible) if choice else None,
            native_best_score=min((a['total_score'] for a in native_eligible), default=None),
            native_rejections=[a['rejection'] for a in assessments if a['rejection'] is not None]))
    sheltered = [r for r in records if r['approach_covered']]
    eligible_cover = [r['site'] for r in sheltered if r['native_eligible_directions']]
    omitted_cover = [r['site'] for r in sheltered if r['diagnostic_route_cost'] is not None
                     and 'route_absent' in r['native_rejections']]
    return dict(world_tick=tick, loop_tick=row['loop_tick'], seat=row['seat'],
        started_tick=(row['mission']['capture'] or {}).get('started_tick'),
        exposed=choice['exposed'] if choice else None,
        selected=choice['selected'] if choice else None,
        choice_unknown=row['choice_unknown'], routes_unknown=row['routes_unknown'],
        sites=records, counts=dict(observed_sites=len(records), approach_covered=len(sheltered),
            native_eligible_covered=len(eligible_cover) if choice else None,
            diagnostic_covered_round_trips=sum(r['diagnostic_route_cost'] is not None for r in sheltered) if batches is not None else None,
            omitted_covered_round_trips=len(omitted_cover) if batches is not None and choice else None,
            missing_cover=sum(r['cover'] is None for r in records)),
        native_eligible_covered=eligible_cover, omitted_covered_round_trips=omitted_cover)


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
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, plan=CASES, runs={},
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_summary_sha256=F.E.digest(source), binary_sha256=F.E.digest(binary),
        runner_sha256=F.E.digest(Path(__file__)),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [V, C, F, V.A, F.E, F.D]},
        scope='Ten diagnostic replays of recorded cases, twenty fixed-clock samples; not a new policy or strength trial. Approach-covered means grounded and approach cover, with departure and low-altitude exceptions recorded separately in raw observations. Unmeasured sites, future cover and physical execution remain unknown.')
    save = lambda: F.D.write(args.out/'summary.json', result)
    save()
    try:
        for label, section, source_name, seat, ticks in CASES:
            for arm in ['predecessor', 'candidate']:
                old = previous[section][source_name][arm] if section == 'regressions' else previous[section][source_name+'-'+arm]
                command = list(old['command'])
                old_root = Path(command[command.index('--out')+1])
                for filename, digest in old['hashes'].items():
                    assert F.E.digest(old_root/filename) == digest
                root = args.out/(label+'-'+arm)
                command[0] = str(binary)
                command[command.index('--out')+1] = str(root)
                if '--trace' not in command:
                    command += ['--trace', 'true']
                command += ['--probe-cover-seat', str(seat), '--probe-cover-ticks', ','.join(map(str, ticks))]
                result['runs'][root.name] = dict(command=command, source_name=source_name, source_arm=arm)
                save()
                print(root.name, flush=True)
                with (args.out/(root.name+'.log')).open('w') as log:
                    subprocess.run(command, check=True, stdout=log, stderr=log, timeout=900)
                before, after = [json.loads((r/'report.json').read_text()) for r in [old_root, root]]
                assert after['physics_ok']
                for field in V.EXACT_REPORT_FIELDS+['metrics', 'policy_configuration']:
                    assert before[field] == after[field], (root.name, field)
                streams = {}
                for filename in V.EXACT_STREAMS:
                    if filename == 'trace.jsonl' and not (old_root/filename).exists():
                        continue
                    digest = F.E.digest(root/filename)
                    assert digest == F.E.digest(old_root/filename), (root.name, filename)
                    streams[filename] = digest
                sensors = C.audit_sensors(old_root, root)
                allocation = F.allocation_audit(root)
                assert allocation == F.allocation_audit(old_root)
                probe = json.loads((root/'cover-probe.json').read_text())
                assert probe['requested_world_ticks'] == ticks and not probe['unreached_world_ticks']
                assert [r['world_tick'] for r in probe['rows']] == ticks
                samples = [audit_sample(row) for row in probe['rows']]
                witnessed = {}
                for row in F.rows(root/'trace.jsonl'):
                    tick = row['observation']['local']['combat']['recovery']['flight']['pilot']['tick']
                    if row['seat'] == seat and tick in ticks:
                        assert tick not in witnessed
                        witnessed[tick] = row
                for raw, sample in zip(probe['rows'], samples):
                    row = witnessed.get(sample['world_tick'])
                    sample['trace_witnessed'] = row is not None
                    if row is not None:
                        assert raw['observation'] == row['observation'] and raw['mission'] == row['mission']
                        assert raw['loop_tick'] == row['tick']
                    if sample['selected'] is not None:
                        assert row is not None, 'native choice has no matching immutable trace observation'
                run = result['runs'][root.name]
                visit_audit = V.A.audit(after)['counts']
                assert not any(visit_audit.get(k, 0) for k in ['unverified', 'corrected_visits', 'milestones_outside_visit'])
                run.update(report_sha256=F.E.digest(root/'report.json'),
                    probe_sha256=F.E.digest(root/'cover-probe.json'),
                    trace_sha256=F.E.digest(root/'trace.jsonl'), unchanged_stream_sha256=streams,
                    unchanged_report_fields=V.EXACT_REPORT_FIELDS+['metrics', 'policy_configuration'],
                    sensor_parity=sensors, allocation=allocation,
                    visit_audit=visit_audit, samples=samples,
                    hashes={p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()},
                    probe_timings=[dict(world_tick=r['world_tick'], choice_ms=r['choice_ms'],
                        routes_ms=r['routes_ms'], route_profile=r['route_profile']) for r in probe['rows']])
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
