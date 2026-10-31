"""Frozen goal-only interventions and retained position trajectories; no new trials."""
import argparse
import copy
import gzip
import json
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.goal_response import f32, read, stats, validate_schema
from tools.flight_audit.input_scaling import models, infer
from tools.flight_audit.settle_recover import inspect_trace

SEEDS = [17, 29, 43]
UPDATES = [0, 16, 32]
INDICES = [1, 5, 9, 13]
DIRECTIONS = ['forward', 'left', 'backward', 'right']
DISTANCES = [1.5, 3., 6., 8., 12., 24.]
CONTROLS = ['steering', 'thrust', 'brake']


def goal_grid():
    points = [{'direction': 'origin', 'distance': 0., 'goal': [0., 0.]}]
    for distance in DISTANCES:
        for name, x, y in [('forward', 0, 1), ('left', -1, 0), ('backward', 0, -1), ('right', 1, 0)]:
            points.append({'direction': name, 'distance': distance, 'goal': [distance*x, distance*y]})
    return points


def move_goal(inputs, goal):
    if len(goal) != 2 or not all(math.isfinite(v) for v in goal):
        raise ValueError('goal must have two finite coordinates')
    out = copy.deepcopy(inputs)
    out['self_goal'][:2] = [f32(v / 300.) for v in goal]
    return out


def local_goal(frame):
    dx = frame['target_position']['x'] - frame['center_of_mass']['x']
    dy = frame['target_position']['y'] - frame['center_of_mass']['y']
    c, s = math.cos(frame['angle']), math.sin(frame['angle'])
    return [c*dx + s*dy, -s*dx + c*dy]


def wrapped(angle):
    return math.atan2(math.sin(angle), math.cos(angle))


def trajectory(trace):
    physical = inspect_trace(trace)
    frames = [trace['initial']] + [s['frame'] for s in trace['steps']]
    distances = [f['errors']['position'] for f in frames]
    bearings = [math.atan2(*local_goal(f)) for f in frames]
    first = frames[0]['center_of_mass']; last = frames[-1]['center_of_mass']
    goal = frames[0]['target_position']; delta = [goal[k]-first[k] for k in ['x', 'y']]
    norm = math.hypot(*delta)
    projection = sum((last[k]-first[k])*v for k, v in zip(['x', 'y'], delta)) / norm if norm else 0.
    path = sum(math.hypot(*(b['center_of_mass'][k]-a['center_of_mass'][k] for k in ['x', 'y'])) for a, b in zip(frames, frames[1:]))
    return {'physical': physical, 'initial_distance': distances[0], 'minimum_distance': min(distances),
            'minimum_distance_tick': distances.index(min(distances)), 'final_distance': distances[-1],
            'first_radius_entry_tick': next((i for i, d in enumerate(distances) if d <= 3), None),
            'initial_goal_bearing_radians': bearings[0], 'final_goal_bearing_radians': bearings[-1],
            'minimum_absolute_goal_bearing_radians': min(abs(v) for v in bearings),
            'signed_heading_change_radians': sum(wrapped(b['angle']-a['angle']) for a, b in zip(frames, frames[1:])),
            'net_displacement_toward_initial_goal': projection, 'path_length': path,
            'controls': {k: stats([s['action'][k] for s in trace['steps']]) for k in CONTROLS}}


def response(rows):
    lookup = {(r['case']['direction'], r['case']['distance']): r['python_action'] for r in rows[1:]}
    def difference(a, b):
        return {k: lookup[a][k] - lookup[b][k] for k in CONTROLS}
    right_left = difference(('right', 8.), ('left', 8.))
    return {'right_minus_left_at_8': right_left,
            'forward_minus_backward_at_8': difference(('forward', 8.), ('backward', 8.)),
            'forward_near6_minus_far12': difference(('forward', 6.), ('forward', 12.)),
            'forward8_minus_origin': difference(('forward', 8.), ('origin', 0.)),
            'all_goal_spans': {k: max(r['python_action'][k] for r in rows[1:]) - min(r['python_action'][k] for r in rows[1:]) for k in CONTROLS},
            'correct_right_left_sign': right_left['steering'] > 0,
            'material_right_left_response': abs(right_left['steering']) >= .05,
            'original_action': rows[0]['python_action']}


def grouped(records):
    return {'anchors': len(records),
            'correct_right_left_sign_count': sum(r['summary']['correct_right_left_sign'] for r in records),
            'material_right_left_response_count': sum(r['summary']['material_right_left_response'] for r in records),
            **{name: {k: stats([r['summary'][name][k] for r in records]) for k in CONTROLS}
               for name in ['right_minus_left_at_8', 'forward_minus_backward_at_8', 'forward_near6_minus_far12', 'forward8_minus_origin', 'all_goal_spans']}}


def anchor_tick(trace):
    return min(120, len(trace['steps'])-1)


