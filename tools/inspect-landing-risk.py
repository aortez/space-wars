#!/usr/bin/env python3
"""Inspect near-term landing interruptions in hash-bound, known recordings.

This is descriptive evidence, not a fitted probability or controller. Features
use a prefix only. Labels stop at the first interruption, physical landing or
uncertain boundary; failed/short observations never become negative examples.
"""
import argparse
from collections import Counter, defaultdict
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import statistics

SPEC = importlib.util.spec_from_file_location('risk_tail',
    Path(__file__).with_name('inspect-landing-tail.py'))
tail = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(tail)
HZ = tail.costs.HZ
MODEL = 'landing-interruption-diagnostic-v1'
CHECKPOINTS = (0, 15, 30, 60, 120)
HORIZON = 10 * HZ
HISTORY = 5 * HZ
RESET_TAGS = {'observation_gap', 'capture_changed', 'counter_reset'}
PLAN_TAGS = RESET_TAGS | {'site_acquired', 'site_cleared', 'site_changed',
                         'phase_changed', 'revision_changed', 'native_replan'}
PRELANDING = {'flying', 'assisted', 'settling'}
RULE = {'checkpoints_seconds': CHECKPOINTS, 'primary_checkpoints_seconds': [0, 15],
    'horizon_seconds': HORIZON / HZ, 'history_seconds': HISTORY / HZ,
    'progress_window_seconds': 1, 'native_progress_age_split_seconds': 5,
    'endpoint': 'first target physical landing or native prelanding replan',
    'same_tick_landing_and_replan': 'ambiguous', 'physical_permissions': False}


def read_bound(path, expected=None):
    data = path.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if expected is not None and digest != expected:
        raise ValueError('source hash mismatch: ' + str(path))
    return json.loads(data), digest


def asteroid_interval(report, scope):
    if scope == 'controlled':
        return None
    value = report.get('asteroids', {}).get('settings', {}).get('interval_seconds')
    if not tail.costs.finite(value) or value < 0:
        raise ValueError('missing or invalid asteroid configuration')
    return value


def physical(sample, target):
    """Use the attempt's target, including at a later mission-change row."""
    c = sample['contact']
    if c.get('planet') != target:
        return 'unknown'
    if c.get('phase') == 'landed':
        return 'landed'
    return 'prelanding' if c.get('phase') in PRELANDING else 'unknown'


def timeline(trace):
    """Check the sparse event contract established by dense LandingTrace replay.

    Every transition retains the immediately preceding observed row. Sparse
    grid spacing is not an observation gap; explicit gap events account for it.
    Geometry is sampled, so endpoint progress is not a dense progress history.
    """
    samples = trace['samples']
    ticks = [s['tick'] for s in samples]
    if (not ticks or any(type(t) is not int or t < 0 for t in ticks)
            or ticks != sorted(set(ticks))):
        raise ValueError('invalid or unordered samples')
    positions = {t: i for i, t in enumerate(ticks)}
    events, missing = [], 0
    event_ticks = [e['tick'] for e in trace['events']]
    if not event_ticks or event_ticks != sorted(set(event_ticks)) or event_ticks[0] != ticks[0]:
        raise ValueError('invalid event ordering')
    for e in trace['events']:
        i = positions[e['tick']]
        before, after = samples[i - 1] if i else None, samples[i]
        if tail.transition(before, after) != {k: e[k] for k in ('tags', 'counter_delta')}:
            raise ValueError('event differs from retained adjacent evidence')
        gap_start = None
        if 'observation_gap' in e['tags']:
            gap_start = before['tick'] + 1
            missing += after['tick'] - gap_start
        events.append({**e, 'before': before, 'after': after, 'gap_start': gap_start})
    if trace['observations'] != ticks[-1] - ticks[0] + 1 - missing:
        raise ValueError('observation coverage differs from event ledger')
    return events


def replan_kind(event, target):
    if RESET_TAGS.intersection(event['tags']):
        return None
    delta = event['counter_delta']
    if delta is None or delta['replans'] <= 0:
        return None
    state = physical(event['before'], target)
    return {'prelanding': 'prelanding_replan', 'landed': 'grounded_replan',
            'unknown': 'unclassified_replan'}[state]


