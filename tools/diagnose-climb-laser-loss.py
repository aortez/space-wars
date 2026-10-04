#!/usr/bin/env python3
"""Observe the frozen fresh-world climb-laser lost win; never change gameplay."""
import argparse
from concurrent.futures import ProcessPoolExecutor
import copy
import csv
import importlib.util
from itertools import zip_longest
import json
from pathlib import Path
import subprocess
import traceback


spec = importlib.util.spec_from_file_location(
    'comparison', Path(__file__).with_name('compare-pursuit-climb-laser.py'))
B = importlib.util.module_from_spec(spec)
spec.loader.exec_module(B)
D, I, P, ROOT = B.D, B.I, B.P, B.ROOT
GROUP = 'fresh-world2-v10-asteroids0-p1-candidate'
END = 36001


def command(prior, root):
    result = list(prior['command'])
    flags = result[1::2]
    assert len(result) % 2 == 1 and len(flags) == len(set(flags))
    assert not any(flag.startswith('--impact-') or flag == '--trace-impact' for flag in flags)
    assert result[result.index('--trace-start-tick') + 1] == '0'
    assert result[result.index('--trace-end-tick') + 1] == str(END)
    result[result.index('--out') + 1] = str(root)
    return result + ['--trace-impact', 'true', '--impact-start-tick', '0',
                     '--impact-end-tick', str(END), '--impact-pod-control', 'bot']


def compare_reports(before, after):
    assert before['dense_trace_ticks'] == after['dense_trace_ticks'] == [0, END]
    assert D.timing_free(before) == D.timing_free(after), 'non-timing report changed'


def point(row):
    value = D.projection(row)
    pilot = row['observation']['local']['combat']['recovery']['flight']['pilot']
    value.update(ship=pilot['ship'], actor=pilot['actor'], location=pilot['location'],
                 recovery=pilot['recovery'], armed=pilot['controls_armed'],
                 mission_recovery=row['mission']['recovery'],
                 non_laser_actions=without_laser(row['actions']))
    return value


def without_laser(actions):
    result = copy.deepcopy(actions)
    for action in result:
        value = action.get('Scenario', {})
        if value.get('kind') == 1398079493:
            assert len(value['payload']) == 3
            value['payload'][1] = 0
    return result


def join_impact(row, trace, expected):
    assert row['schema'] == 1 and (row['tick'], row['seat']) == expected
    assert (trace['tick'], trace['seat']) == expected
    pilot = trace['observation']['local']['combat']['recovery']['flight']['pilot']
    assert pilot['tick'] == expected[0] and pilot['owner'] == f'player_{expected[1]+1}'
    assert not row['overridden'] and row['controls'] == row['bot_controls']
    assert row['actions'] == trace['actions'] and row['goal'] == trace['mission']['goal']
    assert {k: row['controls'][k] for k in trace['controls']} == trace['controls']
    assert (row['ship'], row['form'], row['location']) == (
        pilot['ship'], pilot['ship_form'], pilot['location'])
    assert row['controls_armed'] == pilot['controls_armed']
    assert row['vitals']['health'] == trace['observation']['match_context']['pilot_health'][expected[1]]
    return (pilot['recovery'] or {}).get('ships_lost', 0)


def loss_event(row, previous_losses, losses):
    assert losses in (previous_losses, previous_losses + 1), 'lifetime loss counter jumped'
    return D.loss_receipt(row, row['tick']) if losses > previous_losses else None


