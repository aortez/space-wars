#!/usr/bin/env python3
"""Frozen ordinary-play comparison study; no destination or control intervention."""
import argparse
from collections import Counter, defaultdict
import importlib.util
import json
import math
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location('schedule', Path(__file__).with_name('schedule-transfer-references.py'))
S = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(S)
C, T, F = S.C, S.T, S.F
ALLOWANCES = (32, 64)


def plan(cases):
    conditions = defaultdict(list)
    for c in cases:
        conditions[c['condition']['name']].append(c)
    result = []
    for name, refs in conditions.items():
        sources = sorted({(c['seat'], c['source_tick']) for c in refs})
        groups = [sources] if len({s for s, _ in sources}) == len(sources) else [[s] for s in sources]
        for group in groups:
            result.append(dict(name=name + '-' + '-'.join(f's{s}t{t}' for s, t in group),
                condition=refs[0]['condition'], sources=[dict(seat=s, tick=t) for s, t in group],
                seconds=(max(t for _, t in group) + 122)//60+1,
                references=[c for c in refs if (c['seat'], c['source_tick']) in group]))
    assert len(cases) == 25 and len(result) == 13
    assert sum(len(c['sources']) for c in result) == 21
    return result


def audit_report(report):
    assert report['model'] == 'guided_transfer_comparison_v1'
    cs = report['candidates']
    assert 1 <= len(cs) <= 3 and len({c['destination'] for c in cs}) == len(cs)
    assert cs[0]['current'] and cs[0]['destination'] == report['current_destination']
    assert cs[0]['nomination'] is None and all(not c['current'] for c in cs[1:])
    assert [c['destination'] for c in cs[1:]] == sorted(c['destination'] for c in cs[1:])
    charged, known = 0, []
    for c in cs:
        f = c['forecast']
        if c['nomination'] is not None:
            assert c['nomination']['destination'] == c['destination']
            if not c['nomination']['accepted']:
                assert c['unknown'] and f is None
        if f is None:
            assert c['unknown']
            continue
        assert c['unknown'] is None and f['source_tick'] == report['source_tick']
        assert f['destination'] == c['destination'] and f['horizon_ticks'] == 3600
        assert 0 <= f['ticks'] == f['charged_graph'] <= 3600
        assert len(f['samples']) <= 62
        charged += f['charged_graph']
        if report['ranked']:
            assert f['end'] is not None
        if f['end'] == 'kinematic_handoff':
            assert isinstance(f['handoff_seconds'], (int, float)) and math.isfinite(f['handoff_seconds'])
            assert 0 < f['handoff_seconds'] and math.isclose(f['handoff_seconds'], f['ticks']/60, abs_tol=.00001)
            known.append((f['ticks'], c['destination']))
        else:
            assert f['handoff_seconds'] is None
    assert report['charged_graph'] == charged + int(report['ranked']) <= 10801
    fastest = sorted(d for t, d in known if t == min(x[0] for x in known)) if known and report['ranked'] else []
    assert report['fastest_known_handoffs'] == fastest
    complete = report['ranked'] and not report['candidates_truncated'] and len(known) == len(cs) >= 2
    assert report['preferred_handoffs'] == (fastest if complete else [])


