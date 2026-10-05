#!/usr/bin/env python3
"""Bounded laser-only pursuit-climb comparison on two known integrated games.

First require exact option-off retention, then change only the enabled seat.
Known regressions are qualification cases, never fresh strength evidence.
"""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import copy
import csv
import importlib.util
from itertools import islice, zip_longest
import json
import math
from pathlib import Path
import shutil
import struct
import subprocess
import traceback


spec = importlib.util.spec_from_file_location(
    'diagnosis', Path(__file__).with_name('diagnose-integrated-regression.py'))
D = importlib.util.module_from_spec(spec)
spec.loader.exec_module(D)
I, T, ROOT = D.I, D.T, D.ROOT
PROFILE = 'pursuit_climb_laser_v1'
FIELD = 'pursuit_climb_laser'
FLAG = '--pursuit-climb-laser-seats'
ARMS = ('control', 'candidate')
AIM_LIMIT = struct.unpack('f', struct.pack('f', .08))[0]


def root_of(run):
    return Path(run['command'][run['command'].index('--out') + 1])


def command(prior, binary, root, enabled):
    result = list(prior['command'])
    assert FLAG not in result, 'refuse first-wins option shadowing'
    result[0] = str(binary)
    result[result.index('--out') + 1] = str(root)
    return result + [FLAG, str(prior['item']['seat']) if enabled else 'none']


def without_option(value):
    if isinstance(value, dict):
        return {k: without_option(v) for k, v in value.items() if k != FIELD}
    if isinstance(value, list):
        return [without_option(v) for v in value]
    return value


def audit_actions(native, issued, seat, requested):
    """Flight, wings, cannon, actor and action layout must remain exact."""
    assert len(native) == len(issued) == 3
    before = T.weapon_action(dict(actions=native, seat=seat))
    assert before == dict(laser=False, cannon=False), 'unexpected native climb weapon'
    expected = copy.deepcopy(native)
    expected[2]['Scenario']['payload'][1] = int(requested)
    assert issued == expected, 'climb changed more than the laser bit'


def audit_check(row, trace, previous):
    evidence, mission = row[FIELD], trace['mission']
    telemetry = evidence['telemetry']
    check, observation = telemetry['last'], evidence['observation']
    combat = observation['local']['combat']
    flight = combat['recovery']['flight']
    pilot, target = flight['pilot'], combat['target']
    tick, seat = trace['tick'], trace['seat']
    assert observation == trace['observation']
    assert {k: v for k, v in pilot.items() if k != 'sites'} == row['pilot']
    assert pilot['tick'] == check['tick'] == tick
    assert pilot['owner'] == f'player_{seat+1}'
    assert row['actions'] == trace['actions']
    assert mission[FIELD] == telemetry
    for key in ('pursuit', 'combat', 'reason'):
        assert evidence[key] == mission[key]
    assert mission['goal'] == 'hunt'
    if check['source'] == 'mission':
        assert mission['reason'] == 'climbing for a firing pass'
        assert mission['combat'] is None
    else:
        assert check['source'] == 'combat'
        assert mission['combat']['goal'] == 'climb clear of ground'
        assert check['break_until_tick'] == mission['combat']['breaks']['active_until_tick']
    eligible = (observation['match_rules'] and mission['pursuit'] is not None
                and mission['capture'] is None and mission['recovery'] is None
                and pilot['controls_armed'] and pilot['queries_ready']
                and flight['flight']['enabled'] and pilot['ship_available']
                and pilot['ship_form'] == 'ship' and isinstance(pilot['location'], dict)
                and 'aboard' in pilot['location'])
    until = check['break_until_tick']
    geometry_expected = False
    if not eligible:
        expected = 'unavailable'
    elif until is not None and tick < until:
        expected = 'scheduled_break'
    elif target is None:
        expected = 'no_target'
    elif not target['visible'] or target['ground_occluded']:
        expected = 'occluded'
    elif not combat['laser_available']:
        expected = 'unready'
    else:
        geometry_expected = True
        distance, error = check['distance'], check['heading_error']
        assert all(v is not None and math.isfinite(v) for v in (distance, error))
        geometry = T.relative_geometry(pilot['ship'], target['motion'])
        assert math.isclose(distance, geometry['range'], abs_tol=2.e-3, rel_tol=2.e-6)
        assert abs(math.remainder(error - geometry['aim_error_radians'], 2.*math.pi)) < 2.e-5
        # Use recorded native f32 values for the exact threshold, with an
        # independent approximate geometry check above.
        expected = 'requested' if distance <= 250. and abs(error) < AIM_LIMIT else 'aim_or_range'
    assert check['decision'] == expected
    if not geometry_expected:
        assert check['distance'] is None and check['heading_error'] is None
    requested = expected == 'requested'
    audit_actions(check['native_actions'], row['actions'], seat, requested)
    assert telemetry['checks'] == previous['checks'] + 1
    assert telemetry['requested_ticks'] == previous['requested_ticks'] + int(requested)
    return telemetry


