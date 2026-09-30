#!/usr/bin/env python3
"""Fixed source-local contract replay. No new source selection or live control."""
import argparse
from collections import Counter
import importlib.util
import json
import math
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location('comparison', Path(__file__).with_name('compare-transfer-destinations.py'))
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)
C, T, F, S = M.C, M.T, M.F, M.S
PHASES = ('landing', 'exit', 'outbound', 'claim', 'return_board', 'departure')


def close(a, b):
    assert math.isfinite(a) and math.isfinite(b) and math.isclose(a, b, abs_tol=0.0001), (a, b)


def vector(v):
    return v['x'], v['y']


def local_flag(p):
    c = p['claim']
    if not c or not c['flag']: return None
    f, motion = c['flag'], p['motion']
    dx, dy = [a-b for a, b in zip(vector(f['position']), vector(motion['position']))]
    a = -motion['angle']
    return f['player'], (dx*math.cos(a)-dy*math.sin(a), dx*math.sin(a)+dy*math.cos(a))


def same_planet(a, b):
    def identity(p):
        c = p['claim']
        return (p['index'], p['revision'], p['radius'], c is not None,
                c['owner'] if c else None, c['stage_required_seconds'] if c else None,
                c['flag_interaction_range'] if c else None)
    if identity(a) != identity(b) or not math.isfinite(a['radius']) or a['radius'] <= 0: return False
    fa, fb = local_flag(a), local_flag(b)
    return fa == fb if fa is None or fb is None else fa[0] == fb[0] and math.dist(fa[1], fb[1]) <= .00205


def choice_clock(normal, seat, source):
    """Reconstruct from actual continuously observed choices, never report times."""
    clock = None
    for row in normal:
        if row['seat'] != seat or row['tick'] > source: continue
        tick, mission, local = row['tick'], row['mission'], row['observation']['local']
        p, capture = local['combat']['recovery']['flight']['pilot'], mission['capture']
        visits = [e['tick'] for e in mission['events'] if e['kind'] == 'selected' and e['planet'] == mission['target']]
        visit = visits[-1] if visits else None
        if (capture is None or capture['site'] is None or visit is None or visit > tick
            or mission['goal'] != 'capture' or mission['target'] != p['planet']['index']
            or capture['site']['planet'] != p['planet']['index'] or capture['landing']['landed_tick'] is not None
            or capture['failure'] is not None or capture['goal'] not in ['survey', 'seek_cover', 'approach']):
            clock = None
            continue
        if (clock is None or tick-clock['last'] > 1 or clock['visit'] != visit or clock['site'] != capture['site']
            or not same_planet(clock['planet'], p['planet']) or abs(clock['gravity']-local['objective_gravity']) > .010001):
            clock = dict(first=tick, visit=visit, site=capture['site'], planet=p['planet'], gravity=local['objective_gravity'])
        clock['last'] = tick
    return clock['first'] if clock else None


def walking_reference(survey, site, stage):
    routes = [r for r in survey['sites'][:8] if r['site'] == site]
    assert routes, 'selected walking route missing from original measurement'
    route = routes[0]
    assert not route.get('crossing') and route['returning'] is not None
    for leg in [route['outbound'], route['returning']]:
        assert leg['failure'] is None and not leg['partial'] and leg['start_node'] is not None
        assert leg['reachable_nodes'] > 0 and leg['destination_nodes'] > 0
        assert leg['jumps'] == leg['flights'] == 0 and math.isfinite(leg['length']) and leg['length'] >= 0
    outbound, returning = route['outbound']['length']/5, route['returning']['length']/5
    assert outbound <= 10.599818 and returning <= 10.118013 and abs(stage-3) <= .001
    return dict(zip(PHASES, [23.033333, 1/60, outbound+.15833333, 6-.5/60, returning+2/60, 3.8166666]))


def audit_lifetime(source, rows):
    if source['initial'] is None: return
    seat, tick = source['seat'], source['source_tick']
    o = rows[seat, tick]['observation']
    refs = [c['local_reference'] for c in source['initial']['candidates'] if c['local_reference']['remaining']]
    evidence = [r['evidence'] for r in refs]
    def valid(now):
        local = now['local']
        p = local['combat']['recovery']['flight']['pilot']
        return (len(o['planets']) == len(now['planets']) and all(same_planet(a, b) for a, b in zip(o['planets'], now['planets']))
            and all(p['tick'] <= min(e['tick'], e['route_source_tick'] if e['route_source_tick'] is not None else e['tick'])+1800 for e in evidence)
            and all(e['remote'] or (p['planet']['index'] == e['site']['planet'] and abs(local['objective_gravity']-e['gravity']) <= .010001) for e in evidence))
    state = source['final_state']
    for t in range(tick, state['cancelled_tick']):
        assert valid(rows[seat, t]['observation']), ('local dependency escaped cancellation', seat, t)
    if state['reason'] == 'local reference dependencies changed or expired':
        assert not valid(rows[seat, state['cancelled_tick']]['observation'])


