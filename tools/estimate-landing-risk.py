#!/usr/bin/env python3
"""Fit and compare small offline landing endpoint probability tables.

Separate scope/checkpoint tables prevent silently transporting the first-choice
population to +15 seconds or controlled setups to normal matches. Failed attempts
are a separate competing outcome. Administrative censoring is not a safe landing.
"""
import argparse
from collections import Counter, defaultdict
import copy
import importlib.util
import json
import math
from pathlib import Path
import statistics

SPEC = importlib.util.spec_from_file_location('probability_risk',
    Path(__file__).with_name('inspect-landing-risk.py'))
risk = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(risk)
MODEL = 'landing-risk-probability-v1'
OUTCOMES = ['interrupted', 'landed', 'horizon_clear', 'attempt_ended']
VARIANTS = ['phase', 'phase_recent']
PHASES = ['approach', 'circling', 'alignment', 'descent', 'settling']
RULE = {'checkpoints_seconds': [0, 15], 'horizon_seconds': 10, 'recent_seconds': 5,
    'minimum_classified_attempts': 12, 'minimum_classified_worlds': 4,
    'maximum_censored_fraction': 0.1, 'prior_world_weight': 1.0,
    'prior': {k: 0.25 for k in OUTCOMES}, 'outcomes': OUTCOMES,
    'aggregation': 'equal_world_mean_classified_distribution_plus_one_uniform_prior_world',
    'refinement': 'supported_phase_recent_cell_else_exact_supported_phase_cell',
    'scopes': ['normal', 'controlled'], 'physical_permissions': False}
IDENTITY = ['dataset', 'label', 'seed', 'seat', 'planet', 'selected_tick', 'checkpoint_seconds', 'tick']
OUTCOME_FIELDS = {'actual_ending', 'actual_reason', 'outcome'}


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False)


def key(record):
    return tuple(record[k] for k in IDENTITY)


def mean(values):
    return statistics.mean(values) if values else None


def cell_key(record, recent=None):
    parts = [record['scope'], str(record['checkpoint_seconds']), record['features']['phase']['name']]
    return '/'.join(parts + ([recent] if recent is not None else []))


def recency(feature):
    history = feature.get('recent', {})
    complete = history.get('complete')
    if type(complete) is not bool or history.get('window_seconds') != RULE['recent_seconds']:
        return 'invalid'
    if not complete:
        return 'unknown'
    counts = history.get('counts')
    if not isinstance(counts, dict) or any(type(v) is not int or v < 0 for v in counts.values()):
        return 'invalid'
    if counts.get('prelanding_replan', 0) > 0:
        return 'yes'
    return 'unknown' if counts.get('unclassified_replan', 0) else 'no'


def guard(record):
    f = record['features']
    if f['status'] != 'eligible':
        return f['status']
    if record['scope'] not in RULE['scopes'] or record['checkpoint_seconds'] not in RULE['checkpoints_seconds']:
        return 'scope_or_checkpoint_not_calibrated'
    if (type(record['tick']) is not int or record['tick'] < record['selected_tick']
            or f.get('phase', {}).get('name') not in PHASES):
        return 'invalid_clock_or_phase'
    if (not isinstance(f.get('site'), dict) or not isinstance(f.get('contact'), dict)
            or f['site'].get('planet') != record['planet']
            or risk.physical(f, record['planet']) != 'prelanding'
            or f.get('physical_permissions') is not False):
        return 'invalid_plan_or_contact'
    return None


