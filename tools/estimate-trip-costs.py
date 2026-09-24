#!/usr/bin/env python3
"""Freeze capture-trip estimates from trace prefixes; evaluate on separate worlds.

This is an offline diagnostic, not a controller or a physical permission check.
The model combines the existing ground-score reference with empirical phase
residuals. Historical envelopes are neither confidence intervals nor deadlines.
"""
import argparse
import copy
import importlib.util
import json
import statistics
import sys
from collections import Counter
from pathlib import Path

SPEC = importlib.util.spec_from_file_location('recorded_trip_costs',
    Path(__file__).with_name('analyze-trip-costs.py'))
costs = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = costs
SPEC.loader.exec_module(costs)

MODEL = 'capture-trip-empirical-v1'
POLICY = 'material_mission_v11'
PHASES = ['landing', 'exit', 'outbound', 'claim', 'return_board', 'departure']
MAX_BOUNDARY_WIDTH = 1.0
MAX_SURVEY_AGE_TICKS = 120  # Native live_planning.rs; this does not renew validity.


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + '\n')


def pilot(row):
    return row['observation']['local']['combat']['recovery']['flight']['pilot']


def key(value):
    return value['seat'], value['planet'], value['selected_tick']


def claim_identity(p):
    claim = p['planet']['claim']
    if not claim:
        return None
    flag = claim.get('flag')
    # Retain the measured world/frame pose; do not compare a rotating flag's
    # world coordinates on later ticks as though it had moved on the ground.
    return {'owner': claim['owner'], 'claimant': claim['claimant'],
            'phase': claim['phase'], 'progress': claim['progress'],
            'flag': flag, 'planet_motion': p['planet'].get('motion')}


def snapshot(row, selected, stage):
    """Only the current observation and past telemetry enter this function."""
    p, mission, tick = pilot(row), row['mission'], row['tick']
    target = mission['target']
    c = mission.get('capture')
    local = target == p['planet']['index']
    choice = stage == 'local_choice'
    reference = costs.selected_estimate(row) if choice and local else None
    route = c.get('objective_route') if c and choice else None
    missing = []
    category = None
    if not choice:
        missing.append('landing_choice_not_observed')
    if not local:
        missing.append('remote_ground_unmeasured')
    if not p['ship_available'] or p['ship_form'] != 'ship' or p['location'] == 'on_foot':
        missing.append('requires_aboard_full_ship')
    if choice and not p['queries_ready']:
        missing.append('local_queries_unavailable')
    if reference:
        claim = p['planet']['claim']
        if claim and claim['owner'] == p['owner']:
            missing.append('destination_already_owned')
        elif reference['status'] == 'no_existing_flag_term':
            category = 'no_flag'
        elif reference['status'] == 'selected_route':
            legs = [route['outbound'], route['returning']]
            category = ('crossing' if any(r['flights'] for r in legs) else
                        'jump' if any(r['jumps'] for r in legs) else 'walk')
            if reference['source_tick'] is None:
                missing.append('route_source_unverified_at_choice')
            elif reference['source_tick'] > tick or (reference['validated_tick'] or 0) > tick:
                missing.append('future_route_evidence')
            elif reference['revision'] != p['planet']['revision']:
                missing.append('changed_material')
            elif reference['source_tick'] != tick and (
                    reference['validated_tick'] != tick or tick - reference['source_tick'] > MAX_SURVEY_AGE_TICKS):
                missing.append('route_not_current_at_choice')
            survey = row['observation']['local'].get('landing_objective')
            if survey and (survey.get('actor') != p['owner'] or
                           survey['objective'].get('planet') != target):
                missing.append('route_actor_or_planet_mismatch')
            if reference['crossing_source_tick'] is not None and reference['crossing_source_tick'] > tick:
                missing.append('future_crossing_evidence')
        else:
            missing.append('incomplete_round_trip')
    elif choice:
        missing.append('landing_reference_unavailable')
    if c and choice and (not c['site'] or c['site']['planet'] != target):
        missing.append('site_planet_mismatch')
    refs = {phase: None for phase in PHASES}
    if reference:
        refs.update(outbound=reference['outbound_seconds'],
                    return_board=reference['return_seconds'],
                    claim=reference['full_uninterrupted_claim_seconds'])
    arrivals = [e['tick'] for e in mission['events'] if e['kind'] == 'arrived'
                and e['planet'] == target and selected <= e['tick'] <= tick]
    arrived = min(arrivals) if arrivals else None
    sampled = costs.sample(row, target)
    source = reference['source_tick'] if reference else None
    return {'seat': row['seat'], 'policy': mission['policy'], 'planet': target,
            'selected_tick': selected, 'source_tick': tick, 'stage': stage,
            'elapsed_seconds': (tick - selected) / costs.HZ,
            'elapsed_transfer_seconds': (arrived - selected) / costs.HZ if arrived is not None else None,
            'elapsed_local_approach_seconds': (tick - arrived) / costs.HZ if arrived is not None else None,
            'category': category, 'references': refs, 'missing': missing,
            'dependencies': {'site': c['site'] if c and choice else None,
                'observed_revision': p['planet']['revision'] if local else None,
                'observation_tick': tick, 'route_source_tick': source,
                'route_age_ticks': tick - source if source is not None and source <= tick else None,
                'route_validated_tick': reference['validated_tick'] if reference else None,
                'route_revision': reference['revision'] if reference else None,
                'crossing_source_tick': reference['crossing_source_tick'] if reference else None,
                'objective': claim_identity(p) if local else None,
                'planet_radius': p['planet'].get('radius') if local else None,
                'gravity': p.get('gravity'), 'ship_form': p['ship_form'],
                'renewed_physical_validity': False},
            'exposure': {'ship_geometry_now': sampled.threat,
                         'future_ship_exposure': 'unknown', 'pilot_visibility': 'unmeasured'},
            'diagnostic_only': True}