def analyze_impact(root, report):
    points, previous, counts = {0: [], 1: []}, {}, {0: 0, 1: 0}
    losses, changes, pod = {0: [], 1: []}, [], {0: [], 1: []}
    impacts = iter(D.rows(root / 'impact.jsonl'))
    traces = iter(D.rows(root / 'trace.jsonl'))
    ticks = report['elapsed_ticks']
    for tick in range(ticks):
        for seat in (0, 1):
            row, trace = next(impacts), next(traces)
            count = join_impact(row, trace, (tick, seat))
            value = point(trace)
            receipt = loss_event(row, counts[seat], count)
            if receipt:
                losses[seat].append(dict(receipt=receipt, before=previous.get(seat), after=row))
            old = previous.get(seat)
            if old is None or row['damage'] != old['damage'] or row['vitals'] != old['vitals'] or row['form'] != old['form'] or row['location'] != old['location']:
                changes.append(dict(before=old, after=row))
            # On-foot motion belongs to the pilot body, not the parked pod.
            if row['form'] == 'escape_pod' and isinstance(row['location'], dict) and 'aboard' in row['location']:
                pod[seat].append(dict(tick=tick, metrics=D.B.motion_metrics(row),
                    controls=row['controls'], armed=row['controls_armed'], vitals=row['vitals']))
            points[seat].append(value)
            previous[seat], counts[seat] = row, count
    final = next(impacts)
    assert next(impacts, None) is None and next(traces, None) is None, 'unexpected trace tail'
    assert final['schema'] == 1 and final['final_tick'] == ticks and final['round'] == report['round']
    assert final['config'] == dict(seat=0, control='bot', control_from_tick=0,
                                 trace_start_tick=0, trace_end_tick=END)
    for seat in (0, 1):
        count = (report['final_pilots'][seat]['recovery'] or {}).get('ships_lost', 0)
        receipt = loss_event(dict(tick=ticks, damage=final['damage'][seat]), counts[seat], count)
        if receipt:
            losses[seat].append(dict(receipt=receipt, before=previous[seat], after=final))
        assert len(losses[seat]) == count, 'unrecorded ship loss'
        for loss in losses[seat]:
            receipt = loss['receipt']
            if 'spawn_tick' in receipt:
                receipt['cannon_request_seats_at_spawn'] = [s for s in (0, 1)
                    if points[s][receipt['spawn_tick']]['action']['cannon']]
    evidence = dict(points=points, impact_changes=changes, pod_motion=pod,
                    losses=losses, final=final, pilot_damage_events=report['pilot_damage_events'])
    path = root.parent / (root.name + '-diagnostics.json')
    I.write(path, evidence)
    return dict(rows=ticks * 2, overrides=0, losses=losses, final=final,
                phases={s: D.phase_summary(p) for s, p in points.items()},
                evidence=dict(path=str(path), sha256=P.digest(path)))


def differences(off, on):
    """First difference per physical/control field; unequal endings are explicit."""
    result, common = {}, {0: 0, 1: 0}
    keys = ('action', 'non_laser_actions', 'controls', 'ship', 'actor', 'hull', 'form', 'location', 'recovery',
            'pilot_health', 'supply', 'weapons', 'target_hull', 'target_form', 'mode', 'goal', 'pursuit')
    for left, right in zip_longest(off, on):
        if left is None or right is None:
            continue
        assert (left['tick'], left['seat']) == (right['tick'], right['seat'])
        seat = left['seat']
        common[seat] += 1
        for key in keys:
            name = f'p{seat+1}:{key}'
            if left[key] != right[key] and name not in result:
                result[name] = dict(tick=left['tick'], off=left[key], on=right[key])
    return dict(common_ticks_by_seat=common, first=result)


