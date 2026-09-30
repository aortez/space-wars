#!/usr/bin/env python3
"""Regenerate the fixed 13 source comparisons, paired with remote screening."""
import argparse
from collections import Counter
import copy
import hashlib
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


M = module('transfer_comparison', 'compare-transfer-destinations.py')
L = module('landing_choice', 'compare-landing-choices.py')
R = module('local_composition', 'compose-capture-references.py')
C, T, F, S = M.C, M.T, M.F, M.S


def strip_screen(report):
    r = copy.deepcopy(report)
    for c in r['candidates']:
        screen = c.pop('remote_arrival', None)
        if screen:
            r['charged_graph'] -= screen['charged_graph']
    return r


def audit_screen(screen, source, candidate, row, history):
    tick, dest = source['source_tick'], candidate['destination']
    o = row['observation']
    assert screen['source_tick'] == tick and screen['destination'] == dest
    assert screen['model'] == 'remote_arrival_screen_v1'
    assert screen['acquisition'] == 'requires fresh native survey; choice and duration unknown'
    assert screen['future_threat'] == 'unmodeled; source cover and opponent are historical only'
    assert screen['source_planet'] == next(p for p in o['planets'] if p['index'] == dest)
    remote = source['remote_source']
    assert screen['generation'] == (remote['generation'] if remote else None)
    if screen['sites']:
        expected = [c for c in remote['candidates'] if c['id']['planet'] == dest]
        assert [s['source'] for s in screen['sites']] == expected
    assert len(screen['sites']) <= 4
    directions, unresolved, departure_ties, maximum_error = 0, 0, 0, 0.0
    arrival = screen['arrival']
    if arrival:
        if candidate['source_capture']:
            p = o['local']['combat']['recovery']['flight']['pilot']
            assert arrival['tick'] == tick and arrival['ship'] == p['ship'] and arrival['planet'] == p['planet']
        else:
            forecast = candidate['forecast']
            assert forecast['end'] == 'kinematic_handoff'
            assert arrival['tick'] == tick + forecast['ticks']
            last = forecast['samples'][-1]
            assert arrival['ship'] == last['ship'] and arrival['planet']['motion'] == last['target']
        orbit = source['environment']['planets'][dest]['orbit']
        assert arrival['planet_orbit_omega'] == (orbit['rate'] if orbit else None)
        assert arrival['sun'] == o['local']['sun']
    for site in screen['sites']:
        m = site['source']['measurement']
        if m:
            assert m['tick'] <= tick
            witnesses = [c['measurement'] for r in history if r['actor'] == source['seat'] and r['tick'] < tick
                         and r['evidence']['generation'] == remote['generation']
                         for c in r['evidence']['candidates'] if c['id'] == site['source']['id']]
            assert any(T.f32_identity(w) == T.f32_identity(m) for w in witnesses), 'source sample lacks prior measurement'
        projected = site['projected']
        if projected is None:
            assert not site['directions']
            assert site['unknown'] or screen['unknown'] or arrival is None
            continue
        assert site['unknown'] is None and arrival and m
        age = arrival['tick'] - m['tick']
        assert site['arrival_age_ticks'] == age <= 1800
        assert m['finding'] == 'measured' and m['climb_clear'] is True
        assert m['ship_form'] == 'ship' and m['revision'] == arrival['planet']['revision']
        old, motion = m['site'], arrival['planet']['motion']
        assert old['id'] == projected['id'] and old['revision'] == projected['revision']
        assert old['local_position'] == projected['local_position']
        angle = L.f32(L.f32(motion['angle']) - L.f32(m['planet']['angle']))
        def point(v):
            return L.add(L.vec(motion['position']), L.rotate(L.sub(L.vec(v), L.vec(m['planet']['position'])), angle))
        for field in ['position', 'vehicle_position', 'hatch_position']:
            assert math.dist(point(old[field]), L.vec(projected[field])) < .002
        assert math.dist(L.rotate(L.vec(old['normal']), angle), L.vec(projected['normal'])) < .002
        for a, b in zip(old['boarding_hatches'], projected['boarding_hatches']):
            assert (a is None) == (b is None)
            if a: assert math.dist(point(a), L.vec(b)) < .002
        dx, dy = L.sub(L.vec(projected['vehicle_position']), L.vec(motion['position']))
        velocity = L.add(L.vec(motion['velocity']), L.mul((-dy, dx), L.f32(motion['spin'])))
        assert math.dist(velocity, L.vec(projected['velocity'])) < .002
        local = copy.deepcopy(o['local'])
        local['sun'], local['planet_orbit_omega'] = arrival['sun'], arrival['planet_orbit_omega']
        local['combat']['recovery']['flight']['pilot'].update(tick=arrival['tick'], ship=arrival['ship'], planet=arrival['planet'])
        for i, d in enumerate(site['directions']):
            assert d['side'] == [-1, 1][i]
            predicted = L.solar_plan(local, projected, d['side'])
            actual = d['solar']
            if predicted is None:
                assert actual is None and d['solar_clear']
            else:
                assert actual['forecast_tick'] == arrival['tick']
                assert actual['side'] == d['side'] and actual['departure_side'] in [-1, 1]
                left,right = predicted['departure_corridors']
                if abs(left-right) > .02:
                    assert actual['departure_side'] == predicted['departure_side']
                else:
                    departure_ties += 1
                for k in ['arrival_seconds', 'surface_seconds', 'approach_clearance', 'parked_clearance', 'departure_clearance']:
                    error = abs(actual[k]-predicted[k])
                    maximum_error = max(maximum_error, error)
                    assert error <= .01, (k, actual, predicted)
                clearances = [predicted[k] for k in ['approach_clearance', 'parked_clearance', 'departure_clearance']]
                uncertain = any(abs(v) <= .01 for v in clearances)
                unresolved += uncertain
                if not uncertain: assert d['solar_clear'] == all(v >= 0 for v in clearances)
            directions += 1
    assert directions == screen['charged_graph'] <= 8
    if screen['complete']:
        assert all(s['projected'] is None or len(s['directions']) == 2 for s in screen['sites'])
    return dict(directions=directions, uncertain=unresolved, departure_ties=departure_ties, maximum_solar_error=maximum_error)


