#!/usr/bin/env python3
"""Replay rolling pre-landing estimates without changing the bot or its profile.

Each estimate is conditional on the observed plan continuing successfully.
Invalidation, retry clocks and unknown tails remain distinct from that estimate.
"""
import argparse
import copy
import importlib.util
import json
import math
from pathlib import Path
import statistics

SPEC = importlib.util.spec_from_file_location('frozen_trip_estimates',
    Path(__file__).with_name('estimate-trip-costs.py'))
frozen = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(frozen)
costs = frozen.costs

MODEL = 'rolling-capture-trip-v1'
CAPTURE_TICKS = 150 * costs.HZ
CHECKPOINT_SECONDS = [0, 15, 30, 60, 120]


def objective(p):
    claim = p['planet']['claim']
    if not claim:
        return None
    flag, motion = claim['flag'], p['planet']['motion']
    point = None
    if flag:
        dx = flag['position']['x'] - motion['position']['x']
        dy = flag['position']['y'] - motion['position']['y']
        angle = -motion['angle']
        point = {'x': dx * math.cos(angle) - dy * math.sin(angle),
                 'y': dx * math.sin(angle) + dy * math.cos(angle)}
    return {'owner': claim['owner'], 'flag_owner': flag['player'] if flag else None,
            'local_position': point, 'range': claim['flag_interaction_range'],
            'stage_seconds': claim['stage_required_seconds']}


def same_objective(a, b):
    if a is None or b is None:
        return a == b
    if any(a[k] != b[k] for k in ['owner', 'flag_owner', 'stage_seconds']):
        return False
    if abs(a['range'] - b['range']) >= 0.01:
        return False
    x, y = a['local_position'], b['local_position']
    return x == y if x is None or y is None else costs.point_distance(x, y) < 0.5


def dependencies(row):
    p, c = frozen.pilot(row), row['mission']['capture']
    return {'planet': p['planet']['index'], 'revision': p['planet']['revision'],
            'site': c['site'], 'route': c['objective_route'], 'ship_form': p['ship_form'],
            'objective': objective(p), 'native_replans': c['replans'],
            'capture_started_tick': c['started_tick']}


def changed_dependencies(old, new):
    return [name for name in new if not (same_objective(old[name], new[name])
            if name == 'objective' else old[name] == new[name])]


def budgets(c, tick):
    start = c['started_tick']
    if start is not None and (start < 0 or start > tick):
        raise ValueError('invalid capture clock')
    counts = {name: c.get(name, 0) for name in
              ['replans', 'cover_replans', 'solar_replans', 'live_invalidations', 'circling_replans']}
    ordinary = counts['replans'] - counts['cover_replans'] - counts['solar_replans'] - counts['live_invalidations']
    if ordinary < 0 or any(type(n) is not int or n < 0 for n in counts.values()):
        raise ValueError('inconsistent native retry counters')
    limits = {'ordinary': (ordinary, 4), 'cover': (counts['cover_replans'], 8),
              'solar': (counts['solar_replans'], 8)}
    # Native code tests elapsed > 150 * 60 after its availability guards.
    deadline = start + CAPTURE_TICKS + 1 if start is not None else None
    return {'capture_started_tick': start, 'first_time_limit_tick': deadline,
            'capture_elapsed_seconds': (tick - start) / costs.HZ if start is not None else None,
            'seconds_until_time_limit': max(0, deadline - tick) / costs.HZ if deadline is not None else None,
            'time_limit_reached': tick >= deadline if deadline is not None else None,
            'native_counts': counts,
            'retries_remaining': {k: max(0, limit - n) for k, (n, limit) in limits.items()},
            'retry_limit_reached': any(n >= limit for n, limit in limits.values()),
            'completion_probability': 'unknown',
            'scope': 'capture controller limits; availability guards can defer their check'}


