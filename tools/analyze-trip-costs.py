#!/usr/bin/env python3
"""Compare recorded Spacewars trip milestones with the 04253bc ground score.

Read-only, standard-library analysis. Input is a JSON manifest of run directories;
outputs contain timing bounds, censored attempts, model provenance and sampled
ship-threat geometry. No trace interpolation, physics or controller is executed.
"""
import argparse
import bisect
import hashlib
import json
import math
from dataclasses import dataclass, field
from pathlib import Path

HZ = 60
MODEL = {
    'id': 'ground-score-at-04253bc',
    'source': 'scenarios/spacewars/src/surface_sortie/landing_objective.rs:LandingObjectiveRoute::cost',
    'walking_speed': 5.0,
    'flight_score_speed': 38.0,
    'jump_score_units': 15.0,
    'crossing_recharge_seconds': 4.0,
    'scope': 'real-valued interpretation of a retained selected route score; not renewed route validity',
    'omits': ['transfer', 'landing', 'exit', 'claim', 'final boarding', 'departure'],
}
MILESTONES = ['selected', 'arrived', 'landed', 'exited', 'claim_started',
              'claimed', 'boarded', 'departed']
PHASES = [('transfer', 'selected', 'arrived'), ('landing', 'arrived', 'landed'),
          ('exit', 'landed', 'exited'), ('outbound', 'exited', 'claim_started'),
          ('claim', 'claim_started', 'claimed'), ('return_board', 'claimed', 'boarded'),
          ('departure', 'boarded', 'departed'), ('surface', 'arrived', 'departed'),
          ('ground', 'exited', 'boarded'), ('total', 'selected', 'departed')]