def audit_schedule(schedule, rows, upstream, elapsed):
    assert schedule['observational'] and schedule['physics_queries'] == 0
    assert schedule['playing_graph_allowance'] == 4 and schedule['max_source_age_ticks'] == 120
    allowance = schedule['total_graph_allowance']
    assert allowance in ALLOWANCES
    sources = {s['seat']: s for s in schedule['sources']}
    assert len(sources) == len(schedule['sources'])
    assert all(s['attempted'] for s in sources.values())
    first = min(s['source_tick'] for s in sources.values())
    assert [r['tick'] for r in rows] == list(range(first, elapsed))
    charges, ready, states, snapshots = Counter(), {}, {}, {}
    for n, row in enumerate(rows):
        tick = row['tick']
        assert row['event'] == 'dispatch'
        prior = upstream[tick]['total']
        assert row['playing_charged_graph'] == prior
        remaining = dict(graph=allowance-prior, physics_queries=0)
        assert row['remaining_before_comparison'] == remaining
        a = row['allocation']
        assert a['tick'] == n+1 and a['allowance'] == remaining
        assert 0 <= a['charged']['graph'] <= allowance-prior and a['charged']['physics_queries'] == 0
        assert sum(j['charged']['graph'] for j in a['jobs']) == a['charged']['graph']
        jobs = {j['request']['actor']: j for j in a['jobs']}
        assert len(jobs) == len(a['jobs'])
        actors = {r['seat']: r for r in row['actors']}
        assert len(actors) == len(row['actors'])
        assert set(actors) == {seat for seat, s in sources.items() if s['source_tick'] <= tick}
        assert set(jobs) <= set(actors)
        for seat, r in actors.items():
            source, state, job = sources[seat], r['state'], jobs.get(seat)
            if state is None:
                assert source['rejected'] == r['rejected'] and r['rejected']
                assert job is None and r['candidates'] is None
                continue
            assert r['rejected'] is None
            assert state['actor'] == f'player_{seat+1}' and state['token']['actor'] == seat
            assert state['source_tick'] == source['source_tick']
            assert state['destination'] == source['initial']['current_destination']
            assert source['initial']['source_tick'] == source['source_tick'] and source['initial']['actor'] == state['actor']
            if seat in states:
                for key in ['token', 'actor', 'source_tick', 'destination']:
                    assert state[key] == states[seat][key]
                assert not (states[seat]['phase'] == 'ready' and state['phase'] == 'pending')
            if job:
                assert job['request'] == state['token'] and job['charged']['physics_queries'] == 0
                charges[seat] += job['charged']['graph']
                assert job['phase'].lower() == state['phase']
            assert state['charged_graph'] == charges[seat]
            assert r['candidates'] is not None
            assert len(r['candidates']) == len(source['initial']['candidates'])
            for c, initial, final in zip(r['candidates'], source['initial']['candidates'], source['last_snapshot']['candidates']):
                assert c['destination'] == initial['destination'] and c['unknown'] == initial['unknown']
                f = final['forecast']
                assert 0 <= c['charged_graph'] <= (f['charged_graph'] if f else 0)
                assert c['end'] == (f['end'] if f and c['charged_graph'] == f['charged_graph'] else None)
            if seat in snapshots:
                assert all(a['charged_graph'] <= b['charged_graph'] for a, b in zip(snapshots[seat]['candidates'], r['candidates']))
            assert sum(c['charged_graph'] for c in r['candidates']) + int(r['ranked']) == charges[seat]
            if state['phase'] == 'stale':
                assert job is None and state['validated_tick'] is None and state['reason']
                assert source['source_tick'] <= state['cancelled_tick'] <= tick
                if state['reason'] == 'source expired':
                    assert state['cancelled_tick'] == state['source_tick']+121
                if seat in states and states[seat]['phase'] == 'stale':
                    assert state == states[seat]
                assert r['snapshot_tick'] < state['cancelled_tick']
            else:
                assert state['validated_tick'] == tick and tick-state['source_tick'] <= 120
                assert state['reason'] is None and state['cancelled_tick'] is None
                assert r['snapshot_tick'] == tick and job is not None
                if state['phase'] == 'ready':
                    assert r['ranked']
                    if seat not in ready:
                        assert state['completed_tick'] == tick
                        ready[seat] = state
                    assert state['completed_tick'] == ready[seat]['completed_tick']
                else:
                    assert state['phase'] == 'pending' and not r['ranked'] and state['completed_tick'] is None
            assert state['completed_tick'] == (ready[seat]['completed_tick'] if seat in ready else None)
            states[seat], snapshots[seat] = state, r
    assert schedule['charged_graph'] == sum(charges.values())
    assert schedule['submitted'] == schedule['cancelled'] == len(states)
    assert schedule['completed'] == len(ready)
    summaries = []
    for seat, source in sources.items():
        if source['rejected']:
            assert source['initial'] is source['last_snapshot'] is source['published'] is source['final_state'] is None
            summaries.append(dict(seat=seat, source_tick=source['source_tick'], rejected=source['rejected']))
            continue
        initial, final = source['initial'], source['last_snapshot']
        audit_report(initial)
        audit_report(final)
        assert initial['charged_graph'] == 0 and not initial['ranked']
        for key in ['actor', 'source_tick', 'current_destination', 'current_selected_tick', 'current_goal',
                    'pre_intent_destination', 'evaluator_source_tick', 'candidates_truncated']:
            assert initial[key] == final[key]
        assert len(initial['candidates']) == len(final['candidates'])
        for a, b in zip(initial['candidates'], final['candidates']):
            assert {k: v for k, v in a.items() if k != 'forecast'} == {k: v for k, v in b.items() if k != 'forecast'}
        assert final['charged_graph'] == charges[seat]
        assert source['last_snapshot_tick'] == snapshots[seat]['snapshot_tick']
        assert source['published_state'] == ready.get(seat)
        assert source['published'] == (final if seat in ready else None)
        state = source['final_state']
        assert state['phase'] == 'stale' and state['charged_graph'] == charges[seat]
        if states[seat]['phase'] == 'stale':
            assert state == states[seat]
        else:
            assert state == dict(states[seat], phase='stale', cancelled_tick=elapsed,
                reason='comparison run ended', validated_tick=None)
        summaries.append(dict(seat=seat, source_tick=source['source_tick'], completed=seat in ready,
            completion_age=ready[seat]['completed_tick']-source['source_tick'] if seat in ready else None,
            cancelled=state['reason'], cancellation_age=state['cancelled_tick']-source['source_tick'],
            charged_graph=charges[seat], candidates=len(final['candidates']),
            unknowns=[dict(destination=c['destination'], current=c['current'], reason=c['unknown']) for c in final['candidates'] if c['unknown']],
            forecast_ends=[c['forecast']['end'] for c in final['candidates'] if c['forecast']],
            fastest_known_handoffs=final['fastest_known_handoffs'], preferred_handoffs=final['preferred_handoffs']))
    return summaries


