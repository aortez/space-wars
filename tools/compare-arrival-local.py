#!/usr/bin/env python3
"""Compare same-site arrival-local references on the frozen surveyed-arrival corpus."""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import subprocess


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(file))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


M = module('surveyed_arrivals', 'compare-surveyed-arrivals.py')
N = module('neutral_timing', 'compare-neutral-capture-timing.py')
A, D, C, T, F, Q, KEY = M.A, M.D, M.C, M.T, M.F, M.Q, M.KEY


def neutral_claim(planet):
    c = planet['claim']
    return bool(c and c['planet'] == planet['index'] and c['owner'] is None and c['flag'] is None
                and c['claimant'] is None and c['phase'] == 'idle' and c['progress'] == 0
                and N.finite(c['stage_required_seconds']) and abs(c['stage_required_seconds']-3) <= .001
                and N.finite(c['flag_interaction_range']) and c['flag_interaction_range'] > 0)


def audit_reference(screen, row):
    r, p = screen['local_reference'], A.P.pilot(row)
    assert r['model'] == 'arrival_local_reference_v1' and r['calibration'] == 'neutral_successful_trip_medians_v1'
    assert r['source_tick'] == screen['source_tick'] == row['tick']
    assert r['policy'] == row['mission']['policy']
    assert r['acquisition_seconds'] is None and r['remaining_trip_seconds'] is None
    assert 'unknown' in r['phase_origin'] and 'no elapsed-time subtraction' in r['phase_origin']
    assert 'native selects this site' in r['conditions'] and 'unexposed' in r['conditions']
    assert 'not a solar safety certificate' in r['solar_scope'] and 'unknown' in r['future_threat']
    unknown = ('policy outside arrival-local reference domain' if r['policy'] != 'material_mission_v13' else
               'source ship unavailable for arrival-local reference' if not (p['controls_armed'] and p['queries_ready']
                   and p['ship_available'] and p['ship_form'] == 'ship' and N.finite(p['ship_health'])
                   and p['ship_health'] > 0 and p['location'] == dict(aboard=p['vehicle'])) else screen['unknown'])
    assert r['unknown'] == unknown
    if unknown:
        assert r['complete'] and not r['references'] and r['charged_graph'] == 0
        return dict(unknown=unknown,references=0,numeric=0)
    assert r['charged_graph'] == len(r['references']) <= len(screen['sites']) <= 4
    assert r['complete'] == (screen['complete'] and len(r['references']) == len(screen['sites']))
    numeric = 0
    for source, ref in zip(screen['sites'],r['references']):
        assert screen['complete'] and screen['arrival']
        arrival = screen['arrival']
        sides = [d['side'] for d in source['directions'] if d['solar_clear']]
        assert ref['site'] == source['source']['id'] and ref['projected'] == source['projected']
        sample = source['source']['measurement']
        assert ref['measurement_tick'] == (sample['tick'] if sample else None)
        assert ref['arrival_tick'] == arrival['tick'] and ref['eligible_sides'] == sides
        reason = (source['unknown'] or ('arrival material site unavailable' if source['projected'] is None or sample is None else
                  'claim state outside arrival-local reference domain' if not neutral_claim(arrival['planet']) else
                  'no eligible arrival solar direction' if len(source['directions']) != 2 or not sides else None))
        assert ref['unknown'] == reason
        if reason:
            assert ref['phases'] is None and ref['conditional_seconds'] is None
        else:
            assert set(ref['phases']) == set(N.PHASE_COSTS)
            for key, value in N.PHASE_COSTS.items(): Q.close(ref['phases'][key],value,.000003)
            Q.close(ref['conditional_seconds'],sum(N.PHASE_COSTS.values()),.00001)
            numeric += 1
    return dict(unknown=None,references=len(r['references']),numeric=numeric)


def without_reference(report):
    report = copy.deepcopy(report)
    for candidate in report['candidates']:
        reference = candidate['remote_arrival'].pop('local_reference',None)
        if reference: report['charged_graph'] -= reference['charged_graph']
    return report


def audit_snapshot(snapshot, row):
    for c in snapshot['candidates']:
        screen = c['remote_arrival']
        audit_reference(screen,row)
        assert not snapshot['ranked'] or screen['local_reference']['complete'], 'ranked reference was not finished'


