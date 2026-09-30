#!/usr/bin/env python3
"""Freeze a fresh source after the first real arrival-survey attempt, empty/measured."""
import argparse
from collections import Counter
import copy
import importlib.util
import json
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location('survey', Path(__file__).with_name('survey-predicted-arrivals.py'))
S = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(S)
A, D, C, T, F, Q = S.A, S.D, S.C, S.T, S.F, S.Q
R, SCREEN = A.R, A.R.A
KEY = 'surveyed_arrival_comparison'


def original_report(report):
    original = copy.deepcopy(report['transfer_comparison'])
    original.pop(KEY, None)
    return R.stable_schedule(original)


def audit_trigger(actor, observed, survey, seed, mode, neighbors=False):
    seat = actor['seat']
    events = [r for r in survey if r['seat'] == seat and r['allocation']
              and r['allocation']['charged']['physics_queries'] > 0]
    if not events:
        assert actor['untriggered'] and not actor['unobserved']
        assert all(actor[k] is None for k in ['trigger','source','retained_source','attached_source'])
        return dict(triggered=False)
    row = events[0]
    evidence = next((e for s,e in row['evidence'] if s == seat),None)
    assert actor['trigger'] == dict(tick=row['tick'],plan=row['plan'],evidence=evidence), 'first charged attempt changed'
    assert not actor['untriggered']
    source = actor['source']
    due = row['tick']+(61 if neighbors else 1)
    freeze = min((t for t,s in observed if s == seat and t >= due),default=None) if neighbors else due
    if source is None:
        assert actor['unobserved'] and (freeze,seat) not in observed
        assert actor['retained_source'] is actor['attached_source'] is None
        return dict(triggered=True,unobserved=True)
    assert not actor['unobserved'] and source['attempted'] and source['seat'] == seat
    tick = source['source_tick']
    assert tick == freeze, 'source did not freeze at its first eligible observation tick'
    p, old = A.P.pilot(observed[tick,seat]), A.P.pilot(observed[row['tick'],seat])
    if neighbors:
        disruptions = S.collection_disruptions(observed,survey,row,tick)
        if disruptions:
            assert source['rejected'] == disruptions[0]['reason']
            assert actor['attached_source'] is None
            audit_rejected(source)
            return dict(triggered=True,source_tick=tick,attempt_tick=row['tick'],collection_ticks=60,
                rejected=source['rejected'],disruptions=disruptions)
    retained = actor['retained_source']
    if retained is None:
        assert source['rejected'] == 'retained survey source unavailable' and actor['attached_source'] is None
        return dict(triggered=True,rejected=source['rejected'])
    assert retained['tick'] == tick and retained['episode_seed'] == seed
    for key in ['actor','vehicle','spaceling']:
        assert retained[key] == p[key if key != 'actor' else 'owner']
    expected = []
    if neighbors:
        ids = [v for v in row['plan']['request']['candidates'] if v]
        assert len(ids) == 3
        collected = [dict(id=v,status='pending',reason='sample not observed at measurement tick',measurement=None) for v in ids]
        dest = ids[0]['planet']
        frames = [observed[t,seat] for t in range(row['tick'],tick+1)]
        identity = R.neutral_identity(next(v for v in frames[0]['observation']['planets'] if v['index'] == dest))
        # This fixed quiet corpus inherits complete unchanged physical traces;
        # validate continuous material/actor identity before accepting a group.
        for frame in frames:
            current = A.P.pilot(frame)
            assert all(current[k] == old[k] for k in ['owner','vehicle','spaceling'])
            assert current['ship_available'] and current['ship_form'] == 'ship'
            assert T.f32_identity(identity) == T.f32_identity(R.neutral_identity(next(v for v in frame['observation']['planets'] if v['index'] == dest)))
        assert source['rejected'] is None
        for event in events:
            if event['tick'] >= tick: continue
            assert event['plan'] == row['plan'] and event['tick'] <= row['tick']+60
            result = next((e for s,e in event['evidence'] if s == seat),None)
            if result is None: continue
            assert result['generation'] == row['plan']['request']['generation'] and [c['id'] for c in result['candidates']] == ids
            for i,c in enumerate(result['candidates']):
                m = c['measurement']
                if m is None: continue
                assert collected[i]['measurement'] is None, 'slot was retried'
                assert m['tick'] == event['tick'] and m['ship_form'] == 'ship'
                planet = next(v for v in observed[event['tick'],seat]['observation']['planets'] if v['index'] == dest)
                assert T.f32_identity(m['planet']) == T.f32_identity(planet['motion']) and m['revision'] == planet['revision']
                collected[i] = c
        measured = [c['measurement'] for c in collected if c['measurement']]
        if measured:
            expected = [dict(identity=identity,observed_tick=max(m['tick'] for m in measured)+1,
                survey=dict(generation=row['plan']['request']['generation'],candidates=collected))]
        assert T.f32_identity(retained['groups']) == T.f32_identity(expected)
        attached = copy.deepcopy(retained)
        if mode == 'empty': attached['groups'] = []
        assert actor['attached_source'] == attached
        return dict(triggered=True,source_tick=tick,attempt_tick=row['tick'],collection_ticks=60,
            groups=len(expected),measured_slots=len(measured),missing_slots=3-len(measured),
            sample_ages=[tick-m['tick'] for m in measured],findings=[m['finding'] for m in measured])
    if evidence:
        assert row['plan']['request']['generation'] == evidence['generation'] <= row['tick']
        for candidate in evidence['candidates']:
            m = candidate['measurement']
            if m is None: continue
            assert m['tick'] == row['tick'] and tick-m['tick'] == 1
            destination = candidate['id']['planet']
            frames = [next(v for v in observed[t,seat]['observation']['planets'] if v['index'] == destination)
                      for t in [row['tick'],tick]]
            assert T.f32_identity(m['planet']) == T.f32_identity(frames[0]['motion'])
            assert m['revision'] == frames[0]['revision']
            identity = R.neutral_identity(frames[1])
            valid = (identity is not None and T.f32_identity(identity) == T.f32_identity(R.neutral_identity(frames[0]))
                     and all(p[k] == old[k] for k in ['owner','vehicle','spaceling'])
                     and p['ship_available'] and old['ship_available'] and p['ship_form'] == old['ship_form'] == m['ship_form'] == 'ship')
            if valid:
                expected.append(dict(identity=identity,observed_tick=tick,survey=evidence))
    # The arrival observer requests exactly one site; raw candidate status,
    # negative finding, geometry and all measurement epochs remain untouched.
    assert T.f32_identity(retained['groups']) == T.f32_identity(expected)
    attached = copy.deepcopy(retained)
    if mode == 'empty': attached['groups'] = []
    assert actor['attached_source'] == attached
    return dict(triggered=True,source_tick=tick,attempt_tick=row['tick'],source_age_ticks=1,
        groups=len(expected),findings=[c['measurement']['finding'] for g in expected for c in g['survey']['candidates'] if c['measurement']])


