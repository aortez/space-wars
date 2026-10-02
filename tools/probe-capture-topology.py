#!/usr/bin/env python3
"""Replay remaining enemy approaches and retain native route graph witnesses."""
import argparse
from collections import Counter
import importlib.util
import json
import math
from pathlib import Path
import subprocess


spec = importlib.util.spec_from_file_location('cover_probe', Path(__file__).with_name('probe-cover-alternatives.py'))
P = importlib.util.module_from_spec(spec)
spec.loader.exec_module(P)
V, C, F = P.V, P.C, P.F
CASES = [
    ('failure-cover-on', 'recorded-directed-failure-cover-candidate', [3270, 3930, 8580, 9030]),
    ('failure-cover-off', 'recorded-directed-failure-cover-predecessor', [9270, 9720]),
    ('successful-control', 'recorded-directed-control-cover-candidate', [3510]),
]


def reachable(start, adjacency):
    pending, seen = [start], set()
    while pending:
        node = pending.pop()
        if node not in seen:
            seen.add(node)
            pending.extend(adjacency[node] - seen)
    return seen


def components(nodes, edges):
    """Directed strong components; a one-way connection cannot certify return."""
    forward = {n: set() for n in nodes}
    reverse = {n: set() for n in nodes}
    for edge in edges:
        a, b = edge['from'], edge['to']
        assert a in nodes and b in nodes
        forward[a].add(b)
        reverse[b].add(a)
    pending, result = set(nodes), []
    while pending:
        start = min(pending)
        group = reachable(start, forward) & reachable(start, reverse)
        result.append(sorted(group))
        pending -= group
    return result


def audit_paths(trip, nodes, edges):
    lookup = {(e['from'], e['to']): e for e in edges}
    assert len(lookup) == len(edges)
    route = trip['route']
    for name in ['outbound', 'returning']:
        path, leg = trip[name+'_path'], route[name]
        if leg is None:
            assert path is None
            continue
        assert path is not None and len(path) == len(set(path))
        assert all(n in nodes for n in path)
        path_edges = [lookup[a, b] for a, b in zip(path, path[1:])]
        if leg['failure'] is None:
            assert path and path[0] == leg['start_node']
            assert math.isclose(sum(e['length'] for e in path_edges), leg['length'], rel_tol=1e-5, abs_tol=1e-4)
            assert sum(e['kind'] == 'jump' for e in path_edges) == leg['jumps']
            assert sum(e['kind'] == 'jetpack' for e in path_edges) == leg['flights'] == 0
    if P.route_cost(route) is not None:
        endpoint = route['endpoint']['id']
        assert trip['outbound_path'][-1] == trip['returning_path'][0] == endpoint


def audit_topology(row):
    result = P.audit_sample(row)
    topology = row['topology']
    if topology is None:
        assert row['topology_unknown']
        result['topology_unknown'] = row['topology_unknown']
        return result
    assert row['topology_unknown'] is None
    assert math.isfinite(row['topology_ms']) and row['topology_ms'] >= 0
    base = topology['base']
    p = row['observation']['local']['combat']['recovery']['flight']['pilot']
    assert (base['actor'], base['planet'], base['revision'], base['tick'], base['version']) == (
        p['owner'], p['planet']['index'], p['planet']['revision'], p['tick'], 1)
    assert math.isfinite(topology['gravity']) and topology['gravity'] > 0
    native = {P.identity(r['site']): r for batch in row['route_batches'] for r in batch['sites']}
    assert topology['objective'] == row['route_batches'][0]['objective']
    sites = {P.identity(s['site']): s for s in topology['sites']}
    assert len(sites) == len(topology['sites']) and sites.keys() == native.keys()
    nodes = {n['id'] for n in base['nodes']}
    rejected = {n['id'] for n in base['rejected']}
    assert len(nodes) == len(base['nodes']) and len(rejected) == len(base['rejected'])
    assert not nodes & rejected and nodes | rejected == set(range(512))
    groups = components(nodes, base['edges'])
    component = {n: i for i, group in enumerate(groups) for n in group}
    result['base_graph'] = dict(nodes=len(nodes), edges=len(base['edges']),
        components=groups, rejected_by_reason=dict(Counter(n['reason'] for n in base['rejected'])))
    for site_record in result['sites']:
        site = sites[P.identity(site_record['site'])]
        assert site['with_ship']['route'] == native[P.identity(site['site'])], 'topology changed native route'
        removed_nodes, removed_edges = set(site['removed_nodes']), set(site['removed_edges'])
        assert len(removed_nodes) == len(site['removed_nodes']) and removed_nodes <= nodes
        assert len(removed_edges) == len(site['removed_edges']) and removed_edges <= set(range(len(base['edges'])))
        kept_nodes = nodes - removed_nodes
        kept_edges = [e for i, e in enumerate(base['edges']) if i not in removed_edges]
        assert all(e['from'] in kept_nodes and e['to'] in kept_nodes for e in kept_edges)
        audit_paths(site['with_ship'], kept_nodes, kept_edges)
        audit_paths(site['without_ship'], nodes, base['edges'])
        base_route = site['without_ship']['route']
        assert base_route['site'] == site['site']
        site_record['topology'] = dict(removed_nodes=site['removed_nodes'],
            removed_edges=len(removed_edges), without_ship_cost=P.route_cost(base_route),
            without_ship_outbound=base_route['outbound'], without_ship_returning=base_route['returning'],
            base_start_component=component.get(base_route['outbound']['start_node']))
    result['counts']['round_trips_without_own_ship'] = sum(
        r['topology']['without_ship_cost'] is not None for r in result['sites'])
    result['counts']['covered_round_trips_without_own_ship'] = sum(
        r['approach_covered'] and r['topology']['without_ship_cost'] is not None for r in result['sites'])
    result['counts']['hull_breaks_round_trip'] = sum(
        r['topology']['without_ship_cost'] is not None and r['diagnostic_route_cost'] is None for r in result['sites'])
    return result


