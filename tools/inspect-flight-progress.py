#!/usr/bin/env python3
"""Extract causal landing geometry and progress from existing flight recordings.

This is a diagnostic, not an ETA, stall classifier or landing permission. It
reads no outcome report or fitted profile. Normal and controlled traces retain
their separate scopes; sampled output always follows dense history processing.
"""
import argparse
from collections import deque
import hashlib
import importlib.util
import json
import math
from pathlib import Path

SPEC = importlib.util.spec_from_file_location('landing_phases',
    Path(__file__).with_name('estimate-landing-phases.py'))
phases = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(phases)
SPEC = importlib.util.spec_from_file_location('controlled_ground',
    Path(__file__).with_name('evaluate-controlled-ground.py'))
controlled = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(controlled)
frozen, costs = phases.frozen, phases.costs
MODEL = 'flight-progress-diagnostic-v1'
COUNTERS = ('replans', 'cover_replans', 'solar_replans', 'live_invalidations',
            'objective_replans', 'circling_replans')


def vector(value):
    if not isinstance(value, dict) or not all(costs.finite(value.get(k)) for k in ('x', 'y')):
        raise ValueError('missing or invalid vector')
    return value['x'], value['y']


def subtract(a, b):
    return a[0] - b[0], a[1] - b[1]


def dot(a, b):
    return a[0] * b[0] + a[1] * b[1]


def geometry(row, phase):
    """Positive height is above the suggested ship origin, never foot clearance.

    The right vector is (normal.y, -normal.x), as in tactical_sortie.rs.
    Subtracting frame velocity at the SHIP removes rotation as well as orbital
    translation. Merely subtracting velocity at the site would retain spin drift.
    """
    p, c = frozen.pilot(row), row['mission'].get('capture')
    if phase['name'] is None:
        return None, 'flight_phase_unavailable'
    if not p.get('queries_ready'):
        return None, 'queries_not_ready'
    sites = [s for s in p.get('sites', []) if s['id'] == c['site']]
    if len(sites) != 1:
        return None, 'selected_site_unavailable_or_ambiguous'
    site = sites[0]
    if site.get('revision') != p['planet']['revision']:
        return None, 'stale_site_revision'
    try:
        position = vector(p['ship'].get('position'))
        velocity = vector(p['ship'].get('velocity'))
        motion = p['planet']['motion']
        center, translation = vector(motion.get('position')), vector(motion.get('velocity'))
        target, normal = vector(site.get('vehicle_position')), vector(site.get('normal'))
        spin = motion.get('spin')
        if not costs.finite(spin) or abs(math.hypot(*normal) - 1) > 0.001:
            raise ValueError('invalid frame spin or surface normal')
    except (KeyError, ValueError):
        return None, 'invalid_or_missing_geometry_or_velocity'
    offset, delta = subtract(position, center), subtract(position, target)
    frame_velocity = (translation[0] - spin * offset[1], translation[1] + spin * offset[0])
    relative = subtract(velocity, frame_velocity)
    right, distance = (normal[1], -normal[0]), math.hypot(*delta)
    result = {'distance': distance, 'height': dot(delta, normal),
        'side_error': -dot(delta, right), 'relative_speed': math.hypot(*relative),
        'normal_speed': dot(relative, normal), 'right_speed': dot(relative, right),
        'closing_speed': -dot(delta, relative) / distance if distance > 0 else None,
        'frame_spin': spin}
    gravity = p.get('gravity')
    try:
        g = vector(gravity)
        result.update(gravity_normal=dot(g, normal), gravity_right=dot(g, right))
    except ValueError:
        result.update(gravity_normal=None, gravity_right=None)
    return result, None


