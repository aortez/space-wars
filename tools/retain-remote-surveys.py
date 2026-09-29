#!/usr/bin/env python3
"""Fixed paired arrival screens: active survey versus bounded retained geometry."""
import argparse
from collections import Counter
import importlib.util
import json
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location('arrival', Path(__file__).with_name('screen-remote-arrivals.py'))
A = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(A)
M, L, R, C, T, F, S = A.M, A.L, A.R, A.C, A.T, A.F, A.S


def neutral_identity(p):
    c = p['claim']
    if c is None or c['owner'] is not None or c['flag'] is not None: return None
    assert c['planet'] == p['index']
    return dict(planet=p['index'], revision=p['revision'], radius=p['radius'],
                **{k:c[k] for k in ['stage_required_seconds','flag_interaction_range','captures','neutralizations']})


def audit_memory(source, rows, history, seed):
    tick, seat = source['source_tick'], source['seat']
    memory = source['retained_source']
    if memory is None:
        assert source['rejected'] == 'retained survey source unavailable'
        return dict(unavailable=1)
    o = rows[seat,tick]['observation']
    p = o['local']['combat']['recovery']['flight']['pilot']
    assert memory['tick'] == tick and memory['episode_seed'] == seed
    assert all(memory[k] == p[k if k != 'actor' else 'owner'] for k in ['actor','vehicle','spaceling'])
    groups = memory['groups']
    assert len(groups) <= 2
    assert [g['identity']['planet'] for g in groups] == sorted({g['identity']['planet'] for g in groups})
    counts = Counter(groups=len(groups))
    for group in groups:
        identity, survey = group['identity'], group['survey']
        dest = identity['planet']
        assert T.f32_identity(identity) == T.f32_identity(neutral_identity(next(p for p in o['planets'] if p['index'] == dest)))
        assert survey['generation'] <= group['observed_tick'] <= tick
        candidates = survey['candidates']
        assert 1 <= len(candidates) <= 4
        assert len({c['id']['bearing'] for c in candidates}) == len(candidates)
        assert all(c['id']['planet'] == dest for c in candidates)
        measurements = [c['measurement'] for c in candidates if c['measurement']]
        assert measurements
        # This host dispatches after observations, admitting a measurement next tick.
        assert group['observed_tick'] == max(m['tick'] for m in measurements)+1
        for c in candidates:
            m = c['measurement']
            if not m: continue
            assert survey['generation'] <= m['tick'] < tick and tick-m['tick'] <= 1800
            # This fixed host observes each dispatched snapshot on the next tick.
            assert c['status'] == 'stale' and c['reason'] == 'physics advanced; remeasurement required'
            witnesses = [v['measurement'] for r in history
                if r['actor'] == seat and r['tick'] == m['tick'] and r['evidence']['generation'] == survey['generation']
                for v in r['evidence']['candidates'] if v['id'] == c['id']]
            assert any(T.f32_identity(v) == T.f32_identity(m) for v in witnesses), 'retained sample lacks original dispatch'
            newer = [v for r in history if r['actor'] == seat and m['tick'] < r['tick'] < tick
                and r['evidence']['generation'] >= survey['generation']
                for v in r['evidence']['candidates'] if v['measurement'] and v['measurement']['tick'] == r['tick']
                and v['id']['planet'] == dest
                and (r['evidence']['generation'] > survey['generation'] or v['id'] == c['id'])]
            assert not newer, 'newer dispatched measurement must replace retained evidence, including failures'
            for t in range(m['tick'],tick+1):
                actual = rows[seat,t]['observation']
                pilot = actual['local']['combat']['recovery']['flight']['pilot']
                assert all(pilot[k] == p[k] for k in ['owner','vehicle','spaceling'])
                assert pilot['ship_available'] and pilot['ship_form'] == 'ship'
                planet = next(p for p in actual['planets'] if p['index'] == dest)
                assert T.f32_identity(neutral_identity(planet)) == T.f32_identity(identity)
                if t == m['tick']:
                    assert T.f32_identity(m['planet']) == T.f32_identity(planet['motion'])
                    assert m['revision'] == planet['revision']
            counts['measurements'] += 1
            counts['finding:'+m['finding']] += 1
    return dict(counts)


def stable_schedule(schedule):
    return {k:v for k,v in schedule.items() if k not in ['observation','dispatch']}