def progress(row):
    p, c = frozen.pilot(row), row['mission']['capture']
    landing = c['landing']
    tick = row['tick']
    since = c['circling_progress_tick'] if c['goal'] == 'seek_cover' else (
        landing['last_progress_tick'] if c['goal'] == 'surface' else None)
    if since is not None and not 0 <= since <= tick:
        raise ValueError('invalid progress clock')
    return {'tactical_goal': c['goal'], 'landing_goal': landing['goal'],
            'circling_remaining': c['circling_remaining'] if c['goal'] == 'seek_cover' else None,
            'seconds_since_native_progress': (tick - since) / costs.HZ if since is not None else None,
            'approach_progress_clock': 'not_exposed' if c['goal'] == 'approach' else None,
            'supported_feet': p['landing']['supported_feet'],
            'foot_clearances': p['landing']['foot_clearances'],
            'descent_speed': p['landing']['descent_speed'],
            'lateral_speed': p['landing']['lateral_speed'],
            'touchdown_adjustments': landing['touchdown_adjustments']}


def remaining_landing(cell, elapsed):
    """Condition successful historical durations on still waiting at this age.

    An interval whose upper end has elapsed cannot support another countdown.
    Intervals overlapping the current age retain uncertainty; no midpoint is
    treated as an exact measured duration or an unconditional survival rate.
    """
    if not cell or cell['mode'] != 'absolute' or not cell['samples']:
        return {'seconds': None, 'envelope_seconds': None, 'support': 0,
                'reason': 'no_landing_calibration'}
    remaining = [[max(0.0, a - elapsed), b - elapsed]
                 for sample in cell['samples'] for a, b in [sample['seconds_bounds']] if b > elapsed]
    if not remaining:
        return {'seconds': None, 'envelope_seconds': None, 'support': 0,
                'reason': 'beyond_historical_landing_duration'}
    return {'seconds': statistics.median((a + b) / 2 for a, b in remaining),
            'envelope_seconds': [min(a for a, _ in remaining), max(b for _, b in remaining)],
            'support': len(remaining), 'reason': None}


