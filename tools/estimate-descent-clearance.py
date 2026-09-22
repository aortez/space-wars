#!/usr/bin/env python3
"""Offline descent costs from current foot rays and locally supported motion.

The explicit candidate replaces only descent. Other phases retain the frozen
duration-first approach model. Successful episodes supply conditional durations,
not completion probabilities or permission to land.
"""
import argparse
from collections import Counter, defaultdict
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


approach = module('descent_approach', 'estimate-approach-state.py')
diagnostic = module('descent_diagnostic', 'inspect-landing-tail.py')
composed, phase, frozen, costs = approach.composed, approach.phase, approach.frozen, approach.costs
MODEL = 'descent-clearance-trip-v1'
PROFILE_MODEL = 'descent-clearance-profile-v1'
SCALES = {'clearance': 4.0, 'foot_gap': 2.0, 'descent_speed': 2.0,
          'lateral_speed': 1.0, 'relative_spin': 0.5, 'angle_degrees': 15.0,
          'assist_strength': 0.2, 'gravity_normal': 4.0, 'gravity_right': 2.0}
RULE = {'scales': SCALES, 'target_descent_speed': 2.0, 'minimum_assist': 0.5,
        'minimum_attempts': 3, 'minimum_worlds': 2, 'sample_spacing_ticks': 60,
        'maximum_states_per_attempt': 32, 'maximum_states_per_cell': 2048,
        'aggregation': 'nearest_per_attempt_then_world_median_residuals',
        'selection': 'strict_descent_other_phases_unchanged'}


def state_vector(record):
    clock, contact, geometry = record['phase'], record['contact'], record['geometry']
    if clock['name'] != 'descent':
        return None, None, 'not_descent'
    if (not clock.get('entry_observed') or not costs.finite(clock.get('age_seconds'))
            or clock['age_seconds'] < 0):
        return None, None, 'phase_entry_unobserved'
    if geometry is None:
        return None, None, record['geometry_unavailable'] or 'descent_frame_unavailable'
    if (not contact['matches_target'] or type(contact['supported_feet']) is not int
            or contact['supported_feet'] != 0):
        return None, None, 'descent_contact_or_frame_mismatch'
    feet = contact['foot_clearances']
    if (feet is None or len(feet) != 2 or any(not costs.finite(v) or not 0 <= v < 26 for v in feet)
            or contact['ray_limit_or_no_hit'] is not False):
        return None, None, 'descent_foot_rays_unavailable'
    values = {k: contact.get(k) for k in SCALES if k not in ['clearance', 'foot_gap', 'gravity_normal', 'gravity_right']}
    values.update(clearance=min(feet), foot_gap=abs(feet[1] - feet[0]),
                  gravity_normal=geometry.get('gravity_normal'), gravity_right=geometry.get('gravity_right'))
    if any(not costs.finite(v) for v in values.values()):
        return None, None, 'descent_motion_unavailable'
    if contact['phase'] != 'assisted' or not RULE['minimum_assist'] <= values['assist_strength'] <= 1:
        return None, None, 'descent_assist_unsupported'
    return clock['context'], values, None


def reference_seconds(values):
    return values['clearance'] / RULE['target_descent_speed']


def add_training_run(cells, run, traces):
    """Keep phase qualification and every exclusion; sample by time, not outcome."""
    qualified, exclusions, rejected = {}, [], Counter()
    attempts = {frozen.key(a['selection']): a for a in traces['attempts']}
    for attempt in run['attempts']:
        trace = attempts[frozen.key(attempt['selection'])]
        for name in ['selection', 'phase_episodes', 'phase_breaks']:
            if trace[name] != attempt[name]:
                raise ValueError('training trace lifecycle differs from evaluation')
        phase.add_samples(qualified, run, attempt, attempt['actual'])
    samples_by_episode = defaultdict(list)
    for a in traces['attempts']:
        for row in a['samples']:
            samples_by_episode[(a['selection']['seat'], a['selection']['selected_tick'], row['phase'].get('episode'))].append(row)
    counts = Counter()
    for context in ['initial', 'retry']:
        cell = qualified.get(context + '/descent', {'samples': [], 'excluded': []})
        exclusions.extend(cell['excluded'])
        for episode in cell['samples']:
            last = None
            for row in samples_by_episode[episode['seat'], episode['selected_tick'], episode['episode']]:
                tick = row['tick']
                if not episode['entry_tick'] <= tick < episode['end_tick']:
                    continue
                if last is not None and tick - last < RULE['sample_spacing_ticks']:
                    continue
                last = tick
                name, values, reason = state_vector(row)
                if reason:
                    rejected[reason] += 1
                    continue
                if name != context:
                    raise ValueError('descent feature context differs from outcome')
                ident = (context, approach.attempt_key(episode))
                if counts[ident] >= RULE['maximum_states_per_attempt']:
                    rejected['attempt_capacity'] += 1
                    continue
                counts[ident] += 1
                samples = cells.setdefault(context, [])
                if len(samples) >= RULE['maximum_states_per_cell']:
                    raise ValueError('declared descent state capacity exceeded')
                samples.append({**episode, 'tick': tick, 'state': values,
                    'remaining_descent_seconds': (episode['end_tick'] - tick) / costs.HZ,
                    'post_descent_seconds_bounds': [v - episode['phase_seconds'] for v in episode['landing_seconds_bounds']]})
    return {'episodes': exclusions, 'states': dict(rejected)}


