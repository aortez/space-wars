#!/usr/bin/env python3
"""Paired, fixed-source scan clocks; native selection remains separately observed."""
import argparse
import gzip
import importlib.util
import json
import math
from pathlib import Path
import subprocess


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


N = module('neighbors', 'compare-arrival-neighbors.py')
W = module('waits', 'audit-acquisition-waits.py')


def clean(value):
    if isinstance(value, dict):
        return {k: clean(v) for k, v in value.items() if k not in ('scan_clock', 'scan_cadence', 'first_scan_success')}
    if isinstance(value, list): return [clean(v) for v in value]
    return value


def neutral(planet):
    c = planet['claim']
    return bool(c and c['planet'] == planet['index'] and
        all(c[k] is None for k in ('owner', 'flag', 'claimant')) and
        c['phase'] == 'idle' and c['progress'] == 0 and
        math.isfinite(c['stage_required_seconds']) and abs(c['stage_required_seconds']-3) <= .001 and
        math.isfinite(c['flag_interaction_range']) and c['flag_interaction_range'] > 0)


def scan_tick(handoff, seat, destination, form, last, cadence):
    request = handoff+1
    matching = last and last['planet'] == destination and last['form'] == form and last['tick'] < request
    delay = (15-(request+seat*7) % 15) % 15 if matching and cadence == 'FourHz' else 0
    return request+delay


def sources(report):
    schedule = report['transfer_comparison']
    for source in schedule['sources']:
        yield 'original', source['seat'], source
    fresh = schedule.get('surveyed_arrival_comparison')
    if fresh:
        for actor in fresh['actors']:
            if actor['source'] is not None:
                yield 'surveyed_arrival', actor['seat'], actor['source']


def snapshots(source):
    return [(view, source.get(view)) for view in ('initial', 'published', 'last_snapshot')
        if source.get(view) is not None]


def evidence(root, report):
    """Reconstruct recorded survey history from a complete native prefix only."""
    wanted = {(seat, source['source_tick']) for _, seat, source in sources(report)
        if snapshots(source)}
    probe = report.get('transfer_probe')
    acq = probe.get('acquisition') if probe else None
    start = acq['started_tick'] if acq else None
    end = acq['outcome']['tick'] if start is not None else None
    stop = max([tick for _, tick in wanted]+([end] if end is not None else [0]))
    expected, last = [0, 0], [None, None]
    captured, window, handoff_history = {}, [], None
    first_scan = None
    continuity = {key: True for key in wanted}
    with gzip.open(root/'trace.jsonl.gz', 'rt') as stream:
        for line in stream:
            row = json.loads(line)
            if row['tick'] > stop: break
            seat, tick = row['seat'], row['tick']
            p = W.P.pilot(row)
            if tick != expected[seat] or p['tick'] != tick or p['owner'] != f'player_{seat+1}':
                raise ValueError('incomplete native history prefix or actor mismatch')
            expected[seat] += 1
            if p['queries_ready'] and p['site_query'] == 'survey':
                last[seat] = dict(tick=tick, planet=p['planet']['index'], form=p['ship_form'])
                if start is not None and seat == probe['seat'] and tick >= start and p['planet']['index'] == probe['destination']:
                    first_scan = tick if first_scan is None else first_scan
            if (seat, tick) in wanted: captured[seat, tick] = (row, last[seat])
            if start is not None and seat == probe['seat'] and tick <= (first_scan if first_scan is not None else end):
                planet = next(p for p in row['observation']['planets'] if p['index'] == probe['destination'])
                for key, (source_row, _) in captured.items():
                    if key[0] != seat: continue
                    source_p = W.P.pilot(source_row)
                    source_planet = next(p for p in source_row['observation']['planets'] if p['index'] == probe['destination'])
                    continuity[key] &= (neutral(planet) and planet['revision'] == source_planet['revision'] and
                        p['ship_form'] == source_p['ship_form'])
            if start is not None and seat == probe['seat'] and start <= tick <= end:
                window.append(row)
                if tick == start: handoff_history = last[seat]
    if set(captured) != wanted: raise ValueError('missing forecast source observation')
    native = None
    if start is not None:
        waiting, endpoint = W.probe_window(report, window, probe['seat'])
        samples = [W.native_sample(r, probe['destination']) for r in waiting]
        chosen = acq['outcome']['reason'] == 'site_selected'
        if chosen: samples.append(W.native_sample(endpoint, probe['destination']))
        summary = W.summarize_window(samples, start, end, chosen, probe['seat'], probe['destination'])
        through_scan = [r for r in window if first_scan is not None and r['tick'] <= first_scan]
        requests = all(W.P.pilot(r)['queries_ready'] and r['mission']['goal'] == 'capture' and
            W.P.pilot(r)['planet']['index'] == probe['destination'] and
            (W.P.pilot(r)['site_query'] == 'survey' or
             isinstance(W.P.pilot(r)['site_query'], dict) and 'deferred' in W.P.pilot(r)['site_query'])
            for r in through_scan if r['tick'] > start)
        native = dict(seat=probe['seat'], destination=probe['destination'], handoff=start,
            first_scan=summary['first_survey_tick'], choice=end if chosen else None,
            last_survey=handoff_history, window=summary,
            uninterrupted_requests=requests,
            neutral_material=bool(through_scan) and all(neutral(W.P.pilot(r)['planet']) for r in through_scan),
            revisions=sorted({W.P.pilot(r)['planet']['revision'] for r in through_scan}),
            forms=sorted({W.P.pilot(r)['ship_form'] for r in through_scan}))
        native['source_continuity'] = {f'{seat}:{tick}': value for (seat, tick), value in continuity.items()}
    return captured, native


