#!/usr/bin/env python3
"""Calibrate and replay an offline, locally supported approach-state estimate.

Successful uninterrupted flights supply conditional durations, not completion
probabilities. Every other flight phase and all ground costs keep their frozen
models. Unsupported approach states never fall back to a duration countdown.
"""
import argparse
from collections import defaultdict
import copy
import json
import math
from pathlib import Path
import statistics

import importlib.util


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


composed = module('state_composition', 'compose-trip-estimates.py')
flight = module('state_geometry', 'inspect-flight-progress.py')
phase, frozen, costs = composed.phase, composed.frozen, composed.costs
MODEL = 'approach-state-trip-v1'
PROFILE_MODEL = 'approach-state-profile-v1'
SCALES = {'height': 12.0, 'side_error': 3.0, 'normal_speed': 4.0,
          'right_speed': 4.0, 'gravity_normal': 4.0, 'gravity_right': 3.0}
RULE = {'scales': SCALES, 'progress_tolerance': 4.0, 'minimum_attempts': 3,
        'minimum_worlds': 2, 'maximum_states_per_attempt': 64,
        'maximum_states_per_cell': 2048, 'aggregation': 'nearest_per_attempt_then_world_medians'}


def state_vector(record):
    p, g = record['phase'], record['geometry']
    if p['name'] != 'approach':
        return None, None, 'not_approach'
    if (not costs.finite(p.get('age_seconds')) or p['age_seconds'] < 0
            or not p.get('entry_observed')):
        return None, None, 'phase_entry_unobserved'
    if not g or any(not costs.finite(g.get(k)) for k in SCALES):
        return None, None, record['unavailable_reason'] or 'approach_state_missing'
    progress = record['one_second_progress']
    mode = 'entry' if p['age_seconds'] == 0 else 'progress'
    if mode == 'progress' and (not progress or not costs.finite(progress.get('distance_closed'))):
        return None, None, 'approach_progress_window_missing'
    values = {k: g[k] for k in SCALES}
    if mode == 'progress':
        values['distance_closed'] = progress['distance_closed']
    return f"{p['context']}/{mode}", values, None


def attempt_key(sample):
    return sample['recording_sha256'], sample['seat'], sample['selected_tick']


def add_training_run(cells, run, rows):
    """Reuse phase qualification; later ground failure does not erase a landing."""
    qualified = {}
    for attempt in run['attempts']:
        phase.add_samples(qualified, run, attempt, attempt['actual'])
    by_episode = defaultdict(list)
    for row in rows:
        by_episode[(row['seat'], row['selected_tick'], row['phase'].get('episode'))].append(row)
    exclusions, counts = [], defaultdict(int)
    for context in ['initial', 'retry']:
        cell = qualified.get(context + '/approach', {'samples': [], 'excluded': []})
        exclusions.extend(cell['excluded'])
        for episode in cell['samples']:
            for row in by_episode[(episode['seat'], episode['selected_tick'], episode['episode'])]:
                tick = row['tick']
                if not episode['entry_tick'] <= tick < episode['end_tick']:
                    continue
                name, vector, reason = state_vector(row)
                if reason:
                    continue
                if name.split('/')[0] != context:
                    raise ValueError('feature and outcome phase contexts differ')
                ident = (name, attempt_key(episode))
                if counts[ident] >= RULE['maximum_states_per_attempt']:
                    continue
                counts[ident] += 1
                samples = cells.setdefault(name, [])
                if len(samples) >= RULE['maximum_states_per_cell']:
                    raise ValueError('declared state capacity exceeded; revise development protocol')
                samples.append({**episode, 'tick': tick, 'state': vector,
                    'remaining_approach_seconds': (episode['end_tick'] - tick) / costs.HZ,
                    'post_approach_seconds_bounds': [v - episode['phase_seconds']
                                                    for v in episode['landing_seconds_bounds']]})
    return exclusions


