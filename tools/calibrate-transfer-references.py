#!/usr/bin/env python3
"""Paired ordinary/deferred-pursuit transfers; retain interruptions and refusals."""
import argparse
from collections import Counter
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess

SPEC = importlib.util.spec_from_file_location('transfer', Path(__file__).with_name('probe-transfer-references.py'))
T = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(T)
F = T.F


def discovery_plan():
    return [dict(name=f'fresh{world}-asteroids{interval}',
                 seed=int.from_bytes(hashlib.sha256(f'transfer-calibration-v1:{world}'.encode()).digest()[:8], 'little'),
                 interval=interval, policies=[13, 13])
            for world in range(2) for interval in [0, 3]]


def archive(root):
    digest = F.digest(root / 'trace.jsonl')
    with (root / 'trace.jsonl').open('rb') as source, gzip.open(root / 'trace.jsonl.gz', 'wb', compresslevel=3) as dest:
        shutil.copyfileobj(source, dest)
    (root / 'trace.jsonl').unlink()
    return digest


def read_trace(root, start=0, end=None):
    path = root / 'trace.jsonl.gz'
    with gzip.open(path, 'rb') as stream:
        for line in stream:
            row = json.loads(line)
            if end is not None and row['tick'] > end:
                return
            if row['tick'] >= start:
                yield row


def phases(trace):
    segments = []
    for row in trace[:-1]:  # terminal commands are not executed
        key = dict(goal=row['goal'], frame=row['frame'], avoidance=(row['avoidance'] or {}).get('obstacle'))
        if segments and segments[-1]['phase'] == key:
            segments[-1]['end_tick'] = row['tick'] + 1
        else:
            segments.append(dict(phase=key, start_tick=row['tick'], end_tick=row['tick'] + 1))
    assert sum(s['end_tick'] - s['start_tick'] for s in segments) == trace[-1]['tick'] - trace[0]['tick']
    return dict(executed_ticks=len(trace) - 1,
                goals=dict(Counter(r['goal'] for r in trace[:-1])), segments=segments)


def audit_controls(case, root, trace):
    """Reconcile diagnostic flags to the actual controller trace, not themselves."""
    control = [r for r in read_trace(root, case['source_tick']) if r['seat'] == case['seat']]
    report = json.loads((root / 'report.json').read_text())
    if len(control) + 1 == len(trace):
        assert trace[-1]['match_finished'] and report['round']['outcome'] is not None
        assert trace[-1]['tick'] == report['elapsed_ticks']
        control.append(None)  # match ended after the last executed step
    assert len(control) == len(trace)
    for r, c in zip(trace, control):
        p, m = ((c['observation']['local']['combat']['recovery']['flight']['pilot'], c['mission']) if c else
                (report['final_pilots'][case['seat']], report['missions'][case['seat']]))
        assert r['tick'] == p['tick']
        if c: assert r['tick'] == c['tick']
        for key in ['ship', 'queries_ready', 'ship_available', 'location']:
            assert r[key] == p[key]
        assert r['health'] == p['ship_health'] and r['form'] == p['ship_form']
        assert r['frame'] == p['planet']['index']
        if c: assert r['next_actions'] == c['actions']
        assert r['goal'] == m['goal'] and r['target'] == m['target'] and r['avoidance'] == m['avoidance']
        assert r['recovery_active'] == (m['goal'] == 'recover' or m['recovery'] is not None)
        arrived = any(e['tick'] == r['tick'] and e['kind'] == 'arrived' and e['planet'] == case['destination'] for e in m['events'])
        assert r['arrived'] == arrived
        contacts = r['solver_contacts']
        if c: assert all(contacts[k] == c['landing_diagnostics'][k] for k in ['hull', 'feet', 'landing'])
        assert r['solver_contact'] == (contacts['hull']['count'] > 0 or any(f['count'] > 0 for f in contacts['feet']))
        assert r['debris_contact'] == ((r['damage']['last_contact_tick'] or 0) > case['source_tick'])
    return len(trace)


