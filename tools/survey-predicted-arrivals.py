#!/usr/bin/env python3
"""Paired, observational arrival-directed terrain surveys on fixed sources."""
import argparse
from collections import Counter, defaultdict
import copy
import csv
import hashlib
import json
from pathlib import Path
import subprocess
import importlib.util


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


B = module('body_motion', 'compare-transfer-body-motion.py')
D = module('destination_comparison', 'compare-transfer-destinations.py')
A, F, C, T, Q = B.A, B.F, B.C, B.T, B.Q


def audit_row(row, state, forecast, pilot, remaining, busy, last):
    tick, seat, plan = row['tick'], row['seat'], row['plan']
    assert state['phase'] == 'ready' and state['validated_tick'] == tick
    assert plan['token'] == state['token'] and plan['token']['actor'] == seat
    assert plan['source_tick'] == state['source_tick'] == forecast['source_tick']
    assert plan['completed_tick'] == state['completed_tick'] < tick <= plan['source_tick']+120
    assert forecast['end'] == 'kinematic_handoff'
    sample = forecast['samples'][-1]
    assert sample['after_ticks'] == forecast['ticks']
    assert plan['forecast_model'] == forecast['model'] == 'guided_transfer_forecast_v1'
    assert plan['predicted_arrival_tick'] == plan['source_tick']+forecast['ticks']
    expected = dict(planet=forecast['destination'], bearing=B.radial_bearing(sample['ship'], sample['target'])['nearest_bin'])
    assert plan['site'] == expected
    assert row['remaining_before'] == dict(graph=0, physics_queries=remaining)
    request = plan['request']
    blocked = ('local landing demand' if pilot['site_query'] != 'not_requested' or pilot['landing']['supported_feet'] > 0
               else 'material queries unavailable' if not pilot['queries_ready'] else None)
    assert plan['deferred'] == blocked
    if request:
        assert blocked is None
        assert plan['completed_tick'] < request['generation'] <= tick
        assert request['candidates'] == [expected,None,None,None] and request['sample_climb']
    else:
        assert blocked
    reason = (blocked or ('earlier physical work' if seat in busy else
        'refresh interval' if seat in last and tick < last[seat]+30 else
        'query budget' if remaining < 192 else None))
    assert row['deferred'] == reason
    if reason:
        assert row['allocation'] is None and row['evidence'] == []
        return None, 0
    allocation = row['allocation']
    assert allocation['allowance'] == row['remaining_before']
    used = allocation['charged']['physics_queries']
    assert allocation['charged']['graph'] == 0 and 0 <= used <= min(192, remaining)
    assert sum(j['charged']['physics_queries'] for j in allocation['jobs']) == used
    assert all(j['request']['actor'] == seat and j['charged']['graph'] == 0 for j in allocation['jobs'])
    assert len(row['evidence']) == 1 and row['evidence'][0][0] == seat
    evidence = row['evidence'][0][1]
    assert evidence['generation'] == request['generation'] and len(evidence['candidates']) == 1
    candidate = evidence['candidates'][0]
    assert candidate['id'] == expected
    measurement = candidate['measurement']
    if used:
        assert measurement and measurement['tick'] == tick and measurement['queries'] == used
        assert request['generation'] <= measurement['tick']
        assert candidate['status'] == measurement['finding'] in ['measured','no_landing','incomplete']
        if measurement['site']:
            assert measurement['site']['id'] == expected
        last[seat] = tick
    else:
        assert measurement is None
    return measurement, used


def sensor_digest(root):
    digest = hashlib.sha256()
    for row in T.rows(root/'sensors.jsonl'):
        digest.update((json.dumps(Q.without_wall_times(row),sort_keys=True,separators=(',',':'))+'\n').encode())
    return digest.hexdigest()