def unchanged(before, after):
    assert S.trace_digest(before) == S.trace_digest(after), 'ordinary controller/observation trace changed'
    for name in ['mission-evaluations.jsonl', 'flag-survey.jsonl', 'flag-value-shadow.jsonl']:
        assert F.digest(before/name) == F.digest(after/name), name
    a, b = [json.loads((r/'report.json').read_text()) for r in [before, after]]
    assert F.D.same_physical_outcomes(a, b) and a['missions'] == b['missions']
    S.audit_upstream_parity(before, after)
    return dict(full_trace=True, physics=True, evaluations=True, surveys=True, upstream_work=True)


def audit_candidate(f, observation, environment):
    """Independent model-output checks, including new current-route forecasts."""
    assert f['model'] == 'guided_transfer_forecast_v1'
    ticks = f['ticks']
    assert f['end'] in {None, 'kinematic_handoff', 'planet_envelope', 'sun_envelope', 'boundary_envelope',
                        'controller_interrupted', 'match_time_limit', 'horizon', 'non_finite'}
    assert sum(f[k] for k in ['launch_ticks', 'transfer_ticks', 'solar_escape_ticks']) == ticks
    assert all(0 <= f[k] <= ticks for k in ['avoidance_ticks', 'boundary_ticks', 'frame_changes'])
    phases = f['phases']
    assert len(phases) <= 128 and bool(phases) == (ticks > 0)
    if phases:
        assert phases[0]['start_tick'] == 0
        assert all(p['start_tick'] < p['end_tick'] for p in phases)
        assert all(a['end_tick'] == b['start_tick'] for a, b in zip(phases, phases[1:]))
        assert all(p['goal'] in {'launch', 'transfer', 'avoid_sun'} and 0 <= p['frame'] < len(environment['planets']) for p in phases)
        if f['phases_truncated']:
            assert len(phases) == 128 and phases[-1]['end_tick'] < ticks
        else:
            assert phases[-1]['end_tick'] == ticks
            for goal, key in [('launch', 'launch_ticks'), ('transfer', 'transfer_ticks'), ('avoid_sun', 'solar_escape_ticks')]:
                assert sum(p['end_tick']-p['start_tick'] for p in phases if p['goal'] == goal) == f[key]
            assert sum(p['end_tick']-p['start_tick'] for p in phases if p['obstacle'] is not None) == f['avoidance_ticks']
    expected_ticks = list(range(0, ticks+1, 60))
    if f['end'] is not None and expected_ticks[-1] != ticks:
        expected_ticks.append(ticks)
    samples = f['samples']
    assert [s['after_ticks'] for s in samples] == expected_ticks
    pilot = observation['local']['combat']['recovery']['flight']['pilot']
    assert T.f32_identity(samples[0]['ship']) == T.f32_identity(pilot['ship'])
    assert T.f32_identity(samples[0]['gravity']) == T.f32_identity(pilot['gravity'])
    assert samples[0]['frame'] == pilot['planet']['index']
    target = next(p for p in observation['planets'] if p['index'] == f['destination'])
    assert T.f32_identity(samples[0]['target']) == T.f32_identity(target['motion'])
    ephemeris = S.Q.target_ephemeris(environment['planets'][f['destination']], expected_ticks)
    for s in samples:
        for key, tolerance in [('position', .002), ('velocity', .02)]:
            assert S.Q.distance(s['target'][key], ephemeris[s['after_ticks']][key]) < tolerance
        if f['end'] == 'non_finite' and s['after_ticks'] == ticks:
            continue
        for key in ['ship', 'target']:
            assert all(isinstance(s[key][k], (int, float)) and math.isfinite(s[key][k]) for k in ['angle', 'spin'])
        assert all(isinstance(s['gravity'][axis], (int, float)) and math.isfinite(s['gravity'][axis]) for axis in ['x', 'y'])
        for key, field in [('position', 'target_range'), ('velocity', 'target_relative_speed')]:
            assert isinstance(s[field], (int, float)) and math.isfinite(s[field]) and s[field] >= 0
            assert math.isclose(s[field], S.Q.distance(s['ship'][key], s['target'][key]), abs_tol=.002)
    if f['end'] == 'kinematic_handoff':
        assert samples[-1]['frame'] == f['destination']
        assert S.Q.distance(samples[-1]['ship']['position'], samples[-1]['target']['position']) < target['radius']+105
        assert S.Q.distance(samples[-1]['ship']['velocity'], samples[-1]['target']['velocity']) < 18
        assert f['minimum_planet_clearance'] > 0 and f['minimum_boundary_clearance'] > 0
        assert f['minimum_sun_clearance'] is None or f['minimum_sun_clearance'] > 0
    if f['end'] == 'horizon':
        assert ticks == 3600


