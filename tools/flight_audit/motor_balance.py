"""LF-066: saved GAE policies' force balance, response strength and update credit.

Read-only tensor/inference audit. No optimizer, native reset or physics step.
Force crossings hold all other observations, including previous actions, fixed.
"""
import argparse
import copy
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from tools.flight_motor import aligned, sustained, storage
from . import motor_feedback as feedback

ARMS = ['lambda095', 'lambda1']
SEEDS = [17, 29, 43]
UPDATES = [0, 1, 4, 16, 32, 64, 96, 128]
COMMANDS = [4, 6, 10, 12, 16, 18]
SPEEDS = [i / 2 for i in range(61)]
LOCAL_ERRORS = [-1, -.25, 0, .25, 1]
read = storage.read_json
load = storage.load_torch


def dense_grid(anchors):
    rows = []
    for anchor in anchors:
        x = torch.tensor([anchor['inputs']], dtype=torch.float32).repeat(len(SPEEDS), 1)
        for command in COMMANDS:
            rows.append(feedback.alter(x, [command] * len(x), SPEEDS))
    return torch.cat(rows)


def local_grid(anchors):
    x, _ = feedback.grid(anchors, [0, *COMMANDS], LOCAL_ERRORS)
    # The older helper uses integer error labels for its integer-only panel.
    metadata = torch.tensor([[i, command, error] for i in range(len(anchors))
                             for command in [0, *COMMANDS] for error in LOCAL_ERRORS],
                            dtype=torch.float32)
    return x, metadata


def crossings(speeds, forces):
    """Bracketed static zero crossings; flat/exact-zero samples stay explicit."""
    result = []
    for i in range(len(speeds) - 1):
        left, right = float(forces[i]), float(forces[i + 1])
        if left * right < 0:
            value = speeds[i] - left * (speeds[i + 1] - speeds[i]) / (right - left)
            result.append({'speed': value, 'bracket': [speeds[i], speeds[i + 1]],
                           'restoring': left > 0, 'slope': (right - left) / (speeds[i + 1] - speeds[i])})
        elif left == 0:
            result.append({'speed': speeds[i], 'bracket': [speeds[i], speeds[i]],
                           'restoring': None, 'slope': None})
    if float(forces[-1]) == 0:
        result.append({'speed': speeds[-1], 'bracket': [speeds[-1], speeds[-1]],
                       'restoring': None, 'slope': None})
    return result


def balance_rows(acceleration):
    shaped = acceleration.reshape(54, len(COMMANDS), len(SPEEDS))
    return [{'anchor': anchor, 'command': command,
             'crossings': crossings(SPEEDS, shaped[anchor, j])}
            for anchor in range(54) for j, command in enumerate(COMMANDS)]


def evaluate(model, x):
    with torch.no_grad():
        raw, _ = model(x)
        actions = raw[:, 1:].sigmoid()
        return {'logits': raw[:, 1:], 'actions': actions,
                'acceleration': feedback.acceleration(x, actions)}


def local_summary(x, values):
    forces = values['acceleration'].reshape(54, 7, 5)
    actions = values['actions'].reshape(54, 7, 5, 2)
    # Frozen-action force changes expose the passive brake contribution.
    held = actions[:, :, 2:3].expand(-1, -1, 5, -1).reshape(-1, 2)
    passive = feedback.acceleration(x, held).reshape(54, 7, 5)
    return {'target_acceleration': forces[:, :, 2],
            'acceleration_slope': (forces[:, :, 3] - forces[:, :, 1]) / .5,
            'passive_slope': (passive[:, :, 3] - passive[:, :, 1]) / .5,
            'action_slope': (actions[:, :, 3] - actions[:, :, 1]) / .5}


def masks(x):
    error, groups = feedback.training_groups(x)
    command = x[:, 2] * 20
    for speed in [4, 10, 16]:
        selected = (command - speed).abs() < 1e-5
        for name in ['movement', 'ready_under', 'ready_near', 'ready_over']:
            groups[f'{name}_command_{speed}'] = groups[name] & selected
        # Near-target sides exclude the exact-zero tie and use the same readiness.
        ready = groups['ready_under'] | groups['ready_near'] | groups['ready_over']
        groups[f'near_under_command_{speed}'] = selected & ready & (error >= -3) & (error < 0)
        groups[f'near_over_command_{speed}'] = selected & ready & (error > 0) & (error <= 3)
    return error, groups