def audit_clock(forecast, seat, source, history, native):
    clock = forecast['scan_clock']
    p = W.P.pilot(source)
    destination = forecast['destination']
    planet = next(p for p in source['observation']['planets'] if p['index'] == destination)
    for key, expected in dict(model='conditional_neutral_scan_v1', source_tick=source['tick'],
        actor=p['owner'], destination=destination, revision=planet['revision'], form=p['ship_form'],
        cadence='FourHz', last_survey=history, complete=forecast['end'] is not None,
        site_selection_seconds=None).items():
        if clock[key] != expected: raise ValueError(f'scan clock binding mismatch: {key}')
    assert forecast['source_tick'] == source['tick'] and seat == source['seat']
    handoff = source['tick']+forecast['ticks'] if forecast['end'] == 'kinematic_handoff' else None
    expected_reason = None if neutral(planet) else 'claim outside neutral-idle scan domain'
    if clock['complete'] and handoff is None:
        expected_reason = expected_reason or 'no conditional handoff'
    request = handoff+1 if handoff is not None and expected_reason is None else None
    opportunity = scan_tick(handoff, seat, destination, p['ship_form'], history, clock['cadence']) if request is not None else None
    remaining = (source['observation'].get('match_context') or {}).get('remaining_seconds')
    remaining_ticks = (round(remaining*1_000_000_000)+16_666_666)//16_666_667 if remaining is not None else None
    assert clock['match_remaining_ticks'] == remaining_ticks
    if opportunity is not None and remaining_ticks is not None and opportunity-source['tick'] >= remaining_ticks:
        expected_reason, opportunity = 'match ends before scan opportunity', None
    expected = dict(handoff_tick=handoff, request_tick=request, opportunity_tick=opportunity,
        handoff_to_scan_ticks=opportunity-handoff if opportunity is not None else None, unknown=expected_reason)
    if any(clock[k] != v for k, v in expected.items()): raise ValueError('scan schedule or refusal mismatch')
    comparison = None
    if native is not None and native['seat'] == seat and native['destination'] == destination:
        eligible = (opportunity is not None and native['first_scan'] is not None and
            native['uninterrupted_requests'] and
            source['tick'] < native['handoff'] and
            native['source_continuity'].get(f"{seat}:{source['tick']}") is True and
            native['neutral_material'] and native['revisions'] == [clock['revision']] and
            native['forms'] == [clock['form']] and history == native['last_survey'])
        comparison = dict(comparable=eligible,
            unknown=None if eligible else 'missing handoff/scan or changed native claim, material, form or survey history',
            native_handoff_tick=native['handoff'], native_scan_tick=native['first_scan'],
            native_choice_tick=native['choice'])
        if eligible:
            at_actual = scan_tick(native['handoff'], seat, destination, clock['form'], history, clock['cadence'])
            comparison.update(handoff_error_ticks=handoff-native['handoff'], scan_error_ticks=opportunity-native['first_scan'],
                native_delay_ticks=native['first_scan']-native['handoff'],
                schedule_at_actual_handoff_tick=at_actual,
                actual_handoff_schedule_matches=at_actual == native['first_scan'])
    return dict(clock=clock, native=comparison)


