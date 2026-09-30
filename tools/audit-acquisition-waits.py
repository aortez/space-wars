#!/usr/bin/env python3
"""Separate scan availability from native selection in bound acquisition traces.

Read-only retrospective analysis. Historical no-enemy-flag observations are not
automatically neutral planets, and a scheduled scan is not a successful choice.
"""
import argparse
from collections import Counter, defaultdict
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import tarfile

SPEC = importlib.util.spec_from_file_location('acquisition',
    Path(__file__).with_name('inspect-site-acquisition.py'))
A = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(A)
SPEC = importlib.util.spec_from_file_location('acquisition_probe',
    Path(__file__).with_name('probe-site-acquisition.py'))
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)

HISTORICAL_MANIFEST = '6e5cdab161d4fc7669daa3eface021c63eef06795a025a1f6a671adeefbf7672'
HISTORICAL_ARCHIVE = '9f1e719d5163c154bd2d7e7efd27c0b49f321959b4133b0be8a1774a98a71e2e'


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def bound(path, expected):
    actual = digest(path)
    if actual != expected:
        raise ValueError(f'input hash mismatch: {path}')
    return actual


def native_sample(row, target):
    sample = A.snapshot(row, target)
    p = A.risk.tail.frozen.pilot(row)
    query = p['site_query']
    next_tick = query['deferred']['next_tick'] if isinstance(query, dict) and 'deferred' in query else None
    if next_tick is not None and (type(next_tick) is not int or next_tick <= row['tick']):
        raise ValueError('invalid deferred scan clock')
    sample['next_scan_tick'] = next_tick
    claim = p['planet']['claim']
    sample['claim_domain'] = ('unavailable' if claim is None else
        'neutral_idle' if claim['owner'] is None and claim['flag'] is None and
        claim['claimant'] is None and claim['phase'] == 'idle' and claim['progress'] == 0 else
        'enemy_flag' if sample['objective_required'] else 'other_claim')
    capture = row['mission'].get('capture') if row['mission']['target'] == target else None
    native = capture.get('acquisition') if capture else None
    # Solar escape can bypass the local controller and leave old telemetry.
    current = bool(native and native['tick'] == row['tick'] and native['planet'] == target and
        native['revision'] == p['planet']['revision'] and p['planet']['index'] == target)
    sample['native'] = native if current else None
    sample['native_status'] = 'current' if current else 'stale' if native else 'absent'
    return sample


def summarize_window(samples, arrival, end, chosen, seat, destination):
    """Wait is [arrival, end); a witnessed choice at end is a separate sample."""
    if arrival > end or (not samples and (chosen or arrival != end)):
        raise ValueError('invalid or empty acquisition window')
    expected = list(range(arrival, end + int(chosen)))
    if [s['tick'] for s in samples] != expected:
        raise ValueError('acquisition window is not dense and ordered')
    if any(s['seat'] != seat for s in samples):
        raise ValueError('acquisition actor mismatch')
    waiting = samples[:-1] if chosen else samples
    if any(s['chosen_site'] is not None for s in waiting):
        raise ValueError('choice precedes bound endpoint')
    endpoint = samples[-1] if chosen else None
    if chosen and (not endpoint['target_local'] or endpoint['local_planet'] != destination or
                   endpoint['chosen_site'] is None or endpoint['chosen_site']['planet'] != destination):
        raise ValueError('bound choice missing or belongs to another planet')
    required = {s['objective_required'] for s in samples}
    domain = ('not_observed' if not required else 'enemy_flag' if required == {True} else
        'no_enemy_flag' if required == {False} else 'changed')
    surveys = [s['tick'] for s in samples if s['target_local'] and s['queries_ready'] and s['site_query'] == 'survey']
    deferred = [s for s in waiting if s['target_local'] and s['queries_ready'] and s['site_query'] == 'deferred']
    current = [s['native'] for s in waiting if s.get('native') is not None]
    checks = Counter()
    for native in current:
        checks.update(native['checks'])
    # Cadence agreement is a retrospective check of the known FourHz recordings.
    # Legacy ledgers lack last_survey/next_tick; do not infer those hidden values.
    phase_agrees = None
    if domain == 'no_enemy_flag' and deferred:
        phase_agrees = all((s['tick'] + seat * 7) % 15 != 0 and
            (s.get('next_scan_tick') is None or s['next_scan_tick'] ==
             s['tick'] + 15 - (s['tick'] + seat * 7) % 15) for s in deferred)
        if endpoint is not None and endpoint['site_query'] == 'survey':
            phase_agrees = phase_agrees and (end + seat * 7) % 15 == 0
    result = dict(arrival_tick=arrival, end_tick=end, chosen=chosen, wait_ticks=end-arrival,
        objective_domain=domain,
        claim_domains=dict(Counter(s.get('claim_domain', 'not_recorded') for s in samples)),
        observed_wait_states=dict(Counter(A.state(s, True) for s in waiting)),
        native_wait_status=dict(Counter(s.get('native_status', 'not_recorded') for s in waiting)),
        native_wait_reasons=dict(Counter(s['reason'] for s in current)),
        native_wait_checks=dict(checks),
        first_survey_tick=surveys[0] if surveys else None,
        chosen_on_first_observed_survey=bool(surveys and end == surveys[0]) if chosen else None,
        deferred_ticks=len(deferred), no_flag_four_hz_phase_agrees=phase_agrees,
        endpoint=endpoint, acquisition_prediction_seconds=None)
    if sum(result['observed_wait_states'].values()) != result['wait_ticks']:
        raise ValueError('wait partition does not cover interval')
    return result


