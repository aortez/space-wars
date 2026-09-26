#!/usr/bin/env python3
"""Audit all locally measured alternatives at used historical flag-shadow sources."""
import argparse
from collections import Counter
import gzip
import hashlib
import importlib.util
import json
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
    assert probe['source']['transfer_source'] == case['transfer_source'], 'wrong pinned reference'
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
    assert d['reference'] is None and d['geometry'] is not None
    g = d['geometry']
    assert g['separation'] < g['threshold'] and g['body'] is not None
    assert g['check'] in {'static_planet', 'moving_planet'}
    assert (d['completed_stages'] is not None) == (g['leg'] == 'transfer')
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
        assert all(r['target'] == case['destination'] for r in trace[:-1])
        assert not any(r['arrived'] or r['solver_contact'] or r['debris_contact'] for r in trace[:-1])
        last = trace[-1]
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
        args = ['--world', 'generated', '--seed', str(condition['seed']), '--mode', 'duel',
            '--match', 'true', '--seat', '0', '--asteroid-interval', str(condition['interval']),
            '--p1-policy', f"material_mission_v{condition['policies'][0]}",
            '--p2-policy', f"material_mission_v{condition['policies'][1]}",
            '--trace', 'true', '--trace-start-tick', '0', '--trace-end-tick', '36002',
            '--survey-capture-flags', 'true', '--shadow-capture-flags', 'true',
            '--probe-transfer-seat', str(c['seat']), '--probe-transfer-tick', str(c['source_tick']),
            '--probe-transfer-destination', str(c['destination'])]
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
