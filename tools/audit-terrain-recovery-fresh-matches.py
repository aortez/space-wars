#!/usr/bin/env python3
"""Verify frozen fresh combat matches without executing any simulation."""
import argparse
from collections import Counter
import hashlib
import itertools
import json
import math
from pathlib import Path
import struct
import subprocess
import tarfile


def read(path):
    return json.loads(path.read_text())


def rows(path):
    with path.open() as stream:
        for line in stream:
            yield json.loads(line)


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def timing(values):
    ordered = sorted(values)
    return {'count': len(values), 'mean_ms': sum(values) / len(values),
            **{f'p{p}_ms': ordered[int((len(values) - 1) * p / 100 + 0.5)] for p in (50, 95, 99)},
            'max_ms': ordered[-1]}


def native_f32(value):
    # Typed Rust observations print f32 directly; the snapshot's Value promotes
    # them to f64. Compare the exact original bits, not a numeric tolerance.
    if isinstance(value, float):
        return struct.pack('<f', value)
    if isinstance(value, dict):
        return {k: native_f32(v) for k, v in value.items()}
    if isinstance(value, list):
        return [native_f32(v) for v in value]
    return value


def compare(control, trial):
    keys = ('actions', 'sites', 'decision_fnv64', 'scenario_before_step_fnv64')
    count, first, differences, seat_inputs = 0, dict.fromkeys(keys), Counter(), [0, 0]
    lengths = [0, 0]
    for a, b in itertools.zip_longest(rows(control), rows(trial)):
        lengths[0] += a is not None
        lengths[1] += b is not None
        if a is None or b is None:
            continue
        assert a['tick'] == b['tick'] == count
        for key in keys:
            if a[key] != b[key]:
                differences[key] += 1
                if first[key] is None:
                    first[key] = count
        for seat in (0, 1):
            seat_inputs[seat] += a['actions'][seat*3:seat*3+3] != b['actions'][seat*3:seat*3+3]
        count += 1
    return {'overlapping_ticks': count, 'lengths': lengths, 'first_difference': first,
            'different_ticks': {key: differences[key] for key in keys}, 'different_seat_inputs': seat_inputs}