def finite(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def point_distance(a, b):
    return math.hypot(a['x'] - b['x'], a['y'] - b['y'])


def crossing_seconds(crossing):
    """Read recorded flight timing, without reimplementing native validity checks."""
    if crossing.get('version') != 2 or len(crossing.get('flights', [])) != 2:
        return None
    times = [f.get('seconds') for f in crossing['flights']]
    plan = crossing.get('plan', {})
    points = [plan.get(k, {}) for k in ['start', 'destination']]
    if not all(finite(p.get(k)) for p in points for k in ['x', 'y']):
        return None
    chord = point_distance(*points)
    if not all(finite(t) and 0 < t <= 12 for t in times) or not 1 < chord < 30:
        return None
    return max(max(times) + 4.0 - chord / 5.0, 0.0)


def leg_seconds(route, crossing=None):
    """Interpret complete recorded routes; incomplete/unsupported models stay unknown."""
    if not route or route['failure'] is not None or route['partial']:
        return None
    length = route['length']
    if not finite(length) or length < 0 or route['start_node'] is None:
        return None
    if route['reachable_nodes'] <= 0 or route['destination_nodes'] <= 0:
        return None
    jumps, flights = route['jumps'], route['flights']
    if (type(jumps) is not int or not 0 <= jumps <= 512
            or type(flights) is not int or flights not in (0, 1)):
        return None
    flight_seconds = crossing_seconds(crossing) if crossing is not None else None
    if crossing is not None and flight_seconds is None:
        return None
    seconds = length / 5.0 + jumps * 15.0 / 38.0
    if flights:
        if flight_seconds is None:
            return None
        seconds += flight_seconds
    return seconds


def selected_estimate(row):
    capture = row['mission'].get('capture')
    if not capture or capture['site'] is None:
        return None
    local = row['observation']['local']
    p = local['combat']['recovery']['flight']['pilot']
    route = capture['objective_route']
    claim = p['planet']['claim']
    has_objective = bool(claim and claim['flag'] and claim['owner'] != p['owner'])
    outbound = returning = source_tick = validated_tick = revision = None
    status = 'no_existing_flag_term' if claim and not has_objective else 'unavailable'
    if has_objective and route and route['site'] == capture['site']:
        outbound = leg_seconds(route['outbound'], route.get('crossing'))
        returning = leg_seconds(route['returning'], route.get('crossing'))
        status = 'selected_route' if outbound is not None and returning is not None else 'unavailable'
        if status == 'unavailable':
            outbound = returning = None
        survey = local.get('landing_objective')
        if survey and route in survey['sites']:
            source_tick, validated_tick = survey['tick'], survey.get('validated_tick')
            revision = survey['objective']['revision']
    full_claim = None
    if claim:
        stages = 0 if claim['owner'] == p['owner'] else (
            2 if claim['flag'] and claim['flag']['player'] != p['owner'] else 1)
        full_claim = stages * claim['stage_required_seconds']
    return {
        'observed_tick': row['tick'], 'source_tick': source_tick,
        'validated_tick': validated_tick, 'site': capture['site'],
        'revision': revision, 'observed_planet_revision': p['planet']['revision'], 'status': status,
        'outbound_seconds': outbound, 'return_seconds': returning,
        'ground_score_seconds': outbound + returning if status == 'selected_route' else (
            0.0 if status == 'no_existing_flag_term' else None),
        'full_uninterrupted_claim_seconds': full_claim,
        'crossing_source_tick': ((route or {}).get('crossing') or {}).get('measured_tick'),
        'route_lengths': {k: route[k]['length'] if route[k] else None
                         for k in ['outbound', 'returning']} if route else None,
    }


@dataclass(slots=True)
class Sample:
    tick: int
    exited: bool
    claiming: bool
    claim_status: str | None
    threat: bool | None
    on_foot: bool
    local_planet: int
    revision: int


def sample(row, planet):
    local, mission = row['observation']['local'], row['mission']
    combat = local['combat']
    p = combat['recovery']['flight']['pilot']
    qualifies = (mission.get('capture') is not None and mission['target'] == planet
                 and p['planet']['index'] == planet)
    on_foot = p['location'] == 'on_foot'
    claim = p['planet']['claim']
    claiming = bool(qualifies and on_foot and claim and claim['claimant'] == p['owner']
                    and claim['phase'] in ['lowering', 'raising'] and claim['progress'] > 0)
    target = combat['target']
    threat = None
    if p['queries_ready'] and p['ship_available'] and p['ship_form'] == 'ship' and target:
        # This query originates at the ship, even when its pilot is outside.
        # It describes geometric proximity, not enemy aim, firing or pilot LOS.
        threat = not target['ground_occluded'] and point_distance(
            p['ship']['position'], target['motion']['position']) < 300.0
    return Sample(row['tick'], qualifies and on_foot, claiming,
                  claim['status'] if qualifies and on_foot and claim else None,
                  threat, on_foot, p['planet']['index'], p['planet']['revision'])


def first_boundary(samples, attribute, earliest, latest):
    lower = earliest
    previous = earliest - 1
    gap = False
    for s in samples:
        if s.tick < earliest:
            continue
        if s.tick > latest:
            break
        # A claim can start and reset (or a pilot exit and reboard) inside a
        # recording gap. Later false samples cannot rule out that first event.
        gap |= s.tick > previous + 1
        if getattr(s, attribute):
            return [lower, s.tick]
        if not gap:
            lower = s.tick + 1
        previous = s.tick
    return None


def exposure(samples, start, stop):
    selected = [s for s in samples if start <= s.tick < stop]
    span = stop - start
    positive = sum(s.threat is True for s in selected)
    negative = sum(s.threat is False for s in selected)
    revisions = sorted({(s.local_planet, s.revision) for s in selected})
    return {
        'sampled_ticks': len(selected), 'interval_ticks': span,
        'missing_ticks': span - len(selected), 'dense': len(selected) == span,
        'ship_proxy_positive_ticks': positive, 'ship_proxy_negative_ticks': negative,
        'ship_proxy_unknown_samples': len(selected) - positive - negative,
        'ship_proxy_seconds_bounds': [positive / HZ, (span - negative) / HZ],
        'on_foot_samples': sum(s.on_foot for s in selected),
        'on_foot_ship_proxy_positive_samples': sum(s.on_foot and s.threat is True for s in selected),
        'pilot_visibility': 'not_measured',
        'claim_status_samples': {status: sum(s.claim_status == status for s in selected)
                                 for status in sorted({s.claim_status for s in selected}
                                                      - {None})},
        'observed_planet_revisions': [list(r) for r in revisions],
    }


@dataclass
class Trip:
    seat: int
    planet: int
    start: int
    stop: int
    ending: str
    reason: str | None
    milestones: dict
    policy: str
    physics_from: int
    discarded: dict = field(default_factory=dict)
    samples: list = field(default_factory=list)
    first_estimate: dict | None = None
    landing_estimate: dict | None = None
    return_estimate: dict | None = None
    estimate_signature: str | None = None
    retained_estimate: dict | None = None

    def observe(self, row):
        if not self.start <= row['tick'] < self.stop:
            return
        s = sample(row, self.planet)
        self.samples.append(s)
        c = row['mission'].get('capture')
        if not c or row['mission']['target'] != self.planet or s.local_planet != self.planet:
            return
        landed = self.milestones.get('landed')
        if landed is None or s.tick <= landed[1]:
            estimate = selected_estimate(row)
            if estimate:
                signature = json.dumps([c['site'], c['objective_route'], estimate['status']], sort_keys=True)
                if signature != self.estimate_signature:
                    self.estimate_signature, self.retained_estimate = signature, estimate
                elif (self.retained_estimate['source_tick'] is None and estimate['source_tick'] is not None
                      and estimate['source_tick'] <= self.retained_estimate['observed_tick']):
                    self.retained_estimate = {**self.retained_estimate,
                        **{k: estimate[k] for k in ['source_tick', 'validated_tick', 'revision']}}
                self.first_estimate = self.first_estimate or self.retained_estimate
                self.landing_estimate = {**self.retained_estimate, 'last_seen_tick': s.tick}
        ground = c.get('ground')
        claimed = self.milestones.get('claimed')
        if (self.return_estimate is None and claimed and s.tick >= claimed[1]
                and ground and ground['destination'] == 'hatch' and ground['route']):
            seconds = leg_seconds(ground['route'])
            if seconds is not None:
                self.return_estimate = {'observed_tick': s.tick, 'seconds': seconds,
                    'length': ground['route']['length'], 'revision': s.revision,
                    'continuous_walk': ground.get('continuous_walk', False)}

    def result(self):
        milestones = dict(self.milestones)
        earliest = milestones.get('landed', [self.start])[0]
        for name, attribute in [('exited', 'exited'), ('claim_started', 'claiming')]:
            if name not in milestones:
                later = [milestones[m][1] for m in MILESTONES[MILESTONES.index(name) + 1:]
                         if m in milestones]
                latest = min(later, default=self.stop - 1)
                begins = max(earliest, milestones.get('exited', [earliest])[0])
                boundary = first_boundary(self.samples, attribute, begins, latest)
                if boundary:
                    milestones[name] = boundary
        phases = {}
        for name, begins, ends in PHASES:
            start, finish = milestones.get(begins), milestones.get(ends)
            if start is None:
                phases[name] = {'status': 'not_observed', 'seconds_bounds': None}
                continue
            if finish is None:
                # A later milestone proves this boundary was missed, not that
                # the earlier phase kept running until the attempt stopped.
                later = [milestones[m] for m in MILESTONES[MILESTONES.index(ends) + 1:]
                         if m in milestones]
                if later:
                    finish, status = [start[0], min(m[1] for m in later)], 'boundary_unobserved'
                else:
                    finish, status = [self.stop, self.stop], 'censored'
            else:
                status = 'completed'
            if finish[1] < start[0]:
                raise ValueError(f'nonmonotonic {name}: {start}, {finish}')
            low, high = max(0, finish[0] - start[1]), finish[1] - start[0]
            phases[name] = {
                'status': status, 'seconds_bounds': [low / HZ, high / HZ],
                'physics_valid': start[0] >= self.physics_from,
                'coverage': exposure(self.samples, start[1], max(start[1], finish[0])),
                'coverage_scope': 'certain interior of boundary bounds; no interpolation',
            }
        comparisons = []
        if self.landing_estimate and 'landed' in milestones:
            for phase, key in [('outbound', 'outbound_seconds'), ('return_board', 'return_seconds'),
                               ('ground', 'ground_score_seconds')]:
                predicted = self.landing_estimate[key]
                comparisons.append(self.compare(phase, predicted, phases[phase], self.landing_estimate,
                                                'latest_retained_before_landing'))
        if self.return_estimate:
            comparisons.append(self.compare('return_board', self.return_estimate['seconds'],
                phases['return_board'], self.return_estimate, 'fresh_return_route'))
        return {'seat': self.seat, 'policy': self.policy, 'planet': self.planet,
                'selected_tick': self.start, 'stopped_tick': self.stop,
                'ending': self.ending, 'reason': self.reason,
                'milestones': milestones, 'discarded_out_of_attempt_milestones': self.discarded,
                'phases': phases, 'first_selected_estimate': self.first_estimate,
                'latest_retained_before_landing': self.landing_estimate if 'landed' in milestones else None,
                'fresh_return_estimate': self.return_estimate, 'comparisons': comparisons}

    def compare(self, phase, predicted, actual, estimate, origin):
        reasons = []
        if predicted is None or predicted <= 0:
            reasons.append('no_positive_time_model')
        if actual['status'] != 'completed':
            reasons.append('unfinished_phase')
        bounds = actual['seconds_bounds']
        if bounds is None or bounds[0] != bounds[1]:
            reasons.append('uncertain_phase_boundary')
        if not actual.get('physics_valid'):
            reasons.append('older_physics_in_phase')
        coverage = actual.get('coverage', {})
        if not coverage.get('dense'):
            reasons.append('sparse_observations')
        if coverage.get('observed_planet_revisions') != [[self.planet, estimate['revision']]]:
            reasons.append('changed_or_unverified_ground')
        # The whole ground score omits claiming; report its residual explicitly,
        # without presenting that ratio as a walking-speed calibration.
        measured = bounds[0] if not reasons else None
        return {'phase': phase, 'estimate_origin': origin, 'predicted_seconds': predicted,
                'measured_seconds': measured, 'excluded_reasons': reasons,
                'ratio': measured / predicted if measured is not None else None,
                'residual_seconds': measured - predicted if measured is not None else None}


def build_trips(report, config):
    if report['version'] != 2:
        raise ValueError('unsupported mission report version')
    physics_from = config['physics_valid_from_tick']
    if type(physics_from) is not int or physics_from < 0:
        raise ValueError('physics_valid_from_tick must be explicit and nonnegative')
    trips = []
    if config.get('scope', 'missions') == 'continuation':
        continuation = report['successor_continuation']
        trial = continuation['trial']
        sortie = trial['sortie']
        seat = continuation['actor']
        start = trial['source_tick']
        stop = trial['stopped_tick'] or report['elapsed_ticks']
        fields = {'selected_tick': start, **sortie}
        fields['arrived_tick'] = sortie['surface_started_tick'] or sortie['arrived_tick']
        trip = Trip(seat, trial['proposal']['approach']['site']['planet'], start, stop,
                    'completed' if sortie['departed_tick'] is not None else 'stopped',
                    trial['stop_reason'], {}, report['missions'][seat]['policy'], physics_from)
        trips.append((trip, fields))
    elif config.get('scope', 'missions') == 'missions':
        for seat in config.get('seats', [0, 1]):
            if seat not in [0, 1]:
                raise ValueError('invalid seat')
            visits = report['metrics'][seat]['visits']
            if any(a['selected_tick'] > b['selected_tick'] for a, b in zip(visits, visits[1:])):
                raise ValueError('visits must be ordered by selection tick')
            for i, visit in enumerate(visits):
                start = visit['selected_tick']
                limits = [(report['elapsed_ticks'], 'match_end')]
                if i + 1 < len(visits):
                    limits.append((visits[i + 1]['selected_tick'], 'reselected'))
                for key, ending in [('departed_tick', 'completed'), ('abandoned_tick', 'abandoned')]:
                    if visit[key] is not None:
                        limits.append((visit[key], ending))
                stop, ending = min(limits, key=lambda x: (x[0], x[1] != 'completed'))
                trip = Trip(seat, visit['planet'], start, stop, ending,
                            visit['reason'] if ending == 'abandoned' else None, {},
                            report['missions'][seat]['policy'], physics_from)
                trips.append((trip, visit))
    else:
        raise ValueError('scope must be missions or continuation')
    for trip, fields in trips:
        if trip.stop < trip.start:
            raise ValueError('attempt stops before selection')
        for name in MILESTONES:
            tick = fields.get(name + '_tick')
            if tick is not None:
                belongs = (trip.start <= tick < trip.stop
                           or name == 'selected' and tick == trip.start
                           or name == 'departed' and trip.ending == 'completed' and tick == trip.stop)
                if belongs:
                    trip.milestones[name] = [tick, tick]
                else:
                    trip.discarded[name] = tick
    return [t for t, _ in trips]


def file_hash(path):
    with path.open('rb') as stream:
        result = hashlib.sha256()
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            result.update(chunk)
        return result.hexdigest()


def analyze_run(config, base):
    directory = (base / config['directory']).resolve()
    report_path, trace_path = directory / 'report.json', directory / 'trace.jsonl'
    report = json.loads(report_path.read_text())
    trips = build_trips(report, config)
    by_seat = [[t for t in trips if t.seat == seat] for seat in [0, 1]]
    starts = [[t.start for t in group] for group in by_seat]
    previous = [-1, -1]
    with trace_path.open() as stream:
        for row in map(json.loads, stream):
            seat, tick = row['seat'], row['tick']
            if row['version'] != 1 or seat not in [0, 1] or tick <= previous[seat]:
                raise ValueError('unsupported, duplicate or out-of-order trace observation')
            p = row['observation']['local']['combat']['recovery']['flight']['pilot']
            if tick != p['tick'] or p['owner'] != f'player_{seat + 1}':
                raise ValueError('trace tick/actor does not match its observation')
            previous[seat] = tick
            index = bisect.bisect_right(starts[seat], tick) - 1
            if index >= 0:
                by_seat[seat][index].observe(row)
    return {'label': config['label'], 'directory': str(directory), 'seed': report['seed'],
            'scope': config.get('scope', 'missions'),
            'source_commit': config['source_commit'], 'physics_valid_from_tick': config['physics_valid_from_tick'],
            'report_sha256': file_hash(report_path), 'trace_sha256': file_hash(trace_path),
            'round': report['round'], 'trips': [trip.result() for trip in trips]}


def markdown(result):
    lines = ['# Recorded trip calibration', '',
             'The model is a ground-score reference, not a whole-trip prediction. '
             'Unfinished attempts and sparse observations remain explicit.', '',
             '| Run / seat / selection | End | Outbound | Claim | Return + board | Departure |',
             '| --- | --- | ---: | ---: | ---: | ---: |']
    for run in result['runs']:
        for trip in run['trips']:
            def duration(phase):
                p = trip['phases'][phase]
                bounds = p['seconds_bounds']
                if bounds is None:
                    return '—'
                text = f'{bounds[0]:.2f}' if bounds[0] == bounds[1] else f'{bounds[0]:.2f}–{bounds[1]:.2f}'
                if p['status'] == 'censored':
                    text += '+'
                elif p['status'] == 'boundary_unobserved':
                    text += ' (boundary missed)'
                if not p['physics_valid']:
                    text += ' (old physics)'
                return text
            lines.append(f"| {run['label']} / P{trip['seat'] + 1} / {trip['selected_tick']} "
                         f"| {trip['ending']} | " + ' | '.join(duration(p) for p in
                         ['outbound', 'claim', 'return_board', 'departure']) + ' |')
    lines += ['', 'Times are seconds. A trailing `+` is observed time in an unfinished phase, '
              'not a completed duration. JSON retains bounds, missing samples, rejected comparisons '
              'and exact source hashes. The threat proxy is ship-centered; pilot visibility is unmeasured.', '']
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    if manifest['version'] != 1 or not manifest['runs']:
        raise ValueError('a version-1 manifest must contain runs')
    labels = [r['label'] for r in manifest['runs']]
    if len(set(labels)) != len(labels):
        raise ValueError('run labels must be unique')
    result = {'version': 1, 'tick_hz': HZ, 'model': MODEL, 'runs': []}
    for config in manifest['runs']:
        run = analyze_run(config, args.manifest.resolve().parent)
        result['runs'].append(run)
        print(f"{run['label']}: {len(run['trips'])} recorded attempts", flush=True)
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / 'calibration.json').write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
    (args.out / 'calibration.md').write_text(markdown(result))


if __name__ == '__main__':
    main()
