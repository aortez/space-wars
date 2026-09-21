#!/usr/bin/env python3
"""Compose frozen flight and walking costs without changing the bot or fitting.

Replay predictions before reading an existing, hash-bound outcome evaluation.
Normal missions and controlled capture starts are separate evaluation scopes.
"""
import argparse
import copy
import importlib.util
import json
from pathlib import Path


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


phase = module('composed_landing', 'estimate-landing-phases.py')
walking = module('composed_walking', 'calibrate-walking-costs.py')
rolling, frozen, costs = phase.rolling, phase.frozen, phase.costs
controlled = walking.controlled
MODEL = 'composed-capture-trip-v1'
REGIME_MODEL = 'composed-capture-trip-regimes-v1'
WALK_MODELS = ['affine', 'short-and-affine']
NAMES = ['original', 'age_only', 'phase_only', 'combined']
TAIL = [name for name in frozen.PHASES if name != 'landing']


def ground_tail(anchor, profile, walk_model='affine'):
    """Use only the retained, causally acquired landing-choice snapshot."""
    if walk_model not in WALK_MODELS:
        raise ValueError('unknown walking model selection rule')
    tail = {name: copy.deepcopy(anchor['phases'][name]) for name in TAIL}
    if anchor['category'] == 'walk':
        fitted = walking.predict(anchor, anchor, profile, 'affine')
        for name in walking.LEGS:
            ref, cell = anchor['references'][name], profile['cells'][name]
            # The boundary is the frozen fit's minimum, not a threshold chosen
            # from validation. Each leg selects independently. The old forecast
            # already checks its own domain and snapshot validity; it cannot
            # fill a gap in that evidence or extend the affine upper bound.
            short = (walk_model == 'short-and-affine' and cell['fits'] and costs.finite(ref)
                     and 0 <= ref < cell['reference_seconds_domain'][0])
            if short:
                tail[name].update(model=frozen.MODEL, regime='short_empirical')
            else:
                tail[name] = {**fitted[name], 'reference_seconds': ref, 'model': walking.MODEL}
                if walk_model == 'short-and-affine':
                    tail[name]['regime'] = ('moderate_affine' if cell['fits'] and costs.finite(ref)
                        and cell['reference_seconds_domain'][0] <= ref <= cell['reference_seconds_domain'][1]
                        else 'unsupported')
    return tail


