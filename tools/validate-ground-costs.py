#!/usr/bin/env python3
"""Stratify frozen trip estimates by ground distance, with execution diagnostics.

Consumes estimate-trip-costs.py evaluation output. Cohorts use only the original
prelanding choice; later routes, milestones and controls are evaluation evidence.
No model fitting, new forecasts, interpolation, or runtime decisions occur here.
"""
import argparse
import bisect
import hashlib
import importlib.util
import json
from pathlib import Path

SPEC = importlib.util.spec_from_file_location('frozen_trip_estimates',
    Path(__file__).with_name('estimate-trip-costs.py'))
frozen = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(frozen)
costs = frozen.costs

PHASES = ['exit', 'outbound', 'claim', 'return_board', 'departure']
ENDPOINTS = dict((name, (start, stop)) for name, start, stop in costs.PHASES)
DEFAULT_LONG_SECONDS = 4.0


def classify(choice, minimum):
    if not costs.finite(minimum) or minimum <= 0:
        raise ValueError('long-leg threshold must be finite and positive')
    refs = {name: choice['references'][name] if choice else None for name in ['outbound', 'return_board']}
    long = [name for name, value in refs.items() if costs.finite(value) and value >= minimum]
    category = choice['category'] if choice else None
    group = ('no_choice' if choice is None else 'no_flag' if category == 'no_flag' else
             f"{'long' if long else 'short'}/{category}" if category in ['walk', 'jump', 'crossing']
             else 'unmeasured_route')
    return {'group': group, 'category': category, 'long_legs': long, 'references_seconds': refs,
            'source_tick': choice['source_tick'] if choice else None,
            'choice_evidence_missing': choice['missing'] if choice else ['landing_choice_not_observed']}


def combined_prediction(prediction, names):
    if prediction is None:
        return {'estimate_seconds': None, 'historical_envelope_seconds': None,
                'unknown_reasons': ['landing_choice_not_observed']}
    phases = [prediction['phases'][name] for name in names]
    missing = [name for name, phase in zip(names, phases) if phase['estimate_seconds'] is None]
    return {'estimate_seconds': sum(p['estimate_seconds'] for p in phases) if not missing else None,
        'historical_envelope_seconds': [sum(p['historical_envelope_seconds'][i] for p in phases)
            for i in [0, 1]] if not missing else None,
        'unknown_reasons': [f'{name}:{reason}' for name in missing
            for reason in prediction['phases'][name]['unknown_reasons']]}


def comparison(prediction, actual, changes, source_tick, end_tick, scope_valid=True):
    point = prediction['estimate_seconds']
    bounds = actual['seconds_bounds']
    valid = actual.get('physics_valid', False) and scope_valid
    completed = actual['status'] == 'completed' and valid and bounds is not None
    envelope = prediction['historical_envelope_seconds']
    return {'estimate_seconds': point, 'historical_envelope_seconds': envelope, 'prediction_scope_valid': scope_valid,
        'unknown_reasons': prediction['unknown_reasons'], 'actual': actual,
        'evidence_changes': [c for c in changes if source_tick is not None and source_tick < c['tick'] <= end_tick],
        'error_seconds_bounds': [point - bounds[1], point - bounds[0]] if point is not None and completed else None,
        'censored_elapsed_exceeds_historical_max': bounds[0] > envelope[1]
            if actual['status'] == 'censored' and valid and bounds is not None and envelope is not None else None}


