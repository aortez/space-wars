#!/usr/bin/env python3
"""Frozen local powered-route delivery trials against the walking-bound corpus."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
from pathlib import Path
import subprocess

spec = importlib.util.spec_from_file_location('bounds', Path(__file__).with_name('validate-walk-bounds.py'))
B = importlib.util.module_from_spec(spec)
spec.loader.exec_module(B)
R, P, M, F = B.R, B.P, B.M, B.F


def arguments(item, feedback):
    return B.arguments(item, True) + (['--powered-objective-routes', 'true'] if feedback else [])


def audit_powered(rows):
    first = {}
    for row in rows:
        survey = row.get('landing_objective')
        if not survey:
            continue
        for route in survey['sites'] + ([survey['actual']] if survey.get('actual') else []):
            crossing = route.get('crossing')
            if not crossing:
                continue
            M.audit_publication(row, 'jetpack_round_trip')
            evidence = row['objective_evidence']
            assert crossing['measured_tick'] == survey['tick'] == evidence['measurement_tick']
            assert crossing['launch_until_tick'] == survey['tick'] + 120
            assert route['endpoint'] is not None and route['outbound']['failure'] is None
            assert route['returning'] is not None and route['returning']['failure'] is None
            assert route['outbound']['flights'] + route['returning']['flights'] > 0
            key = (row['seat'], evidence['generation'], json.dumps(route['site'], sort_keys=True))
            first.setdefault(key, row)
    return list(first.values())


def analyze(root, item):
    result = M.analyze(root, item)
    live = json.loads((root/'report.json').read_text())['live_objective_planning']
    assert live['powered_objective_routes'] is True
    assert live['cover_walk_feedback'] is True and live['cover_walk_bounds'] is True
    assert live['sensor_profile'] in ['live_joint_objective_v12', 'live_jetpack_objective_v12']
    feedback = B.audit_methods(F.rows(root/'capture-evidence.jsonl'))
    failed = sum(m.get('corridor_completed', 0)-m.get('corridor_successes', 0)
        + m.get('focused_completed', 0)-m.get('focused_successes', 0)
        for m in result['live_telemetry']['measurements_by_actor'].values())
    assert sum(r['kind'] == 'completed' for r in feedback['first_receipts']) <= failed
    powered = audit_powered(F.rows(root/'capture-evidence.jsonl'))
    first, transitions, prior_keys, selected, seen = {}, [], {}, [], set()
    for row in F.rows(root/'capture-evidence.jsonl'):
        delivery = P.publication(row)
        if delivery:
            first.setdefault((row['seat'], delivery['generation']), delivery)
            key = (row['seat'], delivery['generation'])
            if delivery['matching_capture_site'] and key not in seen:
                selected.append(row)
                seen.add(key)
        key = R.cover_key(row)
        if key != prior_keys.get(row['seat']):
            transitions.append(row)
            prior_keys[row['seat']] = key
    path = root/'powered-route-witnesses.json'
    F.D.write(path, dict(schema=1, **feedback, first_powered_deliveries=powered, first_deliveries=list(first.values()),
        cover_transitions=transitions, selected_site_witnesses=selected))
    result['feedback'] = dict(first_receipts=feedback['first_receipts'], advances=len(feedback['advances']),
        advances_by_kind={k: sum(a['kind'] == k for a in feedback['advances']) for k in ['completed', 'unsupported']})
    result['powered_delivering_requests'] = len(powered)
    result['delivery'] = dict(first_by_request=list(first.values()), selected_requests=len(selected),
        cover_transitions=len(transitions), witness_sha256=F.E.digest(path))
    result['hashes'][path.name] = F.E.digest(path)
    return result


def run(binary, out, item, feedback):
    root = out/(item['name'] + ('-powered-routes' if feedback else '-retained'))
    command = [str(binary), *arguments(item, feedback), '--out', str(root)]
    result = dict(item=item, command=command)
    try:
        with (out/(root.name+'.log')).open('w') as log:
            subprocess.run(command, check=True, stdout=log, stderr=log, timeout=1800)
        result.update((analyze if feedback else B.analyze)(root, item))
    except Exception as error:
        result['error'] = repr(error)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip(), 'freeze code, tests and plan first'
    prior = json.loads(args.prior.read_text())
    assert prior['complete'] and prior['plan'] == P.plan()
    R.verify_inputs(prior)
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        binary_sha256=F.E.digest(binary), prior_summary=dict(path=str(args.prior), sha256=F.E.digest(args.prior)),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [B, B.B, B.B.E, R, P, M, M.E, M.J, M.T, M.V, M.C, F, F.M, F.E, F.D]},
        runner_sha256=F.E.digest(Path(__file__)), plan=P.plan(), retention={}, runs={}, comparisons={})
    save = lambda: F.D.write(args.out/'summary.json', result)
    save()
    try:
        for feedback, phase in [(False, 'directed'), (True, 'directed'), (True, 'armed')]:
            items = [p for p in P.plan() if p['kind'] == phase]
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures = [pool.submit(run, binary, args.out, item, feedback) for item in items]
                for item, future in zip(items, futures):
                    name, new = item['name'], future.result()
                    (result['runs'] if feedback else result['retention'])[name] = new
                    save()
                    assert 'error' not in new, (name, new.get('error'))
                    old = prior['runs'][name]
                    if feedback:
                        result['comparisons'][name] = M.compare(P.root_of(old), P.root_of(new))
                        if not item['powered']:
                            new['walking_retention'] = P.replay_parity(old, new)
                    else:
                        parity = P.replay_parity(old, new)
                        assert parity['reused_ground_before'] == parity['reused_ground_after']
                        new['replay_parity'] = parity
                    print(name + (': powered routes audited' if feedback else ': retained exactly'), flush=True)
                    save()
        R.verify_inputs(prior)
        assert F.E.digest(args.prior) == result['prior_summary']['sha256']
        assert F.E.digest(binary) == result['binary_sha256']
        result['complete'] = True
    except BaseException as error:
        result['error'] = repr(error)
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
