"""Frozen 128/256-update by sampled/mixed execution diagnostic."""
import argparse
from collections import Counter
import gzip
import json
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, identity, sha
from tools.flight_ppo.execution import load_export
from .execution import episode_seed, metrics, rollout
from .execution_report import diagnostics, summarize
from .learning_gain import paired_innovations
from .mixed_training import read, trace_metrics
from .steering_report import paired_outcomes

SEEDS = [17, 29, 43]
CELLS = ['sampled_128', 'mixed_128', 'sampled_256', 'mixed_256']


def frozen_export(directory, seed, update, bridge):
    if seed not in SEEDS or update not in [128, 256]:
        raise ValueError('undeclared model')
    state = torch.load(directory/f'checkpoint-{update:04}.pt', weights_only=True)
    contract = state['contract']
    if (state['update'] != update or contract['config']['learner_seed'] != seed
            or contract['config']['task_recipe'] != 'aligned-approach-v1'
            or contract['config']['exploration'] != {'kind': 'gaussian-latent-ar1-v1', 'rho': 0.}
            or contract['schema'] != bridge.contract['schema']
            or contract['bridge_sha256'] != sha(bridge.executable)):
        raise ValueError('model/task/runtime mismatch')
    if update == 256 and contract != identity(contract['config'], bridge):
        raise ValueError('current checkpoint source/runtime identity mismatch')
    # Update 128 predates evaluator integration. Read its export for inference;
    # never relax or call the old checkpoint's training-continuation path.
    with torch.random.fork_rng(devices=[]):
        model = load_export(directory/f'policy-{update:04}.json', bridge.contract['schema'])
    if state['model'].keys() != model.state_dict().keys() or not all(
            torch.equal(value, state['model'][name]) for name, value in model.state_dict().items()):
        raise ValueError('export/checkpoint tensor mismatch')
    return model


def validate_rows(rows):
    if len(rows) != 64 or {r['repeat'] for r in rows} != set(range(4)):
        raise ValueError('four complete repeats required')
    keys = {(r['repeat'], r['index']) for r in rows}
    if len(keys) != 64 or len({r['index'] for r in rows}) != 16:
        raise ValueError('duplicate or mismatched tasks')
    for repeat in range(4):
        group = [r for r in rows if r['repeat'] == repeat]
        if len(group) != 16 or any(sum(r['recipe'] == f'aligned-approach-v1/stratum-{i}' for r in group) != 4 for i in range(4)):
            raise ValueError('incomplete task/stratum coverage')
    if any(r['outcome'] in [None, 'environment_error', 'budget_censored'] for r in rows):
        raise ValueError('incomplete or invalid physical outcome')


def finding(cells):
    for rows in cells.values():
        validate_rows(rows)
    for cell in CELLS[1:]:
        paired_outcomes(cells[CELLS[0]], cells[cell])
    counts = {cell: summarize(rows)['successes'] for cell, rows in cells.items()}
    repeat_delta = [sum(r['repeat'] == i and r['outcome'] == 'success' for r in cells['sampled_256'])
                    - sum(r['repeat'] == i and r['outcome'] == 'success' for r in cells['sampled_128'])
                    for i in range(4)]
    sampled = counts['sampled_256']-counts['sampled_128']
    mixed = counts['mixed_256']-counts['mixed_128']
    direction = ('opposite_changes' if sampled > 0 and mixed < 0 else
                 'both_declined' if sampled < 0 and mixed < 0 else
                 'sampled_unchanged_mixed_declined' if sampled == 0 and mixed < 0 else 'other')
    return {'success_counts': counts, 'sampled_delta': sampled, 'mixed_delta': mixed,
            'sampled_repeat_deltas': repeat_delta, 'direction': direction,
            'material_opposing_change': sampled >= 8 and mixed < 0 and sum(n > 0 for n in repeat_delta) >= 3,
            'paired_sampled_duration': paired_outcomes(cells['sampled_128'], cells['sampled_256']),
            'paired_mixed_duration': paired_outcomes(cells['mixed_128'], cells['mixed_256']),
            'paired_execution_at_256': paired_outcomes(cells['sampled_256'], cells['mixed_256'])}


def aggregate(records):
    if sorted(r['seed'] for r in records) != SEEDS:
        raise ValueError('three declared models required')
    return {'material_opposing_change': sum(r['finding']['material_opposing_change'] for r in records) >= 2,
            'models_with_opposite_changes': [r['seed'] for r in records if r['finding']['direction'] == 'opposite_changes'],
            'models_declining_in_both_modes': [r['seed'] for r in records if r['finding']['direction'] == 'both_declined'],
            'models_with_unchanged_sampled_success': [r['seed'] for r in records if r['finding']['sampled_delta'] == 0]}


def baseline_paths(archive, seed, cell, repeat, index):
    directory = archive/f'inputs/seed-{seed}'/cell
    stem = f'r{repeat}-clear-{index}.json.gz'
    trace = directory/'traces'/stem
    if not trace.exists():
        trace = trace.with_suffix('')
    return trace, directory/'policy'/stem


def checked_metrics(candidate, trace, rows, mode, repeat, rng_seed):
    strict = trace_metrics(trace)
    if strict['outcome'] is None or len(rows) != strict['ticks']:
        raise ValueError('incomplete trace or missing policy rows')
    if not all(s['action'] == r['action'] for s, r in zip(trace['steps'], rows)):
        raise ValueError('policy/physics action mismatch')
    return metrics(candidate, trace, rows, mode, repeat, rng_seed)