class ComposedTrip(phase.PhaseTrip):
    def __init__(self, selection, ground_profile, phase_profile, walking_profile, walk_model='affine'):
        super().__init__(selection, ground_profile, phase_profile)
        if walk_model not in WALK_MODELS:
            raise ValueError('unknown walking model selection rule')
        self.walk_model = walk_model
        self.model = MODEL if walk_model == 'affine' else REGIME_MODEL
        self.walking_profile = walking_profile
        self.tail_anchor = self.tail = self.first_composed = None

    def observe(self, row):
        result = super().observe(row)
        if result is None:
            return None
        # The superclass remains the unmodified phase-only comparator. Never
        # replace its anchor/original or feed our result back into its clocks.
        result.update(model=self.model, phase_only_total_seconds=result['total_seconds'],
            phase_only_status=result['status'], phase_only_unknown_reasons=list(result['unknown_reasons']))
        if self.anchor is not None:
            if self.tail_anchor is not self.anchor:
                self.tail = ground_tail(self.anchor, self.walking_profile, self.walk_model)
                self.tail_anchor = self.anchor
            result['ground_model'] = walking.MODEL if self.anchor['category'] == 'walk' else frozen.MODEL
            if self.walk_model == 'short-and-affine':
                if self.anchor['category'] == 'walk':
                    result['ground_model'] = REGIME_MODEL
                result['walking_legs'] = {name: {
                    'regime': self.tail[name].get('regime', 'category_empirical'),
                    'model': self.tail[name].get('model', frozen.MODEL),
                    'reference_seconds': self.anchor['references'][name],
                    'estimate_seconds': self.tail[name]['estimate_seconds'],
                    'unknown_reasons': self.tail[name]['unknown_reasons']}
                    for name in walking.LEGS}
            result['ground_reference_seconds'] = {n: self.anchor['references'][n] for n in walking.LEGS}
            result['unknown_tail_phases'] = {n: p['unknown_reasons'] for n, p in self.tail.items()
                                             if p['estimate_seconds'] is None}
            reasons = [r for r in result['unknown_reasons'] if r != 'unknown_surface_or_departure_cost']
            if result['unknown_tail_phases']:
                reasons.append('unknown_surface_or_departure_cost')
            result.update(status='unknown', unknown_reasons=reasons, remaining_seconds=None,
                total_seconds=None, remaining_historical_envelope_seconds=None,
                total_historical_envelope_seconds=None)
            if not reasons:
                remaining = result['landing']['seconds'] + sum(p['estimate_seconds'] for p in self.tail.values())
                envelope = [result['landing']['envelope_seconds'][i]
                    + sum(p['historical_envelope_seconds'][i] for p in self.tail.values()) for i in [0, 1]]
                result.update(status='estimate', remaining_seconds=remaining,
                    total_seconds=result['elapsed_seconds'] + remaining,
                    remaining_historical_envelope_seconds=envelope,
                    total_historical_envelope_seconds=[result['elapsed_seconds'] + x for x in envelope])
            until = result['budget']['seconds_until_time_limit']
            result['budget']['estimated_trip_exceeds_time_limit'] = (
                result['remaining_seconds'] > until if result['remaining_seconds'] is not None and until is not None else None)
        if self.first_composed is None and self.first_choice_tick is not None:
            self.first_composed = copy.deepcopy(result)
        return result

    def record(self):
        return {**super().record(), 'first_composed_prediction': self.first_composed}


def validate_profiles(manifest, profiles, ground_hash):
    for p, model in zip(profiles, [frozen.MODEL, phase.MODEL, walking.MODEL]):
        if p['version'] != 1 or p['model'] != model or p['runtime_revision'] != manifest['runtime_revision']:
            raise ValueError('incompatible frozen model or runtime')
    _, landing, walk = profiles
    if (landing['ground_profile_sha256'] != ground_hash or landing['min_attempts'] != phase.MIN_ATTEMPTS
            or walk['baseline_profile_sha256'] != ground_hash
            or walk['support'] != {'minimum_samples': walking.MIN_SAMPLES, 'minimum_worlds': walking.MIN_WORLDS,
                                   'minimum_reference_span_seconds': walking.MIN_SPAN}):
        raise ValueError('incompatible frozen profile dependency or support rule')


def replay_controlled(config, base, factory, output, trace_hash):
    state, previous, start_seen, signature = None, -1, None, None
    for row in controlled.rows(base / config['directory'] / 'trace.jsonl', trace_hash):
        tick, p = row['tick'], frozen.pilot(row)
        if row['seat'] != config['seat'] or p['tick'] != tick or p['owner'] != f"player_{config['seat'] + 1}" or tick <= previous:
            raise ValueError('invalid controlled trace identity/order')
        previous = tick
        events = row['mission']['events']
        start = events[0]['tick'] if events else None
        if start_seen is not None and start != start_seen:
            raise ValueError('capture start changed within a controlled trial')
        if start is None:
            continue
        start_seen = start
        if state is None:
            state = factory(frozen.snapshot(row, start, 'mission_selection'))
        if p['planet']['index'] != 0:
            # Leaving the controlled frame ends this attempt, even if a later
            # row returns to it. It cannot become a normal interplanetary trip.
            row = {**row, 'mission': {**row['mission'], 'target': None}}
        result = state.observe(row)
        if result is None:
            continue
        current = (result['status'], result.get('plan_generation'), result['unknown_reasons'],
                   result.get('flight_phase', {}).get('episode'))
        if (state.first_choice_tick is not None and (tick - state.first_choice_tick) % costs.HZ == 0
                or result['invalidated_by'] or current != signature):
            output.write(json.dumps(result, allow_nan=False) + '\n')
        signature = current
    return [state.record()] if state else []


