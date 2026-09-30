#!/usr/bin/env python3
"""Frozen paired trial of opt-in published flag costs, including failures."""
import argparse
from collections import Counter
import csv
import hashlib
import json
from pathlib import Path
import subprocess
import importlib.util


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


P = module('promotion', 'validate-capture-value.py')
D, E = P.D, P.E


def plan():
    rows = []
    for world in ['destination', 'value-destination']:
        for seat in [0, 1]:
            for bearing in [0.0, 0.8, 1.2, -0.8]:
                group = f'{world}-p{seat+1}-bearing{bearing}'
                for candidate in [False, True]:
                    rows.append(dict(name=group+('-candidate' if candidate else '-predecessor'),
                        group=group, kind='directed', world=world, seat=seat,
                        seed=42, interval=0, bearing=bearing, candidate=candidate))
    for world in range(4):
        namespace = f'published-flag-costs-v1:{world}'
        seed = int.from_bytes(hashlib.sha256(namespace.encode()).digest()[:8], 'little')
        for interval in [0, 3]:
            for seat in [0, 1]:
                group = f'world{world}-asteroids{interval}-p{seat+1}'
                order = [False, True] if (world+interval+seat) % 2 == 0 else [True, False]
                for candidate in order:
                    rows.append(dict(name=group+('-candidate' if candidate else '-predecessor'),
                        group=group, kind='held_out', world='generated', seat=seat,
                        seed=seed, seed_namespace=namespace, interval=interval, candidate=candidate))
    return rows


def arguments(item):
    seat = item['seat']
    policies = ['material_mission_v10'] * 2
    policies[seat] = 'material_mission_v13'
    directed = item['kind'] == 'directed'
    command = ['--world', item['world'], '--seed', str(item['seed']), '--match', 'true',
        '--mode', 'quiet' if directed else 'duel', '--seat', str(seat if directed else 0),
        '--asteroid-interval', str(item['interval']), '--p1-policy', policies[0], '--p2-policy', policies[1],
        '--survey-capture-flags', 'true', '--measure-mission-progress', 'true',
        '--admit-flag-costs', str(seat) if item['candidate'] else 'none']
    if directed:
        command += ['--mirror', str(seat == 1).lower(), '--flag-bearing', str(item['bearing'])]
    return command, policies


def rows(path):
    with path.open() as stream:
        yield from map(json.loads, stream)


def allocation_audit(root):
    """Sum all three dispatch ledgers, including unpublished and cancelled jobs."""
    live = {}
    with (root/'live-planning.csv').open() as stream:
        for r in csv.DictReader(stream):
            values = live.setdefault(int(r['tick']), [0, 0])
            values[0] += int(r['graph'])
            values[1] += int(r['queries'])
            assert int(r['graph_budget']) == 4 and int(r['query_budget']) == 384
    evaluation = {r['tick']:r for r in rows(root/'mission-evaluation-work.jsonl')}
    flags = {r['tick']:r for r in rows(root/'flag-survey-work.jsonl')}
    assert evaluation.keys() == flags.keys() and live.keys() <= evaluation.keys()
    totals, maximum = Counter(), [0, 0]
    for tick, r in evaluation.items():
        l = live.get(tick, [0, 0])
        for i, (key, quota) in enumerate([('graph', 4), ('physics_queries', 384)]):
            assert r['remaining_before_evaluation'][key] == quota - l[i]
            e = r['charged'][key]
            assert 0 <= e <= r['allowance'][key] <= quota - l[i]
            f = flags[tick]
            assert f['remaining_after_evaluation'][key] == quota - l[i] - e
            a = f['allocation']
            assert a['tick'] == tick
            charge = sum(j['charged'][key] for j in a['jobs'])
            assert charge == a['charged'][key] <= a['allowance'][key] <= quota - l[i] - e
            total = l[i] + e + charge
            assert total <= quota
            maximum[i] = max(maximum[i], total)
            for name, value in [('native_and_neutral', l[i]), ('evaluation', e), ('flags', charge)]:
                totals[f'{name}_{key}'] += value
    return dict(ticks=len(evaluation), charges=dict(totals), maximum=dict(zip(['graph', 'physics_queries'], maximum)),
        scope='Combined native/neutral, evaluation and flag dispatch work. Synchronous sensors, snapshot construction and publication validation are timed separately and outside operation quotas.')