def read_snapshots(config, base):
    """Stream the trace once, without opening its report or inspecting future rows.

    Selection is anchored to a past mission event. Choice is the first sampled
    selected site while still aboard and before touchdown, even if evidence is
    missing. A later, better route never repairs this frozen prediction.
    """
    attempts, previous, observed_dependencies = {}, [-1, -1], {}
    path = base / config['directory'] / 'trace.jsonl'
    with path.open() as stream:
        for row in map(json.loads, stream):
            seat, tick, p = row['seat'], row['tick'], pilot(row)
            if (row['version'] != 1 or seat not in [0, 1] or tick <= previous[seat]
                    or p['tick'] != tick or p['owner'] != f'player_{seat + 1}'):
                raise ValueError('unsupported or nonmonotonic trace identity')
            previous[seat] = tick
            if seat not in config['seats']:
                continue
            mission = row['mission']
            if mission['policy'] != POLICY:
                raise ValueError('only the v11 reference policy is calibrated')
            if any(e['tick'] > tick for e in mission['events']):
                raise ValueError('telemetry contains a future event')
            selections = [e for e in mission['events'] if e['kind'] == 'selected']
            if not selections or mission['target'] != selections[-1]['planet']:
                continue
            selected = selections[-1]['tick']
            ident = (seat, mission['target'], selected)
            if ident not in attempts:
                attempts[ident] = {'selection': snapshot(row, selected, 'mission_selection'),
                                   'choice': None, 'post_choice_changes': []}
            attempt = attempts[ident]
            c = mission.get('capture')
            if (attempt['choice'] is None and c and c['site'] is not None
                    and p['location'] != 'on_foot' and p['ship_available']
                    and p['ship_form'] == 'ship' and c['landing']['landed_tick'] is None):
                attempt['choice'] = snapshot(row, selected, 'local_choice')
                observed_dependencies[ident] = {}
            if attempt['choice'] and c:
                values = {'ship_form': p['ship_form']}
                if p['planet']['index'] == mission['target']:
                    values['revision'] = p['planet']['revision']
                if c['landing']['landed_tick'] is None:
                    values.update(site=c['site'], route=c['objective_route'])
                retained = observed_dependencies[ident]
                for name, value in values.items():
                    if name in retained and retained[name] != value:
                        attempt['post_choice_changes'].append({'tick': tick, 'dependency': name})
                    retained[name] = value
    return list(attempts.values())