def make_profile(runtime, cells, runs):
    output = {}
    for name, samples in cells.items():
        keys = list(SCALES) + (['distance_closed'] if name.endswith('/progress') else [])
        output[name] = {'samples': samples, 'domain': {
            k: [min(s['state'][k] for s in samples), max(s['state'][k] for s in samples)] for k in keys}}
    return {'version': 1, 'model': PROFILE_MODEL, 'runtime_revision': runtime,
            'rule': copy.deepcopy(RULE), 'runs': runs, 'cells': output}


class StateLookup:
    def __init__(self, profile):
        if profile['version'] != 1 or profile['model'] != PROFILE_MODEL or profile['rule'] != RULE:
            raise ValueError('incompatible approach-state profile or support rule')
        self.profile, self.index = profile, defaultdict(list)
        for name, cell in profile['cells'].items():
            if name not in ['initial/entry', 'initial/progress', 'retry/entry', 'retry/progress']:
                raise ValueError('unsupported state cell')
            if not cell['samples'] or len(cell['samples']) > RULE['maximum_states_per_cell']:
                raise ValueError('invalid state cell capacity')
            keys = set(SCALES) | ({'distance_closed'} if name.endswith('/progress') else set())
            if set(cell['domain']) != keys:
                raise ValueError('incomplete state domain')
            for sample in cell['samples']:
                values = sample['state']
                if (set(values) != set(cell['domain']) or any(not costs.finite(v) for v in values.values())
                        or any(not costs.finite(v) or v < 0 for v in
                               [sample['remaining_approach_seconds'], *sample['post_approach_seconds_bounds']])
                        or sample['post_approach_seconds_bounds'][0] > sample['post_approach_seconds_bounds'][1]):
                    raise ValueError('invalid training state or duration')
                bucket = math.floor(values['height'] / SCALES['height'])
                self.index[name, bucket].append(sample)
            for k, span in cell['domain'].items():
                if span != [min(s['state'][k] for s in cell['samples']), max(s['state'][k] for s in cell['samples'])]:
                    raise ValueError('training domain differs from measured states')

    def predict(self, record):
        name, values, reason = state_vector(record)
        result = {'seconds': None, 'envelope_seconds': None, 'reason': reason, 'cell': name,
            'support': 0, 'support_seeds': 0, 'examined_states': 0, 'matched_states': 0,
            'approach_seconds': None, 'post_approach_flight_seconds': None}
        if reason:
            return result
        cell = self.profile['cells'].get(name)
        if not cell:
            return {**result, 'reason': 'no_approach_state_calibration'}
        if any(not cell['domain'][k][0] <= v <= cell['domain'][k][1] for k, v in values.items()):
            return {**result, 'reason': 'approach_state_outside_training_domain'}
        tolerances = {**SCALES, 'distance_closed': RULE['progress_tolerance']}
        bucket = math.floor(values['height'] / SCALES['height'])
        nearest = {}
        for b in [bucket - 1, bucket, bucket + 1]:
            for sample in self.index[name, b]:
                result['examined_states'] += 1
                differences = [abs(v - sample['state'][k]) / tolerances[k] for k, v in values.items()]
                if any(d > 1 for d in differences):
                    continue
                result['matched_states'] += 1
                rank = (sum(d * d for d in differences), sample['tick'])
                ident = attempt_key(sample)
                if ident not in nearest or rank < nearest[ident][0]:
                    nearest[ident] = rank, sample
        samples = [v[1] for v in nearest.values()]
        worlds = defaultdict(list)
        for s in samples:
            worlds[s['seed']].append(s)
        result.update(support=len(samples), support_seeds=len(worlds))
        if len(samples) < RULE['minimum_attempts'] or len(worlds) < RULE['minimum_worlds']:
            return {**result, 'reason': 'insufficient_local_approach_support'}
        balanced = lambda fn: statistics.median(statistics.median(fn(s) for s in rows) for rows in worlds.values())
        approach = balanced(lambda s: s['remaining_approach_seconds'])
        tail = balanced(lambda s: sum(s['post_approach_seconds_bounds']) / 2)
        envelope = [min(s['remaining_approach_seconds'] for s in samples)
                    + min(s['post_approach_seconds_bounds'][0] for s in samples),
                    max(s['remaining_approach_seconds'] for s in samples)
                    + max(s['post_approach_seconds_bounds'][1] for s in samples)]
        return {**result, 'seconds': approach + tail, 'envelope_seconds': envelope,
                'approach_seconds': approach, 'post_approach_flight_seconds': tail}