def first_predictions(report, path):
    """Pin the first supported CURRENT prediction per visit, before later outcomes."""
    visits = {(s, v['planet'], v['selected_tick']):v for s, m in enumerate(report['metrics']) for v in m['visits']}
    first, admissions, coverage = {}, Counter(), Counter()
    certificates = {(s['actor'], s['site']['planet'], s['site']['bearing'], s['generation'], s['source_tick']):s
        for s in rows(path.with_name('flag-survey.jsonl'))}
    for r in rows(path):
        seat = {'player_1':0, 'player_2':1}[r['actor']]
        numeric = [c for c in r['candidates'] if c['total_seconds'] is not None]
        coverage[f'p{seat+1}_reports'] += 1
        coverage[f'p{seat+1}_multiple_numeric'] += len(numeric) >= 2
        for a in r.get('flag_admissions', []):
            admissions[a['reason'] or ('used' if a['used'] else 'valid_not_used')] += 1
            if a['used']:
                assert a['source_tick'] <= a['completed_tick'] == a['validated_tick'] <= r['source_tick']
                cert = certificates[(r['actor'], a['site']['planet'], a['site']['bearing'], a['generation'], a['source_tick'])]
                assert cert['reason'] is None and cert['validated_tick'] == a['validated_tick']
                v = cert['validation']
                assert v['complete'] and v['predicates_valid'] and v['geometry']['valid']
        for c in numeric:
            if c['current'] and r['selected_tick'] is not None:
                first.setdefault((seat, c['planet'], r['selected_tick']), (r, c))
    attempts = []
    for key, (r, c) in first.items():
        v = visits[key]
        source = r['source_tick']
        # A mid-trip first measurement can follow earlier milestones. Retain
        # these as already observed, rather than claiming to have forecast them.
        future = dict(v)
        already = {}
        for milestone in ['arrived_tick', 'landed_tick', 'claimed_tick', 'boarded_tick', 'departed_tick']:
            if future[milestone] is not None and future[milestone] < source:
                already[milestone] = future[milestone]
                future[milestone] = None
        outcome = 'completed' if v['departed_tick'] is not None else 'abandoned' if v['abandoned_tick'] is not None else 'unfinished'
        attempts.append(dict(seat=key[0], source_forecast=r, candidate=c, visit=v,
            outcome=outcome, already_observed=already,
            references=P.milestone_references(c, source, future)))
    return dict(coverage=dict(coverage), admissions=dict(admissions), attempts=attempts,
        outcomes=dict(Counter(a['outcome'] for a in attempts)))


def analyze(root, item, policies):
    report = json.loads((root/'report.json').read_text())
    assert [c['policy'] for c in report['policy_configuration']] == policies
    assert report['seed'] == item['seed'] and report['round']['time_limit_seconds'] == 600
    assert report['live_objective_planning']['enabled_seats'] == []
    admitted = [c.get('flag_cost_model') for c in report['policy_configuration']]
    expected = [None, None]
    if item['candidate']:
        expected[item['seat']] = 'capture_value_published_flags_v1'
    assert admitted == expected
    for progress in report['mission_progress']['players']:
        assert progress['eligible_ticks'] == progress['progress_ticks'] + progress['ticks_without_progress']
        assert progress['distance_observed_ticks'] <= progress['eligible_ticks']
    return dict(players=[D.finished_player(report, s) for s in range(2)],
        progress=report['mission_progress'],
        first_predictions=first_predictions(report, root/'mission-evaluations.jsonl'),
        decisions=P.switch_predictions(report, rows(root/'mission-evaluations.jsonl')),
        allocation=allocation_audit(root),
        timings={k:report.get(k) for k in ['sensors', 'policy', 'steps', 'measured_tick', 'mission_evaluation', 'flag_survey']},
        hashes={p.name:E.digest(p) for p in sorted(root.iterdir()) if p.is_file()})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip(), 'freeze code and plan first'
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=plan(), runs={}, comparisons=[], complete=False,
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        binary_sha256=E.digest(binary), tools={Path(m.__file__).name:E.digest(Path(m.__file__)) for m in [P, D, E]},
        runner_sha256=E.digest(Path(__file__)),
        scope='32 known directed trials and 32 fresh finished matches. Four generated worlds reused across asteroid pressure and seats; correlated, not 16 independent candidate matches. Same v13 predecessor with unused flag surveys versus opt-in published flag costs; v10 opponent. No weights, thresholds or defaults fitted.')
    save = lambda: D.write(args.out/'summary.json', result)
    save()
    for item in result['plan']:
        command, policies = arguments(item)
        name = item['name']
        try:
            run = D.run(binary, args.out, name, command, item['seat'],
                seconds=180 if item['kind'] == 'directed' else 600,
                require_finish=item['kind'] == 'held_out')
            run.update(analyze(args.out/name, item, policies))
            run['log_sha256'] = E.digest(args.out/(name+'.log'))
            result['runs'][name] = run
            save()
        except Exception as error:
            result['error'] = dict(case=item, command=command, error=repr(error))
            save()
            raise
    for item in result['plan']:
        if not item['candidate']:
            continue
        before, after = [args.out/(item['group']+suffix)/'report.json' for suffix in ['-predecessor', '-candidate']]
        a, b = [json.loads(p.read_text()) for p in [before, after]]
        assert a['initial_world'] == b['initial_world']
        same = D.same_physical_outcomes(a, b)
        switches = [r['missions'][item['seat']]['destination_planning']['switches'] for r in [a, b]]
        assert any(switches) or same, 'playing behavior changed without a destination switch'
        result['comparisons'].append(dict(group=item['group'], kind=item['kind'], seat=item['seat'],
            matching_recorded_physical_outcomes=same, predecessor_outcome=D.outcome(a, item['seat']),
            candidate_outcome=D.outcome(b, item['seat'])))
    assert E.digest(binary) == result['binary_sha256']
    result['complete'] = True
    save()


if __name__ == '__main__':
    main()