def audit_allocation(row, original, charges, states, dispatch_index, sources, local_reference=False, site_preference=False, neighbors=False):
    assert row['queue'] == KEY and row['tick'] == original['tick']
    prior = original['playing_charged_graph']
    original_charge = original['allocation']['charged']
    assert 0 <= prior <= 4
    assert row['playing_charged_graph'] == prior
    assert row['original_comparison_charged'] == original_charge
    assert original_charge['physics_queries'] == 0
    remaining = dict(graph=64-prior-original_charge['graph'],physics_queries=0)
    allocation = row['allocation']
    assert row['remaining_before_comparison'] == allocation['allowance'] == remaining
    assert allocation['tick'] == dispatch_index, 'queue dispatch clock differs from world clock'
    used = allocation['charged']
    assert used['physics_queries'] == 0 and 0 <= used['graph'] <= remaining['graph']
    assert sum(j['charged']['graph'] for j in allocation['jobs']) == used['graph']
    expected = {seat:s for seat,s in sources.items() if s['source_tick'] <= row['tick']}
    assert [a['seat'] for a in row['actors']] == sorted(expected), 'missing or duplicate source row'
    active = {a['seat']:a['state'] for a in row['actors'] if a['state'] and a['state']['phase'] != 'stale'}
    assert [j['request']['actor'] for j in allocation['jobs']] == sorted(active), 'missing or unexpected job'
    for job in allocation['jobs']:
        assert job['charged']['physics_queries'] == 0
        state = active[job['request']['actor']]
        assert job['request'] == state['token'] and job['phase'].lower() == state['phase']
        assert job['age_ticks'] == row['tick']-state['source_tick']+1
        charges[job['request']['actor']] += job['charged']['graph']
    for actor in row['actors']:
        seat, state = actor['seat'],actor['state']
        source = expected[seat]
        if state is None:
            assert actor['rejected'] == source['rejected'] and source['initial'] is None
            assert charges[seat] == 0 and actor['ranked'] is None and actor['candidates'] is None
            continue
        assert actor['rejected'] is None
        assert state['token'] == source['final_state']['token']
        assert state['actor'] == f'player_{seat+1}' and state['source_tick'] == source['source_tick']
        assert state['destination'] == source['initial']['current_destination']
        assert state['token']['actor'] == seat and state['charged_graph'] == charges[seat]
        if state['phase'] == 'stale':
            assert state['validated_tick'] is None and state['cancelled_tick'] <= row['tick']
        else:
            assert state['validated_tick'] == row['tick'] and 0 <= row['tick']-state['source_tick'] <= 120
            assert state['cancelled_tick'] is None and state['reason'] is None
        previous = states.get(seat)
        if state['phase'] == 'ready':
            assert state['completed_tick'] == (previous['completed_tick'] if previous and previous['phase'] == 'ready' else row['tick'])
        elif state['phase'] == 'pending':
            assert state['completed_tick'] is None and (previous is None or previous['phase'] == 'pending')
        else:
            assert state['reason'] and state['completed_tick'] == (previous['completed_tick'] if previous else None)
        if seat in states and states[seat]['phase'] == 'stale': assert state == states[seat]
        states[seat] = state
        extra = charges[seat]-sum(c['charged_graph'] for c in actor['candidates'])-int(actor['ranked'])
        sites = sum(len(c['remote_arrival']['sites']) for c in source['initial']['candidates']) if neighbors else 1
        assert 0 <= sites <= 3
        assert 0 <= extra <= sites*(2+int(local_reference)+int(site_preference)), 'per-site screens/reference/preference exceeded bound'
    return prior+original_charge['graph']+used['graph']