def audit_local(r):
    assert r['model'] == 'source_local_reference_v1'
    assert 0 <= r['visit_tick'] <= r['source_tick']
    assert r['elapsed_landing_ticks'] >= 0
    e, full, remaining = r['evidence'], r['full'], r['remaining']
    if e is not None:
        assert e['site']['planet'] == r['destination']
        assert 0 <= e['tick'] <= r['source_tick'] and e['age_ticks'] == r['source_tick']-e['tick'] <= 1800
        assert e['radius'] > 0 and e['flag_range'] > 0
        if e['route_objective']:
            assert 0 <= e['route_source_tick'] <= e['route_validated_tick'] <= e['tick']
            assert r['source_tick']-e['route_source_tick'] <= 1800
            obj = e['route_objective']
            assert obj['planet'] == e['site']['planet'] and obj['revision'] == e['revision']
            assert obj['owner'] == e['observed_owner']
            close(obj['range'], e['flag_range'])
    if remaining is None:
        assert full is None and r['unknown']
        assert r['elapsed_landing_ticks'] == 0
        return
    assert e and full and r['unknown'] is None
    for costs in [full, remaining]:
        assert set(costs) == set(PHASES)
        assert all(math.isfinite(v) and v >= 0 for v in costs.values())
    elapsed = r['elapsed_landing_ticks']
    if r['observed_choice_tick'] is not None:
        assert r['visit_tick'] <= r['observed_choice_tick'] <= r['source_tick']
        assert r['selected_site'] == e['site']
        assert elapsed == r['source_tick']-r['observed_choice_tick']
    else:
        assert elapsed == 0
    for phase in PHASES:
        close(remaining[phase], max(0, full[phase]-elapsed/60) if phase == 'landing' else full[phase])


def audit_composition(report):
    for c in report['candidates']:
        r = c['local_reference']
        assert r['source_tick'] == report['source_tick'] and r['destination'] == c['destination']
        audit_local(r)
        if c['source_capture'] is not None:
            assert c['source_capture'] == ('current_approach' if c['current'] else 'nominated_approach')
            assert c['forecast'] is None and c['unknown']
            if not c['current']:
                assert c['nomination']['accepted'] and r['visit_tick'] == report['source_tick']
                assert r['observed_choice_tick'] is None and r['elapsed_landing_ticks'] == 0
    costs = report['capture_costs']
    if not report['ranked']:
        assert costs == []
        return
    assert len(costs) == len(report['candidates'])
    for c, total in zip(report['candidates'], costs):
        assert total['destination'] == c['destination']
        r, f = c['local_reference'], c['forecast']
        travel = 0.0 if c['source_capture'] else f['handoff_seconds'] if f and f['end'] == 'kinematic_handoff' else None
        local = sum(r['remaining'].values()) if r['remaining'] else None
        components = travel+local if travel is not None and local is not None else None
        for key, value in [('travel_seconds', travel), ('local_seconds', local), ('known_components_seconds', components)]:
            if value is None: assert total[key] is None
            else: close(total[key], value)
        bound = c['source_capture'] == 'current_approach' and r['observed_choice_tick'] is not None and r['evidence'] and r['selected_site'] == r['evidence']['site']
        if components is not None and bound:
            close(total['remaining_trip_seconds'], components)
            assert total['unknown'] is None
        else:
            assert total['remaining_trip_seconds'] is None and total['unknown']


