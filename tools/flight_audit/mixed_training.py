"""Predeclared repeated-trial gates and report for the 256-update comparison."""
import argparse
from collections import Counter
import gzip
import json
import math
from pathlib import Path

import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.execution import MIXED, episode_seed
from tools.flight_ppo.learning_report import parity
from tools.flight_ppo.tests import equal_tree

SEEDS = [17, 29, 43]
SPEC = {'mode': MIXED, 'repeats': 4, 'seed_base': 5310000}
RECIPES = [f'aligned-approach-v1/stratum-{i}' for i in range(4)]


def read(path):
    opener = gzip.open if path.suffix == '.gz' else open
    with opener(path, 'rt') as stream:
        return json.load(stream)


def physical(rows, seed):
    if len(rows) != 64 or {r['repeat'] for r in rows} != set(range(4)):
        raise ValueError('exactly four repeats of sixteen tasks required')
    identities = None
    repeats = []
    for repeat in range(4):
        group = [r for r in rows if r['repeat'] == repeat]
        identity = {(r['index'], r['recipe']) for r in group}
        if len(group) != 16 or len(identity) != 16 or len({r['index'] for r in group}) != 16:
            raise ValueError('duplicate or incomplete task identities')
        if identities is not None and identity != identities:
            raise ValueError('repeats must contain identical tasks')
        identities = identity
        if any(sum(r['recipe'] == recipe for r in group) != 4 for recipe in RECIPES):
            raise ValueError('four tasks in each declared distance stratum required')
        repeats.append(sum(r['outcome'] == 'success' for r in group))
        for row in group:
            if (row['execution_mode'] != MIXED
                    or row['rng_seed'] != episode_seed(SPEC, seed, repeat, row['index'])):
                raise ValueError('execution/episode seed mismatch')
            if row['outcome'] in [None, 'environment_error', 'budget_censored']:
                raise ValueError('incomplete or infrastructure-failed trial')
            if not 1 <= row['ticks'] <= 1800 or not all(
                    math.isfinite(v) for errors in ['initial_errors', 'final_errors']
                    for v in row[errors].values()):
                raise ValueError('invalid physical trial record')
    failures = sum(r['outcome'] not in ['success', 'deadline'] or r['collision'] or r['damage'] for r in rows)
    return {'attempts': 64, 'outcomes': dict(Counter(r['outcome'] for r in rows)),
            'successes': sum(repeats), 'physical_failures': failures,
            'successes_by_repeat': repeats,
            'successes_by_stratum': [sum(r['recipe'] == recipe and r['outcome'] == 'success' for r in rows)
                                     for recipe in RECIPES]}


def paired(before, after):
    def keyed(rows):
        result = {(r['repeat'], r['index']): r for r in rows}
        if len(result) != len(rows):
            raise ValueError('duplicate paired trial')
        return result
    first, second = keyed(before), keyed(after)
    if first.keys() != second.keys():
        raise ValueError('different paired tasks/repeats')
    counts = Counter()
    for key, old in first.items():
        new = second[key]
        if any(old[k] != new[k] for k in ['recipe', 'rng_seed', 'initial_errors', 'execution_mode']):
            raise ValueError('incompatible paired trial')
        a, b = old['outcome'] == 'success', new['outcome'] == 'success'
        counts['retained' if a and b else 'lost' if a else 'gained' if b else 'neither'] += 1
    return {k: counts[k] for k in ['retained', 'lost', 'gained', 'neither']}


def gates(initial, baseline, final, seed):
    before, mid, after = [physical(rows, seed) for rows in [initial, baseline, final]]
    initial_pair, baseline_pair = paired(initial, final), paired(baseline, final)
    gain = after['successes']-before['successes']
    return {'initial': before, 'baseline_128': mid, 'final': after,
            'gain_from_initial': gain, 'gain_from_128': after['successes']-mid['successes'],
            'paired_initial_to_final': initial_pair, 'paired_128_to_final': baseline_pair,
            'learning_evidence': after['successes'] >= 16 and gain >= 16
                and sum(n >= 4 for n in after['successes_by_repeat']) >= 3,
            'scoped_progression': after['successes'] >= 48 and gain >= 32
                and after['physical_failures'] == 0
                and min(after['successes_by_stratum']) >= 8
                and sum(n >= 12 for n in after['successes_by_repeat']) >= 3}


