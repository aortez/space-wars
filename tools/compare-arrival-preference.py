#!/usr/bin/env python3
"""Frozen retained-subset preference replay; full native choice remains unknown."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess

SPEC = importlib.util.spec_from_file_location('arrival_local', Path(__file__).with_name('compare-arrival-local.py'))
L = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(L)
M, C, T, F, Q, KEY = L.M, L.C, L.T, L.F, L.Q, L.KEY
G = M.SCREEN.L


def audit_preference(candidate, row):
    screen = candidate['remote_arrival']
    L.audit_reference(screen, row)
    r = screen['site_preference']
    assert r['model'] == 'arrival_subset_preference_v1' and r['source_tick'] == screen['source_tick']
    assert r['native_choice'] is None and r['acquisition_seconds'] is None
    for condition in ['fresh committed v13', 'no required/rejected/cooldown site or objective', 'unexposed neutral ground']:
        assert condition in r['conditions']
    assert 'query readiness' in r['native_choice_unknown'] and 'retained subset only' in r['native_choice_unknown']
    reason = screen['local_reference']['unknown'] or ('source capture already active; fresh selector history unknown' if candidate['source_capture'] else None)
    assert r['unknown'] == reason
    if reason:
        assert r['complete'] and r['charged_graph'] == 0 and not r['assessments'] and r['preferred'] is None
        assert r['unassessed_bearings'] == list(range(64))
        return
    assert r['charged_graph'] == len(r['assessments']) <= len(screen['sites']) <= 4
    assert r['complete'] == (screen['complete'] and len(r['assessments']) == len(screen['sites']))
    eligible, unassessed = [], set(range(64))
    for source, record in zip(screen['sites'], r['assessments']):
        assert screen['local_reference']['complete']
        arrival, projected, sample = screen['arrival'], source['projected'], source['source']['measurement']
        assert record['site'] == source['source']['id']
        assert record['measurement_tick'] == (sample['tick'] if sample else None)
        assert record['arrival_tick'] == arrival['tick']
        reason = source['unknown'] or ('arrival material site unavailable' if projected is None or sample is None else
            'claim state outside arrival-local reference domain' if not L.neutral_claim(arrival['planet']) else
            'arrival solar screen incomplete' if len(source['directions']) != 2 else None)
        if not reason:
            center = G.vec(arrival['planet']['motion']['position'])
            if any(G.dot(G.unit(G.sub(G.vec(p), center)), G.unit(G.sub(G.vec(p), center))) < .5
                   for p in [arrival['ship']['position'], projected['vehicle_position']]):
                reason = 'degenerate arrival approach geometry'
        if reason:
            assert record['unknown'] == reason and record['short_angle'] is None and not record['eligible']
            continue
        short = G.short_angle(arrival, projected)
        G.close(record['short_angle'], short, .000003)
        preferred = -1 if short < 0 else 1
        directions = [d for side in [preferred, -preferred] for d in source['directions']
                      if d['side'] == side and (arrival['sun'] is not None or side == preferred)]
        expected = [(i,d) for i,d in enumerate(directions) if d['solar_clear']]
        assert len(record['eligible']) == len(expected)
        for value, (order, d) in zip(record['eligible'], expected):
            assert value['site'] == record['site'] and value['side'] == d['side'] and value['direction_order'] == order
            angle = G.directed(short, d['side']) if d['solar'] else short
            G.close(value['approach_score'], G.f32(abs(angle)*G.f32(arrival['planet']['radius']+60)), .002)
            eligible.append(value)
        assert record['unknown'] == (None if expected else 'no eligible native arrival direction')
        unassessed.remove(record['site']['bearing'])
    assert r['unassessed_bearings'] == sorted(unassessed)
    # Scores are checked independently above; preserve exact native f32 ties,
    # never turn the reconstruction tolerance into a tie-breaking tolerance.
    best = min(eligible, key=lambda v:(v['approach_score'], v['site']['bearing'], v['direction_order']), default=None)
    assert r['preferred'] == best


def retrospective(r, seat, destination, native, proof, native_domain_unknown, observed_seconds):
    preferred = r['preferred']
    relation = ('unknown' if preferred is None or native is None else
        'site_mismatch' if preferred['site'] != native['site'] else
        'direction_mismatch' if preferred['side'] != native['side'] else 'site_and_direction_match')
    assessment = next((a for a in r['assessments'] if preferred and a['site'] == preferred['site']), None)
    reason = r['unknown'] or ('preference incomplete' if not r['complete'] else
        'no eligible subset preference' if assessment is None else
        'native choice unavailable' if native is None else
        'no source-specific material join' if proof is None or any(proof.get(k) != v for k,v in
            dict(source_tick=r['source_tick'],seat=seat,destination=destination).items()) else None)
    material_reason = reason or L.join_reason(assessment,proof)
    reason = material_reason or native_domain_unknown
    return dict(source_tick=r['source_tick'],seat=seat,destination=destination,
        classification=relation if reason is None else 'unknown',unknown=reason,raw_relation=relation,
        preferred=preferred,native=native,forecast_short_angle=assessment['short_angle'] if assessment else None,
        same_site_material_join_valid=material_reason is None,native_domain_unknown=native_domain_unknown,
        observed_handoff_to_choice_seconds=observed_seconds,
        native_choice_predicted=False,acquisition_seconds_predicted=None)


def audit_records(root, report, case, controlled, fresh):
    controls = list(C.read_trace(root))
    observed = {(r['tick'],r['seat']):r for r in controls}
    L.audit_local(root, report, case, controlled, fresh)
    records, joins = [], []
    for actor in report['transfer_comparison'][KEY]['actors']:
        source = actor['source']
        if source is None or source['initial'] is None: continue
        row = observed[source['source_tick'],actor['seat']]
        for field in ['initial','last_snapshot','published']:
            snapshot = source[field]
            if snapshot is None: continue
            for candidate in snapshot['candidates']:
                audit_preference(candidate, row)
                assert not snapshot['ranked'] or candidate['remote_arrival']['site_preference']['complete']
        for candidate in source['last_snapshot']['candidates']:
            r = candidate['remote_arrival']['site_preference']
            records.append(dict(seat=actor['seat'], destination=candidate['destination'], preference=r))
            if not controlled or candidate['destination'] != case['destination']: continue
            choice = report['transfer_probe']['landing_choice']['report'] if actor['seat'] == case['seat'] else None
            native = choice['selected'] if choice else None
            proofs = [v for v in fresh['retrospectives'] if v['source_tick'] == r['source_tick'] and v['destination'] == candidate['destination']]
            assert len(proofs) <= 1
            proof = dict(proofs[0],seat=actor['seat']) if proofs and choice else None
            native_unknown, elapsed = 'native choice unavailable', None
            if choice:
                actual_row = observed[choice['tick'],actor['seat']]
                p = L.A.P.pilot(actual_row)
                if proof: assert proof['actual_choice']['ship'] == p['ship'] and proof['native_selected'] == native
                site = next((s for s in p['sites'] if s['id'] == native['site']),None)
                native_unknown = L.N.domain(actual_row,choice,site)
                elapsed = (choice['tick']-report['transfer_probe']['acquisition']['started_tick'])/60
            joins.append(retrospective(r,actor['seat'],candidate['destination'],native,proof,native_unknown,elapsed))
    return dict(records=records, retrospectives=joins)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--baseline',type=Path,default=Path('target/capture-flag-survey/arrival-local-v1'))
    parser.add_argument('--out',type=Path,required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code and plan first'
    projection = Path('docs/data/capture-arrival-local-v1.json')
    previous = json.loads(projection.read_text())
    assert F.digest(args.baseline/'summary.json') == previous['raw_summary_sha256']
    for name,pair in previous['pairs'].items():
        for file,digest in pair['runs']['on']['hashes'].items(): assert F.digest(args.baseline/(name+'-on')/file) == digest
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True,exist_ok=False)
    commands = {}
    for name,pair in previous['pairs'].items():
        commands[name] = {}
        for mode in ['off','on']:
            cmd = [str(binary)]+pair['runs']['on']['command'][1:]
            cmd[cmd.index('--out')+1] = str(args.out/(name+'-'+mode))
            commands[name][mode] = cmd+['--arrival-site-preference',mode]
    result = dict(schema=1,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.digest(binary),reference_sha256=F.digest(projection),plan=previous['plan'],commands=commands,pairs={},
        scope='Conditional retained-subset native preference, one graph step/site, no physics or playing changes. Native global choice, future exposure and acquisition time unknown. One-site corpus does not validate global selection.')
    def save(): (args.out/'summary.json').write_text(json.dumps(result,indent=2,allow_nan=False)+'\n')
    save()
    for spec in previous['plan']:
        case = spec['case']
        for kind in ['ordinary','controlled']:
            name = case['name']+'-'+kind
            baseline = args.baseline/(name+'-on')
            old = json.loads((baseline/'report.json').read_text())
            pair = dict(runs={}); result['pairs'][name] = pair
            for mode in ['off','on']:
                root, cmd = args.out/(name+'-'+mode), commands[name][mode]
                logpath = args.out/(root.name+'.log')
                try:
                    with logpath.open('w') as log: subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT,check=True)
                    C.archive(root)
                    report = json.loads((root/'report.json').read_text())
                    parity = L.D.unchanged(baseline,root)
                    assert M.S.sensor_digest(root) == previous['pairs'][name]['runs']['on']['sensor_sha256']
                    assert M.original_report(old) == M.original_report(report)
                    for file in ['transfer-probe.jsonl','destination-cover.jsonl']:
                        assert (baseline/file).exists() == (root/file).exists()
                        if (baseline/file).exists(): assert F.digest(baseline/file) == F.digest(root/file)
                    for file in ['transfer-comparison-work.jsonl','arrival-survey.jsonl']:
                        assert [Q.without_wall_times(r) for r in T.rows(baseline/file)] == [Q.without_wall_times(r) for r in T.rows(root/file)]
                    assert Q.without_wall_times(old.get('transfer_probe')) == Q.without_wall_times(report.get('transfer_probe'))
                    audit = M.audit_fresh(root,report,case,kind=='controlled',True,mode=='on')
                    if mode == 'off':
                        assert Q.without_wall_times(old['transfer_comparison'][KEY]) == Q.without_wall_times(report['transfer_comparison'][KEY])
                        assert [Q.without_wall_times(r) for r in T.rows(baseline/'surveyed-arrival-comparison.jsonl')] == [Q.without_wall_times(r) for r in T.rows(root/'surveyed-arrival-comparison.jsonl')]
                    else: pair['comparison'] = L.audit_pair(old,report,'site_preference','arrival_site_preference')
                    pair['runs'][mode] = dict(command=cmd,historical_parity=parity,audit=audit,
                        preference=audit_records(root,report,case,kind=='controlled',audit) if mode=='on' else None,
                        trace_sha256=L.D.S.trace_digest(root),sensor_sha256=M.S.sensor_digest(root),log_sha256=F.digest(logpath),
                        hashes={p.name:F.digest(p) for p in sorted(root.iterdir()) if p.is_file()})
                    save(); print(name,mode,'audited',flush=True)
                except Exception as error:
                    result['error'] = dict(case=name,mode=mode,error=repr(error),command=cmd)
                    save(); raise
    assert F.digest(binary) == result['binary_sha256']
    result['complete'] = True
    save()


if __name__ == '__main__': main()