def features(samples, events, target, tick):
    """No actual outcomes, future rows, phase-episode ends or final tails."""
    past = [s for s in samples if s['tick'] <= tick]
    history = [e for e in events if e['tick'] <= tick]
    if not past or past[-1]['tick'] != tick:
        return {'status': 'checkpoint_not_observed'}
    current = past[-1]
    if any(physical(s, target) == 'landed' for s in past):
        return {'status': 'physical_landing_already_observed'}
    if current['site'] is None or current['phase']['name'] is None:
        return {'status': 'active_plan_unavailable'}
    if physical(current, target) != 'prelanding':
        return {'status': 'contact_frame_or_phase_unavailable'}
    start = tick - HISTORY
    recent = [e for e in history if start < e['tick'] <= tick]
    full_history = (past[0]['tick'] <= start and current['capture_started_tick'] is not None
                    and current['capture_started_tick'] <= start
                    and not any(RESET_TAGS.intersection(e['tags']) for e in recent))
    counts = Counter()
    for e in recent:
        if not RESET_TAGS.intersection(e['tags']) and e['counter_delta'] is not None:
            counts.update({k: v for k, v in e['counter_delta'].items() if v > 0})
            kind = replan_kind(e, target)
            if kind:
                counts[kind] += e['counter_delta']['replans']
        if 'revision_changed' in e['tags']:
            counts['revision_changed'] += 1
    recent_evidence = {'complete': full_history, 'window_seconds': HISTORY / HZ,
                       'counts': dict(counts) if full_history else None}

    # Only compare measured endpoints on the same uninterrupted plan and frame.
    # Intermediate query availability is not in this sparse schema.
    old = next((s for s in past if s['tick'] == tick - HZ), None)
    changes = [e for e in history if tick - HZ < e['tick'] <= tick]
    progress = None
    if (old is not None and old['geometry'] is not None and current['geometry'] is not None
            and not any(PLAN_TAGS.intersection(e['tags']) for e in changes)
            and all(old[k] == current[k] for k in ('site', 'revision', 'capture_started_tick'))
            and old['phase'].get('episode') == current['phase'].get('episode')):
        progress = {'from_tick': old['tick'], 'seconds': 1,
            'distance_closed': old['geometry']['distance'] - current['geometry']['distance'],
            'height_reduced': old['geometry']['height'] - current['geometry']['height'],
            'absolute_side_error_reduced': abs(old['geometry']['side_error'])
                                           - abs(current['geometry']['side_error'])}
    # The landing pilot's clock is stale during tactical approach/circling.
    age = current['seconds_since_landing_progress'] if current['phase']['name'] in (
        'alignment', 'descent', 'settling') else None
    return {'status': 'eligible', 'phase': copy.deepcopy(current['phase']),
        'site': copy.deepcopy(current['site']), 'revision': current['revision'],
        'contact': copy.deepcopy(current['contact']), 'geometry': copy.deepcopy(current['geometry']),
        'geometry_unavailable': current['geometry_unavailable'],
        'native_progress_age_seconds': age, 'one_second_endpoint_progress': progress,
        'recent': recent_evidence, 'objective_work': current['objective_work'],
        'queries_ready': current['queries_ready'], 'physical_permissions': False}


