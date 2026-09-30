#!/usr/bin/env python3
"""Fixed first-choice joins; no earlier-source backfill or live cost consumer."""
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

SPEC = importlib.util.spec_from_file_location('neutral', Path(__file__).with_name('compare-neutral-capture-timing.py'))
N = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(N)
R, Q, P, T, F = N.R, N.Q, N.P, N.T, N.F
L, M = R.L, R.L.M


def strip_join(value):
    if isinstance(value, dict):
        return {k: strip_join(v) for k,v in value.items()
                if k not in ['neutral_timing', 'neutral_timing_costs', 'neutral_validation']}
    if isinstance(value, list): return [strip_join(v) for v in value]
    return value


def clean_report(report, compatibility):
    result = copy.deepcopy(report)
    if compatibility:
        result = strip_join(result)
        result['transfer_comparison'].pop('observation')
        result['transfer_comparison'].pop('dispatch')
    else: result.pop('transfer_comparison', None)
    return Q.without_wall_times(result)


def parity(before, after, compatibility):
    count, size, digest = 0, 0, hashlib.sha256()
    with gzip.open(before/'trace.jsonl.gz', 'rb') as a, (after/'trace.jsonl').open('rb') as b:
        for old,new in itertools.zip_longest(a,b):
            assert old == new, 'full controller trace changed'
            count += 1
            size += len(old)
            digest.update(old)
    for name in P.UPSTREAM + ['transfer-probe.jsonl', 'mission-evaluation-work.jsonl']:
        if (before/name).exists(): assert F.digest(before/name) == F.digest(after/name), name
    assert R.live_work(before) == R.live_work(after)
    reports = [json.loads((root/'report.json').read_text()) for root in [before,after]]
    assert clean_report(reports[0],compatibility) == clean_report(reports[1],compatibility), 'old report changed'
    if compatibility:
        a,b = [list(T.rows(root/'transfer-comparison-work.jsonl')) for root in [before,after]]
        assert Q.without_wall_times(a) == Q.without_wall_times(strip_join(b)), 'old comparison work changed'
    return dict(rows=count,decompressed_bytes=size,trace_sha256=digest.hexdigest(),
                sensors=Q.audit_sensors(before,after),full_reports=True,upstream=True)


def audit_composition(report):
    M.audit_report(report)
    L.audit_composition(report)
    costs = report['neutral_timing_costs']
    assert len(costs) == (len(report['candidates']) if report['ranked'] else 0)
    for c in report['candidates']:
        n = c['neutral_timing']
        r = n['record']
        if r:
            assert c['current'] and c['source_capture'] == 'current_approach'
            assert r['source_tick'] == report['source_tick'] and r['planet']['index'] == c['destination']
            assert n['unknown'] == r['unknown']
        else: assert n['unknown']
    for c,total in zip(report['candidates'],costs):
        n = c['neutral_timing']
        assert total == dict(destination=c['destination'],source_tick=report['source_tick'],
            travel_seconds=0.0 if c['source_capture']=='current_approach' else None,
            source_tick_total_seconds=n['record']['total_seconds'] if n['record'] else None,unknown=n['unknown'])


def audit_immutable_source(source):
    initial=source['initial']
    if initial is None:
        assert source['last_snapshot'] is None and source['published'] is None
        return
    original=[c['neutral_timing'] for c in initial['candidates']]
    for key in ['initial','last_snapshot','published']:
        report=source[key]
        if report:
            audit_composition(report)
            assert [c['neutral_timing'] for c in report['candidates']]==original, 'neutral source record changed'