class RollingTrip:
    """One active attempt; only observe() consumes live/prefix information."""
    def __init__(self, selection, profile):
        self.selection = selection
        self.profile = profile
        self.original = None
        self.anchor = None
        self.retained = None
        self.epoch = 0
        self.reference_generation = 0
        self.epoch_tick = None
        self.first_choice_tick = None
        self.previous_tick = None
        self.terminal = False
        self.capture_started_tick = None

    def landing_estimate(self, row, result):
        return remaining_landing(self.profile['cells'].get(f"{self.anchor['category']}/landing"),
                                 result['plan_age_seconds'])

    def record(self):
        return {'selection': self.selection, 'original_prediction': self.original,
                'last_observed_tick': self.previous_tick}

    def observe(self, row):
        tick, p, mission = row['tick'], frozen.pilot(row), row['mission']
        if self.terminal:
            return None
        if self.previous_tick is not None and tick <= self.previous_tick:
            raise ValueError('rolling observations must advance')
        gap = self.previous_tick is not None and tick != self.previous_tick + 1
        self.previous_tick = tick
        if (row['seat'] != self.selection['seat'] or p['tick'] != tick
                or p['owner'] != f"player_{row['seat'] + 1}" or tick < self.selection['selected_tick']):
            raise ValueError('rolling actor or clock mismatch')
        c = mission.get('capture')
        terminal = ('attempt_left' if mission['target'] != self.selection['planet'] else
                    'landed' if c and c['landing']['landed_tick'] is not None else
                    'controller_failed' if c and c['failed_tick'] is not None else
                    'left_ship' if p['location'] == 'on_foot' else
                    'ship_lost' if not p['ship_available'] or p['ship_form'] != 'ship' else None)
        result = {'tick': tick, 'seat': row['seat'], 'planet': self.selection['planet'],
                  'selected_tick': self.selection['selected_tick'], 'model': MODEL,
                  'elapsed_seconds': (tick - self.selection['selected_tick']) / costs.HZ,
                  'status': 'unknown', 'unknown_reasons': [], 'invalidated_by': [],
                  'remaining_seconds': None, 'total_seconds': None,
                  'remaining_historical_envelope_seconds': None,
                  'total_historical_envelope_seconds': None, 'landing': None,
                  'diagnostic_only': True, 'physical_permissions': False}
        if terminal:
            self.terminal = True
            self.anchor = None
            result['terminal_reason'] = (c.get('failure') if c and terminal == 'controller_failed'
                                         else mission.get('reason'))
            if c and terminal != 'attempt_left':
                result['budget'] = budgets(c, tick)
            return {**result, 'status': terminal}
        if not c:
            return {**result, 'unknown_reasons': ['capture_not_started']}
        result['budget'] = budgets(c, tick)
        if self.capture_started_tick is None and c['started_tick'] is not None:
            self.capture_started_tick = c['started_tick']
        result['budget']['first_observed_capture_started_tick'] = self.capture_started_tick
        result['budget']['native_clock_changed'] = c['started_tick'] != self.capture_started_tick
        result['progress'] = progress(row)
        new = dependencies(row)
        changed = changed_dependencies(self.retained, new) if self.retained is not None else []
        # Ground-route or material evidence can refresh while the same physical
        # approach continues. Only observed controller/target restarts reset its
        # age; none of these reset elapsed mission time or the capture budget.
        restarted = any(name in changed for name in
                        ['site', 'native_replans', 'capture_started_tick', 'planet', 'ship_form'])
        if gap and self.first_choice_tick is not None:
            changed.append('observation_gap')
        if changed:
            self.anchor = None
            if self.first_choice_tick is not None:
                self.reference_generation += 1
                result['invalidated_by'] = changed
            if restarted:
                self.epoch += int(self.first_choice_tick is not None)
                self.epoch_tick = tick
        # Preserve the objective origin until it actually changes; otherwise
        # repeated sub-threshold motion could conceal cumulative flag drift.
        if self.retained is None or changed:
            self.retained = copy.deepcopy(new)
        if c['site'] is not None and self.first_choice_tick is None:
            self.first_choice_tick = self.epoch_tick = tick
            snap = frozen.snapshot(row, self.selection['selected_tick'], 'local_choice')
            self.original = frozen.estimate(snap, self.profile)
        if self.first_choice_tick is None:
            return {**result, 'unknown_reasons': ['landing_choice_not_observed']}
        stale = row['observation']['local'].get('objective_work') == 'stale'
        if stale and self.anchor is not None:
            self.anchor = None
            self.reference_generation += 1
            result['invalidated_by'].append('native_objective_evidence_stale')
        if self.anchor is None and c['site'] is not None and not stale:
            snap = frozen.snapshot(row, self.selection['selected_tick'], 'local_choice')
            result['evidence_missing'] = snap['missing']
            if not snap['missing']:
                self.anchor = frozen.estimate(snap, self.profile)
        reasons = result['unknown_reasons']
        if self.anchor is None:
            reasons.append('awaiting_current_plan_evidence')
        if p['planet']['index'] != self.selection['planet']:
            reasons.append('remote_ground_unmeasured')
        if not p['queries_ready']:
            reasons.append('local_queries_unavailable')
        if stale:
            reasons.append('native_objective_evidence_stale')
        if result['budget']['time_limit_reached'] or result['budget']['retry_limit_reached']:
            reasons.append('capture_limit_reached')
        result.update(plan_generation=self.epoch, reference_generation=self.reference_generation,
            plan_observed_tick=self.epoch_tick,
            plan_age_seconds=(tick - self.epoch_tick) / costs.HZ,
            first_choice_tick=self.first_choice_tick,
            seconds_before_current_plan=(self.epoch_tick - self.first_choice_tick) / costs.HZ,
            original_total_seconds=self.original['total_seconds'],
            original_prediction_tick=self.original['source_tick'])
        if self.anchor:
            anchor = self.anchor
            result['category'] = anchor['category']
            result['unknown_tail_phases'] = {name: p['unknown_reasons'] for name, p in anchor['phases'].items()
                                             if name != 'landing' and p['estimate_seconds'] is None}
            result['reference_acquired_tick'] = anchor['source_tick']
            result['route_source_tick'] = anchor['dependencies']['route_source_tick']
            result['route_age_ticks'] = tick - result['route_source_tick'] if result['route_source_tick'] is not None else None
            result['ground_reference_scope'] = 'historical timing only; physical validity is not renewed'
            landing = self.landing_estimate(row, result)
            result['landing'] = landing
            if landing['reason']:
                reasons.append(landing['reason'])
            tail = [anchor['phases'][phase] for phase in frozen.PHASES if phase != 'landing']
            if any(p['estimate_seconds'] is None for p in tail):
                reasons.append('unknown_surface_or_departure_cost')
            if not reasons:
                remaining = landing['seconds'] + sum(p['estimate_seconds'] for p in tail)
                envelope = [landing['envelope_seconds'][i] + sum(p['historical_envelope_seconds'][i] for p in tail)
                            for i in [0, 1]]
                result.update(status='estimate', remaining_seconds=remaining,
                    total_seconds=result['elapsed_seconds'] + remaining,
                    remaining_historical_envelope_seconds=envelope,
                    total_historical_envelope_seconds=[result['elapsed_seconds'] + n for n in envelope])
        until = result['budget']['seconds_until_time_limit']
        result['budget']['estimated_trip_exceeds_time_limit'] = (
            result['remaining_seconds'] > until if result['remaining_seconds'] is not None and until is not None else None)
        return result