def audit_pair(off_root, on_root, archived):
    parity = M.unchanged(off_root, on_root)
    parity['sensors'] = L.audit_sensors(off_root, on_root)
    assert S.trace_digest(off_root) == archived['trace_sha256'], 'default controller trace differs from committed archive'
    reports = [json.loads((p/'report.json').read_text()) for p in [off_root, on_root]]
    old, new = [r['transfer_comparison'] for r in reports]
    assert [(s['seat'],s['source_tick']) for s in old['sources']] == [(s['seat'],s['source_tick']) for s in new['sources']]
    normal = list(C.read_trace(on_root))
    rows = {(r['seat'], r['tick']): r for r in normal}
    history = T.rows(on_root/'destination-cover.jsonl')
    ledger = T.rows(on_root/'transfer-comparison-work.jsonl')
    upstream = S.audit_upstream(on_root)
    old_summary = M.audit_schedule(old, T.rows(off_root/'transfer-comparison-work.jsonl'), S.audit_upstream(off_root), reports[0]['elapsed_ticks'])
    assert old_summary == archived['comparisons'], 'default queue differs from committed archive'
    charges, extra_before = Counter(), Counter()
    for row in ledger:
        prior = upstream[row['tick']]['total']
        assert row['playing_charged_graph'] == prior
        allocation = row['allocation']
        assert row['remaining_before_comparison'] == allocation['allowance'] == dict(graph=64-prior, physics_queries=0)
        assert allocation['charged']['physics_queries'] == 0 and allocation['charged']['graph'] <= 64-prior
        assert allocation['charged']['graph'] == sum(j['charged']['graph'] for j in allocation['jobs'])
        for j in allocation['jobs']:
            assert j['charged']['physics_queries'] == 0
            charges[j['request']['actor']] += j['charged']['graph']
        for actor in row['actors']:
            seat, state = actor['seat'], actor['state']
            if not state: continue
            assert state['charged_graph'] == charges[seat]
            extra = charges[seat]-sum(c['charged_graph'] for c in actor['candidates'])-int(actor['ranked'])
            assert extra_before[seat] <= extra <= 8
            extra_before[seat] = extra
            if state['phase'] != 'stale': assert state['validated_tick'] == row['tick']
    assert new['charged_graph'] == sum(charges.values())
    sources, counts, maximum_error = [], Counter(), 0.0
    for a, b in zip(old['sources'], new['sources']):
        assert (a['seat'], a['source_tick'], a['environment']) == (b['seat'], b['source_tick'], b['environment'])
        assert a['rejected'] == b['rejected']
        if b['initial'] is None:
            counts['rejected'] += 1
            sources.append(b)
            continue
        assert strip_screen(b['initial']) == a['initial']
        # Added solar work may delay publication, not alter motor forecasts/costs.
        if b['published']: assert strip_screen(b['published']) == a['published']
        if b['last_snapshot']['ranked']:
            assert strip_screen(b['last_snapshot']) == a['last_snapshot']
        else:
            # Extra work or an older admitted sample can leave a partial job.
            # The existing prefix checker verifies every retained motor sample.
            R.forecast_parity(dict(sources=[a]), dict(sources=[b]))
            for x,y in zip(a['last_snapshot']['candidates'],b['last_snapshot']['candidates']):
                assert {k:v for k,v in x.items() if k != 'forecast'} == {k:v for k,v in y.items() if k not in ['forecast','remote_arrival']}
            assert not b['last_snapshot']['capture_costs']
        final = b['last_snapshot']
        counts['sources'] += 1
        counts['published'] += b['published'] is not None
        for initial, candidate in zip(b['initial']['candidates'], final['candidates']):
            screen = candidate['remote_arrival']
            assert [s['source'] for s in screen['sites']] == [s['source'] for s in initial['remote_arrival']['sites']]
            result = audit_screen(screen, b, candidate, rows[b['seat'], b['source_tick']], history)
            counts['candidates'] += 1
            counts['sampled_sites'] += len(screen['sites'])
            counts['projected_sites'] += sum(s['projected'] is not None for s in screen['sites'])
            counts['directions'] += result['directions']
            counts['uncertain_solar_signs'] += result['uncertain']
            counts['uncertain_departure_order'] += result['departure_ties']
            maximum_error = max(maximum_error, result['maximum_solar_error'])
            counts['solar_clear_directions'] += sum(d['solar_clear'] for s in screen['sites'] for d in s['directions'])
            counts['screen_unknown:'+str(screen['unknown'])] += screen['unknown'] is not None
            for s in screen['sites']:
                if s['unknown']: counts['site_unknown:'+s['unknown']] += 1
        extra = sum(c['remote_arrival']['charged_graph'] for c in final['candidates'])
        assert b['final_state']['charged_graph'] == sum(c['forecast']['charged_graph'] for c in final['candidates'] if c['forecast'])+extra+int(final['ranked'])
        if b['final_state']['reason'] != 'remote arrival solar context changed or source samples expired':
            assert {k:v for k,v in a['final_state'].items() if k not in ['charged_graph','completed_tick']} == {k:v for k,v in b['final_state'].items() if k not in ['charged_graph','completed_tick']}
        else:
            current = rows[b['seat'],b['final_state']['cancelled_tick']]['observation']['local']
            admitted = [s['source']['measurement']['tick'] for c in b['initial']['candidates']
                        if c['remote_arrival']['unknown'] is None
                        for s in c['remote_arrival']['sites'] if not s['unknown']]
            assert current['sun'] != rows[b['seat'],b['source_tick']]['observation']['local']['sun'] or any(b['final_state']['cancelled_tick']-t > 1800 for t in admitted)
        sources.append(dict(seat=b['seat'], source_tick=b['source_tick'],
            source_remote=b['remote_source'], screens=[c['remote_arrival'] for c in final['candidates']],
            off_completed_tick=a['final_state']['completed_tick'], on_completed_tick=b['final_state']['completed_tick'],
            final_state=b['final_state']))
    return dict(parity=parity, counts=+counts, maximum_solar_error=maximum_error, sources=sources,
                physical_ticks=reports[0]['elapsed_ticks']*2, graph_off=old['charged_graph'], graph_on=new['charged_graph'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, default=Path('docs/data/capture-local-composition-v1.json'))
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'], text=True).strip(), 'freeze code and plan first'
    archive = json.loads(args.reference.read_text())
    assert len(archive['runs']) == 13 and len(archive['plan']) == 13
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
                  binary_sha256=F.digest(binary), reference_sha256=F.digest(args.reference), pairs={})
    save = lambda: (args.out/'summary.json').write_text(json.dumps(result, indent=2)+'\n')
    save()
    for name, previous in archive['runs'].items():
        roots, runs = [], []
        for mode in ['off','on']:
            root = args.out/(name+'-'+mode)
            command = [str(binary)]+previous['command'][1:]
            command[command.index('--out')+1] = str(root)
            command += ['--compare-remote-arrival', str(mode == 'on').lower()]
            with (args.out/(root.name+'.log')).open('w') as log:
                subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True)
            trace = C.archive(root)
            roots.append(root)
            runs.append(dict(command=command, trace_sha256=trace,
                hashes={p.name:F.digest(p) for p in root.iterdir() if p.is_file()}))
        audit = audit_pair(*roots, previous)
        result['pairs'][name] = dict(runs=runs, **audit)
        save()
        print(name, audit['counts'], flush=True)
    assert F.digest(binary) == result['binary_sha256']
    result['complete'] = True
    result['counts'] = dict(sum((Counter(p['counts']) for p in result['pairs'].values()), Counter()))
    result['physical_ticks'] = sum(p['physical_ticks'] for p in result['pairs'].values())
    result['graph_off'] = sum(p['graph_off'] for p in result['pairs'].values())
    result['graph_on'] = sum(p['graph_on'] for p in result['pairs'].values())
    save()
    print(json.dumps(result['counts'],indent=2))


if __name__ == '__main__':
    main()
