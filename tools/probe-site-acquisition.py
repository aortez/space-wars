#!/usr/bin/env python3
"""Follow fixed numeric alternative references to their first actual site choice."""
import argparse
from collections import Counter
import gzip
import hashlib
import importlib.util
import json
import struct
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location('composition', Path(__file__).with_name('compose-capture-references.py'))
L = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(L)
C, T, F = L.C, L.T, L.F
HORIZON_SECONDS = 30
UPSTREAM = ['mission-evaluations.jsonl', 'flag-survey.jsonl', 'flag-value-shadow.jsonl',
            'mission-evaluation-work.jsonl', 'flag-survey-work.jsonl',
            'flag-value-shadow-work.jsonl', 'destination-cover.jsonl']


def plan(reference):
    study = json.loads((reference / 'summary.json').read_text())
    cases, denominator = [], Counter()
    for spec in study['plan']:
        reference_run = spec['name'] + '-budget64'
        for source in study['runs'][reference_run]['source_references']:
            for candidate in source['candidates']:
                numeric = candidate['local_reference']['remaining'] is not None
                denominator['current' if candidate['current'] else
                            'numeric_alternative' if numeric else 'unknown_alternative'] += 1
                if candidate['current'] or not numeric:
                    continue
                old = next(c for c in spec['references'] if c['seat'] == source['seat']
                           and c['source_tick'] == source['source_tick'])
                cases.append(dict(name=f"{spec['condition']['name']}-s{source['seat']}t{source['source_tick']}-p{candidate['destination']}",
                    condition=spec['condition'], seat=source['seat'], source_tick=source['source_tick'],
                    destination=candidate['destination'], reference_run=reference_run,
                    candidate=candidate, transfer_source=old['transfer_source']))
    assert dict(denominator) == dict(current=21, numeric_alternative=20, unknown_alternative=21)
    assert len(cases) == len({c['name'] for c in cases}) == 20
    assert all(c['candidate']['local_reference']['evidence']['remote'] for c in cases)
    return cases, dict(denominator)


def pilot(row):
    return row['observation']['local']['combat']['recovery']['flight']['pilot']


def first_choice(row, start, destination):
    """A current native selection, not a retained site or fabricated finish row."""
    capture = row['mission']['capture']
    if capture is None or capture['site'] is None or capture['started_tick'] is None:
        return None
    a = capture.get('acquisition')
    if (start <= capture['started_tick'] <= row['tick'] and capture['site']['planet'] == destination
        and a and a['tick'] == row['tick'] and a['planet'] == destination and a['selected_site'] == capture['site']):
        return capture['site']
    return None


def endpoint(row, start, destination, horizon, initial_replans=0):
    p, m = pilot(row), row['mission']
    c = m['capture']
    if not p['ship_available'] or p['ship_health'] <= 0 or p['ship_form'] != 'ship': return 'ship_or_pilot_lost'
    if m['goal'] == 'recover' or m['recovery'] is not None: return 'recovery'
    if m['target'] != destination: return 'retargeted'
    if row['tick'] > start and any(e['tick'] == row['tick'] and e['kind'] in {'selected', 'replan', 'arrived'}
                                   for e in m['events']): return 'attempt_replaced'
    if c is None: return 'capture_ended'
    if c['failed_tick'] is not None or c['failure'] is not None: return 'capture_failed'
    if p['planet']['index'] != destination: return 'approach_frame_changed'
    if c['replans'] != initial_replans: return 'capture_replanned'
    if first_choice(row, start, destination) is not None: return 'site_selected'
    if c['site'] is not None: return 'unwitnessed_site'
    if p['location'] == 'on_foot' or c['landing']['landed_tick'] is not None or c['completed_tick'] is not None:
        return 'physical_progress_without_choice'
    if row['tick'] - start >= horizon: return 'observation_horizon'
    return None


def reference_status(case, source, rows):
    reference = case['candidate']['local_reference']
    L.audit_local(reference)
    evidence = reference['evidence']
    target = lambda row: next(p for p in row['observation']['planets'] if p['index'] == case['destination'])
    original = target(source)
    assert original['claim']['owner'] is None and L.local_flag(original) is None
    assert original['revision'] == evidence['revision']
    assert T.f32_identity(original['radius']) == T.f32_identity(evidence['radius'])
    assert T.f32_identity(original['claim']['stage_required_seconds']) == T.f32_identity(evidence['stage_seconds'])
    f32 = lambda v: struct.unpack('!f', struct.pack('!f', v))[0]
    assert f32(f32(original['claim']['flag_interaction_range']) - f32(.2)) == f32(evidence['flag_range'])
    changed = next((r['tick'] for r in rows if not L.same_planet(original, target(r))), None)
    expired = next((r['tick'] for r in rows if r['tick'] - evidence['tick'] > 1800), None)
    return dict(source_planet=original, last_observed_planet=target(rows[-1]), last_observed_tick=rows[-1]['tick'],
                first_incompatible_tick=changed, first_expired_tick=expired,
                evidence_tick=evidence['tick'], evidence_age_at_last_observation=rows[-1]['tick']-evidence['tick'])


