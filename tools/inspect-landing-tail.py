#!/usr/bin/env python3
"""Diagnose recorded landing tails and interruptions without fitting an ETA.

Snapshots and transition evidence use only current/past observations. Durations
and final-segment labels are explicitly retrospective. Existing replay lifecycle
and phase clocks are reused and checked against the source evaluation.
"""
import argparse
from collections import Counter
import copy
import importlib.util
import json
import math
import os
from pathlib import Path


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


flight = module('tail_geometry', 'inspect-flight-progress.py')
composed = module('tail_replay', 'compose-trip-estimates.py')
phase, frozen, costs = composed.phase, composed.frozen, composed.costs
MODEL = 'landing-tail-diagnostic-v1'
COUNTERS = (*flight.COUNTERS, 'invalidations')


def finite(value):
    return value if costs.finite(value) else None


def snapshot(row, clock):
    """Site attitude differs from the native radial landing angle."""
    p, c = frozen.pilot(row), row['mission'].get('capture')
    geometry, reason = flight.geometry(row, clock)
    attitude = {'heading_error_radians': None, 'relative_spin': None}
    if geometry is not None:
        site = next(s for s in p['sites'] if s['id'] == c['site'])
        angle, spin = finite(p['ship'].get('angle')), finite(p['ship'].get('spin'))
        if angle is not None:
            up, n = (-math.sin(angle), math.cos(angle)), flight.vector(site['normal'])
            attitude['heading_error_radians'] = math.atan2(up[0] * n[1] - up[1] * n[0], flight.dot(up, n))
        if spin is not None:
            attitude['relative_spin'] = spin - geometry['frame_spin']
    contact = p.get('landing', {})
    feet = contact.get('supported_feet')
    measured = {k: finite(contact.get(k)) for k in ['altitude', 'angle_degrees', 'descent_speed',
        'lateral_speed', 'relative_spin', 'assist_strength', 'settled_seconds']}
    clearances = contact.get('foot_clearances')
    measured.update(phase=contact.get('phase'), planet=contact.get('planet'),
        matches_target=contact.get('planet') == row['mission']['target'] and row['mission']['target'] is not None,
        supported_feet=feet if type(feet) is int and 0 <= feet <= 2 else None,
        foot_clearances=list(clearances) if isinstance(clearances, list) and len(clearances) == 2
            and all(costs.finite(v) for v in clearances) else None)
    # Material rays use 26 as their no-hit value. Preserve it, never treat it
    # as a measured high-altitude distance or evidence of foot contact.
    measured['ray_limit_or_no_hit'] = (any(v >= 26 for v in measured['foot_clearances'])
                                      if measured['foot_clearances'] is not None else None)
    counts = {k: c.get(k, 0) for k in COUNTERS} if c else None
    if counts and any(type(v) is not int or v < 0 for v in counts.values()):
        raise ValueError('invalid native counter')
    landing = c.get('landing', {}) if c else {}
    since = landing.get('last_progress_tick')
    age = ((row['tick'] - since) / costs.HZ if type(since) is int and 0 <= since <= row['tick']
           and landing.get('goal') in ['approach', 'land', 'reposition'] else None)
    return {'tick': row['tick'], 'seat': row['seat'], 'phase': copy.deepcopy(clock),
        'capture_started_tick': c.get('started_tick') if c else None,
        'site': copy.deepcopy(c.get('site')) if c else None, 'revision': p['planet']['revision'],
        'geometry': geometry, 'geometry_unavailable': reason, 'attitude': attitude, 'contact': measured,
        'native_counts': counts, 'landing_goal': landing.get('goal'),
        'landing_retries': landing.get('landing_retries'),
        'touchdown_adjustments': landing.get('touchdown_adjustments'),
        'seconds_since_landing_progress': age,
        'objective_work': row['observation']['local'].get('objective_work'),
        'queries_ready': p.get('queries_ready'), 'physical_permissions': False}


def transition(before, after):
    """Counter evidence can overlap; a site acquisition is not another retry."""
    tags, delta = [], None
    if before is None:
        return {'tags': ['first_observation'], 'counter_delta': None}
    contiguous = after['tick'] == before['tick'] + 1
    same_capture = before['capture_started_tick'] == after['capture_started_tick']
    if not contiguous:
        tags.append('observation_gap')
    if not same_capture:
        tags.append('capture_changed')
    a, b = before['native_counts'], after['native_counts']
    if contiguous and same_capture and a is not None and b is not None:
        delta = {k: b[k] - a[k] for k in COUNTERS}
        if any(v < 0 for v in delta.values()):
            tags.append('counter_reset')
        if delta['replans'] > 0:
            tags.append('native_replan')
        tags.extend(k for k in COUNTERS if k != 'replans' and delta[k] > 0)
    if before['site'] != after['site']:
        tags.append('site_acquired' if before['site'] is None else
                    'site_cleared' if after['site'] is None else 'site_changed')
    if before['revision'] != after['revision']:
        tags.append('revision_changed')
    if before['phase'].get('episode') != after['phase'].get('episode'):
        tags.append('phase_changed')
    for key in ['supported_feet', 'phase']:
        if before['contact'][key] != after['contact'][key]:
            tags.append('contact_' + key + '_changed')
    old, new = before['contact']['settled_seconds'], after['contact']['settled_seconds']
    if old is not None and new is not None and old > 0 and new == 0:
        tags.append('settle_timer_reset')
    if before['touchdown_adjustments'] != after['touchdown_adjustments']:
        tags.append('touchdown_adjustments_changed')
    return {'tags': tags, 'counter_delta': delta}