def aggregate(records):
    if sorted(r['seed'] for r in records) != SEEDS:
        raise ValueError('three declared learner seeds required')
    completed = all(r['status'] == 'completed' for r in records)
    return {'all_runs_completed': completed,
            'learning_evidence': completed and sum(r['learning_evidence'] for r in records) >= 2,
            'scoped_progression': completed and sum(r['scoped_progression'] for r in records) >= 2,
            'duration_benefit': completed and sum(r['gain_from_128'] >= 8 for r in records) >= 2
                and all(r['gain_from_128'] >= -8 for r in records)}


def training_equal(current, original):
    # Provenance must differ for the new declared budget/evaluation. This is a
    # comparison of fresh runs, never a bypass of the checkpoint resume guard.
    if current.keys() != original.keys():
        return False
    return equal_tree({k: v for k, v in current.items() if k != 'contract'},
                      {k: v for k, v in original.items() if k != 'contract'})


def trace_metrics(trace):
    steps = trace['steps']
    if not steps:
        raise ValueError('empty completed trace')
    def tolerated(step):
        e = step['frame']['errors']
        return e['position'] <= 3 and e['velocity'] <= 1 and abs(e['spin']) <= .2
    outcome = steps[-1]['progress']['outcome']
    if outcome == 'success':
        if len(steps) < 30 or not all(tolerated(s) for s in steps[-30:]):
            raise ValueError('success fails independent thirty-frame tolerance check')
    near = [s for s in steps if s['frame']['errors']['position'] <= 3]
    return {'outcome': outcome, 'ticks': len(steps), 'entered_goal_radius': bool(near),
            'met_all_tolerances': any(tolerated(s) for s in steps),
            'maximum_dwell_ticks': max(s['progress']['dwell_ticks'] for s in steps),
            'minimum_distance': min(s['frame']['errors']['position'] for s in steps),
            'final_errors': steps[-1]['frame']['errors'],
            'thrust_ticks': sum(s['action']['thrust'] for s in steps),
            'brake_ticks': sum(s['action']['brake'] for s in steps),
            'near_ticks': len(near),
            'near_speed_compliant_ticks': sum(s['frame']['errors']['velocity'] <= 1 for s in near),
            'near_spin_compliant_ticks': sum(abs(s['frame']['errors']['spin']) <= .2 for s in near)}