def audit_acquisition(case, report, control):
    probe = report['transfer_probe']
    a, transfer = probe['acquisition'], probe['outcome']
    assert a['schema'] == 1 and a['horizon_ticks'] == HORIZON_SECONDS * 60
    assert 'bounded_acquisition' not in report
    if transfer['reason'] != 'arrived':
        assert a['started_tick'] is None and a['last_observed_tick'] is None and a['outcome'] is None
        assert a['observed_rows'] == 0
        return dict(started=False, transfer_outcome=transfer['reason'], choice_seconds=None)
    start, end = a['started_tick'], a['outcome']
    assert start == transfer['tick']
    assert end['elapsed_ticks'] == end['tick']-start and 0 <= end['elapsed_ticks'] <= a['horizon_ticks']
    assert report['elapsed_ticks'] == end['tick']
    observed = [r for r in control if r['tick'] >= start]
    assert a['last_observed_tick'] == observed[-1]['tick']
    assert [r['tick'] for r in observed] == list(range(start, a['last_observed_tick']+1))
    assert a['observed_rows'] == len(observed)
    assert a['initial_replans'] == observed[0]['mission']['capture']['replans']
    assert any(e['kind'] == 'arrived' and e['tick'] == start and e['planet'] == case['destination']
               for e in observed[0]['mission']['events'])
    assert first_choice(observed[0], start, case['destination']) is None
    for r in observed:
        assert r['seat'] == case['seat'] and pilot(r)['tick'] == r['tick']
        assert pilot(r)['owner'] == f"player_{case['seat']+1}"
    if end['controller_observed']:
        assert end['tick'] == observed[-1]['tick']
        assert endpoint(observed[-1], start, case['destination'], a['horizon_ticks'], a['initial_replans']) == end['reason']
        assert T.f32_identity(end['site']) == T.f32_identity(first_choice(observed[-1], start, case['destination']))
        waiting = observed[:-1]
    else:
        assert end['tick'] == observed[-1]['tick']+1 and end['site'] is None
        assert end['reason'] in {'match_finished', 'runner_ended'}
        assert (report['round']['outcome'] is not None) == (end['reason'] == 'match_finished')
        waiting = observed
    assert all(endpoint(r, start, case['destination'], a['horizon_ticks'], a['initial_replans']) is None for r in waiting)
    assert len(waiting) == end['elapsed_ticks']  # half-open interval, no endpoint command
    reasons = Counter()
    for r in waiting:
        c = r['mission']['capture']
        native = c.get('acquisition') if c else None
        reason = native['reason'] if native and native['tick'] == r['tick'] else 'no_current_native_update'
        reasons[reason] += 1
    status = reference_status(case, control[0], control)
    status['arrival_planet'] = next(p for p in observed[0]['observation']['planets'] if p['index'] == case['destination'])
    status['evidence_age_at_arrival'] = start-status['evidence_tick']
    status['evidence_age_at_endpoint'] = end['tick']-status['evidence_tick']
    status['endpoint_planet'] = status['last_observed_planet'] if end['controller_observed'] else None
    chosen = end['reason'] == 'site_selected'
    same_site = end['site'] == case['candidate']['local_reference']['evidence']['site'] if chosen else None
    return dict(started=True, transfer_outcome=transfer['reason'], arrival_tick=start, outcome=end,
        choice_seconds=end['elapsed_ticks']/60 if chosen else None, selected_site=end['site'] if chosen else None,
        matches_reference_site=same_site,
        compatible_reference_at_choice=bool(chosen and same_site and status['first_incompatible_tick'] is None
                                            and status['first_expired_tick'] is None),
        reference=status, waiting_ticks=len(waiting), native_wait_reasons=dict(reasons),
        ordinary_post_arrival_rows=len(observed)-1)


def audit_source(case, reference, root, probe):
    original = reference/case['reference_run']
    digest, count, source_rows = hashlib.sha256(), 0, {}
    with gzip.open(original/'trace.jsonl.gz', 'rb') as a, (root/'trace.jsonl').open('rb') as b:
        for line in a:
            old = json.loads(line)
            if old['tick'] > case['source_tick']: break
            new_line = next(b)
            new = json.loads(new_line)
            if old['tick'] < case['source_tick']:
                assert line == new_line, 'ordinary prefix changed'
                digest.update(line)
                count += 1
            else:
                assert new['observation'] == old['observation']
                if old['seat'] != case['seat']: assert new_line == line
                source_rows[new['seat']] = new
    assert count == 2*case['source_tick'] and set(source_rows) == {0, 1}
    assert T.f32_identity(probe['source']['transfer_source']) == T.f32_identity(case['transfer_source'])
    assert probe['source']['nomination'] == case['candidate']['nomination']
    assert source_rows[case['seat']]['actions'] == case['candidate']['source_actions']
    return dict(prefix_rows=count, prefix_sha256=digest.hexdigest(), source_and_nomination_exact=True)