class FlightProgress:
    """A bounded one-second history, reset across missing or changed evidence."""
    def __init__(self):
        self.tick = None
        self.attempt = None
        self.clock = phases.PhaseClock()
        self.history = deque(maxlen=costs.HZ + 1)
        self.dependency = None

    def observe(self, row):
        tick, p, mission = row['tick'], frozen.pilot(row), row['mission']
        if (type(tick) is not int or tick < 0 or p['tick'] != tick
                or p['owner'] != f"player_{row['seat'] + 1}"):
            raise ValueError('invalid flight observation identity')
        if self.tick is not None and tick <= self.tick:
            raise ValueError('flight observations must advance within a seat')
        events = mission.get('events', [])
        if any(type(e['tick']) is not int or not 0 <= e['tick'] <= tick for e in events):
            raise ValueError('invalid or future mission event')
        selections = [e for e in events if e['kind'] == 'selected']
        selected = selections[-1] if selections and selections[-1]['planet'] == mission['target'] else None
        c = mission.get('capture')
        start = c.get('started_tick') if c else None
        if start is not None and (type(start) is not int or not 0 <= start <= tick):
            raise ValueError('invalid capture clock')
        attempt = (row.get('scope', 'normal_mission'), row['seat'], mission['target'],
                   selected['tick'] if selected else None, start)
        if attempt != self.attempt:
            self.clock = phases.PhaseClock()
        phase = self.clock.observe(row)
        values, reason = geometry(row, phase)
        dependency = (attempt, phase.get('episode'), p['planet']['revision'])
        reset = ('attempt_changed' if attempt != self.attempt else
                 'observation_gap' if self.tick is not None and tick != self.tick + 1 else
                 'phase_or_revision_changed' if dependency != self.dependency else
                 'geometry_unavailable' if values is None else None)
        if reset:
            self.history.clear()
        self.attempt, self.tick, self.dependency = attempt, tick, dependency
        progress = None
        if values is not None:
            self.history.append((tick, values))
            first_tick, first = self.history[0]
            if tick - first_tick == costs.HZ:
                progress = {'from_tick': first_tick, 'seconds': 1,
                    'distance_closed': first['distance'] - values['distance'],
                    'height_reduced': first['height'] - values['height'],
                    'absolute_side_error_reduced': abs(first['side_error']) - abs(values['side_error'])}
        counts = {k: c.get(k, 0) for k in COUNTERS} if c else None
        if counts and any(type(v) is not int or v < 0 for v in counts.values()):
            raise ValueError('invalid retry counters')
        return {'version': 1, 'model': MODEL, 'scope': attempt[0], 'seat': row['seat'],
            'tick': tick, 'selected_tick': attempt[3], 'capture_started_tick': start,
            'planet': mission['target'], 'site': c.get('site') if c else None,
            'revision': p['planet']['revision'], 'phase': phase,
            'geometry': values, 'unavailable_reason': reason, 'one_second_progress': progress,
            'history_reset': reset, 'native_counts': counts,
            'physical_permissions': False}


def signature(record):
    return (record['selected_tick'], record['capture_started_tick'], record['planet'],
            record['phase'].get('episode'), record['revision'], record['unavailable_reason'],
            record['native_counts'], record['site'])


def replay(path, seat, output, every_ticks=costs.HZ):
    if every_ticks < 1:
        raise ValueError('sampling interval must be positive')
    state, digest = FlightProgress(), hashlib.sha256()
    previous, written, origin, observations, samples = None, None, None, 0, 0

    def emit(record):
        nonlocal written, samples
        if record is not None and record['tick'] != written:
            output.write(json.dumps(record, allow_nan=False) + '\n')
            written, samples = record['tick'], samples + 1

    with path.open('rb') as stream:
        for line in stream:
            digest.update(line)
            raw = json.loads(line)
            if raw['seat'] != seat:
                continue
            row = controlled.adapt(raw) if raw.get('scope') == controlled.SCOPE else raw
            if row['version'] != 1 or row['mission'].get('policy') != frozen.POLICY:
                raise ValueError('unsupported flight trace version or policy')
            record = state.observe(row)
            observations += 1
            changed = previous is None or signature(record) != signature(previous)
            if origin is None or (previous and (record['selected_tick'], record['capture_started_tick'])
                                 != (previous['selected_tick'], previous['capture_started_tick'])):
                origin = None
            if origin is None and record['phase']['name'] is not None:
                origin = record['tick']
            if changed:
                emit(previous)
            if changed or origin is not None and (record['tick'] - origin) % every_ticks == 0:
                emit(record)
            previous = record
    emit(previous)
    return {'version': 1, 'model': MODEL, 'trace': str(path), 'trace_sha256': digest.hexdigest(),
        'seat': seat, 'observations': observations, 'samples': samples, 'every_ticks': every_ticks,
        'limitations': ['Current and past observations only; no outcome, ETA or success probability.',
            'Progress windows require contiguous valid geometry for the same attempt, phase and revision.',
            'Site height is relative to the suggested ship origin, not measured foot clearance.',
            'One-second progress is diagnostic, not the controller\'s private progress clock.']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--trace', required=True, type=Path)
    parser.add_argument('--seat', required=True, type=int, choices=[0, 1])
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--every-ticks', type=int, default=costs.HZ)
    args = parser.parse_args()
    if args.every_ticks < 1:
        parser.error('--every-ticks must be positive')
    if args.out.resolve() == args.trace.resolve():
        parser.error('output must not overwrite the trace')
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open('w') as output:
        metadata = replay(args.trace, args.seat, output, args.every_ticks)
    metadata['output_sha256'] = costs.file_hash(args.out)
    frozen.write_json(args.out.with_suffix('.metadata.json'), metadata)
    print(f"{metadata['observations']} observations; {metadata['samples']} diagnostic samples")


if __name__ == '__main__':
    main()
