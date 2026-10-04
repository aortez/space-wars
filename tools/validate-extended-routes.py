#!/usr/bin/env python3
"""Frozen extended-corridor trials against the requested-route corpus."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
from pathlib import Path
import subprocess

spec = importlib.util.spec_from_file_location('requested', Path(__file__).with_name('validate-requested-routes.py'))
R = importlib.util.module_from_spec(spec)
spec.loader.exec_module(R)
P, M, F = R.P, R.M, R.F


def arguments(item, extended):
    return R.arguments(item, True) + (['--extended-objective-routes', 'true'] if extended else [])


def analyze(root, item):
    result = M.analyze(root, item)
    live = json.loads((root/'report.json').read_text())['live_objective_planning']
    assert live['extended_objective_routes'] is True
    assert live['sensor_profile'] in ['live_joint_objective_v9', 'live_jetpack_objective_v9']
    first, transitions, prior_keys, selected, seen = {}, [], {}, [], set()
    for row in F.rows(root/'capture-evidence.jsonl'):
        delivery = P.publication(row)
        if delivery:
            routes = row['landing_objective']['sites'] + ([row['landing_objective']['actual']] if row['landing_objective']['actual'] else [])
            delivery['outbound_sample_spans'] = [min((r['endpoint']['id']-r['outbound']['start_node']) % 512,
                (r['outbound']['start_node']-r['endpoint']['id']) % 512) for r in routes]
            first.setdefault((row['seat'], delivery['generation']), delivery)
            key = (row['seat'], delivery['generation'])
            if delivery['matching_capture_site'] and key not in seen:
                selected.append(row)
                seen.add(key)
        key = R.cover_key(row)
        if key != prior_keys.get(row['seat']):
            transitions.append(row)
            prior_keys[row['seat']] = key
    path = root/'extended-deliveries.json'
    F.D.write(path, dict(schema=1, first_deliveries=list(first.values()),
        cover_transitions=transitions, selected_site_witnesses=selected))
    result['delivery'] = dict(first_by_request=list(first.values()), selected_requests=len(selected),
        cover_transitions=len(transitions), witness_sha256=F.E.digest(path))
    result['hashes'][path.name] = F.E.digest(path)
    return result


def run(binary, out, item, extended):
    root = out/(item['name'] + ('-extended' if extended else '-retained'))
    command = [str(binary), *arguments(item, extended), '--out', str(root)]
    result = dict(item=item, command=command)
    try:
        with (out/(root.name+'.log')).open('w') as log:
            subprocess.run(command, check=True, stdout=log, stderr=log, timeout=1800)
        result.update((analyze if extended else R.analyze)(root, item))
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
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [R, P, M, M.E, M.J, M.T, M.V, M.C, F, F.M, F.E, F.D]},
        runner_sha256=F.E.digest(Path(__file__)), plan=P.plan(), retention={}, runs={}, comparisons={})
    save = lambda: F.D.write(args.out/'summary.json', result)
    save()
    try:
        for extended, phase in [(False, 'directed'), (True, 'directed'), (True, 'armed')]:
            items = [p for p in P.plan() if p['kind'] == phase]
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures = [pool.submit(run, binary, args.out, item, extended) for item in items]
                for item, future in zip(items, futures):
                    name, new = item['name'], future.result()
                    (result['runs'] if extended else result['retention'])[name] = new
                    save()
                    assert 'error' not in new, (name, new.get('error'))
                    old = prior['runs'][name]
                    if extended:
                        result['comparisons'][name] = M.compare(P.root_of(old), P.root_of(new))
                    else:
                        parity = P.replay_parity(old, new)
                        assert parity['reused_ground_before'] == parity['reused_ground_after']
                        new['replay_parity'] = parity
                    print(name + (': extended audited' if extended else ': retained exactly'), flush=True)
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
