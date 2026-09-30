#!/usr/bin/env python3
"""Separate first-choice neutral timing from cover, without changing controls."""
import argparse
from collections import Counter
import copy
import gzip
import hashlib
import importlib.util
import itertools
import json
import math
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location('followthrough', Path(__file__).with_name('probe-capture-followthrough.py'))
R = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(R)
Q, P, T, F = R.Q, R.P, R.T, R.F
LABEL = 'neutral-capture-timing-v1'
CHOICE_TICKS = 3600
PHASE_COSTS = dict(zip(R.PHASES, [17.866667, 1/60, 4/60, 3+1/60, 2/60, 3.766667]))


def fresh_plan():
    return [dict(name=f'fresh{i}-asteroids{interval}-s{seat}', world=i,
                 seed=int.from_bytes(hashlib.sha256(f'{LABEL}/fresh/{i}'.encode()).digest()[:8], 'little'),
                 interval=interval, seat=seat)
            for i in range(4) for interval in [0, 3] for seat in [0, 1]]


def clean_report(report):
    result = copy.deepcopy(report)
    for key in ['transfer_probe', 'native_capture_probe']:
        source = result.get(key, {}).get('capture_followthrough', {}).get('source')
        if source: source.pop('neutral_timing', None)
    return Q.without_wall_times(result)


def full_parity(before, after):
    count, size, digest = 0, 0, hashlib.sha256()
    with gzip.open(before/'trace.jsonl.gz', 'rb') as a, (after/'trace.jsonl').open('rb') as b:
        for old, new in itertools.zip_longest(a, b):
            assert old == new, 'full controller/observation trace changed'
            count += 1
            size += len(old)
            digest.update(old)
    for name in P.UPSTREAM:
        assert F.digest(before/name) == F.digest(after/name), name
    if (before/'transfer-probe.jsonl').exists():
        assert F.digest(before/'transfer-probe.jsonl') == F.digest(after/'transfer-probe.jsonl')
    assert R.live_work(before) == R.live_work(after)
    reports = [json.loads((root/'report.json').read_text()) for root in [before, after]]
    assert clean_report(reports[0]) == clean_report(reports[1]), 'report changed beyond timing/wall times'
    return dict(controller_rows=count, decompressed_bytes=size, trace_sha256=digest.hexdigest(),
                sensors=Q.audit_sensors(before, after), full_reports=True, upstream=True, planner=True)


def finite(value):
    if isinstance(value, dict): return all(finite(v) for v in value.values())
    if isinstance(value, list): return all(finite(v) for v in value)
    return value is not None and math.isfinite(value)


def exposure(row):
    p, target = P.pilot(row), row['observation']['local']['combat']['target']
    def f32(v):
        # Rust f32 arithmetic saturates to infinity; JSON represents it as null.
        if v is None: return math.nan
        try: return Q.f32(v)
        except OverflowError: return math.copysign(math.inf,v)
    distance = None
    if target:
        x,y = [f32(f32(target['motion']['position'][k])-f32(p['ship']['position'][k])) for k in ['x','y']]
        distance = f32(math.sqrt(f32(f32(x*x)+f32(y*y))))
    return bool(target and not target['ground_occluded'] and distance < 300), distance


