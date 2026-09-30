#!/usr/bin/env python3
"""Frozen v16 neutral approach experiment against retained v15; no tuning."""
import argparse
from collections import Counter
import gzip
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess

spec = importlib.util.spec_from_file_location('landing_handoff', Path(__file__).with_name('compare-landing-handoff.py'))
H = importlib.util.module_from_spec(spec)
spec.loader.exec_module(H)
S, D, F, V, E = H.S, H.D, H.F, H.V, H.E
POLICIES = ('material_mission_v15', 'material_mission_v16')
NEUTRAL_KIND = 'remote landing, hatch and climb samples; live feasibility unknown'


def plan():
    result = []
    for old in H.plan()[:34]:
        row = dict(old, version=old['version']+1)
        row['name'] = row['group']+f'-v{row["version"]}'
        result.append(row)
    for world in range(2):
        namespace = f'approach-survey-behavior-v1:{world}'
        seed = int.from_bytes(hashlib.sha256(namespace.encode()).digest()[:8], 'little')
        for interval in [0, 3]:
            group = f'world{world}-asteroids{interval}'
            variants = [(15, None), (16, 0), (16, 1)]
            rotation = (world+interval) % 3
            for version, seat in variants[rotation:]+variants[:rotation]:
                name = group + ('-v15' if seat is None else f'-v16-p{seat+1}')
                result.append(dict(name=name, group=group, kind='held_out', seed=seed,
                    seed_namespace=namespace, interval=interval, version=version, seat=seat))
    return result


def arguments(item):
    args, policies = S.arguments(item)
    if item['kind'] == 'held_out':
        for seat in range(2):
            if policies[seat] == 13:
                policies[seat] = 15
                args[args.index(f'--p{seat+1}-policy')+1] = POLICIES[0]
    return args+['--trace-neutral-approaches', 'true'], policies


def angle(row, measurement):
    """Independent rigid-frame reprojection; angle is a ranking proxy, not time."""
    planet = next(p for p in row['planets'] if p['index'] == measurement['site']['id']['planet'])
    old, now = measurement['planet'], planet['motion']
    point = measurement['site']['vehicle_position']
    x, y = point['x']-old['position']['x'], point['y']-old['position']['y']
    rotation = now['angle']-old['angle']
    dx, dy = x*math.cos(rotation)-y*math.sin(rotation), x*math.sin(rotation)+y*math.cos(rotation)
    ux, uy = row['ship_position']['x']-now['position']['x'], row['ship_position']['y']-now['position']['y']
    norms = (ux*ux+uy*uy, dx*dx+dy*dy)
    assert all(math.isfinite(n) and .001 <= n <= 3.4028234663852886e38 for n in norms)
    return abs(math.atan2(ux*dy-uy*dx, ux*dx+uy*dy))


def neutral_key(planet):
    claim = planet['claim']
    if (claim is not None and claim['owner'] is None and claim['flag'] is None
            and abs(claim['stage_required_seconds']-3) <= .001):
        return planet['index'], planet['revision'], claim['stage_required_seconds']


def compatible_measurements(row):
    evidence = row['evidence']
    if not row['queries_ready'] or evidence is None:
        return []
    result = []
    for c in evidence['candidates']:
        planet = next((p for p in row['planets'] if p['index'] == c['id']['planet']), None)
        m = c['measurement']
        if m is None or planet is None or neutral_key(planet) is None:
            continue
        if (evidence['generation'] <= m['tick'] <= row['tick'] <= m['tick']+1800
                and m['revision'] == planet['revision'] and m['ship_form'] == 'ship'):
            result.append((c,neutral_key(planet)))
    return result


def positive(row, candidate):
    m, s = candidate['measurement'], candidate['measurement']['site']
    if (m['finding'] == 'measured' and m['climb_clear'] is True and s is not None
            and s['id'] == candidate['id'] and s['revision'] == m['revision']
            and any(h is not None for h in s['boarding_hatches'])):
        try:
            angle(row,m)
            return True
        except (AssertionError, ValueError):
            pass
    return False


