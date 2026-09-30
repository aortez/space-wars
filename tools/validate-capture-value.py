#!/usr/bin/env python3
"""Predeclared v13 promotion gate: directed value cases and held-out matches."""
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import subprocess


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


D = module('destinations', 'compare-capture-destinations.py')
E = module('evaluations', 'compare-capture-evaluation.py')
V = module('value', 'compare-capture-value.py')


def same_f32(a, b):
    # Direct serde output uses the shortest f32 round trip; report.json goes
    # through serde_json::Value and prints that same f32 after widening to f64.
    return struct.pack('!f', a) == struct.pack('!f', b)


def plan():
    rows = []
    for version in [10, 12, 13]:
        rows.append(dict(name=f'regression-v{version}', group='regression', kind='regression',
            version=version, seed=186767996776005237, interval=0, seat=1))
    for world in ['destination', 'value-destination']:
        for seat in range(2):
            for bearing in [0.0, 0.4, 0.8, 1.2]:
                group = f'{world}-p{seat+1}-bearing{bearing}'
                for version in [10, 12, 13]:
                    rows.append(dict(name=f'{group}-v{version}', group=group, kind='directed',
                        world=world, version=version, seed=42, interval=0, seat=seat, bearing=bearing))
    for world in range(4):
        namespace = f'capture-value-promotion-v1:{world}'
        seed = int.from_bytes(hashlib.sha256(namespace.encode()).digest()[:8], 'little')
        for interval in [0, 3]:
            group = f'world{world}-asteroids{interval}'
            variants = [(10, None), (12, 0), (12, 1), (13, 0), (13, 1)]
            rotation = (world+interval) % len(variants)
            for version, seat in variants[rotation:]+variants[:rotation]:
                suffix = 'v10' if seat is None else f'v{version}-p{seat+1}'
                rows.append(dict(name=f'{group}-{suffix}', group=group, kind='held_out',
                    version=version, seed=seed, seed_namespace=namespace, interval=interval, seat=seat))
    return rows


def switch_predictions(report, evaluations):
    """Join the report used at each accepted switch to that exact actual visit."""
    players = [D.finished_player(report, seat) for seat in range(2)]
    wanted = {(seat, switch['source_tick']) for seat, player in enumerate(players)
        for switch in player['switches']}
    frozen = {}
    value_disagreements = {}
    for evaluation in evaluations:
        seat = {'player_1':0, 'player_2':1}[evaluation['actor']]
        key = seat, evaluation['source_tick']
        if key in wanted:
            old = frozen.setdefault(key, evaluation)
            if old != evaluation:
                raise ValueError('conflicting copies of the switch forecast')
        value = evaluation.get('value_comparison')
        if value and value['preferred'] is not None and evaluation['preferred_by_time'] is not None:
            if value['preferred'] != evaluation['preferred_by_time']:
                identity = seat, evaluation['selected_tick'], evaluation['current_target']
                value_disagreements.setdefault(identity, dict(seat=seat, source_tick=evaluation['source_tick'],
                    selected_tick=evaluation['selected_tick'], current=evaluation['current_target'],
                    time_preferred=evaluation['preferred_by_time'], value_preferred=value['preferred'],
                    candidates=evaluation['candidates']))
    records = []
    for seat, player in enumerate(players):
        for switch in player['switches']:
            forecast = frozen.get((seat, switch['source_tick']))
            if forecast is None:
                raise ValueError('switch has no recorded source forecast')
            assert forecast['current_target'] == switch['from']
            assert forecast['completed_tick'] <= switch['tick']
            current = next(c for c in forecast['candidates'] if c['planet'] == switch['from'])
            target = next(c for c in forecast['candidates'] if c['planet'] == switch['to'])
            assert same_f32(current['total_seconds'], switch['current_seconds'])
            assert same_f32(target['total_seconds'], switch['destination_seconds'])
            visits = [v for v in player['visits'] if v['planet'] == switch['to'] and v['selected_tick'] == switch['tick']]
            if len(visits) != 1:
                raise ValueError('switch does not identify exactly one actual visit')
            visit = visits[0]
            departure = visit['departed_tick']
            if departure is not None and departure < switch['tick']:
                raise ValueError('departure predates the accepted switch')
            actual = (departure-switch['source_tick'])/60 if departure is not None else None
            records.append(dict(seat=seat, switch=switch, source_forecast=forecast, actual_visit=visit,
                outcome='completed' if departure is not None else 'abandoned' if visit['abandoned_tick'] is not None else 'unfinished',
                actual_seconds_from_source=actual,
                milestone_references=milestone_references(target, switch['source_tick'], visit),
                error_seconds=switch['destination_seconds']-actual if actual is not None else None))
    return dict(switches=records, first_value_time_disagreements=list(value_disagreements.values()),
        scope='Frozen accepted-switch forecasts and exact visit identities; unchosen alternatives have no observed outcome. Value/time disagreements alone do not prove an admitted control decision.')


