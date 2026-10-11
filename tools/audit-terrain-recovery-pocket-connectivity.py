#!/usr/bin/env python3
"""Independently audit native barrier witnesses without running a simulation."""
import argparse
from collections import deque
import hashlib
import json
import math
from pathlib import Path
import subprocess
import tarfile


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(1024*1024), b''):
            h.update(block)
    return h.hexdigest()


def read(path):
    return json.loads(path.read_text())


def rows(path):
    with path.open() as f:
        for line in f:
            yield json.loads(line)


def flood(side, blocked):
    source = (side//2, side//2)
    if source in blocked:
        return set(), False
    seen, queue, boundary = {source}, deque([source]), False
    while queue:
        x, y = queue.popleft()
        boundary |= x in (0, side-1) or y in (0, side-1)
        for ny in range(max(0, y-1), min(side, y+2)):
            for nx in range(max(0, x-1), min(side, x+2)):
                p = (nx, ny)
                if p not in blocked and p not in seen:
                    seen.add(p)
                    queue.append(p)
    return seen, boundary


def audit_grid(grid, diagnostic):
    g, origin = grid['grid'], grid['origin']
    side = g['half_cells']*2+1
    spacing, angle = g['spacing'], g['angle']
    assert side in (121, 241, 481)
    assert abs(spacing*g['half_cells']-6) < 1e-5
    assert abs(g['penetration_allowance']-0.025) < 1e-8
    assert abs(grid['capsule_radius']-0.15) < 1e-6
    assert abs(grid['effective_radius']-(grid['capsule_radius']-0.025)) < 1e-6
    assert abs(grid['cell_circumradius']-spacing/math.sqrt(2)) < 1e-7
    assert grid['numerical_margin'] >= 0.002
    assert math.dist(origin.values(), diagnostic['actor']['position'].values()) < 1e-4
    data = grid['rows']
    assert len(data) == side and all(len(row) == side and set(row) <= set('Br.') for row in data)
    blocked = {(x, y) for y, row in enumerate(data) for x, c in enumerate(row) if c == 'B'}
    reached, boundary = flood(side, blocked)
    assert boundary == grid['reaches_boundary']
    assert grid['source_blocked'] == ((side//2, side//2) in blocked)
    assert reached == {(x, y) for y, row in enumerate(data) for x, c in enumerate(row) if c == 'r'}
    assert len(reached) == grid['reachable_cells']
    assert grid['native_queries'] == side*side
    witnesses = set()
    min_margin = math.inf
    for witness in grid['barrier']:
        x, y = witness['cell']
        assert (x, y) in blocked and (x, y) not in witnesses
        witnesses.add((x, y))
        dx, dy = (x-g['half_cells'])*spacing, (y-g['half_cells'])*spacing
        expected = (origin['x']+dx*math.cos(angle)-dy*math.sin(angle),
                    origin['y']+dx*math.sin(angle)+dy*math.cos(angle))
        center, solid = witness['center'], witness['solid_point']
        assert math.dist(expected, (center['x'], center['y'])) < 3e-4
        distance = math.hypot(center['x']-solid['x'], center['y']-solid['y'])
        assert abs(distance-witness['distance']) < 3e-5
        margin = grid['effective_radius']-distance-grid['cell_circumradius']-grid['numerical_margin']
        assert margin > 0
        assert witness['distance']+grid['cell_circumradius']+grid['numerical_margin'] < grid['effective_radius']
        min_margin = min(min_margin, margin)
        assert witness['collider']['entity'] == diagnostic['terrain_entity']
        assert witness['collider']['role'] >= diagnostic['terrain_first_chunk_role']
    expected_frontier = set()
    for x, y in reached:
        for ny in range(max(0, y-1), min(side, y+2)):
            for nx in range(max(0, x-1), min(side, x+2)):
                if (nx, ny) in blocked:
                    expected_frontier.add((nx, ny))
    assert witnesses == expected_frontier
    enclosed = not grid['source_blocked'] and not boundary
    if enclosed:
        independently_reached, independent_boundary = flood(side, witnesses)
        assert not independent_boundary and independently_reached == reached
    target_counts = {'source_component': 0, 'other_cell': 0, 'outside_grid': 0}
    nearest = math.inf
    for t in diagnostic['standing_targets']:
        p = t['standing_center']
        dx, dy = p['x']-origin['x'], p['y']-origin['y']
        nearest = min(nearest, math.hypot(dx, dy))
        x = math.floor((dx*math.cos(angle)+dy*math.sin(angle))/spacing+g['half_cells']+0.5)
        y = math.floor((-dx*math.sin(angle)+dy*math.cos(angle))/spacing+g['half_cells']+0.5)
        key = ('outside_grid' if not (0 <= x < side and 0 <= y < side)
               else 'source_component' if (x, y) in reached else 'other_cell')
        target_counts[key] += 1
    bounds = None
    if reached:
        bounds = [[(min(p[i] for p in reached)-g['half_cells']-0.5)*spacing,
                   (max(p[i] for p in reached)-g['half_cells']+0.5)*spacing] for i in (0, 1)]
    return {'spacing':spacing, 'enclosed':enclosed, 'source_blocked':grid['source_blocked'],
            'reaches_boundary':boundary, 'reachable_cells':len(reached), 'component_bounds':bounds,
            'barrier_witnesses':len(witnesses), 'minimum_extra_margin':min_margin if witnesses else None,
            'native_queries':side*side, 'standing_targets':target_counts,
            'nearest_standing_center':nearest if diagnostic['standing_targets'] else None}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    root, repo = args.root.resolve(), Path(__file__).resolve().parent.parent
    runtime, inputs = read(root/'runtime.json'), read(root/'source-inputs.json')
    assert digest(root/'pocket-connectivity-diagnostic') == runtime['binary_sha256']
    assert digest(root/'source-inputs.json') == runtime['source_inputs_sha256']
    process = subprocess.Popen(['git','archive',runtime['source_commit']], cwd=repo, stdout=subprocess.PIPE)
    checked = set()
    with tarfile.open(fileobj=process.stdout, mode='r|') as archive:
        for item in archive:
            if item.name in inputs:
                data = item.linkname.encode() if item.issym() else archive.extractfile(item).read()
                assert hashlib.sha256(data).hexdigest() == inputs[item.name]
                checked.add(item.name)
    assert process.wait() == 0 and checked == set(inputs)
    for name, sha in read(root/'references/provenance.json')['files'].items():
        assert digest(root/'references'/name) == sha
    results = read(root/'results.json')
    assert [r['fork_tick'] for r in results] == [8025, 8325]
    checkpoints, ticks, audit_samples = [], 0, 0
    for result in results:
        fork, end = result['fork_tick'], result['fork_tick']+481
        assert result['exit_code'] == 0 and result['binary_sha256'] == runtime['binary_sha256']
        assert result['reference_sha256'] == digest(root/'references/angular-footing/actions.jsonl')
        assert result['candidate_sha256'] == digest(root/'references'/str(fork)/'actions.jsonl')
        path = root/'cases'/str(fork)/'output'
        summary = read(path/'summary.json')
        assert (summary['fork_tick'],summary['end_tick']) == (fork,end)
        assert read(path/f'state-{end}.json') == read(root/'references'/str(fork)/'attempt-finished.json')
        assert summary['prefix_ticks_verified'] == fork and summary['candidate_decisions_verified'] == 482
        assert not any(summary[k] for k in ('controller_reset','new_policy','extra_simulated_continuation'))
        ticks += fork+482
        attempts = {r['tick']:r for r in rows(root/'references'/str(fork)/'rejoin-decisions.jsonl')}
        assert [c['tick'] for c in summary['checkpoints']] == [fork,end]
        for check in summary['checkpoints']:
            assert all(check[k] for k in ('matches_retained_physics','scenario_unchanged','controllers_unchanged','physics_unchanged'))
            tick = check['tick']
            diagnostic = read(path/f'connectivity-{tick}.json')
            assert diagnostic['tick'] == tick and diagnostic['physics_unchanged'] and not diagnostic['controller_input']
            assert diagnostic['physics_fingerprint'] == check['physics'] == attempts[tick]['physics']
            assert diagnostic['actor'] == attempts[tick]['pilot']['actor']
            assert diagnostic['revision'] == attempts[tick]['pilot']['planet']['revision']
            grids = [audit_grid(g, diagnostic) for g in diagnostic['grids']]
            assert all(abs(g['spacing']-expected) < 1e-8 for g,expected in zip(grids,[0.1,0.05,0.025])) and len(grids) == 3
            checkpoints.append({'fork':fork,'tick':tick,'revision':diagnostic['revision'],'physics':check['physics'],'grids':grids})
        samples = list(rows(path/'samples.jsonl'))
        assert [s['tick'] for s in samples] == list(range(60,end+1,60))
        for sample in samples:
            a = sample['audit']
            assert not a['issues'] and a['max_speed'] < 500
            assert a['occupied_cells']+a['loose_cells']+a['removed_cells'] == summary['initial_cells']
        audit_samples += len(samples)
    artifacts = read(root/'case-manifest.json')
    for name, sha in artifacts.items():
        assert digest(root/name) == sha
    report = {'schema':1,'source_inputs_checked':len(checked),'case_artifacts_checked':len(artifacts),
              'verified_decision_ticks':ticks,'material_audit_samples':audit_samples,
              'native_queries':sum(g['native_queries'] for c in checkpoints for g in c['grids']),
              'checkpoints':checkpoints,'passed':True}
    if args.output:
        args.output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))


if __name__ == '__main__':
    main()