def training_summary(rollout, before, after):
    flat = {key: value.flatten(0, 1) for key, value in rollout.items()}
    x = flat['observations']; error, groups = masks(x)
    with torch.no_grad():
        raw, _ = before(x); future, _ = after(x)
        logp = before.log_prob(raw, flat['latent'], flat['actuator_latents'], x)
        torch.testing.assert_close(logp, flat['logp'], atol=1e-4, rtol=1e-5)
        adv = aligned.advantages(flat['advantages'], x[:, 2] > 0)
        dist = before.distribution(raw, x)
        mean = raw[:, 1:].sigmoid(); sample = flat['actuator_latents'].sigmoid()
        post = future[:, 1:].sigmoid()
        force = feedback.acceleration(x, mean)
        sampled = feedback.acceleration(x, sample)
        force_delta = feedback.acceleration(x, post) - force
        score = adv[:, None] * (flat['actuator_latents'] - raw[:, 1:]) / dist.scale.square()
        desired = -error.sign()
        corrective_sample = (sampled - force) * desired > 0
        opposing_sample = (sampled - force) * desired < 0
        fields = {'error': error, 'raw_advantage': flat['advantages'], 'advantage': adv,
                  'mean_acceleration': force, 'sample_acceleration': sampled,
                  'delta_acceleration': force_delta, 'corrective_acceleration_delta': desired * force_delta,
                  'mean_thrust': mean[:, 0], 'mean_brake': mean[:, 1],
                  'delta_thrust': post[:, 0] - mean[:, 0], 'delta_brake': post[:, 1] - mean[:, 1],
                  'thrust_score': score[:, 0], 'brake_score': score[:, 1],
                  'corrective_thrust_score': desired * score[:, 0],
                  'corrective_brake_score': -desired * score[:, 1],
                  'corrective_update': (desired * force_delta > 0).float(),
                  'corrective_sample_count': corrective_sample.float(),
                  'corrective_sample_positive_count': (corrective_sample & (adv > 0)).float(),
                  'corrective_sample_advantage_sum': adv * corrective_sample,
                  'opposing_sample_count': opposing_sample.float(),
                  'opposing_sample_positive_count': (opposing_sample & (adv > 0)).float(),
                  'opposing_sample_advantage_sum': adv * opposing_sample}
        stats = {name: {'count': int(mask.sum()),
                        **{key: float(value[mask].double().sum()) for key, value in fields.items()}}
                 for name, mask in groups.items()}
        # Check zero-speed logits/distributions directly, not just retained scores.
        stopping = x[:, 2] == 0
        assert torch.equal(raw[stopping], future[stopping])
        assert torch.equal(dist.scale[stopping], after.distribution(future, x).scale[stopping])
    return stats, float((logp - flat['logp']).abs().max())


def aggregate(training, lo, hi):
    totals = {}
    for row in training:
        if not lo <= row['update'] <= hi:
            continue
        for name, values in row['groups'].items():
            out = totals.setdefault(name, {key: 0 for key in values})
            for key, value in values.items():
                out[key] += value
    return {name: {'count': values['count'],
                   **{key: value / values['count'] if values['count'] else None
                      for key, value in values.items() if key != 'count'}}
            for name, values in totals.items()}


def audit(a, arm, seed):
    start = time.monotonic(); out = a / f'audit/{arm}/seed-{seed}'; out.mkdir()
    run = a / f'inputs/{arm}/seed-{seed}'; anchors = read(a / 'anchors.json')
    assert len(anchors) == 54
    dense = dense_grid(anchors)
    local, metadata = local_grid(anchors)
    # Input layout remains an open-wing, nonrotating planet control diagnostic.
    assert torch.equal(dense[:, 10], torch.ones(len(dense))) and not dense[:, 8:10].count_nonzero()
    first = load(run / 'checkpoint-0000.pt.gz'); rng = torch.get_rng_state().clone()
    with torch.random.fork_rng(devices=[]):
        before = sustained.ResidualModel(first['contract']['schema'])
        after = sustained.ResidualModel(first['contract']['schema'])
    before.requires_grad_(False); after.requires_grad_(False)
    prior = first; history = []; grids = []; training = []; parity = []; maximum = 0.
    with Bridge(a / 'runtime/flight_motor_bridge') as bridge:
        for update in range(129):
            current = first if update == 0 else load(run / f'checkpoint-{update:04}.pt.gz')
            sustained.assert_parent_actor(first['model'], current['model'])
            after.load_state_dict(current['model'])
            snapshot = copy.deepcopy(after.state_dict())
            values = evaluate(after, local)
            history.append({'update': update, **values, **local_summary(local, values)})
            if update in UPDATES:
                values = evaluate(after, dense)
                grids.append({'update': update, **values, 'balances': balance_rows(values['acceleration'])})
            if update in [0, 128]:
                parity.append({'update': update, **feedback.native_parity(
                    bridge, run / f'policy-{update:04}.json', after, local)})
            if update:
                before.load_state_dict(prior['model'])
                stats, error = training_summary(load(run / f'rollout-{update:04}.pt.gz'), before, after)
                training.append({'update': update, 'groups': stats}); maximum = max(maximum, error)
            assert equal_tree(snapshot, after.state_dict())
            prior = current
            if time.monotonic() - start > 600:
                raise TimeoutError('balance audit budget')
    neutral = dense.clone(); neutral[:, 15:18] = 0
    neutral_values = evaluate(after, neutral)
    assert equal_tree(prior['model'], after.state_dict()) and torch.equal(rng, torch.get_rng_state())
    data = {'dense_inputs': dense, 'local_inputs': local, 'local_metadata': metadata,
            'grids': grids, 'history': history, 'neutral_final': neutral_values,
            'neutral_balances': balance_rows(neutral_values['acceleration'])}
    storage.dump_torch(out / 'responses.pt.gz', data)
    atomic_json(out / 'training.json', training)
    result = {'arm': arm, 'seed': seed, 'parity': parity, 'native_python_comparisons': 2 * len(local),
              'likelihood_rows': 131072, 'protected_stopping_rows': 65536,
              'max_log_probability_error': maximum, 'frozen_parent_checkpoints': 129,
              'training_windows': {name: aggregate(training, lo, hi) for name, lo, hi in
                                   [('all', 1, 128), ('early', 1, 32), ('middle', 33, 96), ('late', 97, 128)]},
              'weights_rng_unchanged': True, 'training_ticks': 0, 'physics_ticks': 0, 'optimizer_steps': 0,
              'seconds': time.monotonic() - start}
    assert result['seconds'] < 600
    atomic_json(out / 'results.json', result)


