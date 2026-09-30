#!/usr/bin/env python3
"""Frozen v14 evidence/behavior experiment against retained v13; no tuning."""
import argparse
from collections import Counter
import csv
import gzip
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import importlib.util
from itertools import zip_longest


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


V = module('value_validation', 'validate-capture-value.py')
F = module('flag_surveys', 'compare-flag-surveys.py')
D, E = V.D, V.E
FLAG_KIND = 'historical published flag walk; cover and live feasibility unknown'


def plan():
    result = [dict(name=f'regression-v{v}', group='regression', kind='regression',
        version=v, seat=1, seed=186767996776005237, interval=0) for v in [13, 14]]
    for world in ['destination', 'value-destination']:
        for seat in range(2):
            for bearing in [0.0, 0.4, 0.8, 1.2]:
                group = f'{world}-p{seat+1}-bearing{bearing}'
                for version in [13, 14]:
                    result.append(dict(name=f'{group}-v{version}', group=group, kind='directed',
                        world=world, seat=seat, bearing=bearing, version=version, seed=42, interval=0))
    for world in range(4):
        namespace = f'survey-value-behavior-v1:{world}'
        seed = int.from_bytes(hashlib.sha256(namespace.encode()).digest()[:8], 'little')
        for interval in [0, 3]:
            group = f'world{world}-asteroids{interval}'
            variants = [(13, None), (14, 0), (14, 1)]
            shift = (world+interval) % len(variants)
            for version, seat in variants[shift:]+variants[:shift]:
                suffix = 'v13' if seat is None else f'v14-p{seat+1}'
                result.append(dict(name=f'{group}-{suffix}', group=group, kind='held_out',
                    seed=seed, seed_namespace=namespace, interval=interval, version=version, seat=seat))
    return result


def arguments(item):
    directed = item['kind'] == 'directed'
    policies = [10, 10] if item['kind'] == 'regression' else [13, 13]
    if item['seat'] is not None:
        policies[item['seat']] = item['version']
    result = ['--world', item['world'] if directed else 'generated', '--seed', str(item['seed']),
        '--mode', 'quiet' if directed else 'duel', '--match', 'true',
        '--seat', str(item['seat'] if directed else 0), '--asteroid-interval', str(item['interval']),
        '--p1-policy', f'material_mission_v{policies[0]}', '--p2-policy', f'material_mission_v{policies[1]}',
        '--survey-capture-flags', 'true', '--trace-destination-behavior', 'true']
    if directed:
        result += ['--mirror', str(item['seat'] == 1).lower(), '--flag-bearing', str(item['bearing'])]
    return result, policies


def rows(path):
    opener = gzip.open if path.suffix == '.gz' else open
    with opener(path, 'rt') as stream:
        yield from map(json.loads, stream)