def audit_pair(case, ordinary_root, controlled_root):
    ordinary, controlled = [T.rows(root / 'transfer-probe.jsonl') for root in [ordinary_root, controlled_root]]
    normal_report, controlled_report = [json.loads((root / 'report.json').read_text())['transfer_probe']
                                       for root in [ordinary_root, controlled_root]]
    assert normal_report['source'] == controlled_report['source']
    assert normal_report.get('pursuit_policy', 'ordinary') == 'ordinary'
    assert controlled_report['pursuit_policy'] == 'defer_new'
    deferred = []
    for r in controlled:
        comparison = r.get('pursuit_control')
        if comparison is None:
            assert r['tick'] == case['source_tick'] or r['match_finished']
            continue
        if comparison['deferred_new_pursuit']:
            assert comparison['ordinary_new_pursuit']
            assert comparison['ordinary_pursuit']['started_tick'] == r['tick']
            assert comparison['ordinary_target'] is None
            deferred.append(r['tick'])
        else:
            assert comparison['same_intent'] and comparison['same_mission'], 'changed control beyond new pursuit deferral'
    cutoff = deferred[0] if deferred else controlled[-1]['tick']
    if deferred:
        assert cutoff > case['source_tick']
        assert normal_report['outcome']['tick'] == cutoff
        assert normal_report['outcome']['elapsed_ticks'] == cutoff - case['source_tick']
        if normal_report['outcome']['reason'] != 'retargeted':
            # Physics may interrupt at the very observation where a new hunt
            # becomes eligible. Neither terminal command is executed.
            assert normal_report['outcome']['reason'] in {'solver_or_debris_contact', 'ship_or_pilot_lost', 'recovery'}
            assert normal_report['outcome'] == controlled_report['outcome']
    else:
        assert normal_report['outcome'] == controlled_report['outcome']
        assert len(ordinary) == len(controlled)
        assert [{k:v for k,v in r.items() if k != 'pursuit_control'} for r in ordinary] == [
               {k:v for k,v in r.items() if k != 'pursuit_control'} for r in controlled]
    left, right = [list(read_trace(root, case['source_tick'], cutoff)) for root in [ordinary_root, controlled_root]]
    end_rows = 0 if not deferred and controlled[-1]['match_finished'] else 2
    assert len(left) == len(right) == (cutoff-case['source_tick'])*2 + end_rows
    for a, b in zip(left, right):
        assert a['tick'] == b['tick'] and a['seat'] == b['seat']
        if a['tick'] < cutoff or not deferred or a['seat'] != case['seat']:
            assert a == b, 'diverged before suppressed pursuit'
        else:
            assert a['observation'] == b['observation']
            p = a['mission']['pursuit']
            assert p is not None and p['started_tick'] == cutoff
            assert any(e['kind'] == 'pursuit_started' and e['tick'] == cutoff for e in a['mission']['events'])
            comparison = next(r['pursuit_control'] for r in controlled if r['tick'] == cutoff)
            assert a['actions'] == comparison['ordinary_actions'] and p == comparison['ordinary_pursuit']
    return dict(first_deferred_tick=deferred[0] if deferred else None,
                deferred_observations=len(deferred),
                deferred_executed_observations=sum(t < controlled[-1]['tick'] for t in deferred),
                exact_pair_through_tick=cutoff if not deferred else cutoff-1,
                ordinary_outcome=normal_report['outcome'], controlled_outcome=controlled_report['outcome'],
                ordinary_phases=phases(ordinary), controlled_phases=phases(controlled),
                reconciled_probe_rows=[audit_controls(case, root, trace) for root, trace in
                                       [(ordinary_root, ordinary), (controlled_root, controlled)]])


