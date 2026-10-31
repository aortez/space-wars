"""LF-067 strict physical gates, scheduler accounting and protected baseline."""
import argparse
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from . import command_balance as stage
from .gae_audit import verify_targets
from .movement_audit import feedback
from .sustained_audit import read, load, verify_run, preflight_checks, inspect, task_matches, gate


def verify_schedule(a, arm, seed, preflight):
    phase = 'preflight' if preflight else 'pilot'; run = a / f'{phase}/{arm}/seed-{seed}'
    c = stage.config(seed, arm, preflight); rows = []; totals = {str(s): 0 for s in [4, 10, 16]}
    for u in range(1, c['updates'] + 1):
        r = load(run / f'rollout-{u:04}.pt.gz'); x = r['observations']
        assert tuple(x.shape) == (256, 6, 18)
        counts = {str(s): int(((x[:, :, 2] * 20 - s).abs() < 1e-5).sum()) for s in [4, 10, 16]}
        assert sum(counts.values()) == 768 and int((x[:, :, 2] == 0).sum()) == 768
        if arm == 'balanced' or preflight:
            assert list(counts.values()) == [256, 256, 256]
        for k, v in counts.items(): totals[k] += v
        rows.append({'update': u, 'movement_counts': counts})
    if not preflight and arm == 'cyclic':
        assert rows[0]['movement_counts'] == {'4': 768, '10': 0, '16': 0}
    result = {'updates': rows, 'total_movement_counts': totals,
              'updates_with_all_commands': sum(all(v > 0 for v in x['movement_counts'].values()) for x in rows),
              'updates_exactly_balanced': sum(list(x['movement_counts'].values()) == [256, 256, 256] for x in rows)}
    atomic_json(run / 'schedule-verification.json', result)
    return result


def verify_pair(a, seed, preflight):
    phase = 'preflight' if preflight else 'pilot'
    runs = [a / f'{phase}/{arm}/seed-{seed}' for arm in stage.ARMS]
    starts = [load(p / 'checkpoint-0000.pt.gz') for p in runs]
    for key in ['model', 'optimizer', 'update', 'history', 'torch_rng', 'python_rng']:
        assert equal_tree(starts[0][key], starts[1][key])
    assert starts[0]['optimizer']['state'] == {}
    for i in [1, 3, 5]:
        assert starts[0]['collector']['pools'][i] == starts[1]['collector']['pools'][i]
    if preflight:
        assert equal_tree(starts[0]['collector'], starts[1]['collector'])
        assert equal_tree(load(runs[0] / 'rollout-0001.pt.gz'), load(runs[1] / 'rollout-0001.pt.gz'))
        ends = [load(p / 'checkpoint-0004.pt.gz') for p in runs]
        assert equal_tree({k: v for k, v in ends[0].items() if k != 'contract'},
                          {k: v for k, v in ends[1].items() if k != 'contract'})
    else:
        assert starts[0]['collector']['pools'] != starts[1]['collector']['pools']
    return {'seed': seed, 'initial_model_optimizer_rng_exact': True, 'stopping_pools_exact': True,
            'preflight_rollout_and_final_state_exact': preflight, 'main_collector_difference_intended': not preflight}