def label(trace, events, actual, tick):
    """A positive must precede any uncertain interval and physical landing.

    A gap can begin inside the horizon even when its next observation is later.
    Stop at the first physical landing, not the controller acknowledgement.
    A simultaneous landing/replan has no defensible within-tick ordering here.
    """
    if not trace['samples'][0]['tick'] <= tick <= trace['samples'][-1]['tick'] or tick >= actual['stopped_tick']:
        raise ValueError('forecast outside observed attempt')
    target, end = trace['selection']['planet'], tick + HORIZON
    endpoints = []
    for e in events:
        if e['gap_start'] is not None and tick < e['gap_start'] <= end:
            endpoints.append((e['gap_start'], 'censored_observation_gap'))
        if not tick < e['tick'] <= end:
            continue
        if 'capture_changed' in e['tags'] or 'counter_reset' in e['tags']:
            endpoints.append((e['tick'], 'censored_capture_or_counter_reset'))
        kind = replan_kind(e, target)
        if kind == 'prelanding_replan':
            endpoints.append((e['tick'], 'interrupted'))
        elif kind == 'unclassified_replan':
            endpoints.append((e['tick'], 'censored_unclassified_replan'))
    for s in trace['samples']:
        if tick < s['tick'] <= end:
            state = physical(s, target)
            if state == 'landed':
                endpoints.append((s['tick'], 'landed'))
            elif state == 'unknown':
                endpoints.append((s['tick'], 'censored_contact_frame_or_phase'))
    last = trace['samples'][-1]['tick']
    if actual['stopped_tick'] <= end:
        status = ('attempt_ended' if actual['ending'] not in ('match_end', 'completed')
                  else 'censored_recording_end')
        endpoints.append((actual['stopped_tick'], status))
    if last < end:
        status = ('attempt_ended' if actual['stopped_tick'] <= last + 1
                  and actual['ending'] not in ('match_end', 'completed') else 'censored_recording_end')
        endpoints.append((last + 1, status))
    # This is a competing-endpoint description, not a Kaplan-Meier estimate.
    stop = min((t for t, _ in endpoints), default=end)
    tied = {s for t, s in endpoints if t == stop}
    uncertain = sorted(s for s in tied if s.startswith('censored_') or s == 'attempt_ended')
    status = (uncertain[0] if uncertain else 'ambiguous_landing_replan' if
              {'interrupted', 'landed'} <= tied else next(iter(tied), 'horizon_clear'))
    return {'status': status, 'tick': stop, 'observed_future_seconds': (stop - tick) / HZ,
            'horizon_seconds': HORIZON / HZ, 'tied_endpoints': sorted(tied),
            'interrupted': True if status == 'interrupted' else False if status in
                ('landed', 'horizon_clear') else None}


def strata(f):
    """Declared descriptive splits; none is a fitted threshold or risk rule."""
    recent, g = f['recent'], f['geometry']
    counts = recent['counts']
    result = {'phase': f['phase']['name'], 'context': f['phase']['context'],
        'feet': str(f['contact']['supported_feet']),
        'native_contact': f['contact']['phase'], 'objective_work': str(f['objective_work']),
        'queries_ready': str(f['queries_ready'])}
    for key in ('prelanding_replan', 'live_invalidations', 'revision_changed'):
        result['recent_' + key] = ('unknown' if counts is None else
                                   'yes' if counts.get(key, 0) > 0 else 'no')
    if counts and counts.get('unclassified_replan', 0) > 0 and not counts.get('prelanding_replan', 0):
        result['recent_prelanding_replan'] = 'unknown'
    progress, age = f['one_second_endpoint_progress'], f['native_progress_age_seconds']
    result['endpoint_progress'] = ('unknown' if progress is None else
        'no_net_closure' if progress['distance_closed'] <= 0 else 'closing')
    result['native_progress'] = ('unknown' if age is None else 'age_ge_5s' if age >= 5 else 'age_lt_5s')
    result['closing_speed'] = ('unknown' if g is None or g['closing_speed'] is None else
                                'not_closing' if g['closing_speed'] <= 0 else 'closing')
    return result


def statistics_for(rows):
    counts = Counter(r['outcome']['status'] for r in rows)
    positive = sum(r['outcome']['interrupted'] is True for r in rows)
    unknown = sum(r['outcome']['interrupted'] is None for r in rows)
    worlds = defaultdict(list)
    for r in rows:
        worlds[r['seed']].append(r)
    bounds = []
    for group in worlds.values():
        yes = sum(r['outcome']['interrupted'] is True for r in group)
        maybe = sum(r['outcome']['interrupted'] is None for r in group)
        bounds.append((yes / len(group), (yes + maybe) / len(group)))
    return {'attempts': len(rows), 'worlds': len(worlds), 'outcomes': dict(counts),
        'interrupted': positive, 'unknown': unknown,
        'observed_fraction_bounds': [positive / len(rows), (positive + unknown) / len(rows)] if rows else None,
        'equal_world_fraction_bounds': [statistics.mean(b[i] for b in bounds) for i in (0, 1)] if bounds else None}