def replay(prior, inputs, out):
    name = prior['item']['name']
    root, old = out / name, inputs / name
    cmd = command(prior, root)
    with (out / (name + '.log')).open('x') as log:
        subprocess.run(cmd, check=True, stdout=log, stderr=log, timeout=1800)
    result = dict(item=prior['item'], command=cmd, hashes=I.raw_hashes(root),
                  log_sha256=P.digest(out / (name + '.log')))
    # Persist hashes before any audit: audit failures never require another game.
    I.write(out / (name + '-raw.json'), result)
    before, after = [json.loads((p / 'report.json').read_text()) for p in (old, root)]
    compare_reports(before, after)
    assert set(result['hashes']) == set(prior['hashes']) | {'impact.jsonl'}
    exact = sorted(set(prior['hashes']) - {'report.json', 'sensors.jsonl', 'live-planning.csv'})
    for filename in exact:
        assert result['hashes'][filename] == prior['hashes'][filename], filename
    sensor_rows = D.compare_jsonl(old / 'sensors.jsonl', root / 'sensors.jsonl')
    with (old / 'live-planning.csv').open() as a, (root / 'live-planning.csv').open() as b:
        left = [D.timing_free(row) for row in csv.DictReader(a)]
        right = [D.timing_free(row) for row in csv.DictReader(b)]
        assert left == right, 'charged planning ledger changed'
    result.update(I.analyze(root, prior, out))
    for key in ('players', 'allocation', 'continuation', 'route_summary', 'prediction_outcomes', 'retry'):
        # JSON normalizes integer dictionary keys in the saved prior summary.
        assert json.loads(json.dumps(result[key])) == prior[key], key
    laser, witnesses = B.audit_laser(root, prior['item'], after)
    assert laser == prior['laser']
    I.write(out / (name + '-laser.json'), dict(summary=laser, witnesses=witnesses))
    result.update(laser=laser, parity=dict(exact_streams=exact, non_timing_report=True,
        sensor_rows=sensor_rows, planning_rows=len(left), derived_audits=True),
        diagnosis=analyze_impact(root, after))
    assert I.raw_hashes(root) == result['hashes']
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior', type=Path, required=True)
    parser.add_argument('--inputs', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert __debug__
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip(), 'freeze runner and plan first'
    prior = json.loads(args.prior.read_text())
    assert prior['complete'] and len(prior['runs']) == 192
    selected = [prior['runs'][GROUP + '-' + arm] for arm in ('off', 'on')]
    inputs = B.tool_inputs()
    for name in ('tools/diagnose-climb-laser-loss.py', 'tools/tests/test_climb_laser_loss.py',
                 'docs/pursuit-climb-laser-loss-plan.md'):
        inputs[name] = P.digest(ROOT / name)
    assert P.digest(prior['binary']['path']) == prior['binary']['sha256']
    for run in selected:
        files = {p.name: dict(sha256=P.digest(p), bytes=p.stat().st_size)
                 for p in (args.inputs / run['item']['name']).iterdir()}
        assert files == run['archive']['files'], 'changed extracted evidence'
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, source_commit=subprocess.check_output(
        ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        prior_summary=dict(path=str(args.prior), sha256=P.digest(args.prior)),
        binary=prior['binary'], inputs=inputs,
        plan=dict(group=GROUP, observational_only=True, default_changes=False,
                  commands=[command(run, args.out / run['item']['name']) for run in selected]), runs={})
    save = lambda: I.write(args.out / 'summary.json', result)
    save()
    try:
        with ProcessPoolExecutor(max_workers=2) as pool:
            futures = {r['item']['name']: pool.submit(replay, r, args.inputs, args.out) for r in selected}
            for name, future in futures.items():
                try:
                    result['runs'][name] = future.result()
                    print(name, 'exact gameplay parity and diagnostics audited', flush=True)
                except Exception:
                    result['runs'][name] = dict(error=traceback.format_exc())
                    print(name, result['runs'][name]['error'], flush=True)
                save()
        assert all('error' not in r for r in result['runs'].values()), 'replay/audit failed'
        roots = [args.out / r['item']['name'] for r in selected]
        result['differences'] = differences(*(map(point, D.rows(root / 'trace.jsonl')) for root in roots))
        for name, digest in inputs.items():
            assert P.digest(ROOT / name) == digest
        assert P.digest(args.prior) == result['prior_summary']['sha256']
        assert P.digest(prior['binary']['path']) == prior['binary']['sha256']
        for run in selected:
            assert {p.name: dict(sha256=P.digest(p), bytes=p.stat().st_size)
                    for p in (args.inputs / run['item']['name']).iterdir()} == run['archive']['files']
        result['complete'] = True
    finally:
        save()


if __name__ == '__main__':
    main()