def current_reason(record, origin, row):
    """Independent eligibility from raw post-intent rows; no clock renewal."""
    o,m,p = row['observation'],row['mission'],P.pilot(row)
    local,old = o['local'],origin['observation']['local']
    if any(local[k] != old[k] for k in ['sun','planet_orbit_omega']): return 'neutral solar environment changed'
    c = m['capture']
    visit = [e['tick'] for e in m['events'] if e['kind']=='selected' and e['planet']==m['target']]
    if (m['policy'] != record['policy'] or m['goal'] != 'capture' or m['target'] != record['planet']['index']
        or m['recovery'] is not None or not c or c['policy'] != 'tactical_sortie_v11'
        or (visit[-1] if visit else None) != record['visit_tick'] or c['started_tick'] != record['started_tick']
        or m['completed_sorties'] != record['completed_sorties'] or R.retry_counts(c) != record['attempt_counters']
        or any(c[k] is not None for k in ['failed_tick','failure','completed_tick'])
        or any(c['landing'][k] is not None for k in ['landed_tick','claimed_tick','boarded_tick'])
        or c['site'] != record['selected']['site']): return 'neutral capture attempt changed'
    planet = p['planet']
    if (any(planet[k] != record['planet'][k] for k in ['index','revision','radius','claim'])
        or not N.finite(planet['motion']) or [q for q in o['planets'] if q['index']==planet['index']] != [planet]):
        return 'neutral material or claim changed'
    query = p['site_query']
    sites = [s for s in p['sites'] if s['id']==record['selected']['site']]
    if (not p['queries_ready'] or query not in ['survey',dict(selected=record['selected']['site'])]
        or len(p['sites'])>64 or len(sites)!=1): return 'neutral selected site unmeasured'
    site = sites[0]
    if (site['id']['planet'] != planet['index'] or site['revision'] != planet['revision']
        or not all(N.finite(site[k]) for k in ['local_position','position','normal','velocity','vehicle_position','hatch_position'])
        or Q.dot(Q.vec(site['normal']),Q.vec(site['normal'])) < .0001
        or not all(N.finite(h) for h in site['boarding_hatches'] if h is not None)
        or not any(h is not None for h in site['boarding_hatches'])
        or math.dist(Q.vec(site['local_position']),Q.vec(record['site']['local_position'])) > .002):
        return 'neutral selected material or hatch changed'
    exposed,distance = N.exposure(row)
    if local['combat']['target'] and (not N.finite(local['combat']['target']['motion']) or not N.finite(distance)):
        return 'neutral opponent geometry unavailable'
    if exposed: return 'neutral capture exposed to opponent'
    if c['goal'] not in ['seek_cover','approach']: return 'neutral approach phase unavailable'
    if c['solar'] != record['selected']['solar']: return 'neutral selected direction or source solar plan changed'
    solar = Q.solar_plan(local,site,record['selected']['side'],circling=c['goal']=='seek_cover')
    if (solar is None) != (record['selected']['solar'] is None): return 'neutral current solar assessment unavailable or unsafe'
    if solar and any(solar[k] < -Q.CLEARANCE_TOLERANCE for k in ['approach_clearance','parked_clearance','departure_clearance']):
        return 'neutral current solar assessment unavailable or unsafe'
    return None


def audit_validation(validation, record, origin, row):
    assert current_reason(record,origin,row) is None
    assert validation['tick'] == row['tick']
    circling = row['mission']['capture']['goal']=='seek_cover'
    assert validation['circling'] == circling
    site = next(s for s in P.pilot(row)['sites'] if s['id']==record['selected']['site'])
    solar = Q.solar_plan(row['observation']['local'],site,record['selected']['side'],circling=circling)
    actual = validation['solar']
    assert (solar is None)==(actual is None)
    ambiguity = []
    if solar:
        assert actual['departure_side'] in [-1,1]
        assert actual['forecast_tick'] == row['tick'] and record['selected']['solar']['forecast_tick']==record['source_tick']
        for k in ['side','surface_seconds']: assert actual[k] == solar[k]
        Q.close(actual['arrival_seconds'],solar['arrival_seconds'],Q.SCORE_TOLERANCE)
        for k in ['approach_clearance','parked_clearance','departure_clearance']:
            Q.close(actual[k],solar[k],Q.CLEARANCE_TOLERANCE)
            assert actual[k]>=0
            if abs(solar[k])<=Q.CLEARANCE_TOLERANCE: ambiguity.append(k)
        if abs(solar['departure_corridors'][0]-solar['departure_corridors'][1]) > 2*Q.CLEARANCE_TOLERANCE:
            assert actual['departure_side']==solar['departure_side']
    return ambiguity