class LandingTrace(phase.PhaseTrip):
    def __init__(self, selection, _profile=None):
        # Empty timing cells: reuse lifecycle/clock code without a fitted ETA.
        super().__init__(selection, {'cells': {}})
        self.samples, self.events = [], []
        self.previous = None
        self.observations = 0
        self.contact_ticks = Counter()

    def emit(self, sample):
        if sample is not None and (not self.samples or self.samples[-1]['tick'] != sample['tick']):
            self.samples.append(sample)

    def observe(self, row):
        result = super().observe(row)
        if result is None:
            return None
        current = snapshot(row, result['flight_phase'])
        event = transition(self.previous, current)
        if self.previous and current['tick'] == self.previous['tick'] + 1:
            p = self.previous
            if p['phase']['name'] is not None:
                feet = p['contact']['supported_feet'] if p['contact']['matches_target'] else None
                self.contact_ticks[f"{p['phase']['episode']}/{feet}"] += 1
        if event['tags']:
            self.emit(self.previous)
            self.events.append({'tick': current['tick'], **event})
        if event['tags'] or self.first_choice_tick is not None and (row['tick'] - self.first_choice_tick) % costs.HZ == 0:
            self.emit(current)
        self.previous = current
        self.observations += 1
        return result

    def record(self):
        self.emit(self.previous)
        return {'selection': self.selection, 'first_choice_tick': self.first_choice_tick,
            'phase_episodes': self.clock.episodes, 'phase_breaks': self.clock.breaks,
            'samples': self.samples, 'events': self.events, 'observations': self.observations,
            'contact_ticks': dict(self.contact_ticks)}


def partition(episodes, start, end):
    """Account observed intervals exactly; gaps are not invented phase time."""
    if type(start) is not int or type(end) is not int or end < start:
        raise ValueError('invalid interval')
    totals, covered, previous_end = Counter(), 0, start
    for e in episodes:
        stop = min(e['end_tick'] if e['end_tick'] is not None else e['last_tick'] + 1, e['last_tick'] + 1)
        a, b = max(start, e['entry_tick']), min(end, stop)
        if b <= a:
            continue
        if a < previous_end:
            raise ValueError('overlapping phase episodes')
        totals[e['name']] += (b - a) / costs.HZ
        covered += b - a
        previous_end = b
    totals['unclassified_or_unobserved'] = (end - start - covered) / costs.HZ
    return dict(totals)


def duration(attempt, trace, start):
    actual = attempt['actual']
    landed = actual['milestones'].get('landed')
    complete = bool(landed and landed[0] == landed[1] and start <= landed[0] < actual['stopped_tick'])
    last = trace['samples'][-1]['tick'] if trace['samples'] else start
    end = landed[0] if complete else min(actual['stopped_tick'], last + 1)
    if end < start:
        return None
    breaks = [b for b in trace['phase_breaks'] if start < b['tick'] <= end]
    final_start = breaks[-1]['tick'] if breaks else start
    events = [e for e in trace['events'] if start < e['tick'] <= end]
    counts = Counter()
    for e in events:
        if e['counter_delta'] and 'counter_reset' not in e['tags']:
            counts.update({k: v for k, v in e['counter_delta'].items() if v > 0})
    return {'start_tick': start, 'end_tick': end, 'landed_exactly': complete,
        'landing_boundary': landed, 'ending': actual['ending'], 'observed_seconds': (end - start) / costs.HZ,
        'phase_seconds': partition(trace['phase_episodes'], start, end),
        'breaks': breaks, 'native_counter_increments': dict(counts),
        'time_before_final_break_seconds': (final_start - start) / costs.HZ,
        'final_segment_seconds': (end - final_start) / costs.HZ,
        'final_segment_phase_seconds': partition(trace['phase_episodes'], final_start, end),
        'interpretation': 'Retrospective elapsed partition; pre-break time is not counterfactual avoidable cost.'}