def domain(row, choice, site):
    o, m, p = row['observation'], row['mission'], P.pilot(row)
    c, local = m['capture'], o['local']
    flight = local['combat']['recovery']['flight']
    claim, tick = p['planet']['claim'], p['tick']
    visit = [e['tick'] for e in m['events'] if e['kind'] == 'selected' and e['planet'] == m['target']]
    if m['policy'] != 'material_mission_v13' or c['policy'] != 'tactical_sortie_v11' or not choice['commit_descent']:
        return 'policy outside timing domain'
    if ([o['version'],local['version'],local['combat']['version'],local['combat']['recovery']['version'],flight['version'],flight['flight']['version'],p['version']] != [1,1,2,1,2,1,1]
        or not p['controls_armed'] or not p['queries_ready'] or not p['ship_available'] or p['ship_form'] != 'ship'
        or not finite(p['ship_health']) or p['ship_health'] <= 0 or p['location'] != dict(aboard=p['vehicle'])):
        return 'source ship unavailable'
    a = c.get('acquisition')
    if (m['goal'] != 'capture' or m['target'] != p['planet']['index'] or m['recovery'] is not None
        or not visit or visit[-1] > tick or c['started_tick'] is None or c['started_tick'] > tick
        or any(c[k] is not None for k in ['failed_tick','failure','completed_tick'])
        or any(c['landing'][k] is not None for k in ['landed_tick','claimed_tick','boarded_tick'])
        or choice['tick'] != tick or choice['planet'] != p['planet']['index'] or choice['revision'] != p['planet']['revision']
        or c['site'] != choice['selected']['site'] or not a or a['tick'] != tick or a['reason'] != 'selected_site'
        or a['planet'] != p['planet']['index'] or a['revision'] != p['planet']['revision'] or a['selected_site'] != c['site']):
        return 'not a fresh capture choice'
    if (len(o['planets']) > 8 or [q for q in o['planets'] if q['index'] == p['planet']['index']] != [p['planet']]
        or not all(finite(v) for v in [p['ship'],p['planet']['motion'],p['gravity'],p['planet']['radius']]) or p['planet']['radius'] <= 0):
        return 'source geometry unavailable'
    if claim is None: return 'ownership unknown'
    if (claim['planet'] != p['planet']['index'] or claim['owner'] is not None or claim['flag'] is not None
        or claim['claimant'] is not None or claim['phase'] != 'idle' or claim['progress'] != 0
        or not finite(claim['stage_required_seconds']) or abs(claim['stage_required_seconds']-3) > .001
        or not finite(claim['flag_interaction_range']) or claim['flag_interaction_range'] <= 0):
        return 'claim state outside neutral timing domain'
    if local['combat']['target'] and (not finite(local['combat']['target']['motion']) or not finite(exposure(row)[1])):
        return 'opponent geometry unavailable'
    if choice['exposed']: return 'source exposed to opponent'
    if site is None: return 'selected material site unavailable'
    if (site['id']['planet'] != p['planet']['index'] or site['revision'] != p['planet']['revision']
        or not all(finite(site[k]) for k in ['local_position','position','normal','velocity','vehicle_position','hatch_position'])
        or Q.dot(Q.vec(site['normal']),Q.vec(site['normal'])) < .0001
        or not all(finite(v) for v in site['boarding_hatches'] if v is not None) or not any(site['boarding_hatches'])):
        return 'selected material or hatch unavailable'
    sun, solar, omega = local['sun'], choice['selected']['solar'], local['planet_orbit_omega']
    if ((omega is not None and not finite(omega)) or (sun is not None and (not finite(sun) or sun['radius'] <= 0 or sun['heat_radius'] < 0))):
        return 'solar geometry unavailable'
    if sun is None and solar is None: return None
    if (sun is None or solar is None or not finite(solar) or solar['forecast_tick'] != tick
        or solar['side'] != choice['selected']['side'] or solar['arrival_seconds'] < 0 or solar['surface_seconds'] <= 0
        or any(solar[k] < 0 for k in ['approach_clearance','parked_clearance','departure_clearance'])):
        return 'selected solar plan unavailable'
    return None