class StateTrip(composed.ComposedTrip):
    def __init__(self, selection, ground, landing, walking, lookup):
        super().__init__(selection, ground, landing, walking, walk_model='short-and-affine')
        self.lookup, self.flight = lookup, flight.FlightProgress()
        self.first_state = None

    def observe(self, row):
        if self.terminal:
            return None
        state = self.flight.observe(row)
        result = super().observe(row)
        # The standalone diagnostic starts a fresh capture scope; the mission
        # comparator correctly retains retry context across a capture restart.
        # Keep its authoritative phase clock while retaining the reset window.
        if result.get('flight_phase') is not None:
            state['phase'] = copy.deepcopy(result['flight_phase'])
        result.update(model=MODEL, duration_total_seconds=result['total_seconds'],
            duration_status=result['status'], duration_unknown_reasons=list(result['unknown_reasons']),
            duration_landing=copy.deepcopy(result.get('landing')), approach_state=state)
        # Apply only after the unchanged comparator has run. Never feed the new
        # forecast into its anchors, clocks, phase-only totals or first result.
        if result.get('landing') is not None and result['flight_phase']['name'] == 'approach':
            landing = self.lookup.predict(state)
            reasons = [r for r in result['unknown_reasons'] if r != result['landing']['reason']]
            if landing['reason']:
                reasons.append(landing['reason'])
            result.update(landing=landing, status='unknown', unknown_reasons=reasons,
                remaining_seconds=None, total_seconds=None, remaining_historical_envelope_seconds=None,
                total_historical_envelope_seconds=None)
            if not reasons:
                remaining = landing['seconds'] + sum(p['estimate_seconds'] for p in self.tail.values())
                envelope = [landing['envelope_seconds'][i]
                    + sum(p['historical_envelope_seconds'][i] for p in self.tail.values()) for i in [0, 1]]
                result.update(status='estimate', remaining_seconds=remaining,
                    total_seconds=result['elapsed_seconds'] + remaining,
                    remaining_historical_envelope_seconds=envelope,
                    total_historical_envelope_seconds=[result['elapsed_seconds'] + v for v in envelope])
            until = result['budget']['seconds_until_time_limit']
            result['budget']['estimated_trip_exceeds_time_limit'] = (
                result['remaining_seconds'] > until if result['remaining_seconds'] is not None and until is not None else None)
        if self.first_state is None and self.first_choice_tick is not None:
            self.first_state = copy.deepcopy(result)
        return result

    def record(self):
        return {**super().record(), 'first_state_prediction': self.first_state}


def evaluate_run(run, attempts, updates):
    updates = list(updates)
    indexed = {(frozen.key(u), u['tick']): u for u in updates}
    result = composed.evaluate_run(run, attempts, updates)
    for attempt in result['attempts']:
        actual = attempt['actual']
        for c in attempt['checkpoints']:
            u = indexed.get((frozen.key(attempt['selection']), c['tick']))
            eligible = u and c['status'] not in ['already_landed', 'attempt_ended', 'missing_observation']
            c['duration_total_seconds'] = u['duration_total_seconds'] if eligible else None
            c['duration_status'] = u['duration_status'] if eligible else c['status']
            c['duration_unknown_reasons'] = u['duration_unknown_reasons'] if eligible else []
            total = actual['phases']['total']['seconds_bounds']
            old = c['duration_total_seconds']
            c['duration_error_seconds_bounds'] = [old - total[1], old - total[0]] if old is not None and actual['ending'] == 'completed' else None
            c['approach_state'] = u['approach_state'] if eligible else None
            c['state_landing'] = u.get('landing') if eligible else None
            c['walking_legs'] = u.get('walking_legs', {}) if eligible else {}
            landed = actual['milestones'].get('landed')
            point = u.get('landing', {}).get('seconds') if eligible and u.get('landing') else None
            c['state_landing_error_seconds_bounds'] = ([point + (c['tick'] - t) / costs.HZ for t in reversed(landed)]
                if point is not None and landed and c['tick'] < landed[0] <= landed[1] < actual['stopped_tick'] else None)
    return result


