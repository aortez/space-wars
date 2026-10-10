#!/usr/bin/env python3
"""Independently check frozen hatch-arrival pairs and report all outcomes."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess
import tarfile


def read(path):
    return json.loads(path.read_text())


def rows(path):
    with path.open() as f:
        for line in f:
            yield json.loads(line)


def sha(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda:f.read(1024*1024), b''):
            h.update(block)
    return h.hexdigest()


def controls(action):
    value = action['Scenario']
    assert value['kind'] == 1398079490
    p = value['payload']
    assert len(p) == 8
    return struct.unpack('<f',bytes(p[:4]))[0],p[4],p[5],p[6],p[7]


def boarding(event, seat):
    before, after = event['before'], event['after']
    assert before['controls_armed'] and before['transfer'] == 'ready'
    assert before['location'] == 'on_foot' and before['balanced']
    assert before['supported_planet'] == before['planet']['index']
    assert before['ship_available'] and before['ship_form'] == 'ship'
    assert before['landing']['phase'] == 'landed'
    action = controls(event['action'])
    assert action[2] == 1 and action[4] == seat
    actor, planet = before['actor'], before['planet']['motion']
    distances = [math.hypot(actor['position']['x']-h['x'],actor['position']['y']-h['y']) for h in before['boarding_hatches'] if h]
    offset = (actor['position']['x']-planet['position']['x'],actor['position']['y']-planet['position']['y'])
    relative = (actor['velocity']['x']-planet['velocity']['x']+offset[1]*planet['spin'],
                actor['velocity']['y']-planet['velocity']['y']-offset[0]*planet['spin'])
    speed = math.hypot(*relative)
    assert min(distances) <= 3.0001 and speed <= 2.0001
    assert after['tick'] == before['tick']+1
    assert after['location'] == {'aboard':after['vehicle']}
    assert after['last_transfer'] == 'boarded' and after['transfers'] == before['transfers']+1
    return {'tick':after['tick'],'revision':before['planet']['revision'],
            'hatch_distance':min(distances),'relative_speed':speed}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root',type=Path)
    parser.add_argument('--output',type=Path)
    parser.add_argument('--completed-only',action='store_true',help='audit finished cases while the matrix is still running')
    args = parser.parse_args()
    root = args.root.resolve()
    repo = Path(__file__).resolve().parent.parent
    runtime = read(root/'runtime.json')
    assert sha(root/'hatch-arrival-comparison') == runtime['binary_sha256']
    assert sha(root/'source-inputs.json') == runtime['source_inputs_sha256']
    inputs = read(root/'source-inputs.json')
    checked = set()
    process = subprocess.Popen(['git','archive',runtime['source_commit']],cwd=repo,stdout=subprocess.PIPE)
    with tarfile.open(fileobj=process.stdout,mode='r|') as archive:
        for item in archive:
            if item.name in inputs:
                data = item.linkname.encode() if item.issym() else archive.extractfile(item).read()
                assert hashlib.sha256(data).hexdigest() == inputs[item.name]
                checked.add(item.name)
    assert process.wait() == 0 and checked == set(inputs)
    for name,digest in read(root/'references/provenance.json')['files'].items():
        assert sha(root/'references'/name) == digest
    matrix = read(root/'matrix.json')
    assert len(matrix) == 17 and matrix[0]['fork'] == 24743
    assert {(c['case']['seed'],c['case']['shape'],c['case']['slumping'],c['case']['candidate_seat']) for c in matrix[1:]} == {
        (seed,shape,collapse,seat) for seed in (10101,20202) for shape in ('round','angular') for collapse in (False,True) for seat in (0,1)}
    cases = []
    pending = []
    artifacts = samples = decisions = 0
    for item in matrix:
        here = root/'cases'/item['name']
        if args.completed_only and not (here/'manifest.json').exists():
            pending.append(item['name'])
            continue
        result = read(here/'result.json')
        assert result['exit_code'] == 0 and result['binary_sha256'] == runtime['binary_sha256']
        assert result['source_commit'] == runtime['source_commit']
        for name,digest in read(here/'manifest.json').items():
            assert sha(here/name) == digest
            artifacts += 1
        out = here/'output'
        pair = read(out/'summary.json')
        assert pair['case'] == item['case'] and pair['fork'] == item['fork']
        assert pair['control_final_matches_reference'] and pair['prefix_decisions_verified'] == item['fork']
        assert not any(pair[k] for k in ('controller_reset','input_override','physics_edited'))
        reference = root/'references'/item['reference']
        recorded = list(rows(reference/'actions.jsonl'))
        assert read(out/'control/final.json') == read(reference/'final.json')
        assert read(out/'control/initial.json') == read(out/'candidate/initial.json') == read(out/'fork.json')
        original = (out/'control/initial-controllers.txt').read_text()
        candidate = (out/'candidate/initial-controllers.txt').read_text()
        assert 'hatch_supported_arrival: true' in candidate
        assert candidate.replace('hatch_supported_arrival: true','hatch_supported_arrival: false') == original
        seat = item['case']['candidate_seat']
        compared = {}
        for name in ('control','candidate'):
            arm = out/name
            s = read(arm/'summary.json')
            assert s['case'] == item['case'] and s['start_tick'] == item['fork']
            assert s['hatch_supported_arrival'] == (name == 'candidate')
            assert s['end_tick'] <= 36000 and s['match']['outcome'] is not None
            assert not any(s[k] for k in ('controller_reset','input_override','physics_edited'))
            assert s['only_selected_option_changed'] and s['opponent_reactive']
            actions = list(rows(arm/'actions.jsonl'))
            assert [r['tick'] for r in actions] == list(range(s['start_tick'],s['end_tick']))
            first_action = first_state = None
            for r in actions:
                old = recorded[r['tick']] if r['tick'] < len(recorded) else None
                if old is None or old['actions'] != r['actions']:
                    if first_action is None:first_action = r['tick']
                if old is None or old['scenario_before_step_fnv64'] != r['scenario_before_step_fnv64']:
                    if first_state is None:first_state = r['tick']
                if name == 'control':assert r == old
            assert s['first_input_difference'] == first_action
            assert s['first_scenario_difference'] == first_state
            if name == 'control':
                assert s['reference_decisions_verified'] == len(actions)
                decisions += len(actions)+item['fork']
                if item['fork'] == 0:assert s['metrics'] == read(reference/'summary.json')['metrics']
            gates = [boarding(event,seat) for event in s['boardings']]
            active_ticks = airborne = steering = selected_ticks = 0
            recovery_hatch = recovery_airborne = recovery_steering = 0
            for row in rows(arm/'hatch-decisions.jsonl'):
                p = row['pilot']
                assert row['tick'] == p['tick']
                assert row['action'] == actions[row['tick']-s['start_tick']]['actions'][seat*3]
                assert p['location'] == 'on_foot' and p['ship_available'] and p['ship_form'] == 'ship'
                active_ticks += 1
                action = controls(row['action'])
                if not p['controls_armed']:assert action[:4] == (0.0,0,0,0)
                if row['ground'] and row['ground'].get('hatch_supported_arrival',False):
                    assert name == 'candidate'
                    selected_ticks += 1
                actor = p['actor']['position']
                near = any(math.hypot(actor['x']-h['x'],actor['y']-h['y']) < 2.8 for h in p['boarding_hatches'] if h)
                recovering_hatch = row['ground'] is not None and row['ground']['destination'] == 'hatch'
                recovery_hatch += recovering_hatch
                if p['supported_planet'] is None and near:
                    airborne += 1
                    steering += action[0] != 0
                    recovery_airborne += recovering_hatch
                    recovery_steering += recovering_hatch and action[0] != 0
            assert (active_ticks,airborne,steering) == (s['hatch_decisions'],s['airborne_near_hatch'],s['airborne_near_hatch_steering'])
            for row in rows(arm/'samples.jsonl'):
                a = row['state']['audit']
                assert not a['issues'] and a['max_speed'] < 500
                assert a['occupied_cells']+a['loose_cells']+a['removed_cells'] == s['initial_cells']
                samples += 1
            for index,m in enumerate(s['metrics']):
                for tick in m['completed']:
                    complete = read(arm/f'completion-{tick}-p{index+1}.json')
                    p = complete['pilots'][index]['recovery']['flight']['pilot']
                    b = complete['brains'][index]
                    assert p['ship_available'] and p['ship_form'] == 'ship' and p['location'] == {'aboard':p['vehicle']}
                    assert b['recovery'] is None
            compared[name] = {k:s[k] for k in ('end_tick','first_input_difference','first_scenario_difference','hatch_decisions','airborne_near_hatch','airborne_near_hatch_steering','initial_physics','initial_controller_fnv64','initial_completed','match')}
            compared[name].update(boardings=gates,completed=s['metrics'][seat]['completed'],
                                  final_recoveries=[b['completed_recoveries'] for b in s['brains']],
                                  metrics=s['metrics'],selected_ground_ticks=selected_ticks,
                                  recovery_hatch_ticks=recovery_hatch,
                                  recovery_airborne_near_hatch=recovery_airborne,
                                  recovery_airborne_steering=recovery_steering)
        assert compared['control']['initial_physics'] == compared['candidate']['initial_physics']
        assert compared['control']['initial_controller_fnv64'] == compared['candidate']['initial_controller_fnv64']
        cases.append({'name':item['name'],'case':item['case'],'fork':item['fork'],**compared})
    target = next(c for c in cases if c['fork'] == 24743)
    target_completed = target['candidate']['completed']
    report = {'schema':1,'passed':True,'complete':not pending,'pending_cases':pending,'source_inputs_checked':len(checked),
              'artifacts_checked':artifacts,'material_samples':samples,'control_decisions_verified':decisions,
              'target_recovered':bool(target_completed),
              'target_first_completion':target_completed[0] if target_completed else None,
              'matrix_cases':sum(c['fork']==0 for c in cases),'cases':cases}
    if args.output:args.output.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(report,indent=2))


if __name__ == '__main__':
    main()