def audit_option(root, item, report):
    seat = item['seat']
    assert report[FIELD]['profile'] == PROFILE
    assert report[FIELD]['enabled_seats'] == [s == seat for s in (0, 1)]
    for s, descriptor in enumerate(report['policy_configuration']):
        assert descriptor.get(FIELD + '_model') == (PROFILE if s == seat else None)
    assert report['policy_configuration'][seat]['policy'] == 'material_mission_v13'
    counters = dict(checks=0, requested_ticks=0, last=None)
    decisions, sources, requests, witnesses = Counter(), Counter(), [], []
    captures, traces = D.rows(root / 'capture-evidence.jsonl'), D.rows(root / 'trace.jsonl')
    total = 0
    for index, (row, trace) in enumerate(zip_longest(captures, traces)):
        assert row is not None and trace is not None, 'mismatched dense streams'
        key = index // 2, index % 2
        assert (row['pilot']['tick'], row['seat']) == (trace['tick'], trace['seat']) == key
        assert row['actions'] == trace['actions']
        m = trace['mission']
        if trace['seat'] != seat:
            assert FIELD not in row and FIELD not in m
        else:
            gate = m[FIELD]
            current = gate['last'] is not None and gate['last']['tick'] == trace['tick']
            assert (FIELD in row) == current
            selected_climb = (m['reason'] == 'climbing for a firing pass'
                              or (m['combat'] or {}).get('goal') == 'climb clear of ground')
            assert current == selected_climb, 'missing or misplaced climb check'
            if current:
                counters = audit_check(row, trace, counters)
                check = counters['last']
                decisions[check['decision']] += 1
                sources[check['source']] += 1
                witnesses.append(row)
                if check['decision'] == 'requested':
                    requests.append(check['tick'])
            assert gate == counters, 'unrecorded telemetry change'
            if (m['combat'] or {}).get('goal', '').startswith('flyby /'):
                assert T.weapon_action(row) == dict(laser=False, cannon=False)
        total += 1
    assert total == report['elapsed_ticks'] * 2
    for s, mission in enumerate(report['missions']):
        assert mission.get(FIELD) == (counters if s == seat else None)
    return dict(counters=counters, decisions=dict(decisions), sources=dict(sources),
                request_ticks=requests, dense_rows=total), witnesses


def replay_parity(prior, run):
    old, new = root_of(prior), root_of(run)
    before, after = [json.loads((p / 'report.json').read_text()) for p in (old, new)]
    assert FIELD not in after
    assert D.timing_free(before) == D.timing_free(after), 'option-off report changed'
    excluded = ('report.json', 'sensors.jsonl', 'live-planning.csv')
    exact = sorted(set(prior['hashes']) - set(excluded))
    assert set(prior['hashes']) == set(run['hashes'])
    for name in exact:
        assert prior['hashes'][name] == run['hashes'][name], f'option-off stream changed: {name}'
    sensors = D.compare_jsonl(old / 'sensors.jsonl', new / 'sensors.jsonl')
    with (old / 'live-planning.csv').open() as a, (new / 'live-planning.csv').open() as b:
        left, right = [[D.timing_free(r) for r in csv.DictReader(f)] for f in (a, b)]
    assert left == right, 'option-off charged planning changed'
    assert prior['players'] == run['players'] and prior['allocation'] == run['allocation']
    return dict(exact_streams=exact, non_timing_report=True, sensor_rows=sensors,
                planning_rows=len(left), exact_players_and_allocation=True)


def compare_prefix(before, after, first_request):
    matched = 0
    for a, b in zip_longest(before, after):
        assert a is not None and b is not None, 'changed match ending before a changed action'
        assert (a['tick'], a['seat']) == (b['tick'], b['seat'])
        stripped = without_option(b)
        if a['actions'] != b['actions']:
            check = b['mission'][FIELD]['last']
            assert a['tick'] == first_request == check['tick'] and check['decision'] == 'requested'
            assert a['actions'] == check['native_actions']
            stripped['actions'] = a['actions']
            assert a == stripped, 'first changed request altered its input or guidance'
            return dict(tick=a['tick'], seat=a['seat'], exact_prefix_rows=matched,
                        native_actions=a['actions'], issued_actions=b['actions'],
                        same_observation_and_guidance=True)
        assert a == stripped, 'state changed before the first laser request'
        matched += 1
    assert first_request is None
    return dict(tick=None, exact_prefix_rows=matched)