def audit_neutral_cancellation(record, origin, row, reason):
    expected=current_reason(record,origin,row)
    if expected==reason: return []
    if expected is None and reason=='neutral current solar assessment unavailable or unsafe':
        site=next(s for s in P.pilot(row)['sites'] if s['id']==record['selected']['site'])
        solar=Q.solar_plan(row['observation']['local'],site,record['selected']['side'],
            circling=row['mission']['capture']['goal']=='seek_cover')
        ambiguous=[k for k in ['approach_clearance','parked_clearance','departure_clearance']
            if solar and abs(solar[k])<=Q.CLEARANCE_TOLERANCE]
        assert ambiguous, 'solar cancellation lacks an unsafe or unresolved-sign witness'
        return ambiguous
    raise AssertionError((reason,expected))


def audit_terminal_reason(source, origin, row):
    """An observed endpoint is not by itself evidence for a cancellation label."""
    state=source['final_state']
    reason=state['reason']
    assert state['cancelled_tick']==row['tick']
    o,p,m=row['observation'],P.pilot(row),row['mission']
    old,prior=origin['observation'],P.pilot(origin)
    if reason=='comparison run ended': return
    if reason.startswith('neutral'):
        record=source['initial']['candidates'][0]['neutral_timing']['record']
        assert record
        audit_neutral_cancellation(record,origin,row,reason)
    elif reason=='source expired': assert row['tick']-source['source_tick']==121
    elif reason=='planet material or ownership changed':
        def identity(planet):
            c=planet['claim']
            return [planet[k] for k in ['index','radius','revision']]+[None if c is None else
                [c['owner'],c['flag']['player'] if c['flag'] else None,c['captures'],c['neutralizations']]]
        assert [identity(q) for q in old['planets']] != [identity(q) for q in o['planets']]
    elif reason=='ship or pilot changed':
        assert (any(p[k]!=prior[k] for k in ['ship_available','ship_form','location','ship_health'])
                or not p['controls_armed'] or not N.finite(p['ship_health']) or p['ship_health']<=0)
    elif reason=='unassisted flight ended':
        land=p['landing']
        assert land['phase']!='flying' or land['supported_feet']!=0 or land['assist_strength']!=0 or not o['local']['combat']['recovery']['flight']['flight']['enabled']
    elif reason=='physical contact':
        d=row['landing_diagnostics']
        if not (d['hull']['count']>0 or any(f['count']>0 for f in d['feet'])):
            # The host also sees remembered debris contacts. These traces do
            # not archive that damage-observation field; do not invent it.
            return 'contact subtype unverified: no retained solver witness'
    elif reason=='match ended':
        assert o['match_context'] and (o['match_context']['finished'] or not all(o['match_context']['pilots_alive']))
    elif reason=='transfer changed':
        c,prior_c=m['capture'],origin['mission']['capture']
        assert (m['target']!=origin['mission']['target'] or m['goal'] not in ['launch','transfer','avoid_sun','capture']
            or m['recovery'] is not None or m.get('pursuit') is not None
            or (c is None)!=(prior_c is None) or (c and (c['site']!=prior_c['site']
                or c['landing']['landed_tick'] is not None or c['failure'] is not None or c['goal'] not in ['survey','seek_cover','approach'])))
    else: raise AssertionError(f'unmeasured terminal cancellation: {reason}')


