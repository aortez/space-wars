#!/usr/bin/env python3
"""Inspect the retained recovery stall without changing or rerunning a game."""
import argparse
import gzip
import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BUNDLE = 'docs/data/crossing-arrival-v1.json.gz'
FIXTURE = 'crates/spacewars-ai/tests/fixtures/recovery-route-stall.json'
CASE = 'new-world1-p2-asteroids3-no-stop'
TICKS = (22575, 22965, 23205, 23655)
NATIVE_PREFIX = 'RECOVERY_ROUTE_PROBE '
SOURCES = (
    'scenarios/spacewars/src/surface_sortie/jetpack.rs',
    'scenarios/spacewars/src/surface_sortie/ground_navigation.rs',
    'scenarios/spacewars/src/surface_sortie/ground_navigation/routes.rs',
    'scenarios/spacewars/src/surface_sortie/jetpack/forecast.rs',
    'crates/spacewars-ai/src/ground_task.rs',
    'crates/spacewars-ai/src/ground_task/jetpack.rs',
    'crates/spacewars-ai/src/recovery_task.rs',
)


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def write(path, value):
    Path(path).parent.mkdir(parents=True, exist_ok=True)
    Path(path).write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')


def source():
    manifest = json.loads((ROOT/'docs/data/crossing-arrival-v1.json').read_text())
    assert manifest['review_bundle']['path'] == BUNDLE
    assert digest(ROOT/BUNDLE) == manifest['review_bundle']['sha256']
    assert (ROOT/BUNDLE).stat().st_size == manifest['review_bundle']['bytes']
    with gzip.open(ROOT/BUNDLE, 'rt') as stream:
        data = json.load(stream)
    # This diagnosis uses the exact arrival candidate's runtime contracts.
    summary = data['summary']
    assert summary['complete']
    for path, sha in summary['inputs'].items():
        assert digest(ROOT/path) == sha, f'predecessor input changed: {path}'
    return data


def project(row):
    o = row['observation']['local']['combat']['recovery']
    task = row['mission']['recovery']['ground']
    assert row['seat'] == 1 and row['tick'] in TICKS
    assert o['ground']['tick'] == row['tick'] and o['jetpack']['surveyed']
    return dict(tick=row['tick'], pilot=o['flight']['pilot'], ground=o['ground'],
                jetpack=o['jetpack'], observed_route=task['route'],
                ground_started_tick=task['started_tick'], target=task['target'])


def trace_samples(trace, data):
    archive = data['summary']['runs'][CASE]['archive']
    assert digest(trace) == archive['files']['trace.jsonl']['sha256']
    assert Path(trace).stat().st_size == archive['files']['trace.jsonl']['bytes']
    samples = []
    with Path(trace).open() as stream:
        for line in stream:
            row = json.loads(line)
            if row['seat'] == 1 and row['tick'] in TICKS:
                samples.append(project(row))
    assert [s['tick'] for s in samples] == list(TICKS)
    return samples


def extract(trace, destination):
    data = source()
    archive = data['summary']['runs'][CASE]['archive']
    samples = trace_samples(trace, data)
    assert not Path(destination).exists(), 'do not overwrite the retained fixture'
    write(destination, dict(schema=1, source_bundle=BUNDLE, source_sha256=digest(ROOT/BUNDLE),
        source_run=CASE, seat=1, archive_sha256=archive['sha256'],
        trace=archive['files']['trace.jsonl'], samples=samples))


def vector(value):
    return value['x'], value['y']


def local(point, frame):
    x, y = (point[k]-frame['position'][k] for k in ('x', 'y'))
    c, s = math.cos(frame['angle']), math.sin(frame['angle'])
    return x*c+y*s, y*c-x*s


