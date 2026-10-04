#!/usr/bin/env python3
"""Replay the integrated P1 asteroid regression with observational diagnostics.

No policy/physics option changes. Require exact gameplay/evidence parity before
summarizing combat modes, present-aim windows and native damage provenance.
"""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import csv
import importlib.util
from itertools import zip_longest
import json
import math
from pathlib import Path
import subprocess
import traceback


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


I = module('integrated', 'validate-integrated-bot.py')
T = module('threat', 'analyze-transfer-threat.py')
B = module('pod', 'validate-pod-braking.py')
GROUP = 'fresh-world0-v10-asteroids3-p1'
IMPACT_START, END = 7600, 36001
ROOT = Path(__file__).resolve().parents[1]


def rows(path):
    with path.open() as stream:
        for line in stream:
            yield json.loads(line)


def timing_free(value):
    """Remove millisecond measurements only; retain counts and physical clocks."""
    if isinstance(value, dict):
        return {k: timing_free(v) for k, v in value.items() if not k.endswith('_ms')}
    if isinstance(value, list):
        return [timing_free(v) for v in value]
    return value


def command(prior, root):
    result = list(prior['command'])
    flags = ['--trace-start-tick', '--trace-end-tick', '--trace-impact',
             '--impact-start-tick', '--impact-end-tick', '--impact-pod-control']
    assert not any(flag in result for flag in flags)
    result[result.index('--out') + 1] = str(root)
    result += ['--trace-start-tick', '0', '--trace-end-tick', str(END),
               '--trace-impact', 'true', '--impact-start-tick', str(IMPACT_START),
               '--impact-end-tick', str(END), '--impact-pod-control', 'bot']
    return result


def compare_reports(before, after):
    assert before['dense_trace_ticks'] == [0, 0]
    assert after['dense_trace_ticks'] == [0, END]
    left, right = [timing_free({k: v for k, v in r.items() if k != 'dense_trace_ticks'})
                   for r in (before, after)]
    assert left == right, 'non-timing report changed'


def compare_jsonl(before, after):
    count = 0
    for left, right in zip_longest(rows(before), rows(after)):
        assert left is not None and right is not None
        assert timing_free(left) == timing_free(right), f'non-timing stream changed: {before.name}'
        count += 1
    return count


def checked_trace(new_rows, old_rows, ticks):
    """The new stream is dense; every previously recorded row must agree exactly."""
    previous = iter(old_rows)
    retained = next(previous, None)
    matched = 0
    source = iter(new_rows)
    for tick in range(ticks):
        for seat in (0, 1):
            row = next(source, None)
            assert row is not None and (row['tick'], row['seat']) == (tick, seat), 'trace is not dense'
            p = row['observation']['local']['combat']['recovery']['flight']['pilot']
            assert p['tick'] == tick and p['owner'] == f'player_{seat+1}'
            if retained is not None:
                key = retained['tick'], retained['seat']
                assert key >= (tick, seat), 'old trace row missing'
                if key == (tick, seat):
                    assert retained == row, f'original observation changed at {key}'
                    matched += 1
                    retained = next(previous, None)
            yield row
    assert next(source, None) is None and retained is None, 'unexpected trace tail'
    assert matched > 0