def evaluate_run(run, attempts, updates):
    rows = list(updates)
    indexed = {(frozen.key(r), r['tick']): r for r in rows}
    result = phase.evaluate_run(run, attempts, rows)
    for a in result['attempts']:
        for c in a['checkpoints']:
            c['combined_total_seconds'] = c.pop('phase_total_seconds')
            c['combined_error_seconds_bounds'] = c.pop('phase_error_seconds_bounds')
            u = indexed.get((frozen.key(a['selection']), c['tick']))
            eligible = c['status'] not in ['attempt_ended', 'already_landed', 'missing_observation']
            old = u['phase_only_total_seconds'] if u and eligible else None
            actual = a['actual']['phases']['total']['seconds_bounds']
            c.update(phase_only_total_seconds=old,
                phase_only_error_seconds_bounds=[old - actual[1], old - actual[0]]
                    if old is not None and a['actual']['ending'] == 'completed' else None,
                phase_only_status=u['phase_only_status'] if u and eligible else c['status'],
                phase_only_unknown_reasons=u['phase_only_unknown_reasons'] if u and eligible else [],
                category=u.get('category') if u and eligible else None,
                ground_model=u.get('ground_model') if u and eligible else None,
                unknown_tail_phases=u.get('unknown_tail_phases', {}) if u and eligible else {},
                ground_reference_seconds=u.get('ground_reference_seconds') if u and eligible else None)
            if u and u['model'] == REGIME_MODEL:
                c['walking_legs'] = u.get('walking_legs', {}) if eligible else {}
    return result


def comparisons(rows):
    pair = [r for r in rows if all(r[n + '_error_seconds_bounds'] is not None for n in ['phase_only', 'combined'])]
    common = [r for r in rows if all(r[n + '_error_seconds_bounds'] is not None for n in NAMES)]
    numeric = lambda r, n: r[n + '_total_seconds'] is not None
    return {'checkpoints': len(rows), 'statuses': dict(frozen.Counter(r['status'] for r in rows)),
        'unknown_reasons': dict(frozen.Counter(x for r in rows for x in r['unknown_reasons'])),
        'unknown_tail_reasons': dict(frozen.Counter(f'{p}/{x}' for r in rows
            for p, reasons in r['unknown_tail_phases'].items() for x in reasons)),
        'numeric': {n: sum(numeric(r, n) for r in rows) for n in NAMES},
        'coverage_gained': sum(numeric(r, 'combined') and not numeric(r, 'phase_only') for r in rows),
        'coverage_lost': sum(numeric(r, 'phase_only') and not numeric(r, 'combined') for r in rows),
        'all_completed_error': {n: frozen.error_summary([r[n + '_error_seconds_bounds'] for r in rows]) for n in NAMES},
        'paired_completed': len(pair),
        'paired_error': {n: frozen.error_summary([r[n + '_error_seconds_bounds'] for r in pair]) for n in ['phase_only', 'combined']},
        'four_way_completed': len(common),
        'four_way_error': {n: frozen.error_summary([r[n + '_error_seconds_bounds'] for r in common]) for n in NAMES}}


def summarize(runs):
    attempts = [a for r in runs for a in r['attempts']]
    actual = [a['actual'] for a in attempts if a['actual']] + [a for r in runs for a in r['unobserved_attempts']]
    checkpoints = [c for a in attempts for c in a['checkpoints']]
    return {'runs': len(runs), 'observed_attempts': len(attempts), 'recorded_attempts': len(actual),
        'attempts_without_choice': sum(a['original_prediction'] is None for a in attempts),
        'unobserved_attempts': sum(len(r['unobserved_attempts']) for r in runs),
        'endings': dict(frozen.Counter(a['ending'] for a in actual)),
        'trial_outcomes': dict(frozen.Counter(r['trial_outcome'] for r in runs if 'trial_outcome' in r)),
        'checkpoints': {str(seconds): comparisons(rows := [c for c in checkpoints if c['after_seconds'] == seconds])
            | {'by_category': {category: comparisons([c for c in rows if c['category'] == category])
                              for category in ['no_flag', 'walk', 'jump', 'crossing']}}
            for seconds in rolling.CHECKPOINT_SECONDS}}