def probe_window(report, rows, seat):
    """Bind the probe clock to the real handoff and first native endpoint."""
    probe = report['transfer_probe']
    acq, transfer = probe['acquisition'], probe['outcome']
    start, outcome = acq['started_tick'], acq['outcome']
    destination = probe['destination']
    if (probe['seat'] != seat or transfer['reason'] != 'arrived' or start != transfer['tick'] or
        acq['schema'] != 1 or outcome is None or not rows):
        raise ValueError('acquisition is not bound to a real transfer handoff')
    end, observed = outcome['tick'], outcome['controller_observed']
    if (type(observed) is not bool or outcome['elapsed_ticks'] != end-start or
        not 0 <= end-start <= acq['horizon_ticks'] or
        [r['tick'] for r in rows] != list(range(start,end+int(observed))) or
        acq['last_observed_tick'] != rows[-1]['tick'] or acq['observed_rows'] != len(rows)):
        raise ValueError('probe clock or observation count mismatch')
    if any(r['seat'] != seat or P.pilot(r)['tick'] != r['tick'] or
           P.pilot(r)['owner'] != f'player_{seat+1}' for r in rows):
        raise ValueError('probe observation actor or clock mismatch')
    first = rows[0]['mission']
    if (first['target'] != destination or first['capture'] is None or
        first['capture']['replans'] != acq['initial_replans'] or
        P.first_choice(rows[0],start,destination) is not None or
        not any(e['kind'] == 'arrived' and e['tick'] == start and e['planet'] == destination
                for e in first['events'])):
        raise ValueError('handoff does not start the recorded capture attempt')
    waiting = rows[:-1] if observed else rows
    reason = lambda r: P.endpoint(r,start,destination,acq['horizon_ticks'],acq['initial_replans'])
    if any(reason(r) is not None for r in waiting):
        raise ValueError('probe window hides an earlier native endpoint')
    stop = rows[-1] if observed else None
    if observed:
        if (reason(stop) != outcome['reason'] or
            P.first_choice(stop,start,destination) != outcome['site']):
            raise ValueError('probe endpoint is not witnessed')
    elif (outcome['site'] is not None or outcome['reason'] not in ('match_finished','runner_ended') or
          report['elapsed_ticks'] != end or
          (report['round']['outcome'] is not None) != (outcome['reason'] == 'match_finished')):
        raise ValueError('invalid unobserved match/runner censor')
    return waiting, stop