def audit_evidence(schedule, normal, remote_rows):
    rows = {(r['seat'], r['tick']): r for r in normal}
    for source in schedule['sources']:
        if source['initial'] is None: continue
        report, seat, tick = source['initial'], source['seat'], source['source_tick']
        o = rows[seat, tick]['observation']
        p = o['local']['combat']['recovery']['flight']['pilot']
        for c in report['candidates']:
            r, destination = c['local_reference'], c['destination']
            e = r['evidence']
            if c['source_capture']:
                assert p['queries_ready'] and p['planet']['index'] == destination
                if c['current']:
                    m = rows[seat, tick]['mission']
                    assert m['goal'] == 'capture' and m['capture']['site'] == r['selected_site']
                else:
                    close_to = math.dist(vector(p['ship']['position']), vector(p['planet']['motion']['position']))
                    assert close_to < p['planet']['radius']+105
                    assert math.dist(vector(p['ship']['velocity']), vector(p['planet']['motion']['velocity'])) < 18
            if r['observed_choice_tick'] is not None:
                assert c['source_capture'] == 'current_approach'
                assert r['observed_choice_tick'] == choice_clock(normal, seat, tick)
            if e is None: continue
            planet = next(v for v in o['planets'] if v['index'] == destination)
            assert e['revision'] == planet['revision'] and e['observed_owner'] == planet['claim']['owner']
            close(e['radius'], planet['radius'])
            close(e['stage_seconds'], planet['claim']['stage_required_seconds'])
            close(e['flag_range'], planet['claim']['flag_interaction_range']-.2)
            if r['remaining'] and planet['claim']['flag']:
                assert all(e[key] is not None for key in ['route_objective', 'route_source_tick', 'route_validated_tick']), 'numeric flagged reference requires original route provenance'
            if e['remote']:
                assert planet['claim']['flag'] is None and planet['claim']['owner'] is None
                matches = [c2['measurement'] for row in remote_rows if row['actor'] == seat and row['tick'] <= tick
                    for c2 in row['evidence']['candidates'] if c2['id'] == e['site'] and c2['measurement'] and c2['measurement']['tick'] == e['tick']]
                assert matches, 'remote reference lacks original measurement'
                if r['remaining']:
                    assert any(v['finding'] == 'measured' and v['climb_clear'] and v['site']
                        and any(v['site']['boarding_hatches']) and v['revision'] == e['revision'] for v in matches)
            else:
                assert p['planet']['index'] == destination
                measurement = rows[seat, e['tick']]['observation']['local']
                mp = measurement['combat']['recovery']['flight']['pilot']
                assert mp['planet']['index'] == destination
                close(e['gravity'], measurement['objective_gravity'])
                assert abs(e['gravity']-o['local']['objective_gravity']) <= .010001
                assert any(s['id'] == e['site'] and any(s['boarding_hatches']) for s in mp['sites'])
                assert any(v['site'] == e['site'] and all(v[k] for k in ['grounded', 'approach', 'departure']) for v in measurement['cover'])
                if e['route_objective']:
                    survey = measurement['landing_objective']
                    assert T.f32_identity(e['route_objective']) == T.f32_identity(survey['objective'])
                    assert e['route_source_tick'] == survey['tick'] and e['route_validated_tick'] == (survey['validated_tick'] or survey['tick'])
                    flag = local_flag(planet)
                    assert flag and flag[0] == e['route_objective']['owner'] == planet['claim']['owner']
                    assert math.dist(flag[1], vector(e['route_objective']['position'])) <= .00205
                    if r['remaining']:
                        assert survey['version'] == 1 and survey['actor'] == report['actor']
                        expected = walking_reference(survey, e['site'], e['stage_seconds'])
                        for key in PHASES: close(r['full'][key], expected[key])
            if r['remaining'] and planet['claim']['flag'] is None:
                for key, value in zip(PHASES, [17.866667, 1/60, 4/60, 3+1/60, 2/60, 3.766667]): close(r['full'][key], value)
        audit_lifetime(source, rows)


