#!/usr/bin/env python3
"""Frozen v15 landing handoff comparison; retain v14 and all refusals."""
import argparse
import gzip
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess

spec = importlib.util.spec_from_file_location('survey_value', Path(__file__).with_name('compare-survey-value.py'))
S = importlib.util.module_from_spec(spec)
spec.loader.exec_module(S)
D, F, V, E = S.D, S.F, S.V, S.E
POLICIES = ('material_mission_v14', 'material_mission_v15')


def plan():
    result = []
    for old in S.plan()[:34]:
        row = dict(old, version=old['version']+1)
        row['name'] = row['group']+f'-v{row["version"]}'
        result.append(row)
    for world in range(2):
        namespace = f'costed-landing-handoff-v1:{world}'
        seed = int.from_bytes(hashlib.sha256(namespace.encode()).digest()[:8], 'little')
        for interval in [0, 3]:
            group = f'world{world}-asteroids{interval}'
            variants = [(14, None), (15, 0), (15, 1)]
            rotation = (world+interval) % 3
            for version, seat in variants[rotation:]+variants[:rotation]:
                name = group + ('-v14' if seat is None else f'-v15-p{seat+1}')
                result.append(dict(name=name, group=group, kind='held_out', seed=seed,
                    seed_namespace=namespace, interval=interval, version=version, seat=seat))
    return result


def arguments(item):
    args, policies = S.arguments(item)
    if item['kind'] == 'held_out':
        for seat in range(2):
            if policies[seat] == 13:
                policies[seat] = 14
                args[args.index(f'--p{seat+1}-policy')+1] = POLICIES[0]
    return args, policies