def gates(pilot, combat, weapon):
    """Approximate present-aim/readiness window, not a hit or safer-action forecast.

    Use the existing bounded-lead diagnostic. Do not classify f32-adjacent range
    or alignment boundaries: those remain explicitly indeterminate.
    """
    target = combat['target']
    if target is None:
        return dict(blockers=['no_target'], uncertain=False, geometry=None)
    geometry = T.relative_geometry(pilot['ship'], target['motion'])
    assert all(math.isfinite(v) for v in geometry.values() if v is not None)
    distance, aim = geometry['range'], abs(geometry['aim_error_radians'])
    boundaries = [250.] if weapon == 'laser' else [20., 220.]
    uncertain = abs(aim - .08) <= 1.e-5 or any(abs(distance - r) <= 1.e-3 for r in boundaries)
    blockers = []
    if not pilot['controls_armed'] or not pilot['queries_ready']:
        blockers.append('controls_or_queries_unready')
    if (not pilot['ship_available'] or pilot['ship_form'] != 'ship'
            or not isinstance(pilot['location'], dict) or 'aboard' not in pilot['location']):
        blockers.append('not_aboard_full_ship')
    if not target['visible'] or target['ground_occluded']:
        blockers.append('visibility')
    if not combat['laser_available' if weapon == 'laser' else 'cannon_ready']:
        blockers.append('readiness')
    if aim >= .08:
        blockers.append('aim')
    if (weapon == 'laser' and distance > 250.) or (weapon == 'cannon' and not 20. <= distance <= 220.):
        blockers.append('range')
    return dict(blockers=blockers, uncertain=uncertain, geometry=geometry)


def projection(row):
    c, m = row['observation']['local']['combat'], row['mission']
    p = c['recovery']['flight']['pilot']
    combat_goal = (m.get('combat') or {}).get('goal')
    mode = ('combat:' + combat_goal if combat_goal else
            'mission:' + (m.get('reason') or m['goal']))
    action = T.weapon_action(row)
    checks = {weapon: gates(p, c, weapon) for weapon in ('laser', 'cannon')}
    for weapon, check in checks.items():
        if combat_goal == 'engage ship' and not check['uncertain']:
            assert action[weapon] == (not check['blockers']), (row['tick'], row['seat'], weapon, check)
    return dict(tick=row['tick'], seat=row['seat'], mode=mode, goal=m['goal'],
                pursuit=m.get('pursuit'), breaks=(m.get('combat') or {}).get('breaks'),
                hull=p['ship_health'], form=p['ship_form'], vehicle=p['vehicle'],
                losses=(p.get('recovery') or {}).get('ships_lost', 0),
                pilot_health=row['observation']['match_context']['pilot_health'][row['seat']],
                controls=row['controls'], actions=row['actions'], action=action,
                supply=c['supply'], weapons=c['weapons'],
                target_hull=c['target']['health'] if c['target'] else None,
                target_form=c['target']['ship_form'] if c['target'] else None,
                geometry=checks['laser']['geometry'],
                gates={k: {field: value for field, value in v.items() if field != 'geometry'}
                       for k, v in checks.items()})


def phase_summary(points):
    phases = []
    expected = None
    for point in points:
        assert expected is None or point['tick'] == expected, 'phase observations are not contiguous'
        expected = point['tick'] + 1
        if not phases or phases[-1]['mode'] != point['mode']:
            phases.append(dict(mode=point['mode'], start=point['tick'], end=point['tick'],
                               first=point, last=point, counts=Counter()))
        phase = phases[-1]
        assert point['tick'] == phase['end']
        phase['end'] += 1
        phase['last'] = point
        phase['counts']['ticks'] += 1
        for weapon in ('laser', 'cannon'):
            gate = point['gates'][weapon]
            phase['counts'][weapon + '_requests'] += int(point['action'][weapon])
            phase['counts'][weapon + '_window'] += int(not gate['blockers'] and not gate['uncertain'])
            phase['counts'][weapon + '_indeterminate'] += int(gate['uncertain'])
            for reason in gate['blockers']:
                phase['counts'][weapon + '_blocked_' + reason] += 1
    return phases


def loss_receipt(row, loss_tick):
    damage = row['damage']
    assert row['tick'] == loss_tick and damage['last_damage_tick'] == loss_tick
    assert damage['last_ship_lost']
    result = dict(tick=loss_tick, source=damage['last_source'], amount=damage['last_damage_percent'])
    if damage['last_contact_tick'] == loss_tick and damage['last_contact_source'] == damage['last_source']:
        spawn = damage['last_contact_spawn_tick']
        if spawn is not None:
            assert 0 <= spawn < loss_tick
            result.update(spawn_tick=spawn, contact_age_ticks=loss_tick-spawn)
    return result


