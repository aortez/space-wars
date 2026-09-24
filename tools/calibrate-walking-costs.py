#!/usr/bin/env python3
"""Fit conditional walking costs, then compare frozen forecasts on other worlds.

Only first-choice route references are predictors. Execution diagnostics qualify
training phases and stratify evaluation; they never repair a forecast. This is
an offline experiment, not a completion-probability model or bot policy.
"""
import argparse
from collections import Counter
import importlib.util
import json
import math
from pathlib import Path

SPEC = importlib.util.spec_from_file_location('controlled_ground',
    Path(__file__).with_name('evaluate-controlled-ground.py'))
controlled = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(controlled)
ground, frozen, costs = controlled.ground, controlled.frozen, controlled.costs
MODEL = 'controlled-walking-affine-v1'
LEGS = ['outbound', 'return_board']
VARIANTS = ['baseline', 'offset_only', 'affine']
MIN_SAMPLES, MIN_WORLDS, MIN_SPAN = 6, 3, 4.0


def linear_fit(samples, affine):
    """Nonnegative weighted least squares; each world has total weight one."""
    counts = Counter(s['seed'] for s in samples)
    points = [(s['reference_seconds'], s['actual_seconds'], 1 / counts[s['seed']]) for s in samples]
    if not points or any(not costs.finite(v) or v < 0 for x, y, _ in points for v in [x, y]):
        raise ValueError('finite nonnegative training measurements required')
    weight = sum(w for _, _, w in points)
    mx = sum(w * x for x, _, w in points) / weight
    my = sum(w * y for _, y, w in points) / weight
    if not affine:
        candidates = [(max(0, my - mx), 1.0)]
    else:
        xx = sum(w * (x - mx) ** 2 for x, _, w in points)
        if xx <= 0:
            raise ValueError('affine fit needs distinct references')
        slope = sum(w * (x - mx) * (y - my) for x, y, w in points) / xx
        intercept = my - slope * mx
        candidates = [(my, 0.0), (0.0, sum(w * x * y for x, y, w in points) / sum(w * x * x for x, _, w in points))]
        if intercept >= 0 and slope >= 0:
            candidates.append((intercept, slope))
    error = lambda pair: sum(w * (a - pair[0] - pair[1] * r) ** 2 for r, a, w in points)
    intercept, slope = min(candidates, key=error)
    residuals = [a - intercept - slope * r for r, a, _ in points]
    return {'overhead_seconds': intercept, 'reference_scale': slope,
        'training_residual_envelope_seconds': [min(residuals), max(residuals)],
        'world_weighted_squared_error': error((intercept, slope)) / weight}


def fit_cell(records):
    samples = [r for r in records if not r['excluded_reasons']]
    refs = [s['reference_seconds'] for s in samples]
    domain = [min(refs), max(refs)] if refs else None
    reasons = []
    if len(samples) < MIN_SAMPLES:
        reasons.append('insufficient_samples')
    if len({s['seed'] for s in samples}) < MIN_WORLDS:
        reasons.append('insufficient_worlds')
    if domain is None or domain[1] - domain[0] < MIN_SPAN:
        reasons.append('insufficient_distance_span')
    return {'samples': samples, 'excluded': [r for r in records if r['excluded_reasons']],
        'reference_seconds_domain': domain, 'unsupported_reasons': reasons,
        'fits': {name: linear_fit(samples, name == 'affine') for name in VARIANTS[1:]} if not reasons else {}}


def predict(choice, baseline, profile, variant):
    """Pure snapshot-only forecast; no actual routes, outcomes or future rows."""
    phases = {}
    for phase in LEGS:
        cell = profile['cells'][phase]
        reasons = list(choice['missing']) if choice else ['landing_choice_not_observed']
        if choice and choice['category'] != 'walk':
            reasons.append('requires_original_walk_choice')
        ref = choice['references'][phase] if choice else None
        if not cell['fits']:
            reasons.extend(cell['unsupported_reasons'])
        elif not costs.finite(ref) or not cell['reference_seconds_domain'][0] <= ref <= cell['reference_seconds_domain'][1]:
            reasons.append('reference_outside_training_domain')
        point = envelope = None
        if not reasons:
            fit = cell['fits'][variant]
            point = fit['overhead_seconds'] + fit['reference_scale'] * ref
            envelope = [max(0, point + x) for x in fit['training_residual_envelope_seconds']]
        phases[phase] = {'estimate_seconds': point, 'historical_envelope_seconds': envelope,
            'unknown_reasons': reasons}
    phases['claim'] = baseline['phases']['claim'] if baseline else {
        'estimate_seconds': None, 'historical_envelope_seconds': None, 'unknown_reasons': ['landing_choice_not_observed']}
    phases['ground'] = ground.combined_prediction({'phases': phases}, ['outbound', 'claim', 'return_board'])
    return phases