def fit_cell(rows):
    worlds = defaultdict(list)
    for r in rows:
        worlds[r['seed']].append(r)
    distributions, per_world = [], []
    known, censored = 0, 0
    for seed, group in sorted(worlds.items()):
        counts = Counter(r['outcome']['status'] for r in group)
        n = sum(counts[k] for k in OUTCOMES)
        unknown = len(group) - n
        known += n
        censored += unknown
        if n:
            distributions.append({k: counts[k] / n for k in OUTCOMES})
        per_world.append({'seed': seed, 'attempts': len(group), 'classified': n,
                          'censored': unknown, 'outcomes': dict(counts)})
    reason = ('too_few_classified_attempts' if known < RULE['minimum_classified_attempts'] else
              'too_few_classified_worlds' if len(distributions) < RULE['minimum_classified_worlds'] else
              'too_much_censoring' if censored / len(rows) > RULE['maximum_censored_fraction'] else None)
    probabilities = None
    if reason is None:
        weight = RULE['prior_world_weight']
        probabilities = {k: (sum(d[k] for d in distributions) + weight * RULE['prior'][k]) /
                             (len(distributions) + weight) for k in OUTCOMES}
    return {'status': 'supported' if reason is None else 'unknown', 'reason': reason,
        'probabilities': probabilities, 'attempts': len(rows), 'classified': known,
        'censored': censored, 'classified_worlds': len(distributions), 'worlds': len(worlds),
        'outcomes': dict(Counter(r['outcome']['status'] for r in rows)), 'per_world': per_world,
        'empirical_censoring_bounds': {k: [mean([w['outcomes'].get(k, 0) / w['attempts'] for w in per_world]),
            mean([(w['outcomes'].get(k, 0) + w['censored']) / w['attempts'] for w in per_world])]
            for k in OUTCOMES}}


def fit(rows):
    groups = defaultdict(list)
    seen = set()
    for r in rows:
        identity = key(r)[:-1]
        if identity in seen:
            raise ValueError('duplicate training observation')
        seen.add(identity)
        if guard(r) is not None:
            continue
        if not isinstance(r.get('outcome'), dict):
            raise ValueError('eligible training observation has no endpoint')
        status = r['outcome']['status']
        if status not in OUTCOMES and not (status.startswith('censored_') or status == 'ambiguous_landing_replan'):
            raise ValueError('unknown endpoint schema')
        groups[cell_key(r)].append(r)
        recent = recency(r['features'])
        if recent == 'invalid':
            raise ValueError('malformed training history')
        if recent in ('yes', 'no'):
            groups[cell_key(r, recent)].append(r)
    return {k: fit_cell(v) for k, v in sorted(groups.items())}


def predict(profile, record, variant):
    """Pure lookup; labels, final attempts and other future fields are unused."""
    if variant not in VARIANTS or profile['model'] != MODEL or profile['rule'] != RULE:
        raise ValueError('incompatible probability profile or variant')
    result = {'status': 'unknown', 'requested': variant, 'reason': guard(record),
              'horizon_seconds': RULE['horizon_seconds'], 'physical_permissions': False}
    if result['reason'] is not None:
        return result
    parent_key = cell_key(record)
    parent = profile['cells'].get(parent_key)
    if parent is None or parent['status'] != 'supported':
        return {**result, 'reason': 'phase_support_unavailable', 'phase_cell': parent_key,
                'support_reason': parent['reason'] if parent else 'cell_absent'}
    selected, selected_key, used, reason = parent, parent_key, 'phase', 'phase_baseline'
    if variant == 'phase_recent':
        recent = recency(record['features'])
        if recent == 'invalid':
            return {**result, 'reason': 'invalid_recent_history'}
        child_key = cell_key(record, recent)
        child = profile['cells'].get(child_key)
        if recent == 'unknown':
            reason = 'recent_history_unknown_keep_phase'
        elif child is None or child['status'] != 'supported':
            reason = 'recent_support_unavailable_keep_phase'
        else:
            selected, selected_key, used, reason = child, child_key, 'phase_recent', 'supported_recent_refinement'
    return {**result, 'status': 'numeric', 'reason': reason, 'used': used, 'cell': selected_key,
        'probabilities': copy.deepcopy(selected['probabilities']),
        'support': {k: copy.deepcopy(selected[k]) for k in ('attempts', 'classified', 'censored',
                     'classified_worlds', 'worlds', 'outcomes', 'empirical_censoring_bounds')}}