def flag_references(evaluations, samples, policies=('material_mission_v14',)):
    """Bind every consumed reference to one published source, independently of Rust."""
    surveys = {(s['actor'], s['source_tick'], s['site']['planet'], s['site']['bearing']):s for s in samples}
    assert len(surveys) == len(samples)
    used, numeric, distinct = 0, 0, set()
    for r in evaluations:
        for c in r['candidates']:
            if c['evidence_kind'] != FLAG_KIND:
                continue
            assert r['policy'] in policies and r['model'] == 'capture_mission_survey_value_v1'
            identity = r['actor'], c['evidence_tick'], c['site']['planet'], c['site']['bearing']
            s = surveys[identity]
            assert s['reason'] is None and s['site'] == c['site']
            assert s['generation'] <= s['source_tick'] <= s['completed_tick'] == s['validated_tick'] <= r['source_tick']
            assert r['source_tick'] <= r['completed_tick']
            assert c['evidence_age_ticks'] == r['source_tick'] - s['source_tick'] <= 1800
            assert c['route_source_tick'] == s['source_tick'] and c['route_validated_tick'] == s['validated_tick']
            assert c['revision'] == s['measurement']['revision'] == s['objective']['revision']
            assert c['observed_owner'] == s['objective']['owner'] and c['observed_owner'] != r['actor']
            v = s['validation']
            assert v['model'] == 'captured_query_unions_v1'
            assert v['complete'] and v['predicates_valid'] and v['geometry']['valid'] and v['predicate_failure'] is None
            objective, source = s['objective'], v['source_objective']
            assert all(source[k] == objective[k] for k in ['planet', 'revision', 'owner'])
            assert math.dist([source['position'][k] for k in ['x','y']], [objective['position'][k] for k in ['x','y']]) <= .002
            assert abs(source['range']-objective['range']) <= .0001
            assert math.isfinite(v['source_radius']) and v['source_radius'] > 0
            m = s['measurement']
            site = m['site']
            assert m['tick'] == s['source_tick'] and m['finding'] == 'measured' and m['ship_form'] == 'ship'
            assert m['climb_clear'] is True and site['id'] == s['site'] and site['revision'] == c['revision']
            assert site['hatch_has_settling_margin'] and any(h is not None for h in site['boarding_hatches'])
            route = s['route']
            assert route['site'] == s['site'] and route['endpoint'] is not None and route.get('crossing') is None
            for leg, bound in [('outbound',10.599817),('returning',10.118012)]:
                path = route[leg]
                assert path['failure'] is None and not path['partial']
                assert path['jumps'] == path['flights'] == 0 and 0 <= path['length']/5 <= bound
            costs = dict(landing=23.033333, exit=1/60,
                outbound=s['route']['outbound']['length']/5+0.15833333,
                claim=6-0.5/60, return_board=s['route']['returning']['length']/5+2/60,
                departure=3.8166666)
            assert set(c['local']) == set(costs)
            assert all(math.isclose(c['local'][k], value, abs_tol=1e-5) for k, value in costs.items())
            if c['total_seconds'] is not None:
                assert c['unknown_reason'] is None
                assert math.isclose(c['total_seconds'], sum(costs.values())+c['travel_seconds'], abs_tol=1e-4)
                units = 1 if c['first_rebuild_foothold'] else 2
                assert c['value']['priority_units'] == units and c['value']['ownership_swing'] == 2
                assert math.isclose(c['value']['seconds_per_unit'], c['total_seconds']/units, abs_tol=1e-5)
                numeric += 1
            used += 1
            distinct.add(identity)
    return dict(references=used, numeric_references=numeric, distinct_surveys=len(distinct),
        scope='Repeated reports are not independent opportunities. Source geometry was validated at publication, not at comparison or arrival.')


def work_audit(root, report):
    """Reconcile native remote dispatcher -> evaluator -> flag queue each tick."""
    live, declared = {}, {}
    with (root/'live-planning.csv').open() as stream:
        for row in csv.DictReader(stream):
            entry = live.setdefault(int(row['tick']), dict(graph=0, physics_queries=0))
            entry['graph'] += int(row['graph'])
            entry['physics_queries'] += int(row['queries'])
            assert int(row['graph_budget']) == 4 and int(row['query_budget']) == 384
            value = dict(graph=int(row['total_graph']), physics_queries=int(row['total_queries']))
            assert declared.setdefault(int(row['tick']), value) == value
    assert live == declared
    evaluations = list(rows(root/'mission-evaluation-work.jsonl'))
    flags = list(rows(root/'flag-survey-work.jsonl'))
    assert len(evaluations) == len(flags) == report['elapsed_ticks']
    assert len({r['tick'] for r in evaluations}) == len(evaluations)
    cap = dict(graph=4, physics_queries=384)
    maximum, total = dict(graph=0, physics_queries=0), Counter()
    ticks = set()
    start = report['initial_world']['local']['combat']['recovery']['flight']['pilot']['tick']
    for offset, (e, f) in enumerate(zip(evaluations, flags)):
        assert e['tick'] == f['tick'] == start+offset
        ticks.add(e['tick'])
        previous = live.get(e['tick'], dict(graph=0, physics_queries=0))
        for kind in cap:
            assert e['remaining_before_evaluation'][kind] == cap[kind]-previous[kind]
            assert 0 <= e['charged'][kind] <= e['allowance'][kind] <= e['remaining_before_evaluation'][kind]
            assert f['remaining_after_evaluation'][kind] == e['remaining_before_evaluation'][kind]-e['charged'][kind]
            total['evaluation_'+kind] += e['charged'][kind]
        assert e['charged']['physics_queries'] == 0
        combined = F.charge(f)
        for kind in cap:
            maximum[kind] = max(maximum[kind], combined[kind])
            total['flag_'+kind] += f['allocation']['charged'][kind]
    assert live.keys() <= ticks
    assert total['evaluation_graph'] == report['mission_evaluation']['charged']
    for kind in cap:
        assert total['flag_'+kind] == report['flag_survey']['telemetry'][kind]
    return dict(ticks=len(ticks), maximum_combined_work=maximum, totals=dict(total),
        scope='Shared remote/evaluation work only; synchronous native sensors, snapshots, serialization and CPU time are outside the operation quota.')