def execution_audit(path, attempt, trace_hash):
    stats = {p: {'route_modes': set(), 'bad_route_ticks': 0, 'posture_recovery_ticks': 0,
        'primary_ticks': 0, 'ticks': 0} for p in LEGS}
    for row in controlled.rows(path, trace_hash):
        for phase, audit in stats.items():
            window = attempt['execution'].get(phase)
            if not window or not window['start_tick'] <= row['tick'] < window['stop_tick']:
                continue
            audit['ticks'] += 1
            p = frozen.pilot(row)
            if p['planet']['index'] != 0 or p['location'] != 'on_foot':
                continue
            audit['primary_ticks'] += row['controls']['thrust'] is True
            posture = row.get('posture')
            if not posture:
                raise ValueError('walking calibration needs measured posture')
            audit['posture_recovery_ticks'] += posture['balance'] != 'Balanced'
            g = row['mission']['capture']['ground']
            expected = 'flag' if phase == 'outbound' else 'hatch'
            if g and g['destination'] == expected and g.get('route'):
                route = g['route']
                audit['route_modes'].add('powered' if route['flights'] else 'jump' if route['jumps'] else 'walk')
                audit['bad_route_ticks'] += bool(route['failure'] or route['partial'])
    for phase, audit in stats.items():
        audit['route_modes'] = sorted(audit['route_modes'])
        window = attempt['execution'].get(phase)
        audit['dense'] = bool(window and window['dense'] and audit['ticks'] == window['interval_ticks'])
    return stats


def measurement(run, attempt, phase, audit):
    choice = attempt['choice'] if attempt else None
    observed = attempt['phases'][phase] if attempt else None
    ref = choice['references'][phase] if choice else None
    reasons = list(choice['missing']) if choice else ['landing_choice_not_observed']
    if choice and choice['category'] != 'walk':
        reasons.append('requires_original_walk_choice')
    if not costs.finite(ref) or ref < 0:
        reasons.append('missing_reference')
    actual = observed['actual'] if observed else {}
    bounds = actual.get('seconds_bounds')
    if actual.get('status') != 'completed':
        reasons.append(actual.get('status', 'not_observed'))
    if not actual.get('physics_valid') or not observed or not observed['prediction_scope_valid']:
        reasons.append('invalid_scope_or_physics')
    if bounds is None or bounds[0] != bounds[1] or not costs.finite(bounds[0]):
        reasons.append('inexact_boundary')
    if observed and observed['evidence_changes']:
        reasons.append('changed_dependencies')
    if not audit or not audit['dense']:
        reasons.append('execution_not_dense')
    if audit:
        if audit['route_modes'] != ['walk'] or audit['bad_route_ticks']:
            reasons.append('execution_not_uninterrupted_walk')
        if audit['primary_ticks']:
            reasons.append('primary_input')
        if audit['posture_recovery_ticks']:
            reasons.append('posture_recovery')
    return {'label': run['label'], 'seed': run['seed'], 'phase': phase,
        'ending': run['outcome'], 'reference_seconds': ref,
        'actual_seconds': bounds[0] if bounds and bounds[0] == bounds[1] else None,
        'excluded_reasons': reasons, 'execution': audit}


def load_inputs(evaluation_path, manifest_path):
    evaluation = json.loads(evaluation_path.read_text())
    manifest = json.loads(manifest_path.read_text())
    if (evaluation['version'] != 1 or evaluation['model'] != 'controlled-ground-evaluation-v1'
            or evaluation['scope'] != controlled.SCOPE or manifest['scope'] != controlled.SCOPE
            or evaluation['manifest_sha256'] != costs.file_hash(manifest_path)
            or len(evaluation['runs']) != len(manifest['runs'])
            or len({r['label'] for r in evaluation['runs']}) != len(evaluation['runs'])):
        raise ValueError('matching controlled evaluation and complete manifest required')
    for run, config in zip(evaluation['runs'], manifest['runs']):
        if (run['label'] != config['label'] or run['seed'] != config['seed']
                or run['source_commit'] != manifest['runtime_revision'] or len(run['attempts']) > 1):
            raise ValueError('controlled run identity mismatch')
        path = manifest_path.resolve().parent / config['directory'] / 'report.json'
        if costs.file_hash(path) != run['report_sha256']:
            raise ValueError('controlled report changed')
        report = json.loads(path.read_text())
        if 'offset' in config and not math.isclose(report['offset'], config['offset'], rel_tol=0, abs_tol=1e-6):
            raise ValueError('initial placement differs from manifest')
    return evaluation, manifest


def observations(evaluation, manifest, base):
    records = []
    for run, config in zip(evaluation['runs'], manifest['runs']):
        attempt = run['attempts'][0] if run['attempts'] else None
        path = base / config['directory'] / 'trace.jsonl'
        if attempt:
            audit = execution_audit(path, attempt, run['trace_sha256'])
        else:
            if costs.file_hash(path) != run['trace_sha256']:
                raise ValueError('unavailable setup trace changed')
            audit = {}
        records.append({'label': run['label'], 'seed': run['seed'], 'outcome': run['outcome'],
            'legs': {phase: measurement(run, attempt, phase, audit.get(phase)) for phase in LEGS}})
    return records


