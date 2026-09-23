#!/usr/bin/env python3
"""Audit recorded time before a first landing choice, without changing controls.

Arrival comes from the bound mission lifecycle. The dense observation ledger
keeps absent/deferred scans separate from measured negative routes. Its states
describe available evidence, not reconstructed controller branch decisions.
"""
import argparse
from collections import Counter, defaultdict
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import statistics

SPEC = importlib.util.spec_from_file_location('acquisition_risk',
    Path(__file__).with_name('inspect-landing-risk.py'))
risk = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(risk)
costs = risk.tail.costs
MODEL = 'site-acquisition-diagnostic-v1'


def adapt(row, scope):
    if scope == 'controlled':
        return risk.tail.composed.controlled.adapt(row)
    if scope != 'normal' or row.get('scope') == risk.tail.composed.controlled.SCOPE:
        raise ValueError('recording scope mismatch')
    return row


def variant(value):
    if isinstance(value, str):
        return value
    if isinstance(value, dict) and len(value) == 1:
        return next(iter(value))
    return 'unknown'


def route_status(route):
    """Recorded leg evidence only; do not renew native route permissions."""
    if route.get('crossing') is not None:
        return 'powered_unclassified'
    for name in ('outbound', 'returning'):
        leg = route.get(name)
        if leg is None:
            return name + '_absent'
        if leg.get('failure') is not None:
            return name + '_' + leg['failure']
        if costs.leg_seconds(leg) is None:
            return name + '_incomplete'
    return 'complete_recorded_round_trip'


def snapshot(row, target):
    p = risk.tail.frozen.pilot(row)
    if p['tick'] != row['tick'] or p['owner'] != f"player_{row['seat'] + 1}":
        raise ValueError('actor or observation clock mismatch')
    local, mission = row['observation']['local'], row['mission']
    capture = mission.get('capture') if mission['target'] == target else None
    if capture and any(type(capture.get(k, 0)) is not int or capture.get(k, 0) < 0
                       for k in risk.tail.COUNTERS):
        raise ValueError('invalid native counter')
    claim = p['planet']['claim']
    required = bool(claim and claim['flag'] and claim['owner'] != p['owner'])
    survey = local.get('landing_objective')
    routes = survey['sites'] if survey else []
    statuses = Counter(route_status(r) for r in routes)
    complete = [r for r in routes if route_status(r) == 'complete_recorded_round_trip']
    current = bool(survey and (survey['tick'] == row['tick'] or
        survey.get('validated_tick') == row['tick'] and 0 <= row['tick'] - survey['tick'] <= 120))
    offset = {k: p['ship']['position'][k] - p['planet']['motion']['position'][k] for k in ('x', 'y')}
    distance = math.hypot(offset['x'], offset['y'])
    relative = {k: p['ship']['velocity'][k] - p['planet']['motion']['velocity'][k] for k in ('x', 'y')}
    return {'tick': row['tick'], 'seat': row['seat'], 'mission_goal': mission.get('goal'),
        'target_local': p['planet']['index'] == target, 'local_planet': p['planet']['index'],
        'revision': p['planet']['revision'], 'controls_armed': p['controls_armed'],
        'ship_available': p['ship_available'], 'ship_form': p['ship_form'],
        'location': variant(p['location']), 'queries_ready': p['queries_ready'],
        'site_query': variant(p['site_query']), 'site_count': len(p['sites']),
        'objective_required': required, 'objective_work': local.get('objective_work'),
        'survey_tick': survey['tick'] if survey else None, 'survey_age_current': current,
        'validated_routes_only': survey.get('validated_routes_only', False) if survey else None,
        'route_status_counts': dict(statuses), 'complete_routes': len(complete),
        'matching_complete_routes': sum(any(s['id'] == r['site'] for s in p['sites']) for r in complete),
        'capture_present': capture is not None,
        'capture_started_tick': capture.get('started_tick') if capture else None,
        'capture_goal': capture['goal'] if capture else None,
        'capture_failed_tick': capture.get('failed_tick') if capture else None,
        'chosen_site': capture['site'] if capture else None,
        'native_counts': {k: capture.get(k, 0) for k in risk.tail.COUNTERS} if capture else None,
        'radial_altitude': distance - p['planet']['radius'],
        'radial_speed': sum(offset[k] * relative[k] for k in ('x', 'y')) / distance if distance else None,
        'relative_speed': math.hypot(relative['x'], relative['y']),
        'controls': row.get('controls'), 'physical_permissions': False}