def verify_source(source, manifest, manifest_hash, profiles_hashes):
    scope = manifest.get('scope', 'missions')
    if (source['version'] != 1 or source['manifest_sha256'] != manifest_hash
            or [r['label'] for r in source['runs']] != [c['label'] for c in manifest['runs']]):
        raise ValueError('matching complete source evaluation required')
    if scope == controlled.SCOPE:
        valid = (source['model'] == 'controlled-ground-evaluation-v1' and source['scope'] == scope
                 and source['profile_sha256'] == profiles_hashes['ground'])
    else:
        valid = (source['model'] == phase.MODEL and source['ground_profile_sha256'] == profiles_hashes['ground']
                 and source['phase_profile_sha256'] == profiles_hashes['phase'])
    if not valid:
        raise ValueError('incompatible outcome source or profile dependency')


def verify_baseline(result, source, scope):
    original = {frozen.key(a['selection']): a for a in source['attempts']}
    if set(original) != {frozen.key(a['selection']) for a in result['attempts']}:
        raise ValueError('replayed attempts differ from source evaluation')
    for a in result['attempts']:
        b = original[frozen.key(a['selection'])]
        if a['original_prediction'] != b['prediction' if scope == controlled.SCOPE else 'original_prediction']:
            raise ValueError('original forecast changed')
        if scope == controlled.SCOPE:
            continue
        if len(a['checkpoints']) != len(b['checkpoints']):
            raise ValueError('baseline checkpoint coverage changed')
        for c, d in zip(a['checkpoints'], b['checkpoints']):
            for new, old in [('phase_only_total_seconds', 'phase_total_seconds'),
                             ('phase_only_error_seconds_bounds', 'phase_error_seconds_bounds'),
                             ('phase_only_status', 'status'), ('phase_only_unknown_reasons', 'unknown_reasons'),
                             ('age_only_total_seconds', 'age_only_total_seconds'), ('flight_phase', 'flight_phase')]:
                if c[new] != d[old]:
                    raise ValueError(f'phase-only comparator changed: {new}')