def estimate(snap, profile):
    """Pure, cheap phase composition; no report, world query, or rollout."""
    phases = {}
    category = snap['category']
    for phase in PHASES:
        ref = snap['references'][phase]
        cell = profile['cells'].get(f'{category}/{phase}')
        reasons = list(snap['missing'])
        if not cell or not cell['samples']:
            reasons.append('no_calibration_samples')
        elif cell['mode'] == 'residual':
            domain = cell['reference_seconds_domain']
            if ref is None or not costs.finite(ref) or not domain[0] <= ref <= domain[1]:
                reasons.append('reference_outside_calibration_domain')
        value = envelope = None
        if not reasons:
            offset = ref if cell['mode'] == 'residual' else 0.0
            value = max(0.0, offset + cell['median_midpoint_seconds'])
            envelope = [max(0.0, offset + x) for x in cell['observed_seconds_envelope']]
        phases[phase] = {'reference_seconds': ref, 'estimate_seconds': value,
                         'historical_envelope_seconds': envelope,
                         'basis': cell['mode'] if cell else None,
                         'calibration_cell': f'{category}/{phase}',
                         'calibration_samples': len(cell['samples']) if cell else 0,
                         'calibration_seeds': len({s['seed'] for s in cell['samples']}) if cell else 0,
                         'unknown_reasons': reasons}
    complete = all(p['estimate_seconds'] is not None for p in phases.values())
    remaining = sum(p['estimate_seconds'] for p in phases.values()) if complete else None
    envelope = ([sum(p['historical_envelope_seconds'][i] for p in phases.values())
                 for i in [0, 1]] if complete else None)
    return {**copy.deepcopy(snap), 'phases': phases,
            'remaining_seconds': remaining, 'remaining_historical_envelope_seconds': envelope,
            'total_seconds': snap['elapsed_seconds'] + remaining if complete else None,
            'total_historical_envelope_seconds': [snap['elapsed_seconds'] + x for x in envelope]
                if complete else None,
            'unknown_phases': [name for name, p in phases.items() if p['estimate_seconds'] is None],
            'interpretation': 'conditional historical reference; no completion probability or time bound'}


def actual_phases(trip, snap):
    phases = copy.deepcopy(trip['phases'])
    # The model starts at the first observed choice, not arrival or touchdown.
    landed = trip['milestones'].get('landed')
    if snap['source_tick'] >= trip['stopped_tick'] or landed and landed[0] < snap['source_tick']:
        return {name: {'status': 'prediction_outside_prelanding_scope',
                       'physics_valid': False, 'seconds_bounds': None} for name in PHASES}
    if landed and landed[0] >= snap['source_tick']:
        phases['landing'] = {'status': 'completed', 'physics_valid': True,
            'seconds_bounds': [(t - snap['source_tick']) / costs.HZ for t in landed]}
    else:
        phases['landing'] = {'status': 'censored', 'physics_valid': True,
            'seconds_bounds': [max(0, trip['stopped_tick'] - snap['source_tick']) / costs.HZ] * 2}
    return {name: phases[name] for name in PHASES}