def search_history(root, seat):
    searches = {}
    for row in F.rows(root/'trace.jsonl'):
        if row['seat'] != seat:
            continue
        c = (row['mission'] or {}).get('capture') or {}
        state = c.get('cover_response') or {}
        search = state.get('search')
        if not search:
            continue
        key = search['started_tick']
        record = searches.setdefault(key, dict(capture_started=c['started_tick'], search_started=key,
            probes=[], last_search=None, failure=None))
        record['last_search'] = search
        record['failure'] = c['failure']
        p = row['observation']['local']['combat']['recovery']['flight']['pilot']
        survey = row['observation']['local']['landing_objective']
        if isinstance(p['site_query'], dict) and 'selected' in p['site_query'] and survey:
            for route in survey['sites']:
                if route['site'] == p['site_query']['selected']:
                    entry = dict(world_tick=p['tick'], route=route)
                    if entry not in record['probes']:
                        record['probes'].append(entry)
    return list(searches.values())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--study', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip(), 'freeze diagnostic and plan first'
    source = args.study/'summary.json'
    previous = json.loads(source.read_text())
    assert previous['complete']
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, plan=CASES, runs={},
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_summary_sha256=F.E.digest(source), binary_sha256=F.E.digest(binary),
        runner_sha256=F.E.digest(Path(__file__)),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [P, V, C, F, V.A, F.E, F.D]},
        scope='Three recorded replays, seven fixed-clock observations. Graphs describe native bounded outer-contour walk/jump measurements. Without-ship routes are counterfactual, not physical capture evidence. Extra diagnostics do not enter live controls or quotas. No policy change or strength trial.')
    save = lambda: F.D.write(args.out/'summary.json', result)
    save()
    try:
        for label, source_name, ticks in CASES:
            old = previous['regressions'][source_name]['candidate']
            command = list(old['command'])
            old_root = Path(command[command.index('--out')+1])
            for filename, digest in old['hashes'].items():
                assert F.E.digest(old_root/filename) == digest
            root = args.out/label
            command[0] = str(binary)
            command[command.index('--out')+1] = str(root)
            command += ['--probe-cover-seat', '0', '--probe-cover-ticks', ','.join(map(str, ticks)), '--probe-cover-topology', 'true']
            result['runs'][label] = dict(command=command, source_name=source_name, source_arm='candidate')
            save()
            print(label, flush=True)
            with (args.out/(label+'.log')).open('w') as log:
                subprocess.run(command, check=True, stdout=log, stderr=log, timeout=900)
            before, after = [json.loads((r/'report.json').read_text()) for r in [old_root, root]]
            assert after['physics_ok']
            fields = V.EXACT_REPORT_FIELDS+['metrics', 'policy_configuration', 'cover_response', 'destination_retry']
            for field in fields:
                assert (field in before) == (field in after) and before.get(field) == after.get(field), (label, field)
            streams = {}
            for filename in V.EXACT_STREAMS:
                digest = F.E.digest(root/filename)
                assert digest == F.E.digest(old_root/filename), (label, filename)
                streams[filename] = digest
            sensors = C.audit_sensors(old_root, root)
            allocation = F.allocation_audit(root)
            assert allocation == F.allocation_audit(old_root)
            probe = json.loads((root/'cover-probe.json').read_text())
            assert probe['requested_world_ticks'] == ticks and not probe['unreached_world_ticks']
            assert [r['world_tick'] for r in probe['rows']] == ticks
            samples = [audit_topology(r) for r in probe['rows']]
            trace = {r['observation']['local']['combat']['recovery']['flight']['pilot']['tick']: r
                     for r in F.rows(root/'trace.jsonl') if r['seat'] == 0}
            for row in probe['rows']:
                witness = trace[row['world_tick']]
                assert row['observation'] == witness['observation'] and row['mission'] == witness['mission']
                assert row['loop_tick'] == witness['tick']
            result['runs'][label].update(samples=samples, searches=search_history(root, 0),
                unchanged_stream_sha256=streams, unchanged_report_fields=fields,
                sensor_parity=sensors, allocation=allocation,
                hashes={p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()},
                timings=[{k: r[k] for k in ['world_tick', 'choice_ms', 'routes_ms', 'topology_ms', 'route_profile', 'topology_profile']} for r in probe['rows']])
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