def state(sample, arrived):
    """A mutually exclusive observation partition, not causal attribution."""
    if not arrived:
        return 'before_recorded_arrival'
    if not sample['capture_present']:
        return 'capture_absent'
    if not sample['target_local']:
        return 'outside_target_frame'
    if not sample['ship_available'] or sample['ship_form'] != 'ship' or sample['location'] != 'aboard':
        return 'ship_or_pilot_unavailable'
    if not sample['controls_armed']:
        return 'controls_unarmed'
    if not sample['queries_ready']:
        return 'queries_unavailable'
    if sample['capture_failed_tick'] is not None:
        return 'controller_failed'
    if sample['objective_required'] and sample['objective_work'] == 'stale':
        return 'objective_stale'
    if sample['site_query'] in ('deferred', 'not_requested', 'unknown'):
        return 'scan_' + sample['site_query']
    if sample['site_count'] == 0:
        return 'measured_scan_empty'
    if not sample['objective_required']:
        return 'candidates_without_flag_requirement'
    if not sample['survey_age_current']:
        return 'candidates_without_current_routes'
    if sample['matching_complete_routes']:
        return 'candidates_with_recorded_round_trip'
    if sample['route_status_counts'].get('powered_unclassified'):
        return 'powered_routes_unclassified'
    if sample['complete_routes']:
        return 'routes_without_matching_current_site'
    return 'candidates_without_complete_round_trip'