def replay(config, base, profile, output, trip_factory=RollingTrip):
    """Write forecasts while reading the trace, before opening any outcome report."""
    previous = [-1, -1]
    active = [None, None]
    attempts = []
    with (base / config['directory'] / 'trace.jsonl').open() as stream:
        for row in map(json.loads, stream):
            seat, tick, p = row['seat'], row['tick'], frozen.pilot(row)
            if (row['version'] != 1 or seat not in [0, 1] or tick <= previous[seat]
                    or p['tick'] != tick or p['owner'] != f'player_{seat + 1}'):
                raise ValueError('unsupported or nonmonotonic trace identity')
            previous[seat] = tick
            if seat not in config['seats']:
                continue
            m = row['mission']
            if m['policy'] != frozen.POLICY or any(e['tick'] > tick for e in m['events']):
                raise ValueError('unsupported policy or future event')
            selections = [e for e in m['events'] if e['kind'] == 'selected']
            selected = selections[-1] if selections and selections[-1]['planet'] == m['target'] else None
            ident = (seat, m['target'], selected['tick']) if selected else None
            state = active[seat]
            if state is not None and ident != frozen.key(state.selection):
                if not state.terminal:
                    # A reselected copy of the same planet is a different attempt.
                    retired = {**row, 'mission': {**row['mission'], 'target': None}}
                    result = state.observe(retired)
                    output.write(json.dumps(result, allow_nan=False) + '\n')
                state = active[seat] = None
            if state is None and selected:
                state = active[seat] = trip_factory(frozen.snapshot(row, selected['tick'], 'mission_selection'), profile)
                attempts.append(state)
            if state is None:
                continue
            result = state.observe(row)
            if result is not None:
                # Fixed one-second samples plus lifecycle/phase/evidence events.
                signature = (result['status'], result.get('plan_generation'), result['unknown_reasons'],
                             result.get('progress', {}).get('tactical_goal'),
                             result.get('progress', {}).get('landing_goal'),
                             result.get('flight_phase', {}).get('episode'))
                if (state.first_choice_tick is not None and (tick - state.first_choice_tick) % costs.HZ == 0
                        or result['invalidated_by'] or signature != getattr(state, '_last_signature', None)):
                    output.write(json.dumps(result, allow_nan=False) + '\n')
                state._last_signature = signature
    return [s.record() for s in attempts]