def summarize(rows):
    groups = defaultdict(list)
    availability = Counter()
    for r in rows:
        prefix = (r['scope'], str(r['checkpoint_seconds']))
        availability['/'.join((*prefix, r['features']['status']))] += 1
        if r['features']['status'] != 'eligible':
            continue
        values = {**strata(r['features']), 'asteroids': str(r['asteroid_interval_seconds'])}
        groups[(*prefix, 'all', 'all')].append(r)
        groups[(*prefix, 'world', str(r['seed']))].append(r)
        for key, value in values.items():
            groups[(*prefix, key, value)].append(r)
        # Show the recent-replan signal within phase and asteroid conditions,
        # instead of treating different flight states as interchangeable.
        for condition in ('phase', 'asteroids'):
            groups[(*prefix, condition + '_by_recent_replan',
                    values[condition] + '/' + values['recent_prelanding_replan'])].append(r)
    return {'availability': dict(sorted(availability.items())),
            'groups': [{'scope': k[0], 'checkpoint_seconds': int(k[1]), 'split': k[2], 'value': k[3],
                        **statistics_for(v)} for k, v in sorted(groups.items())]}


def load_runs(manifest, base):
    """Bind existing dense-replay derivatives; raw recordings stay in prior archives."""
    seen, names, provenance = set(), set(), []
    for dataset in manifest['datasets']:
        if dataset['name'] in names:
            raise ValueError('duplicate dataset')
        names.add(dataset['name'])
        paths = {k: base / dataset[k] for k in ('index', 'manifest', 'evaluation')}
        bound = {k: read_bound(p, dataset[k + '_sha256']) for k, p in paths.items()}
        index, config, evaluation = (bound[k][0] for k in ('index', 'manifest', 'evaluation'))
        tail.verify_evaluation(config, evaluation, manifest['runtime_revision'], bound['manifest'][1])
        if (index['version'] != 1 or index['model'] != tail.MODEL
                or index['runtime_revision'] != manifest['runtime_revision']
                or not any(s['name'] == dataset['source_name'] and
                    s['manifest_sha256'] == bound['manifest'][1] and
                    s['evaluation_sha256'] == bound['evaluation'][1] for s in index['sources'])):
            raise ValueError('unbound derived index')
        indexed = [r for r in index['runs'] if r['dataset'] == dataset['source_name']]
        if len(indexed) != len(evaluation['runs']):
            raise ValueError('derived run count differs')
        scope = 'controlled' if config.get('scope') == tail.composed.controlled.SCOPE else 'normal'
        for entry, source, job in zip(indexed, evaluation['runs'], config['runs']):
            path = paths['index'].parent / entry['path']
            trace, digest = read_bound(path, entry['sha256'])
            if (entry['label'] != job['label'] or any(entry[k] != source[k] or trace[k] != source[k]
                    for k in ('label', 'seed', 'trace_sha256', 'report_sha256'))
                    or len(trace['attempts']) != len(source['attempts'])
                    or entry['attempts'] != len(trace['attempts'])):
                raise ValueError('derived run identity differs')
            report_path = paths['manifest'].parent / job['directory'] / 'report.json'
            report, report_hash = read_bound(report_path, source['report_sha256'])
            if report['seed'] != source['seed']:
                raise ValueError('report world differs')
            for seat in {a['selection']['seat'] for a in trace['attempts']}:
                key = (source['trace_sha256'], seat)
                if key in seen:
                    raise ValueError('duplicate recording and seat')
                seen.add(key)
            for t, a in zip(trace['attempts'], source['attempts']):
                if any(t[k] != a[k] for k in ('selection', 'phase_episodes', 'phase_breaks')):
                    raise ValueError('derived attempt lifecycle differs')
                actual, selection = a['actual'], t['selection']
                if (actual['seat'] != selection['seat'] or actual['planet'] != selection['planet']
                        or actual['selected_tick'] != selection['selected_tick']
                        or actual['ending'] != t['actual_ending']):
                    raise ValueError('actual attempt identity differs')
            provenance.append({'dataset': dataset['name'], 'path': str(path.resolve()), 'sha256': digest,
                'report': str(report_path.resolve()), 'report_sha256': report_hash})
            yield {'dataset': dataset['name'], 'scope': scope, 'label': source['label'],
                'seed': source['seed'], 'asteroid_interval_seconds': asteroid_interval(report, scope),
                'trial_outcome': source.get('trial_outcome'), 'attempts': trace['attempts'],
                'source_attempts': source['attempts'], 'unobserved_attempts': source.get('unobserved_attempts', []),
                'provenance': provenance[-1]}


