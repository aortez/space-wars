#!/usr/bin/env python3
"""Audit recorded visit endings against their own first terminal mission event."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path


SCOPE = ('Read-only correction ledger for historical mission metrics. The first same-planet '
         'replan or departure after selection closes a visit; later events cannot change it. '
         'Missing/truncated selection history stays unverified. Landing/claim/boarding '
         'milestones are checked for clock bounds, not reconstructed from mission events. '
         'Following pursuit is chronology, not evidence that pursuit caused a failure.')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def audit(report):
    records = []
    counts, reasons = Counter(), Counter()
    for seat, metrics in enumerate(report['metrics']):
        events = report['missions'][seat]['events']
        if any(a['tick'] > b['tick'] for a, b in zip(events, events[1:])):
            raise ValueError('mission events are not ordered')
        selections = {}
        for index, event in enumerate(events):
            if event['kind'] == 'selected':
                identity = event['planet'], event['tick']
                if identity in selections:
                    raise ValueError('ambiguous repeated selection identity')
                selections[identity] = index
        seen = set()
        for visit in metrics['visits']:
            identity = visit['planet'], visit['selected_tick']
            if identity in seen:
                raise ValueError('duplicate visit identity')
            seen.add(identity)
            counts['visits'] += 1
            record = dict(seat=seat, planet=identity[0], selected_tick=identity[1],
                          recorded=visit, status='unverified')
            records.append(record)
            if identity not in selections:
                record['unknown'] = 'selection absent from retained event history'
                counts['unverified'] += 1
                continue
            index = selections[identity]
            following = events[index+1:]
            next_selection = next((i for i, e in enumerate(following) if e['kind'] == 'selected'), None)
            span = following if next_selection is None else following[:next_selection]
            terminal_index = next((i for i, e in enumerate(span)
                if e['planet'] == visit['planet'] and e['kind'] in ['replan', 'departed']), None)
            terminal = None if terminal_index is None else span[terminal_index]
            if terminal is None and next_selection is not None:
                record['unknown'] = 'next selection has no preceding terminal event'
                counts['unverified'] += 1
                continue
            expected = dict(departed_tick=None, abandoned_tick=None, reason=None)
            if terminal is not None:
                if terminal['kind'] == 'departed':
                    expected['departed_tick'] = terminal['tick']
                else:
                    expected.update(abandoned_tick=terminal['tick'], reason=terminal['reason'])
                    reasons[terminal['reason'] or 'unspecified'] += 1
            outcome = ('unfinished' if terminal is None else
                       'completed' if terminal['kind'] == 'departed' else 'abandoned')
            changes = {key: dict(recorded=visit[key], expected=value)
                       for key, value in expected.items() if visit[key] != value}
            outside = {key: visit[key] for key in ['arrived_tick', 'landed_tick', 'claimed_tick', 'boarded_tick']
                       if visit[key] is not None and (visit[key] < visit['selected_tick']
                           or terminal is not None and visit[key] > terminal['tick'])}
            later = [] if terminal_index is None else span[terminal_index+1:]
            record.update(status='verified', selection=events[index], terminal=terminal,
                          outcome=outcome, expected=expected, corrections=changes,
                          milestones_outside_visit=outside,
                          following_pursuit=next((e for e in later if e['kind'] == 'pursuit_started'), None))
            counts['verified'] += 1
            counts[outcome] += 1
            counts['corrected_visits'] += bool(changes)
            counts['milestones_outside_visit'] += bool(outside)
            for field in changes:
                counts['corrected_'+field] += 1
    return dict(scope=SCOPE, counts=dict(counts), terminal_reasons=dict(reasons), visits=records)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--study', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        raise FileExistsError(args.out)
    summary_path = args.study/'summary.json'
    summary = json.loads(summary_path.read_text())
    if not summary['complete']:
        raise ValueError('study is incomplete')
    result = dict(schema=1, scope=SCOPE, source_summary=str(summary_path),
                  summary_sha256=digest(summary_path), auditor_sha256=digest(Path(__file__)), runs={})
    totals = Counter()
    for name, source in summary['runs'].items():
        path = args.study/name/'report.json'
        sha = digest(path)
        if sha != source['report_sha256']:
            raise ValueError(f'{name}: report hash changed')
        run = audit(json.loads(path.read_text()))
        run['report_sha256'] = sha
        result['runs'][name] = run
        totals.update(run['counts'])
    result['counts'] = dict(totals)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(result, indent=2, allow_nan=False)+'\n')
    print(json.dumps(result['counts'], sort_keys=True))


if __name__ == '__main__':
    main()