def audit_sources(case, schedule, normal):
    assert [(s['seat'], s['source_tick']) for s in schedule['sources']] == [(s['seat'], s['tick']) for s in case['sources']]
    for source in schedule['sources']:
        seat, tick = source['seat'], source['source_tick']
        control = next(r for r in normal if r['seat'] == seat and r['tick'] == tick)
        observation, mission = control['observation'], control['mission']
        environment = source['environment']
        assert environment['tick'] == tick and len(environment['planets']) == len(observation['planets'])
        for body, planet in zip(environment['planets'], observation['planets']):
            assert T.f32_identity(body['motion']) == T.f32_identity(planet['motion'])
            assert T.f32_identity(body['radius']) == T.f32_identity(planet['radius'])
        assert T.f32_identity(environment['sun'][0] if environment['sun'] else None) == T.f32_identity(observation['sun'])
        r = source['initial']
        if r is None:
            assert source['rejected']
            continue
        assert r['actor'] == f'player_{seat+1}' and r['source_tick'] == tick
        assert r['current_destination'] == mission['target'] and r['current_goal'] == mission['goal']
        before = next(row for row in normal if row['seat'] == seat and row['tick'] == tick-1)
        assert r['pre_intent_destination'] == before['mission']['target']
        selections = [e['tick'] for row in normal if row['seat'] == seat and row['tick'] <= tick
                      for e in row['mission']['events'] if e['kind'] == 'selected' and e['planet'] == mission['target']]
        assert r['current_selected_tick'] == max(selections)
        alternatives = sorted(p['index'] for p in observation['planets'] if p['index'] != mission['target']
            and (p['claim'] is None or p['claim']['owner'] != r['actor']))
        assert r['candidates_truncated'] == (len(alternatives) > 2)
        assert [c['destination'] for c in r['candidates']] == [mission['target']]+alternatives[:2]
        assert r['candidates'][0]['source_actions'] == control['actions']
        for snapshot in [r, source['last_snapshot']]:
            for c in snapshot['candidates']:
                if c['forecast'] is not None:
                    audit_candidate(c['forecast'], observation, environment)