def audit_work(root, report, cap):
    upstream = S.audit_upstream(root)
    charges, prior_extra, last = Counter(), Counter(), {}
    ledger = T.rows(root/'transfer-comparison-work.jsonl')
    for row in ledger:
        prior = upstream[row['tick']]['total']
        allocation = row['allocation']
        assert row['playing_charged_graph'] == prior
        assert row['remaining_before_comparison'] == allocation['allowance'] == dict(graph=64-prior,physics_queries=0)
        assert 0 <= allocation['charged']['graph'] <= 64-prior and allocation['charged']['physics_queries'] == 0
        assert allocation['charged']['graph'] == sum(j['charged']['graph'] for j in allocation['jobs'])
        for job in allocation['jobs']:
            assert job['charged']['physics_queries'] == 0
            charges[job['request']['actor']] += job['charged']['graph']
        for actor in row['actors']:
            seat, state = actor['seat'], actor['state']
            if not state: continue
            assert state['charged_graph'] == charges[seat]
            extra = charges[seat] - sum(c['charged_graph'] for c in actor['candidates']) - int(actor['ranked'])
            assert prior_extra[seat] <= extra <= cap
            prior_extra[seat] = extra
            if state['phase'] == 'stale':
                assert state['validated_tick'] is None and state['cancelled_tick'] <= row['tick']
            else:
                assert state['validated_tick'] == row['tick'] and row['tick']-state['source_tick'] <= 120
            if seat in last and last[seat]['phase'] == 'stale': assert state == last[seat]
            last[seat] = state
    schedule = report['transfer_comparison']
    assert schedule['charged_graph'] == sum(charges.values())
    for source in schedule['sources']:
        final = source['last_snapshot']
        if final is None: continue
        extra = sum(c['remote_arrival']['charged_graph'] for c in final['candidates'])
        assert source['final_state'] == last[source['seat']]
        assert final['charged_graph'] == source['final_state']['charged_graph']
        assert final['charged_graph'] == sum(c['forecast']['charged_graph'] for c in final['candidates'] if c['forecast'])+int(final['ranked'])+extra
    return dict(charges)