def compare_forecasts(predictions, runs, records):
    comparisons = []
    for pred, run, record in zip(predictions, runs, records):
        assert pred['label'] == run['label'] == record['label']
        attempt = run['attempts'][0] if run['attempts'] else None
        phases = {}
        for phase in LEGS + ['ground']:
            observed = attempt['phases'][phase] if attempt else None
            actual = observed['actual'] if observed else {}
            bounds = actual.get('seconds_bounds')
            exact = actual.get('status') == 'completed' and actual.get('physics_valid') and observed['prediction_scope_valid'] and bounds and bounds[0] == bounds[1]
            clean = not record['legs'][phase]['excluded_reasons'] if phase in LEGS else all(not r['excluded_reasons'] for r in record['legs'].values())
            values = {name: pred[name][phase]['estimate_seconds'] for name in VARIANTS}
            phases[phase] = {'actual': actual, 'clean_walk': clean,
                'errors': {name: value - bounds[0] if exact and value is not None else None for name, value in values.items()}}
        comparisons.append({**record, 'phases': phases})
    summary = {}
    for phase in LEGS + ['ground']:
        rows = [r['phases'][phase] for r in comparisons]
        groups = {'all_completed': rows, 'clean_walk': [r for r in rows if r['clean_walk']],
            'paired_baseline_affine': [r for r in rows if all(r['errors'][m] is not None for m in ['baseline', 'affine'])],
            'paired_clean_walk': [r for r in rows if r['clean_walk'] and all(r['errors'][m] is not None for m in ['baseline', 'affine'])]}
        summary[phase] = {name: {m: frozen.error_summary([[r['errors'][m]] * 2 for r in rows if r['errors'][m] is not None]) for m in VARIANTS}
            for name, rows in groups.items()}
    return comparisons, summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['calibrate', 'evaluate'])
    parser.add_argument('--evaluation', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--profile', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    evaluation, manifest = load_inputs(args.evaluation, args.manifest)
    args.out.mkdir(parents=True, exist_ok=True)
    sources = [frozen.source_summary(r) for r in evaluation['runs']]
    if args.mode == 'calibrate':
        records = observations(evaluation, manifest, args.manifest.resolve().parent)
        profile = {'version': 1, 'model': MODEL, 'runtime_revision': manifest['runtime_revision'],
            'baseline_profile_sha256': evaluation['profile_sha256'], 'runs': sources,
            'source_evaluation_sha256': costs.file_hash(args.evaluation),
            'support': {'minimum_samples': MIN_SAMPLES, 'minimum_worlds': MIN_WORLDS, 'minimum_reference_span_seconds': MIN_SPAN},
            'weighting': 'each seed contributes total weight one within each phase',
            'cells': {phase: fit_cell([r['legs'][phase] for r in records]) for phase in LEGS},
            'limitations': ['Conditional completed walking only; every exclusion retained.',
                'No extrapolation, physical permission, route renewal or completion probability.',
                'Historical residual ranges are not confidence intervals.',
                'Native fixed-side exits limit outbound direction coverage.']}
        frozen.write_json(args.out / 'profile.json', profile)
        frozen.write_json(args.out / 'training.json', {'runs': records})
        print(json.dumps({p: {k: v for k, v in c.items() if k not in ['samples', 'excluded']} | {'samples': len(c['samples']), 'worlds': len({s['seed'] for s in c['samples']})} for p, c in profile['cells'].items()}, indent=2))
        return
    if args.profile is None:
        parser.error('evaluate needs a frozen --profile')
    profile = json.loads(args.profile.read_text())
    if (profile['version'] != 1 or profile['model'] != MODEL or profile['runtime_revision'] != manifest['runtime_revision']
            or profile['baseline_profile_sha256'] != evaluation['profile_sha256']):
        raise ValueError('incompatible walking profile, runtime or baseline')
    for source in sources:
        frozen.ensure_independent(profile, source)
    predictions = []
    for run in evaluation['runs']:
        a = run['attempts'][0] if run['attempts'] else None
        choice, baseline = (a['choice'], a['prediction']) if a else (None, None)
        predictions.append({'label': run['label'], 'choice': choice,
            'baseline': {p: ground.combined_prediction(baseline, [p] if p != 'ground' else ['outbound', 'claim', 'return_board']) for p in LEGS + ['ground']},
            **{m: predict(choice, baseline, profile, m) for m in VARIANTS[1:]}})
    frozen.write_json(args.out / 'predictions.json', {'model': MODEL, 'profile_sha256': costs.file_hash(args.profile), 'runs': predictions})
    records = observations(evaluation, manifest, args.manifest.resolve().parent)
    comparisons, summary = compare_forecasts(predictions, evaluation['runs'], records)
    frozen.write_json(args.out / 'evaluation.json', {'version': 1, 'model': MODEL,
        'source_evaluation_sha256': costs.file_hash(args.evaluation), 'manifest_sha256': costs.file_hash(args.manifest),
        'profile_sha256': costs.file_hash(args.profile), 'runs': comparisons, 'summary': summary,
        'outcomes': dict(Counter(r['outcome'] for r in records)),
        'scope': 'First-choice forecasts; actual execution only stratifies results. Affine is predeclared; no validation refit.'})
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