def inspect(manifest_path, out):
    manifest, manifest_hash = read_bound(manifest_path)
    if manifest['version'] != 1 or not manifest['datasets']:
        raise ValueError('invalid risk manifest')
    out.mkdir(parents=True, exist_ok=False)
    records, retained, audit, runs = [], [], [], []
    for run in load_runs(manifest, manifest_path.resolve().parent):
        identity = {k: run[k] for k in ('dataset', 'scope', 'label', 'seed', 'asteroid_interval_seconds')}
        runs.append({**identity, 'attempts': len(run['attempts']), 'trial_outcome': run['trial_outcome'],
            'unobserved_attempts': run['unobserved_attempts'], 'provenance': run['provenance']})
        for attempt, source in zip(run['attempts'], run['source_attempts']):
            events = timeline(attempt)
            selection = attempt['selection']
            key = {k: selection[k] for k in ('seat', 'planet', 'selected_tick')}
            for e in events:
                kind = replan_kind(e, selection['planet'])
                if kind or 'site_acquired' in e['tags'] or RESET_TAGS.intersection(e['tags']):
                    audit.append({**identity, **key, 'tick': e['tick'], 'kind': kind,
                        'tags': e['tags'], 'counter_delta': e['counter_delta'],
                        'before_contact': e['before']['contact'] if e['before'] else None,
                        'after_contact': e['after']['contact'], 'gap_start': e['gap_start']})
            for seconds in CHECKPOINTS:
                tick = attempt['first_choice_tick']
                tick = tick + seconds * HZ if tick is not None else None
                f = features(attempt['samples'], events, selection['planet'], tick) if tick is not None else {
                    'status': 'no_observed_choice'}
                records.append({**identity, **key, 'checkpoint_seconds': seconds, 'tick': tick, 'features': f})
            retained.append((attempt, source['actual'], events))
    # Persist causal inputs before outcome labelling. No fitting is performed.
    feature_path = out / 'features.json'
    tail.frozen.write_json(feature_path, {'model': MODEL, 'rule': RULE, 'records': records})
    feature_hash = tail.costs.file_hash(feature_path)
    for i, (attempt, actual, events) in enumerate(retained):
        for record in records[i * len(CHECKPOINTS):(i + 1) * len(CHECKPOINTS)]:
            record['actual_ending'] = actual['ending']
            record['actual_reason'] = actual.get('reason')
            record['outcome'] = (label(attempt, events, actual, record['tick'])
                if record['features']['status'] == 'eligible' else None)
    tail.frozen.write_json(out / 'outcomes.json', {'model': MODEL, 'features_sha256': feature_hash, 'records': records})
    tail.frozen.write_json(out / 'event-audit.json', {'events': audit})
    tail.frozen.write_json(out / 'summary.json', {'model': MODEL, 'rule': RULE, 'manifest_sha256': manifest_hash,
        'features_sha256': feature_hash, 'recordings': len(runs), 'worlds': len({r['seed'] for r in runs}),
        'attempts': len(retained), 'endings': dict(Counter(a['ending'] for _, a, _ in retained)),
        'runs': runs, **summarize(records), 'limitations': [
            'Known recordings only: no independent validation, probability fit or controller change.',
            'One row per attempt at each fixed checkpoint; later checkpoints condition on surviving that long.',
            'Ten-second endpoint fractions include first landing as a competing event.',
            'Censoring bounds are descriptive extremes, not confidence intervals or calibrated probabilities.',
            'Recent history needs five complete seconds in one capture scope.',
            'Endpoint progress compares two measured samples, not a dense proof of continuous progress.',
            'Live invalidation and objective replan counters overlap.',
            'Raw trace hashes are inherited from verified dense replay; this run binds derived data and reports.']})
    if tail.costs.file_hash(feature_path) != feature_hash:
        raise ValueError('causal features changed during labelling')
    print(json.dumps({'recordings': len(runs), 'worlds': len({r['seed'] for r in runs}),
                      'attempts': len(retained), 'features_sha256': feature_hash}))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    inspect(args.manifest, args.out)


if __name__ == '__main__':
    main()
