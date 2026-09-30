#!/usr/bin/env python3
"""Compare fixed center/neighbor surveys without changing playing decisions."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location('preference', Path(__file__).with_name('compare-arrival-preference.py'))
V = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(V)
L, M, C, T, F, Q, KEY = V.L, V.M, V.C, V.T, V.F, V.Q, V.KEY


def original(report):
    result = M.original_report(report)
    result.pop('arrival_survey', None)
    return result


def audit_mode(report, mode):
    schedule = report['transfer_comparison']
    pattern = schedule['arrival_survey'].get('pattern')
    collection = schedule[KEY].get('arrival_collection')
    assert schedule[KEY]['mode'] == 'measured'
    assert mode in ['nearest','neighbors3']
    assert pattern == ('neighbors3' if mode == 'neighbors3' else None)
    assert collection == (dict(model='neighbors3_window_v1',ticks=60) if mode == 'neighbors3' else None)


def relation(a, b):
    if a is None or b is None: return 'unknown'
    if a['site'] != b['site']: return 'site_mismatch'
    return 'site_and_direction_match' if a['side'] == b['side'] else 'direction_mismatch'


def subset_comparison(r, center, join, proof):
    """Center-only and full preferences use the SAME frozen arrival frame."""
    records = [a for a in r['assessments'] if a['site'] == center]
    assert len(records) <= 1
    values = records[0]['eligible'] if records else []
    best = min(values,key=lambda v:(v['approach_score'],v['direction_order']),default=None)
    result = dict(source_tick=r['source_tick'],center=center,center_only_preference=best,
        subset_preference=r['preferred'],center_to_subset_relation=relation(best,r['preferred']),
        complete=r['complete'],unknown=r['unknown'],unassessed_bearings=r['unassessed_bearings'],
        scope='Same frozen endpoint and original measurements. No comparison of accuracy across source epochs; no global choice or acquisition prediction.')
    if join is not None:
        narrowed = dict(r,preferred=best)
        result['center_to_native'] = V.retrospective(narrowed,join['seat'],join['destination'],join['native'],proof,
            join['native_domain_unknown'],join['observed_handoff_to_choice_seconds'])
        native_best = proof['native_best_retained'] if proof else None
        joined_ids = [s['id'] for s in proof['sites']] if proof else []
        material = [dict(site=a['site'],unknown=L.join_reason(a,proof)) for a in r['assessments']
            if a['eligible'] or a['site'] in joined_ids]
        reason = r['unknown'] or ('preference incomplete' if not r['complete'] else
            'no eligible subset preference' if r['preferred'] is None else
            'native retained choice unavailable' if native_best is None else
            'no source-specific material join' if any(proof.get(k) != v for k,v in
                dict(source_tick=r['source_tick'],seat=join['seat'],destination=join['destination']).items()) else
            next((a['unknown'] for a in material if a['unknown']),None) or join['native_domain_unknown'])
        result['native_subset'] = dict(best=native_best,material_joins=material,
            raw_relation=relation(r['preferred'],native_best),
            classification=relation(r['preferred'],native_best) if reason is None else 'unknown',unknown=reason,
            scope='Actual best among source-admitted/projected retained sites only; refused or missing planned slots are outside this denominator.')
    return result


def audit_subsets(report, fresh, preference):
    result = []
    for actor in report['transfer_comparison'][KEY]['actors']:
        source = actor['source']
        if source is None or source['initial'] is None: continue
        center = actor['trigger']['plan']['site']
        for candidate in source['last_snapshot']['candidates']:
            if candidate['destination'] != center['planet']: continue
            screen = candidate['remote_arrival']
            r = screen['site_preference']
            joins = [j for j in preference['retrospectives'] if
                (j['source_tick'],j['seat'],j['destination']) == (r['source_tick'],actor['seat'],candidate['destination'])]
            proofs = [p for p in fresh['retrospectives'] if
                (p['source_tick'],p['destination']) == (r['source_tick'],candidate['destination'])]
            assert len(joins) <= 1 and len(proofs) <= 1
            proof = dict(proofs[0],seat=actor['seat']) if proofs else None
            comparison = subset_comparison(r,center,joins[0] if joins else None,proof)
            comparison['coverage'] = dict(planned_slots=sum(v is not None for v in actor['trigger']['plan']['request']['candidates']),
                retained_slots=len(screen['sites']),measured_slots=sum(s['source']['measurement'] is not None for s in screen['sites']),
                admitted_sites=sum(s['projected'] is not None and s['unknown'] is None for s in screen['sites']),
                eligible_sites=sum(bool(a['eligible']) for a in r['assessments']),eligible_directions=sum(len(a['eligible']) for a in r['assessments']))
            result.append(comparison)
    return result


def audit_parity(baseline, root, old, report, nearest):
    parity = L.D.unchanged(baseline,root)
    assert M.S.sensor_digest(baseline) == M.S.sensor_digest(root)
    assert original(old) == original(report)
    for file in ['transfer-probe.jsonl','destination-cover.jsonl']:
        assert (baseline/file).exists() == (root/file).exists()
        if (baseline/file).exists(): assert F.digest(baseline/file) == F.digest(root/file)
    files = ['transfer-comparison-work.jsonl']
    if nearest:
        assert Q.without_wall_times(old['transfer_comparison']) == Q.without_wall_times(report['transfer_comparison'])
        files += ['arrival-survey.jsonl','surveyed-arrival-comparison.jsonl']
    for file in files:
        assert [Q.without_wall_times(r) for r in T.rows(baseline/file)] == [Q.without_wall_times(r) for r in T.rows(root/file)]
    assert Q.without_wall_times(old.get('transfer_probe')) == Q.without_wall_times(report.get('transfer_probe'))
    return dict(parity, native_sensors=True,original_comparisons=True,nearest_reports_and_work=nearest)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--baseline',type=Path,default=Path('target/capture-flag-survey/arrival-preference-v1'))
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code and plan first'
    projection = Path('docs/data/capture-arrival-preference-v1.json')
    previous = json.loads(projection.read_text())
    assert F.digest(args.baseline/'summary.json') == previous['raw_summary_sha256']
    # The compact projection deliberately omits the original candidate's native
    # timing evidence. Reuse it from the hash-verified raw plan for those audits.
    plan = json.loads((args.baseline/'summary.json').read_text())['plan']
    assert [{k:spec['case'][k] for k in case} for spec,case in zip(plan,previous['plan'])] == previous['plan']
    assert len(plan) == len(previous['plan'])
    for name,pair in previous['pairs'].items():
        for file,digest in pair['runs']['on']['hashes'].items(): assert F.digest(args.baseline/(name+'-on')/file) == digest
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True,exist_ok=False)
    commands = {}
    for name in previous['pairs']:
        commands[name] = {}
        for mode in ['nearest','neighbors3']:
            cmd = [str(binary)]+previous['commands'][name]['on'][1:]
            cmd[cmd.index('--out')+1] = str(args.out/(name+'-'+mode))
            commands[name][mode] = cmd+['--arrival-survey-sites',mode]
    result = dict(schema=1,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.digest(binary),reference_sha256=F.digest(projection),plan=plan,commands=commands,pairs={},
        scope='Three fixed measured candidates under unchanged physical caps, once per 30 ticks, fixed first+61 source. Conditional subset only. Center-only comparison within that same frame isolates candidate contribution; nearest/neighbor source epochs differ. No playing, cost or policy changes.')
    def save(): (args.out/'summary.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    save()
    for spec in plan:
        case = spec['case']
        for kind in ['ordinary','controlled']:
            name = case['name']+'-'+kind
            baseline = args.baseline/(name+'-on')
            old = json.loads((baseline/'report.json').read_text())
            pair = dict(runs={}); result['pairs'][name] = pair
            for mode in ['nearest','neighbors3']:
                root,cmd = args.out/(name+'-'+mode),commands[name][mode]
                logpath = args.out/(root.name+'.log')
                try:
                    with logpath.open('w') as log: subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,check=True)
                    C.archive(root)
                    report = json.loads((root/'report.json').read_text())
                    audit_mode(report,mode)
                    parity = audit_parity(baseline,root,old,report,mode=='nearest')
                    survey = M.S.audit_survey(root,report,case,kind=='controlled')
                    fresh = M.audit_fresh(root,report,case,kind=='controlled',True,True)
                    preference = V.audit_records(root,report,case,kind=='controlled',fresh)
                    pair['runs'][mode] = dict(command=cmd,historical_parity=parity,survey=survey,audit=fresh,preference=preference,
                        subsets=audit_subsets(report,fresh,preference),trace_sha256=L.D.S.trace_digest(root),
                        sensor_sha256=M.S.sensor_digest(root),log_sha256=F.digest(logpath),
                        hashes={p.name:F.digest(p) for p in sorted(root.iterdir()) if p.is_file()})
                    save(); print(name,mode,'audited',flush=True)
                except Exception as error:
                    result['error'] = dict(case=name,mode=mode,error=repr(error),command=cmd)
                    save(); raise
    assert F.digest(binary) == result['binary_sha256']
    result['complete'] = True
    save()


if __name__ == '__main__': main()