def audit_survey(root, report, case, controlled):
    schedule = report['transfer_comparison']
    # Existing consumers remain separately audited, including their graph work.
    if controlled:
        D.audit_schedule(schedule, T.rows(root/'transfer-comparison-work.jsonl'), D.S.audit_upstream(root), report['elapsed_ticks'], terminal_observed=True)
    else:
        A.R.audit_work(root, report, 16)
    rows = list(T.rows(root/'arrival-survey.jsonl'))
    work = {r['tick']:r for r in T.rows(root/'transfer-comparison-work.jsonl')}
    live_queries, busy = {}, defaultdict(set)
    for r in csv.DictReader((root/'live-planning.csv').open()):
        tick = int(r['tick'])
        assert live_queries.setdefault(tick,int(r['total_queries'])) == int(r['total_queries'])
        if int(r['queries']): busy[tick].add(int(r['actor']))
    flags = {r['tick']:r for r in T.rows(root/'flag-survey-work.jsonl')}
    for tick,row in flags.items():
        busy[tick].update(j['request']['actor'] for j in row['allocation']['jobs'] if j['charged']['physics_queries'])
    observed = {(r['tick'],r['seat']):r for r in C.read_trace(root)}
    last, spent, measurements, reasons, requests = {}, Counter(), [], Counter(), {}
    seen = set()
    for row in rows:
        tick, seat = row['tick'],row['seat']
        assert (tick,seat) not in seen
        seen.add((tick,seat))
        state = next(a['state'] for a in work[tick]['actors'] if a['seat'] == seat)
        source = next(s for s in schedule['sources'] if s['seat'] == seat)
        published = source['published']
        current = observed[tick,seat]
        eligible = []
        for candidate in published['candidates']:
            planet = next(p for p in current['observation']['planets'] if p['index'] == candidate['destination'])
            claim, forecast = planet['claim'],candidate['forecast']
            if claim and claim['owner'] is None and claim['flag'] is None and forecast and forecast['end'] == 'kinematic_handoff':
                eligible.append(candidate)
        candidate = min(eligible,key=lambda c:(not c['current'],c['destination']))
        leftover = 384-live_queries.get(tick,0)-flags[tick]['allocation']['charged']['physics_queries']-spent[tick]
        measurement, used = audit_row(row,state,candidate['forecast'],A.P.pilot(current),leftover,busy[tick],last)
        request = row['plan']['request']
        if request:
            assert requests.setdefault(seat, request) == request, 'generation changed within an uninterrupted eligible window'
        spent[tick] += used
        reasons[row['deferred'] or 'attempt'] += 1
        if measurement:
            planet = next(p for p in current['observation']['planets'] if p['index'] == row['plan']['site']['planet'])
            assert measurement['revision'] == planet['revision']
            assert T.f32_identity(measurement['planet']) == T.f32_identity(planet['motion'])
            entry = dict(seat=seat,plan=row['plan'],measurement=measurement)
            if controlled:
                probe = report['transfer_probe']
                choice_tick = probe['acquisition']['outcome']['tick']
                choice = observed[choice_tick,seat]
                fresh = next((s for s in A.P.pilot(choice)['sites'] if s['id'] == row['plan']['site']),None)
                controls = [observed[t,seat] for t in range(tick,choice_tick+1)]
                changes = A.identity_changes(controls,row['plan']['site']['planet'])
                selected = probe['landing_choice']['report']['selected']['site']
                valid = A.age_and_identity_valid(tick,choice_tick,changes)
                entry['retrospective'] = dict(choice_tick=choice_tick,selected=selected,
                    covers_selected=selected==row['plan']['site'],age_ticks=choice_tick-tick,identity=changes,
                    age_and_identity_valid=valid,
                    geometry=A.geometry_residual(A.project(measurement,A.P.pilot(choice)['planet']['motion']),fresh)
                        if valid and measurement['site'] is not None else None)
            measurements.append(entry)
    assert sum(spent.values()) == schedule['arrival_survey']['physics_queries']
    assert len(measurements) == schedule['arrival_survey']['attempts']
    # Log coverage itself must be dense throughout each eligible Ready window,
    # including local/budget/refresh deferrals and ending on invalidation.
    for source in schedule['sources']:
        if source['published']:
            start = source['published_state']['completed_tick']+1
            end = source['final_state']['cancelled_tick']
            assert [t for t,s in sorted(seen) if s == source['seat']] == list(range(start,end))
    return dict(rows=len(rows),reasons=dict(reasons),queries=sum(spent.values()),measurements=measurements,
        maximum_combined_queries=max((spent[t]+live_queries.get(t,0)+flags[t]['allocation']['charged']['physics_queries'] for t in flags),default=0))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--ordinary',type=Path,default=Path('target/capture-flag-survey/remote-retention-v1'))
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code and plan first'
    paths = [Path('docs/data/'+p) for p in ['capture-remote-retention-v1.json',
        'capture-followthrough-v1.json','capture-site-acquisition-v1.json','capture-arrival-replay-v1.json']]
    retained, physical, acquisition, previous = [json.loads(p.read_text()) for p in paths]
    assert F.digest(args.ordinary/'summary.json') == retained['raw_summary_sha256']
    specs = A.plan(retained,physical)
    assert specs == previous['plan']
    for spec in specs:
        root = args.ordinary/(spec['retained_run']+'-on')
        for file,digest in retained['pairs'][spec['retained_run']]['runs'][1]['hashes'].items():
            assert F.digest(root/file) == digest
    args.out.mkdir(parents=True,exist_ok=False)
    binary = args.binary.resolve(strict=True)
    commands = {}
    for spec in specs:
        case = spec['case']
        for kind in ['ordinary','controlled']:
            name = case['name']+'-'+kind
            base = (retained['pairs'][spec['retained_run']]['runs'][1]['command'] if kind=='ordinary'
                    else spec['command']+['--compare-transfer-sources',f"{case['seat']}:{case['source_tick']}",
                        '--transfer-comparison-allowance','64','--compare-neutral-timing','true'])
            commands[name] = {}
            for arm in ['off','on']:
                command = [str(binary)]+base[1:]
                command[command.index('--out')+1] = str(args.out/(name+'-'+arm))
                commands[name][arm] = command+['--survey-predicted-arrival',str(arm=='on').lower()]
    result = dict(schema=1,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.digest(binary),input_hashes={str(p):F.digest(p) for p in paths},
        plan=specs,commands=commands,pairs={},scope='Four correlated ordinary sources and their four controlled full capture replays, each survey off/on. Existing point forecasts, observations, controls, evidence and budgets must match. New physical measurements use only residual query quota and never enter playing state or prior sources. Retrospective choice comparisons do not authorize cost admission or predict capture success.')
    def save(): (args.out/'summary.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    save()
    for spec in specs:
        case = spec['case']
        old = args.ordinary/(spec['retained_run']+'-on')
        for kind in ['ordinary','controlled']:
            name = case['name']+'-'+kind
            pair = dict(runs={})
            result['pairs'][name] = pair
            for arm in ['off','on']:
                root,command = args.out/(name+'-'+arm),commands[name][arm]
                log_path = args.out/(root.name+'.log')
                try:
                    with log_path.open('w') as log:
                        subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,check=True)
                    report = json.loads((root/'report.json').read_text())
                    if kind == 'controlled':
                        historical = A.audit_replay(spec,root,old,physical,acquisition,retained['pairs'][spec['retained_run']])
                        previous_source = json.loads((old/'report.json').read_text())['transfer_comparison']['sources'][0]
                        old_forecast = next(c['forecast'] for c in previous_source['published']['candidates'] if c['destination'] == case['destination'])
                        current_source = report['transfer_comparison']['sources'][0]
                        if current_source['published']:
                            current_forecast = next(c['forecast'] for c in current_source['published']['candidates'] if c['destination'] == case['destination'])
                            assert T.f32_identity(current_forecast) == T.f32_identity(old_forecast), 'existing point forecast changed'
                    else:
                        C.archive(root)
                        historical = D.unchanged(old,root)
                        assert sensor_digest(old) == sensor_digest(root)
                        before = json.loads((old/'report.json').read_text())['transfer_comparison']
                        now = copy.deepcopy(report['transfer_comparison']); now.pop('arrival_survey',None)
                        assert A.R.stable_schedule(before) == A.R.stable_schedule(now)
                    run = dict(command=command,historical=historical,trace_sha256=D.S.trace_digest(root),
                        sensor_sha256=sensor_digest(root),log_sha256=F.digest(log_path))
                    pair['runs'][arm] = run
                    if arm == 'on':
                        off = args.out/(name+'-off')
                        pair['parity'] = D.unchanged(off,root)
                        assert pair['runs']['off']['sensor_sha256'] == run['sensor_sha256']
                        before = json.loads((off/'report.json').read_text())['transfer_comparison']
                        now = copy.deepcopy(report['transfer_comparison']); now.pop('arrival_survey')
                        assert A.R.stable_schedule(before) == A.R.stable_schedule(now)
                        assert [Q.without_wall_times(r) for r in T.rows(off/'transfer-comparison-work.jsonl')] == [Q.without_wall_times(r) for r in T.rows(root/'transfer-comparison-work.jsonl')]
                        pair['survey'] = audit_survey(root,report,case,kind=='controlled')
                    run['hashes'] = {p.name:F.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
                    print(name,arm,'audited',flush=True)
                    save()
                except Exception as error:
                    result['error'] = dict(case=name,arm=arm,error=repr(error),command=command,
                        log_sha256=F.digest(log_path) if log_path.exists() else None)
                    save()
                    raise
    measurements = [m for p in result['pairs'].values() for m in p['survey']['measurements']]
    result['aggregate'] = dict(pairs=len(result['pairs']),runs=sum(len(p['runs']) for p in result['pairs'].values()),
        attempts=len(measurements),queries=sum(p['survey']['queries'] for p in result['pairs'].values()),
        findings=dict(Counter(m['measurement']['finding'] for m in measurements)),
        cover_native_choice=sum(m.get('retrospective',{}).get('covers_selected',False) for m in measurements),
        age_and_identity_valid=sum(m.get('retrospective',{}).get('age_and_identity_valid',False) for m in measurements))
    save()


if __name__ == '__main__': main()