def audit_case(root, item, result):
    path = root / 'cases' / item['name'] / 'output'
    assert result['exit_code'] == 0, result
    s = read(path / 'summary.json')
    assert s['case'] == item['case'] == read(path / 'case.json')
    assert s['controller_reset'] is False and s['profile_from_tick_zero']
    assert s['end_tick'] == s['match']['finished_tick'] <= 36000
    initial, final = read(path / 'initial.json'), read(path / 'final.json')
    assert final['brains'] == s['brains']
    for index, row in enumerate(rows(path / 'run-actions.jsonl')):
        assert row['tick'] == index and len(row['actions']) == 6 and len(row['sites']) == 2
    assert index + 1 == s['end_tick']
    sample_ticks = []
    for row in rows(path / 'run-samples.jsonl'):
        sample_ticks.append(row['tick'])
        a = row['state']['audit']
        assert not a['issues'] and a['max_speed'] < 500
        assert a['occupied_cells'] + a['loose_cells'] + a['removed_cells'] == s['initial_cells']
    expected = list(range(60, s['end_tick'] + 1, 60))
    if s['end_tick'] % 60:
        expected.append(s['end_tick'])
    assert sample_ticks == expected
    completions = []
    for seat in (0, 1):
        assert len(s['metrics'][seat]['completed']) == s['brains'][seat]['completed_recoveries']
        for tick in s['metrics'][seat]['completed']:
            decision = read(path / f'completion-{tick}-p{seat+1}-decision.json')
            state = read(path / f'completion-{tick}-p{seat+1}.json')
            p = decision['observations'][seat]['recovery']['flight']['pilot']
            assert p['tick'] == tick and p['ship_form'] == 'ship' and p['ship_available']
            assert p['location'] == {'aboard': seat}
            assert decision['brains'][seat]['recovery']['status'] == 'succeeded'
            physical = lambda v: {k: native_f32(x) for k, x in v.items() if k not in ('sites', 'site_query')}
            # Completion clears the task. The later inspection requests the
            # idle site's survey, while both readings retain the same physics.
            assert physical(state['pilots'][seat]['recovery']['flight']['pilot']) == physical(p)
            assert p['recovery']['rebuilds'] > 0
            completions.append({'seat': seat, 'tick': tick, 'native_rebuilds': p['recovery']['rebuilds'],
                                'landing': p['landing'], 'health': p['ship_health']})
    features, work, reviews = Counter(), Counter(), {}
    for row in rows(path / 'features.jsonl'):
        assert 0 <= row['tick'] < s['end_tick']
        features[f"p{row['seat']+1}:{row['kind']}"] += 1
        if row['kind'] == 'survey':
            for name, cap in (('coarse_candidates', 8), ('refined_candidates', 8),
                              ('offset_checks', 240), ('staging_route_checks', 16)):
                value = (row['survey'].get('refinement') or {}).get(name, 0)
                assert 0 <= value <= cap
                work[name] = max(work[name], value)
        if row['kind'] == 'pod_review' and row['review'] is not None:
            r = row['review']
            assert row['seat'] == item['case']['candidate_seat'] and r['surveys'] <= 15
            if r['started_tick'] is not None:
                assert r['blocked_tick'] < r['started_tick'] <= r['blocked_tick'] + 900
                assert r['deadline_tick'] == r['started_tick'] + 5400
            reviews[r['blocked_tick']] = r
    costs = {'sensors': [[], []], 'policies': [[], []], 'step': []}
    for tick, row in enumerate(rows(path / 'costs.jsonl')):
        assert row['tick'] == tick
        for group, key in (('sensors', 'sensor_ms'), ('policies', 'policy_ms')):
            for seat in (0, 1):
                costs[group][seat].append(row[key][seat])
        costs['step'].append(row['step_ms'])
    assert len(costs['step']) == s['end_tick']
    measured = {'sensors': [timing(v) for v in costs['sensors']],
                'policies': [timing(v) for v in costs['policies']], 'step': timing(costs['step'])}
    for group in ('sensors', 'policies', 'step'):
        wanted = [s['timing'][group]] if group == 'step' else s['timing'][group]
        actual = [measured[group]] if group == 'step' else measured[group]
        for a, b in zip(wanted, actual, strict=True):
            for key in a:
                assert math.isclose(a[key], b[key], rel_tol=1e-10, abs_tol=1e-12), (group, key, a, b)
    return {'name': item['name'], 'summary': s, 'completions': completions,
            'features': dict(features), 'max_survey_work': dict(work), 'pod_reviews': list(reviews.values()),
            'audit_samples': len(sample_ticks), 'final_terrain': final['audit']}, initial


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    args = parser.parse_args()
    root, repo = args.root.resolve(), Path(__file__).resolve().parent.parent
    runtime, inputs = read(root / 'runtime.json'), read(root / 'source-inputs.json')
    assert digest(root / 'fresh-match-comparison') == runtime['binary_sha256']
    assert digest(root / 'source-inputs.json') == runtime['source_inputs_sha256']
    checked = set()
    process = subprocess.Popen(['git', 'archive', runtime['source_commit']], cwd=repo, stdout=subprocess.PIPE)
    with tarfile.open(fileobj=process.stdout, mode='r|') as archive:
        for item in archive:
            if item.name in inputs:
                payload = item.linkname.encode() if item.issym() else archive.extractfile(item).read()
                assert hashlib.sha256(payload).hexdigest() == inputs[item.name], item.name
                checked.add(item.name)
    assert process.wait() == 0 and checked == set(inputs)
    matrix, results = read(root / 'matrix.json'), read(root / 'results.json')
    assert len(matrix) == len(results) == 48
    settings, cases = {}, []
    for item, result in zip(matrix, results, strict=True):
        assert item['name'] == result['name']
        assert result['binary_sha256'] == runtime['binary_sha256'] and result['source_commit'] == runtime['source_commit']
        receipt, initial = audit_case(root, item, result)
        path = root / 'cases' / item['name'] / 'output'
        if item['arm'] == 'control':
            settings[item['setting']] = (path, initial, receipt)
        else:
            control, old_initial, old_receipt = settings[item['setting']]
            assert initial == old_initial, item['name']
            controllers = (path / 'initial-controllers.txt').read_text()
            assert controllers.count('terrain_recovery: true') == 1
            assert controllers.replace('terrain_recovery: true', 'terrain_recovery: false') == (control / 'initial-controllers.txt').read_text()
            receipt['comparison'] = compare(control / 'run-actions.jsonl', path / 'run-actions.jsonl')
            receipt['same_match_result'] = receipt['summary']['match'] == old_receipt['summary']['match']
        cases.append(receipt)
        print(json.dumps({'case': item['name'], 'ticks': receipt['summary']['end_tick'],
                          'completion_count': len(receipt['completions']), 'features': receipt['features'],
                          'comparison': receipt.get('comparison')}), flush=True)
    assert len(settings) == 16
    receipts = {str(p.relative_to(root)): {'sha256': digest(p), 'bytes': p.stat().st_size}
                for p in sorted((root / 'cases').rglob('*')) if p.is_file()}
    report = {'schema': 1, 'runtime': runtime, 'cases': cases, 'artifact_receipts': receipts,
              'source_files_verified': len(checked), 'auditor_sha256': digest(Path(__file__))}
    (root / 'audit.json').write_text(json.dumps(report, indent=2) + '\n')
    print(f'Verified {len(cases)} matches, {len(receipts)} artifacts and {len(checked)} source files.')


if __name__ == '__main__':
    main()