def audit_pair(off, on):
    left, right = [r['transfer_comparison'][KEY] for r in [off,on]]
    assert right['arrival_local_reference'] and 'arrival_local_reference' not in left
    assert {k:v for k,v in Q.without_wall_times(left).items() if k not in ['actors','charged_graph']} == {
        k:v for k,v in Q.without_wall_times(right).items() if k not in ['actors','charged_graph','arrival_local_reference']}
    total_extra, sources = 0, []
    assert len(left['actors']) == len(right['actors'])
    for a,b in zip(left['actors'],right['actors']):
        assert {k:v for k,v in a.items() if k != 'source'} == {k:v for k,v in b.items() if k != 'source'}
        x,y = a['source'],b['source']
        if x is None: assert y is None; continue
        varying = ['initial','last_snapshot','published','published_state','final_state']
        assert {k:v for k,v in x.items() if k not in varying} == {k:v for k,v in y.items() if k not in varying}
        for field in ['initial','last_snapshot','published']:
            if x[field] is None: assert y[field] is None; continue
            assert without_reference(y[field]) == x[field], 'old forecast, solar, cost or ranking changed'
        extra = (y['last_snapshot']['charged_graph']-x['last_snapshot']['charged_graph']) if x['last_snapshot'] else 0
        for field in ['published_state','final_state']:
            if x[field] is None: assert y[field] is None; continue
            assert y[field]['charged_graph']-x[field]['charged_graph'] == extra
            varying_state = ['completed_tick','charged_graph']
            if field == 'published_state':
                varying_state.append('validated_tick')
                for state in [x[field],y[field]]:
                    assert state['validated_tick'] == state['completed_tick']
            assert {k:v for k,v in x[field].items() if k not in varying_state} == {
                k:v for k,v in y[field].items() if k not in varying_state}
            if x[field]['completed_tick'] is not None:
                assert y[field]['completed_tick'] >= x[field]['completed_tick']
        total_extra += extra
        sources.append(dict(source_tick=x['source_tick'],extra_graph=extra,
            ready_off=x['published_state']['completed_tick'] if x['published_state'] else None,
            ready_on=y['published_state']['completed_tick'] if y['published_state'] else None))
    assert right['charged_graph']-left['charged_graph'] == total_extra
    return dict(extra_graph=total_extra,sources=sources)


def join_reason(reference, screen_join):
    if screen_join is None: return 'no audited native geometry join'
    sites = [s for s in screen_join['sites'] if s['id'] == reference['site'] and s['measurement_tick'] == reference['measurement_tick']]
    assert len(sites) <= 1
    if not sites: return 'original measurement not joined'
    site = sites[0]
    if not site['retrospective']['choice']['age_and_identity_valid']:
        return 'measurement expired or actor/material identity changed before native choice'
    geometry = site['same_frame_geometry_residual']
    if not geometry or not geometry['within_reconstruction_tolerance']:
        return 'native material geometry differs from historical sample'
    return None


def retrospective(reference, report, case, controls, screen_join):
    # Native choice and physical outcomes are joined only after the run.
    cap, choice = [report['transfer_probe'][k] for k in ['capture_followthrough','landing_choice']]
    selected = (choice['report'] or {}).get('selected')
    matched = selected is not None and selected['site'] == reference['site'] and selected['side'] in reference['eligible_sides']
    if not matched:
        return dict(matched=False,selected=selected,phases={},unknown='native site/direction not covered')
    reason = join_reason(reference,screen_join)
    if reason:
        return dict(matched=False,selected=selected,phases={},unknown=reason)
    actual = A.H.audit_capture(case,report,controls)
    rows = [r for r in controls if r['tick'] >= cap['milestones']['choice']]
    site = next(s for s in A.P.pilot(rows[0])['sites'] if s['id'] == selected['site'])
    native_unknown = N.domain(rows[0],choice['report'],site)
    later = [r['tick'] for r in rows[1:] if N.exposure(r)[0]]
    timing_unknown = reference['unknown'] or native_unknown or ('later exposure outside reference conditions' if later else None)
    return dict(matched=True,selected=selected,measurement_tick=reference['measurement_tick'],
        native_choice_tick=rows[0]['tick'],native_domain_unknown=native_unknown,
        timing_unknown=timing_unknown,
        outcome=actual['outcome'],milestones=actual['milestones'],
        handoff_to_choice_seconds=(rows[0]['tick']-report['transfer_probe']['acquisition']['started_tick'])/60,
        phases=A.H.phases(actual['milestones'],reference['phases'] if timing_unknown is None else None,cap['outcome']),
        first_later_exposure_tick=later[0] if later else None,later_exposed_rows=len(later))