def summarize(runs):
    result = composed.summarize(runs)
    for seconds in composed.rolling.CHECKPOINT_SECONDS:
        rows = [c for r in runs for a in r['attempts'] for c in a['checkpoints'] if c['after_seconds'] == seconds]
        paired = [r for r in rows if all(r[n + '_error_seconds_bounds'] is not None for n in ['duration', 'combined'])]
        result['checkpoints'][str(seconds)]['state_vs_duration'] = {
            'duration_numeric': sum(r['duration_total_seconds'] is not None for r in rows),
            'state_numeric': sum(r['combined_total_seconds'] is not None for r in rows),
            'gained': sum(r['combined_total_seconds'] is not None and r['duration_total_seconds'] is None for r in rows),
            'lost': sum(r['combined_total_seconds'] is None and r['duration_total_seconds'] is not None for r in rows),
            'paired_completed': len(paired), 'paired_error': {n: frozen.error_summary([r[n + '_error_seconds_bounds'] for r in paired])
                for n in ['duration', 'combined']}}
    return result


def calibrate(manifest_path, out):
    manifest, base = json.loads(manifest_path.read_text()), manifest_path.resolve().parent
    cells, sources, excluded, seen, datasets = {}, [], [], set(), []
    for dataset in manifest['datasets']:
        evaluation_path = base / dataset['evaluation']
        evaluation = json.loads(evaluation_path.read_text())
        datasets.append({'name': dataset['name'], 'evaluation_sha256': costs.file_hash(evaluation_path)})
        runs = {r['label']: r for r in evaluation['runs']}
        for config in dataset['features']:
            path = base / config['path']
            metadata = json.loads((base / config['metadata']).read_text())
            run = runs[config['label']]
            if (run['source_commit'] != manifest['runtime_revision'] or metadata['model'] != flight.MODEL
                    or metadata['trace_sha256'] != run['trace_sha256'] or metadata['output_sha256'] != costs.file_hash(path)):
                raise ValueError('unbound training features or incompatible runtime')
            ident = run['trace_sha256'], metadata['seat']
            if ident in seen and run['attempts']:
                raise ValueError('duplicate training recording and seat')
            seen.add(ident)
            rows = [json.loads(line) for line in path.read_text().splitlines()]
            if any(row['seat'] != metadata['seat'] or row['model'] != flight.MODEL for row in rows):
                raise ValueError('feature identity differs from source')
            source = {**frozen.source_summary(run), 'label': dataset['name'] + '/' + run['label'],
                      'features_sha256': metadata['output_sha256']}
            excluded.extend(add_training_run(cells, {**run, 'label': source['label']}, rows))
            sources.append(source)
    profile = make_profile(manifest['runtime_revision'], cells, sources)
    profile.update(training_manifest_sha256=costs.file_hash(manifest_path), datasets=datasets, excluded=excluded)
    StateLookup(profile)
    frozen.write_json(out / 'profile.json', profile)
    print(json.dumps({name: len(c['samples']) for name, c in profile['cells'].items()}))