def run(a):
    started = time.monotonic(); deadline = started + 600
    torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
    global_rng = torch.get_rng_state().clone()
    cases = [{'kind': 'original'}] + [{'kind': 'goal_intervention', **p} for p in goal_grid()]
    assert len(cases) == 26
    all_records = []; paths = []; replay_ticks = 0; setups = 0
    for seed in SEEDS:
        traces = {}
        for update in UPDATES:
            for index, direction in zip(INDICES, DIRECTIONS):
                t = read(a/f'inputs/seed-{seed}/update-{update}/clear-{index}.json')
                assert t['task']['goal_velocity'] == {'x': 0., 'y': 0.}
                traces[update, index] = t
                paths.append({'seed': seed, 'update': update, 'index': index, 'direction': direction, **trajectory(t)})
        for index in INDICES:
            assert all(traces[u, index]['task'] == traces[32, index]['task'] and traces[u, index]['initial'] == traces[32, index]['initial'] for u in UPDATES)
        with Bridge(a/'runtime/flight_ppo_bridge') as bridge, torch.random.fork_rng(devices=[]):
            validate_schema(bridge.contract['schema'])
            anchors = []
            for index in INDICES:
                t = traces[32, index]
                for kind, tick in [('start', 0), ('later', anchor_tick(t))]:
                    assert 0 <= tick < len(t['steps'])
                    if tick:
                        prefix = dict(t, steps=t['steps'][:tick])
                        assert prefix['steps'][-1]['progress']['outcome'] is None
                        item = bridge.call('restore', traces=[prefix])['items'][0]
                        assert item['frame'] == prefix['steps'][-1]['frame']
                        assert item['inputs']['self_goal'][29:32] == [prefix['steps'][-1]['action'][k] for k in CONTROLS]
                    else:
                        item = bridge.call('reset', tasks=[t['task']])['items'][0]
                        assert item['frame'] == t['initial']
                    assert all(abs(v/300 - x) < 1e-7 for v, x in zip(local_goal(item['frame']), item['inputs']['self_goal'][:2]))
                    assert item['frame']['gravity'] == {'x': 0., 'y': 0.}
                    replay_ticks += tick; setups += 1
                    assert replay_ticks <= 1440 and setups <= 24
                    original = item['inputs']; inputs = [original] + [move_goal(original, c['goal']) for c in cases[1:]]
                    out = a/f'anchors/seed-{seed}/{kind}-{index}'; out.mkdir(parents=True)
                    atomic_json(out/'anchor.json', {'kind': kind, 'index': index, 'tick': tick, 'item': item})
                    with gzip.open(out/'inputs.json.gz', 'wt') as f:
                        json.dump(inputs, f, allow_nan=False)
                    anchors.append({'kind': kind, 'index': index, 'tick': tick, 'inputs': inputs})
            for update in UPDATES:
                directory = a/f'inputs/seed-{seed}/update-{update}'
                model, folded, state = models(directory, update, bridge.contract['schema'])
                assert state['update'] == update and state['contract']['config']['value_gradient'] == 'head-only-v1'
                assert state['contract']['config']['input_scaling'] == 'schema-v1'
                model.eval(); folded.eval(); model.requires_grad_(False); folded.requires_grad_(False)
                weights = copy.deepcopy(model.state_dict()); folded_weights = copy.deepcopy(folded.state_dict())
                rng = torch.get_rng_state().clone()
                bridge.call('load_policy', path=str((directory/f'policy-{update:04}.json').resolve()))
                for anchor in anchors:
                    if time.monotonic() > deadline: raise TimeoutError('recovery-response wall budget')
                    inputs = anchor['inputs']; saved = copy.deepcopy(inputs)
                    raw, commands, native, parity = infer(bridge, model, folded, inputs)
                    assert inputs == saved
                    if anchor['kind'] == 'start' or update == 32:
                        expected = traces[update, anchor['index']]['steps'][anchor['tick']]['action']
                        assert max(abs(native[0]['action'][k]-expected[k]) for k in CONTROLS) <= 1e-6
                    rows = [{'case': c, 'python_raw': r, 'python_action': cmd, 'native': n} for c, r, cmd, n in zip(cases, raw, commands, native)]
                    record = {'seed': seed, 'update': update, 'kind': anchor['kind'], 'index': anchor['index'],
                              'anchor_tick': anchor['tick'], 'parity': parity, 'summary': response(rows)}
                    out = a/f'response/seed-{seed}/update-{update}'/f"{anchor['kind']}-{anchor['index']}"; out.mkdir(parents=True)
                    atomic_json(out/'outputs.json', rows); atomic_json(out/'summary.json', record)
                    all_records.append(record)
                    atomic_json(a/'progress.json', {'batches': len(all_records), 'comparisons': len(all_records)*26, 'replay_ticks': replay_ticks})
                assert equal_tree(weights, model.state_dict()) and equal_tree(folded_weights, folded.state_dict()) and torch.equal(rng, torch.get_rng_state())
    assert torch.equal(global_rng, torch.get_rng_state())
    assert len(all_records) == 72 and len(paths) == 36 and setups == 24
    result = {'experiment_id': a.name, 'records': all_records, 'trajectories': paths,
              'groups': {str(seed): {str(u): {kind: grouped([r for r in all_records if r['seed'] == seed and r['update'] == u and r['kind'] == kind]) for kind in ['start', 'later']} for u in UPDATES} for seed in SEEDS},
              'physical_anchor_count': 24, 'inference_batches': 72, 'native_python_comparisons': 1872,
              'replay_action_ticks': replay_ticks, 'neutral_setup_ticks': setups, 'retained_trajectories_inspected': 36,
              'maximum_raw_parity_error': max(r['parity']['raw_error'] for r in all_records),
              'maximum_fold_error': max(r['parity']['fold_error'] for r in all_records),
              'maximum_action_error': max(r['parity']['action_error'] for r in all_records),
              'parameters_and_rng_unchanged': True, 'original_action_checks': 48,
              'seconds': time.monotonic()-started, 'optimizer_steps': 0, 'new_flight_trials': 0,
              'confirmation_used': False, 'model_promoted': False, 'general_navigation_claim': False}
    assert result['seconds'] < 600
    atomic_json(a/'results.json', result)


def main():
    p = argparse.ArgumentParser(); p.add_argument('--archive', type=Path, required=True)
    run(p.parse_args().archive)


if __name__ == '__main__': main()