def add_samples(cells, run, attempt, trip):
    snap = attempt['choice']
    if snap is None:
        return
    category = snap['category']
    actual = actual_phases(trip, snap)
    for phase, measured in actual.items():
        ref = snap['references'][phase]
        mode = 'residual' if phase == 'claim' or category != 'no_flag' and phase in ['outbound', 'return_board'] else 'absolute'
        cell = cells.setdefault(f'{category}/{phase}', {'mode': mode, 'samples': [], 'excluded': []})
        reasons = list(snap['missing'])
        if measured['status'] != 'completed':
            reasons.append(measured['status'])
        if not measured.get('physics_valid'):
            reasons.append('older_or_unverified_physics')
        bounds = measured['seconds_bounds']
        if bounds is None or bounds[1] - bounds[0] > MAX_BOUNDARY_WIDTH:
            reasons.append('uncertain_boundary')
        if mode == 'residual' and (ref is None or not costs.finite(ref)):
            reasons.append('missing_reference')
        milestone = {'landing': 'landed', 'exit': 'exited', 'outbound': 'claim_started',
                     'claim': 'claimed', 'return_board': 'boarded', 'departure': 'departed'}[phase]
        end = trip['milestones'].get(milestone, [trip['stopped_tick']] * 2)[1]
        if any(snap['source_tick'] < change['tick'] <= end for change in attempt['post_choice_changes']):
            reasons.append('changed_dependencies_after_choice')
        source = {'label': run['label'], 'seed': run['seed'], 'seat': trip['seat'],
                  'selected_tick': trip['selected_tick'], 'prediction_tick': snap['source_tick'],
                  'ending': trip['ending']}
        if reasons:
            cell['excluded'].append({**source, 'reasons': reasons})
        else:
            offset = ref if mode == 'residual' else 0.0
            cell['samples'].append({**source, 'reference_seconds': ref,
                                    'seconds_bounds': [t - offset for t in bounds]})


def finish_profile(cells, runs, revision):
    for cell in cells.values():
        samples = cell['samples']
        bounds = [s['seconds_bounds'] for s in samples]
        refs = [s['reference_seconds'] for s in samples if s['reference_seconds'] is not None]
        cell['observed_seconds_envelope'] = [min(b[0] for b in bounds), max(b[1] for b in bounds)] if bounds else None
        cell['median_midpoint_seconds'] = statistics.median((b[0] + b[1]) / 2 for b in bounds) if bounds else None
        cell['reference_seconds_domain'] = [min(refs), max(refs)] if refs else None
    return {'version': 1, 'model': MODEL, 'policy': POLICY, 'runtime_revision': revision,
            'tick_hz': costs.HZ, 'max_training_boundary_width_seconds': MAX_BOUNDARY_WIDTH,
            'runs': runs, 'cells': cells,
            'limitations': ['Completed phases only; exclusions and unfinished attempts retained.',
                'Empirical min/max envelopes are not confidence intervals or guaranteed bounds.',
                'Per-phase sums ignore correlations and do not estimate completion probability.',
                'No extrapolation of route or claim references outside sampled domains.',
                'Radius, gravity, contact gaps, moving obstacles and damage are not predictive features.',
                'No future pilot visibility, remote surface route or strategic selection model.']}


def compare(trip, snap, prediction):
    actual = actual_phases(trip, snap)
    comparisons = {}
    for phase, measured in actual.items():
        point = prediction['phases'][phase]['estimate_seconds']
        envelope = prediction['phases'][phase]['historical_envelope_seconds']
        bounds = measured['seconds_bounds']
        completed = measured['status'] == 'completed'
        comparisons[phase] = {'actual': measured,
            'error_seconds_bounds': [point - bounds[1], point - bounds[0]]
                if completed and bounds and point is not None else None,
            'historical_envelope_contains_actual_bounds':
                envelope[0] <= bounds[0] and bounds[1] <= envelope[1]
                if completed and bounds and envelope is not None else None}
    total = trip['phases']['total']
    point, envelope, bounds = prediction['total_seconds'], prediction['total_historical_envelope_seconds'], total['seconds_bounds']
    scope_valid = actual['landing']['status'] != 'prediction_outside_prelanding_scope'
    finished = total['status'] == 'completed' and scope_valid
    return {'ending': trip['ending'], 'reason': trip['reason'], 'actual_phases': comparisons,
            'prediction_scope_valid': scope_valid,
            'actual_total': total,
            'total_error_seconds_bounds': [point - bounds[1], point - bounds[0]]
                if finished and point is not None else None,
            'total_historical_envelope_contains_actual_bounds':
                envelope[0] <= bounds[0] and bounds[1] <= envelope[1]
                if finished and envelope is not None else None,
            'censored_elapsed_exceeds_historical_max':
                bounds[0] > envelope[1] if scope_valid and not finished and envelope is not None else None}