def milestone_references(candidate, source_tick, visit):
    """Cumulative source-time costs, never retimed to a later native milestone."""
    elapsed = candidate['travel_seconds']
    phases = candidate['local']
    records = []
    for milestone, costs in [('arrived_tick', []), ('landed_tick', ['landing']),
            ('claimed_tick', ['exit', 'outbound', 'claim']),
            ('boarded_tick', ['return_board']), ('departed_tick', ['departure'])]:
        elapsed += sum(phases[key] for key in costs)
        actual = visit[milestone]
        if actual is not None and actual < source_tick:
            raise ValueError('actual switch milestone predates its source')
        actual_seconds = None if actual is None else (actual-source_tick)/60
        records.append(dict(milestone=milestone, predicted_seconds=elapsed, actual_tick=actual,
            actual_seconds=actual_seconds,
            error_seconds=None if actual_seconds is None else elapsed-actual_seconds))
    return dict(records=records,
        scope='Original v12/v13 conditional cumulative transfer/local references. Acquisition delay and combat exposure are unmodelled; material changes after prediction are not collected. Missing milestones remain unknown.')


def compare(out, baseline, candidate, seat):
    a, b = [json.loads((out/name/'report.json').read_text()) for name in [baseline, candidate]]
    assert a['initial_world'] == b['initial_world'], 'paired initial world changed'
    same = D.same_physical_outcomes(a, b)
    switches = b['missions'][seat]['destination_planning']['switches']
    if a['policy_configuration'][seat]['policy'] == 'material_mission_v10':
        assert switches or same, 'different playing outcomes without a switch'
    return dict(baseline=baseline, candidate=candidate, seat=seat,
        matching_recorded_physical_outcomes=same,
        baseline_outcome=D.outcome(a, seat), candidate_outcome=D.outcome(b, seat))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip(), 'freeze code and plan first'
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=plan(), runs={}, comparisons=[], complete=False,
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        binary_sha256=E.digest(binary),
        tools={p.name:E.digest(p) for p in map(Path, [__file__, D.__file__, E.__file__, V.__file__])},
        scope='Existing v13 candidate versus retained v12/v10. Three known regression runs, 48 directed trials and 40 held-out finished matches. Four world seeds, reused controls and mirrored seats are correlated. No coefficients, thresholds or defaults change; the new conditional scan composition remains observational.')
    save = lambda: D.write(args.out/'summary.json', result)
    save()
    for item in result['plan']:
        name, seat = item['name'], item['seat']
        policies = [10, 10]
        if seat is not None: policies[seat] = item['version']
        directed = item['kind'] == 'directed'
        command = ['--world', item['world'] if directed else 'generated', '--seed', str(item['seed']),
            '--mode', 'quiet' if directed else 'duel', '--match', 'true', '--seat', str(seat or 0),
            '--asteroid-interval', str(item['interval']),
            '--p1-policy', f'material_mission_v{policies[0]}', '--p2-policy', f'material_mission_v{policies[1]}']
        if directed:
            command += ['--mirror', str(seat == 1).lower(), '--flag-bearing', str(item['bearing'])]
        try:
            run = D.run(binary, args.out, name, command, seat or 0,
                seconds=180 if directed else 600, require_finish=not directed)
            root = args.out/name
            report = json.loads((root/'report.json').read_text())
            assert report['seed'] == item['seed'] and report['round']['time_limit_seconds'] == 600
            assert [p['policy'] for p in report['policy_configuration']] == [f'material_mission_v{v}' for v in policies]
            assert report['live_objective_planning']['enabled_seats'] == []
            assert report['live_objective_planning']['allowance'] == {'graph':4, 'physics_queries':384}
            with (root/'mission-evaluations.jsonl').open() as stream:
                run['predictions'] = E.prediction_results(report, map(json.loads, stream))
            with (root/'mission-evaluations.jsonl').open() as stream:
                run['decisions'] = switch_predictions(report, map(json.loads, stream))
            run['players'] = [D.finished_player(report, s) for s in range(2)]
            run['coverage'] = V.coverage(root/'mission-evaluations.jsonl')
            allocation = E.allocation_audit([root, root])
            run['allocation_audit'] = dict(charges=allocation['charges'][0],
                local_rows=allocation['local_rows'],
                scope='Every recorded dispatch tick sums to its reported graph/query charge and stays within the shared 4/384 quota. Native synchronous sensors and snapshot construction are outside this quota.')
            run['timings'] = {key:report[key] for key in ['sensors', 'policy', 'steps', 'mission_evaluation']}
            run['hashes'] = {p.name:E.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
            run['log_sha256'] = E.digest(args.out/(name+'.log'))
            result['runs'][name] = run
            save()
        except Exception as error:
            result['error'] = dict(case=item, command=command, error=repr(error))
            save()
            raise
    for item in result['plan']:
        if item['version'] == 10: continue
        base = item['group']+'-v10'
        result['comparisons'].append(compare(args.out, base, item['name'], item['seat']))
        if item['version'] == 13:
            result['comparisons'].append(compare(args.out, item['name'].replace('-v13', '-v12'), item['name'], item['seat']))
    result['held_out_outcomes'] = {}
    for version in [12, 13]:
        pairs = [c for c in result['comparisons'] if c['baseline'].startswith('world') and
            c['baseline'].endswith('-v10') and f'-v{version}-' in c['candidate']]
        result['held_out_outcomes'][str(version)] = dict(candidate=dict(Counter(c['candidate_outcome'] for c in pairs)),
            control_same_seat=dict(Counter(c['baseline_outcome'] for c in pairs)))
    assert E.digest(binary) == result['binary_sha256']
    result['complete'] = True
    save()


if __name__ == '__main__': main()