def audit_timing(row, choice, wrapper):
    if choice['report'] is None:
        assert wrapper == dict(report=None,unknown=choice['unknown'])
        return dict(prediction=None, unknown=choice['unknown'])
    assert wrapper['unknown'] is None
    r, p, m = wrapper['report'], P.pilot(row), row['mission']
    selected = choice['report']['selected']
    site = next((s for s in p['sites'] if s['id'] == selected['site']), None)
    for key, value in dict(model='neutral_capture_timing_v1', policy=m['policy'], actor=p['owner'], vehicle=p['vehicle'],
        spaceling=p['spaceling'], source_tick=p['tick'], started_tick=m['capture']['started_tick'],
        completed_sorties=m['completed_sorties'], attempt_counters=R.retry_counts(m['capture']),
        planet=p['planet'],ship=p['ship'],gravity=p['gravity'],site=site,selected=selected,
        opponent=row['observation']['local']['combat']['target'], source_exposed=choice['report']['exposed'],
        cover=next((v for v in row['observation']['local']['cover'] if v['site'] == selected['site']),None)).items():
        assert r[key] == value, key
    visits = [e['tick'] for e in m['events'] if e['kind'] == 'selected' and e['planet'] == m['target']]
    assert r['visit_tick'] == (visits[-1] if visits else None)
    exposed, distance = exposure(row)
    assert exposed == r['source_exposed']
    if distance is None or not math.isfinite(distance): assert r['opponent_distance'] is None
    else: Q.close(r['opponent_distance'],distance,.001)
    unknown = domain(row,choice['report'],site)
    assert r['unknown'] == unknown
    if unknown is not None: assert r['phases'] is None and r['total_seconds'] is None
    else:
        assert set(r['phases']) == set(PHASE_COSTS)
        for k,v in PHASE_COSTS.items(): Q.close(r['phases'][k],v,.000003)
        Q.close(r['total_seconds'],sum(PHASE_COSTS.values()),.00001)
    cover = r['cover']
    return dict(prediction=r['phases'], unknown=unknown, source_exposed=exposed,
        cover='missing' if cover is None else 'all' if all(cover[k] for k in ['grounded','approach','departure'])
        else 'partial' if any(cover[k] for k in ['grounded','approach','departure']) else 'none')


def audit_selected(row, choice):
    """Check the chosen current direction; this is not a full fresh-world rerank."""
    assert choice['physics_queries'] == 0 and finite(choice['assessment_ms']) and choice['assessment_ms'] >= 0
    if choice['report'] is None: return dict(kind='unavailable',reason=choice['unknown'])
    p, m, r = P.pilot(row),row['mission'],choice['report']
    a, c = r['selected'],m['capture']
    assert r['tick'] == row['tick'] == p['tick'] and r['planet'] == p['planet']['index']
    assert r['revision'] == p['planet']['revision'] == a['revision']
    assert a['site'] == c['site'] == c['acquisition']['selected_site']
    assert c['acquisition']['reason'] == 'selected_site' and c['acquisition']['tick'] == p['tick']
    assert a['rejection'] is None and a in r['assessments']
    assert r['site_count'] == len(p['sites']) <= 64 and len(r['assessments']) <= 128
    site = p['sites'][a['site_order']]
    assert site['id'] == a['site'] and site['revision'] == a['revision']
    Q.close(a['short_angle'],Q.short_angle(p,site),.000001)
    expected = Q.solar_plan(row['observation']['local'],site,a['side'])
    ambiguity, error, departure_ambiguous = [],0,False
    assert (expected is None) == (a['solar'] is None)
    assert T.f32_identity(a['solar']) == T.f32_identity(c['solar'])
    if expected:
        assert a['solar']['forecast_tick'] == p['tick'] and a['solar']['side'] == a['side']
        assert a['solar']['surface_seconds'] == expected['surface_seconds']
        Q.close(a['solar']['arrival_seconds'],expected['arrival_seconds'],Q.SCORE_TOLERANCE)
        assert a['solar']['departure_side'] in [-1,1]
        left,right = expected['departure_corridors']
        departure_ambiguous = abs(left-right) <= 2*Q.CLEARANCE_TOLERANCE
        if not departure_ambiguous: assert a['solar']['departure_side'] == expected['departure_side']
        for key in ['approach_clearance','parked_clearance','departure_clearance']:
            Q.close(a['solar'][key],expected[key],Q.CLEARANCE_TOLERANCE)
            error = max(error,abs(a['solar'][key]-expected[key]))
            if abs(expected[key]) <= Q.CLEARANCE_TOLERANCE: ambiguity.append(key)
            else: assert expected[key] >= 0
    assert r['exposed'] == exposure(row)[0]
    return dict(kind='selected_direction',independent_solar_max_error=error,unresolved_clearance_signs=ambiguity,
                unresolved_departure_side=departure_ambiguous)