def audit_handoffs(records, evaluations, decisions):
    wanted = {(r['seat'], r['switch']['tick']):r for r in decisions['switches']
        if r['source_forecast']['policy'] == POLICIES[1]}
    latest, native, previous_tick = {}, {}, {}
    for row in records:
        seat, tick, h = row['seat'], row['tick'], row['handoff']
        assert tick > previous_tick.get(seat, -1)
        previous_tick[seat] = tick
        if h is None:
            continue
        assert row['mission']['destination_planning']['landing_handoff'] == h
        key = seat, h['switch_tick']
        switch = wanted[key]
        forecast = switch['source_forecast']
        c = next(c for c in forecast['candidates'] if c['planet'] == switch['switch']['to'])
        assert h['source_tick'] == forecast['source_tick'] and h['site'] == c['site']
        assert h['evidence_tick'] == c['evidence_tick'] <= h['source_tick'] <= h['switch_tick'] <= tick
        assert row['mission']['policy'] == POLICIES[1]
        p = row['observation']['local']['combat']['recovery']['flight']['pilot']
        assert p['tick'] == tick and p['owner'] == f'player_{seat+1}'
        capture = row['mission']['capture']
        if row['visit_tick'] == h['switch_tick']:
            native.setdefault(key, []).append((tick, None if capture is None else capture['site']))
        old = latest.get(key)
        for field in ['started_tick','accepted_tick','landed_tick','completed_tick','invalidated_tick']:
            value = h[field]
            if value is not None:
                assert h['switch_tick'] <= value <= tick
                if old is None or old[field] is None:
                    assert value == tick, 'missing native milestone observation'
        for earlier,later in [('started_tick','accepted_tick'),('accepted_tick','landed_tick'),('landed_tick','completed_tick')]:
            if h[later] is not None:
                assert h[earlier] is not None and h[earlier] <= h[later]
        if h['invalidated_tick'] is not None:
            assert h['completed_tick'] is None and h['reason'] is not None
            assert all(h[f] is None or h[f] <= h['invalidated_tick'] for f in ['accepted_tick','landed_tick'])
        if h['started_tick'] is not None:
            assert h['started_tick'] == switch['actual_visit']['arrived_tick']
        if old is None:
            assert tick == h['switch_tick'], 'missing original handoff'
        else:
            for field in ['started_tick','accepted_tick','landed_tick','completed_tick','invalidated_tick']:
                if old[field] is not None:
                    assert old[field] == h[field], 'handoff milestone changed'
            if old['reason'] is not None:
                assert old['reason'] == h['reason']
        if h['accepted_tick'] == tick:
            assert p['queries_ready'] and capture['site'] == h['site']
            assert tick-h['evidence_tick'] <= 1800
            assert tick < h['started_tick']+120
            acquisition = capture['acquisition']
            assert acquisition['tick'] == tick and acquisition['reason'] == 'selected_site'
            assert acquisition['planet'] == p['planet']['index'] == c['planet']
            assert acquisition['revision'] == p['planet']['revision'] == c['revision']
            assert acquisition['required_site'] == acquisition['selected_site'] == h['site']
            assert acquisition['checks']['eligible'] > 0
            assert any(s['id'] == h['site'] and s['revision'] == p['planet']['revision']
                and any(v is not None for v in s['boarding_hatches']) for s in p['sites'])
        if h['landed_tick'] == tick:
            assert h['accepted_tick'] is not None and capture['goal'] == 'surface'
            assert capture['site'] == h['site']
            assert p['planet']['index'] == c['planet'] and p['planet']['revision'] == c['revision']
            assert p['landing']['phase'] == 'landed' and p['transfer'] == 'ready'
            assert capture['landing']['landed_tick'] is not None and p['queries_ready']
            site = next(s for s in p['sites'] if s['id'] == h['site'])
            assert site['revision'] == p['planet']['revision']
            assert math.dist([p['ship']['position'][k] for k in ['x','y']],
                [site['vehicle_position'][k] for k in ['x','y']]) <= 10.0001
        if h['completed_tick'] is not None:
            assert h['landed_tick'] is not None and h['invalidated_tick'] is None
            assert h['completed_tick'] == switch['actual_visit']['departed_tick']
        latest[key] = h
    assert latest.keys() == wanted.keys(), 'missing handoff for accepted v15 switch'
    suppressed, fresh = 0, 0
    for r in evaluations:
        if r['policy'] != POLICIES[1]:
            continue
        key = int(r['actor'][-1])-1, r['selected_tick']
        h = latest.get(key)
        if (h is None or h['invalidated_tick'] is None or r['completed_tick'] < h['invalidated_tick']
                or r['current_target'] != h['site']['planet']):
            continue
        assert r['source_tick'] >= h['invalidated_tick'], 'pre-refusal source published after invalidation'
        c = next((c for c in r['candidates'] if c['current']), None)
        if c is None or c['total_seconds'] is None:
            suppressed += 1
            continue
        assert c['evidence_tick'] >= h['invalidated_tick']
        assert c['evidence_kind'] not in [S.FLAG_KIND, 'remote landing, hatch and climb samples; live feasibility unknown']
        sites = [site for tick, site in native[key] if tick <= r['source_tick']]
        assert sites and c['site'] == sites[-1], 'cost is not for the native fallback site'
        fresh += 1
    return dict(references=[dict(seat=k[0], **h) for k,h in latest.items()],
        refused_current_reports_unknown=suppressed, fresh_native_replacement_reports=fresh,
        scope='Matching bearing and native touchdown do not certify identical walking routes or calibrate whole-trip timing. Invalidated forecasts remain in the switch record.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--predecessor', type=Path, default=Path('target/flag-value-behavior/frozen-v1'))
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'], text=True).strip(), 'freeze code and plan first'
    old_summary = args.predecessor/'summary.json'
    assert F.digest(old_summary) == '1c123fce75b2278557d72e6874b0a1c08798236e3f8cec471fa72e68f826e6b1'
    old = json.loads(old_summary.read_text())
    args.out.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve(strict=True)
    result = dict(schema=1, plan=plan(), complete=False, runs={}, comparisons=[], predecessor_parity=[],
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'], text=True).strip(),
        binary_sha256=F.digest(binary), predecessor_summary_sha256=F.digest(old_summary),
        tools={Path(m.__file__).name:F.digest(Path(m.__file__)) for m in [S,V,F,D,E,V.V]},
        runner_sha256=F.digest(Path(__file__)),
        scope='46 runs: 32 directed, two known-regression runs, 12 finished-match smoke tests over two fresh worlds. Correlated seat/pressure variants. V14 retained; no weight, margin or duration fitting. Native sensors/serialization outside shared planning quota.')
    save = lambda: D.write(args.out/'summary.json', result)
    save()
    for item in result['plan']:
        name, seat = item['name'], item['seat'] or 0
        command, policies = arguments(item)
        directed = item['kind'] == 'directed'
        try:
            run = D.run(binary, args.out, name, command, seat, seconds=180 if directed else 600, require_finish=not directed)
            root = args.out/name
            report = json.loads((root/'report.json').read_text())
            assert report['seed'] == item['seed'] and report['round']['time_limit_seconds'] == 600
            assert [p['policy'] for p in report['policy_configuration']] == [f'material_mission_v{v}' for v in policies]
            run['players'] = [D.finished_player(report,s) for s in range(2)]
            run['predictions'] = E.prediction_results(report,S.rows(root/'mission-evaluations.jsonl'))
            run['decisions'] = V.switch_predictions(report,S.rows(root/'mission-evaluations.jsonl'))
            run['handoffs'] = audit_handoffs(S.rows(root/'landing-handoffs.jsonl'), S.rows(root/'mission-evaluations.jsonl'), run['decisions'])
            run['flag_references'] = S.flag_references(S.rows(root/'mission-evaluations.jsonl'), list(S.rows(root/'flag-survey.jsonl')), POLICIES)
            run['work'] = S.work_audit(root,report)
            run['progress'] = S.progress(S.validated_trace(S.rows(root/'destination-behavior.jsonl'),report))
            run['coverage'] = V.V.coverage(root/'mission-evaluations.jsonl')
            trace = root/'destination-behavior.jsonl'
            with trace.open('rb') as src, gzip.open(str(trace)+'.gz','wb',compresslevel=1) as dst:
                shutil.copyfileobj(src,dst)
            trace.unlink()
            run['hashes'] = {p.name:F.digest(p) for p in root.iterdir() if p.is_file()}
            run['log_sha256'] = F.digest(args.out/(name+'.log'))
            result['runs'][name] = run
            if item['version'] == 14 and item['kind'] != 'held_out':
                previous = args.predecessor/name
                for file in ['report.json','mission-evaluations.jsonl']:
                    assert F.digest(previous/file) == old['runs'][name]['hashes'][file]
                previous_report = json.loads((previous/'report.json').read_text())
                assert D.same_physical_outcomes(previous_report,report)
                assert previous_report['missions'] == report['missions']
                assert F.digest(previous/'mission-evaluations.jsonl') == F.digest(root/'mission-evaluations.jsonl')
                result['predecessor_parity'].append(name)
            save()
        except Exception as error:
            result['error'] = dict(case=item, command=command, error=repr(error))
            save()
            raise
    for item in result['plan']:
        if item['version'] != 15: continue
        a,b = item['group']+'-v14', item['name']
        reports = [json.loads((args.out/n/'report.json').read_text()) for n in [a,b]]
        assert reports[0]['initial_world'] == reports[1]['initial_world']
        result['comparisons'].append(dict(baseline=a,candidate=b,seat=item['seat'],
            baseline_outcome=D.outcome(reports[0],item['seat']),candidate_outcome=D.outcome(reports[1],item['seat']),
            **S.compare_controls(args.out,a,b,reports)))
    assert F.digest(binary) == result['binary_sha256']
    result['complete'] = True
    save()


if __name__ == '__main__': main()
