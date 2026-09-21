#!/usr/bin/env python3
"""Evaluate the frozen ground model on explicitly controlled physical flag trips.

The adapter reuses the existing snapshot, phase and diagnostic functions. Its
synthetic selected/arrived events mean capture-controller start, never normal
mission selection. Controlled recordings cannot be used as ordinary matches.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

SPEC = importlib.util.spec_from_file_location('ground_validation',
    Path(__file__).with_name('validate-ground-costs.py'))
ground = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ground)
frozen, costs = ground.frozen, ground.costs
SCOPE = 'controlled_ground_v1'


def adapt(raw):
    if raw['version'] != 1 or raw['scope'] != SCOPE:
        raise ValueError('expected an explicit controlled-ground trace')
    start = raw['capture_started_tick']
    if start is not None and (type(start) is not int or not 0 <= start <= raw['tick']):
        raise ValueError('invalid or future capture-controller start')
    return {'version': 1, 'tick': raw['tick'], 'seat': raw['seat'],
        'scope': SCOPE, 'observation': {'local': raw['observation']},
        'mission': {'policy': frozen.POLICY, 'target': 0,
            'capture': raw['capture'] if start is not None else None,
            'events': [{'kind': kind, 'tick': start, 'planet': 0} for kind in ['selected', 'arrived']]
                      if start is not None else []},
        'controls': raw['controls'], 'posture': raw.get('posture')}


def prefix(path, seat, profile):
    result = {'selection': None, 'choice': None, 'prediction': None, 'post_choice_changes': [],
        'first_frame_change_tick': None, 'first_ship_loss_tick': None,
        'mission_departed_tick': None, 'native_milestones': {}}
    previous, start_seen, dependencies = -1, None, {}
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for line in stream:
            digest.update(line)
            raw = json.loads(line)
            row = adapt(raw)
            tick, p = row['tick'], frozen.pilot(row)
            if row['seat'] != seat or p['owner'] != f'player_{seat + 1}' or p['tick'] != tick or tick <= previous:
                raise ValueError('invalid controlled trace identity/order')
            previous = tick
            start = raw['capture_started_tick']
            if start_seen is not None and start != start_seen:
                raise ValueError('capture start changed within a controlled trial')
            if start is None:
                continue
            start_seen = start
            if result['selection'] is None:
                result['selection'] = frozen.snapshot(row, start, 'mission_selection')
            c = row['mission']['capture']
            if any(result[k] is not None for k in ['first_frame_change_tick', 'first_ship_loss_tick', 'mission_departed_tick']):
                continue
            if not p['ship_available'] or p['ship_form'] != 'ship':
                result['first_ship_loss_tick'] = tick
                continue
            if p['planet']['index'] != 0:
                result['first_frame_change_tick'] = tick
                continue
            for name in ['landed', 'claimed', 'boarded']:
                value = c['landing'].get(name + '_tick') if c else None
                if value is not None:
                    if not start <= value <= tick:
                        raise ValueError('invalid native milestone in controlled scope')
                    result['native_milestones'].setdefault(name, value)
            # Match mission_pilot.rs's coordinator gate. The standalone tactical
            # controller instead waits for three seconds of clear flight. Use
            # milestones known before this tick, as the coordinator does.
            marks = result['native_milestones']
            if (all(name in marks and marks[name] < tick for name in ['claimed', 'boarded'])
                    and p['location'] != 'on_foot'
                    and costs.point_distance(p['ship']['position'], p['planet']['motion']['position']) > p['planet']['radius'] + 70):
                result['mission_departed_tick'] = tick
                continue
            if (result['choice'] is None and c and c['site'] is not None
                    and p['location'] != 'on_foot' and p['ship_available'] and p['ship_form'] == 'ship'
                    and c['landing']['landed_tick'] is None):
                choice = frozen.snapshot(row, start, 'local_choice')
                result['choice'] = choice
                result['prediction'] = frozen.estimate(choice, profile)
            if result['choice'] and c:
                values = {'ship_form': p['ship_form']}
                if p['planet']['index'] == 0:
                    values['revision'] = p['planet']['revision']
                if c['landing']['landed_tick'] is None:
                    values.update(site=c['site'], route=c['objective_route'])
                for name, value in values.items():
                    if name in dependencies and dependencies[name] != value:
                        result['post_choice_changes'].append({'tick': tick, 'dependency': name})
                    dependencies[name] = value
    result['trace_sha256'] = digest.hexdigest()
    return result


def make_trip(report, observed, seat):
    if observed['selection'] is None:
        if report['capture_started_tick'] is not None:
            raise ValueError('capture started but no trace observation was retained')
        return None
    start = observed['selection']['selected_tick']
    if start != report['capture_started_tick']:
        raise ValueError('report and trace disagree on capture start')
    capture = report['capture']
    completed, changed = observed['mission_departed_tick'], observed['first_frame_change_tick']
    failed = capture['failed_tick']
    elapsed = report['elapsed_ticks']
    stops = [(elapsed, 'time_limit' if elapsed >= report['seconds'] * costs.HZ else 'stopped', None)]
    if failed is not None:
        stops.append((failed + 1, 'abandoned', capture['failure']))
    if changed is not None:
        stops.append((changed, 'frame_changed', 'left controlled destination frame'))
    if observed['first_ship_loss_tick'] is not None:
        stops.append((observed['first_ship_loss_tick'], 'ship_lost', 'capture requires ship recovery'))
    if completed is not None:
        stops.append((completed, 'completed', None))
    priority = {'completed': 0, 'ship_lost': 1, 'frame_changed': 2, 'abandoned': 3, 'time_limit': 4, 'stopped': 5}
    stop, ending, reason = min(stops, key=lambda s: (s[0], priority[s[1]]))
    if not start <= stop <= report['elapsed_ticks']:
        raise ValueError('invalid controlled timing bounds')
    milestones = {'selected': [start, start], 'arrived': [start, start]}
    for name in ['landed', 'claimed', 'boarded', 'departed']:
        tick = completed if name == 'departed' else observed['native_milestones'].get(name)
        if tick is not None:
            if not start <= tick <= stop:
                raise ValueError('controlled milestone outside its attempt')
            milestones[name] = [tick, tick]
    return costs.Trip(seat, 0, start, stop, ending, reason, milestones, frozen.POLICY, 0)


def rows(path, expected_hash):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for raw in stream:
            digest.update(raw)
            yield adapt(json.loads(raw))
    if digest.hexdigest() != expected_hash:
        raise ValueError('controlled trace changed after predictions were frozen')


def evaluate(report, observed, config, path):
    trip = make_trip(report, observed, config['seat'])
    if trip is None:
        return None
    for row in rows(path, observed['trace_sha256']):
        trip.observe(row)
    actual = trip.result()
    observed = {**observed, 'recorded_attempt': actual,
        'actual': frozen.compare(actual, observed['choice'], observed['prediction'])
                  if observed['choice'] else None}
    result = ground.assess(observed, 4.0)
    result['scope_events'] = {k: observed[k] for k in ['first_frame_change_tick', 'first_ship_loss_tick', 'mission_departed_tick', 'native_milestones']}
    result['execution'] = {}
    windows = []
    for phase in ['outbound', 'claim', 'return_board']:
        if actual['phases'][phase]['status'] not in ['completed', 'censored']:
            continue
        begins, ends = ground.ENDPOINTS[phase]
        start = actual['milestones'][begins][1]
        stop = actual['milestones'].get(ends, [actual['stopped_tick']] * 2)[0]
        window = ground.ExecutionWindow(result, phase, start, max(start, stop))
        result['execution'][phase] = window.stats
        windows.append(window)
    for row in rows(path, observed['trace_sha256']):
        for window in windows:
            if not window.start <= row['tick'] < window.stop:
                continue
            window.observe(row)
            first = window.stats['first_route']
            if first and first['observed_tick'] == row['tick']:
                path_nodes = row['mission']['capture']['ground'].get('path', [])
                deltas = {(b - a + 256) % 512 - 256 for a, b in zip(path_nodes, path_nodes[1:])}
                signs = {(x > 0) - (x < 0) for x in deltas}
                first['path_direction'] = next(iter(signs)) if len(signs) == 1 else None
                first['path_nodes'] = len(path_nodes)
    for window in windows:
        window.stats['missing_ticks'] = window.stats['interval_ticks'] - window.stats['sampled_ticks']
        window.stats['dense'] = window.stats['missing_ticks'] == 0
    choice = result['choice']
    band = config['band']
    result['first_choice_in_requested_band'] = bool(choice and choice['category'] == 'walk'
        and all(costs.finite(choice['references'][p]) and band['minimum'] <= choice['references'][p] * 5 < band['maximum']
                for p in ['outbound', 'return_board']))
    return result


def trial_outcome(setup, attempt):
    # Physical preparation can leave the initial planet before capture starts.
    # Keep the whole trial, including any zero-length frame-change attempt, but
    # never label a route measured on another planet as a valid target setup.
    if setup['setup'] and setup['setup']['survey']['objective']['planet'] != 0:
        return 'setup_wrong_planet'
    return attempt['ending'] if attempt else 'setup_unavailable'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--profile', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    manifest, profile = json.loads(args.manifest.read_text()), json.loads(args.profile.read_text())
    configs = manifest['runs']
    if (manifest['version'] != 1 or manifest['scope'] != SCOPE or not configs
            or manifest['runtime_revision'] != profile['runtime_revision'] or profile['model'] != frozen.MODEL
            or len({c['label'] for c in configs}) != len(configs)):
        raise ValueError('explicit controlled scope, one frozen runtime and unique trials required')
    for c in configs:
        if c['seat'] not in [0, 1] or c['seed'] in {r['seed'] for r in profile['runs']}:
            raise ValueError('invalid seat or training world reused for evaluation')
    base = args.manifest.resolve().parent
    observed = [prefix(base / c['directory'] / 'trace.jsonl', c['seat'], profile) for c in configs]
    args.out.mkdir(parents=True, exist_ok=True)
    frozen.write_json(args.out / 'predictions.json', {'scope': SCOPE, 'profile_sha256': costs.file_hash(args.profile),
        'runs': [{'label': c['label'], **{k: o[k] for k in ['selection', 'choice', 'prediction', 'trace_sha256']}}
                 for c, o in zip(configs, observed)]})
    runs = []
    for c, o in zip(configs, observed):
        directory = base / c['directory']
        report = json.loads((directory / 'report.json').read_text())
        setup = report['controlled_ground']
        if (report['version'] != 1 or report['world'] != 'generated' or report['seed'] != c['seed'] or report['seat'] != c['seat']
                or report['mode'] != 'capture' or not report['survey_landing'] or report['landing_threat']
                or report['edit'] != 'none' or report['policy_configuration']['policy'] != frozen.POLICY
                or not setup or setup['version'] != 1 or setup['band'] != c['band']):
            raise ValueError('controlled report does not match the declared trial')
        if not report['audit_passed'] or report['audit_failures'] or report['final_audit']['issues']:
            raise ValueError('controlled physical audit failed')
        source = {'label': c['label'], 'seed': c['seed'], 'source_commit': manifest['runtime_revision'],
            'report_sha256': costs.file_hash(directory / 'report.json'), 'trace_sha256': o['trace_sha256']}
        frozen.ensure_independent(profile, source)
        a = evaluate(report, o, c, directory / 'trace.jsonl')
        runs.append({**source, 'scope': SCOPE, 'setup': setup, 'attempts': [a] if a else [],
            'outcome': trial_outcome(setup, a), 'elapsed_ticks': report['elapsed_ticks']})
        print(f"{c['label']}: {runs[-1]['outcome']}", flush=True)
    frozen.write_json(args.out / 'evaluation.json', {'version': 1, 'model': 'controlled-ground-evaluation-v1',
        'scope': SCOPE, 'manifest_sha256': costs.file_hash(args.manifest), 'profile_sha256': costs.file_hash(args.profile),
        'runs': runs, 'summary': ground.summarize(runs), 'trials': len(runs),
        'setup_unavailable': sum(r['outcome'] == 'setup_unavailable' for r in runs),
        'limitations': ['conditional ground costs only; controlled capture start is not mission selection',
            'setup geometry queries are outside the execution planning quota',
            'no fitting, completion probability, remote-transfer estimate or physical permission']})


if __name__ == '__main__':
    main()