def audit_approaches(records, evaluations, *, start_tick, ticks, actors):
    wanted = {(r['actor'], r['source_tick']):r for r in evaluations if r['policy'] == POLICIES[1]}
    seen, admitted, requests, generations, counts = {}, {}, {}, set(), Counter()
    for row in records:
        actor, tick = row['actor'], row['tick']
        index = counts['input_rows']
        assert actors and (tick,actor) == (start_tick+index//len(actors),actors[index%len(actors)]), 'non-dense input history'
        assert row['policy'] == POLICIES[1]
        counts['input_rows'] += 1
        keys = {k for p in row['planets'] if (k := neutral_key(p)) is not None}
        admitted = {k:v for k,v in admitted.items() if k[0] != actor or (
            row['queries_ready'] and v['key'] in keys and 0 <= tick-v['measurement']['tick'] <= 1800)}
        evidence = row['evidence']
        if evidence is not None:
            assert len(evidence['candidates']) == 2
            assert len({(c['id']['planet'],c['id']['bearing']) for c in evidence['candidates']}) == len(evidence['candidates'])
            assert all(0 <= c['id']['bearing'] < 64 for c in evidence['candidates'])
            assert len({c['id']['planet'] for c in evidence['candidates']}) == 1
            planet = next(p for p in row['planets'] if p['index'] == evidence['candidates'][0]['id']['planet'])
            assert evidence['generation'] <= tick
            identity = row['target'], row['selected_tick'], neutral_key(planet)
            ids = [c['id'] for c in evidence['candidates']]
            old = requests.get(actor)
            if old is not None and old[0] == identity and old[1] != evidence['generation']:
                assert evidence['generation'] >= old[1]+30, 'request changed inside refresh bound'
                assert evidence['generation'] == tick and ids != old[2]
            if old is not None and old[1] == evidence['generation']:
                assert ids == old[2], 'bearings changed without a new generation'
            requests[actor] = identity, evidence['generation'], ids
            generations.add((actor, evidence['generation']))
            for c in evidence['candidates']:
                if (m := c['measurement']) is not None:
                    assert m['tick'] <= tick
                    identity = actor, m['tick'], c['id']['planet'], c['id']['bearing']
                    assert seen.setdefault(identity, m) == m, 'original measurement changed'
        else:
            # Request absence can also mean eligibility reset; this cadence
            # assertion covers uninterrupted requests, not those hidden gaps.
            requests.pop(actor, None)
        compatible = compatible_measurements(row)
        valid = [c['measurement'] for c,_ in compatible if positive(row,c)]
        for planet in {key[0] for _,key in compatible}:
            choices = [(c,key) for c,key in compatible if key[0] == planet and positive(row,c)]
            if not choices:
                admitted.pop((actor,planet), None)
                counts['negative_replacements'] += 1
            else:
                c,key = min(choices, key=lambda v: (angle(row,v[0]['measurement']),-v[0]['measurement']['tick'],v[0]['id']['bearing']))
                admitted[actor,planet] = dict(key=key,measurement=c['measurement'])
        report = wanted.pop((actor,tick), None)
        if report is None:
            continue
        counts['reports_joined'] += 1
        assert report['current_target'] == row['target'] and report['selected_tick'] == row['selected_tick']
        for c in report['candidates']:
            if c['evidence_kind'] != NEUTRAL_KIND or c['local'] is None:
                continue
            assert row['queries_ready']
            m = admitted[actor,c['planet']]['measurement']
            assert m['tick'] == c['evidence_tick'] and m['site']['id'] == c['site'], 'reference was not retained or newly admitted'
            assert m['revision'] == c['revision'] and c['observed_owner'] is None
            assert c['evidence_age_ticks'] == tick-m['tick'] <= 1800
            counts['neutral_references'] += 1
            costs = dict(landing=17.866667, exit=1/60, outbound=4/60,
                claim=181/60, return_board=2/60, departure=226/60)
            assert c['local'].keys() == costs.keys()
            assert all(math.isclose(c['local'][k], v, abs_tol=1e-5) for k,v in costs.items())
            choices = [v for v in valid if v['site']['id']['planet'] == c['planet']]
            if not choices:
                counts['retained_references_without_current_measurement'] += 1
                continue
            chosen = next(v for v in choices if v['site']['id'] == c['site'] and v['tick'] == c['evidence_tick'])
            # JSON decimal output and independent double arithmetic need a small
            # angular tolerance. Rust unit tests exercise exact tie ordering.
            assert angle(row, chosen) <= min(angle(row,v) for v in choices)+1e-5, 'longer measured approach selected'
            counts['ranked_references'] += 1
            counts['two_site_rankings'] += len(choices) == 2
    assert not wanted, 'missing immutable evaluator input'
    assert counts['input_rows'] == ticks*len(actors), 'incomplete input history'
    return dict(**counts, observed_generations=len(generations),
        scope='Original pre-dispatch inputs, immutable measurement age, rigid reprojection and shortest angular ranking. Retained evidence need not belong to a pending replacement generation. No arrival-time or future-cover certification.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/release/examples/surface_mission_soak'))
    parser.add_argument('--predecessor', type=Path, default=Path('target/costed-landing-handoff/frozen-v1'))
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'], text=True).strip(), 'freeze code and plan first'
    old_summary = args.predecessor/'summary.json'
    assert F.digest(old_summary) == '03e2ca83139228fcb78fe998df4afa2f3c4fcc4e671a5c08d8080800e8813f3c'
    old = json.loads(old_summary.read_text())
    args.out.mkdir(parents=True, exist_ok=False)
    binary = args.binary.resolve(strict=True)
    result = dict(schema=1, plan=plan(), complete=False, runs={}, comparisons=[], predecessor_parity=[],
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'], text=True).strip(),
        binary_sha256=F.digest(binary), predecessor_summary_sha256=F.digest(old_summary),
        tools={Path(m.__file__).name:F.digest(Path(m.__file__)) for m in [H,S,V,F,D,E,V.V]},
        runner_sha256=F.digest(Path(__file__)),
        scope='46 runs: 32 directed, two known-regression runs, 12 finished-match smoke tests over two fresh worlds. Correlated seat/pressure variants. V15 retained; no timing/weight/margin fitting. Native sensors and serialization outside shared quota.')
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
            run['handoffs'] = H.audit_handoffs(S.rows(root/'landing-handoffs.jsonl'), S.rows(root/'mission-evaluations.jsonl'), run['decisions'], POLICIES)
            actors = [f'player_{s+1}' for s in ([seat] if directed else range(2)) if policies[s] == 16]
            run['approaches'] = audit_approaches(S.rows(root/'neutral-approach-inputs.jsonl'), S.rows(root/'mission-evaluations.jsonl'),
                start_tick=report['initial_world']['local']['combat']['recovery']['flight']['pilot']['tick'],
                ticks=report['elapsed_ticks'], actors=actors)
            run['flag_references'] = S.flag_references(S.rows(root/'mission-evaluations.jsonl'), list(S.rows(root/'flag-survey.jsonl')), POLICIES)
            run['work'] = S.work_audit(root,report)
            run['evaluation_jobs'] = {k:report['mission_evaluation'][k] for k in ['completed','cancelled','pending']}
            run['progress'] = S.progress(S.validated_trace(S.rows(root/'destination-behavior.jsonl'),report))
            run['coverage'] = V.V.coverage(root/'mission-evaluations.jsonl')
            for name_to_compress in ['destination-behavior.jsonl', 'neutral-approach-inputs.jsonl']:
                trace = root/name_to_compress
                with trace.open('rb') as src, gzip.open(str(trace)+'.gz','wb',compresslevel=1) as dst:
                    shutil.copyfileobj(src,dst)
                trace.unlink()
            run['hashes'] = {p.name:F.digest(p) for p in root.iterdir() if p.is_file()}
            run['log_sha256'] = F.digest(args.out/(name+'.log'))
            result['runs'][name] = run
            if item['version'] == 15 and item['kind'] != 'held_out':
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
        if item['version'] != 16: continue
        a,b = item['group']+'-v15', item['name']
        reports = [json.loads((args.out/n/'report.json').read_text()) for n in [a,b]]
        assert reports[0]['initial_world'] == reports[1]['initial_world']
        result['comparisons'].append(dict(baseline=a,candidate=b,seat=item['seat'],
            baseline_outcome=D.outcome(reports[0],item['seat']),candidate_outcome=D.outcome(reports[1],item['seat']),
            **S.compare_controls(args.out,a,b,reports)))
    assert F.digest(binary) == result['binary_sha256']
    result['complete'] = True
    save()


if __name__ == '__main__': main()