def fresh_cases(discoveries):
    cases = []
    for condition, sources in discoveries:
        assert len({s['seat'] for s in sources}) == len(sources) <= 2
        for source in sources:
            assert 60 <= source['source_tick'] < 10800
            alternatives = source['alternatives']
            assert 1 <= len(alternatives) <= 2
            assert [d['destination'] for d in alternatives] == sorted({d['destination'] for d in alternatives})
            for diagnostic in alternatives:
                assert diagnostic['destination'] not in [source['current_target'], source['transfer_source']['frame']]
                cases.append(dict(name=f"{condition['name']}-s{source['seat']}-t{source['source_tick']}-p{diagnostic['destination']}",
                    condition=condition, source_tick=source['source_tick'], destination=diagnostic['destination'], seat=source['seat'],
                    transfer_source=source['transfer_source'], expected_reason=diagnostic['reason'], expected_diagnostic=diagnostic,
                    group='fresh'))
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--shadow-reference', type=Path, required=True)
    parser.add_argument('--previous-probes', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    opts.out.mkdir(parents=True, exist_ok=False)
    previous = json.loads((opts.previous_probes / 'summary.json').read_text())
    historical = [dict(c, group='historical') for c in previous['plan'] if previous['runs'][c['name']]['probe']['accepted']]
    assert len(historical) == 9 and len(previous['plan']) == 31
    result = dict(schema=1, discovery_plan=discovery_plan(), historical_plan=historical, fresh_plan=None,
        discovery={}, normal_regression={}, branches={}, pairs={},
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()),
        binary_sha256=F.digest(opts.binary), previous_summary_sha256=F.digest(opts.previous_probes / 'summary.json'),
        scope='Fixed 31-case normal regression, all9 formerly switch-eligible historical pairs, plus first eligible interplanetary alternatives per seat in4 fixed new-world conditions. No retries/strength selection. Optional mission-priority deferral changes flight and potentially combat exposure; opponent/world mechanics remain live. No live model change or fitted constants.')
    save = lambda: F.D.write(opts.out / 'summary.json', result)
    save()
    normal_root = opts.out / 'normal-regression'
    with (opts.out / 'normal-regression.log').open('w') as log:
        subprocess.run(['python3', str(Path(__file__).with_name('probe-transfer-references.py')), '--reference', str(opts.shadow_reference),
                        '--out', str(normal_root), '--binary', str(opts.binary)], check=True, stdout=log, stderr=log)
    normal = json.loads((normal_root / 'summary.json').read_text())
    assert normal['plan'] == previous['plan']
    for case in normal['plan']:
        name = case['name']
        assert normal['runs'][name]['trace_sha256'] == previous['runs'][name]['trace_sha256']
        assert normal['runs'][name]['probe'] == previous['runs'][name]['probe']
    result['normal_regression'] = dict(summary_sha256=F.digest(normal_root / 'summary.json'), cases=31,
                                       exact_controller_traces=True, exact_outcomes_and_diagnostics=True)
    save()
    discovery_root = opts.out / 'discovery'
    discovery_root.mkdir()
    discoveries = []
    for condition in result['discovery_plan']:
        args = T.probe_args(dict(condition=condition, seat=0, source_tick=0, destination=0))[:-6]
        args += ['--sample-transfer-sources', 'true']
        run = F.D.run(opts.binary, discovery_root, condition['name'], args, 0, seconds=180)
        root = discovery_root / condition['name']
        sources = T.rows(root / 'transfer-sources.jsonl')
        report = json.loads((root / 'report.json').read_text())
        assert [any(s['seat'] == seat for s in sources) for seat in [0, 1]] == report['transfer_sources']['found']
        run.update(sources=sources, found=report['transfer_sources']['found'], trace_sha256=archive(root),
                   sources_sha256=F.digest(root / 'transfer-sources.jsonl'))
        result['discovery'][condition['name']] = run
        discoveries.append((condition, sources))
        save()
    fresh = fresh_cases(discoveries)
    result['fresh_plan'] = fresh
    save()  # freeze every discovered pair before executing either mode
    expected = T.reference_prefixes(opts.shadow_reference, historical)
    expected.update(T.reference_prefixes(discovery_root, fresh))
    for case in historical + fresh:
        branch_paths = {}
        for mode in ['ordinary', 'defer_new']:
            if case['group'] == 'historical' and mode == 'ordinary':
                branch_paths[mode] = normal_root / case['name']
                continue
            name = case['name'] + '-' + mode
            args = T.probe_args(case) + ['--probe-transfer-pursuit', mode]
            run = F.D.run(opts.binary, opts.out, name, args, case['seat'], seconds=case['source_tick']//60 + 61)
            root = opts.out / name
            report = json.loads((root / 'report.json').read_text())
            probe = report['transfer_probe']
            run.update(probe=T.audit_probe(case, probe, T.rows(root / 'transfer-probe.jsonl')),
                       parity=T.audit_prefix(root, case, expected[(case['condition']['name'], case['source_tick'])], probe),
                       trace_sha256=archive(root), probe_trace_sha256=F.digest(root / 'transfer-probe.jsonl'))
            result['branches'][name] = run
            branch_paths[mode] = root
            save()
        result['pairs'][case['name']] = audit_pair(case, branch_paths['ordinary'], branch_paths['defer_new'])
        save()
        print(case['name'], result['pairs'][case['name']]['controlled_outcome'], flush=True)
    assert F.digest(opts.binary) == result['binary_sha256']
    result['outcomes'] = {group: {mode: dict(Counter(result['pairs'][c['name']][mode+'_outcome']['reason']
                        for c in historical+fresh if c['group'] == group)) for mode in ['ordinary', 'controlled']}
                        for group in ['historical', 'fresh']}
    save()


if __name__ == '__main__':
    main()