def audit(root, report):
    captured, native = evidence(root, report)
    records = []
    for stage, seat, source in sources(report):
        views = snapshots(source)
        if not views:
            records.append(dict(stage=stage, seat=seat, unknown=source['rejected'] or 'no snapshot'))
            continue
        row, history = captured[seat, source['source_tick']]
        for view, snapshot in views:
            assert snapshot['source_tick'] == source['source_tick']
            for candidate in snapshot['candidates']:
                record = dict(stage=stage, view=view, seat=seat, source_tick=source['source_tick'], destination=candidate['destination'])
                if candidate['forecast'] is None:
                    record['unknown'] = candidate['unknown']
                else:
                    assert candidate['forecast']['destination'] == candidate['destination']
                    record.update(audit_clock(candidate['forecast'], seat, row, history, native))
                records.append(record)
    return dict(records=records, native=native)


def audit_composition(candidate, composition):
    """Check the conditional sum against its separately recorded source parts."""
    f = candidate['forecast']
    clock = f['scan_clock']
    screen = candidate['remote_arrival']
    local = screen['local_reference']
    for key, value in dict(model='conditional_first_scan_success_v1', actor=clock['actor'],
        source_tick=clock['source_tick'], destination=candidate['destination'],
        revision=clock['revision'], handoff_tick=clock['handoff_tick'],
        scan_tick=clock['opportunity_tick'], remaining_trip_seconds=None).items():
        assert composition[key] == value, ('composition binding', key)
    assert 'first scan selects this retained usable site' in composition['conditions']
    assert len(composition['references']) == len(local['references'])
    parent_valid = (clock['unknown'] is None and clock['complete'] and
        f['end'] == 'kinematic_handoff' and screen['complete'] and screen['unknown'] is None and
        local['complete'] and local['unknown'] is None and
        clock['opportunity_tick'] is not None and clock['handoff_tick'] is not None and
        f['handoff_seconds'] is not None)
    assert (composition['unknown'] is None) == parent_valid, 'unexpected parent admission/refusal'
    for reference, source in zip(composition['references'], local['references']):
        for key, value in dict(site=source['site'], measurement_tick=source['measurement_tick'],
            geometry_tick=source['arrival_tick'], eligible_sides=source['eligible_sides'],
            local_seconds=source['conditional_seconds']).items():
            assert reference[key] == value, ('reference binding', key)
        total = reference['conditional_total_seconds']
        valid = (parent_valid and source['unknown'] is None and
            source['measurement_tick'] is not None and clock['opportunity_tick'] is not None and
            0 <= clock['opportunity_tick']-source['measurement_tick'] <= 1800 and
            source['projected'] is not None and bool(source['eligible_sides']) and
            source['conditional_seconds'] is not None)
        assert (total is not None) == valid, 'invented or missing conditional reference'
        if valid:
            assert reference['unknown'] is None
            assert clock['source_tick']+f['ticks'] == clock['handoff_tick'] == source['arrival_tick']
            delay = clock['opportunity_tick']-clock['handoff_tick']
            assert 1 <= delay <= 15 and delay == clock['handoff_to_scan_ticks']
            assert abs(composition['travel_seconds']-f['handoff_seconds']) < 0.00001
            assert abs(composition['scan_wait_seconds']-delay/60) < 0.00001
            expected = f['handoff_seconds']+delay/60+source['conditional_seconds']
            assert math.isfinite(total) and abs(total-expected) < 0.00002
            assert abs(source['conditional_seconds']-sum(source['phases'].values())) < 0.00002
        else:
            assert reference['unknown'] is not None and reference['exceeds_match_time'] is None