def forecast_parity(old, new):
    assert len(old['sources']) == len(new['sources'])
    counts = Counter()
    for a, b in zip(old['sources'], new['sources']):
        assert (a['seat'], a['source_tick'], a['environment']) == (b['seat'], b['source_tick'], b['environment'])
        if b['initial'] is None:
            assert b['rejected']; counts['rejected'] += 1; continue
        assert a['initial'] is not None
        for key in ['source_tick', 'actor', 'current_destination', 'current_selected_tick', 'current_goal', 'pre_intent_destination', 'evaluator_source_tick', 'candidates_truncated']:
            assert a['initial'][key] == b['initial'][key]
        for x, y in zip(a['last_snapshot']['candidates'], b['last_snapshot']['candidates']):
            for key in ['destination', 'current', 'nomination', 'source_actions', 'unknown']: assert x[key] == y[key]
            xf, yf = x['forecast'], y['forecast']
            if yf is None: assert xf is None; counts['unknown'] += 1
            elif yf['end'] is not None:
                assert T.f32_identity(xf) == T.f32_identity(yf); counts['complete_exact'] += 1
            else:
                assert yf['ticks'] < xf['ticks']
                assert T.f32_identity(yf['samples']) == T.f32_identity([s for s in xf['samples'] if s['after_ticks'] <= yf['ticks']])
                phases = [dict(p, end_tick=min(p['end_tick'], yf['ticks'])) for p in xf['phases'] if p['start_tick'] < yf['ticks']]
                assert yf['phases'] == phases
                counts['partial_exact_prefix'] += 1
    return dict(counts)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    previous = json.loads((opts.reference/'summary.json').read_text())
    assert len(previous['plan']) == 13 and sum(len(c['sources']) for c in previous['plan']) == 21
    opts.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=previous['plan'], allowance=64, runs={},
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()),
        binary_sha256=F.digest(opts.binary), reference_summary_sha256=F.digest(opts.reference/'summary.json'),
        scope='Fixed prior21sources/13ordinary trials, shared64, unchanged playing4. Source-local conditional components; unknown acquisition interval retained. No live selection, tuning, deployment, strength or hardware claim.')
    save = lambda: F.D.write(opts.out/'summary.json', result)
    save()
    for case in result['plan']:
        name = case['name']+'-budget64'
        baseline = opts.reference/name
        for file, digest in previous['runs'][name]['raw_hashes'].items(): assert F.digest(baseline/file) == digest
        args = T.probe_args(case['references'][0])[:-6]
        sources = ','.join(f"{s['seat']}:{s['tick']}" for s in case['sources'])
        run = F.D.run(opts.binary, opts.out, name, args+['--compare-transfer-sources', sources, '--transfer-comparison-allowance', '64'], 0, seconds=case['seconds'])
        root = opts.out/name
        run['trace_sha256'] = C.archive(root)
        run['unchanged'] = M.unchanged(baseline, root)
        report = json.loads((root/'report.json').read_text())
        schedule = report['transfer_comparison']
        run['comparisons'] = M.audit_schedule(schedule, T.rows(root/'transfer-comparison-work.jsonl'), S.audit_upstream(root), report['elapsed_ticks'])
        run['forecast_parity'] = forecast_parity(json.loads((baseline/'report.json').read_text())['transfer_comparison'], schedule)
        normal = list(C.read_trace(root))
        M.audit_sources(case, schedule, normal)
        audit_evidence(schedule, normal, T.rows(root/'destination-cover.jsonl'))
        del normal
        for source in schedule['sources']:
            for key in ['initial', 'last_snapshot', 'published']:
                if source[key]: audit_composition(source[key])
        run['source_references'] = [dict(seat=s['seat'], source_tick=s['source_tick'],
            candidates=s['initial']['candidates'] if s['initial'] else [],
            capture_costs=s['published']['capture_costs'] if s['published'] else [],
            final_state=s['final_state']) for s in schedule['sources']]
        # Source records retain only the new contract, not duplicate trajectories.
        for s in run['source_references']:
            for c in s['candidates']: c.pop('forecast')
        run['timing'] = {key: schedule[key] for key in ['observation', 'dispatch']}
        run['raw_hashes'] = {p.name: F.digest(p) for p in root.iterdir() if p.is_file()}
        result['runs'][name] = run
        save()
        print(name, run['forecast_parity'], flush=True)
    assert F.digest(opts.binary) == result['binary_sha256']
    refs = [s for r in result['runs'].values() for s in r['source_references']]
    cs = [c for s in refs for c in s['candidates']]
    totals = [c for s in refs for c in s['capture_costs']]
    result['results'] = dict(sources=len(refs), candidates=len(cs), completed=sum(bool(s['capture_costs']) for s in refs),
        local_numeric=sum(c['local_reference']['remaining'] is not None for c in cs),
        local_unknowns=dict(Counter(c['local_reference']['unknown'] for c in cs if c['local_reference']['unknown'])),
        source_capture=dict(Counter(c['source_capture'] for c in cs if c['source_capture'])),
        numeric_component_sums=sum(c['known_components_seconds'] is not None for c in totals),
        remaining_trip_references=sum(c['remaining_trip_seconds'] is not None for c in totals),
        composition_unknowns=dict(Counter(c['unknown'] for c in totals if c['unknown'])),
        cancellations=dict(Counter(s['final_state']['reason'] for s in refs if s['final_state'])))
    save()
    print(json.dumps(result['results'], indent=2))


if __name__ == '__main__': main()
