"""LF-069 physical qualification, exact replay, coverage and stopping retention."""
import argparse
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_audit.motor_feedback import alter, acceleration, native_parity
from . import single_speed as stage
from .gae_audit import verify_targets
from .sustained_audit import read, load, verify_run, preflight_checks, inspect, task_matches, gate


def feedback_inputs(anchors):
    rows, metadata = [], []
    for anchor in anchors:
        x = torch.tensor([anchor['inputs']], dtype=torch.float32)
        for command in [0, 16]:
            for error in [-6, -.25, 0, .25, 6]:
                rows.append(alter(x, [command], [command+error])[0])
                metadata.append({'anchor': anchor['index'], 'command': command, 'error': error})
    return torch.stack(rows), metadata


def feedback(a):
    x, metadata = feedback_inputs(read(a/'inputs/feedback-anchors.json')); records = []
    assert x.shape == (540, 18)
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        for seed in stage.SEEDS:
            run = a/f'pilot/speed16/seed-{seed}'; original = None
            for phase, update in [('initial', 0), ('final', 86)]:
                state = load(run/f'checkpoint-{update:04}.pt.gz')
                with torch.random.fork_rng(devices=[]):
                    model = stage.make_model(state['contract']['schema'], state['contract']['config'])
                model.load_state_dict(state['model']); model.requires_grad_(False)
                parity = native_parity(bridge, run/f'policy-{update:04}.json', model, x)
                with torch.no_grad():
                    logits = model(x)[0]; actions = logits[:, 1:].sigmoid(); force = acceleration(x, actions)
                    scales = model.distribution(logits, x).scale
                stop = x[:, 2] == 0
                if original is None: original = (logits[stop].clone(), scales[stop].clone())
                else:
                    assert torch.equal(original[0], logits[stop]) and torch.equal(original[1], scales[stop])
                slopes = []
                for i in range(0, len(x), 5):
                    slope = (actions[i+3] - actions[i+1]) / .5
                    slopes.append({'anchor': metadata[i]['anchor'], 'command': metadata[i]['command'],
                        'throttle_per_speed': float(slope[0]), 'brake_per_speed': float(slope[1]),
                        'acceleration_per_speed': float((force[i+3] - force[i+1]) / .5),
                        'acceleration_at_under6': float(force[i]), 'acceleration_at_target': float(force[i+2]),
                        'acceleration_at_over6': float(force[i+4])})
                records.append({'seed': seed, 'phase': phase, 'actions': actions.tolist(), 'logits': logits.tolist(),
                    'acceleration': force.tolist(), 'slopes': slopes, 'parity': parity})
    result = {'inputs': x.tolist(), 'metadata': metadata, 'records': records, 'native_python_comparisons': 3240,
              'zero_command_before_after_rows': 810, 'new_physics_ticks': 0}
    atomic_json(a/'feedback.json', result)
    return result


def verify_schedule(a, seed, preflight):
    run = a/f"{'preflight' if preflight else 'pilot'}/speed16/seed-{seed}"
    c = stage.config(seed, 'speed16', preflight)
    for update in range(1, c['updates']+1):
        x = load(run/f'rollout-{update:04}.pt.gz')['observations']
        assert x.shape == (256, 6, 18)
        assert torch.equal(x[:, [0, 2, 4], 2], torch.full((256, 3), .8))
        assert not x[:, [1, 3, 5], 2].count_nonzero()
    result = {'updates': c['updates'], 'movement_command': 16, 'rows_per_update': 768,
              'movement_rows': c['updates']*768, 'stopping_rows': c['updates']*768}
    atomic_json(run/'schedule-verification.json', result)
    return result