def join(attempt, trace):
    for name in ['selection', 'phase_episodes', 'phase_breaks']:
        if attempt[name] != trace[name]:
            raise ValueError('replayed lifecycle differs from source evaluation: ' + name)
    samples = {r['tick']: r for r in trace['samples']}
    handoffs = []
    for e in trace['phase_episodes']:
        if e['name'] != 'approach' or e['closed_by'] != 'phase_change' or e['end_tick'] is None:
            continue
        d = duration(attempt, trace, e['end_tick'])
        if d is not None:
            handoffs.append({'episode': e['episode'], 'context': e['context'], 'entry_observed': e['entry_observed'],
                'handoff': samples.get(e['end_tick']), 'duration': d})
    first = trace['first_choice_tick']
    return {**trace, 'actual_ending': attempt['actual']['ending'],
        'from_first_choice': duration(attempt, trace, first) if first is not None else None,
        'handoffs': handoffs}


def verify_evaluation(config, evaluation, revision, manifest_hash):
    runtime = evaluation.get('runtime_revision')
    # The original phase-calibration schema records runtime per run only.
    if runtime is None and evaluation['model'] == phase.MODEL:
        runtime = config['runtime_revision']
    if (config['runtime_revision'] != revision or runtime != revision
            or evaluation['manifest_sha256'] != manifest_hash
            or len(config['runs']) != len(evaluation['runs'])
            or any(r['source_commit'] != revision for r in evaluation['runs'])):
        raise ValueError('unbound evaluation manifest or incompatible runtime')


def inspect(manifest_path, out):
    manifest, base = json.loads(manifest_path.read_text()), manifest_path.resolve().parent
    if manifest['version'] != 1 or not manifest['datasets']:
        raise ValueError('invalid diagnostic manifest')
    out.mkdir(parents=True, exist_ok=True)
    sources, runs, seen, labels = [], [], set(), set()
    for dataset in manifest['datasets']:
        name = dataset['name']
        if name in labels:
            raise ValueError('duplicate dataset label')
        labels.add(name)
        inputs, evaluation_path = base / dataset['manifest'], base / dataset['evaluation']
        config, evaluation = json.loads(inputs.read_text()), json.loads(evaluation_path.read_text())
        inputs_hash, evaluation_hash = costs.file_hash(inputs), costs.file_hash(evaluation_path)
        verify_evaluation(config, evaluation, manifest['runtime_revision'], inputs_hash)
        sources.append({'name': name, 'manifest_sha256': inputs_hash, 'evaluation_sha256': evaluation_hash})
        for job, source in zip(config['runs'], evaluation['runs']):
            folder = inputs.resolve().parent / job['directory']
            trace_path = folder / 'trace.jsonl'
            trace_hash = costs.file_hash(trace_path)
            if (job['label'] != source['label'] or trace_hash != source['trace_sha256']
                    or costs.file_hash(folder / 'report.json') != source['report_sha256']
                    or source['source_commit'] != manifest['runtime_revision']):
                raise ValueError('source recording or report changed')
            seats = job.get('seats', [job.get('seat')])
            for seat in seats:
                if (trace_hash, seat) in seen and source['attempts']:
                    raise ValueError('duplicate recording and seat')
                seen.add((trace_hash, seat))
            with open(os.devnull, 'w') as sink:
                if config.get('scope') == composed.controlled.SCOPE:
                    records = composed.replay_controlled(job, inputs.resolve().parent, LandingTrace, sink, trace_hash)
                else:
                    records = composed.rolling.replay(job, inputs.resolve().parent, None, sink, trip_factory=LandingTrace)
            if costs.file_hash(trace_path) != trace_hash or len(records) != len(source['attempts']):
                raise ValueError('trace changed or attempt count differs')
            result = {'dataset': name, 'label': source['label'], 'seed': source['seed'],
                'trace_sha256': trace_hash, 'report_sha256': source['report_sha256'],
                'trial_outcome': source.get('trial_outcome'),
                'attempts': [join(a, t) for a, t in zip(source['attempts'], records)]}
            path = out / f'run-{len(runs)}.json'
            frozen.write_json(path, result)
            runs.append({k: v for k, v in result.items() if k != 'attempts'} | {
                'path': path.name, 'sha256': costs.file_hash(path), 'attempts': len(records)})
            print(name, job['label'], len(records), flush=True)
        if costs.file_hash(inputs) != inputs_hash or costs.file_hash(evaluation_path) != evaluation_hash:
            raise ValueError('evaluation changed during inspection')
    frozen.write_json(out / 'index.json', {'version': 1, 'model': MODEL,
        'manifest_sha256': costs.file_hash(manifest_path), 'runtime_revision': manifest['runtime_revision'],
        'sources': sources, 'runs': runs, 'limitations': [
            'Known-recording diagnosis, not new validation or calibration.',
            'Snapshots use current/past evidence; duration and final-segment labels use outcomes.',
            'Elapsed time before a final break is not proven avoidable delay.',
            'Counter tags can overlap; site acquisition is distinct from a native replan.',
            'Missing geometry and uncertain/censored landing boundaries remain explicit.',
            'No control changes, ETA, success probability or landing permissions.']})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    inspect(args.manifest, args.out)


if __name__ == '__main__':
    main()