def audit_reference(reference, case, root, schedule):
    """Old interventions verify source epochs and hypotheses, not later play."""
    normal = list(C.read_trace(root))
    audit_sources(case, schedule, normal)
    compared = []
    for c in case['references']:
        old_root = reference/c['name']
        old = json.loads((old_root/'report.json').read_text())['transfer_probe']['forecast']
        prior = list(C.read_trace(old_root, end=c['source_tick']))
        assert [r for r in normal if r['tick'] < c['source_tick']] == [r for r in prior if r['tick'] < c['source_tick']]
        at = lambda rows: next(r for r in rows if r['seat'] == c['seat'] and r['tick'] == c['source_tick'])
        assert at(normal)['observation'] == at(prior)['observation']
        source = next(s for s in schedule['sources'] if s['seat'] == c['seat'])
        assert T.f32_identity(source['environment']) == T.f32_identity(old['environment'])
        if source['initial'] is None:
            compared.append(dict(name=c['name'], source_exact=True, rejected=source['rejected']))
            continue
        assert source['initial']['candidates'][0]['source_actions'] == at(normal)['actions']
        candidate = next((c2 for c2 in source['last_snapshot']['candidates'] if c2['destination'] == c['destination']), None)
        if candidate is None:
            compared.append(dict(name=c['name'], source_exact=True, shortlisted=False))
            continue
        assert candidate['source_actions'] == old['source_actions']
        forecast = candidate['forecast']
        assert forecast is not None
        expected = old['report']
        if forecast['end']:
            assert T.f32_identity(forecast) == T.f32_identity(expected)
        else:
            assert forecast['ticks'] < expected['ticks']
            assert T.f32_identity(forecast['samples']) == T.f32_identity([s for s in expected['samples'] if s['after_ticks'] <= forecast['ticks']])
        compared.append(dict(name=c['name'], source_exact=True, shortlisted=True,
            completed=forecast['end'] is not None, exact_forecast_or_sample_prefix=True))
    return compared


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--baseline-binary', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    opts = parser.parse_args()
    opts.binary, opts.baseline_binary = [p.resolve(strict=True) for p in [opts.binary, opts.baseline_binary]]
    previous = json.loads((opts.reference/'summary.json').read_text())
    cases = plan(previous['previous_plan']+previous['holdout_plan'])
    for case in previous['previous_plan']+previous['holdout_plan']:
        root = opts.reference/case['name']
        assert F.digest(root/'report.json') == previous['runs'][case['name']]['report_sha256']
        assert S.trace_digest(root) == previous['runs'][case['name']]['trace_sha256']
    opts.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=cases, allowances=ALLOWANCES, max_age_ticks=120, baselines={}, runs={},
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()),
        binary_sha256=F.digest(opts.binary), baseline_binary_sha256=F.digest(opts.baseline_binary),
        reference_summary_sha256=F.digest(opts.reference/'summary.json'),
        scope='21 unique fixed sources from all25 prior nominations, grouped into13 ordinary physical trials. Frozen ordinary baseline plus shared32/64 comparison observer. Existing playing planners stay4. No controls changed, travel handoffs only, no capture costs/value, no source reselection or tuning, no Pi or match-strength claim.')
    save = lambda: F.D.write(opts.out/'summary.json', result)
    save()  # Complete fixed plan before physical outcomes.
    for case in cases:
        args = T.probe_args(case['references'][0])[:-6]
        assert not any('probe-transfer' in a for a in args)
        name = case['name']+'-ordinary'
        run = F.D.run(opts.baseline_binary, opts.out, name, args, 0, seconds=case['seconds'])
        baseline = opts.out/name
        run['trace_sha256'] = C.archive(baseline)
        run['raw_hashes'] = {p.name: F.digest(p) for p in baseline.iterdir() if p.is_file()}
        result['baselines'][name] = run
        save()
        for allowance in ALLOWANCES:
            name = case['name']+f'-budget{allowance}'
            sources = ','.join(f"{s['seat']}:{s['tick']}" for s in case['sources'])
            run = F.D.run(opts.binary, opts.out, name, args+['--compare-transfer-sources', sources,
                '--transfer-comparison-allowance', str(allowance)], 0, seconds=case['seconds'])
            root = opts.out/name
            run.update(allowance=allowance, trace_sha256=C.archive(root))
            run['unchanged'] = unchanged(baseline, root)
            report = json.loads((root/'report.json').read_text())
            schedule = report['transfer_comparison']
            run['comparisons'] = audit_schedule(schedule, T.rows(root/'transfer-comparison-work.jsonl'), S.audit_upstream(root), report['elapsed_ticks'])
            run['reference_comparisons'] = audit_reference(opts.reference, case, root, schedule)
            run['timing'] = {key: schedule[key] for key in ['observation', 'dispatch']}
            run['raw_hashes'] = {p.name: F.digest(p) for p in root.iterdir() if p.is_file()}
            result['runs'][name] = run
            save()
            print(name, run['comparisons'], flush=True)
    assert F.digest(opts.binary) == result['binary_sha256'] and F.digest(opts.baseline_binary) == result['baseline_binary_sha256']
    result['results'] = {}
    for allowance in ALLOWANCES:
        rs = [r for r in result['runs'].values() if r['allowance'] == allowance]
        cs = [c for r in rs for c in r['comparisons']]
        result['results'][str(allowance)] = dict(sources=len(cs), completed=sum(c.get('completed', False) for c in cs),
            fully_ranked=sum(bool(c.get('preferred_handoffs')) for c in cs),
            cancelled=dict(Counter(c.get('cancelled', c.get('rejected')) for c in cs)),
            charged_graph=sum(c.get('charged_graph', 0) for c in cs),
            completion_ages=[c['completion_age'] for c in cs if c.get('completed')],
            max_dispatch_ms=max(r['timing']['dispatch']['max_ms'] for r in rs),
            max_observation_ms=max(r['timing']['observation']['max_ms'] for r in rs))
    save()


if __name__ == '__main__':
    main()