def assess(attempt, minimum):
    choice, prediction, trip = attempt['choice'], attempt['prediction'], attempt['recorded_attempt']
    result = {'selection': attempt['selection'], 'classification': classify(choice, minimum),
        'ending': trip['ending'] if trip else 'unmatched_attempt', 'reason': trip['reason'] if trip else None,
        'choice': choice, 'prediction': prediction, 'actual_trip': trip, 'phases': {}}
    if trip is None:
        return result
    changes = attempt['post_choice_changes']
    scope = attempt['actual']['prediction_scope_valid'] if attempt['actual'] else False
    start_tick = choice['source_tick'] if choice else None
    unavailable = {'status': 'not_observed', 'physics_valid': False, 'seconds_bounds': None}
    for phase in PHASES:
        actual = trip['phases'][phase]
        stop = trip['milestones'].get(ENDPOINTS[phase][1], [trip['stopped_tick']] * 2)[1]
        result['phases'][phase] = comparison(combined_prediction(prediction, [phase]), actual,
                                            changes, start_tick, stop, scope)
    for name, names in [('ground', ['outbound', 'claim', 'return_board']), ('landed_to_departed', PHASES)]:
        if name == 'ground':
            actual = trip['phases']['ground']
            stop = trip['milestones'].get('boarded', [trip['stopped_tick']] * 2)[1]
        else:
            landed = trip['milestones'].get('landed')
            departed = trip['milestones'].get('departed')
            stop = departed[1] if departed else trip['stopped_tick']
            end = departed or [stop, stop]
            actual = {'status': 'completed' if departed else 'censored',
                'physics_valid': trip['phases']['exit'].get('physics_valid', False),
                'seconds_bounds': [max(0, end[0] - landed[1]) / costs.HZ,
                                   (end[1] - landed[0]) / costs.HZ]} if landed else unavailable
        result['phases'][name] = comparison(combined_prediction(prediction, names), actual, changes, start_tick, stop, scope)
    return result


def phase_summary(records):
    return {'actual_statuses': dict(frozen.Counter(r['actual']['status'] for r in records)),
        'numeric_predictions': sum(r['estimate_seconds'] is not None for r in records),
        'unknown_reasons': dict(frozen.Counter(reason for r in records for reason in r['unknown_reasons'])),
        'completed_error': frozen.error_summary([r['error_seconds_bounds'] for r in records]),
        'unchanged_evidence_error': frozen.error_summary([r['error_seconds_bounds'] for r in records if not r['evidence_changes']]),
        'changed_evidence_error': frozen.error_summary([r['error_seconds_bounds'] for r in records if r['evidence_changes']]),
        'censored_above_historical_max': sum(r['censored_elapsed_exceeds_historical_max'] is True for r in records)}


def summarize(runs):
    attempts = [a for r in runs for a in r['attempts']]
    groups = {'all': attempts}
    for a in attempts:
        groups.setdefault(a['classification']['group'], []).append(a)
    return {name: {'attempts': len(group), 'endings': dict(frozen.Counter(a['ending'] for a in group)),
        'phases': {phase: phase_summary([a['phases'][phase] for a in group if phase in a['phases']])
                   for phase in PHASES + ['ground', 'landed_to_departed']}}
        for name, group in groups.items()}


class ExecutionWindow:
    """Post-hoc observations inside the certain interior of one ground phase."""
    def __init__(self, record, phase, start, stop):
        self.record, self.phase, self.start, self.stop = record, phase, start, stop
        self.stats = {'start_tick': start, 'stop_tick': stop, 'interval_ticks': stop - start,
            'sampled_ticks': 0, 'matching_on_foot_ticks': 0, 'balanced_ticks': 0, 'supported_ticks': 0,
            'unknown_support_ticks': 0, 'input_ticks': dict(full=0, partial=0, neutral=0, unknown=0),
            'primary_held_ticks': 0, 'goals': {}, 'first_route': None, 'counter_ranges': {},
            'scope': 'evaluation observations; first route is not a replacement prediction or renewed permission'}
        self.goals = frozen.Counter()

    def observe(self, row):
        s, p, c = self.stats, frozen.pilot(row), row['mission'].get('capture')
        s['sampled_ticks'] += 1
        planet = self.record['selection']['planet']
        if not (c and row['mission']['target'] == planet and p['planet']['index'] == planet and p['location'] == 'on_foot'):
            return
        s['matching_on_foot_ticks'] += 1
        s['balanced_ticks'] += p.get('balanced') is True
        s['supported_ticks'] += p.get('supported_planet') == planet
        s['unknown_support_ticks'] += 'supported_planet' not in p
        controls = row.get('controls', {})
        turn = controls.get('turn')
        bucket = 'unknown' if not costs.finite(turn) else 'full' if abs(turn) >= 0.999 else 'neutral' if turn == 0 else 'partial'
        s['input_ticks'][bucket] += 1
        # This is the emitted shared primary control. A 'jump' goal alone is
        # not evidence that a jump was commanded or that a physical jump occurred.
        s['primary_held_ticks'] += controls.get('thrust') is True
        g = c.get('ground')
        self.goals[g['goal'] if g else 'no_ground_task'] += 1
        s['goals'] = dict(self.goals)
        if not g:
            return
        for name in ['replans', 'invalidations', 'jumps', 'jetpack_crossings', 'flight_interruptions', 'claim_relocations']:
            if name in g:
                bounds = s['counter_ranges'].setdefault(name, [g[name], g[name]])
                bounds[0], bounds[1] = min(bounds[0], g[name]), max(bounds[1], g[name])
        expected = 'flag' if self.phase == 'outbound' else 'hatch' if self.phase == 'return_board' else None
        if s['first_route'] is None and expected is not None and g['destination'] == expected and g.get('route'):
            s['first_route'] = {'observed_tick': row['tick'], 'route': g['route'], 'revision': g.get('revision'),
                'nominal_seconds': costs.leg_seconds(g['route']), 'continuous_walk': g.get('continuous_walk', False)}