def audit_rejected(source):
    assert source['rejected']
    assert all(source[k] is None for k in ['initial','last_snapshot','last_snapshot_tick','published','published_state','final_state'])


def audit_fresh(root, report, case, controlled, local_reference=False, site_preference=False):
    schedule = report['transfer_comparison'][KEY]
    neighbors = 'arrival_collection' in schedule
    if neighbors: assert schedule['arrival_collection'] == dict(model='neighbors3_window_v1',ticks=60)
    assert schedule.get('arrival_local_reference',False) == local_reference
    assert schedule.get('arrival_site_preference',False) == site_preference
    assert not site_preference or local_reference
    assert schedule['queue'] == KEY and schedule['observational'] and schedule['physics_queries'] == 0
    assert schedule['max_source_age_ticks'] == 120
    survey = list(T.rows(root/'arrival-survey.jsonl'))
    controls = list(C.read_trace(root))
    observed = {(r['tick'],r['seat']):r for r in controls}
    actors = schedule['actors']
    assert [a['seat'] for a in actors] == [0,1]
    triggers = [audit_trigger(a,observed,survey,report['seed'],schedule['mode'],neighbors) for a in actors]
    original = {r['tick']:r for r in T.rows(root/'transfer-comparison-work.jsonl')}
    ledger = list(T.rows(root/'surveyed-arrival-comparison.jsonl'))
    starts = [a['source']['source_tick'] for a in actors if a['source']]
    sources = {a['seat']:a['source'] for a in actors if a['source']}
    assert [r['tick'] for r in ledger] == ([t for t in original if t >= min(starts)] if starts else [])
    charges,states,high,after_retirement = Counter(),{},0,Counter()
    for ordinal,row in enumerate(ledger,1):
        high = max(high,audit_allocation(row,original[row['tick']],charges,states,ordinal,sources,local_reference,site_preference,neighbors))
        for actor in row['actors']:
            state = actor['state']
            old = next((a['state'] for a in original[row['tick']]['actors'] if a['seat'] == actor['seat']),None)
            if state and state['phase'] != 'stale' and old and old['phase'] == 'stale':
                after_retirement[actor['seat']] += 1
    assert schedule['charged_graph'] == sum(charges.values())
    admitted = [s for s in sources.values() if s['initial'] is not None]
    for s in sources.values():
        if s['initial'] is None: audit_rejected(s)
    with_environment = [s for s in sources.values() if s['environment'] is not None]
    D.audit_sources(dict(sources=[dict(seat=s['seat'],tick=s['source_tick']) for s in with_environment]),dict(sources=with_environment),controls)
    assert schedule['submitted'] == len(admitted)
    assert schedule['completed'] == sum(s['published'] is not None for s in admitted)
    assert schedule['cancelled'] == sum(s['final_state']['phase'] == 'stale' for s in admitted)
    for s in admitted:
        if s['published']:
            published = next(a for row in ledger if row['tick'] == s['published_state']['completed_tick'] for a in row['actors'] if a['seat'] == s['seat'])
            assert published['state'] == s['published_state']
            assert s['published']['ranked'] and s['published']['charged_graph'] == published['state']['charged_graph']
            assert s['published'] == s['last_snapshot']
    counts,screen_records,retrospectives = Counter(),[],[]
    history = [dict(actor=r['seat'],tick=r['tick'],evidence=e) for r in survey for seat,e in r['evidence'] if seat == r['seat']]
    for actor in actors:
        source = actor['source']
        if source is None: continue
        counts['sources'] += 1
        if source['initial'] is None:
            assert source['rejected']; counts['rejected'] += 1; continue
        final = source['last_snapshot']
        state = source['final_state']
        assert state == states[source['seat']]
        assert state['charged_graph'] == final['charged_graph'] == charges[source['seat']]
        extra = sum(c['remote_arrival']['local_reference']['charged_graph'] for c in final['candidates']) if local_reference else 0
        if site_preference: extra += sum(c['remote_arrival']['site_preference']['charged_graph'] for c in final['candidates'])
        assert final['charged_graph'] == sum(c['forecast']['charged_graph'] for c in final['candidates'] if c['forecast'])+int(final['ranked'])+sum(c['remote_arrival']['charged_graph'] for c in final['candidates'])+extra
        counts['published'] += source['published'] is not None
        counts['cancellation:'+state['reason']] += 1
        for first,candidate in zip(source['initial']['candidates'],final['candidates']):
            screen = candidate['remote_arrival']
            assert ('local_reference' in screen) == local_reference
            assert ('site_preference' in screen) == site_preference
            assert [s['source'] for s in screen['sites']] == [s['source'] for s in first['remote_arrival']['sites']], 'frozen evidence changed'
            attached = actor['attached_source']
            group = next((g for g in attached['groups'] if g['identity']['planet'] == candidate['destination']),None)
            screen_source = dict(source,remote_source=group['survey'] if group else None)
            check = SCREEN.audit_screen(screen,screen_source,candidate,observed[source['source_tick'],source['seat']],history)
            counts.update(dict(candidates=1,sites=len(screen['sites']),projected=sum(s['projected'] is not None for s in screen['sites']),
                directions=check['directions'],solar_clear=sum(d['solar_clear'] for s in screen['sites'] for d in s['directions'])))
            if screen['unknown']: counts['screen_unknown:'+screen['unknown']] += 1
            for site in screen['sites']:
                if site['unknown']: counts['site_unknown:'+site['unknown']] += 1
            screen_records.append(dict(seat=source['seat'],source_tick=source['source_tick'],screen=screen,audit=check))
            admitted = [s for s in screen['sites'] if s['projected'] is not None and s['unknown'] is None]
            if controlled and candidate['destination'] == case['destination'] and screen['complete'] and screen['unknown'] is None and admitted and (neighbors or len(admitted) == len(screen['sites'])):
                probe = report['transfer_probe']
                start = source['source_tick']
                physical = [r for r in controls if r['seat'] == source['seat'] and r['tick'] >= start]
                joined_screen = dict(screen,sites=admitted) if neighbors else screen
                retrospectives.append(A.compare_screen(dict(case,source_tick=start),joined_screen,physical,
                    probe['acquisition']['started_tick'],probe['acquisition']['outcome']['tick'],probe['landing_choice']))
        for cost in final['capture_costs']:
            assert cost['remaining_trip_seconds'] is None and cost['unknown'], 'arrival geometry must not invent acquisition time'
    return dict(triggers=triggers,counts=dict(counts),graph=dict(charges),maximum_combined_graph=high,
        validated_ticks_after_original_retirement=dict(after_retirement),screens=screen_records,retrospectives=retrospectives)


