#!/usr/bin/env python3
"""Audit the retained Round input matrix and summarize route-entry witnesses."""
import argparse
from collections import Counter
import hashlib
import json
import math
from pathlib import Path
import runpy
import struct
import subprocess
import tarfile


def read(p):
    return json.loads(p.read_text())


def rows(p):
    with p.open() as f:
        for line in f:
            yield json.loads(line)


def sha(p):
    h = hashlib.sha256()
    with p.open('rb') as f:
        for b in iter(lambda:f.read(1024*1024), b''):
            h.update(b)
    return h.hexdigest()


def control(a):
    v = a['Scenario']
    assert v['kind'] == 1398079490 and len(v['payload']) == 8
    b = v['payload']
    return struct.unpack('<f',bytes(b[:4]))[0],b[4],b[5],b[6],b[7]


def summarize(data, initial_nodes):
    nodes = dict(initial_nodes)
    result = {'ticks':0,'jump_presses':0,'native_jumps':0,'native_getups':0,
              'supported_entry':None,'supported_destination':None,'native_ready':None,
              'first_waypoint_advance':None,'minimum_start_distance':None,
              'minimum_hatch_distance':None,'transfer_counts':Counter(),'ground_goals':Counter(),
              'initial_revision':None,'first_revision_change':None,'native_ready_revision':None,
              'supported_entry_revision':None}
    prev_primary = False
    first_jumps, last_jumps, first_getups, last_getups = None,None,None,None
    for row in data:
        result['ticks'] += 1
        if result['initial_revision'] is None:result['initial_revision'] = row['revision']
        if row['revision'] != result['initial_revision'] and result['first_revision_change'] is None:
            result['first_revision_change'] = row['tick']
        action = control(row['actions'][3])
        result['jump_presses'] += bool(action[1] and not prev_primary)
        prev_primary = action[1]
        if row['native_jumps'] is not None:
            if first_jumps is None:first_jumps = row['native_jumps']
            last_jumps = row['native_jumps']
        if row['native_getups'] is not None:
            if first_getups is None:first_getups = row['native_getups']
            last_getups = row['native_getups']
        for n in row['ground_nodes'] or []:nodes[n['id']] = n['position']
        if row['foot']:
            for node,key in ((252,'supported_entry'),(253,'supported_destination')):
                a,b = row['foot'],nodes[node]
                distance = math.hypot(a['x']-b['x'],a['y']-b['y'])
                if node == 252:
                    result['minimum_start_distance'] = min(result['minimum_start_distance'] or math.inf,distance)
                if distance < 0.85 and row['balanced'] and row['supported_planet'] == row['planet']:
                    if result[key] is None:
                        result[key] = row['tick']
                        if node == 252:result['supported_entry_revision'] = row['revision']
        if row['actor']:
            a = row['actor']['position']
            for b in filter(None,row['hatches']):
                distance = math.hypot(a['x']-b['x'],a['y']-b['y'])
                result['minimum_hatch_distance'] = min(result['minimum_hatch_distance'] or math.inf,distance)
        result['transfer_counts'][row['transfer']] += 1
        if row['transfer'] == 'ready' and result['native_ready'] is None:
            result['native_ready'] = row['tick']
            result['native_ready_revision'] = row['revision']
        ground = row['ground']
        if ground:
            result['ground_goals'][ground['goal']] += 1
            if ground['waypoint'] > 0 and result['first_waypoint_advance'] is None:result['first_waypoint_advance'] = row['tick']
    result['native_jumps'] = (last_jumps-first_jumps) if first_jumps is not None else 0
    result['native_getups'] = (last_getups-first_getups) if first_getups is not None else 0
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('root',type=Path)
    p.add_argument('--output',type=Path)
    args = p.parse_args()
    root = args.root.resolve()
    repo = Path(__file__).resolve().parent.parent
    runtime = read(root/'runtime.json')
    assert sha(root/'round-entry-diagnostic') == runtime['binary_sha256']
    inputs = read(root/'source-inputs.json')
    assert sha(root/'source-inputs.json') == runtime['source_inputs_sha256']
    process = subprocess.Popen(['git','archive',runtime['source_commit']],cwd=repo,stdout=subprocess.PIPE)
    checked = set()
    with tarfile.open(fileobj=process.stdout,mode='r|') as archive:
        for item in archive:
            if item.name in inputs:
                data = item.linkname.encode() if item.issym() else archive.extractfile(item).read()
                assert hashlib.sha256(data).hexdigest() == inputs[item.name]
                checked.add(item.name)
    assert process.wait() == 0 and checked == set(inputs)
    for name,digest in read(root/'references/provenance.json')['files'].items():assert sha(root/'references'/name) == digest
    result = read(root/'result.json')
    assert result['exit_code'] == 0 and result['binary_sha256'] == runtime['binary_sha256']
    assert result['reference_sha256'] == sha(root/'references/actions.jsonl')
    out = root/'output'
    summary = read(out/'summary.json')
    assert summary['forks'] == [24743,26229] and summary['trial_count'] == 60
    assert summary['control_decisions_verified'] == 26829 and summary['horizon_ticks'] == 600
    assert not any(summary[k] for k in ('physics_edited','controller_reset','new_production_policy'))
    for r in summary['samples']:
        a = r['audit'];assert not a['issues'] and a['max_speed'] < 500
        assert a['occupied_cells']+a['loose_cells']+a['removed_cells'] == summary['initial_cells']
    reference = list(rows(root/'references/actions.jsonl'))
    baseline = list(rows(out/'control-decisions.jsonl'))
    assert [r['tick'] for r in baseline] == list(range(24743,26829))
    for r in baseline:assert r['actions'] == r['planned_actions'] == reference[r['tick']]['actions']
    geometry_helper = runpy.run_path(str(repo/'tools/audit-terrain-recovery-pocket-connectivity.py'))
    cases = []
    for fork in (24743,26229):
        initial = read(out/f'fork-{fork}.json')['pilots'][1]['recovery']['flight']['pilot']
        inspection = read(out/f'inspection-{fork}.json')
        assert inspection == read(root/f'references/inspection-{fork}.json')
        nodes = {n['id']:n['position'] for n in inspection['routes']['map']['nodes'] if n['id'] in (252,253)}
        diagnostic = read(out/f'connectivity-{fork}.json')
        assert diagnostic['physics_unchanged'] and not diagnostic['controller_input']
        assert diagnostic['physics_fingerprint'] == inspection['routes']['physics_fingerprint']
        geometry = [geometry_helper['audit_grid'](g,diagnostic) for g in diagnostic['grids']]
        trials = []
        for index,horizontal in enumerate((-1,-0.5,0,0.5,1)):
            for delay in (0,12,24):
                for steer_ticks in (12,30):
                    name = f'h{index}-delay{delay}-air{steer_ticks}'
                    here = out/f'trials-{fork}'/name
                    s = read(here/'summary.json')
                    assert s['recipe'] == {'horizontal':horizontal,'delay':delay,'steer_ticks':steer_ticks}
                    assert s['start_tick'] == fork and fork < s['end_tick'] <= fork+600
                    assert s['initial_physics'] == diagnostic['physics_fingerprint']
                    assert not s['controller_reset'] and not s['world_edited']
                    assert s['host_queues_cloned'] and s['opponent_reactive'] and s['only_p2_surface_input_overridden']
                    data = list(rows(here/'decisions.jsonl'))
                    assert [r['tick'] for r in data] == list(range(fork,s['end_tick']))
                    for key in ('actor','actor_up','location','ship_available','ship_form','landing','supported_planet','balanced','transfer','last_transfer'):
                        assert data[0][key] == initial[key]
                    presses = []
                    boarding_gate = None
                    previous_interact = False
                    for row in data:
                        assert row['actions'][:3] == row['planned_actions'][:3]
                        assert row['actions'][4:] == row['planned_actions'][4:]
                        action = control(row['actions'][3]);assert action[-1] == 1
                        if row['phase'] == 'original_controller':assert row['actions'] == row['planned_actions']
                        elif row['phase'] == 'native_boarding_ready':
                            assert row['transfer'] == 'ready' and action[0] == action[1] == action[3] == 0
                            assert row['supported_planet'] == row['planet'] and row['balanced']
                            distances = [math.hypot(row['actor']['position']['x']-h['x'],row['actor']['position']['y']-h['y']) for h in row['hatches'] if h]
                            speed = math.hypot(row['relative_velocity']['x'],row['relative_velocity']['y'])
                            assert min(distances) <= 3.0 and speed <= 2.0
                            assert row['landing']['phase'] == 'landed' and row['ship_available']
                            assert row['support'] is not None
                            assert bool(action[2]) != previous_interact
                            boarding_gate = {'tick':row['tick'],'hatch_distance':min(distances),
                                             'relative_speed':speed,'revision':row['revision'],
                                             'support_collider':row['support']['collider'],
                                             'supported_planet':row['supported_planet'],
                                             'balanced':row['balanced'],'landing_phase':row['landing']['phase']}
                        else:
                            assert row['phase'] == 'scripted_single_jump'
                            assert action[0] == horizontal and action[2] == action[3] == 0
                            if action[1]:
                                assert row['tick'] > fork+delay and row['balanced'] and not row['needs_get_up']
                                assert row['supported_planet'] == row['planet']
                                presses.append(row['tick'])
                        previous_interact = bool(action[2])
                    assert presses == ([] if s['launch_requested'] is None else [s['launch_requested']])
                    final = read(here/'final.json')
                    pilot = final['pilots'][1]['recovery']['flight']['pilot']
                    for key in ('actor','actor_up','planet','location','ship','ship_available','ship_form','landing','supported_planet','balanced','transfer','last_transfer','recovery'):
                        assert pilot[key] == s['final_pilot'][key]
                    if s['boarded_tick'] is not None:
                        assert s['boarded_tick'] == s['end_tick']
                        assert s['final_pilot']['location'] == {'aboard':1} and s['final_pilot']['ship_available']
                        assert s['final_pilot']['last_transfer'] == 'boarded'
                        assert s['final_pilot']['transfers'] == initial['transfers']+1
                        assert data[-1]['transfer'] == 'ready' and control(data[-1]['actions'][3])[2] == 1
                    else:assert s['end_tick'] == fork+600 or final['match']['finished_tick'] is not None
                    for sample in s['samples']:
                        a = sample['audit'];assert not a['issues'] and a['max_speed'] < 500
                        assert a['occupied_cells']+a['loose_cells']+a['removed_cells'] == s['initial_cells']
                    jump = s['launch_requested']
                    before = next((r for r in data if r['tick'] == jump),None)
                    after = next((r for r in data if jump is not None and r['tick'] == jump+1),None)
                    accepted = before is not None and after is not None and before['native_jumps'] is not None and after['native_jumps'] == before['native_jumps']+1
                    neutral_missing = not initial['controls_armed'] and horizontal != 0
                    if neutral_missing:
                        scripted = [r for r in data if r['phase'] == 'scripted_single_jump']
                        assert scripted and all(control(r['actions'][3])[0] != 0 for r in scripted)
                        assert all(r['native_jumps'] == data[0]['native_jumps'] for r in scripted)
                        assert not accepted
                    trials.append({'name':name,'recipe':s['recipe'],'boarded_tick':s['boarded_tick'],
                                   'launch_requested':jump,'script_jump_accepted':accepted,
                                   'neutral_rearm_missing':neutral_missing,'boarding_gate':boarding_gate,
                                   **summarize(data,nodes)})
        assert len({read(out/f'trials-{fork}'/t['name']/'summary.json')['initial_controllers'] for t in trials}) == 1
        cases.append({'fork':fork,'initial_controls_armed':initial['controls_armed'],
                      'geometry':geometry,'control':summarize([r for r in baseline if fork <= r['tick'] < fork+600],nodes),'trials':trials})
    manifest = read(root/'case-manifest.json')
    for name,digest in manifest.items():assert sha(root/name) == digest
    report = {'schema':1,'passed':True,'source_inputs_checked':len(checked),'artifacts_checked':len(manifest),
              'control_decisions_verified':26829,'cases':cases,
              'trial_ticks':sum(t['ticks'] for c in cases for t in c['trials']),
              'scripted_native_jumps':sum(t['script_jump_accepted'] for c in cases for t in c['trials']),
              'neutral_rearm_missing':sum(t['neutral_rearm_missing'] for c in cases for t in c['trials']),
              'native_boardings':sum(t['boarded_tick'] is not None for c in cases for t in c['trials'])}
    if args.output:args.output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))


if __name__ == '__main__':main()