def evaluation_identities(run, config, prediction, scope):
    if scope == controlled.SCOPE and not run['attempts'] and not prediction['attempts']:
        # Different requested bands can all fail preparation, leaving identical
        # traces before any capture attempt exists. Retain these distinct trials
        # without admitting repeated copies of the same setup configuration.
        setup = json.dumps({k: config[k] for k in ['seed', 'seat', 'band', 'offset'] if k in config}, sort_keys=True)
        return {(run['trace_sha256'], config['seat'], setup)}
    return {(run['trace_sha256'], s) for s in config.get('seats', [config.get('seat')])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['manifest', 'source-evaluation', 'ground-profile', 'phase-profile', 'walking-profile', 'out']:
        parser.add_argument('--' + name, required=True, type=Path)
    parser.add_argument('--walk-model', choices=WALK_MODELS, default='affine')
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    scope = manifest.get('scope', 'missions')
    if scope == 'missions':
        frozen.load_manifest(args.manifest)
    elif (scope != controlled.SCOPE or manifest['version'] != 1 or not manifest['runs']
            or len({c['label'] for c in manifest['runs']}) != len(manifest['runs'])
            or any(c['seat'] not in [0, 1] for c in manifest['runs'])):
        raise ValueError('invalid controlled manifest')
    paths = {'ground': args.ground_profile, 'phase': args.phase_profile, 'walking': args.walking_profile}
    hashes = {name: costs.file_hash(path) for name, path in paths.items()}
    profiles = [json.loads(path.read_text()) for path in paths.values()]
    validate_profiles(manifest, profiles, hashes['ground'])
    provenance = {'version': 1, 'model': MODEL if args.walk_model == 'affine' else REGIME_MODEL, 'scope': scope,
        'runtime_revision': manifest['runtime_revision'], 'profile_sha256': hashes,
        'manifest_sha256': costs.file_hash(args.manifest),
        'source_evaluation_sha256': costs.file_hash(args.source_evaluation)}
    base = args.manifest.resolve().parent
    args.out.mkdir(parents=True, exist_ok=True)
    forecasts = []
    for i, config in enumerate(manifest['runs']):
        trace = base / config['directory'] / 'trace.jsonl'
        trace_hash = costs.file_hash(trace)
        updates = args.out / f'updates-{i}.jsonl'
        factory = lambda selection: ComposedTrip(selection, *profiles, walk_model=args.walk_model)
        with updates.open('w') as stream:
            if scope == controlled.SCOPE:
                attempts = replay_controlled(config, base, factory, stream, trace_hash)
            else:
                attempts = rolling.replay(config, base, profiles[0], stream,
                    trip_factory=lambda selection, _: factory(selection))
                if costs.file_hash(trace) != trace_hash:
                    raise ValueError('trace changed during prediction replay')
        forecasts.append({'label': config['label'], 'attempts': attempts, 'trace_sha256': trace_hash,
                          'updates_file': updates.name, 'updates_sha256': costs.file_hash(updates)})
        print(f"{config['label']}: {len(attempts)} composed attempts", flush=True)
    frozen.write_json(args.out / 'predictions.json', {**provenance, 'runs': forecasts})
    # Only now open the outcome evaluation. Its trace/report hashes bind every
    # reused measurement to the replay; its model and manifest bind the scope.
    source = json.loads(args.source_evaluation.read_text())
    if costs.file_hash(args.source_evaluation) != provenance['source_evaluation_sha256']:
        raise ValueError('source evaluation changed during replay')
    verify_source(source, manifest, provenance['manifest_sha256'], hashes)
    evaluated, seen = [], set()
    for config, prediction, run in zip(manifest['runs'], forecasts, source['runs']):
        if (run['source_commit'] != manifest['runtime_revision'] or run['trace_sha256'] != prediction['trace_sha256']
                or run['report_sha256'] != costs.file_hash(base / config['directory'] / 'report.json')):
            raise ValueError('source runtime, trace or report changed')
        identities = evaluation_identities(run, config, prediction, scope)
        if seen & identities:
            raise ValueError('duplicate evaluation recording and seat')
        seen |= identities
        for profile in profiles:
            frozen.ensure_independent(profile, run)
        trips = ([a['actual_trip'] for a in run['attempts']] if scope == controlled.SCOPE else
                 [a['actual'] for a in run['attempts'] if a['actual']] + run['unobserved_attempts'])
        with (args.out / prediction['updates_file']).open() as stream:
            result = evaluate_run({**run, 'trips': trips}, prediction['attempts'], map(json.loads, stream))
        verify_baseline(result, run, scope)
        if scope == controlled.SCOPE:
            result['trial_outcome'] = run['outcome']
        evaluated.append(result)
    frozen.write_json(args.out / 'evaluation.json', {**provenance, 'runs': evaluated,
        'summary': summarize(evaluated), 'limitations': [
            'Replay alone does not establish independent validation; a separate data-generation protocol is required. No changed bot behavior or refitting.',
            'Conditional completion times, not probabilities, permissions or guaranteed bounds.',
            'First-choice walking calibration applied to currently acquired route references.',
            ('Non-walking categories keep the frozen empirical tail; walking never falls back outside its domain.'
                if args.walk_model == 'affine' else
                'Each leg uses supported empirical costs below the affine minimum, affine costs within its domain, otherwise unknown. Non-walking categories are unchanged.'),
            'Controlled totals start at capture-controller start; preparation and remote transfer are excluded.']})


if __name__ == '__main__':
    main()
