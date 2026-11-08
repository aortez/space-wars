"""Physical qualification and provenance checks for the frozen LF-055 stage."""
import argparse
from collections import Counter
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.motor_retention import inspect as retention_inspect, seed_gate
from .audit import inspect as training_inspect, read
from .task import SEEDS
from .velocity import candidates, config, reward


def task_matches(actual, expected):
    if isinstance(expected, dict):
        return actual.keys() == expected.keys() and all(task_matches(actual[k],v) for k,v in expected.items())
    if isinstance(expected, float):
        return math.isclose(actual,expected,rel_tol=1e-6,abs_tol=1e-6)
    return actual == expected


def inspect(trace):
    result = retention_inspect(trace)
    tail = trace['steps'][-120:]
    result['tail_velocity_rms'] = math.sqrt(sum(s['item']['errors']['velocity']**2 for s in tail)/len(tail))
    result['mean_actions'] = {k: sum(s['action'][k] for s in trace['steps'])/len(trace['steps']) for k in ['steering','thrust','brake']}
    result['tail_actions'] = {k: sum(s['action'][k] for s in tail)/len(tail) for k in ['steering','thrust','brake']}
    result['maximum_velocity_error'] = max(s['item']['errors']['velocity'] for s in trace['steps'])
    return result


def gate(initial, final):
    brake = [r for r in final if r['family'] == 'brake']; track = [r for r in final if r['family'] == 'track']
    assert len(brake) == 24 and len(track) == 48
    def counts(rows, key):
        return {str(v): sum(r['retained'] for r in rows if r[key] == v) for v in sorted({r[key] for r in rows})}
    braking = sum(r['retained'] for r in brake); tracking = sum(r['retained'] for r in track)
    old = {r['index']: r['retained'] for r in initial}
    regression = [{**r, 'parent_success': old[r['index']]} for r in final if r['family'] in ['turn','hold']]
    heading = seed_gate(regression)
    gain = tracking - sum(r['retained'] for r in initial if r['family'] == 'track')
    safety = sum(r['outcome'] not in ['success','deadline'] for r in final)
    checks = {'braking': braking >= 20 and min(counts(brake,'level').values()) >= 6 and min(counts(brake,'direction').values()) >= 10,
              'tracking': tracking >= 40 and min(counts(track,'level').values()) >= 12 and min(counts(track,'mode').values()) >= 4,
              'tracking_gain': gain >= 24, 'heading_retention': heading['passed'], 'physical_safety': safety == 0}
    return {'passed': all(checks.values()), 'checks': checks, 'braking_retained': braking, 'tracking_retained': tracking,
            'tracking_gain': gain, 'braking_by_speed': counts(brake,'level'), 'braking_by_turn': counts(brake,'direction'),
            'tracking_by_speed': counts(track,'level'), 'tracking_by_mode': counts(track,'mode'),
            'heading': heading, 'physical_failures': safety}