def verify(a, preflight):
    start = time.monotonic(); records = []; evaluated = parent_exact = zero_retained = 0
    assert read(a/'experiment-plan.json')['training_failure_policy'] == 'record-physical-failures-v1'
    tasks = read(a/'tasks/development.json')
    assert len(tasks) == 104 and sum(t['task']['speed'] == 16 for t in tasks) == 24
    for seed in stage.SEEDS:
        r = verify_run(a, 'speed16', seed, preflight, stage=stage)
        r['target_verification'] = verify_targets(a, 'speed16', seed, preflight, stage=stage)
        r['schedule_verification'] = verify_schedule(a, seed, preflight); records.append(r)
        if preflight:
            checks = preflight_checks({**r, 'arm': 'sustain'}, 'record-physical-failures-v1')
            exposure = r['training_stats'].get('movement_ticks_after_360', 0) + sum(v['forgone_continuation_ticks'] for v in r['training_physical_failures'])
            checks['three_slot_continuation_exposure'] = exposure >= 1620
            r['preflight_checks'] = checks
            assert all(checks.values())
            continue
        # Three pools each complete >=24 movement episodes; their longest pool has three cases.
        assert min(v for k, v in r['training_coverage'].items() if int(k) % 2 == 0) >= 8
        run = a/f'pilot/speed16/seed-{seed}'; phases = {}; parent = []
        for task in tasks:
            if task['suite'].startswith('legacy'):
                parent.append({**task, **inspect(read(a/f"inputs/seed-{seed}/reference/case-{task['legacy_index']:02}.json.gz"))})
        for phase in ['initial', 'final']:
            directory = run/f'evaluation-{phase}'; rows = read(directory/'results.json')
            assert len(rows) == len(read(directory/'parity.json')) == 104; phases[phase] = []
            for row, task in zip(rows, tasks):
                assert row['index'] == task['index']
                trace = read(directory/f"case-{row['index']:02}.json.gz")
                assert task_matches(trace['task'], task['task'])
                found = inspect(trace)
                assert all(row[k] == found[k] for k in ['ticks', 'outcome', 'final_errors'])
                if task['suite'].startswith('legacy'):
                    assert trace == read(a/f"inputs/seed-{seed}/reference/case-{task['legacy_index']:02}.json.gz")
                    parent_exact += 1
                if phase == 'final' and task['task']['speed'] == 0:
                    assert trace == read(run/f"evaluation-initial/case-{task['index']:02}.json.gz")
                    zero_retained += 1
                phases[phase].append({**task, **found}); evaluated += found['ticks']
        r['gate'] = gate(phases['initial'], phases['final'], parent)
        r['trajectories'] = phases
    common = {'records': records, 'training_failure_policy': 'record-physical-failures-v1',
        'optimizer_steps': sum(r['optimizer_steps'] + r['repeated_optimizer_steps'] for r in records),
        'restore_ticks': sum(r['restore_ticks'] for r in records),
        'neutral_setup_ticks': sum(r['neutral_setups'] for r in records), 'exact_resumes': 3,
        'verified_gae_rows': sum(r['target_verification']['rows'] for r in records)}
    if preflight:
        result = {**common, 'passed': True, 'training_ticks': 23040}
        assert result['optimizer_steps'] <= 360 and result['restore_ticks'] <= 12000
        target = 'preflight-results.json'
    else:
        pre = read(a/'preflight-results.json'); assert pre['passed']
        assert parent_exact == 336 and zero_retained == 240
        feedback(a)
        qualified = sum(r['gate']['passed'] for r in records)
        result = {**common, 'experiment_id': a.name, 'qualified_seeds': qualified,
            'movement_learning_seeds': sum(r['gate']['movement_learning_passed'] for r in records),
            'passed': qualified >= 2, 'verification_passed': True,
            'training_ticks': 423936, 'main_training_ticks': 396288,
            'main_movement_ticks': 198144, 'main_stopping_ticks': 198144,
            'evaluation_ticks': evaluated, 'evaluation_episodes': 624,
            'native_python_comparisons': 3864, 'likelihood_comparisons': 414720,
            'protected_stopping_rows': 207360, 'frozen_parent_checkpoints': 282,
            'original_parent_traces_exact': 336, 'zero_command_initial_final_traces_exact': 240,
            'zero_command_feedback_rows_exact': 810, 'declared_parent_initializations': 6,
            'confirmation_used': False, 'model_promoted': False, 'navigation_qualified': False}
        for key in ['optimizer_steps', 'restore_ticks', 'neutral_setup_ticks', 'exact_resumes', 'verified_gae_rows']:
            result[key] += pre[key]
        result['neutral_setup_ticks'] += 624
        assert result['optimizer_steps'] <= 6624 and result['restore_ticks'] <= 24000 and evaluated <= 561600
        assert result['verified_gae_rows'] == 36864 and result['exact_resumes'] == 6
        target = 'results.json'
    result['lambda1_telescoping_rows'] = result['verified_gae_rows']
    result['seconds'] = time.monotonic() - start
    assert result['seconds'] < 600
    atomic_json(a/target, result)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(); parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--preflight', action='store_true'); args = parser.parse_args()
    torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
    verify(args.archive, args.preflight)