def audit_pair(before, after):
    """On/off physical parity ends at handoff; it does not claim later parity."""
    reports = [json.loads((r/'report.json').read_text()) for r in [before, after]]
    old, new = [r['transfer_probe'] for r in reports]
    assert old == {k: v for k, v in new.items() if k != 'acquisition'} | {'scope': old['scope']}
    assert F.digest(before/'transfer-probe.jsonl') == F.digest(after/'transfer-probe.jsonl')
    count, digest = 0, hashlib.sha256()
    with gzip.open(before/'trace.jsonl.gz', 'rb') as a, (after/'trace.jsonl').open('rb') as b:
        for line in a:
            assert line == next(b), 'continuation changed transfer controls/observations'
            count += 1
            digest.update(line)
    expected = 2*(old['outcome']['tick'] + (old['outcome']['reason'] != 'match_finished'))
    assert count == expected
    for name in UPSTREAM:
        # These files contain no wall times. The on run may append work after
        # executing the handoff command; every old byte must remain a prefix.
        old_bytes = (before/name).read_bytes()
        with (after/name).open('rb') as stream:
            assert stream.read(len(old_bytes)) == old_bytes, name
    return dict(controller_rows=count, controller_prefix_sha256=digest.hexdigest(),
                exact_transfer_trace=True, exact_upstream_prefix=True)


def aggregate(runs):
    outcomes, waits = Counter(), []
    same, compatible = 0, 0
    for r in runs.values():
        a = r['acquisition']
        outcomes[a['outcome']['reason'] if a['started'] else 'transfer_'+a['transfer_outcome']] += 1
        if a['choice_seconds'] is not None:
            waits.append(a['choice_seconds'])
            same += a['matches_reference_site']
            compatible += a['compatible_reference_at_choice']
    return dict(cases=len(runs), outcomes=dict(outcomes), completed_choice_seconds=waits,
                same_reference_site=same, compatible_reference_at_choice=compatible)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--baseline-binary', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    opts = parser.parse_args()
    opts.binary, opts.baseline_binary = [p.resolve(strict=True) for p in [opts.binary, opts.baseline_binary]]
    cases, denominator = plan(opts.reference)
    opts.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=cases, denominator=denominator, horizon_seconds=HORIZON_SECONDS, runs={},
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()),
        binary_sha256=F.digest(opts.binary), baseline_binary_sha256=F.digest(opts.baseline_binary),
        reference_summary_sha256=F.digest(opts.reference/'summary.json'),
        scope='All20 source-numeric alternatives from62 fixed candidates, no outcome selection. Neutral remote references only;21 current and21 unknown alternatives remain outside this conditional follow-through. Correlated engineering trials, not a calibration, strength test or Pi benchmark. Transfer suppresses new pursuit; ordinary controls resume after actual arrival. Stops at first observed choice/interruption or30s censor. Native bounded acquisition disabled. No deployment or live model changes.')
    save = lambda: F.D.write(opts.out/'summary.json', result)
    save()  # freeze the full denominator, hypotheses and horizon before physics
    for case in cases:
        args = T.probe_args(case) + ['--probe-transfer-pursuit', 'defer_new', '--bounded-acquisition-seats', 'none']
        roots = [opts.out/(case['name']+suffix) for suffix in ['-off', '-on']]
        off = F.D.run(opts.baseline_binary, opts.out, roots[0].name, args, case['seat'], seconds=case['source_tick']//60+91)
        before = json.loads((roots[0]/'report.json').read_text())['transfer_probe']
        pinned = dict(case, expected_reason=before['source']['diagnostic']['reason'])
        off['probe'] = T.audit_probe(pinned, before, T.rows(roots[0]/'transfer-probe.jsonl'))
        off['source'] = audit_source(case, opts.reference, roots[0], before)
        off['trace_sha256'] = C.archive(roots[0])
        C.audit_controls(case, roots[0], T.rows(roots[0]/'transfer-probe.jsonl'))
        on = F.D.run(opts.binary, opts.out, roots[1].name,
                     args+['--probe-acquisition-seconds', str(HORIZON_SECONDS)], case['seat'], seconds=case['source_tick']//60+91)
        report = json.loads((roots[1]/'report.json').read_text())
        on['source'] = audit_source(case, opts.reference, roots[1], report['transfer_probe'])
        on['parity'] = audit_pair(*roots)
        control = [r for r in T.rows(roots[1]/'trace.jsonl') if r['seat'] == case['seat'] and r['tick'] >= case['source_tick']]
        on['acquisition'] = audit_acquisition(case, report, control)
        on['trace_sha256'] = C.archive(roots[1])
        on['off'] = off
        on['files'] = {root.name: {p.name: F.digest(p) for p in sorted(root.iterdir()) if p.is_file()} for root in roots}
        result['runs'][case['name']] = on
        save()
        print(case['name'], on['acquisition'], flush=True)
    assert F.digest(opts.binary) == result['binary_sha256']
    assert F.digest(opts.baseline_binary) == result['baseline_binary_sha256']
    result['results'] = aggregate(result['runs'])
    save()


if __name__ == '__main__':
    main()
