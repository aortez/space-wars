#!/usr/bin/env python3
"""Fixed residual-budget forecast study; never drop expired or interrupted jobs."""
import argparse
from collections import Counter
import csv
import gzip
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import statistics
import subprocess

SPEC = importlib.util.spec_from_file_location('forecast', Path(__file__).with_name('forecast-transfer-references.py'))
Q = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(Q)
C, T, F = Q.C, Q.T, Q.F
ALLOWANCES = (4, 32)
MAX_AGE = 120


def trace_digest(root):
    digest = hashlib.sha256()
    with gzip.open(root / 'trace.jsonl.gz', 'rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return digest.hexdigest()


def audit_upstream(root):
    """Reconcile separately emitted dispatch ledgers, including shadow charges."""
    live = {}
    live_jobs = {}
    with (root / 'live-planning.csv').open() as stream:
        for r in csv.DictReader(stream):
            tick = int(r['tick'])
            assert int(r['graph_budget']) == 4 and int(r['query_budget']) == 384
            work = (int(r['total_graph']), int(r['total_queries']))
            assert live.setdefault(tick, work) == work
            charged = live_jobs.setdefault(tick, [0, 0])
            charged[0] += int(r['graph'])
            charged[1] += int(r['queries'])
    assert all(tuple(live_jobs[tick]) == work for tick, work in live.items())
    evaluations = T.rows(root / 'mission-evaluation-work.jsonl')
    flags = {r['tick']: r for r in T.rows(root / 'flag-survey-work.jsonl')}
    shadows = {r['tick']: r for r in T.rows(root / 'flag-value-shadow-work.jsonl')}
    assert len(evaluations) == len(flags) == len(shadows)
    upstream = {}
    for e in evaluations:
        tick = e['tick']
        assert tick == len(upstream), 'full physical dispatch history required'
        lg, lq = live.get(tick, (0, 0))
        assert 0 <= lg <= 4 and 0 <= lq <= 384
        assert e['remaining_before_evaluation'] == dict(graph=4-lg, physics_queries=384-lq)
        assert e['allowance']['graph'] <= 4-lg and e['allowance']['physics_queries'] == 0
        eg = e['charged']['graph']
        assert 0 <= eg <= e['allowance']['graph'] and e['charged']['physics_queries'] == 0
        f, s = flags[tick], shadows[tick]
        assert f['remaining_after_evaluation'] == dict(graph=4-lg-eg, physics_queries=384-lq)
        assert f['allocation']['allowance'] == f['remaining_after_evaluation']
        fg, fq = (f['allocation']['charged'][k] for k in ['graph', 'physics_queries'])
        assert 0 <= fg <= 4-lg-eg and 0 <= fq <= 384-lq
        assert (fg, fq) == tuple(sum(j['charged'][k] for j in f['allocation']['jobs']) for k in ['graph', 'physics_queries'])
        assert s['remaining_after_flag_survey'] == dict(graph=4-lg-eg-fg, physics_queries=0)
        sg = s['charged']['graph']
        assert 0 <= sg <= 4-lg-eg-fg and s['charged']['physics_queries'] == 0
        upstream[tick] = dict(live=lg, evaluator=eg, flags=fg, shadow=sg, total=lg+eg+fg+sg)
    report = json.loads((root / 'report.json').read_text())
    assert len(upstream) == report['elapsed_ticks']
    assert sum(r['evaluator'] for r in upstream.values()) == report['mission_evaluation']['charged']
    assert sum(r['shadow'] for r in upstream.values()) == report['flag_survey']['shadow']['charged']
    for k, column in [('graph', 0), ('physics_queries', 1)]:
        assert sum(r[column] for r in live.values()) == report['live_objective_planning']['telemetry'][k]
        assert sum(r['allocation']['charged'][k] for r in flags.values()) == report['flag_survey']['telemetry'][k]
    return upstream


def audit_upstream_parity(before, after):
    for name in ['flag-survey-work.jsonl', 'flag-value-shadow-work.jsonl']:
        assert F.digest(before/name) == F.digest(after/name), f'changed upstream work: {name}'
    def csv_work(root):
        with (root/'live-planning.csv').open() as stream:
            return [{k: v for k, v in r.items() if k != 'dispatch_ms'} for r in csv.DictReader(stream)]
    assert csv_work(before) == csv_work(after), 'changed live planner allocation'
    a, b = [json.loads((root/'report.json').read_text()) for root in [before, after]]
    for key in ['charged', 'completed', 'cancelled', 'pending']:
        assert a['mission_evaluation'][key] == b['mission_evaluation'][key]
    for key in ['live_objective_planning', 'flag_survey']:
        counts = lambda r: {k: v for k, v in r[key]['telemetry'].items() if not k.endswith('_ms')}
        assert counts(a) == counts(b), 'changed upstream telemetry'


def audit_lifetime(case, schedule, rows, expected, upstream):
    allowance = schedule['total_graph_allowance']
    assert allowance in ALLOWANCES
    assert schedule['playing_graph_allowance'] == 4 and schedule['max_source_age_ticks'] == MAX_AGE
    assert schedule['observational'] and schedule['physics_queries'] == 0
    assert T.f32_identity(schedule['source_environment']) == T.f32_identity(expected['environment'])
    assert schedule['source_actions'] == expected['source_actions']
    assert rows and rows[-1]['event'] == 'finish'
    assert rows[-1]['state'] == schedule['final_state']
    dispatch = rows[:-1]
    total = 0
    first_ready = None
    zero_pending = 0
    for n, row in enumerate(dispatch):
        tick = row['tick']
        assert row['event'] == 'dispatch' and tick == case['source_tick'] + n
        assert row['playing_graph_allowance'] == 4 and row['total_graph_allowance'] == allowance
        prior = upstream[tick]['total']
        assert row['playing_charged_graph'] == prior
        remaining = dict(graph=allowance-prior, physics_queries=0)
        assert row['remaining_before_forecast'] == remaining
        a = row['allocation']
        assert a['allowance'] == remaining and a['tick'] == n+1
        charge = a['charged']['graph']
        assert 0 <= charge <= remaining['graph'] and a['charged']['physics_queries'] == 0
        assert sum(j['charged']['graph'] for j in a['jobs']) == charge
        assert all(j['charged']['physics_queries'] == 0 for j in a['jobs'])
        assert all(math.isfinite(row[k]) and row[k] >= 0 for k in ['observation_ms', 'dispatch_ms'])
        total += charge
        state = row['state']
        if state is None:
            assert schedule['rejected'] and not a['jobs'] and charge == 0
            continue
        assert state['source_tick'] == case['source_tick'] and state['destination'] == case['destination']
        assert state['actor'] == ['player_1', 'player_2'][case['seat']]
        assert state['token']['actor'] == case['seat'] and state['charged_graph'] == total
        assert all(j['request'] == state['token'] for j in a['jobs'])
        age = tick - case['source_tick']
        if state['phase'] == 'stale':
            assert state['cancelled_tick'] == tick and state['reason'] and state['validated_tick'] is None
            assert not a['jobs'] and charge == 0 and n == len(dispatch)-1
            if state['reason'] == 'source expired': assert age == MAX_AGE + 1
        else:
            assert age <= MAX_AGE and state['validated_tick'] == tick
            assert state['reason'] is None and state['cancelled_tick'] is None
            assert len(a['jobs']) == 1 and a['jobs'][0]['phase'].lower() == state['phase']
            if state['phase'] == 'ready':
                if first_ready is None:
                    assert state['completed_tick'] == tick
                    first_ready = state
                assert state['completed_tick'] == first_ready['completed_tick']
                assert state['completed_tick'] <= tick and total == expected['report']['charged_graph']
            else:
                assert state['phase'] == 'pending' and first_ready is None and state['completed_tick'] is None
                zero_pending += charge == 0
                assert total < expected['report']['charged_graph']
    assert schedule['charged_graph'] == total
    assert schedule['published_state'] == first_ready
    assert schedule['completed'] == int(first_ready is not None)
    if first_ready:
        assert first_ready['completed_tick'] - case['source_tick'] <= MAX_AGE
        assert T.f32_identity(schedule['published']) == T.f32_identity(expected['report'])
    else:
        assert schedule['published'] is None
    final = schedule['final_state']
    if final is not None:
        assert final['phase'] == 'stale' and final['validated_tick'] is None
        assert final['token']['actor'] == case['seat'] and final['source_tick'] == case['source_tick']
        assert final['completed_tick'] == (first_ready['completed_tick'] if first_ready else None)
        assert final['charged_graph'] == total and final['cancelled_tick'] <= rows[-1]['tick']
        assert final['cancelled_tick'] >= case['source_tick']
        if dispatch and dispatch[-1]['state'] and dispatch[-1]['state']['phase'] == 'stale':
            assert final == dispatch[-1]['state'], 'finish rewrote a cancellation'
        else:
            assert final['cancelled_tick'] == rows[-1]['tick'], 'terminal cancellation must match terminal observation'
        if final['reason'] == 'source expired':
            assert final['cancelled_tick'] - case['source_tick'] == MAX_AGE + 1
        assert schedule['submitted'] == schedule['cancelled'] == 1 and schedule['rejected'] is None
    else:
        assert schedule['submitted'] == schedule['cancelled'] == 0 and total == 0 and schedule['rejected']
    return dict(completed=first_ready is not None,
        completion_age_ticks=first_ready['completed_tick']-case['source_tick'] if first_ready else None,
        cancellation_reason=final['reason'] if final else schedule['rejected'],
        cancellation_age_ticks=final['cancelled_tick']-case['source_tick'] if final else None,
        charged_graph=total, zero_work_pending_ticks=zero_pending, observed_dispatches=len(dispatch),
        maximum_dispatch_ms=max((r['dispatch_ms'] for r in dispatch), default=0),
        maximum_observation_ms=max((r['observation_ms'] for r in dispatch), default=0))


def aggregate(runs):
    output = {}
    for allowance in ALLOWANCES:
        cases = [r['schedule'] for r in runs.values() if r['allowance'] == allowance]
        ages = [r['completion_age_ticks'] for r in cases if r['completed']]
        output[str(allowance)] = dict(cases=len(cases), completed=len(ages),
            mean_completion_age_ticks=statistics.mean(ages) if ages else None,
            max_completion_age_ticks=max(ages, default=None),
            cancellation_reasons=dict(Counter(r['cancellation_reason'] for r in cases)),
            charged_graph=sum(r['charged_graph'] for r in cases),
            zero_work_pending_ticks=sum(r['zero_work_pending_ticks'] for r in cases),
            maximum_dispatch_ms=max((r['maximum_dispatch_ms'] for r in cases), default=0),
            maximum_observation_ms=max((r['maximum_observation_ms'] for r in cases), default=0))
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    opts.out.mkdir(parents=True, exist_ok=False)
    previous = json.loads((opts.reference / 'summary.json').read_text())
    cases = previous['previous_plan'] + previous['holdout_plan']
    assert len(cases) == 25 and len({c['name'] for c in cases}) == 25
    result = dict(schema=1, plan=cases, allowances=list(ALLOWANCES), max_age_ticks=MAX_AGE, runs={},
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()),
        binary_sha256=F.digest(opts.binary), reference_summary_sha256=F.digest(opts.reference / 'summary.json'),
        scope='All25 frozen physical branches at total graph4/32. Existing planners stay at4; extra28 is forecast-only. Lifetime120ticks. No model fitting or live decisions. Full controller/probe/evaluation/physics parity and immutable completed report equality. Source capture/validation/publication outside quota. Headless engineering corpus, not Pi performance or match strength.')
    save = lambda: (opts.out / 'summary.json').write_text(json.dumps(result, indent=2) + '\n')
    save()
    for case in cases:
        before = opts.reference / case['name']
        old = previous['runs'][case['name']]
        assert F.digest(before / 'report.json') == old['report_sha256']
        old_trace = trace_digest(before)
        assert old_trace == old['trace_sha256']
        expected = json.loads((before / 'report.json').read_text())['transfer_probe']['forecast']
        for allowance in ALLOWANCES:
            name = case['name'] + f'-budget{allowance}'
            run = F.D.run(opts.binary, opts.out, name,
                T.probe_args(case) + ['--probe-transfer-pursuit', 'defer_new',
                    '--schedule-transfer-forecast', 'true', '--transfer-forecast-allowance', str(allowance)],
                case['seat'], seconds=case['source_tick']//60 + 61)
            root = opts.out / name
            run.update(allowance=allowance, trace_sha256=C.archive(root))
            run['unchanged'] = Q.audit_unchanged(case, before, root, old_trace, run['trace_sha256'])
            audit_upstream_parity(before, root)
            run['unchanged']['exact_upstream_work'] = True
            report = json.loads((root / 'report.json').read_text())
            probe = report['transfer_probe']
            trace = T.rows(root / 'transfer-probe.jsonl')
            run['probe'] = T.audit_probe(case, probe, trace)
            run['reconciled_probe_rows'] = C.audit_controls(case, root, trace)
            run['schedule'] = audit_lifetime(case, probe['forecast_schedule'],
                T.rows(root / 'transfer-forecast-work.jsonl'), expected, audit_upstream(root))
            assert T.rows(root / 'transfer-forecast-work.jsonl')[-1]['tick'] == probe['outcome']['tick']
            run['raw_hashes'] = {p.name: F.digest(p) for p in root.iterdir() if p.is_file()}
            result['runs'][name] = run
            save()
            print(name, run['schedule'], flush=True)
    assert F.digest(opts.binary) == result['binary_sha256']
    result['results'] = aggregate(result['runs'])
    save()


if __name__ == '__main__':
    main()