def read_control(root, seat, report):
    probe = report.get('native_capture_probe',report.get('transfer_probe'))
    outcome = probe['capture_followthrough']['outcome'] or probe.get('no_choice')
    if outcome is None: outcome = probe['acquisition']['outcome'] or probe['outcome']
    observed = outcome.get('controller_observed', outcome.get('observed', outcome['reason'] not in ['match_finished','runner_ended']))
    expected = 2*(report['elapsed_ticks']+int(observed))
    rows, count, size, digest = [], 0, 0, hashlib.sha256()
    with (root/'trace.jsonl').open('rb') as stream:
        for line in stream:
            row = json.loads(line)
            assert (row['tick'],row['seat']) == (count//2,count%2)
            assert P.pilot(row)['tick'] == row['tick']
            assert P.pilot(row)['owner'] == f"player_{row['seat']+1}"
            if row['seat'] == seat: rows.append(row)
            count += 1
            size += len(line)
            digest.update(line)
    assert count == expected
    sensors = list(T.rows(root/'sensors.jsonl'))
    assert len(sensors) == count
    assert all((v['tick'],v['seat']) == (i//2+1,i%2) for i,v in enumerate(sensors))
    return rows,dict(controller_rows=count,decompressed_bytes=size,trace_sha256=digest.hexdigest(),sensor_rows=len(sensors))


def first_choice(rows):
    for row in rows:
        c = row['mission']['capture']
        a = c.get('acquisition') if c else None
        if row['tick'] < CHOICE_TICKS and a and a['reason'] == 'selected_site' and a['tick'] == row['tick']:
            return row
    return None


def audit_trip(report, rows, timing):
    probe = report.get('native_capture_probe',report.get('transfer_probe'))
    cap, choice = probe['capture_followthrough'], probe['landing_choice']
    assert cap['schema'] == 1 and cap['horizon_ticks'] == 7200
    if 'native_capture_probe' in report:
        assert probe['schema'] == 1 and probe['seat'] == rows[0]['seat'] and probe['choice_window_ticks'] == CHOICE_TICKS
        first = first_choice(rows)
        if first is None:
            assert cap['source'] is None and cap['outcome'] is None and cap['observed_rows'] == 0
            assert cap['last_observed_tick'] is None and cap['touchdown'] is None and cap['tactical_completed_tick'] is None
            assert choice is None and all(v is None for v in cap['milestones'].values())
            stop = probe['no_choice']
            reason = stop['reason']
            assert reason == ('no_native_choice' if rows[-1]['tick'] == CHOICE_TICKS else
                              'match_finished' if report['round']['outcome'] else 'runner_ended')
            assert stop['tick'] == report['elapsed_ticks']
            assert stop['observed'] == (reason == 'no_native_choice')
            assert rows[-1]['tick'] == stop['tick']-int(not stop['observed'])
            assert stop['tick'] == CHOICE_TICKS if stop['observed'] else stop['tick'] <= CHOICE_TICKS
            return dict(outcome=reason,old_prediction=None,prediction=None,unknown='no native choice',phases={})
        assert cap['source'] is not None, 'first native choice must retain a capture source'
        assert probe['no_choice'] is None and choice['observation_tick'] == first['tick']
        assert choice['reference'] == first['mission']['capture']['acquisition']['selected_site']
    if cap['source'] is None:
        assert not probe['acquisition']['outcome'] or probe['acquisition']['outcome']['reason'] != 'site_selected'
        assert cap['outcome'] is None and cap['observed_rows'] == 0 and all(v is None for v in cap['milestones'].values())
        return dict(outcome='no_site_choice',old_prediction=None,prediction=None,unknown='no native choice',phases={})
    rows = [row for row in rows if row['tick'] >= cap['milestones']['choice']]
    source, outcome = cap['source'], cap['outcome']
    assert source['pilot'] == P.pilot(rows[0]) and source['mission'] == rows[0]['mission']
    assert choice['actor'] == source['pilot']['owner'] and choice['observation_tick'] == rows[0]['tick']
    assert source['selected'] == (choice['report'] or {}).get('selected') and source['choice_unknown'] == choice['unknown']
    assert cap['observed_rows'] == len(rows) and cap['last_observed_tick'] == rows[-1]['tick']
    assert source['physics_queries'] == 0 and cap['horizon_ticks'] == 7200
    if timing: estimate = audit_timing(rows[0],choice,source['neutral_timing'])
    else:
        assert 'neutral_timing' not in source
        estimate = dict(prediction=None,unknown='timing disabled')
    native = audit_selected(rows[0],choice)
    if outcome['reason'] == 'unsupported_source':
        claim = source['pilot']['planet']['claim']
        assert choice['report'] is None or claim is None or claim['owner'] is not None or claim['flag'] is not None
        assert len(rows) == 1 and outcome['controller_observed'] and outcome['tick'] == rows[0]['tick']
        assert outcome['elapsed_ticks'] == 0 and outcome['tick'] == report['elapsed_ticks']
        assert cap['touchdown'] is None and cap['tactical_completed_tick'] is None
        assert cap['milestones'] == dict.fromkeys(R.NAMES) | dict(choice=rows[0]['tick'])
        if source['reference'] is not None: R.L.audit_local(source['reference'])
        return dict(outcome='unsupported_source',old_prediction=(source['reference'] or {}).get('full'),phases={},native=native,**estimate)
    old = R.cost_reference(source,rows[0])
    milestones, tactical, touchdown, reason = R.reconstruct(source,rows)
    assert cap['milestones'] == milestones and cap['tactical_completed_tick'] == tactical
    expected = reason or ('match_finished' if report['round']['outcome'] else 'runner_ended')
    assert outcome['reason'] == expected and outcome['controller_observed'] == (reason is not None)
    assert outcome['tick'] == rows[-1]['tick']+int(reason is None) == report['elapsed_ticks']
    assert outcome['elapsed_ticks'] == outcome['tick']-rows[0]['tick']
    if touchdown:
        p, origin = P.pilot(touchdown),source['pilot']
        planet = next(q for q in touchdown['observation']['planets'] if q['index'] == origin['planet']['index'])
        site = next(s for s in origin['sites'] if s['id'] == source['selected']['site'])
        expected_pos = Q.rotate(Q.sub(Q.vec(site['vehicle_position']),Q.vec(origin['planet']['motion']['position'])),-origin['planet']['motion']['angle'])
        actual = Q.rotate(Q.sub(Q.vec(p['ship']['position']),Q.vec(planet['motion']['position'])),-planet['motion']['angle'])
        assert cap['touchdown']['tick'] == touchdown['tick'] and cap['touchdown']['ship'] == p['ship']
        for key,value in [('expected_local',expected_pos),('actual_local',actual)]:
            for a,b in zip(Q.vec(cap['touchdown'][key]),value): Q.close(a,b,.002)
        Q.close(cap['touchdown']['offset'],math.dist(expected_pos,actual),.003)
    else: assert cap['touchdown'] is None
    later = [r['tick'] for r in rows[1:] if exposure(r)[0]]
    return dict(outcome=outcome['reason'],old_prediction=old,**estimate,milestones=milestones,native=native,
        elapsed_seconds=outcome['elapsed_ticks']/60,phases=R.phases(milestones,estimate['prediction'],outcome),
        first_later_exposure_tick=later[0] if later else None,later_exposed_rows=len(later))


def aggregate(runs):
    audits = [r['audit'] for r in runs.values()]
    return dict(cases=len(audits),outcomes=dict(Counter(a['outcome'] for a in audits)),
        old_numeric=sum(a['old_prediction'] is not None for a in audits),numeric=sum(a['prediction'] is not None for a in audits),
        unknowns=dict(Counter(a['unknown'] for a in audits if a['prediction'] is None)),
        covers=dict(Counter(a.get('cover','no source') for a in audits)),
        later_exposed=sum(a.get('first_later_exposure_tick') is not None for a in audits),
        physical_ticks=sum(r['elapsed_ticks'] for r in runs.values()),
        completed_errors=[dict(name=n,seconds=sum(a['phases'][k]['error_seconds'] for k in R.PHASES),
                              later_exposed=a['first_later_exposure_tick'] is not None)
                          for n,r in runs.items() if (a:=r['audit'])['outcome'] == 'departed' and a['prediction'] is not None],
        phase_errors={k:[a['phases'][k]['error_seconds'] for a in audits if a['phases'].get(k,{}).get('error_seconds') is not None] for k in R.PHASES})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--binary',type=Path,default=Path('target/release/examples/surface_mission_soak'))
    opts = parser.parse_args()
    opts.binary = opts.binary.resolve(strict=True)
    previous = json.loads((opts.reference/'summary.json').read_text())
    cases, fresh = previous['plan'],fresh_plan()
    assert len(cases) == 20 and len(fresh) == 16
    assert not {c['seed'] for c in fresh} & {c['condition']['seed'] for c in cases}
    opts.out.mkdir(parents=True,exist_ok=False)
    result = dict(schema=1,plan=cases,denominator=previous['denominator'],fresh_plan=fresh,runs={},fresh_runs={},
        choice_window_ticks=CHOICE_TICKS,capture_horizon_ticks=7200,
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        source_dirty=bool(subprocess.check_output(['git','status','--porcelain'],text=True).strip()),
        binary_sha256=F.digest(opts.binary),reference_summary_sha256=F.digest(opts.reference/'summary.json'),
        scope='Stateless existing phase medians at unexposed neutral native choices; cover separate. Same20 full parity against previous executable; four predetermined fresh worlds, both seats, quiet/asteroids, timing off/on. First native choice regardless of admission; no fresh nomination. All interruptions/unknowns/censors retained. Source-only exposure; later exposure observed separately. No fitting, playing cost admission, deployment or strength claim.')
    assert not result['source_dirty'], 'freeze runtime/runner/plan before physics'
    save = lambda:F.D.write(opts.out/'summary.json',result)
    save()

    def audit(root, run, seat, timing, before=None, case=None):
        report = json.loads((root/'report.json').read_text())
        if before: run['parity'] = full_parity(before,root)
        rows,run['trace'] = read_control(root,seat,report)
        run['audit'] = audit_trip(report,rows,timing)
        if case: run['legacy_audit'] = R.audit_capture(case,report,rows)
        run['budget'] = R.audit_budget(root,report)
        run['probe'] = report.get('native_capture_probe',report.get('transfer_probe'))
        run['trace_sha256'] = R.C.archive(root)
        assert run['trace_sha256'] == run['trace']['trace_sha256']
        run['files'] = {p.name:F.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
        run['log_sha256'] = F.digest(root.parent/(root.name+'.log'))
        return run

    for case in cases:
        name = case['name']+'-on'
        before,root = opts.reference/name,opts.out/name
        for f,h in previous['runs'][case['name']]['files'].items(): assert F.digest(before/f) == h
        args = T.probe_args(case)+['--probe-transfer-pursuit','defer_new','--bounded-acquisition-seats','none',
            '--probe-acquisition-seconds','30','--probe-landing-reference-bearing',str(case['candidate']['local_reference']['evidence']['site']['bearing']),
            '--probe-capture-seconds','120','--probe-neutral-timing','true']
        run = F.D.run(opts.binary,opts.out,name,args,case['seat'],seconds=case['source_tick']//60+211)
        result['runs'][case['name']] = audit(root,run,case['seat'],True,before,case)
        save()
    for case in fresh:
        for mode in ['off','on']:
            name = case['name']+'-'+mode
            args = ['--world','generated','--seed',str(case['seed']),'--mode','duel','--match','true','--seat','0',
                '--asteroid-interval',str(case['interval']),'--p1-policy','material_mission_v13','--p2-policy','material_mission_v13',
                '--trace','true','--trace-start-tick','0','--trace-end-tick','10862','--survey-capture-flags','true','--shadow-capture-flags','true',
                '--bounded-acquisition-seats','none','--probe-native-capture-seat',str(case['seat']),
                '--native-choice-seconds','60','--native-capture-seconds','120','--probe-neutral-timing',str(mode=='on').lower()]
            run = F.D.run(opts.binary,opts.out,name,args,case['seat'],seconds=181)
            before = opts.out/(case['name']+'-off') if mode == 'on' else None
            result['fresh_runs'][name] = audit(opts.out/name,run,case['seat'],mode=='on',before)
            save()
    assert F.digest(opts.binary) == result['binary_sha256']
    result['results'] = aggregate(result['runs'])
    result['fresh_results'] = aggregate({n:r for n,r in result['fresh_runs'].items() if n.endswith('-on')})
    save()


if __name__ == '__main__': main()