def evaluate_run(run, attempts, updates):
    trips = {frozen.key(t): t for t in run['trips']}
    indexed = {}
    for row in updates:
        indexed.setdefault(frozen.key(row), {})[row['tick']] = row
    records = []
    for a in attempts:
        ident = frozen.key(a['selection'])
        trip = trips.pop(ident, None)
        rows = indexed.get(ident, {})
        original = a['original_prediction']
        checkpoints = []
        if original and trip:
            landed = trip['milestones'].get('landed')
            for seconds in CHECKPOINT_SECONDS:
                tick = original['source_tick'] + seconds * costs.HZ
                status = ('attempt_ended' if tick >= trip['stopped_tick'] else
                          'already_landed' if landed and tick >= landed[0] else 'missing_observation')
                update = rows.get(tick)
                eligible = status == 'missing_observation'
                completed = trip['ending'] == 'completed'
                actual = trip['phases']['total']['seconds_bounds']
                point = update['total_seconds'] if update and eligible else None
                old = original['total_seconds'] if eligible else None
                checkpoints.append({'after_seconds': seconds, 'tick': tick,
                    'status': update['status'] if eligible and update else status,
                    'unknown_reasons': update['unknown_reasons'] if eligible and update else [],
                    'original_total_seconds': old, 'rolling_total_seconds': point,
                    'plan_generation': update.get('plan_generation') if eligible and update else None,
                    'plan_age_seconds': update.get('plan_age_seconds') if eligible and update else None,
                    'seconds_before_current_plan': update.get('seconds_before_current_plan') if eligible and update else None,
                    'original_error_seconds_bounds': [old - actual[1], old - actual[0]] if completed and old is not None else None,
                    'rolling_error_seconds_bounds': [point - actual[1], point - actual[0]] if completed and point is not None else None,
                    'budget': update.get('budget') if eligible and update else None})
        records.append({**a, 'actual': trip, 'checkpoints': checkpoints,
                        'update_count': len(rows),
                        'invalidations': [{'tick': t, 'causes': u['invalidated_by']} for t, u in rows.items() if u['invalidated_by']]})
    return {**frozen.source_summary(run), 'attempts': records, 'unobserved_attempts': list(trips.values())}


def summarize(runs):
    result = {}
    for seconds in CHECKPOINT_SECONDS:
        rows = [c for run in runs for a in run['attempts'] for c in a['checkpoints'] if c['after_seconds'] == seconds]
        paired = [c for c in rows if c['original_error_seconds_bounds'] is not None and c['rolling_error_seconds_bounds'] is not None]
        result[str(seconds)] = {'statuses': dict(frozen.Counter(c['status'] for c in rows)),
            'unknown_reasons': dict(frozen.Counter(r for c in rows for r in c['unknown_reasons'])),
            'paired_completed': len(paired),
            'original_error': frozen.error_summary([c['original_error_seconds_bounds'] for c in paired]),
            'rolling_error': frozen.error_summary([c['rolling_error_seconds_bounds'] for c in paired])}
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--profile', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    manifest = frozen.load_manifest(args.manifest)
    profile = json.loads(args.profile.read_text())
    if (profile['version'] != 1 or profile['model'] != frozen.MODEL
            or profile['runtime_revision'] != manifest['runtime_revision']):
        raise ValueError('incompatible frozen profile')
    args.out.mkdir(parents=True, exist_ok=True)
    forecasts, paths = [], []
    for index, config in enumerate(manifest['runs']):
        path = args.out / f'updates-{index}.jsonl'
        with path.open('w') as output:
            attempts = replay(config, args.manifest.resolve().parent, profile, output)
        forecasts.append({'label': config['label'], 'attempts': attempts, 'updates_file': path.name,
                          'updates_sha256': costs.file_hash(path)})
        paths.append(path)
        print(f"{config['label']}: {len(attempts)} rolling attempts", flush=True)
    frozen.write_json(args.out / 'predictions.json', {'version': 1, 'model': MODEL,
        'profile_sha256': costs.file_hash(args.profile), 'manifest_sha256': costs.file_hash(args.manifest), 'runs': forecasts})
    evaluated = []
    for config, prediction, path in zip(manifest['runs'], forecasts, paths):
        run = costs.analyze_run(config, args.manifest.resolve().parent)
        frozen.ensure_independent(profile, run)
        with path.open() as stream:
            evaluated.append(evaluate_run(run, prediction['attempts'], map(json.loads, stream)))
    frozen.write_json(args.out / 'evaluation.json', {'version': 1, 'model': MODEL,
        'profile_sha256': costs.file_hash(args.profile), 'manifest_sha256': costs.file_hash(args.manifest),
        'runs': evaluated, 'summary': summarize(evaluated)})


if __name__ == '__main__':
    main()