def source_info(manifest_path):
    manifest, digest = risk.read_bound(manifest_path)
    if manifest['version'] != 1 or not manifest['datasets']:
        raise ValueError('invalid source manifest')
    runs, traces = {}, set()
    base = manifest_path.resolve().parent
    for d in manifest['datasets']:
        index, _ = risk.read_bound(base / d['index'], d['index_sha256'])
        config, _ = risk.read_bound(base / d['manifest'], d['manifest_sha256'])
        if (index['model'] != risk.tail.MODEL or index['runtime_revision'] != manifest['runtime_revision']
                or config['runtime_revision'] != manifest['runtime_revision']
                or not any(s['name'] == d['source_name'] and s['manifest_sha256'] == d['manifest_sha256']
                    and s['evaluation_sha256'] == d['evaluation_sha256'] for s in index['sources'])):
            raise ValueError('incompatible or unbound source index')
        entries = [r for r in index['runs'] if r['dataset'] == d['source_name']]
        if [r['label'] for r in entries] != [r['label'] for r in config['runs']]:
            raise ValueError('source run list differs')
        scope = 'controlled' if config.get('scope') == risk.tail.composed.controlled.SCOPE else 'normal'
        for r in entries:
            identity = (d['name'], r['label'])
            if identity in runs:
                raise ValueError('duplicate source run')
            runs[identity] = {**r, 'scope': scope}
            traces.add(r['trace_sha256'])
    return {'manifest_sha256': digest, 'runtime_revision': manifest['runtime_revision'],
            'worlds': sorted({r['seed'] for r in runs.values()}), 'traces': sorted(traces), 'runs': runs}


def bound_features(features_path, evidence_path, manifest_path):
    data, digest = risk.read_bound(features_path)
    evidence, evidence_hash = risk.read_bound(evidence_path)
    source = source_info(manifest_path)
    if (data['model'] != risk.MODEL or canonical(data['rule']) != canonical(risk.RULE)
            or evidence['model'] != risk.MODEL or evidence['features_sha256'] != digest
            or evidence['manifest_sha256'] != source['manifest_sha256']):
        raise ValueError('unbound or incompatible causal features')
    counts, identities = Counter(), set()
    for r in data['records']:
        if OUTCOME_FIELDS.intersection(r):
            raise ValueError('outcome fields in causal feature input')
        run_key = (r['dataset'], r['label'])
        run = source['runs'].get(run_key)
        if (run is None or run['seed'] != r['seed'] or run['scope'] != r['scope']
                or key(r)[:-1] in identities or r['checkpoint_seconds'] not in risk.CHECKPOINTS):
            raise ValueError('feature run identity differs or observation duplicated')
        identities.add(key(r)[:-1])
        counts[run_key] += 1
    if any(counts[k] != r['attempts'] * len(risk.CHECKPOINTS) for k, r in source['runs'].items()):
        raise ValueError('feature population differs from source attempts')
    source.update(features_sha256=digest, evidence_sha256=evidence_hash)
    return data, source


def bound_outcomes(path, features, feature_hash):
    data, digest = risk.read_bound(path)
    if data['model'] != risk.MODEL or data['features_sha256'] != feature_hash or len(data['records']) != len(features['records']):
        raise ValueError('unbound outcome data')
    for r, f in zip(data['records'], features['records']):
        if {k: v for k, v in r.items() if k not in OUTCOME_FIELDS} != f:
            raise ValueError('outcomes changed the causal observation')
    return data, digest


def calibrate(features_path, evidence_path, manifest_path, outcomes_path, out):
    features, source = bound_features(features_path, evidence_path, manifest_path)
    outcomes, digest = bound_outcomes(outcomes_path, features, source['features_sha256'])
    profile = {'version': 1, 'model': MODEL, 'rule': RULE, 'diagnostic_rule': features['rule'],
        'runtime_revision': source['runtime_revision'], 'training_worlds': source['worlds'],
        'training_traces': source['traces'], 'training_recordings': len(source['runs']),
        'source_manifest_sha256': source['manifest_sha256'], 'features_sha256': source['features_sha256'],
        'evidence_sha256': source['evidence_sha256'], 'outcomes_sha256': digest, 'cells': fit(outcomes['records'])}
    out.mkdir(parents=True, exist_ok=False)
    risk.tail.frozen.write_json(out / 'profile.json', profile)
    print(json.dumps({'cells': len(profile['cells']), 'supported': sum(c['status'] == 'supported' for c in profile['cells'].values()),
                      'worlds': len(source['worlds']), 'profile_sha256': risk.tail.costs.file_hash(out / 'profile.json')}))