def make_profile(runtime, cells, runs):
    return {'version': 1, 'model': PROFILE_MODEL, 'runtime_revision': runtime,
        'rule': copy.deepcopy(RULE), 'runs': runs, 'cells': {name: {
            'samples': samples, 'domain': {k: [min(s['state'][k] for s in samples),
                max(s['state'][k] for s in samples)] for k in SCALES}}
            for name, samples in cells.items()}}


class DescentLookup:
    def __init__(self, profile):
        if profile['version'] != 1 or profile['model'] != PROFILE_MODEL or profile['rule'] != RULE:
            raise ValueError('incompatible descent profile or rule')
        self.profile, self.index = profile, defaultdict(list)
        for name, cell in profile['cells'].items():
            if name not in ['initial', 'retry'] or not 0 < len(cell['samples']) <= RULE['maximum_states_per_cell']:
                raise ValueError('invalid descent cell or capacity')
            if set(cell['domain']) != set(SCALES):
                raise ValueError('incomplete descent domain')
            for s in cell['samples']:
                v, bounds = s['state'], s['post_descent_seconds_bounds']
                if (set(v) != set(SCALES) or any(not costs.finite(x) for x in v.values())
                        or len(bounds) != 2 or any(not costs.finite(x) or x < 0 for x in [s['remaining_descent_seconds'], *bounds])
                        or bounds[0] > bounds[1] or not 0 <= v['clearance'] < 26 or v['foot_gap'] < 0
                        or v['clearance'] + v['foot_gap'] >= 26
                        or not RULE['minimum_assist'] <= v['assist_strength'] <= 1):
                    raise ValueError('invalid descent training state or duration')
                self.index[name, math.floor(v['clearance'] / SCALES['clearance'])].append(s)
            for key, span in cell['domain'].items():
                if span != [min(s['state'][key] for s in cell['samples']), max(s['state'][key] for s in cell['samples'])]:
                    raise ValueError('descent domain differs from measured states')

    def predict(self, record):
        name, values, reason = state_vector(record)
        result = {'seconds': None, 'envelope_seconds': None, 'reason': reason, 'cell': name,
            'support': 0, 'support_seeds': 0, 'examined_states': 0, 'matched_states': 0,
            'descent_seconds': None, 'post_descent_flight_seconds': None, 'reference_seconds': None}
        if reason:
            return result
        cell = self.profile['cells'].get(name)
        if not cell:
            return {**result, 'reason': 'no_descent_clearance_calibration'}
        if any(not cell['domain'][k][0] <= v <= cell['domain'][k][1] for k, v in values.items()):
            return {**result, 'reason': 'descent_outside_training_domain'}
        bucket, nearest = math.floor(values['clearance'] / SCALES['clearance']), {}
        for b in [bucket - 1, bucket, bucket + 1]:
            for sample in self.index[name, b]:
                result['examined_states'] += 1
                differences = [abs(v - sample['state'][k]) / SCALES[k] for k, v in values.items()]
                if any(d > 1 for d in differences):
                    continue
                result['matched_states'] += 1
                rank, ident = (sum(d * d for d in differences), sample['tick']), approach.attempt_key(sample)
                if ident not in nearest or rank < nearest[ident][0]:
                    nearest[ident] = rank, sample
        samples = [v[1] for v in nearest.values()]
        worlds = defaultdict(list)
        for sample in samples:
            worlds[sample['seed']].append(sample)
        result.update(support=len(samples), support_seeds=len(worlds))
        if len(samples) < RULE['minimum_attempts'] or len(worlds) < RULE['minimum_worlds']:
            return {**result, 'reason': 'insufficient_local_descent_support'}
        reference = reference_seconds(values)
        residual = lambda s: s['remaining_descent_seconds'] - reference_seconds(s['state'])
        balanced = lambda fn: statistics.median(statistics.median(fn(s) for s in rows) for rows in worlds.values())
        descent = max(0.0, reference + balanced(residual))
        tail = balanced(lambda s: sum(s['post_descent_seconds_bounds']) / 2)
        envelope = [max(0.0, reference + min(map(residual, samples))) + min(s['post_descent_seconds_bounds'][0] for s in samples),
                    max(0.0, reference + max(map(residual, samples))) + max(s['post_descent_seconds_bounds'][1] for s in samples)]
        return {**result, 'seconds': descent + tail, 'envelope_seconds': envelope,
                'reference_seconds': reference, 'descent_seconds': descent, 'post_descent_flight_seconds': tail}


