#!/usr/bin/env python3
"""Frozen unsupported-walker trials against the completed-walk feedback corpus."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
import math
from pathlib import Path
import subprocess

spec = importlib.util.spec_from_file_location('feedback', Path(__file__).with_name('validate-walk-feedback.py'))
B = importlib.util.module_from_spec(spec)
spec.loader.exec_module(B)
R, P, M, F = B.R, B.P, B.M, B.F


def arguments(item, feedback):
    return B.arguments(item, True) + (['--cover-walk-bounds', 'true'] if feedback else [])


def same_objective(a, b):
    return (a is not None and b is not None
        and all(a[k] == b[k] for k in ['planet', 'revision', 'owner'])
        and math.hypot(*(a['position'][k]-b['position'][k] for k in ['x', 'y'])) < .5
        and abs(a['range']-b['range']) < .01)


def audit_methods(rows):
    previous, receipts, advances = {}, {}, []
    for row in rows:
        p, e = row['pilot'], row.get('objective_evidence')
        tick, seat = p['tick'], row['seat']
        completed = e.get('exhausted_walk') if e else None
        unsupported = e.get('unsupported_walk') if e else None
        assert not (completed and unsupported), 'unsupported is not a completed walk'
        attempt = completed or unsupported
        kind = 'completed' if completed else 'unsupported'
        if unsupported:
            assert unsupported['max_steps'] == 224
            assert 224 < unsupported['required_steps'] <= 260
        if attempt:
            assert attempt['actor'] == p['owner'] == f'player_{seat+1}'
            assert p['site_query'] == dict(selected=attempt['site'])
            assert e['tick'] == tick and e['generation'] is not None
            assert e['measurement_tick'] <= e['request_tick'] <= tick
            assert 0 <= tick-e['measurement_tick'] <= 120
            assert same_objective(e['objective'], e['source_objective'])
            key = (seat, e['generation'])
            record = dict(kind=kind, seat=seat, tick=tick, generation=e['generation'],
                measurement_tick=e['measurement_tick'], age=tick-e['measurement_tick'], site=attempt['site'])
            if unsupported:
                record['bounds'] = {k: unsupported[k] for k in ['required_steps', 'max_steps']}
            if key in receipts:
                assert receipts[key]['kind'] == kind
                assert receipts[key].get('bounds') == record.get('bounds')
                assert receipts[key]['measurement_tick'] == record['measurement_tick']
                assert receipts[key]['site'] == record['site']
            receipts.setdefault(key, record)
        capture = row.get('capture') or {}
        response = capture.get('cover_response') or {}
        search = response.get('search')
        if not search:
            previous.pop(seat, None)
            continue
        deferred, pending = search.get('walk_deferred', []), search['pending']
        assert search['deadline_tick'] == search['started_tick'] + 600
        assert search['probes'] <= 8 and len(deferred) + len(pending) <= 8
        assert len({(s['planet'], s['bearing']) for s in deferred + pending}) == len(deferred) + len(pending)
        context = tuple(json.dumps(search[k], sort_keys=True) for k in
            ['planet', 'revision', 'objective', 'started_tick', 'deadline_tick'])
        old_context, old_pending, old_deferred = previous.get(seat, (None, [], []))
        if context == old_context and len(deferred) > len(old_deferred):
            assert deferred[:-1] == old_deferred and old_pending[0] == deferred[-1]
            assert attempt and attempt['site'] == deferred[-1]
            assert e['invalidated_by'] is None and e['submission_deferred_by'] is None
            assert search['started_tick'] <= e['measurement_tick'] <= e['request_tick'] <= tick
            assert row['objective_work'] in ['pending', 'ready']
            assert same_objective(search['objective'], e['objective'])
            assert same_objective(search['objective'], e['source_objective'])
            assert search['planet'] == p['planet']['index'] == attempt['site']['planet']
            assert search['revision'] == p['planet']['revision']
            advances.append(dict(kind=kind, row=row))
        previous[seat] = (context, pending, deferred)
    return dict(first_receipts=list(receipts.values()), advances=advances)


def analyze(root, item):
    result = M.analyze(root, item)
    live = json.loads((root/'report.json').read_text())['live_objective_planning']
    assert live['cover_walk_feedback'] is True and live['cover_walk_bounds'] is True
    assert live['sensor_profile'] in ['live_joint_objective_v11', 'live_jetpack_objective_v11']
    feedback = audit_methods(F.rows(root/'capture-evidence.jsonl'))
    failed = sum(m.get('corridor_completed', 0)-m.get('corridor_successes', 0)
        + m.get('focused_completed', 0)-m.get('focused_successes', 0)
        for m in result['live_telemetry']['measurements_by_actor'].values())
    assert sum(r['kind'] == 'completed' for r in feedback['first_receipts']) <= failed
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
    path = root/'walk-bounds-witnesses.json'
    F.D.write(path, dict(schema=1, **feedback, first_deliveries=list(first.values()),
        cover_transitions=transitions, selected_site_witnesses=selected))
    result['feedback'] = dict(first_receipts=feedback['first_receipts'], advances=len(feedback['advances']),
        advances_by_kind={k: sum(a['kind'] == k for a in feedback['advances']) for k in ['completed', 'unsupported']})
    result['delivery'] = dict(first_by_request=list(first.values()), selected_requests=len(selected),
        cover_transitions=len(transitions), witness_sha256=F.E.digest(path))
    result['hashes'][path.name] = F.E.digest(path)
    return result


def run(binary, out, item, feedback):
    root = out/(item['name'] + ('-bounds' if feedback else '-retained'))
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
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [B, B.E, R, P, M, M.E, M.J, M.T, M.V, M.C, F, F.M, F.E, F.D]},
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
                    else:
                        parity = P.replay_parity(old, new)
                        assert parity['reused_ground_before'] == parity['reused_ground_after']
                        new['replay_parity'] = parity
                    print(name + (': bounds audited' if feedback else ': retained exactly'), flush=True)
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