def validate_holdout(profile, source):
    if (profile['version'] != 1 or profile['model'] != MODEL or profile['rule'] != RULE
            or profile['runtime_revision'] != source['runtime_revision']):
        raise ValueError('incompatible profile or runtime')
    if set(profile['training_worlds']).intersection(source['worlds']) or set(profile['training_traces']).intersection(source['traces']):
        raise ValueError('training/validation world or recording overlap')


def forecast(features_path, evidence_path, manifest_path, profile_path, out):
    features, source = bound_features(features_path, evidence_path, manifest_path)
    profile, profile_hash = risk.read_bound(profile_path)
    validate_holdout(profile, source)
    rows = [{**{k: r[k] for k in IDENTITY}, 'scope': r['scope'], 'asteroid_interval_seconds': r['asteroid_interval_seconds'],
             'predictions': {v: predict(profile, r, v) for v in VARIANTS}} for r in features['records']]
    if out.exists():
        raise ValueError('forecast already exists')
    risk.tail.frozen.write_json(out, {'model': MODEL, 'rule': RULE, 'profile_sha256': profile_hash,
        'features_sha256': source['features_sha256'], 'evidence_sha256': source['evidence_sha256'],
        'manifest_sha256': source['manifest_sha256'], 'runtime_revision': source['runtime_revision'],
        'worlds': source['worlds'], 'records': rows})
    print(json.dumps({'forecasts_sha256': risk.tail.costs.file_hash(out), 'rows': len(rows)}))


def score(probabilities, status):
    return {'brier': sum((probabilities[k] - int(k == status)) ** 2 for k in OUTCOMES),
        'interruption_brier': (probabilities['interrupted'] - int(status == 'interrupted')) ** 2,
        'log_loss': -math.log(probabilities[status])}


def auc(pairs):
    yes, no = ([p for p, y in pairs if y == target] for target in (True, False))
    return (sum((a > b) + 0.5 * (a == b) for a in yes for b in no) / (len(yes) * len(no))) if yes and no else None


def metrics(rows, variant):
    numeric = [r for r in rows if r['predictions'][variant]['status'] == 'numeric']
    known = [r for r in numeric if r['outcome']['status'] in OUTCOMES]
    scores = [score(r['predictions'][variant]['probabilities'], r['outcome']['status']) for r in known]
    bounds = []
    for r in numeric:
        p, s = r['predictions'][variant]['probabilities'], r['outcome']['status']
        values = [score(p, s)['brier']] if s in OUTCOMES else [score(p, k)['brier'] for k in OUTCOMES]
        bounds.append([min(values), max(values)])
    reliability = []
    for lo, hi in zip([0, .1, .25, .5, .75], [.1, .25, .5, .75, 1]):
        group = [r for r in known if lo <= r['predictions'][variant]['probabilities']['interrupted'] < hi or
                 hi == 1 and r['predictions'][variant]['probabilities']['interrupted'] == 1]
        reliability.append({'bin': [lo, hi], 'count': len(group),
            'mean_predicted': mean([r['predictions'][variant]['probabilities']['interrupted'] for r in group]),
            'observed_fraction': mean([int(r['outcome']['status'] == 'interrupted') for r in group])})
    return {'observations': len(rows), 'numeric': len(numeric), 'classified': len(known),
        'censored': len(numeric) - len(known), 'worlds': len({r['seed'] for r in known}),
        'outcomes': dict(Counter(r['outcome']['status'] for r in numeric)),
        'unknown_reasons': dict(Counter(r['predictions'][variant]['reason'] for r in rows
                                       if r['predictions'][variant]['status'] != 'numeric')),
        'mean_scores': {k: mean([s[k] for s in scores]) for k in ('brier', 'interruption_brier', 'log_loss')},
        'all_numeric_brier_bounds': [mean([b[i] for b in bounds]) for i in (0, 1)],
        'interruption_auc': auc([(r['predictions'][variant]['probabilities']['interrupted'],
                                 r['outcome']['status'] == 'interrupted') for r in known]),
        'reliability': reliability}