def audit_pair(off_root, on_root, baseline, previous):
    for file,digest in previous['runs'][1]['hashes'].items(): assert F.digest(baseline/file) == digest
    archive_parity = M.unchanged(baseline,off_root)
    archive_parity['sensors'] = L.audit_sensors(baseline,off_root)
    assert S.trace_digest(off_root) == previous['runs'][1]['trace_sha256']
    parity = M.unchanged(off_root,on_root)
    parity['sensors'] = L.audit_sensors(off_root,on_root)
    reports = [json.loads((r/'report.json').read_text()) for r in [baseline,off_root,on_root]]
    archived, old, new = [r['transfer_comparison'] for r in reports]
    assert stable_schedule(archived) == stable_schedule(old), 'active-only queue/report changed'
    def stable_ledger(root):
        return [{k:v for k,v in r.items() if k != 'dispatch_ms'} for r in T.rows(root/'transfer-comparison-work.jsonl')]
    assert stable_ledger(baseline) == stable_ledger(off_root)
    audit_work(off_root,reports[1],8)
    audit_work(on_root,reports[2],16)
    assert [(s['seat'],s['source_tick']) for s in old['sources']] == [(s['seat'],s['source_tick']) for s in new['sources']]
    rows = {(r['seat'],r['tick']):r for r in C.read_trace(on_root)}
    history = T.rows(on_root/'destination-cover.jsonl')
    counts, sources, max_error = Counter(), [], 0.0
    for a,b in zip(old['sources'],new['sources']):
        assert (a['environment'],a['remote_source'],a['rejected']) == (b['environment'],b['remote_source'],b['rejected'])
        counts.update({'retained_'+k:v for k,v in audit_memory(b,rows,history,reports[2]['seed']).items()})
        if b['initial'] is None:
            counts['rejected'] += 1
            sources.append(b)
            continue
        assert A.strip_screen(a['initial']) == A.strip_screen(b['initial'])
        R.forecast_parity(dict(sources=[a]),dict(sources=[b]))
        if b['published']:
            assert a['published'] and A.strip_screen(a['published']) == A.strip_screen(b['published'])
        if b['last_snapshot']['ranked']:
            assert A.strip_screen(a['last_snapshot']) == A.strip_screen(b['last_snapshot'])
        counts['sources'] += 1
        counts['published'] += b['published'] is not None
        counts['cancellation:'+b['final_state']['reason']] += 1
        for initial,candidate,old_candidate in zip(b['initial']['candidates'],b['last_snapshot']['candidates'],a['last_snapshot']['candidates']):
            assert {k:v for k,v in candidate.items() if k not in ['forecast','remote_arrival']} == {k:v for k,v in old_candidate.items() if k not in ['forecast','remote_arrival']}
            screen = candidate['remote_arrival']
            assert [s['source'] for s in screen['sites']] == [s['source'] for s in initial['remote_arrival']['sites']]
            group = next((g for g in b['retained_source']['groups'] if g['identity']['planet'] == candidate['destination']),None)
            source = dict(b,remote_source=group['survey'] if group else None)
            audit = A.audit_screen(screen,source,candidate,rows[b['seat'],b['source_tick']],history)
            max_error = max(max_error,audit['maximum_solar_error'])
            counts['candidates'] += 1
            counts['sampled_sites'] += len(screen['sites'])
            counts['projected_sites'] += sum(s['projected'] is not None for s in screen['sites'])
            counts['directions'] += audit['directions']
            counts['uncertain_solar_signs'] += audit['uncertain']
            counts['uncertain_departure_order'] += audit['departure_ties']
            counts['solar_clear_directions'] += sum(d['solar_clear'] for s in screen['sites'] for d in s['directions'])
            if screen['unknown']: counts['screen_unknown:'+screen['unknown']] += 1
            for s in screen['sites']:
                if s['unknown']: counts['site_unknown:'+s['unknown']] += 1
            local = candidate['local_reference']
            numeric_remote = bool(local['remaining'] and local['evidence']['remote'])
            counts['numeric_remote'] += numeric_remote
            counts['numeric_remote_with_sites'] += numeric_remote and bool(screen['sites'])
            if screen['sites'] and not old_candidate['remote_arrival']['sites']:
                counts['newly_sampled_candidates'] += 1
                counts['newly_sampled_numeric_remote'] += numeric_remote
        if b['final_state']['reason'] == 'remote arrival solar context changed or source samples expired':
            cancelled = b['final_state']['cancelled_tick']
            admitted = [s['source']['measurement']['tick'] for c in b['initial']['candidates']
                if c['remote_arrival']['unknown'] is None for s in c['remote_arrival']['sites'] if s['unknown'] is None]
            assert any(cancelled-t > 1800 for t in admitted) or rows[b['seat'],cancelled]['observation']['local']['sun'] != rows[b['seat'],b['source_tick']]['observation']['local']['sun']
        else:
            assert {k:v for k,v in a['final_state'].items() if k not in ['charged_graph','completed_tick']} == {k:v for k,v in b['final_state'].items() if k not in ['charged_graph','completed_tick']}
        sources.append(dict(seat=b['seat'],source_tick=b['source_tick'],retained_source=b['retained_source'],
            active_source=b['remote_source'],screens=[c['remote_arrival'] for c in b['last_snapshot']['candidates']],
            off_completed_tick=a['final_state']['completed_tick'],on_completed_tick=b['final_state']['completed_tick'],
            final_state=b['final_state']))
    return dict(archive_parity=archive_parity,parity=parity,counts=+counts,sources=sources,
        maximum_solar_error=max_error,physical_ticks=reports[1]['elapsed_ticks']*2,
        graph_off=old['charged_graph'],graph_on=new['charged_graph'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference',type=Path,default=Path('docs/data/capture-remote-arrival-v1.json'))
    parser.add_argument('--baseline',type=Path,default=Path('target/capture-flag-survey/remote-arrival-v1'))
    parser.add_argument('--binary',type=Path,default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code and plan first'
    archive = json.loads(args.reference.read_text())
    assert len(archive['pairs']) == 13
    assert F.digest(args.baseline/'summary.json') == archive['raw_summary_sha256']
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True,exist_ok=False)
    result = dict(schema=1,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.digest(binary),reference_sha256=F.digest(args.reference),pairs={})
    save = lambda: (args.out/'summary.json').write_text(json.dumps(result,indent=2)+'\n')
    save()
    for name,previous in archive['pairs'].items():
        roots,runs = [],[]
        for mode in ['off','on']:
            root = args.out/(name+'-'+mode)
            command = [str(binary)]+previous['runs'][1]['command'][1:]
            command[command.index('--out')+1] = str(root)
            command += ['--retain-remote-surveys',str(mode == 'on').lower()]
            with (args.out/(root.name+'.log')).open('w') as log:
                subprocess.run(command,stdout=log,stderr=subprocess.STDOUT,check=True)
            trace = C.archive(root)
            roots.append(root)
            runs.append(dict(command=command,trace_sha256=trace,
                hashes={p.name:F.digest(p) for p in root.iterdir() if p.is_file()}))
        audit = audit_pair(*roots,args.baseline/(name+'-on'),previous)
        result['pairs'][name] = dict(runs=runs,**audit)
        save()
        print(name,audit['counts'],flush=True)
    assert F.digest(binary) == result['binary_sha256']
    result['complete'] = True
    result['counts'] = dict(sum((Counter(p['counts']) for p in result['pairs'].values()),Counter()))
    for k in ['physical_ticks','graph_off','graph_on']: result[k] = sum(p[k] for p in result['pairs'].values())
    save()
    print(json.dumps(result['counts'],indent=2))


if __name__ == '__main__': main()