class DescentTrip(approach.StateTrip):
    def __init__(self, selection, ground, landing, walking, approach_lookup, descent_lookup):
        super().__init__(selection, ground, landing, walking, approach_lookup, flight_model='duration-then-state')
        self.descent_lookup, self.first_descent = descent_lookup, None

    def observe(self, row):
        result = super().observe(row)
        if result is None:
            return None
        result.update(model=MODEL, comparator=copy.deepcopy({k: result.get(k) for k in [
            'total_seconds', 'remaining_seconds', 'status', 'unknown_reasons', 'landing',
            'total_historical_envelope_seconds', 'remaining_historical_envelope_seconds', 'budget']}),
            descent_state=None, descent_lookup_attempted=False)
        if result.get('landing') is not None and result['flight_phase']['name'] == 'descent':
            state = diagnostic.snapshot(row, result['flight_phase'])
            landing = self.descent_lookup.predict(state)
            reasons = [r for r in result['unknown_reasons'] if r != result['landing']['reason']]
            if landing['reason']:
                reasons.append(landing['reason'])
            result.update(descent_state=state, descent_lookup_attempted=True,
                landing=landing, status='unknown', unknown_reasons=reasons,
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
        if self.first_descent is None and self.first_choice_tick is not None:
            self.first_descent = copy.deepcopy(result)
        return result

    def record(self):
        return {**super().record(), 'first_descent_prediction': self.first_descent}


def calibrate(manifest_path, out):
    manifest, cells, runs, exclusions, seen, sources = json.loads(manifest_path.read_text()), {}, [], [], set(), []
    for dataset in manifest['datasets']:
        index_path, evaluation_path = (manifest_path.resolve().parent / dataset[k] for k in ['index', 'evaluation'])
        index, evaluation = json.loads(index_path.read_text()), json.loads(evaluation_path.read_text())
        if (index['model'] != diagnostic.MODEL or index['runtime_revision'] != manifest['runtime_revision']
                or len(index['sources']) != 1 or index['sources'][0]['evaluation_sha256'] != costs.file_hash(evaluation_path)
                or [r['label'] for r in index['runs']] != [r['label'] for r in evaluation['runs']]):
            raise ValueError('unbound descent training index or evaluation')
        sources.append({'name': dataset['name'], 'index_sha256': costs.file_hash(index_path),
                        'evaluation_sha256': costs.file_hash(evaluation_path)})
        for meta, run in zip(index['runs'], evaluation['runs']):
            trace_path = index_path.parent / meta['path']
            if (costs.file_hash(trace_path) != meta['sha256'] or run['trace_sha256'] != meta['trace_sha256']
                    or run['report_sha256'] != meta['report_sha256'] or run['source_commit'] != manifest['runtime_revision']):
                raise ValueError('training recording or runtime differs')
            traces = json.loads(trace_path.read_text())
            identities = {(run['trace_sha256'], a['selection']['seat']) for a in run['attempts']}
            if seen & identities:
                raise ValueError('duplicate descent training recording and seat')
            seen |= identities
            source = {**frozen.source_summary(run), 'label': dataset['name'] + '/' + run['label'],
                      'features_sha256': meta['sha256']}
            exclusions.append({'label': source['label'], **add_training_run(cells, {**run, 'label': source['label']}, traces)})
            runs.append(source)
    profile = make_profile(manifest['runtime_revision'], cells, runs)
    profile.update(training_manifest_sha256=costs.file_hash(manifest_path), sources=sources, excluded=exclusions)
    DescentLookup(profile)
    frozen.write_json(out / 'profile.json', profile)
    print(json.dumps({name: len(c['samples']) for name, c in profile['cells'].items()}))


def evaluate_run(run, attempts, updates):
    updates = list(updates)
    result = approach.evaluate_run(run, attempts, updates)
    indexed = {(frozen.key(u), u['tick']): u for u in updates}
    for a in result['attempts']:
        for c in a['checkpoints']:
            u = indexed.get((frozen.key(a['selection']), c['tick']))
            eligible = u and c['status'] not in ['already_landed', 'attempt_ended', 'missing_observation']
            c['comparator'] = u['comparator'] if eligible else None
            c['descent_state'] = u['descent_state'] if eligible else None
            c['descent_lookup_attempted'] = u['descent_lookup_attempted'] if eligible else False
    return result


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
    ap, dp = json.loads(args.state_profile.read_text()), json.loads(args.descent_profile.read_text())
    al, dl = approach.StateLookup(ap), DescentLookup(dp)
    if any(p['runtime_revision'] != manifest['runtime_revision'] for p in [ap, dp]):
        raise ValueError('state profile runtime differs')
    provenance = {'version': 1, 'model': MODEL, 'scope': scope, 'runtime_revision': manifest['runtime_revision'],
        'manifest_sha256': costs.file_hash(args.manifest), 'profile_sha256': hashes,
        'state_profile_sha256': costs.file_hash(args.state_profile), 'descent_profile_sha256': costs.file_hash(args.descent_profile),
        'source_evaluation_sha256': costs.file_hash(args.source_evaluation)}
    base, forecasts = args.manifest.resolve().parent, []
    for i, config in enumerate(manifest['runs']):
        trace = base / config['directory'] / 'trace.jsonl'
        trace_hash = costs.file_hash(trace)
        path = args.out / f'updates-{i}.jsonl'
        factory = lambda selection: DescentTrip(selection, *profiles, al, dl)
        with path.open('w') as output:
            if scope == composed.controlled.SCOPE:
                attempts = composed.replay_controlled(config, base, factory, output, trace_hash)
            else:
                attempts = composed.rolling.replay(config, base, profiles[0], output,
                    trip_factory=lambda selection, _: factory(selection))
                if costs.file_hash(trace) != trace_hash:
                    raise ValueError('trace changed during replay')
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
        for p in [*profiles, ap, dp]:
            frozen.ensure_independent(p, run)
        trips = ([a['actual_trip'] for a in run['attempts']] if scope == composed.controlled.SCOPE else
                 [a['actual'] for a in run['attempts'] if a['actual']] + run['unobserved_attempts'])
        with (args.out / prediction['updates_file']).open() as stream:
            result = evaluate_run({**run, 'trips': trips}, prediction['attempts'], map(json.loads, stream))
        composed.verify_baseline(result, run, scope)
        if scope == composed.controlled.SCOPE:
            result['trial_outcome'] = run['outcome']
        results.append(result)
    frozen.write_json(args.out / 'evaluation.json', {**provenance, 'runs': results,
        'summary': composed.summarize(results), 'limitations': [
            'Conditional uninterrupted costs, not interruption or success probabilities.',
            'Strict descent support: no duration fallback. All other phases retain the expiry comparator.',
            'Post-descent flight includes any later contact/alignment episodes before acknowledgement.',
            'Empirical ranges are not confidence intervals or physical permissions.',
            'New-world validation requires a separately frozen generation protocol.']})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['calibrate', 'evaluate'])
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    for name in ['source-evaluation', 'ground-profile', 'phase-profile', 'walking-profile', 'state-profile', 'descent-profile']:
        parser.add_argument('--' + name, type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    if args.mode == 'calibrate':
        calibrate(args.manifest, args.out)
    else:
        if not all([args.source_evaluation, args.ground_profile, args.phase_profile, args.walking_profile, args.state_profile, args.descent_profile]):
            parser.error('evaluate requires source evaluation and all five profiles')
        evaluate(args)


if __name__ == '__main__':
    main()
