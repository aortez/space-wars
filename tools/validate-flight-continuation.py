#!/usr/bin/env python3
"""Frozen active-flight checks against the local powered-route corpus."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
import math
from pathlib import Path
import subprocess

spec = importlib.util.spec_from_file_location('routes', Path(__file__).with_name('validate-powered-routes.py'))
R = importlib.util.module_from_spec(spec)
spec.loader.exec_module(R)
P, M, F = R.P, R.M, R.F


def arguments(item, enabled):
    return R.arguments(item, True) + (['--active-flight-checks', 'true'] if enabled else [])


def retention_plan():
    return [p for p in P.plan() if p['kind'] == 'directed' or p['name'] == 'shared-armed-world1-p1-powered']


def audit_prediction(c, tick, charge):
    request = c['request']
    launch = request['launch']
    assert c['version'] == 1 and c['tick'] == tick and c['charge'] == charge
    assert launch['version'] == 2 and launch['launch_until_tick'] == launch['measured_tick'] + 120
    assert launch['measured_tick'] <= request['launched_tick'] <= launch['launch_until_tick']
    assert 0 <= tick - request['launched_tick']
    assert request['phase'] in ['Lift', 'Cross', 'Descend']
    assert M.same_corridor(request['plan'], launch['plan'])
    if c['rejection'] is not None:
        assert c['remaining'] is None and c['plan'] is None
        return False
    assert tick - request['launched_tick'] < 720
    assert math.isfinite(charge) and 0.05 <= charge <= 1.0
    assert M.same_corridor(request['plan'], c['plan'])
    assert request['plan']['direction'] == c['plan']['direction']
    f = c['remaining']
    assert all(math.isfinite(f[k]) for k in ['seconds', 'burn_seconds', 'arrival_speed'])
    assert 0 <= f['seconds'] <= (720 - (tick-request['launched_tick']))/60.0 + 1e-6
    assert 0 <= f['burn_seconds'] <= (charge-0.05)*3.0 + 1e-6
    assert 0 <= f['arrival_speed'] <= 7.0
    return True


def audit_continuations(rows):
    latest, presses, flights, previous = {}, {}, {}, {}
    approved, rejected, without_launch, witnesses = 0, Counter(), 0, []
    for row in rows:
        seat, p = row['seat'], row['pilot']
        tick = p['tick']
        j = row.get('jetpack')
        g = (row.get('capture') or {}).get('ground')
        if j and j.get('vehicle_forecast'):
            latest[seat] = j['vehicle_forecast']
        if j and g and g['goal'] == 'jetpack_lift' and j['charge'] >= 0.98:
            action = row['actions'][0]['Scenario']
            assert action['kind'] == 0x53550002 and action['payload'][7] == seat
            if action['payload'][4] == 1:
                presses[seat, tick] = dict(row=row, forecast=latest.get(seat))
        if j and (c := j.get('vehicle_continuation')):
            request, born = c['request'], c['request']['launched_tick']
            key = (seat, born)
            assert j['surveyed'] and key in presses
            press = presses[key]
            assert request['launch'] == press['forecast']
            assert M.same_corridor(request['plan'], press['row']['capture']['ground']['crossing']['plan'])
            assert request['plan']['anchor']['Vehicle']['index'] == p['vehicle']
            old_ground = previous[seat]
            assert old_ground and old_ground['crossing']['goal'] == request['phase']
            assert old_ground['crossing']['plan'] == request['plan']
            assert flights.setdefault(key, request['launch']) == request['launch']
            if audit_prediction(c, tick, j['charge']):
                approved += 1
                without_launch += int(j.get('vehicle_forecast') is None)
            else:
                rejected[c['rejection']] += 1
                if g:
                    assert g['goal'] in ['settle', 'blocked']
                assert row['actions'][0]['Scenario']['payload'][4] == 0
            witnesses.append(row)
        previous[seat] = g
    return dict(approved=approved, rejected=dict(rejected), flights=len(flights),
        approved_without_launch_forecast=without_launch, witnesses=witnesses,
        launch_presses=[presses[k]['row'] for k in flights])


def analyze(root, item):
    result = R.analyze(root, item)
    report = json.loads((root/'report.json').read_text())
    config = report['active_flight_checks']
    assert config['profile'] == 'vehicle_flight_continuation_v1'
    assert config['enabled_seats'] == [item['powered'] and i == item['seat'] for i in range(2)]
    continuation = audit_continuations(F.rows(root/'capture-evidence.jsonl'))
    path = root/'flight-continuation-witnesses.json'
    F.D.write(path, dict(schema=1, **continuation))
    result['continuation'] = {k:v for k,v in continuation.items() if k not in ['witnesses', 'launch_presses']}
    result['hashes'][path.name] = F.E.digest(path)
    return result


def run(binary, out, item, enabled):
    root = out/(item['name'] + ('-continuation' if enabled else '-retained'))
    command = [str(binary), *arguments(item, enabled), '--out', str(root)]
    result = dict(item=item, command=command)
    try:
        with (out/(root.name+'.log')).open('x') as log:
            subprocess.run(command, check=True, stdout=log, stderr=log, timeout=1800)
        result.update((analyze if enabled else R.analyze)(root, item))
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
    R.R.verify_inputs(prior)
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        binary_sha256=F.E.digest(binary), prior_summary=dict(path=str(args.prior), sha256=F.E.digest(args.prior)),
        tools={name:F.E.digest(Path(__file__).with_name(name)) for name in [*prior['tools'], 'validate-powered-routes.py']},
        runner_sha256=F.E.digest(Path(__file__)), plan=P.plan(), retention_plan=retention_plan(), retention={}, runs={}, comparisons={})
    save = lambda: F.D.write(args.out/'summary.json', result)
    save()
    try:
        for enabled, items in [(False, retention_plan()),
                               (True, [p for p in P.plan() if p['kind'] == 'directed']),
                               (True, [p for p in P.plan() if p['kind'] == 'armed'])]:
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures = [pool.submit(run, binary, args.out, item, enabled) for item in items]
                for item, future in zip(items, futures):
                    name, new = item['name'], future.result()
                    (result['runs'] if enabled else result['retention'])[name] = new
                    save()
                    assert 'error' not in new, (name, new.get('error'))
                    old = prior['runs'][name]
                    if enabled:
                        result['comparisons'][name] = M.compare(P.root_of(old), P.root_of(new))
                        if not item['powered']:
                            new['walking_retention'] = P.replay_parity(old, new)
                    else:
                        new['replay_parity'] = P.replay_parity(old, new)
                    print(name + (': continuation audited' if enabled else ': retained exactly'), flush=True)
                    save()
        R.R.verify_inputs(prior)
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
