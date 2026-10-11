#!/usr/bin/env python3
"""Join published mission costs to exact visits and observed arrival milestones."""
from collections import Counter
import argparse
import hashlib
import json
from pathlib import Path


SCOPE = ('First current cost, current/alternative pair and complete shortlist available during each recorded visit. '
         'Availability uses evaluator completion, not the earlier source tick. Arrival is a physical milestone, '
         'not the native descent/commitment gate. Missing, abandoned and unarrived visits remain in the denominator; '
         'repeated worlds and report counts are not independent decisions.')


def before_end(visit, tick):
    return tick >= visit['selected_tick'] and all(
        end is None or tick < end for end in [visit['departed_tick'], visit['abandoned_tick']])


def phase(visit, record):
    if record is None:
        return 'unavailable'
    arrival = visit['arrived_tick']
    if arrival is None:
        return 'no_arrival_observed'
    return 'before_arrival' if record['completed_tick'] < arrival else 'at_or_after_arrival'


def analyze(report, evaluations, seat):
    visits = report['metrics'][seat]['visits']
    lookup = {(v['planet'], v['selected_tick']):dict(visit=v,
        first_current=None, first_pair=None, first_complete=None) for v in visits}
    assert len(lookup) == len(visits), 'ambiguous visit identity'
    counts = Counter()
    last_completed = -1
    for row in evaluations:
        if row['actor'] != f'player_{seat+1}':
            continue
        source, completed = row['source_tick'], row['completed_tick']
        assert completed is not None and source <= completed
        assert completed >= last_completed, 'evaluations out of completion order'
        last_completed = completed
        key = (row['current_target'], row['selected_tick'])
        if key not in lookup:
            assert row['selected_tick'] is None, 'evaluation references an absent visit'
            continue
        record = lookup[key]
        visit = record['visit']
        if not before_end(visit, source) or not before_end(visit, completed):
            counts['reports_outside_active_visit'] += 1
            continue
        current = next((c for c in row['candidates'] if c['current']), None)
        if current is None or row['inactive_reason'] is not None:
            continue
        assert current['planet'] == visit['planet']
        counts['active_reports'] += 1
        if (visit['arrived_tick'] is not None and completed < visit['arrived_tick']
                and current['unknown_reason'] == 'remote or local surface unmeasured'):
            owner = 'neutral' if current['ownership_known'] and current['observed_owner'] is None else 'owned_or_unknown'
            counts[f'before_arrival_current_surface_unmeasured_{owner}'] += 1
        numeric = [c for c in row['candidates'] if c['total_seconds'] is not None]
        has_current = current['total_seconds'] is not None
        paired = has_current and any(not c['current'] for c in numeric)
        complete = paired and len(numeric) == len(row['candidates']) and not row['candidates_truncated']
        reference = dict(source_tick=source, completed_tick=completed,
            current_owner=current['observed_owner'], current_ownership_known=current['ownership_known'],
            current_evidence_tick=current['evidence_tick'],
            numeric_planets=[c['planet'] for c in numeric],
            current_seconds=current['total_seconds'], model=row['model'])
        for name, available in [('first_current', has_current), ('first_pair', paired), ('first_complete', complete)]:
            if available and record[name] is None:
                record[name] = reference
    for record in lookup.values():
        for name in ['first_current', 'first_pair', 'first_complete']:
            counts[f'{name}_{phase(record["visit"], record[name])}'] += 1
    counts['visits'] = len(visits)
    return dict(scope=SCOPE, counts=dict(counts), visits=list(lookup.values()))


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--study', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    opts = parser.parse_args()
    assert not opts.out.exists(), 'preserve the earlier analysis'
    study = json.loads((opts.study/'summary.json').read_text())
    assert study['complete'] and len(study['runs']) == len(study['plan'])
    result = dict(schema=1, scope=SCOPE, study=str(opts.study),
        study_summary_sha256=digest(opts.study/'summary.json'),
        analyzer_sha256=digest(Path(__file__)), records={}, totals={})
    totals = {}
    for item in study['plan']:
        name = item['name']
        path = opts.study/name
        hashes = {filename:digest(path/filename) for filename in ['report.json', 'mission-evaluations.jsonl']}
        for filename, sha256 in hashes.items():
            assert study['runs'][name]['hashes'][filename] == sha256, (name, filename)
        report = json.loads((path/'report.json').read_text())
        with (path/'mission-evaluations.jsonl').open() as stream:
            record = analyze(report, map(json.loads, stream), item['seat'])
        record['hashes'] = hashes
        record['seat'] = item['seat']
        result['records'][name] = record
        key = item['kind'] + ('_candidate' if item['candidate'] else '_predecessor')
        totals.setdefault(key, Counter()).update(record['counts'])
    result['totals'] = {key:dict(value) for key,value in totals.items()}
    with opts.out.open('x') as stream:
        stream.write(json.dumps(result, indent=2, allow_nan=False)+'\n')
    print(json.dumps(result['totals'], indent=2))


if __name__ == '__main__':
    main()