def load_manifest(path):
    manifest = json.loads(path.read_text())
    if manifest['version'] != 1 or not manifest['runs']:
        raise ValueError('nonempty version-1 manifest required')
    labels = [c['label'] for c in manifest['runs']]
    if len(labels) != len(set(labels)):
        raise ValueError('duplicate run label')
    for c in manifest['runs']:
        if (c['source_commit'] != manifest['runtime_revision'] or c['physics_valid_from_tick'] != 0
                or c.get('scope', 'missions') != 'missions' or not c['seats']
                or any(s not in [0, 1] for s in c['seats'])):
            raise ValueError('normal current-physics runs of one frozen runtime are required')
    return manifest


def source_summary(run):
    return {k: run[k] for k in ['label', 'seed', 'source_commit', 'report_sha256', 'trace_sha256']}


def ensure_independent(profile, run):
    for training in profile['runs']:
        if (run['seed'] == training['seed'] or run['trace_sha256'] == training['trace_sha256']
                or run['report_sha256'] == training['report_sha256']):
            raise ValueError('evaluation must use disjoint world seeds and recordings')


def error_summary(values):
    exact = [v[0] for v in values if v is not None and v[0] == v[1]]
    return {'completed_exact_comparisons': len(exact),
            'median_absolute_error_seconds': statistics.median(abs(e) for e in exact) if exact else None,
            'mean_absolute_error_seconds': statistics.mean(abs(e) for e in exact) if exact else None,
            'signed_error_range_seconds': [min(exact), max(exact)] if exact else None}


def summarize(runs):
    attempts = [a for r in runs for a in r['attempts']]
    unobserved = [t for r in runs for t in r['attempts_without_observation']]
    records = [a['recorded_attempt'] for a in attempts if a['recorded_attempt']] + unobserved
    numeric = [a for a in attempts if a['prediction'] and a['prediction']['total_seconds'] is not None]
    compared = [a for a in numeric if a['actual']]
    complete = [a for a in compared if a['actual']['total_error_seconds_bounds'] is not None]
    groups = {}
    for changed, name in [(False, 'unchanged_dependencies'), (True, 'changed_dependencies')]:
        group = [a for a in complete if bool(a['post_choice_changes']) == changed]
        groups[name] = error_summary([a['actual']['total_error_seconds_bounds'] for a in group])
    return {'recorded_attempts': len(records), 'endings': dict(Counter(t['ending'] for t in records)),
            'attempts_without_observation': len(unobserved),
            'attempts_without_local_choice': sum(a['choice'] is None for a in attempts),
            'numeric_predictions': len(numeric), 'completed_numeric_predictions': len(complete),
            'completed_inside_historical_envelope': sum(
                a['actual']['total_historical_envelope_contains_actual_bounds'] is True for a in complete),
            'censored_above_historical_max': sum(
                a['actual']['censored_elapsed_exceeds_historical_max'] is True for a in compared),
            'unknown_choice_categories': dict(Counter(a['choice']['category'] for a in attempts
                if a['choice'] and a['prediction']['total_seconds'] is None)),
            'total_error': error_summary([a['actual']['total_error_seconds_bounds'] for a in compared]),
            'total_error_by_dependency_changes': groups,
            'phase_errors': {phase: error_summary([
                a['actual']['actual_phases'][phase]['error_seconds_bounds'] for a in compared])
                for phase in PHASES}}