def combine(traces):
    ticks = sum(t['ticks'] for t in traces)
    near = sum(t['near_ticks'] for t in traces)
    return {'attempts': len(traces), 'ticks': ticks,
            'entered_goal_radius': sum(t['entered_goal_radius'] for t in traces),
            'met_all_tolerances': sum(t['met_all_tolerances'] for t in traces),
            'maximum_dwell_ticks': max(t['maximum_dwell_ticks'] for t in traces),
            'thrust_fraction': sum(t['thrust_ticks'] for t in traces)/ticks,
            'brake_fraction': sum(t['brake_ticks'] for t in traces)/ticks,
            'near_speed_compliance': sum(t['near_speed_compliant_ticks'] for t in traces)/near if near else None,
            'near_spin_compliance': sum(t['near_spin_compliant_ticks'] for t in traces)/near if near else None,
            'mean_final_distance': sum(t['final_errors']['position'] for t in traces)/len(traces)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    records = []
    with Bridge(args.archive/'runtime/flight_ppo_bridge') as bridge:
        for seed in SEEDS:
            run, inputs = args.archive/f'seed-{seed}', args.archive/f'inputs/seed-{seed}'
            if not (run/'summary.json').exists():
                records.append({'seed': seed, 'status': 'incomplete_or_failed'})
                continue
            summary = read(run/'summary.json')
            assert summary['completed_updates'] == 256 and summary['environment_steps'] == 262144
            assert sum(e['ticks'] for e in summary['completed_episodes']+summary['unfinished_episodes']) == 262144
            assert summary['evaluation'] == SPEC
            for update in [0, 128]:
                current = torch.load(run/f'checkpoint-{update:04}.pt', weights_only=True)
                original = torch.load(inputs/f'checkpoint-{update:04}.pt', weights_only=True)
                assert training_equal(current, original), f'seed {seed} update {update} differs from parent'
            for name in ['training-admission.json', 'development-admission.json']:
                assert read(run/name) == read(inputs/name)
            baseline = read(inputs/'baseline-results.json')
            initial, final = summary['initial_evaluation'], summary['final_evaluation']
            phase_traces = {}
            for phase, rows in [('initial', initial), ('final', final)]:
                metrics = []
                for row in rows:
                    directory = run/f'evaluation-{phase}'/f"repeat-{row['repeat']}"
                    name = f"clear-{row['index']}"
                    trace = read(directory/f'{name}.json')
                    actions = read(directory/'policy'/f'{name}.json.gz')
                    assert len(trace['steps']) == len(actions) == row['ticks']
                    assert all(s['action'] == a['action'] for s, a in zip(trace['steps'], actions))
                    metric = trace_metrics(trace)
                    assert metric['outcome'] == row['outcome'] and metric['final_errors'] == row['final_errors']
                    metrics.append({**metric, 'index': row['index'], 'repeat': row['repeat'], 'recipe': row['recipe']})
                phase_traces[phase] = {'summary': combine(metrics), 'episodes': metrics}
            state = torch.load(run/'checkpoint-0256.pt', weights_only=True)
            training = [trace_metrics(read(p)) for p in sorted((run/'training-traces').glob('*.json'))]
            training += [trace_metrics(t) for t in state['collector']['traces'] if t['steps']]
            assert sum(t['ticks'] for t in training) == 262144
            assert all(math.isfinite(v) for h in summary['history'] for v in h.values() if isinstance(v, float))
            records.append({'seed': seed, 'status': 'completed', **gates(initial, baseline, final, seed),
                'matched_training_state_updates': [0, 128], 'training_ticks': 262144,
                'evaluation_ticks': sum(row['ticks'] for row in initial+final),
                'training_seconds': summary['training_seconds_including_checkpoints'],
                'total_seconds': summary['total_seconds'], 'evaluation_trace_inspection': phase_traces,
                'training_diagnostics': combine(training),
                'training_outcomes': dict(Counter(e['outcome'] for e in summary['completed_episodes'])),
                'censored': len(summary['unfinished_episodes']), 'parity': parity(bridge, run, state),
                'kl_early_stopped_updates': sum(h['kl_early_stop'] for h in summary['history']),
                'optimizer_summary': {k: {'first': summary['history'][0][k], 'last': summary['history'][-1][k],
                    'mean': sum(h[k] for h in summary['history'])/256,
                    'maximum': max(h[k] for h in summary['history'])}
                    for k in ['value_loss', 'policy_loss', 'entropy', 'gradient_norm', 'clip_fraction', 'kl']}})
    atomic_json(args.out, {'experiment_id': args.archive.name, 'seeds': records, 'gates': aggregate(records),
        'total_training_ticks': sum(r.get('training_ticks', 0) for r in records),
        'total_evaluation_ticks': sum(r.get('evaluation_ticks', 0) for r in records),
        'reused_baseline_ticks': 177917, 'confirmation_status': 'unused by design; separate decision after review',
        'general_flight_claim': False, 'obstacle_navigation_claim': False})


if __name__ == '__main__':
    main()