def analyze_diagnostics(root, old_root, report):
    ticks = report['elapsed_ticks']
    points, trace_witnesses = {0: [], 1: []}, []
    old_count = [0, 0]
    def old_rows():
        for row in rows(old_root / 'trace.jsonl'):
            old_count[row['seat']] += 1
            yield row
    for row in checked_trace(rows(root / 'trace.jsonl'), old_rows(), ticks):
        seat = row['seat']
        point = projection(row)
        previous = points[seat][-1] if points[seat] else None
        if previous is None or point['mode'] != previous['mode'] or point['losses'] != previous['losses']:
            trace_witnesses.append(row)
        points[seat].append(point)
    impact, final = {}, None
    for row in rows(root / 'impact.jsonl'):
        if 'final_tick' in row:
            assert final is None
            final = row
            continue
        assert final is None
        key = row['tick'], row['seat']
        assert key not in impact
        assert not row['overridden'] and row['controls'] == row['bot_controls']
        point = points[row['seat']][row['tick']]
        assert row['goal'] == point['goal'] and row['form'] == point['form']
        assert row['actions'] == point['actions']
        assert {k: row['controls'][k] for k in point['controls']} == point['controls']
        assert T.weapon_action(row) == point['action']
        impact[key] = row
    assert set(impact) == {(t, s) for t in range(IMPACT_START, ticks) for s in (0, 1)}
    assert final and final['final_tick'] == ticks and final['round'] == report['round']
    assert final['config'] == dict(seat=0, control='bot', control_from_tick=0,
                                  trace_start_tick=IMPACT_START, trace_end_tick=END)
    losses, changes, motion = {0: [], 1: []}, [], {0: [], 1: []}
    for seat in (0, 1):
        for point in points[seat]:
            tick = point['tick']
            row = impact.get((tick, seat))
            if row is None:
                continue
            old = impact.get((tick-1, seat))
            if tick and point['losses'] > points[seat][tick-1]['losses']:
                receipt = loss_receipt(row, tick)
                if 'spawn_tick' in receipt:
                    receipt['cannon_request_seats_at_spawn'] = [s for s in (0, 1)
                        if points[s][receipt['spawn_tick']]['action']['cannon']]
                losses[seat].append(dict(receipt=receipt, before=points[seat][tick-1], after=point,
                                         impact=row))
            if old is None or row['damage'] != old['damage'] or row['vitals'] != old['vitals']:
                changes.append(row)
            if row['form'] == 'escape_pod':
                metrics = B.motion_metrics(row)
                motion[seat].append(dict(tick=tick, metrics=metrics, controls=row['controls'],
                                         health=row['vitals']['health'], recovery=row['recovery']))
        pilot = report['final_pilots'][seat]
        final_losses = (pilot.get('recovery') or {}).get('ships_lost', 0)
        if final_losses > points[seat][-1]['losses']:
            receipt = loss_receipt(dict(tick=ticks, damage=final['damage'][seat]), ticks)
            if 'spawn_tick' in receipt:
                receipt['cannon_request_seats_at_spawn'] = [s for s in (0, 1)
                    if points[s][receipt['spawn_tick']]['action']['cannon']]
            losses[seat].append(dict(receipt=receipt, before=points[seat][-1],
                                     after=dict(tick=ticks, pilot=pilot, vitals=report['round']['pilots'][seat]),
                                     impact=dict(motion=final['motion'][seat], damage=final['damage'][seat])))
        assert len(losses[seat]) == final_losses - points[seat][0]['losses'], 'unrecorded ship loss'
    summary = dict(trace_rows_by_seat={s: len(p) for s, p in points.items()},
                   original_trace_rows_matched=old_count,
                   impact_rows=len(impact), overrides=0,
                   phases={s: phase_summary(p) for s, p in points.items()},
                   losses=losses, final_impact=final)
    evidence = dict(points=points, trace_witnesses=trace_witnesses,
                    impact_changes=changes, pod_motion=motion)
    I.write(root.parent / (root.name + '-diagnostics.json'), evidence)
    return summary