def aggregate(records):
    groups = defaultdict(list)
    for record in records:
        window = record.get('window')
        groups['not_arrived' if window is None else window['objective_domain']].append(record)
    result = {}
    for domain, items in sorted(groups.items()):
        windows = [r['window'] for r in items if r.get('window') is not None]
        chosen = [w for w in windows if w['chosen']]
        states = Counter()
        for w in windows:
            states.update(w['observed_wait_states'])
        result[domain] = dict(attempts=len(items), choices=len(chosen),
            chosen_wait_ticks=dict(sorted(Counter(w['wait_ticks'] for w in chosen).items())),
            total_wait_ticks=sum(w['wait_ticks'] for w in windows),
            chosen_on_first_observed_survey=sum(w['chosen_on_first_observed_survey'] is True for w in chosen),
            observed_wait_states=dict(states))
    return result


def historical(archive, manifest):
    bound(archive, HISTORICAL_ARCHIVE)
    bound(manifest, HISTORICAL_MANIFEST)
    files = json.loads(manifest.read_text())['files']
    prefix = 'target/bot-site-acquisition/normal/'
    records, inputs = [], {}
    with tarfile.open(archive, 'r:gz') as source:
        def read(name):
            key = prefix + name
            data = source.extractfile(key).read()
            if len(data) != files[key]['bytes'] or hashlib.sha256(data).hexdigest() != files[key]['sha256']:
                raise ValueError(f'archive member hash mismatch: {key}')
            inputs[key] = files[key]
            return data
        runs = json.loads(read('runs.json'))
        for run in runs:
            data = read(run['ledger'])
            if hashlib.sha256(data).hexdigest() != run['ledger_sha256']:
                raise ValueError('ledger differs from bound run')
            by_attempt = defaultdict(list)
            for line in data.splitlines():
                sample = json.loads(line)
                by_attempt[sample['seat'], sample['selected_tick']].append(sample)
            for attempt in run['attempts']:
                selection = attempt['selection']
                chosen = attempt['first_choice_tick'] is not None
                arrival = attempt['arrival_tick']
                record = dict(label=run['label'], seed=run['seed'], seat=selection['seat'],
                    selected_tick=selection['selected_tick'], destination=selection['planet'],
                    policy=selection['policy'], ending=attempt['ending'], reason=attempt['reason'],
                    arrival_status=attempt['arrival_status'], raw_trace_sha256=run['trace_sha256'], window=None)
                if arrival is not None:
                    end = attempt['first_choice_tick'] if chosen else attempt['stopped_tick']
                    samples = [{k:v for k,v in s.items() if k not in ('selected_tick','evidence_state')}
                        for s in by_attempt[selection['seat'], selection['selected_tick']]
                        if arrival <= s['tick'] < end + int(chosen)]
                    window = summarize_window(samples, arrival, end, chosen, selection['seat'], selection['planet'])
                    if chosen and window['endpoint'] != attempt['choice']:
                        raise ValueError('choice differs from bound run')
                    if window['wait_ticks'] != attempt['local_observed_ticks'] or attempt['local_missing_ticks'] != 0:
                        raise ValueError('local wait differs from bound run')
                    record['window'] = window
                    if window['objective_domain'] == 'no_enemy_flag' and window['wait_ticks'] > 1:
                        record['wait_samples'] = samples[:-1] if chosen else samples
                records.append(record)
    return dict(scope='Historical v11 normal matches, all attempts; no-enemy-flag does not establish neutral-idle claim state. Earlier derived ledgers, not newly replayed physics.',
        archive=str(archive), archive_sha256=HISTORICAL_ARCHIVE, manifest_sha256=HISTORICAL_MANIFEST,
        inputs=inputs, runs=len(runs), worlds=len({r['seed'] for r in runs}),
        attempts=records, groups=aggregate(records))


