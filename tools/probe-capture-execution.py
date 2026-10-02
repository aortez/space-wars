#!/usr/bin/env python3
"""Paired physical capture attempts at the frozen jetpack study's seven clocks."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import subprocess

spec = importlib.util.spec_from_file_location('jetpack', Path(__file__).with_name('probe-capture-jetpack.py'))
J = importlib.util.module_from_spec(spec)
spec.loader.exec_module(J)
T, P, V, C, F = J.T, J.P, J.V, J.C, J.F


def physical(p):
    return {k: v for k, v in p.items() if k not in ['site_query', 'sites']}


def audit_launch(launch):
    assert launch['equipment']['charge'] >= 0.98
    forecast = launch['latest_forecast']
    assert forecast and forecast['version'] == 2
    assert forecast['measured_tick'] <= launch['tick'] <= forecast['launch_until_tick']
    assert 0 <= forecast['launch_until_tick']-forecast['measured_tick'] <= 120
    corridor, plan = launch['ground']['crossing']['plan'], forecast['plan']
    assert corridor['planet'] == plan['planet'] and corridor['revision'] == plan['revision']
    a, b = corridor['anchor']['Vehicle'], plan['anchor']['Vehicle']
    assert a['index'] == b['index'] and a['form'] == b['form'] == 'ship'
    assert math.dist(J.vector(a['position']), J.vector(b['position'])) <= 0.5
    assert abs((a['angle']-b['angle']+math.pi) % math.tau-math.pi) <= 0.1
    reverse = corridor['direction'] != plan['direction']
    for key, other in [('start','destination'),('destination','start')]:
        assert math.dist(J.vector(corridor[key]), J.vector(plan[other if reverse else key])) <= 0.5
    assert abs(corridor['cruise_radius']-plan['cruise_radius']) <= 0.25


def audit_arm(arm, rows, initial, horizon):
    assert 0 <= arm['end_tick']-arm['start_tick'] <= horizon
    assert arm['start_tick'] == initial['tick']
    assert [r['tick'] for r in rows] == list(range(arm['start_tick'], arm['end_tick']+1))
    assert physical(rows[0]['observation']['combat']['recovery']['flight']['pilot']) == physical(initial)
    assert arm['telemetry'] == rows[-1]['telemetry']
    assert arm['stop'] == rows[-1]['stop'] and rows[-1]['actions'] is None
    milestones = dict.fromkeys(['landed', 'exited', 'neutralized', 'claimed', 'boarded'])
    base_claim = initial['planet']['claim']
    launch_ticks, completion_ticks, charges, burns = [], [], [], []
    previous_goal = None
    seat = ['player_1','player_2'].index(initial['owner'])
    for i, r in enumerate(rows):
        tick = r['tick']
        if i < len(rows)-1:
            assert r['stop'] is None and r['actions'] is not None
            assert r['planet'] == initial['planet']['index']
            assert r['ship_available'] and r['ship_form'] == 'ship'
            assert r['match_context'] is None or (r['match_context']['pilots_alive'][seat] and not r['match_context']['finished'])
            assert len(r['actions']) == 3
            assert r['actions'][2] == {'Scenario':dict(kind=0x53550005,payload=[seat,0,0])}
        if 'observation' in r:
            p = r['observation']['combat']['recovery']['flight']['pilot']
            for key in ['tick','location','vehicle','transfers','ship_form','ship_available','ship','actor','landing']:
                assert r[key] == p[key], key
            assert r['planet'] == p['planet']['index']
            assert r['goal'] == r['telemetry']['goal']
            if r['observation']['landing_objective'] is not None:
                survey = r['observation']['landing_objective']
                assert survey['planning'] == arm['planning'] and survey['tick'] == tick
                assert 0 <= len(survey['sites']) <= 8
        flags = dict(
            landed=r['landing']['planet'] == initial['planet']['index'] and r['landing']['phase'] == 'landed',
            exited=r['location'] == 'on_foot',
            neutralized=r['claim']['neutralizations'] > base_claim['neutralizations'],
            claimed=r['claim']['owner'] == initial['owner'] and r['claim']['captures'] > base_claim['captures'])
        for key, present in flags.items():
            if present and milestones[key] is None: milestones[key] = tick
        if milestones['claimed'] is not None and r['transfers'] >= initial['transfers']+2 and r['location'] == {'aboard':initial['vehicle']}:
            if milestones['boarded'] is None: milestones['boarded'] = tick
        assert r['milestones'] == milestones
        if r['ground_goal'] == 'jetpack_lift' and previous_goal != 'jetpack_lift': launch_ticks.append(tick)
        previous_goal = r['ground_goal']
        if r['crossing'] and r['crossing']['completed_tick'] is not None:
            tick = r['crossing']['completed_tick']
            if tick not in completion_ticks: completion_ticks.append(tick)
        if r['charge'] is not None:
            assert math.isfinite(r['charge']) and 0 <= r['charge'] <= 1
            charges.append(r['charge'])
            burns.append(r['burn_seconds'])
    assert arm['milestones'] == milestones
    assert [s['tick'] for s in arm['launches']] == launch_ticks
    assert arm['crossing_completions'] == completion_ticks
    assert arm['lowest_charge'] == min(charges, default=1.0)
    if burns:
        assert all(math.isfinite(v) for v in burns) and burns == sorted(burns)
        assert math.isclose(arm['burn_seconds'], burns[-1]-burns[0], abs_tol=1e-5)
    else: assert arm['burn_seconds'] is None
    for launch in arm['launches']:
        audit_launch(launch)
        witness = rows[launch['tick']-arm['start_tick']]
        assert launch['ground'] == witness['telemetry']['ground']
        assert launch['equipment'] == witness['observation']['combat']['recovery']['jetpack']
    expected_audits = list(range(arm['start_tick'], arm['end_tick']+1, 60))
    if expected_audits[-1] != arm['end_tick']: expected_audits.append(arm['end_tick'])
    assert [a['tick'] for a in arm['audits']] == expected_audits
    conserved = {a['audit']['occupied_cells']+a['audit']['removed_cells'] for a in arm['audits']}
    actual_ok = len(conserved) == 1 and all(not a['audit']['issues'] and a['audit']['max_speed'] < 500 for a in arm['audits'])
    assert arm['physics_ok'] == actual_ok
    stop, last = arm['stop'], rows[-1]
    if stop == 'controller_completed':
        assert arm['telemetry']['completed_tick'] == arm['end_tick'] and arm['telemetry']['failed_tick'] is None
        assert all(milestones[k] is not None for k in ['landed','exited','claimed','boarded'])
        assert milestones['landed'] <= milestones['exited'] < milestones['claimed'] <= milestones['boarded'] < arm['end_tick']
        assert last['landing']['phase'] != 'landed' and last['landing']['supported_feet'] == 0
    elif stop == 'controller_failed': assert arm['telemetry']['failed_tick'] == arm['end_tick']
    elif stop == 'horizon': assert arm['end_tick']-arm['start_tick'] == horizon
    elif stop == 'local_frame_changed': assert last['planet'] != initial['planet']['index']
    elif stop == 'physics_failure': assert not actual_ok
    elif stop == 'match_finished': assert last['match_context']['finished']
    elif stop == 'actor_or_vehicle_lost':
        assert not last['ship_available'] or last['ship_form'] != 'ship' or not last['match_context']['pilots_alive'][seat]
    else: raise AssertionError(stop)
    return dict(name=arm['name'], start_tick=arm['start_tick'], end_tick=arm['end_tick'],
        stop=stop, failure=arm['telemetry']['failure'], milestones=milestones,
        selected_site=arm['telemetry']['site'], launches=len(launch_ticks),
        completed_crossings=len(completion_ticks), lowest_charge=arm['lowest_charge'],
        burn_seconds=arm['burn_seconds'], physics_ok=actual_ok, trace_rows=len(rows))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--study', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'], text=True).strip(), 'freeze implementation and plan first'
    source = args.study/'summary.json'
    previous = json.loads(source.read_text())
    assert previous['complete'] and previous['plan'] == [list(c) for c in T.CASES]
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, plan=previous['plan'], runs={},
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        source_summary_sha256=F.E.digest(source), binary_sha256=F.E.digest(binary), runner_sha256=F.E.digest(Path(__file__)),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [J,T,P,V,C,F,V.A,F.E,F.D]},
        scope='Fresh local controllers in independent physical clones. Paired walking/powered models with native synchronous cadenced sensors, source settings, quiet weapons, idle opponent and 180-second horizon. Not original controller memory, live-budget integration or strength evidence.')
    save = lambda: F.D.write(args.out/'summary.json', result)
    save()
    try:
        for label, _, ticks in T.CASES:
            old = previous['runs'][label]
            command = list(old['command'])
            old_root = Path(command[command.index('--out')+1])
            for filename, digest in old['hashes'].items(): assert F.E.digest(old_root/filename) == digest
            root = args.out/label
            command[0] = str(binary)
            command[command.index('--out')+1] = str(root)
            command += ['--probe-cover-execution','true']
            result['runs'][label] = dict(command=command,source_path=str(old_root))
            save()
            print(label, flush=True)
            with (args.out/(label+'.log')).open('w') as log:
                subprocess.run(command,check=True,stdout=log,stderr=log,timeout=1800)
            before, after = [json.loads((r/'report.json').read_text()) for r in [old_root,root]]
            assert after['physics_ok']
            fields = old['unchanged_report_fields']
            T.report_parity(before,after,fields)
            streams = {}
            for filename in V.EXACT_STREAMS:
                digest = F.E.digest(root/filename)
                assert digest == F.E.digest(old_root/filename), (label,filename)
                streams[filename] = digest
            sensors = C.audit_sensors(old_root,root)
            allocation = F.allocation_audit(root)
            assert allocation == F.allocation_audit(old_root)
            prior, probe = [json.loads((r/'cover-probe.json').read_text()) for r in [old_root,root]]
            assert probe['execution_model'] == 'fresh_local_capture_pair_v1'
            assert probe['requested_world_ticks'] == ticks and not probe['unreached_world_ticks']
            assert [r['world_tick'] for r in probe['rows']] == ticks
            samples = []
            for before, row in zip(prior['rows'],probe['rows']):
                existing = {k:v for k,v in row.items() if not k.startswith('execution')}
                assert C.without_wall_times(before) == C.without_wall_times(existing)
                assert row['execution_unknown'] is None
                execution = row['execution']
                assert execution['horizon_ticks'] == 10800
                assert execution['settings']['cover_response'] == (label != 'failure-cover-off')
                assert execution['settings']['seed'] == 42
                assert execution['settings']['cadence'] == 'FourHz'
                assert not execution['settings']['cover_retry'] and not execution['settings']['bounded_acquisition']
                assert [a['planning'] for a in execution['arms']] == ['joint_round_trip','jetpack_round_trip']
                initial = row['observation']['local']['combat']['recovery']['flight']['pilot']
                arms = []
                for arm in execution['arms']:
                    trace = list(F.rows(root/arm['trace']))
                    arms.append(audit_arm(arm,trace,initial,execution['horizon_ticks']))
                samples.append(dict(world_tick=row['world_tick'], arms=arms))
            result['runs'][label].update(samples=samples, unchanged_stream_sha256=streams, unchanged_report_fields=fields,
                sensor_parity=sensors, allocation=allocation, hashes={p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()})
            save()
        assert F.E.digest(binary) == result['binary_sha256']
        result['complete'] = True
    except BaseException as e:
        result['error'] = repr(e)
        raise
    finally: save()


if __name__ == '__main__': main()