def replay(prior, out):
    name = prior['item']['name']
    root = out / name
    old_root = Path(prior['command'][prior['command'].index('--out') + 1])
    cmd = command(prior, root)
    with (out / (name + '.log')).open('x') as log:
        subprocess.run(cmd, check=True, stdout=log, stderr=log, timeout=1800)
    result = dict(item=prior['item'], command=cmd, hashes=I.raw_hashes(root),
                  log_sha256=T.digest(out / (name + '.log')))
    before, after = [json.loads((p / 'report.json').read_text()) for p in (old_root, root)]
    compare_reports(before, after)
    exact = [f for f in prior['hashes'] if f not in ('report.json', 'trace.jsonl', 'sensors.jsonl', 'live-planning.csv')]
    for filename in exact:
        assert result['hashes'][filename] == prior['hashes'][filename], filename
    sensor_rows = compare_jsonl(old_root / 'sensors.jsonl', root / 'sensors.jsonl')
    with (old_root / 'live-planning.csv').open() as a, (root / 'live-planning.csv').open() as b:
        left = [timing_free(row) for row in csv.DictReader(a)]
        right = [timing_free(row) for row in csv.DictReader(b)]
        assert left == right, 'charged planning ledger changed'
    result.update(I.analyze(root, prior, out))
    assert result['players'] == prior['players'] and result['allocation'] == prior['allocation']
    result['parity'] = dict(exact_streams=exact, non_timing_report=True,
                           sensor_rows=sensor_rows, planning_rows=len(left), players=True)
    result['diagnosis'] = analyze_diagnostics(root, old_root, after)
    path = out / (name + '-diagnostics.json')
    result['diagnostic_evidence'] = dict(path=str(path), sha256=T.digest(path))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert __debug__
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip(), 'freeze code and plan first'
    prior = json.loads(args.prior.read_text())
    assert prior['complete'] and len(prior['runs']) == 112
    selected = [prior['runs'][GROUP + '-' + arm] for arm in ('control', 'candidate')]
    binary = Path(prior['binary']['path'])
    assert T.digest(binary) == prior['binary']['sha256']
    for run in selected:
        root = Path(run['command'][run['command'].index('--out') + 1])
        assert I.raw_hashes(root) == run['hashes'], 'changed original evidence'
    args.out.mkdir(parents=True, exist_ok=False)
    inputs = I.tool_inputs()
    inputs.update({str(Path(m.__file__).resolve().relative_to(ROOT)): T.digest(m.__file__) for m in (T, B)})
    inputs[str(Path(__file__).resolve().relative_to(ROOT))] = T.digest(__file__)
    result = dict(schema=1, complete=False, source_commit=subprocess.check_output(
        ['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        prior_summary=dict(path=str(args.prior), sha256=T.digest(args.prior)),
        binary=prior['binary'], inputs=inputs,
        plan=dict(group=GROUP, purpose='observational replay, not an independent strength sample',
                  commands=[command(run, args.out / run['item']['name']) for run in selected],
                  policy_changes=False, default_changes=False), runs={})
    save = lambda: I.write(args.out / 'summary.json', result)
    save()
    try:
        with ThreadPoolExecutor(max_workers=2) as pool:
            futures = {r['item']['name']: pool.submit(replay, r, args.out) for r in selected}
            for name, future in futures.items():
                try:
                    result['runs'][name] = future.result()
                    print(name, 'exact gameplay parity and diagnostics audited', flush=True)
                except Exception:
                    result['runs'][name] = dict(error=traceback.format_exc())
                save()
        assert all('error' not in r for r in result['runs'].values()), 'replay/audit failed'
        assert T.digest(args.prior) == result['prior_summary']['sha256']
        assert T.digest(binary) == result['binary']['sha256']
        for path, expected in inputs.items():
            assert T.digest(ROOT / path) == expected
        for run in selected:
            root = Path(run['command'][run['command'].index('--out') + 1])
            assert I.raw_hashes(root) == run['hashes']
        result['complete'] = True
    finally:
        save()


if __name__ == '__main__':
    main()