def add_execution(config, base, run, expected_hash):
    windows = [[], []]
    for a in run['attempts']:
        if not a['classification']['long_legs'] or a['actual_trip'] is None:
            continue
        t = a['actual_trip']
        a['execution'] = {}
        for phase in ['outbound', 'claim', 'return_board']:
            if t['phases'][phase]['status'] not in ['completed', 'censored']:
                continue
            begins, ends = ENDPOINTS[phase]
            start = t['milestones'][begins][1]
            stop = t['milestones'].get(ends, [t['stopped_tick']] * 2)[0]
            window = ExecutionWindow(a, phase, start, max(start, stop))
            a['execution'][phase] = window.stats
            windows[a['selection']['seat']].append(window)
    if not any(windows):
        return
    for seat in [0, 1]:
        windows[seat].sort(key=lambda w: w.start)
        if any(a.stop > b.start for a, b in zip(windows[seat], windows[seat][1:])):
            raise ValueError('overlapping execution scopes')
    starts = [[w.start for w in row] for row in windows]
    digest = hashlib.sha256()
    with (base / config['directory'] / 'trace.jsonl').open('rb') as stream:
        for raw in stream:
            digest.update(raw)
            row = json.loads(raw)
            seat, tick = row['seat'], row['tick']
            index = bisect.bisect_right(starts[seat], tick) - 1
            if index >= 0 and tick < windows[seat][index].stop:
                windows[seat][index].observe(row)
    if digest.hexdigest() != expected_hash:
        raise ValueError('execution trace differs from frozen evaluation source')
    for row in windows:
        for window in row:
            window.stats['missing_ticks'] = window.stats['interval_ticks'] - window.stats['sampled_ticks']
            window.stats['dense'] = window.stats['missing_ticks'] == 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--evaluation', required=True, type=Path)
    parser.add_argument('--manifest', type=Path, help='Optional original manifest for execution diagnostics')
    parser.add_argument('--min-leg-seconds', default=DEFAULT_LONG_SECONDS, type=float)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    classify(None, args.min_leg_seconds)  # Validate even an empty evaluation.
    evaluation = json.loads(args.evaluation.read_text())
    if evaluation['version'] != 1 or evaluation['model'] != frozen.MODEL:
        raise ValueError('expected a version-1 frozen trip evaluation')
    runs = [{**frozen.source_summary(r), 'attempts': [assess(a, args.min_leg_seconds) for a in r['attempts']],
             'unobserved_attempts': r['attempts_without_observation']} for r in evaluation['runs']]
    if args.manifest:
        manifest = frozen.load_manifest(args.manifest)
        if costs.file_hash(args.manifest) != evaluation['manifest_sha256']:
            raise ValueError('manifest differs from frozen evaluation input')
        configs = {c['label']: c for c in manifest['runs']}
        for run in runs:
            add_execution(configs[run['label']], args.manifest.resolve().parent, run, run['trace_sha256'])
            print(f"{run['label']}: ground execution inspected", flush=True)
    result = {'version': 1, 'model': 'frozen-ground-validation-v1',
        'source_evaluation_sha256': costs.file_hash(args.evaluation), 'profile_sha256': evaluation['profile_sha256'],
        'minimum_long_leg_seconds': args.min_leg_seconds, 'runs': runs, 'summary': summarize(runs),
        'unobserved_attempt_count': sum(len(r['unobserved_attempts']) for r in runs),
        'scope': 'cohorts fixed by prelanding references; later routes/controls are diagnostics only; no fitting or physical permission'}
    args.out.mkdir(parents=True, exist_ok=True)
    frozen.write_json(args.out / 'ground-validation.json', result)


if __name__ == '__main__':
    main()