def audit_schedule(root, report, rows, expected, planned_sources):
    schedule = report['transfer_comparison']
    work = list(T.rows(root/'transfer-comparison-work.jsonl'))
    assert [(s['seat'],s['source_tick']) for s in schedule['sources']]==[(s['seat'],s['tick']) for s in planned_sources]
    if expected is None:
        assert len(schedule['sources'])==1 and not schedule['sources'][0]['attempted']
        assert all(schedule['sources'][0][key] is None for key in ['initial','last_snapshot','published',
            'published_state','final_state','environment','rejected','last_snapshot_tick'])
        assert not work and all(schedule[k]==0 for k in ['submitted','completed','cancelled','charged_graph'])
        return dict(no_choice=True,validations=0,sources=schedule['sources'])
    M.audit_sources(dict(sources=planned_sources),schedule,rows)
    # Reconcile the shared residual after all existing playing consumers.
    upstream = []
    for row in T.rows(root/'flag-value-shadow-work.jsonl'):
        upstream.append(dict(total=4-row['remaining_after_flag_survey']['graph']+row['charged']['graph']))
    lookup = {(r['seat'],r['tick']):r for r in rows}
    terminal_observed = all((s['seat'],report['elapsed_ticks']) in lookup
        for s in schedule['sources'] if s['final_state'] and s['final_state']['cancelled_tick']==report['elapsed_ticks'])
    unverified_terminal=[]
    if terminal_observed:
        for s in schedule['sources']:
            if s['final_state'] and s['final_state']['cancelled_tick']==report['elapsed_ticks']:
                unknown=audit_terminal_reason(s,lookup[s['seat'],s['source_tick']],lookup[s['seat'],report['elapsed_ticks']])
                if unknown: unverified_terminal.append(dict(seat=s['seat'],tick=report['elapsed_ticks'],unknown=unknown))
    summaries = M.audit_schedule(strip_join(schedule),strip_join(work),upstream,report['elapsed_ticks'],terminal_observed=terminal_observed)
    validations, ambiguities, records = 0,[],{}
    for source in schedule['sources']:
        audit_immutable_source(source)
        if not source['initial']: continue
        current = source['initial']['candidates'][0]['neutral_timing']['record']
        if isinstance(expected,dict): assert current == expected
        else: assert current is None, 'earlier or old choice was backfilled'
        if current and current['total_seconds'] is not None:
            records[source['seat']] = current
    first_ready={}
    for dispatch in work:
        for actor in dispatch['actors']:
            seat,state = actor['seat'],actor['state']
            if not state: continue
            if state['phase']=='ready': first_ready.setdefault(seat,state)
            if seat not in records or state['phase']=='stale':
                assert state.get('neutral_validation') is None
                continue
            record = records[seat]
            ambiguity = audit_validation(state['neutral_validation'],record,lookup[seat,record['source_tick']],lookup[seat,dispatch['tick']])
            validations += 1
            if ambiguity: ambiguities.append(dict(seat=seat,tick=dispatch['tick'],components=ambiguity))
    for source in schedule['sources']:
        assert source['published_state']==first_ready.get(source['seat'])
        state = source['final_state']
        if state: assert state.get('neutral_validation') is None
        if state and state['reason'].startswith('neutral'):
            record = records[source['seat']]
            ambiguity=audit_neutral_cancellation(record,lookup[source['seat'],record['source_tick']],
                lookup[source['seat'],state['cancelled_tick']],state['reason'])
            if ambiguity: ambiguities.append(dict(seat=source['seat'],tick=state['cancelled_tick'],components=ambiguity,cancellation=True))
    return dict(no_choice=False,validations=validations,solar_ambiguities=ambiguities,
                unverified_terminal=unverified_terminal,summaries=summaries,sources=schedule['sources'])