class Attempt:
    def __init__(self, source, trace):
        self.actual, self.selection = source['actual'], source['selection']
        self.start, self.stop = self.actual['selected_tick'], self.actual['stopped_tick']
        self.choice = trace['first_choice_tick']
        if self.choice is not None and not self.start <= self.choice < self.stop:
            raise ValueError('choice outside attempt')
        boundary = self.actual['milestones'].get('arrived')
        self.arrival = boundary[0] if boundary and boundary[0] == boundary[1] else None
        self.arrival_status = 'exact' if self.arrival is not None else 'uncertain' if boundary else 'not_recorded'
        if self.arrival is not None and not self.start <= self.arrival < self.stop:
            raise ValueError('arrival outside attempt')
        if self.choice is not None and self.arrival is not None and self.choice < self.arrival:
            raise ValueError('choice precedes recorded arrival')
        self.end = self.choice if self.choice is not None else self.stop
        self.counts, self.local_counts, self.increments = Counter(), defaultdict(Counter), Counter()
        self.previous = self.first = self.last = self.endpoint = self.stop_sample = None
        self.observed = self.local_observed = 0
        self.first_evidence, self.events = {}, []
        self.unknown_counter_intervals = 0

    def observe(self, sample):
        tick = sample['tick']
        if tick == self.stop:
            self.stop_sample = sample
            return
        if not self.start <= tick <= self.end:
            return
        if self.previous is not None and tick <= self.previous['tick']:
            raise ValueError('duplicate or unordered attempt observation')
        if sample['seat'] != self.selection['seat']:
            raise ValueError('wrong attempt seat')
        if sample['chosen_site'] is not None and tick != self.choice:
            raise ValueError('first choice differs from bound dense replay')
        if tick == self.choice:
            if sample['chosen_site'] is None:
                raise ValueError('bound choice absent from raw observation')
            self.endpoint = sample
            return
        self.first = self.first or sample
        self.last = sample
        self.observed += 1
        arrived = self.arrival is not None and tick >= self.arrival
        sample['evidence_state'] = state(sample, arrived)
        self.counts[sample['evidence_state']] += 1
        if arrived:
            self.local_observed += 1
            for key in ('site_query', 'objective_work', 'objective_required', 'queries_ready'):
                self.local_counts[key][str(sample[key])] += 1
            self.local_counts['route_status_observations'].update(sample['route_status_counts'])
            for name, present in {
                'current_candidates': sample['site_count'] > 0,
                'age_current_survey': sample['survey_age_current'],
                'recorded_complete_routes': sample['complete_routes'] > 0,
                'matching_recorded_routes': sample['matching_complete_routes'] > 0,
                'stale': sample['objective_work'] == 'stale',
            }.items():
                if present:
                    self.first_evidence.setdefault(name, tick)
            old = self.previous
            if old and old['capture_present'] and sample['capture_present']:
                a, b = old['native_counts'], sample['native_counts']
                delta = {k: b[k] - a[k] for k in risk.tail.COUNTERS}
                if (tick != old['tick'] + 1 or old['capture_started_tick'] != sample['capture_started_tick']
                        or any(v < 0 for v in delta.values())):
                    self.unknown_counter_intervals += 1
                elif any(delta.values()):
                    self.increments.update(delta)
                    self.events.append({'tick': tick, 'delta': delta,
                        'objective_work': sample['objective_work'], 'site_query': sample['site_query'],
                        'had_chosen_site': old['chosen_site'] is not None})
        self.previous = sample

    def result(self):
        if self.choice is not None and self.endpoint is None:
            raise ValueError('choice tick missing from raw recording')
        ticks = self.end - self.start
        local_ticks = self.end - self.arrival if self.arrival is not None else None
        return {'selection': self.selection, 'ending': self.actual['ending'], 'reason': self.actual['reason'],
            'stopped_tick': self.stop, 'first_choice_tick': self.choice,
            'arrival_status': self.arrival_status, 'arrival_tick': self.arrival,
            'cohort': ('chosen' if self.choice is not None else 'no_choice') + '/' + self.arrival_status,
            'prechoice_seconds': ticks / costs.HZ,
            'local_prechoice_seconds': local_ticks / costs.HZ if local_ticks is not None else None,
            'observed_ticks': self.observed, 'missing_ticks': ticks - self.observed,
            'local_observed_ticks': self.local_observed,
            'local_missing_ticks': local_ticks - self.local_observed if local_ticks is not None else None,
            'evidence_state_ticks': dict(self.counts), 'local_counts': dict(self.local_counts),
            'native_counter_increments': dict(self.increments),
            'unknown_counter_intervals': self.unknown_counter_intervals,
            'first_evidence': self.first_evidence, 'counter_events': self.events,
            'first': self.first, 'last': self.last, 'choice': self.endpoint, 'stop': self.stop_sample}


def aggregate(attempts):
    cohorts = defaultdict(list)
    for a in attempts:
        cohorts[a['cohort']].append(a)
    result = {}
    for key, rows in sorted(cohorts.items()):
        local = [a['local_prechoice_seconds'] for a in rows if a['local_prechoice_seconds'] is not None]
        states, counts = Counter(), Counter()
        for a in rows:
            states.update(a['evidence_state_ticks'])
            counts.update(a['native_counter_increments'])
        result[key] = {'attempts': len(rows), 'endings': dict(Counter(a['ending'] for a in rows)),
            'reasons': dict(Counter(str(a['reason']) for a in rows)),
            'prechoice_seconds': sum(a['prechoice_seconds'] for a in rows),
            'local_seconds': sum(local), 'local_seconds_median': statistics.median(local) if local else None,
            'local_seconds_max': max(local) if local else None,
            'missing_ticks': sum(a['missing_ticks'] for a in rows),
            'evidence_state_ticks': dict(states), 'native_counter_increments': dict(counts)}
    return result