def run(archive, seed):
    out = archive/f'seed-{seed}'
    out.mkdir();(out/'traces').mkdir();(out/'policy').mkdir()
    start = time.monotonic();deadline = start+1200;records = [];ticks = 0
    inputs = archive/f'inputs/seed-{seed}'
    with torch.random.fork_rng(devices=[]), Bridge(archive/'runtime/flight_ppo_bridge') as bridge:
        model = frozen_export(inputs, seed, 256, bridge)
        before = {name: value.clone() for name, value in model.state_dict().items()}
        batch = read(inputs/'development-admission.json')
        assert batch['complete'] and batch['seed'] == 4210002
        candidates = [r['candidate'] for r in batch['attempts'] if r['admitted']]
        assert len(candidates) == 16
        try:
            for repeat in range(4):
                for candidate in candidates:
                    rng = episode_seed(seed, repeat, candidate['index'])
                    trace, rows = rollout(bridge, model, candidate, 'sampled', rng, deadline)
                    record = checked_metrics(candidate, trace, rows, 'sampled_256', repeat, rng)
                    ticks += len(rows)
                    assert ticks <= 115200
                    pairing = {}
                    for cell in ['sampled_128', 'mixed_256']:
                        trace_path, policy_path = baseline_paths(archive, seed, cell, repeat, candidate['index'])
                        old_trace, old_rows = read(trace_path), read(policy_path)
                        assert trace['initial'] == old_trace['initial']
                        if cell == 'mixed_256':
                            assert rows[0]['actor'] == old_rows[0]['actor']
                        pairing[cell] = paired_innovations(rows, old_rows)
                    record['pairing'] = pairing
                    stem = f"r{repeat}-clear-{candidate['index']}.json.gz"
                    for folder, value in [('traces', trace), ('policy', rows)]:
                        with gzip.open(out/folder/stem, 'wt') as stream:
                            json.dump(value, stream, allow_nan=False)
                    records.append(record)
                    atomic_json(out/'progress.json', {'episodes': len(records), 'physics_ticks': ticks,
                        'outcomes': dict(Counter(r['outcome'] for r in records))})
            assert all(torch.equal(value, before[name]) for name, value in model.state_dict().items())
            validate_rows(records)
        except Exception as error:
            atomic_json(out/'interruption.json', {'error': str(error), 'completed_episodes': records,
                                                 'physics_ticks': ticks})
            raise
    atomic_json(out/'results.json', {'seed': seed, 'episodes': records, 'physics_ticks': ticks,
        'seconds': time.monotonic()-start, 'model_unchanged': True, 'optimizer_steps': 0,
        'checkpoint_sha256': sha(inputs/'checkpoint-0256.pt')})


def report(archive, output):
    records = [];pooled = {cell: [] for cell in CELLS}
    for seed in SEEDS:
        result = read(archive/f'seed-{seed}/results.json')
        cells = {'sampled_256': result['episodes']}
        inputs = archive/f'inputs/seed-{seed}'
        batch = read(inputs/'development-admission.json')
        candidates = [r['candidate'] for r in batch['attempts'] if r['admitted']]
        for cell in ['sampled_128', 'mixed_128', 'mixed_256']:
            rows = []
            for repeat in range(4):
                for candidate in candidates:
                    paths = baseline_paths(archive, seed, cell, repeat, candidate['index'])
                    rows.append(checked_metrics(candidate, read(paths[0]), read(paths[1]), cell, repeat,
                                                episode_seed(seed, repeat, candidate['index'])))
            cells[cell] = rows
        record = {k: result[k] for k in ['seed', 'physics_ticks', 'seconds', 'model_unchanged', 'optimizer_steps', 'checkpoint_sha256']}
        record.update(finding=finding(cells),
            cells={cell: {'physical': summarize(rows), 'diagnostics': diagnostics(rows),
                          'repeats': [summarize([r for r in rows if r['repeat'] == i]) for i in range(4)]}
                   for cell, rows in cells.items()},
            pairing={cell: {'shared_ticks': sum(r['pairing'][cell]['shared_ticks'] for r in result['episodes']),
                            'maximum_error': max(r['pairing'][cell]['maximum_abs_standardized_innovation_error'] for r in result['episodes'])}
                     for cell in ['sampled_128', 'mixed_256']}, episodes=cells)
        records.append(record)
        for cell, rows in cells.items():
            pooled[cell].extend(rows)
    atomic_json(output, {'experiment_id': archive.name, 'seeds': records, 'finding': aggregate(records),
        'new_physics_ticks': sum(r['physics_ticks'] for r in records), 'preflight_physics_ticks': 160,
        'reused_physics_ticks': {cell: sum(r['ticks'] for r in rows) for cell, rows in pooled.items() if cell != 'sampled_256'},
        'pooled_descriptive_only': {cell: {'physical': summarize(rows), 'diagnostics': diagnostics(rows)} for cell, rows in pooled.items()},
        'optimizer_steps': 0, 'qualification_gate_changed': False, 'confirmation_used': False, 'general_flight_claim': False})


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--seed', type=int, choices=SEEDS)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    if (args.seed is None) == (args.report is None):
        parser.error('choose exactly one of --seed or --report')
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.report is not None:
        report(args.archive, args.report)
    else:
        run(args.archive, args.seed)


if __name__ == '__main__':
    main()