def validated_trace(records, report):
    assert report['mode'] in ['duel','quiet']
    seats = [0,1] if report['mode'] == 'duel' else [report['seat']]
    start = report['initial_world']['local']['combat']['recovery']['flight']['pilot']['tick']
    count = 0
    for r in records:
        assert (r['tick'],r['seat']) == (start+count//len(seats),seats[count%len(seats)])
        count += 1
        yield r
    assert count == report['elapsed_ticks']*len(seats), 'missing or extra behavior rows'


def progress(records):
    """Direct range measurements during uninterrupted transfer, not a stall oracle."""
    state, result = {}, [dict(transfer_ticks=0, longest_no_range_gain_ticks=0,
        ticks_without_range_gain_after_10s=0, ownership_swing_tick_sum=0) for _ in range(2)]
    previous = {}
    for r in records:
        seat, tick = r['seat'], r['tick']
        assert seat in [0, 1] and (seat not in previous or tick == previous[seat]+1)
        previous[seat] = tick
        row = result[seat]
        owned = r['owned_planets']
        if owned is not None:
            row['ownership_swing_tick_sum'] += owned[seat]-owned[1-seat]
        distance = r['distance_to_target']
        if (r['goal'] != 'transfer' or not r['aboard'] or not r['ship_available']
                or r['target'] is None or r['selected_tick'] is None or distance is None):
            state.pop(seat, None)
            continue
        assert math.isfinite(distance) and distance >= 0
        row['transfer_ticks'] += 1
        key = r['target'], r['selected_tick'], r['ship_form']
        old = state.get(seat)
        if old is None or old['key'] != key or distance <= old['range']-2.0:
            old = state[seat] = dict(key=key, range=distance, tick=tick)
        age = tick-old['tick']
        row['longest_no_range_gain_ticks'] = max(row['longest_no_range_gain_ticks'], age)
        row['ticks_without_range_gain_after_10s'] += age >= 600
    return dict(players=result,
        scope='Per-tick measured range to the moving target. Reset on visit/form/transfer-phase change or >=2-unit improvement. Detours can legitimately fail this measure; landing, walking, recovery and combat stalls are not measured. Ownership integral describes recorded duration only.')


def compare_controls(out, a, b, reports):
    switches = [s['tick'] for r in reports for seat in range(2) for s in D.finished_player(r, seat)['switches']]
    cutoff = min(switches, default=math.inf)
    if switches:
        for report in reports:
            start = report['initial_world']['local']['combat']['recovery']['flight']['pilot']['tick']
            assert start+report['elapsed_ticks'] > cutoff, 'run ended before the first switch'
    left, right = [validated_trace(rows(out/name/'destination-behavior.jsonl.gz'), report)
        for name, report in zip([a,b],reports)]
    count = 0
    for x, y in zip_longest(left, right):
        if x is None or y is None:
            assert cutoff != math.inf
            break
        if x['tick'] >= cutoff or y['tick'] >= cutoff:
            break
        assert {k:v for k,v in x.items() if k != 'policy'} == {k:v for k,v in y.items() if k != 'policy'}
        count += 1
    same = D.same_physical_outcomes(*reports)
    assert switches or same, 'physical change without a switch'
    return dict(identical_behavior_rows_before_first_switch=count,
        first_switch_tick=None if cutoff == math.inf else cutoff, matching_recorded_physical_outcomes=same)


def predecessor_manifest(root):
    source = 'aa69ee52610a95439862ee3f99845bbdd238c610'
    experiment = json.loads((root/'plan.json').read_text())
    assert experiment['source'] == source
    assert experiment['cases'] == [[s,b] for s in range(2) for b in [0.0,0.4,0.8,1.2]]
    files = [root/'plan.json',root/'surface-mission-soak-aa69ee5']
    files += [root/f'p{s+1}-bearing{b}'/file for s,b in experiment['cases']
        for file in ['report.json','mission-evaluations.jsonl']]
    return dict(source_commit=source,root=str(root.resolve()),
        hashes={str(p.relative_to(root)):F.digest(p) for p in files})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--predecessor-exploration', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip(), 'freeze code and plan first'
    binary = args.binary.resolve(strict=True)
    predecessor = predecessor_manifest(args.predecessor_exploration)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=plan(), runs={}, comparisons=[], predecessor_parity=[], complete=False,
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        binary_sha256=F.digest(binary),
        predecessor=predecessor,
        tools={Path(m.__file__).name:F.digest(Path(m.__file__)) for m in [V, F, D, E, V.V]},
        runner_sha256=F.digest(Path(__file__)),
        scope='V14 remote evidence candidate versus v13, unchanged costs/value weights/hysteresis. 32 directed cases, two regression runs, 24 held-out finished matches over four correlated worlds. Defaults unchanged; not a whole-bot equal-CPU-budget claim.')
    save = lambda: D.write(args.out/'summary.json', result)
    save()
    for item in result['plan']:
        name, seat = item['name'], item['seat']
        command, policies = arguments(item)
        directed = item['kind'] == 'directed'
        try:
            run = D.run(binary, args.out, name, command, seat or 0,
                seconds=180 if directed else 600, require_finish=not directed)
            root = args.out/name
            report = json.loads((root/'report.json').read_text())
            assert report['seed'] == item['seed'] and report['round']['time_limit_seconds'] == 600
            assert [p['policy'] for p in report['policy_configuration']] == [f'material_mission_v{v}' for v in policies]
            assert report['live_objective_planning']['enabled_seats'] == []
            assert report['live_objective_planning']['allowance'] == dict(graph=4, physics_queries=384)
            run['players'] = [D.finished_player(report, s) for s in range(2)]
            run['predictions'] = E.prediction_results(report, rows(root/'mission-evaluations.jsonl'))
            run['decisions'] = V.switch_predictions(report, rows(root/'mission-evaluations.jsonl'))
            run['coverage'] = V.V.coverage(root/'mission-evaluations.jsonl')
            run['surveys'] = F.coverage(root)
            run['flag_references'] = flag_references(rows(root/'mission-evaluations.jsonl'), list(rows(root/'flag-survey.jsonl')))
            run['work'] = work_audit(root, report)
            run['progress'] = progress(validated_trace(rows(root/'destination-behavior.jsonl'), report))
            run['timings'] = {key:report[key] for key in ['sensors', 'policy', 'steps', 'mission_evaluation', 'flag_survey']}
            trace = root/'destination-behavior.jsonl'
            run['behavior_trace_sha256'] = F.digest(trace)
            with trace.open('rb') as src, gzip.open(str(trace)+'.gz', 'wb', compresslevel=1) as dest:
                shutil.copyfileobj(src, dest)
            trace.unlink()
            run['hashes'] = {p.name:F.digest(p) for p in root.iterdir() if p.is_file()}
            run['log_sha256'] = F.digest(args.out/(name+'.log'))
            result['runs'][name] = run
            if directed and item['world'] == 'value-destination' and item['version'] == 13:
                old = args.predecessor_exploration/f'p{seat+1}-bearing{item["bearing"]}'
                for file in ['report.json','mission-evaluations.jsonl']:
                    assert F.digest(old/file) == predecessor['hashes'][str((old/file).relative_to(args.predecessor_exploration))]
                old_report = json.loads((old/'report.json').read_text())
                assert D.same_physical_outcomes(old_report, report) and old_report['missions'] == report['missions']
                assert F.digest(old/'mission-evaluations.jsonl') == F.digest(root/'mission-evaluations.jsonl')
                result['predecessor_parity'].append(dict(case=name, reference_report_sha256=F.digest(old/'report.json'),
                    reference_evaluations_sha256=F.digest(old/'mission-evaluations.jsonl'), exact_evaluations_missions_and_outcomes=True))
            save()
        except Exception as error:
            result['error'] = dict(case=item, command=command, error=repr(error))
            save()
            raise
    for item in result['plan']:
        if item['version'] != 14: continue
        a, b = item['group']+'-v13', item['name']
        reports = [json.loads((args.out/n/'report.json').read_text()) for n in [a, b]]
        assert reports[0]['initial_world'] == reports[1]['initial_world']
        result['comparisons'].append(dict(baseline=a, candidate=b, seat=item['seat'],
            baseline_outcome=D.outcome(reports[0], item['seat']), candidate_outcome=D.outcome(reports[1], item['seat']),
            **compare_controls(args.out, a, b, reports)))
    assert F.digest(binary) == result['binary_sha256']
    result['complete'] = True
    save()


if __name__ == '__main__': main()