def audit_capture_compositions(root, report, case, controlled):
    """Freeze report copies, then join actual choice/capture only retrospectively."""
    fresh = N.M.audit_fresh(root, report, case, controlled, True, True)
    local = N.L.audit_local(root, report, case, controlled, fresh)
    clocks = audit(root, report)
    native = clocks['native']
    records, joins = [], []
    for stage, seat, source in sources(report):
        for view, snapshot in snapshots(source):
            compositions = snapshot.get('first_scan_success', [])
            expected = [c for c in snapshot['candidates'] if c.get('forecast') and
                c['forecast'].get('scan_clock') and c.get('remote_arrival') and
                c['remote_arrival'].get('local_reference')]
            if snapshot['ranked']:
                assert [r['destination'] for r in compositions] == [c['destination'] for c in expected]
            else:
                assert not compositions
            for candidate, composition in zip(expected, compositions):
                audit_composition(candidate, composition)
                records.append(dict(stage=stage, seat=seat, view=view, composition=composition))
                # Earlier report copies remain audited above. Count only the
                # final source reference once, never as independent predictions.
                if stage != 'surveyed_arrival' or view != 'last_snapshot' or not controlled:
                    continue
                if native is None or native['seat'] != seat or native['destination'] != candidate['destination']:
                    continue
                for reference in composition['references']:
                    matches = [j for j in local['retrospectives'] if j.get('matched') and
                        j['selected']['site'] == reference['site'] and
                        j['measurement_tick'] == reference['measurement_tick']]
                    assert len(matches) <= 1
                    actual = matches[0] if matches else None
                    clock_record = next(r for r in clocks['records'] if r.get('clock') and
                        r['stage'] == stage and r['view'] == view and r['seat'] == seat and
                        r['destination'] == candidate['destination'])
                    scan_join = clock_record['native']
                    reason = (reference['unknown'] or
                        ('native first scan/choice differs' if native['first_scan'] != native['choice'] else None) or
                        ('scan history/domain changed' if not scan_join or not scan_join['comparable'] else None) or
                        ('native material/site/direction not covered' if actual is None else None) or
                        (actual['timing_unknown'] if actual else None))
                    milestones = actual['milestones'] if actual else None
                    departure = milestones.get('departed') if milestones else None
                    if departure is None:
                        reason = reason or 'capture did not complete departure'
                    join = dict(seat=seat, destination=candidate['destination'], source_tick=source['source_tick'],
                        site=reference['site'], measurement_tick=reference['measurement_tick'],
                        geometry_tick=reference['geometry_tick'], native_first_scan=native['first_scan'],
                        native_choice=native['choice'], conditional_total_seconds=reference['conditional_total_seconds'],
                        unknown=reason, actual=actual)
                    if reason is None:
                        observed = (departure-source['source_tick'])/60
                        join.update(observed_total_seconds=observed,
                            error_seconds=reference['conditional_total_seconds']-observed)
                    joins.append(join)
    return dict(records=records, retrospective_captures=joins)