def inspect(manifest_path, out):
    manifest, manifest_hash = risk.read_bound(manifest_path)
    out.mkdir(parents=True, exist_ok=False)
    sources, results = [], []
    for number, run in enumerate(risk.load_runs(manifest, manifest_path.resolve().parent)):
        if results and run['scope'] != results[0]['scope']:
            raise ValueError('analyze normal and controlled scopes separately')
        report = Path(run['provenance']['report'])
        raw = report.with_name('trace.jsonl')
        attempts = [Attempt(a, t) for a, t in zip(run['source_attempts'], run['attempts'])]
        active, positions = defaultdict(list), defaultdict(int)
        for a in attempts:
            active[a.selection['seat']].append(a)
        for items in active.values():
            if any(a.stop > b.start for a, b in zip(items, items[1:])):
                raise ValueError('overlapping attempt intervals')
        digest, clocks = hashlib.sha256(), {}
        ledger = out / f'prechoice-{number}.jsonl'
        with ledger.open('w') as output, raw.open('rb') as source:
            for line in source:
                digest.update(line)
                row = adapt(json.loads(line), run['scope'])
                tick, seat = row['tick'], row['seat']
                if type(tick) is not int or tick <= clocks.get(seat, -1):
                    raise ValueError('invalid raw recording order')
                clocks[seat] = tick
                items = active[seat]
                while positions[seat] < len(items) and tick > items[positions[seat]].stop:
                    positions[seat] += 1
                pos = positions[seat]
                # Adjacent attempts can share a terminal/selection observation.
                for a in items[pos:pos + 2]:
                    if a.start <= tick <= a.end or tick == a.stop:
                        sample = snapshot(row, a.selection['planet'])
                        a.observe(sample)
                        output.write(json.dumps({'selected_tick': a.start, **sample}, separators=(',', ':')) + '\n')
        # The per-run derived provenance carries the exact raw hash, even for
        # controlled setup failures with no attempts.
        derived, _ = risk.read_bound(Path(run['provenance']['path']), run['provenance']['sha256'])
        if digest.hexdigest() != derived['trace_sha256']:
            raise ValueError('raw recording hash differs from bound replay')
        record = {k: run[k] for k in ('dataset', 'scope', 'label', 'seed',
            'asteroid_interval_seconds', 'trial_outcome', 'unobserved_attempts')}
        record.update(attempts=[a.result() for a in attempts], ledger=ledger.name,
            ledger_sha256=costs.file_hash(ledger), trace_sha256=digest.hexdigest())
        results.append(record)
        sources.append({**run['provenance'], 'trace': str(raw), 'trace_sha256': digest.hexdigest()})
        print(run['label'], len(attempts), 'attempts', flush=True)
    summary = {'model': MODEL, 'runtime_revision': manifest['runtime_revision'],
        'scope': results[0]['scope'] if results else None,
        'manifest_sha256': manifest_hash, 'runs': len(results),
        'worlds': len({r['seed'] for r in results}),
        'attempts': sum(len(r['attempts']) for r in results),
        'unobserved_attempts': sum(len(r['unobserved_attempts']) for r in results),
        'trial_outcomes': dict(Counter(str(r['trial_outcome']) for r in results)),
        'cohorts': aggregate([a for r in results for a in r['attempts']]),
        'by_world': {str(seed): aggregate([a for r in results if r['seed'] == seed for a in r['attempts']])
                     for seed in sorted({r['seed'] for r in results})},
        'limitations': ['Known recordings; no new simulations or causal behavior comparison.',
            'Mission arrival is a controller handoff, not physical landing.',
            'Controlled arrival is the explicit capture start, not a measured interplanetary arrival.',
            'Wait intervals are half-open; choice and attempt-stop rows are endpoints.',
            'State bins describe observations; controller branch and rejection causes are not inferred.',
            'Recorded complete walking/jumping legs do not renew objective or physical permissions.',
            'Survey freshness checks only its recorded age; actor/objective, joint endpoints and solar gates remain native.',
            'Powered routes are unclassified; omitted/deferred candidates are unknown, not unreachable.',
            'Repeated route observations and native counter increments are not independent route jobs.',
            'Whole-run planner totals include both actors and cannot be assigned to individual attempts.']}
    for name, value in [('summary', summary), ('runs', results), ('sources', sources)]:
        (out / (name + '.json')).write_text(json.dumps(value, indent=2) + '\n')
    return summary


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    inspect(args.manifest, args.out)