def verify(a):
    start = time.monotonic(); records = []; restore_ticks = optimizer_steps = evaluation_ticks = setups = 0
    for seed in SEEDS:
        run = a/f'pilot/seed-{seed}'; resumed = a/f'resumed/seed-{seed}'
        summary = read(run/'summary.json'); c = config(seed)
        assert summary['completed_updates'] == 128 and summary['environment_steps'] == 131072
        assert summary['parameter_count'] == 3463 and summary['total_seconds'] < 900
        final = torch.load(run/'checkpoint-0128.pt', weights_only=True)
        again = torch.load(resumed/'checkpoint-0128.pt', weights_only=True)
        assert equal_tree(final, again)
        assert equal_tree(torch.load(run/'rollout-0128.pt',weights_only=True), torch.load(resumed/'rollout-0128.pt',weights_only=True))
        zero = torch.load(run/'checkpoint-0000.pt', weights_only=True)
        parent = torch.load(a/f'inputs/seed-{seed}/checkpoint-0064.pt', weights_only=True)
        assert equal_tree(zero['model'],parent['model']) and not zero['optimizer']['state'] and zero['update'] == 0 and zero['history'] == []
        assert not equal_tree(final['model'],zero['model']) and zero['collector']['steps'] == 0
        assert read(run/'policy-0000.json') == read(a/f'inputs/seed-{seed}/policy-0064.json')
        assert final['contract']['config'] == c
        assert read(run/'training-candidates.json') == candidates(c['train_task_seed']) == read(a/'tasks/training.json')
        assert read(run/'development-candidates.json') == candidates(c['development_task_seed'],True) == read(a/'tasks/development.json')
        previous = torch.load(run/'checkpoint-0127.pt',weights_only=True)
        restore_ticks += sum(len(t['steps']) for t in previous['collector']['traces'])
        optimizer_steps += sum(h['minibatches'] for h in summary['history']) + summary['history'][-1]['minibatches']
        state = final['collector']; coverage = Counter(e['candidate']['index'] for e in state['completed'])
        assert set(coverage) == set(range(104)) and min(coverage.values()) >= 3
        total = 0; outcomes = Counter()
        for e in state['completed']:
            r = training_inspect(read(run/f"training-traces/episode-{e['id']:06}.json"),reward)
            assert r['ticks'] == e['ticks'] and r['outcome'] == e['outcome'] and abs(r['reward']-e['reward']) < 1e-9
            total += r['ticks']; outcomes[e['outcome']] += 1
        total += sum(training_inspect(t,reward)['ticks'] for t in state['traces']); assert total == 131072
        setups += state['started'] + 4 + state['started'] - previous['collector']['started']
        phases = {}
        for phase in ['initial','final']:
            out = run/f'evaluation-{phase}'; rows = read(out/'results.json'); assert len(rows) == 104
            assert len(read(out/'parity.json')) == 104
            phases[phase] = []
            for row, candidate in zip(rows, candidates(c['development_task_seed'],True)):
                assert row['index'] == candidate['index']
                trace = read(out/f"case-{row['index']:02}.json"); assert task_matches(trace['task'],candidate['task'])
                r = inspect(trace)
                assert all(row[k] == r[k] for k in ['ticks','outcome','final_errors'])
                if phase == 'initial' and 'parent_index' in candidate:
                    assert trace == read(a/f"inputs/seed-{seed}/reference/case-{candidate['parent_index']:02}.json")
                phases[phase].append({**candidate,**r}); evaluation_ticks += r['ticks']; setups += 1
        records.append({'seed':seed, 'gate':gate(phases['initial'],phases['final']), 'trajectories':phases,
                        'training_coverage':dict(sorted(coverage.items())), 'training_outcomes':dict(outcomes),
                        'optimizer_steps':sum(h['minibatches'] for h in summary['history']),
                        'kl_stops':sum(h['kl_early_stop'] for h in summary['history']), 'main_seconds':summary['total_seconds']})
    result = {'experiment_id':a.name, 'passed':sum(r['gate']['passed'] for r in records) >= 2, 'records':records,
              'training_ticks':396288,'main_training_ticks':393216,'restart_training_ticks':3072,
              'restore_ticks':restore_ticks,'optimizer_steps':optimizer_steps,'evaluation_ticks':evaluation_ticks,
              'evaluation_episodes':624,'neutral_setup_ticks':setups,'native_python_comparisons':624,
              'exact_resumes':3,'exact_parent_weights':3,'exact_parent_retention_traces':96,
              'confirmation_used':False,'model_promoted':False,'navigation_qualified':False,'seconds':time.monotonic()-start}
    assert restore_ticks <= 4308 and optimizer_steps <= 6192 and evaluation_ticks <= 561600 and result['seconds'] < 300
    atomic_json(a/'results.json',result)


if __name__ == '__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);args=p.parse_args()
    torch.set_num_threads(1);verify(args.archive)
