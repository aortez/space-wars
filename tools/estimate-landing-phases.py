#!/usr/bin/env python3
"""Calibrate and replay read-only landing estimates from observed flight phases.

Historical successful episodes must still be in the current phase at its current
age. Retries have separate evidence; no initial-approach fallback is permitted.
This does not change the controller, renew routes, or predict success probability.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import statistics

SPEC = importlib.util.spec_from_file_location('rolling_trip_estimates',
    Path(__file__).with_name('roll-trip-estimates.py'))
rolling = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(rolling)
frozen, costs = rolling.frozen, rolling.costs

MODEL = 'phase-landing-estimate-v1'
MIN_ATTEMPTS = 2


def phase_name(row):
    p, m = frozen.pilot(row), row['mission']
    c = m.get('capture')
    if (not c or not c['site'] or c['failed_tick'] is not None
            or c['landing']['landed_tick'] is not None
            or m['target'] != p['planet']['index'] or c['site']['planet'] != m['target']
            or p['location'] == 'on_foot' or not p['ship_available'] or p['ship_form'] != 'ship'):
        return None
    if c['goal'] == 'seek_cover':
        return 'circling'
    if c['goal'] == 'approach':
        return 'approach'
    if c['goal'] == 'surface':
        if c['landing']['goal'] in ['approach', 'reposition']:
            return 'alignment'
        if c['landing']['goal'] == 'land':
            if p['landing']['planet'] != m['target']:
                return None
            # A supported foot is contact, not the native landed acknowledgement.
            return 'settling' if p['landing']['supported_feet'] > 0 else 'descent'
    return None


class PhaseClock:
    """Observed contiguous episodes, with gaps and physical restarts retained."""
    def __init__(self):
        self.previous_tick = None
        self.plan = None
        self.generation = 0
        self.had_site = False
        self.current = None
        self.episodes = []
        self.breaks = []

    def observe(self, row):
        tick, p, m = row['tick'], frozen.pilot(row), row['mission']
        c = m.get('capture')
        if self.previous_tick is not None and tick <= self.previous_tick:
            raise ValueError('phase observations must advance')
        contiguous = self.previous_tick is not None and tick == self.previous_tick + 1
        gap = self.previous_tick is not None and not contiguous
        self.previous_tick = tick
        plan = (m['target'], p['ship_form'], c['started_tick'], c['replans'], c['site']) if c else None
        restart = self.had_site and plan != self.plan
        if restart:
            self.generation += 1
        self.plan = plan
        self.had_site |= bool(c and c['site'])
        name = phase_name(row)
        native = None
        if name is not None:
            native = c['goal_since'] if name in ['circling', 'approach'] else c['landing']['goal_since']
            if type(native) is not int or not 0 <= native <= tick:
                raise ValueError('invalid native phase clock')
        context = 'retry' if self.generation or c and c['replans'] else 'initial'
        reason = 'observation_gap' if gap else 'plan_restart' if restart else None
        if reason:
            self.breaks.append({'tick': tick, 'reason': reason})
        old = self.current
        # Unsupported phases are also breaks: do not assume an unobserved
        # controller transition left the same physical approach intact.
        if old and name is None:
            landed = c and c['landing']['landed_tick'] is not None
            reason = reason or ('landed' if landed else 'phase_unavailable')
            if reason == 'phase_unavailable':
                self.breaks.append({'tick': tick, 'reason': reason})
        changed = (reason is not None or old is None or name != old['name']
                   or context != old['context'])
        if changed and old:
            old.update(end_tick=tick, closed_by=reason or 'phase_change')
            self.current = None
        if name is not None and changed:
            # An isolated first sample is not an observed entry unless its
            # native goal clock explicitly starts now. Contact has no such clock.
            exact_entry = (contiguous and old is not None) or (name != 'settling' and native == tick)
            self.current = {'name': name, 'context': context, 'episode': len(self.episodes),
                'plan_generation': self.generation, 'entry_tick': tick, 'entry_observed': exact_entry,
                'last_tick': tick, 'end_tick': None, 'closed_by': None}
            self.episodes.append(self.current)
        if self.current:
            self.current['last_tick'] = tick
            return {k: self.current[k] for k in ['name', 'context', 'episode', 'plan_generation',
                                               'entry_tick', 'entry_observed']} | {
                'age_seconds': (tick - self.current['entry_tick']) / costs.HZ
                    if self.current['entry_observed'] else None}
        return {'name': None, 'context': context, 'episode': None, 'age_seconds': None}


def remaining(profile, phase):
    result = {'seconds': None, 'envelope_seconds': None, 'support': 0,
              'support_episodes': 0, 'support_seeds': 0, 'reason': None,
              'cell': f"{phase['context']}/{phase['name']}"}
    if phase['name'] is None:
        return {**result, 'reason': 'flight_phase_unavailable'}
    age = phase['age_seconds']
    if age is None:
        return {**result, 'reason': 'phase_entry_unobserved'}
    if not costs.finite(age) or age < 0:
        raise ValueError('invalid phase age')
    cell = profile['cells'].get(result['cell']) if profile else None
    if not cell or not cell['samples']:
        return {**result, 'reason': 'no_phase_calibration'}
    # Condition on still being IN this phase, not merely on not having landed.
    # Repeated contact episodes/retries cannot give one attempt extra weight.
    groups, seeds, bounds = {}, set(), []
    for s in cell['samples']:
        if s['phase_seconds'] <= age:
            continue
        a, b = s['landing_seconds_bounds']
        if not 0 <= s['phase_seconds'] <= a <= b:
            raise ValueError('inconsistent historical phase duration')
        span = [max(0.0, a - age), b - age]
        bounds.append(span)
        ident = (s['recording_sha256'], s['seat'], s['selected_tick'])
        groups.setdefault(ident, []).append(sum(span) / 2)
        seeds.add(s['seed'])
    result.update(support=len(groups), support_episodes=len(bounds), support_seeds=len(seeds))
    if len(groups) < MIN_ATTEMPTS:
        return {**result, 'reason': 'insufficient_surviving_phase_attempts'}
    return {**result, 'seconds': statistics.median(statistics.median(v) for v in groups.values()),
            'envelope_seconds': [min(v[0] for v in bounds), max(v[1] for v in bounds)]}


class PhaseTrip(rolling.RollingTrip):
    def __init__(self, selection, ground_profile, phase_profile=None):
        super().__init__(selection, ground_profile)
        self.phase_profile = phase_profile
        self.clock = PhaseClock()
        self.phase = None

    def observe(self, row):
        if self.terminal:
            return None
        self.phase = self.clock.observe(row)
        result = super().observe(row)
        result.update(model=MODEL, flight_phase=self.phase)
        return result

    def landing_estimate(self, row, result):
        old = super().landing_estimate(row, result)
        new = remaining(self.phase_profile, self.phase)
        tail = [self.anchor['phases'][name] for name in frozen.PHASES if name != 'landing']
        ready = not result['unknown_reasons']
        complete = ready and old['seconds'] is not None and all(p['estimate_seconds'] is not None for p in tail)
        result['age_only_total_seconds'] = (result['elapsed_seconds'] + old['seconds']
            + sum(p['estimate_seconds'] for p in tail)) if complete else None
        result['age_only_landing'] = old
        result['age_only_landing_seconds'] = old['seconds'] if ready else None
        result['phase_landing_seconds'] = new['seconds'] if ready else None
        return new

    def record(self):
        return {**super().record(), 'phase_episodes': self.clock.episodes, 'phase_breaks': self.clock.breaks}


def add_samples(cells, run, attempt, trip):
    landed = trip['milestones'].get('landed') if trip else None
    for e in attempt['phase_episodes']:
        cell = cells.setdefault(f"{e['context']}/{e['name']}", {'samples': [], 'excluded': []})
        source = {'label': run['label'], 'seed': run['seed'], **{
            k: attempt['selection'][k] for k in ['seat', 'planet', 'selected_tick']},
            'recording_sha256': run['trace_sha256'],
            'episode': e['episode'], 'entry_tick': e['entry_tick'], 'end_tick': e['end_tick'],
            'ending': trip['ending'] if trip else 'unmatched_attempt'}
        reasons = []
        if not e['entry_observed']:
            reasons.append('phase_entry_unobserved')
        if not landed or landed[0] < e['entry_tick'] or landed[1] >= trip['stopped_tick']:
            reasons.append('landing_not_completed_in_scope')
        elif (landed[1] - landed[0]) / costs.HZ > frozen.MAX_BOUNDARY_WIDTH:
            reasons.append('uncertain_landing_boundary')
        if e['closed_by'] not in ['phase_change', 'landed'] or e['end_tick'] is None:
            reasons.append('phase_not_completed')
        if landed and (e['end_tick'] is not None and e['end_tick'] > landed[0]
                or any(e['entry_tick'] < b['tick'] <= landed[1] for b in attempt['phase_breaks'])):
            reasons.append('interrupted_before_landing')
        if reasons:
            cell['excluded'].append({**source, 'reasons': reasons})
        else:
            cell['samples'].append({**source,
                'phase_seconds': (e['end_tick'] - e['entry_tick']) / costs.HZ,
                'landing_seconds_bounds': [(t - e['entry_tick']) / costs.HZ for t in landed]})


def evaluate_run(run, attempts, updates):
    rows = list(updates)
    by_key = {(frozen.key(r), r['tick']): r for r in rows}
    result = rolling.evaluate_run(run, attempts, rows)
    for a in result['attempts']:
        trip, original = a['actual'], a['original_prediction']
        for c in a['checkpoints']:
            c['phase_total_seconds'] = c.pop('rolling_total_seconds')
            c['phase_error_seconds_bounds'] = c.pop('rolling_error_seconds_bounds')
            u = by_key.get((frozen.key(a['selection']), c['tick']))
            eligible = c['status'] not in ['attempt_ended', 'already_landed', 'missing_observation']
            age_only = u.get('age_only_total_seconds') if eligible and u else None
            c['age_only_total_seconds'] = age_only
            c['flight_phase'] = u.get('flight_phase') if eligible and u else None
            actual = trip['phases']['total']['seconds_bounds']
            c['age_only_error_seconds_bounds'] = ([age_only - actual[1], age_only - actual[0]]
                if trip['ending'] == 'completed' and age_only is not None else None)
            c['landing_errors'] = {name: None for name in ['original', 'age_only', 'phase']}
            c['landing_unknown_reason'] = u.get('landing', {}).get('reason') if u and u.get('landing') else None
            landed = trip['milestones'].get('landed')
            if eligible and u and landed and c['tick'] < landed[0] <= landed[1] < trip['stopped_tick']:
                points = {'original': original['phases']['landing']['estimate_seconds'],
                          'age_only': u.get('age_only_landing_seconds'), 'phase': u.get('phase_landing_seconds')}
                for name, point in points.items():
                    origin = original['source_tick'] if name == 'original' else c['tick']
                    c['landing_errors'][name] = [point + (origin - t) / costs.HZ for t in reversed(landed)] if point is not None else None
    return result


def comparisons(rows):
    names = ['original', 'age_only', 'phase']
    paired = [r for r in rows if all(r[f'{n}_error_seconds_bounds'] is not None for n in names)]
    landed = [r for r in rows if all(r['landing_errors'][n] is not None for n in names)]
    return {'checkpoints': len(rows), 'statuses': dict(frozen.Counter(r['status'] for r in rows)),
        'unknown_reasons': dict(frozen.Counter(x for r in rows for x in r['unknown_reasons'])),
        'paired_completed': len(paired),
        'total_error': {n: frozen.error_summary([r[f'{n}_error_seconds_bounds'] for r in paired]) for n in names},
        'paired_landed': len(landed),
        'landing_error': {n: frozen.error_summary([r['landing_errors'][n] for r in landed]) for n in names},
        'all_phase_completed_error': frozen.error_summary([r['phase_error_seconds_bounds'] for r in rows]),
        'all_phase_landed_error': frozen.error_summary([r['landing_errors']['phase'] for r in rows])}


def summarize(runs):
    result = {}
    for seconds in rolling.CHECKPOINT_SECONDS:
        rows = [c for run in runs for a in run['attempts'] for c in a['checkpoints'] if c['after_seconds'] == seconds]
        result[str(seconds)] = comparisons(rows) | {'by_context': {
            context: comparisons([r for r in rows if r['flight_phase'] and r['flight_phase']['context'] == context])
            for context in ['initial', 'retry']}, 'by_phase': {
            phase: comparisons([r for r in rows if r['flight_phase'] and r['flight_phase']['name'] == phase])
            for phase in ['circling', 'approach', 'alignment', 'descent', 'settling']}}
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['calibrate', 'evaluate'])
    parser.add_argument('--manifest', required=True, type=Path)
    parser.add_argument('--ground-profile', required=True, type=Path)
    parser.add_argument('--phase-profile', type=Path)
    parser.add_argument('--out', required=True, type=Path)
    args = parser.parse_args()
    manifest = frozen.load_manifest(args.manifest)
    base = args.manifest.resolve().parent
    ground = json.loads(args.ground_profile.read_text())
    if ground['version'] != 1 or ground['model'] != frozen.MODEL or ground['runtime_revision'] != manifest['runtime_revision']:
        raise ValueError('incompatible ground profile')
    phase = None
    ground_hash = costs.file_hash(args.ground_profile)
    if args.mode == 'evaluate':
        if not args.phase_profile:
            parser.error('evaluate requires --phase-profile')
        phase = json.loads(args.phase_profile.read_text())
        if (phase['version'] != 1 or phase['model'] != MODEL
                or phase['runtime_revision'] != manifest['runtime_revision']
                or phase['ground_profile_sha256'] != ground_hash or phase['min_attempts'] != MIN_ATTEMPTS):
            raise ValueError('incompatible phase profile')
    args.out.mkdir(parents=True, exist_ok=True)
    forecasts, paths = [], []
    for index, config in enumerate(manifest['runs']):
        path = args.out / f'updates-{index}.jsonl'
        with path.open('w') as stream:
            attempts = rolling.replay(config, base, ground, stream,
                trip_factory=lambda selection, profile: PhaseTrip(selection, profile, phase))
        forecasts.append({'label': config['label'], 'attempts': attempts,
                          'updates_file': path.name, 'updates_sha256': costs.file_hash(path)})
        paths.append(path)
        print(f"{config['label']}: {len(attempts)} phase replays", flush=True)
    provenance = {'version': 1, 'model': MODEL, 'ground_profile_sha256': ground_hash,
        'phase_profile_sha256': costs.file_hash(args.phase_profile) if phase else None,
        'manifest_sha256': costs.file_hash(args.manifest)}
    # All forecasts are closed and hashed before opening any outcome report.
    frozen.write_json(args.out / 'predictions.json', {**provenance, 'runs': forecasts})
    cells, evaluated, sources, seen = {}, [], [], set()
    for config, prediction, path in zip(manifest['runs'], forecasts, paths):
        run = costs.analyze_run(config, base)
        sources.append(frozen.source_summary(run))
        if args.mode == 'calibrate':
            identities = {(run['trace_sha256'], seat) for seat in config['seats']}
            if seen & identities:
                raise ValueError('duplicate training recording and seat')
            seen |= identities
            trips = {frozen.key(t): t for t in run['trips']}
            for attempt in prediction['attempts']:
                add_samples(cells, run, attempt, trips.get(frozen.key(attempt['selection'])))
        else:
            frozen.ensure_independent(ground, run)
            frozen.ensure_independent(phase, run)
        with path.open() as stream:
            evaluated.append(evaluate_run(run, prediction['attempts'], map(json.loads, stream)))
    if args.mode == 'calibrate':
        frozen.write_json(args.out / 'profile.json', {'version': 1, 'model': MODEL,
            'runtime_revision': manifest['runtime_revision'], 'ground_profile_sha256': ground_hash,
            'min_attempts': MIN_ATTEMPTS, 'runs': sources, 'cells': cells,
            'limitations': [
                'Conditional on reaching native landing acknowledgement without a subsequent plan restart.',
                'Initial and retry evidence are separate. No fallback to initial approach durations.',
                'Each eligible attempt has equal weight; repeated phase episodes are median-aggregated within it.',
                'Ground categories are pooled for flight only; original ground-tail domains still apply.',
                'Phase age and contact are features; distance, speed, gravity and damage are not.',
                'Min/max historical envelopes are not confidence intervals, deadlines or physical permissions.',
                'Censored and interrupted episodes remain exclusions, not fabricated completed durations.',
                'No completion probability, remote-route model or strategic selection changes.']})
    frozen.write_json(args.out / 'evaluation.json', {**provenance, 'runs': evaluated, 'summary': summarize(evaluated)})


if __name__ == '__main__':
    main()