def verify(a):
    start = time.monotonic(); records = []; initial = {}; zero = {}; dense_initial = {}
    for arm in ARMS:
        for seed in SEEDS:
            out = a / f'audit/{arm}/seed-{seed}'; r = read(out / 'results.json')
            data = load(out / 'responses.pt.gz'); training = read(out / 'training.json')
            expected_inputs, expected_metadata = local_grid(read(a / 'anchors.json'))
            assert torch.equal(data['local_inputs'], expected_inputs)
            if (out / 'local-metadata-v2.pt.gz').exists():
                # Retain the original artifact and explicitly identify its
                # truncated labels; policy inputs/outputs are not regenerated.
                assert torch.equal(data['local_metadata'], expected_metadata.to(torch.int64))
                data['local_metadata'] = load(out / 'local-metadata-v2.pt.gz')
            assert torch.equal(data['local_metadata'], expected_metadata)
            assert len(data['dense_inputs']) == 19764 and len(data['local_inputs']) == 1890
            assert [g['update'] for g in data['grids']] == UPDATES
            assert [h['update'] for h in data['history']] == list(range(129))
            assert [t['update'] for t in training] == list(range(1, 129))
            for name, lo, hi in [('all', 1, 128), ('early', 1, 32), ('middle', 33, 96), ('late', 97, 128)]:
                assert r['training_windows'][name] == aggregate(training, lo, hi)
            assert r['training_windows']['all']['movement']['count'] == 65536
            assert r['training_windows']['all']['stopping']['count'] == 65536
            assert r['weights_rng_unchanged'] and r['seconds'] < 600
            stop = data['local_metadata'][:, 1] == 0
            for h in data['history']:
                assert torch.equal(h['logits'][stop], data['history'][0]['logits'][stop])
                assert torch.equal(h['actions'][stop], data['history'][0]['actions'][stop])
                assert equal_tree(local_summary(data['local_inputs'], h),
                                  {k: h[k] for k in ['target_acceleration', 'acceleration_slope', 'passive_slope', 'action_slope']})
            for g in data['grids']:
                assert equal_tree(g['balances'], balance_rows(g['acceleration']))
            if arm == ARMS[0]:
                initial[seed] = data['history'][0]; dense_initial[seed] = data['grids'][0]
                zero[seed] = data['history'][0]['logits'][stop]
            else:
                assert equal_tree(initial[seed], data['history'][0])
                assert equal_tree(dense_initial[seed], data['grids'][0])
                assert torch.equal(zero[seed], data['history'][-1]['logits'][stop])
            records.append(r)
    result = {'experiment_id': a.name, 'verification_passed': True, 'records': records,
              'source_policy_runs': 6, 'common_anchors': 54, 'dense_response_rows': 948672,
              'local_history_rows': 1462860, 'neutral_response_rows': 118584,
              'native_python_comparisons': sum(r['native_python_comparisons'] for r in records),
              'likelihood_rows': sum(r['likelihood_rows'] for r in records),
              'protected_stopping_rows': sum(r['protected_stopping_rows'] for r in records),
              'frozen_parent_checkpoints': 774, 'paired_initial_local_rows': 5670,
              'paired_initial_dense_rows': 59292, 'training_ticks': 0, 'physics_ticks': 0,
              'optimizer_steps': 0, 'new_checkpoints': 0, 'new_policy_exports': 0,
              'confirmation_used': False, 'model_promoted': False, 'navigation_qualified': False,
              'verification_revision': 'fractional-local-metadata-v2',
              'seconds': time.monotonic() - start}
    assert result['native_python_comparisons'] == 22680 and result['likelihood_rows'] == 786432
    assert result['protected_stopping_rows'] == 393216 and result['seconds'] < 300
    atomic_json(a / 'results.json', result)


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--archive', type=Path, required=True)
    p.add_argument('--arm', choices=ARMS); p.add_argument('--seed', type=int, choices=SEEDS)
    args = p.parse_args(); torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
    if args.arm:
        audit(args.archive, args.arm, args.seed)
    else:
        verify(args.archive)