def geometry(sample):
    """Explain pre-clearance bounds, not physical flight feasibility.

    The source-bound survey examines eight nearest adjacent gaps. Each has
    eight symmetric endpoint margins and three possible cruise heights.
    A rejected height is never passed to the physical clearance query.
    """
    m, p = sample['ground'], sample['pilot']
    nodes = m['nodes']
    edges = {(e['from'], e['to']) for e in m['edges']}
    actor = local(p['actor']['position'], p['planet']['motion'])
    gaps = []
    for i, a in enumerate(nodes):
        b = nodes[(i+1) % len(nodes)]
        if (a['id'], b['id']) in edges and (b['id'], a['id']) in edges:
            continue
        if math.dist(vector(a['position']), vector(b['position'])) <= 12:
            gaps.append(i)
    gaps.sort(key=lambda i: (math.dist(vector(nodes[i]['position']), actor), i))
    index = next(i for i in gaps if (nodes[i]['id'], nodes[(i+1) % len(nodes)]['id']) == (276, 282))
    margin_rows = []
    for margin in range(min(8, len(nodes)//2)):
        a, b = nodes[(index-margin) % len(nodes)], nodes[(index+1+margin) % len(nodes)]
        radius_a, radius_b = math.hypot(*vector(a['position'])), math.hypot(*vector(b['position']))
        distance = math.dist(vector(a['position']), vector(b['position']))
        heights = []
        for height in (3, 5, 7):
            rise = abs(radius_a-radius_b)+height
            reason = ('endpoint_distance' if not 1 <= distance <= 24 else
                      'rise_over_10' if rise > 10 else 'needs_physical_clearance')
            heights.append(dict(height=height, required_rise=rise, result=reason))
        margin_rows.append(dict(margin=margin, from_node=a['id'], to_node=b['id'],
            from_radius=radius_a, to_radius=radius_b, endpoint_distance=distance, heights=heights))
    return dict(tick=sample['tick'], gap=[276, 282], gap_rank=gaps.index(index)+1,
        examined_in_first_eight=index in gaps[:8], endpoint_margins=margin_rows,
        pre_clearance_rejections=sum(h['result'] != 'needs_physical_clearance'
            for row in margin_rows for h in row['heights']),
        charge=sample['jetpack']['charge'],
        ground_budget_remaining_ticks=90*60-(sample['tick']-sample['ground_started_tick']))


def verify_fixture(fixture, data):
    assert fixture['schema'] == 1 and fixture['source_run'] == CASE and fixture['seat'] == 1
    assert fixture['source_bundle'] == BUNDLE and fixture['source_sha256'] == digest(ROOT/BUNDLE)
    archive = data['summary']['runs'][CASE]['archive']
    assert fixture['archive_sha256'] == archive['sha256']
    assert fixture['trace'] == archive['files']['trace.jsonl']
    assert [s['tick'] for s in fixture['samples']] == list(TICKS)
    for s in fixture['samples']:
        assert s['pilot']['tick'] == s['ground']['tick'] == s['tick']
        assert s['pilot']['owner'] == s['ground']['actor'] == 'player_2'
        assert s['pilot']['location'] == 'on_foot' and s['pilot']['queries_ready']
        assert s['ground']['planet'] == s['pilot']['planet']['index'] == 0
        assert s['ground']['revision'] == s['pilot']['planet']['revision'] == 21
        assert s['jetpack']['surveyed'] and len(s['jetpack']['terrain_crossings']) <= 8
        assert s['ground_started_tick'] == 18278


def native_results(log):
    lines = Path(log).read_text().splitlines()
    rows = [json.loads(line.split(NATIVE_PREFIX, 1)[1]) for line in lines if NATIVE_PREFIX in line]
    assert [row['tick'] for row in rows] == list(TICKS)
    results = [line for line in lines if line.startswith('test result:')]
    assert results and all(line.startswith('test result: ok.') and '; 0 failed;' in line
                           for line in results), 'native tests did not all pass'
    for row in rows:
        for route in ('ground_flag', 'combined_flag', 'ground_hatches', 'combined_hatches'):
            assert row[route]['path'] == [] and row[route]['diagnostics']['failure'] is not None
        assert row['synthetic_bridge_flag']['path']
        assert row['synthetic_bridge_flag']['diagnostics']['failure'] is None
        path = row['synthetic_bridge_flag']['path']
        assert any([a, b] == [276, 282] for a, b in zip(path, path[1:]))
    return rows


def analyze(fixture_path, trace, native_log, destination):
    data = source()
    fixture = json.loads(Path(fixture_path).read_text())
    verify_fixture(fixture, data)
    assert fixture['samples'] == trace_samples(trace, data), 'fixture differs from consumed observations'
    rows = native_results(native_log)
    g = [geometry(s) for s in fixture['samples']]
    assert all(r['examined_in_first_eight'] and r['pre_clearance_rejections'] == 24 for r in g)
    write(destination, dict(schema=1, decision='no_existing_measured_fallback',
        runtime_changed=False, fresh_games=0, replayed_games=0, default_promotion=False,
        fixture_sha256=digest(fixture_path), source_bundle_sha256=fixture['source_sha256'],
        verified_predecessor_inputs=len(data['summary']['inputs']),
        predecessor_runtime_source_commit=data['summary']['runtime_source_commit'],
        predecessor_binary_sha256=data['summary']['binary']['sha256'],
        verified_source_trace=fixture['trace'],
        native_log_sha256=digest(native_log),
        sources={path:digest(ROOT/path) for path in SOURCES},
        auditor_sha256=digest(__file__),
        native_test_sha256=digest(ROOT/'crates/spacewars-ai/tests/recovery_route_stall.rs'),
        geometry=g, native_routes=rows,
        limitations=['Physical clearance and powered flight above the existing envelope were not measured.',
                    'The synthetic edge is a graph diagnostic, not an authorized crossing.',
                    'No complete flag or pod route exists in these four consumed maps; unmeasured routes may exist.',
                    'The arrival candidate remains unqualified after its recorded survival regression.']))


if __name__ == '__main__':
    assert __debug__
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='action', required=True)
    p = sub.add_parser('extract')
    p.add_argument('--trace', type=Path, required=True)
    p.add_argument('--out', type=Path, default=ROOT/FIXTURE)
    p = sub.add_parser('analyze')
    p.add_argument('--fixture', type=Path, default=ROOT/FIXTURE)
    p.add_argument('--trace', type=Path, required=True)
    p.add_argument('--native-log', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if args.action == 'extract':
        extract(args.trace, args.out)
    else:
        analyze(args.fixture, args.trace, args.native_log, args.out)