def impact_audit(root, report):
    losses, previous, final = {0: [], 1: []}, {}, None
    previous_losses = {}
    traces = iter(islice(D.rows(root / 'trace.jsonl'), D.IMPACT_START * 2, None))
    expected = (D.IMPACT_START, 0)
    count = 0
    for row in D.rows(root / 'impact.jsonl'):
        if 'final_tick' in row:
            assert final is None
            final = row
            continue
        assert final is None and (row['tick'], row['seat']) == expected
        tick, seat = expected
        assert not row['overridden'] and row['controls'] == row['bot_controls']
        trace = next(traces)
        assert (trace['tick'], trace['seat']) == expected
        pilot = trace['observation']['local']['combat']['recovery']['flight']['pilot']
        assert row['actions'] == trace['actions']
        assert row['form'] == pilot['ship_form'] and row['goal'] == trace['mission']['goal']
        assert {k: row['controls'][k] for k in trace['controls']} == trace['controls']
        # Impact `recovery` is task telemetry, not lifetime ship-loss counters.
        # Read the actor's recovery observation from the exact matching tick.
        lost = (pilot['recovery'] or {}).get('ships_lost', 0)
        if seat not in previous:
            assert lost == 0, 'loss predates this diagnostic window'
        elif lost > previous_losses[seat]:
            assert lost == previous_losses[seat] + 1
            losses[seat].append(dict(receipt=D.loss_receipt(row, tick), before=previous[seat], after=row))
        else:
            assert lost == previous_losses[seat], 'lifetime losses decreased'
        previous[seat] = row
        previous_losses[seat] = lost
        expected = (tick, 1) if seat == 0 else (tick + 1, 0)
        count += 1
    ticks = report['elapsed_ticks']
    assert expected == (ticks, 0) and final['final_tick'] == ticks
    assert next(traces, None) is None
    assert final['round'] == report['round']
    assert final['config'] == dict(seat=0, control='bot', control_from_tick=0,
                                 trace_start_tick=D.IMPACT_START, trace_end_tick=D.END)
    for seat in (0, 1):
        lost = (report['final_pilots'][seat]['recovery'] or {}).get('ships_lost', 0)
        if lost > previous_losses[seat]:
            assert lost == previous_losses[seat] + 1
            losses[seat].append(dict(receipt=D.loss_receipt(dict(tick=ticks, damage=final['damage'][seat]), ticks),
                                     before=previous[seat], after=final))
        assert len(losses[seat]) == lost, 'ship loss outside recorded evidence'
    return dict(rows=count, overrides=0, losses=losses, final=final)


def run_case(prior, job, out, previous=None):
    root = root_of(job)
    result = copy.deepcopy(job)
    log = root.parent / (root.name + '.log')
    try:
        if previous is None:
            assert not root.exists()
            with log.open('x') as stream:
                subprocess.run(job['command'], check=True, stdout=stream, stderr=stream, timeout=1800)
        else:
            assert all(previous[key] == job[key] for key in ('command', 'item', 'enabled', 'prior'))
            assert I.raw_hashes(root) == previous['hashes'], 'changed completed raw game'
            assert T.digest(log) == previous['log_sha256']
            result['reused_raw'] = True
        result['hashes'] = I.raw_hashes(root)
        result.update(I.analyze(root, job, out))
        report = json.loads((root / 'report.json').read_text())
        if not job['enabled']:
            result['parity'] = replay_parity(prior, result)
        else:
            result['laser'], checks = audit_option(root, job['item'], report)
            requests = result['laser']['request_ticks']
            result['first_difference'] = compare_prefix(D.rows(root_of(prior) / 'trace.jsonl'),
                D.rows(root / 'trace.jsonl'), requests[0] if requests else None)
            impact = impact_audit(root, report)
            path = out / (root.name + '-laser-evidence.json')
            I.write(path, dict(checks=checks, impact=impact))
            result['evidence'] = dict(path=str(path), sha256=T.digest(path))
            result['impact'] = dict(rows=impact['rows'], overrides=0,
                losses={s: [r['receipt'] for r in rows] for s, rows in impact['losses'].items()})
        if previous is not None:
            assert I.raw_hashes(root) == previous['hashes']
        result['audited'] = True
    except Exception:
        result['error'] = traceback.format_exc()
    finally:
        if root.exists():
            result['hashes'] = I.raw_hashes(root)
        if log.exists():
            result['log_sha256'] = T.digest(log)
    return result