def comparison(rows):
    both = [r for r in rows if all(r['predictions'][v]['status'] == 'numeric' for v in VARIANTS)]
    known = [r for r in both if r['outcome']['status'] in OUTCOMES]
    differences = defaultdict(list)
    for r in known:
        a, b = (score(r['predictions'][v]['probabilities'], r['outcome']['status']) for v in VARIANTS)
        differences[r['seed']].append({k: b[k] - a[k] for k in a})
    return {'models': {v: metrics(rows, v) for v in VARIANTS}, 'paired_numeric': len(both),
        'paired_classified': len(known), 'refinements': sum(r['predictions']['phase_recent']['used'] == 'phase_recent' for r in both),
        'changed_probabilities': sum(r['predictions']['phase']['probabilities'] != r['predictions']['phase_recent']['probabilities'] for r in both),
        'paired_models': {v: metrics(both, v) for v in VARIANTS},
        'paired_equal_world_score_difference': {k: mean([mean([d[k] for d in values]) for values in differences.values()])
                                               for k in ('brier', 'interruption_brier', 'log_loss')},
        'per_world_score_difference': [{'seed': w, 'classified': len(values),
            **{k: mean([d[k] for d in values]) for k in ('brier', 'interruption_brier', 'log_loss')}}
            for w, values in sorted(differences.items())]}


def evaluate(features_path, outcomes_path, predictions_path, out):
    features, feature_hash = risk.read_bound(features_path)
    outcomes, outcome_hash = bound_outcomes(outcomes_path, features, feature_hash)
    predictions, prediction_hash = risk.read_bound(predictions_path)
    if predictions['model'] != MODEL or predictions['features_sha256'] != feature_hash or len(predictions['records']) != len(outcomes['records']):
        raise ValueError('unbound forecasts')
    groups, rows = defaultdict(list), []
    for p, r in zip(predictions['records'], outcomes['records']):
        if key(p) != key(r) or p['scope'] != r['scope'] or p['asteroid_interval_seconds'] != r['asteroid_interval_seconds']:
            raise ValueError('forecast/outcome identity differs')
        joined = {**p, 'features': r['features'], 'outcome': r['outcome'], 'actual_ending': r['actual_ending']}
        rows.append(joined)
        prefix = (r['scope'], r['checkpoint_seconds'])
        groups[(*prefix, 'all', 'all')].append(joined)
        groups[(*prefix, 'asteroids', str(r['asteroid_interval_seconds']))].append(joined)
        if r['features']['status'] == 'eligible':
            groups[(*prefix, 'phase', r['features']['phase']['name'])].append(joined)
            groups[(*prefix, 'recent', recency(r['features']))].append(joined)
    if out.exists():
        raise ValueError('evaluation already exists')
    risk.tail.frozen.write_json(out, {'model': MODEL, 'rule': RULE, 'features_sha256': feature_hash,
        'outcomes_sha256': outcome_hash, 'predictions_sha256': prediction_hash, 'profile_sha256': predictions['profile_sha256'],
        'worlds': predictions['worlds'], 'records': rows,
        'groups': [{'scope': k[0], 'checkpoint_seconds': k[1], 'split': k[2], 'value': k[3], **comparison(v)}
                   for k, v in sorted(groups.items())]})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['calibrate', 'forecast', 'evaluate'])
    parser.add_argument('--features', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    for name in ('evidence', 'manifest', 'outcomes', 'profile', 'predictions'):
        parser.add_argument('--' + name, type=Path)
    a = parser.parse_args()
    needed = {'calibrate': ['evidence', 'manifest', 'outcomes'], 'forecast': ['evidence', 'manifest', 'profile'],
              'evaluate': ['outcomes', 'predictions']}[a.mode]
    if any(getattr(a, n) is None for n in needed):
        parser.error('mode requires ' + ', '.join('--' + n for n in needed))
    if a.mode == 'calibrate':
        calibrate(a.features, a.evidence, a.manifest, a.outcomes, a.out)
    elif a.mode == 'forecast':
        forecast(a.features, a.evidence, a.manifest, a.profile, a.out)
    else:
        evaluate(a.features, a.outcomes, a.predictions, a.out)


if __name__ == '__main__':
    main()