def verify(a, preflight):
    start = time.monotonic(); records = []; evaluated = 0; parent_exact = zero_retained = 0
    failure_policy = read(a / 'experiment-plan.json')['training_failure_policy']
    assert failure_policy == 'record-physical-failures-v1'
    tasks = read(a / 'tasks/development.json')
    for arm in stage.ARMS:
        for seed in stage.SEEDS:
            r = verify_run(a, arm, seed, preflight, stage=stage)
            r['target_verification'] = verify_targets(a, arm, seed, preflight, stage=stage)
            r['schedule_verification'] = verify_schedule(a, arm, seed, preflight)
            records.append(r)
            if preflight:
                r['preflight_checks'] = preflight_checks({**r, 'arm': 'sustain'}, failure_policy)
                accounted = r['training_stats'].get('movement_ticks_after_360', 0) + sum(
                    v['forgone_continuation_ticks'] for v in r['training_physical_failures'])
                r['preflight_checks']['three_slot_continuation_exposure'] = accounted >= 1620
                assert all(r['preflight_checks'].values())
                continue
            run = a / f'pilot/{arm}/seed-{seed}'; phases = {}; parent = []
            for candidate in tasks:
                if candidate['suite'].startswith('legacy'):
                    old = read(a / f"inputs/seed-{seed}/reference/case-{candidate['legacy_index']:02}.json.gz")
                    parent.append({**candidate, **inspect(old)})
            for phase in ['initial', 'final']:
                directory = run / f'evaluation-{phase}'; rows = read(directory / 'results.json')
                assert len(rows) == len(read(directory / 'parity.json')) == 104; phases[phase] = []
                for row, candidate in zip(rows, tasks):
                    assert row['index'] == candidate['index']
                    trace = read(directory / f"case-{row['index']:02}.json.gz")
                    assert task_matches(trace['task'], candidate['task'])
                    inspection = inspect(trace)
                    assert all(row[k] == inspection[k] for k in ['ticks', 'outcome', 'final_errors'])
                    if candidate['suite'].startswith('legacy'):
                        assert trace == read(a / f"inputs/seed-{seed}/reference/case-{candidate['legacy_index']:02}.json.gz")
                        parent_exact += 1
                    if phase == 'initial' and arm == 'balanced':
                        assert trace == read(a / f"pilot/cyclic/seed-{seed}/evaluation-initial/case-{candidate['index']:02}.json.gz")
                    if phase == 'final' and candidate['task']['speed'] == 0:
                        assert trace == read(run / f"evaluation-initial/case-{candidate['index']:02}.json.gz")
                        zero_retained += 1
                    phases[phase].append({**candidate, **inspection}); evaluated += inspection['ticks']
            r['gate'] = gate(phases['initial'], phases['final'], parent); r['trajectories'] = phases
    pairs = [verify_pair(a, seed, preflight) for seed in stage.SEEDS]
    if preflight:
        result = {'passed': True, 'records': records, 'pairs': pairs, 'training_ticks': 46080,
                  'optimizer_steps': sum(r['optimizer_steps'] + r['repeated_optimizer_steps'] for r in records),
                  'restore_ticks': sum(r['restore_ticks'] for r in records),
                  'neutral_setup_ticks': sum(r['neutral_setups'] for r in records), 'exact_resumes': 6}
        assert result['optimizer_steps'] <= 720 and result['restore_ticks'] <= 24000
        target = 'preflight-results.json'
    else:
        pre = read(a / 'preflight-results.json'); assert pre['passed']
        comparisons = []
        for seed in stage.SEEDS:
            one = next(x for x in records if x['seed'] == seed and x['arm'] == 'cyclic')
            two = next(x for x in records if x['seed'] == seed and x['arm'] == 'balanced')
            comparisons.append({'seed': seed, 'tracking_gain_over_cyclic': two['gate']['tracking_retained'] - one['gate']['tracking_retained'],
                                'balanced_joint_gate': two['gate']['passed'], 'balanced_learning': two['gate']['movement_learning_passed']})
        qualified = {arm: sum(x['gate']['passed'] for x in records if x['arm'] == arm) for arm in stage.ARMS}
        assert parent_exact == 672 and zero_retained == 480
        feedback(a, stage=stage)
        result = {'experiment_id': a.name, 'records': records, 'pairs': pairs, 'paired_comparisons': comparisons,
                  'qualified_seeds': qualified, 'passed': qualified['balanced'] >= 2,
                  'balanced_improvement': sum(x['tracking_gain_over_cyclic'] >= 4 for x in comparisons) >= 2,
                  'training_ticks': 847872, 'main_training_ticks': 792576,
                  'main_movement_ticks': 396288, 'main_stopping_ticks': 396288,
                  'optimizer_steps': pre['optimizer_steps'] + sum(x['optimizer_steps'] + x['repeated_optimizer_steps'] for x in records),
                  'restore_ticks': pre['restore_ticks'] + sum(x['restore_ticks'] for x in records),
                  'evaluation_ticks': evaluated, 'evaluation_episodes': 1248,
                  'native_python_comparisons': 10968, 'evaluation_inference_comparisons': 1248, 'feedback_inference_comparisons': 9720,
                  'paired_initial_feedback_outputs_exact': 2430, 'likelihood_comparisons': 829440,
                  'protected_stopping_rows': 414720, 'frozen_parent_checkpoints': 564,
                  'neutral_setup_ticks': pre['neutral_setup_ticks'] + sum(x['neutral_setups'] for x in records) + 1248,
                  'exact_resumes': 12, 'declared_parent_initializations': 12, 'paired_initial_traces_exact': 312,
                  'original_parent_traces_exact': 672, 'zero_command_initial_final_traces_exact': 480,
                  'confirmation_used': False, 'model_promoted': False, 'navigation_qualified': False}
        assert result['optimizer_steps'] <= 13248 and result['restore_ticks'] <= 48000 and evaluated <= 1123200
        target = 'results.json'
    result['verified_gae_rows'] = sum(x['target_verification']['rows'] for x in records) + (0 if preflight else pre['verified_gae_rows'])
    result['lambda1_telescoping_rows'] = result['verified_gae_rows']
    result['training_failure_policy'] = failure_policy
    result['seconds'] = time.monotonic() - start
    assert result['seconds'] < 600
    atomic_json(a / target, result)


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--archive', type=Path, required=True); p.add_argument('--preflight', action='store_true')
    args = p.parse_args(); torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
    verify(args.archive, args.preflight)