def verify_prior(prior):
    assert prior['complete'] and set(prior['runs']) == {D.GROUP + '-' + arm for arm in ARMS}
    assert T.digest(prior['binary']['path']) == prior['binary']['sha256']
    for run in prior['runs'].values():
        assert I.raw_hashes(root_of(run)) == run['hashes'], 'retained raw evidence changed'
        for field in ('audit', 'diagnostic_evidence'):
            assert T.digest(run[field]['path']) == run[field]['sha256']
    for path, digest in prior['inputs'].items():
        assert T.digest(ROOT / path) == digest, 'retained diagnostic tool changed'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior', type=Path, required=True)
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--previous', type=Path, help='re-audit four hash-bound games without rerunning them')
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert __debug__
    assert bool(args.binary) != bool(args.previous), 'choose a new binary or saved games to re-audit'
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip(), 'freeze code, tests and plan first'
    prior = json.loads(args.prior.read_text())
    verify_prior(prior)
    previous = json.loads(args.previous.read_text()) if args.previous else None
    if previous is not None:
        assert previous['profile'] == PROFILE
        assert previous['prior_summary']['sha256'] == T.digest(args.prior)
        assert T.digest(previous['binary']['path']) == previous['binary']['sha256']
        assert set(previous['runs']) == {a + suffix for a in ARMS for suffix in ('-retained', '-laser')}
    args.out.mkdir(parents=True, exist_ok=False)
    if previous is not None:
        binary = Path(previous['binary']['path'])
    else:
        binary = (args.out / 'surface_mission_soak').resolve()
        shutil.copy2(args.binary, binary)
        binary.chmod(0o555)
    inputs = dict(prior['inputs'])
    inputs[str(Path(__file__).resolve().relative_to(ROOT))] = T.digest(__file__)
    jobs = []
    for enabled in (False, True):
        for arm in ARMS:
            old = prior['runs'][D.GROUP + '-' + arm]
            name = arm + ('-laser' if enabled else '-retained')
            item = dict(old['item'], name=name, stage='known_qualification')
            root = root_of(previous['runs'][name]) if previous is not None else args.out / name
            jobs.append(dict(item=item, enabled=enabled, prior=old['item']['name'],
                command=command(old, binary, root, enabled)))
    result = dict(schema=1, complete=False, profile=PROFILE,
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
        binary=dict(path=str(binary), sha256=T.digest(binary)), inputs=inputs,
        prior_summary=dict(path=str(args.prior), sha256=T.digest(args.prior)),
        plan=dict(purpose='known qualification; no new independent worlds or default promotion',
                  jobs=jobs, default_changes=False, tuning_after_results=False), runs={})
    if previous is not None:
        assert result['plan'] == previous['plan'], 're-audit cannot change the frozen plan'
        result['previous_summary'] = dict(path=str(args.previous), sha256=T.digest(args.previous))
    result['runtime_source_commit'] = (previous.get('runtime_source_commit', previous['source_commit'])
                                       if previous is not None else result['source_commit'])
    save = lambda: I.write(args.out / 'summary.json', result)
    save()
    try:
        for enabled in (False, True):
            phase = [job for job in jobs if job['enabled'] == enabled]
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures = [pool.submit(run_case, prior['runs'][j['prior']], j, args.out,
                    previous['runs'][j['item']['name']] if previous is not None else None) for j in phase]
                # Drain and retain both results even if either audit fails.
                for job, future in zip(phase, futures):
                    run = future.result()
                    result['runs'][job['item']['name']] = run
                    save()
                    print(job['item']['name'], 'audited' if run.get('audited') else 'FAILED', flush=True)
            assert all(result['runs'][j['item']['name']].get('audited') for j in phase), 'phase failed; raw evidence retained'
        verify_prior(prior)
        assert T.digest(args.prior) == result['prior_summary']['sha256']
        assert T.digest(binary) == result['binary']['sha256']
        for path, digest in inputs.items():
            assert T.digest(ROOT / path) == digest
        if previous is not None:
            assert T.digest(args.previous) == result['previous_summary']['sha256']
        result['complete'] = True
    finally:
        save()


if __name__ == '__main__':
    main()