def forecast_pair(left, right):
    """Extra screening may shorten work; exact equal incomplete outputs are valid."""
    if left is None or right is None:
        assert left is right
        return 'unknown'
    if T.f32_identity(left) == T.f32_identity(right):
        return 'complete_exact' if right['end'] is not None else 'partial_exact_equal'
    assert right['end'] is None and right['ticks'] < left['ticks']
    assert T.f32_identity(right['samples']) == T.f32_identity([s for s in left['samples'] if s['after_ticks'] <= right['ticks']])
    phases = [dict(p,end_tick=min(p['end_tick'],right['ticks'])) for p in left['phases'] if p['start_tick'] < right['ticks']]
    assert right['phases'] == phases
    return 'partial_exact_prefix'


def audit_pair(empty, measured):
    a,b = [r['transfer_comparison'][KEY] for r in [empty,measured]]
    counts = Counter()
    for x,y in zip(a['actors'],b['actors']):
        assert (x['seat'],x['trigger'],x['retained_source'],x['untriggered'],x['unobserved']) == (y['seat'],y['trigger'],y['retained_source'],y['untriggered'],y['unobserved'])
        left,right = x['source'],y['source']
        if left is None: assert right is None; continue
        assert (left['source_tick'],left['environment'],left['rejected']) == (right['source_tick'],right['environment'],right['rejected'])
        if left['initial'] is None: assert right['initial'] is None; continue
        assert SCREEN.strip_screen(left['initial']) == SCREEN.strip_screen(right['initial'])
        if left['last_snapshot']['ranked'] and right['last_snapshot']['ranked']:
            assert SCREEN.strip_screen(left['last_snapshot']) == SCREEN.strip_screen(right['last_snapshot'])
        for lc,rc in zip(left['last_snapshot']['candidates'],right['last_snapshot']['candidates']):
            assert {k:v for k,v in lc.items() if k not in ['forecast','remote_arrival']} == {k:v for k,v in rc.items() if k not in ['forecast','remote_arrival']}
            counts[forecast_pair(lc['forecast'],rc['forecast'])] += 1
    return dict(counts)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--baseline',type=Path,default=Path('target/capture-flag-survey/arrival-survey-v1'))
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code and plan first'
    reference = Path('docs/data/capture-arrival-survey-v1.json')
    archive = json.loads(reference.read_text())
    assert F.digest(args.baseline/'summary.json') == archive['raw_summary_sha256']
    for name,pair in archive['pairs'].items():
        for file,digest in pair['runs']['on']['hashes'].items():
            assert F.digest(args.baseline/(name+'-on')/file) == digest
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True,exist_ok=False)
    commands = {}
    for name,pair in archive['pairs'].items():
        commands[name] = {}
        for mode in ['empty','measured']:
            cmd = [str(binary)]+pair['runs']['on']['command'][1:]
            cmd[cmd.index('--out')+1] = str(args.out/(name+'-'+mode))
            commands[name][mode] = cmd+['--compare-surveyed-arrival',mode]
    result = dict(schema=1,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.digest(binary),reference_sha256=F.digest(reference),plan=archive['plan'],commands=commands,pairs={},
        scope='First charged arrival attempt triggers one new source next tick, empty/measured historical geometry. Same point forecasts, controls, physical survey and original sources; shared64 graph allowance, no additional physical queries. Four correlated sources in one quiet world; no playing strength or complete capture-cost claim.')
    def save(): (args.out/'summary.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    save()
    for spec in archive['plan']:
        case = spec['case']
        for kind in ['ordinary','controlled']:
            name = case['name']+'-'+kind
            baseline = args.baseline/(name+'-on')
            previous = json.loads((baseline/'report.json').read_text())
            pair = dict(runs={}); result['pairs'][name] = pair
            for mode in ['empty','measured']:
                root = args.out/(name+'-'+mode)
                cmd = commands[name][mode]
                logpath = args.out/(root.name+'.log')
                try:
                    with logpath.open('w') as log: subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,check=True)
                    C.archive(root)
                    report = json.loads((root/'report.json').read_text())
                    assert report['transfer_comparison'][KEY]['mode'] == mode
                    parity = D.unchanged(baseline,root)
                    assert S.sensor_digest(root) == archive['pairs'][name]['runs']['on']['sensor_sha256']
                    assert original_report(previous) == original_report(report)
                    for file in ['transfer-probe.jsonl','destination-cover.jsonl']:
                        assert (baseline/file).exists() == (root/file).exists()
                        if (baseline/file).exists(): assert F.digest(baseline/file) == F.digest(root/file), file
                    assert Q.without_wall_times(previous.get('transfer_probe')) == Q.without_wall_times(report.get('transfer_probe'))
                    for file in ['transfer-comparison-work.jsonl','arrival-survey.jsonl']:
                        assert [Q.without_wall_times(r) for r in T.rows(baseline/file)] == [Q.without_wall_times(r) for r in T.rows(root/file)], file
                    survey = S.audit_survey(root,report,case,kind=='controlled')
                    assert survey == archive['pairs'][name]['survey']
                    audit = audit_fresh(root,report,case,kind=='controlled')
                    pair['runs'][mode] = dict(command=cmd,historical_parity=parity,audit=audit,
                        trace_sha256=D.S.trace_digest(root),sensor_sha256=S.sensor_digest(root),log_sha256=F.digest(logpath),
                        hashes={p.name:F.digest(p) for p in sorted(root.iterdir()) if p.is_file()})
                    if mode == 'measured':
                        empty = args.out/(name+'-empty')
                        pair['parity'] = D.unchanged(empty,root)
                        pair['comparison'] = audit_pair(json.loads((empty/'report.json').read_text()),report)
                    save(); print(name,mode,'audited',flush=True)
                except Exception as error:
                    result['error'] = dict(case=name,mode=mode,error=repr(error),command=cmd,log_sha256=F.digest(logpath) if logpath.exists() else None)
                    save(); raise
    assert F.digest(binary) == result['binary_sha256']
    result['aggregate'] = {mode:dict(sum((Counter(p['runs'][mode]['audit']['counts']) for p in result['pairs'].values()),Counter())) for mode in ['empty','measured']}
    result['complete'] = True
    save()


if __name__ == '__main__': main()