def markdown(result):
    summary = result['summary']
    lines = ['# Frozen capture-trip estimates', '',
        'Seconds from mission selection to departure. Estimates use only the first observed '
        'landing choice; historical envelopes are conditional references, not promised bounds.', '',
        f"{summary['recorded_attempts']} attempts; {summary['numeric_predictions']} numeric estimates; "
        f"{summary['completed_numeric_predictions']} completed numeric comparisons. "
        f"{summary['attempts_without_local_choice']} attempts never expose a pre-landing choice. "
        'Failures and missing evidence remain in the JSON report.', '',
        '| Run / seat / selection | Class | Estimate | Historical envelope | Actual | Ending |',
        '| --- | --- | ---: | ---: | ---: | --- |']
    for run in result['runs']:
        for a in run['attempts']:
            p, actual = a.get('prediction'), a.get('actual')
            if not p or not actual:
                continue
            point, envelope = p['total_seconds'], p['total_historical_envelope_seconds']
            bounds = actual['actual_total']['seconds_bounds']
            value = 'unknown' if point is None else f'{point:.2f}'
            span = '—' if envelope is None else f'{envelope[0]:.2f}–{envelope[1]:.2f}'
            elapsed = f'{bounds[0]:.2f}–{bounds[1]:.2f}' if bounds else '—'
            if actual['ending'] != 'completed':
                elapsed += '+'
            lines.append(f"| {run['label']} / P{p['seat'] + 1} / {p['selected_tick']} | "
                         f"{p['category']} | {value} | {span} | {elapsed} | {actual['ending']} |")
    lines += ['', 'A trailing + is elapsed time before an unfinished attempt stops. '
              'JSON includes every attempt, missing choices, phase errors, evidence ages and sources. '
              'Pilot exposure remains unmeasured.', '']
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['calibrate', 'evaluate'])
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--profile', type=Path)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    manifest = load_manifest(args.manifest)
    base = args.manifest.resolve().parent
    args.out.mkdir(parents=True, exist_ok=True)
    if args.mode == 'calibrate':
        cells, sources, records = {}, [], []
        for config in manifest['runs']:
            snapshots = read_snapshots(config, base)
            run = costs.analyze_run(config, base)
            trips = {key(t): t for t in run['trips']}
            for attempt in snapshots:
                ident = key(attempt['selection'])
                if ident in trips:
                    add_samples(cells, run, attempt, trips[ident])
            sources.append(source_summary(run))
            records.append({**run, 'snapshots': snapshots})
            print(f"{config['label']}: {len(snapshots)} calibration attempts", flush=True)
        write_json(args.out / 'profile.json', finish_profile(cells, sources, manifest['runtime_revision']))
        write_json(args.out / 'calibration.json', {'version': 1, 'runs': records})
        return
    if args.profile is None:
        parser.error('evaluate requires a frozen --profile')
    profile = json.loads(args.profile.read_text())
    if (profile['version'] != 1 or profile['model'] != MODEL
            or profile['runtime_revision'] != manifest['runtime_revision']):
        raise ValueError('incompatible profile or runtime revision')
    predictions, dependency_changes = [], []
    for config in manifest['runs']:
        attempts = read_snapshots(config, base)
        changes = []
        for a in attempts:
            # Later observations belong to evaluation, never the prediction file.
            changes.append(a.pop('post_choice_changes'))
            a['selection_prediction'] = estimate(a['selection'], profile)
            a['prediction'] = estimate(a['choice'], profile) if a['choice'] else None
        dependency_changes.append(changes)
        predictions.append({'label': config['label'], 'attempts': attempts})
        print(f"{config['label']}: {len(attempts)} frozen predictions", flush=True)
    # Materialize predictions before opening any evaluation reports.
    frozen = {'version': 1, 'model': MODEL, 'profile_sha256': costs.file_hash(args.profile),
              'manifest_sha256': costs.file_hash(args.manifest), 'runs': predictions}
    write_json(args.out / 'predictions.json', frozen)
    evaluated = copy.deepcopy(frozen)
    for config, run, changes in zip(manifest['runs'], evaluated['runs'], dependency_changes):
        actual = costs.analyze_run(config, base)
        ensure_independent(profile, actual)
        run.update(source_summary(actual))
        trips = {key(t): t for t in actual['trips']}
        seen = set()
        for a, changed in zip(run['attempts'], changes):
            ident = key(a['selection'])
            t = trips.get(ident)
            seen.add(ident)
            a['actual'] = compare(t, a['choice'], a['prediction']) if t and a['choice'] else None
            a['recorded_attempt'] = t
            a['post_choice_changes'] = changed
        run['attempts_without_observation'] = [t for ident, t in trips.items() if ident not in seen]
    evaluated['summary'] = summarize(evaluated['runs'])
    write_json(args.out / 'evaluation.json', evaluated)
    (args.out / 'evaluation.md').write_text(markdown(evaluated))


if __name__ == '__main__':
    main()