def evaluate(args):
    manifest = json.loads(args.manifest.read_text())
    scope = manifest.get('scope', 'missions')
    if scope == 'missions':
        frozen.load_manifest(args.manifest)
    elif (scope != composed.controlled.SCOPE or manifest['version'] != 1 or not manifest['runs']
            or len({c['label'] for c in manifest['runs']}) != len(manifest['runs'])
            or any(c['seat'] not in [0, 1] for c in manifest['runs'])):
        raise ValueError('invalid controlled manifest')
    paths = {'ground': args.ground_profile, 'phase': args.phase_profile, 'walking': args.walking_profile}
    hashes = {k: costs.file_hash(p) for k, p in paths.items()}
    profiles = [json.loads(p.read_text()) for p in paths.values()]
    composed.validate_profiles(manifest, profiles, hashes['ground'])
    profile = json.loads(args.state_profile.read_text())
    lookup = StateLookup(profile)
    if profile['runtime_revision'] != manifest['runtime_revision']:
        raise ValueError('approach-state runtime differs')
    provenance = {'version': 1, 'model': MODEL, 'scope': scope, 'runtime_revision': manifest['runtime_revision'],
        'manifest_sha256': costs.file_hash(args.manifest), 'profile_sha256': hashes,
        'state_profile_sha256': costs.file_hash(args.state_profile),
        'source_evaluation_sha256': costs.file_hash(args.source_evaluation)}
    base, forecasts = args.manifest.resolve().parent, []
    for i, config in enumerate(manifest['runs']):
        trace = base / config['directory'] / 'trace.jsonl'
        trace_hash = costs.file_hash(trace)
        path = args.out / f'updates-{i}.jsonl'
        factory = lambda selection: StateTrip(selection, *profiles, lookup)
        with path.open('w') as output:
            if scope == composed.controlled.SCOPE:
                attempts = composed.replay_controlled(config, base, factory, output, trace_hash)
            elif scope == 'missions':
                attempts = composed.rolling.replay(config, base, profiles[0], output,
                    trip_factory=lambda selection, _: factory(selection))
                if costs.file_hash(trace) != trace_hash:
                    raise ValueError('trace changed during replay')
            else:
                raise ValueError('unknown evaluation scope')
        forecasts.append({'label': config['label'], 'attempts': attempts, 'trace_sha256': trace_hash,
            'updates_file': path.name, 'updates_sha256': costs.file_hash(path)})
        print(config['label'], len(attempts), flush=True)
    frozen.write_json(args.out / 'predictions.json', {**provenance, 'runs': forecasts})
    source = json.loads(args.source_evaluation.read_text())
    if costs.file_hash(args.source_evaluation) != provenance['source_evaluation_sha256']:
        raise ValueError('outcome source changed')
    composed.verify_source(source, manifest, provenance['manifest_sha256'], hashes)
    results, seen = [], set()
    for config, prediction, run in zip(manifest['runs'], forecasts, source['runs']):
        if (run['trace_sha256'] != prediction['trace_sha256'] or run['source_commit'] != manifest['runtime_revision']
                or run['report_sha256'] != costs.file_hash(base / config['directory'] / 'report.json')):
            raise ValueError('outcome recording or runtime changed')
        identities = composed.evaluation_identities(run, config, prediction, scope)
        if seen & identities:
            raise ValueError('duplicate evaluation recording and seat')
        seen |= identities
        for p in [*profiles, profile]:
            frozen.ensure_independent(p, run)
        trips = ([a['actual_trip'] for a in run['attempts']] if scope == composed.controlled.SCOPE else
                 [a['actual'] for a in run['attempts'] if a['actual']] + run['unobserved_attempts'])
        with (args.out / prediction['updates_file']).open() as stream:
            result = evaluate_run({**run, 'trips': trips}, prediction['attempts'], map(json.loads, stream))
        composed.verify_baseline(result, run, scope)
        if scope == composed.controlled.SCOPE:
            result['trial_outcome'] = run['outcome']
        results.append(result)
    frozen.write_json(args.out / 'evaluation.json', {**provenance, 'runs': results, 'summary': summarize(results),
        'limitations': ['Conditional on uninterrupted landing; no success or interruption probability.',
            'State support is local and bounded; unsupported approaches never fall back to phase age.',
            'Other flight phases and all ground models retain their frozen estimates.',
            'World-balanced empirical component ranges are not confidence intervals or permissions.',
            'Replay alone does not establish new validation; generation must follow a frozen protocol.']})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['calibrate', 'evaluate'])
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    for name in ['source-evaluation', 'ground-profile', 'phase-profile', 'walking-profile', 'state-profile']:
        parser.add_argument('--' + name, type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    if args.mode == 'calibrate':
        calibrate(args.manifest, args.out)
    else:
        if not all([args.source_evaluation, args.ground_profile, args.phase_profile, args.walking_profile, args.state_profile]):
            parser.error('evaluate requires source evaluation and all four profiles')
        evaluate(args)


if __name__ == '__main__':
    main()