def plan(neutral, compatibility):
    cases = []
    for family, runs in [('historical',neutral['runs']),('fresh',neutral['fresh_runs'])]:
        for name,run in runs.items():
            if family=='fresh' and not name.endswith('-on'): continue
            probe = run['probe']
            source = probe['capture_followthrough']['source']
            command = run['command']
            seat = int(command[command.index('--probe-transfer-seat' if family=='historical' else '--probe-native-capture-seat')+1])
            cases.append(dict(name=family+'-'+name, family=family,command=command,hashes=run['files'],
                expected=source['neutral_timing']['report'] if source else None,
                sources=f"{seat}:{source['pilot']['tick'] if source else 3601}",enabled=True))
    assert len(cases)==36 and sum(c['expected'] is not None for c in cases)==35
    for name,run in compatibility['runs'].items():
        for enabled in [False,True]:
            cases.append(dict(name='compat-'+name+('-on' if enabled else '-off'),family='compatibility',
                command=run['command'],hashes=run['raw_hashes'],expected=False,sources=None,enabled=enabled))
    assert len(cases)==62
    return cases


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--neutral',type=Path,required=True)
    parser.add_argument('--compatibility',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--binary',type=Path,default=Path('target/release/examples/surface_mission_soak'))
    opts=parser.parse_args()
    opts.binary=opts.binary.resolve(strict=True)
    archives=[json.loads((root/'summary.json').read_text()) for root in [opts.neutral,opts.compatibility]]
    cases=plan(*archives)
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze before physics'
    opts.out.mkdir(parents=True,exist_ok=False)
    result=dict(schema=1,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.digest(opts.binary),reference_hashes=[F.digest(root/'summary.json') for root in [opts.neutral,opts.compatibility]],
        plan=cases,runs={},allowance=64)
    save=lambda:F.D.write(opts.out/'summary.json',result)
    save()
    for case in cases:
        command=case['command'].copy()
        before=Path(command[command.index('--out')+1])
        for name,digest in case['hashes'].items(): assert F.digest(before/name)==digest
        root=opts.out/case['name']
        command[0]=str(opts.binary)
        command[command.index('--out')+1]=str(root)
        if case['sources']: command+=['--compare-transfer-sources',case['sources'],'--transfer-comparison-allowance','64']
        command+=['--compare-neutral-timing',str(case['enabled']).lower()]
        with (opts.out/(case['name']+'.log')).open('w') as log: subprocess.run(command,check=True,stdout=log,stderr=log,timeout=900)
        report=json.loads((root/'report.json').read_text())
        same=parity(before,root,case['family']=='compatibility')
        # Keep only the bounded queue lifetime while checking the whole trace byte-for-byte.
        slots=report['transfer_comparison']['sources']
        wanted={(s['seat'],t) for s in slots for t in range(s['source_tick']-1,min(s['source_tick']+122,report['elapsed_ticks']+1))}
        with (root/'trace.jsonl').open() as stream:
            rows=[]
            for line in stream:
                row=json.loads(line)
                if (row['seat'],row['tick']) in wanted: rows.append(row)
        specification=command[command.index('--compare-transfer-sources')+1]
        planned_sources=[dict(zip(['seat','tick'],map(int,s.split(':')))) for s in specification.split(',')]
        joined=audit_schedule(root,report,rows,case['expected'],planned_sources) if case['enabled'] else dict(disabled=True)
        budget=R.audit_budget(root,report)
        trace=R.C.archive(root)
        assert trace==same['trace_sha256']
        result['runs'][case['name']]=dict(command=command,parity=same,join=joined,budget=budget,
            charged_graph=report['transfer_comparison']['charged_graph'],elapsed_ticks=report['elapsed_ticks'],
            files={p.name:F.digest(p) for p in sorted(root.iterdir()) if p.is_file()},log_sha256=F.digest(opts.out/(case['name']+'.log')))
        save()
        print(case['name'],report['transfer_comparison']['charged_graph'],[(s['rejected'],(s['final_state'] or {}).get('reason')) for s in slots],flush=True)
    assert F.digest(opts.binary)==result['binary_sha256']
    result['results']=dict(runs=len(result['runs']),physical_ticks=sum(r['elapsed_ticks'] for r in result['runs'].values()),
        validations=sum(r['join'].get('validations',0) for r in result['runs'].values()),
        comparison_graph=sum(r['charged_graph'] for r in result['runs'].values()),
        cancellations=dict(Counter(s['final_state']['reason'] for r in result['runs'].values()
            for s in r['join'].get('sources',[]) if s['final_state'])))
    save()


if __name__=='__main__': main()