def parity(baseline, root, old, report):
    result = N.L.D.unchanged(baseline, root)
    assert N.M.S.sensor_digest(baseline) == N.M.S.sensor_digest(root)
    strip = lambda value: N.Q.without_wall_times(clean(value))
    assert strip(old['transfer_comparison']) == strip(report['transfer_comparison'])
    assert strip(old.get('transfer_probe')) == strip(report.get('transfer_probe'))
    for file in ['transfer-probe.jsonl', 'destination-cover.jsonl']:
        assert (baseline/file).exists() == (root/file).exists()
        if (baseline/file).exists(): assert W.digest(baseline/file) == W.digest(root/file)
    for file in ['transfer-comparison-work.jsonl', 'arrival-survey.jsonl', 'surveyed-arrival-comparison.jsonl']:
        assert [strip(r) for r in N.T.rows(baseline/file)] == [strip(r) for r in N.T.rows(root/file)], file
    return dict(result, native_sensors=True, original_reports_and_work=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--baseline', type=Path, default=Path('target/capture-flag-survey/arrival-neighbors-v1'))
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'], text=True).strip(), 'freeze code and plan first'
    projection = Path('docs/data/capture-arrival-neighbors-v1.json')
    previous = json.loads(projection.read_text())
    W.bound(args.baseline/'summary.json', previous['raw_summary_sha256'])
    baseline = json.loads((args.baseline/'summary.json').read_text())
    assert baseline['complete'] and len(baseline['pairs']) == 8
    for name, pair in baseline['pairs'].items():
        for file, digest in pair['runs']['neighbors3']['hashes'].items():
            W.bound(args.baseline/(name+'-neighbors3')/file, digest)
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    commands = {}
    cases = {spec['case']['name']: spec['case'] for spec in baseline['plan']}
    for name in baseline['pairs']:
        commands[name] = {}
        for mode in ['off', 'on']:
            cmd = [str(binary)]+baseline['commands'][name]['neighbors3'][1:]
            cmd[cmd.index('--out')+1] = str(args.out/(name+'-'+mode))
            commands[name][mode] = cmd+['--forecast-scan-clock', str(mode == 'on').lower()]
    result = dict(schema=1, source_commit=subprocess.check_output(['git','rev-parse','HEAD'], text=True).strip(),
        binary_sha256=W.digest(binary), baseline_sha256=W.digest(args.baseline/'summary.json'),
        tools={p.name:W.digest(p) for p in [Path(__file__), Path(W.__file__), Path(N.__file__)]},
        commands=commands, pairs={}, complete=False,
        scope='Four correlated source ticks in one quiet world, ordinary and controlled, off/on. Conditional first-scan-success references preserve geometry epochs and do not establish actual selection or whole-trip duration. Actual-handoff schedule agreement and capture joins are retrospective, not retimed forecasts.')
    def save(): (args.out/'summary.json').write_text(json.dumps(result, indent=2, allow_nan=False)+'\n')
    save()
    for name, modes in commands.items():
        old_root = args.baseline/(name+'-neighbors3')
        old = json.loads((old_root/'report.json').read_text())
        pair = {}; result['pairs'][name] = pair
        for mode, cmd in modes.items():
            root, logpath = args.out/(name+'-'+mode), args.out/(name+'-'+mode+'.log')
            try:
                with logpath.open('w') as log: subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, check=True)
                N.C.archive(root)
                report = json.loads((root/'report.json').read_text())
                same = parity(old_root, root, old, report)
                if mode == 'on': assert report['transfer_comparison']['scan_cadence'] == 'FourHz'
                else: assert 'scan_clock' not in json.dumps(report) and 'scan_cadence' not in report['transfer_comparison']
                pair[mode] = dict(parity=same, audit=audit(root, report) if mode == 'on' else None,
                    capture_compositions=audit_capture_compositions(root, report,
                        cases[name.rsplit('-', 1)[0]], name.endswith('-controlled')) if mode == 'on' else None,
                    log_sha256=W.digest(logpath), hashes={p.name:W.digest(p) for p in sorted(root.iterdir()) if p.is_file()})
                save(); print(name, mode, 'audited', flush=True)
            except Exception as error:
                result['error'] = dict(case=name, mode=mode, error=repr(error), command=cmd)
                save(); raise
    assert W.digest(binary) == result['binary_sha256']
    result['complete'] = True
    save()


if __name__ == '__main__': main()