def audit_local(root, report, case, controlled, fresh_audit):
    controls = list(C.read_trace(root))
    observed = {(r['tick'],r['seat']):r for r in controls}
    rows, joins, charges = [], [], 0
    for actor in report['transfer_comparison'][KEY]['actors']:
        source = actor['source']
        if source is None or source['initial'] is None: continue
        row = observed[source['source_tick'],actor['seat']]
        for field in ['initial','last_snapshot','published']:
            if source[field] is None: continue
            audit_snapshot(source[field],row)
        for c in source['last_snapshot']['candidates']:
            screen = c['remote_arrival']
            local = screen['local_reference']
            charges += local['charged_graph']
            rows.append(dict(seat=actor['seat'],destination=c['destination'],reference=local,
                audit=audit_reference(screen,row),old_site=c['local_reference']['evidence']['site'] if c['local_reference']['evidence'] else None))
            if controlled and c['destination'] == case['destination']:
                matched_screens = [j for j in fresh_audit['retrospectives'] if j['source_tick'] == source['source_tick'] and j['destination'] == c['destination']]
                assert len(matched_screens) <= 1
                screen_join = matched_screens[0] if matched_screens else None
                for ref in local['references']:
                    joins.append(retrospective(ref,report,case,[r for r in controls if r['seat'] == actor['seat']],screen_join))
    return dict(charged_graph=charges,records=rows,retrospectives=joins)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--baseline',type=Path,default=Path('target/capture-flag-survey/surveyed-arrival-v1'))
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code and plan first'
    projection = Path('docs/data/capture-surveyed-arrival-v1.json')
    previous = json.loads(projection.read_text())
    assert F.digest(args.baseline/'summary.json') == previous['raw_summary_sha256']
    for name,pair in previous['pairs'].items():
        for file,digest in pair['runs']['measured']['hashes'].items():
            assert F.digest(args.baseline/(name+'-measured')/file) == digest
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True,exist_ok=False)
    commands = {}
    for name,pair in previous['pairs'].items():
        commands[name] = {}
        for mode in ['off','on']:
            cmd = [str(binary)]+pair['runs']['measured']['command'][1:]
            cmd[cmd.index('--out')+1] = str(args.out/(name+'-'+mode))
            commands[name][mode] = cmd+['--arrival-local-reference',mode]
    result = dict(schema=1,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.digest(binary),reference_sha256=F.digest(projection),plan=previous['plan'],commands=commands,pairs={},
        scope='Same measured-site conditional successful-trip medians, not geometry-specific timing. Shared64 graph allowance, one extra step/site, no physical queries or playing changes. Acquisition/future threat/whole-trip cost remain unknown. Four correlated quiet flights; no calibration or strength claim.')
    def save(): (args.out/'summary.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    save()
    for spec in previous['plan']:
        case = spec['case']
        for kind in ['ordinary','controlled']:
            name = case['name']+'-'+kind
            baseline = args.baseline/(name+'-measured')
            old = json.loads((baseline/'report.json').read_text())
            pair = dict(runs={}); result['pairs'][name] = pair
            for mode in ['off','on']:
                root = args.out/(name+'-'+mode)
                cmd = commands[name][mode]
                logpath = args.out/(root.name+'.log')
                try:
                    with logpath.open('w') as log: subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,check=True)
                    C.archive(root)
                    report = json.loads((root/'report.json').read_text())
                    assert report['transfer_comparison'][KEY]['mode'] == 'measured'
                    parity = D.unchanged(baseline,root)
                    assert M.S.sensor_digest(root) == previous['pairs'][name]['runs']['measured']['sensor_sha256']
                    assert M.original_report(old) == M.original_report(report)
                    for file in ['transfer-probe.jsonl','destination-cover.jsonl']:
                        assert (baseline/file).exists() == (root/file).exists()
                        if (baseline/file).exists(): assert F.digest(baseline/file) == F.digest(root/file)
                    for file in ['transfer-comparison-work.jsonl','arrival-survey.jsonl']:
                        assert [Q.without_wall_times(r) for r in T.rows(baseline/file)] == [Q.without_wall_times(r) for r in T.rows(root/file)]
                    assert Q.without_wall_times(old.get('transfer_probe')) == Q.without_wall_times(report.get('transfer_probe'))
                    audit = M.audit_fresh(root,report,case,kind=='controlled',mode=='on')
                    if mode == 'off':
                        assert Q.without_wall_times(old['transfer_comparison'][KEY]) == Q.without_wall_times(report['transfer_comparison'][KEY])
                        assert [Q.without_wall_times(r) for r in T.rows(baseline/'surveyed-arrival-comparison.jsonl')] == [Q.without_wall_times(r) for r in T.rows(root/'surveyed-arrival-comparison.jsonl')]
                    else:
                        pair['comparison'] = audit_pair(old,report)
                    pair['runs'][mode] = dict(command=cmd,historical_parity=parity,audit=audit,
                        local=audit_local(root,report,case,kind=='controlled',audit) if mode=='on' else None,
                        trace_sha256=D.S.trace_digest(root),sensor_sha256=M.S.sensor_digest(root),log_sha256=F.digest(logpath),
                        hashes={p.name:F.digest(p) for p in sorted(root.iterdir()) if p.is_file()})
                    save(); print(name,mode,'audited',flush=True)
                except Exception as error:
                    result['error'] = dict(case=name,mode=mode,error=repr(error),command=cmd,log_sha256=F.digest(logpath) if logpath.exists() else None)
                    save(); raise
    assert F.digest(binary) == result['binary_sha256']
    result['complete'] = True
    save()


if __name__ == '__main__': main()