def current(arrivals):
    projection_path = Path(__file__).resolve().parents[1]/'docs/data/capture-arrival-neighbors-v1.json'
    projection = json.loads(projection_path.read_text())
    bound(arrivals/'summary.json', projection['raw_summary_sha256'])
    summary = json.loads((arrivals/'summary.json').read_text())
    if (not summary['complete'] or len(summary['pairs']) != 8 or
        set(summary['pairs']) != set(projection['pairs']) or
        any(set(pair['runs']) != {'nearest','neighbors3'} for pair in summary['pairs'].values())):
        raise ValueError('incomplete frozen sixteen-run mode coverage')
    records, skipped = [], []
    fingerprints = {}
    for name, pair in summary['pairs'].items():
        for mode, run in pair['runs'].items():
            root = arrivals/(name+'-'+mode)
            for file in ('report.json', 'trace.jsonl.gz'):
                bound(root/file, run['hashes'][file])
            report = json.loads((root/'report.json').read_text())
            probe = report.get('transfer_probe')
            acq = probe.get('acquisition') if probe else None
            if acq is None or acq['started_tick'] is None:
                if name.endswith('-controlled'):
                    raise ValueError('controlled condition missing acquisition window')
                skipped.append(dict(name=root.name, reason='no acquisition probe window'))
                continue
            outcome = acq['outcome']
            if outcome is None:
                raise ValueError('unfinished acquisition probe')
            start, end = acq['started_tick'], outcome['tick']
            chosen = outcome['reason'] == 'site_selected'
            seat = int(run['command'][run['command'].index('--seat')+1])
            destination = probe['destination']
            observed = []
            raw_window = hashlib.sha256()
            with gzip.open(root/'trace.jsonl.gz', 'rt') as trace:
                for line in trace:
                    row = json.loads(line)
                    if row['seat'] == seat and start <= row['tick'] < end + int(outcome['controller_observed']):
                        raw_window.update(json.dumps(row,sort_keys=True,separators=(',',':')).encode())
                        observed.append(row)
            waiting, stop = probe_window(report,observed,seat)
            samples = [native_sample(r,destination) for r in waiting]
            if chosen:
                samples.append(native_sample(stop,destination))
            window = summarize_window(samples,start,end,chosen,seat,destination)
            window['stop_witness'] = native_sample(stop,destination) if stop is not None and not chosen else None
            if chosen and (not outcome['controller_observed'] or
                window['endpoint']['chosen_site'] != outcome['site'] or
                window['endpoint']['native'] is None or
                window['endpoint']['native']['selected_site'] != outcome['site'] or
                window['endpoint']['native']['reason'] != 'selected_site'):
                raise ValueError('native choice is not witnessed by current telemetry')
            key = raw_window.hexdigest()
            records.append(dict(name=root.name, seat=seat, destination=destination,
                command=run['command'], outcome=outcome, window=window,
                report_sha256=run['hashes']['report.json'], trace_sha256=run['hashes']['trace.jsonl.gz'],
                raw_window_sha256=key, duplicate_of=fingerprints.get(key)))
            fingerprints.setdefault(key, root.name)
    unique = [r for r in records if r['duplicate_of'] is None]
    return dict(scope='Frozen v13 controlled continuation windows only. Same-window observer modes are duplicates; four same-world source ticks remain correlated.',
        source_commit=summary['source_commit'], binary_sha256=summary['binary_sha256'],
        summary_sha256=projection['raw_summary_sha256'], projection_sha256=digest(projection_path),
        runs=records, skipped=skipped, unique_windows=len(unique), groups=aggregate(unique))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--historical',type=Path,required=True)
    parser.add_argument('--arrivals',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    if args.out.exists():
        raise ValueError('use a new output file')
    result = dict(model='acquisition_wait_audit_v1', tool_sha256=digest(Path(__file__)),
        helpers_sha256={Path(m.__file__).name:digest(Path(m.__file__)) for m in (A,P)},
        historical=historical(args.historical/'evidence.tar.gz',args.historical/'manifest.json'),
        current=current(args.arrivals),
        limits=['Retrospective evidence; no new simulations or changes to playing behavior.',
            'Half-open wait intervals exclude the site-selection endpoint.',
            'Deferred scans measure sensor availability, not unavailable terrain.',
            'Missing or stale native telemetry cannot identify a controller branch.',
            'Native candidate counters count repeated checks; solar subcategories can overlap.',
            'No-enemy-flag legacy observations do not establish the neutral timing model domain.',
            'First-scan selection here is not a guaranteed or maximum future acquisition duration.',
            'No acquisition prediction, complete trip cost, confidence interval or win-rate claim.'])
    args.out.parent.mkdir(parents=True,exist_ok=True)
    args.out.write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    print(json.dumps({k:result[k]['groups'] for k in ('historical','current')},indent=2))


if __name__ == '__main__':
    main()
