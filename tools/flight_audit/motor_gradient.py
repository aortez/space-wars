"""LF-068: exact saved-update replay and per-command learning-signal audit.

Only private model/optimizer copies change. No native reset, step, training
trajectory, new policy export or checkpoint selection is performed.
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
from . import motor_balance as balance
from . import motor_gradient_math as math

ARMS = ['cyclic', 'balanced']
SEEDS = [17, 29, 43]
UPDATES = [1, 16, 43, 64, 86]
read, load = storage.read_json, storage.load_torch


def replay(model, optimizer, rollout, c):
    names, params = math.parameters(model)
    account = math.AdamAccounting(optimizer, params)
    f = {k: v.flatten(0, 1) for k, v in rollout.items()}
    move = f['observations'][:, 2] > 0
    adv = aligned.advantages(f['advantages'], move)
    logs, traces = [], []
    for epoch in range(c['epochs']):
        for indices in torch.randperm(len(adv)).split(c['minibatch']):
            x = f['observations'][indices]
            actor, value = model(x)
            logp, entropy = sustained.terms(model, actor, f, indices, c)
            logratio = logp - f['logp'][indices]; ratio = logratio.exp()
            policy, entropy_loss, move_loss, stop_loss = aligned.actor_losses(ratio, adv[indices], entropy, move[indices], c)
            value_loss = .5 * (value - f['returns'][indices]).square().mean()
            loss = policy + c['value_weight'] * value_loss + entropy_loss
            kl = ((ratio - 1) - logratio).mean().detach()
            assert torch.isfinite(loss) and kl <= c['target_kl']
            surrogate = torch.minimum(ratio * adv[indices], ratio.clamp(1-c['clip'], 1+c['clip']) * adv[indices])
            components = math.command_losses(surrogate, x) + [.5 * c['stopping_actor_weight'] * stop_loss,
                         entropy_loss, c['value_weight'] * value_loss]
            gradients = torch.stack([math.gradient(part, params) for part in components])
            optimizer.zero_grad(set_to_none=True); loss.backward()
            total_gradient = math.flatten([p.grad for p in params])
            torch.testing.assert_close(gradients.sum(0), total_gradient, atol=1e-6, rtol=1e-4)
            assert gradients[3].count_nonzero() == 0
            before = math.flatten(params).clone()
            norm = torch.nn.utils.clip_grad_norm_(model.parameters(), c['grad_clip'], error_if_nonfinite=True)
            clip = float((c['grad_clip'] / (norm + 1e-6)).clamp(max=1))
            optimizer.step()
            delta = math.flatten(params) - before
            allocation, moment_error = account.allocate(gradients, clip, delta)
            actor_norm = gradients[:5].sum(0).norm()
            traces.append({'epoch': epoch, 'indices': indices, 'gradients': gradients,
                'total_gradient': total_gradient, 'clip_factor': clip,
                'actor_only_clip_factor': float((c['grad_clip'] / (actor_norm + 1e-6)).clamp(max=1)),
                'delta': delta, 'allocation': allocation, 'moment_error': moment_error,
                'command_rows': [int(mask.sum()) for mask in math.masks_by_command(x)]})
            logs.append({'policy_loss': float(policy.detach()), 'movement_policy_loss': float(move_loss.detach()),
                'stopping_policy_loss': float(stop_loss.detach()), 'value_loss': float(value_loss.detach()),
                'entropy_loss': float(entropy_loss.detach()), 'kl': float(kl), 'gradient_norm': float(norm),
                'clip_fraction': float(((ratio-1).abs() > c['clip']).float().mean().detach())})
    metrics = {'minibatches': len(logs), 'kl_early_stop': False, 'movement_rows': int(move.sum()),
        'stopping_rows': int((~move).sum()), 'movement_positive_advantages': int((adv[move] > 0).sum()),
        'stopping_positive_advantages': int((adv[~move] > 0).sum()),
        **{k: sum(row[k] for row in logs) / len(logs) for k in logs[0]}}
    return metrics, traces


def normalization(model, rollout):
    _, params = math.parameters(model)
    f = {k: v.flatten(0, 1) for k, v in rollout.items()}; x = f['observations']
    actor, _ = model(x)
    ratio = (model.log_prob(actor, f['latent'], f['actuator_latents'], x) - f['logp']).exp()
    variants = math.normalized_variants(f['advantages'], x)
    gradients = {name: torch.stack([math.gradient(part, params)
                    for part in math.command_losses(ratio * advantage, x)]) for name, advantage in variants.items()}
    stats = []
    for command, mask in zip(math.COMMANDS, math.masks_by_command(x)):
        n = int(mask.sum())
        stats.append({'command': command, 'count': n,
            'raw_to_pooled_sign_flips': int(((variants['raw'][mask] > 0) != (variants['pooled'][mask] > 0)).sum()),
            'advantages': {name: {'mean': float(a[mask].mean()) if n else None,
                          'std': float(a[mask].std(unbiased=False)) if n else None,
                          'positive': int((a[mask] > 0).sum())} for name, a in variants.items()}})
    return {'gradients': gradients, 'stats': stats}


def probe(model, x, metadata, rollout):
    _, params = math.parameters(model)
    rows = rollout['observations'].flatten(0, 1)
    all_x = torch.cat((x, rows)); actor, _ = model(all_x)
    forces = feedback.acceleration(all_x, actor[:, 1:].sigmoid())
    groups = []
    for panel in ['anchors_mean', 'anchor_51']:
        for command in [0, *math.COMMANDS]:
            for error in [-3, 0, 3]:
                mask = (metadata[:, 1] == command) & (metadata[:, 2] == error)
                if panel == 'anchor_51': mask &= metadata[:, 0] == 51
                groups.append(({'panel': panel, 'command': command, 'error': error}, mask.nonzero().flatten()))
    _, masks = balance.masks(rows)
    for command in math.COMMANDS:
        for side in ['under', 'over']:
            groups.append(({'panel': 'saved_ready_near', 'command': command, 'side': side},
                           masks[f'near_{side}_command_{command}'].nonzero().flatten() + len(x)))
    output, jacobians = [], []
    for label, indices in groups:
        n = len(indices)
        value = forces[indices].mean() if n else forces.sum() * 0
        jac = math.gradient(value, params)
        if label['command'] == 0: assert not jac.count_nonzero()
        output.append({**label, 'count': n, 'indices': indices.tolist(), 'force': float(value.detach()) if n else None})
        jacobians.append(jac)
    return {'inputs': all_x, 'groups': output, 'jacobians': torch.stack(jacobians),
            'forces': forces.detach(), 'logits': actor.detach()}


def pair_summary(vectors, masks):
    return {kind: [{'left': math.COMMANDS[i], 'right': math.COMMANDS[j],
                   'cosine': math.cosine(vectors[i][mask], vectors[j][mask])}
                  for i, j in [(0, 1), (0, 2), (1, 2)]] for kind, mask in masks.items()}


def run(a, arm, seed):
    started = time.monotonic(); out = a / f'audit/{arm}/seed-{seed}'; out.mkdir()
    source = a / f'inputs/{arm}/seed-{seed}'
    x, metadata = feedback.grid(read(a/'anchors.json'), [0, *math.COMMANDS], [-3, 0, 3])
    assert x.shape == (648, 18)
    initial = load(source/'checkpoint-0000.pt.gz'); records, parity = [], []
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        for update in UPDATES:
            before = initial if update == 1 else load(source/f'checkpoint-{update-1:04}.pt.gz')
            after = load(source/f'checkpoint-{update:04}.pt.gz'); rollout = load(source/f'rollout-{update:04}.pt.gz')
            c = before['contract']['config']
            with torch.random.fork_rng(devices=[]):
                model = sustained.ResidualModel(before['contract']['schema'])
                final = sustained.ResidualModel(before['contract']['schema'])
            model.load_state_dict(before['model']); final.load_state_dict(after['model'])
            names, params = math.parameters(model); masks = math.parameter_masks(names, params)
            assert sum(p.numel() for p in params) == 3493
            for state in [before, after]: sustained.assert_parent_actor(initial['model'], state['model'])
            optimizer = torch.optim.Adam(model.parameters(), lr=c['learning_rate'], eps=1e-5)
            optimizer.load_state_dict(copy.deepcopy(before['optimizer']))
            norm = normalization(model, rollout)
            probes = probe(model, x, metadata, rollout)
            with torch.no_grad():
                raw_final, _ = final(probes['inputs'])
                final_forces = feedback.acceleration(probes['inputs'], raw_final[:, 1:].sigmoid())
                stop = probes['inputs'][:, 2] == 0
                assert torch.equal(probes['logits'][stop], raw_final[stop])
                assert torch.equal(model.distribution(probes['logits'], probes['inputs']).scale[stop],
                                   final.distribution(raw_final, probes['inputs']).scale[stop])
            if update == 1: parity.append(feedback.native_parity(bridge, source/'policy-0000.json', model, x))
            if update == 86: parity.append(feedback.native_parity(bridge, source/'policy-0086.json', final, x))
            torch.set_rng_state(before['torch_rng'])
            with torch.no_grad():
                for t, rows in enumerate(rollout['observations']):
                    latent, z, logp, _ = model.sample(rows)
                    for actual, expected in [(latent, rollout['latent'][t]), (z, rollout['actuator_latents'][t]), (logp, rollout['logp'][t])]:
                        assert torch.equal(actual, expected)
            metrics, steps = replay(model, optimizer, rollout, c)
            assert len(steps) == 24
            assert equal_tree(model.state_dict(), after['model']) and equal_tree(optimizer.state_dict(), after['optimizer'])
            assert torch.equal(torch.get_rng_state(), after['torch_rng'])
            assert metrics == {key: after['history'][-1][key] for key in metrics}
            jac = probes['jacobians'].double()
            allocation = torch.stack([s['allocation'] for s in steps]).sum(0)
            delta = math.flatten([after['model'][n] - before['model'][n] for n in names]).double()
            # Telescoping float32 differences need their own explicit rounding remainder.
            telescope = delta - torch.stack([s['delta'].double() for s in steps]).sum(0)
            assert telescope.abs().max() < 2e-6
            effects = jac @ allocation.T
            total_effect = jac @ delta
            summaries = []
            for i, group in enumerate(probes['groups']):
                indices = group['indices']; n = group['count']
                summaries.append({k: v for k, v in group.items() if k != 'indices'} | {
                    'actual_change': float((final_forces[indices] - probes['forces'][indices]).mean()) if n else None,
                    'linear_change': float(total_effect[i]) if n else None,
                    'allocated_change': effects[i].tolist() if n else None,
                    'gradient_descent': {key: (-jac[i] @ value.double().T).tolist() if n else None for key, value in norm['gradients'].items()}})
            r = {'update': update, 'exact_replay': True, 'metrics': metrics, 'normalization': norm['stats'],
                 'normalization_pairs': {key: pair_summary(value, masks) for key, value in norm['gradients'].items()},
                 'minibatch_pairs': [pair_summary(s['gradients'], masks) for s in steps],
                 'clip_factors': [s['clip_factor'] for s in steps], 'actor_only_clip_factors': [s['actor_only_clip_factor'] for s in steps],
                 'allocated_delta_norms': allocation.norm(dim=1).tolist(), 'delta_norm': float(delta.norm()),
                 'max_moment_error': max(s['moment_error'] for s in steps),
                 'max_step_rounding': max(float(s['allocation'][-1].abs().max()) for s in steps),
                 'max_telescope_rounding': float(telescope.abs().max()), 'probes': summaries}
            storage.dump_torch(out/f'update-{update:04}.pt.gz', {'parameter_names': names, 'parameter_masks': masks,
                'steps': steps, 'normalization': norm, 'probes': probes, 'final_forces': final_forces,
                'actual_delta': delta, 'telescope_rounding': telescope, 'allocation': allocation,
                'replay_model': model.state_dict(), 'replay_optimizer': optimizer.state_dict(), 'replay_rng': torch.get_rng_state()})
            records.append(r)
            print(f'{arm} seed {seed} update {update}: exact replay, {len(steps)} optimizer steps', flush=True)
            if time.monotonic() - started > 600: raise TimeoutError('gradient audit budget')
    atomic_json(out/'results.json', {'arm': arm, 'seed': seed, 'records': records, 'parity': parity,
        'seconds': time.monotonic()-started, 'saved_sampling_rows': 7680, 'protected_stopping_rows': 3840,
        'diagnostic_optimizer_steps': 120, 'training_ticks': 0, 'physics_ticks': 0})


def verify(a):
    started = time.monotonic(); records = []; maximum = 0.
    for arm in ARMS:
        for seed in SEEDS:
            out = a/f'audit/{arm}/seed-{seed}'; r = read(out/'results.json')
            assert r['seconds'] < 600 and len(r['records']) == 5
            for result, update in zip(r['records'], UPDATES):
                v = load(out/f'update-{update:04}.pt.gz')
                saved = load(a/f'inputs/{arm}/seed-{seed}/checkpoint-{update:04}.pt.gz')
                assert result['update'] == update and result['exact_replay']
                assert equal_tree(saved['model'], v['replay_model']) and equal_tree(saved['optimizer'], v['replay_optimizer'])
                assert torch.equal(saved['torch_rng'], v['replay_rng'])
                assert len(v['steps']) == 24 and v['probes']['jacobians'].shape == (30, 3493)
                for step in v['steps']:
                    torch.testing.assert_close(step['gradients'].sum(0), step['total_gradient'], atol=1e-6, rtol=1e-4)
                    torch.testing.assert_close(step['allocation'].sum(0), step['delta'].double(), atol=1e-12, rtol=1e-10)
                    assert not step['gradients'][3].count_nonzero()
                    assert not step['gradients'][5][v['parameter_masks']['actor']].count_nonzero()
                    assert step['actor_only_clip_factor'] + 1e-6 >= step['clip_factor']
                torch.testing.assert_close(v['allocation'].sum(0) + v['telescope_rounding'], v['actual_delta'], atol=1e-12, rtol=1e-10)
                jac = v['probes']['jacobians'].double(); predicted = jac @ v['actual_delta']
                for i, group in enumerate(v['probes']['groups']):
                    if group['count']:
                        assert abs(result['probes'][i]['linear_change'] - float(predicted[i])) < 1e-12
                        actual = float((v['final_forces'][group['indices']] - v['probes']['forces'][group['indices']]).mean())
                        assert result['probes'][i]['actual_change'] == actual
                        if group['command'] == 0: assert actual == 0 and not jac[i].count_nonzero()
                maximum = max(maximum, result['max_step_rounding'])
            records.append(r)
    result = {'experiment_id': a.name, 'verification_passed': True, 'records': records,
        'source_policy_runs': 6, 'selected_updates': UPDATES, 'exact_update_replays': 30,
        'diagnostic_optimizer_steps': 720, 'replayed_minibatch_rows': 184320,
        'saved_sampling_rows': 46080, 'protected_stopping_rows': 23040, 'frozen_parent_checkpoints': 60,
        'native_python_comparisons': sum(p['count'] for r in records for p in r['parity']),
        'max_step_rounding': maximum, 'training_ticks': 0, 'physics_ticks': 0,
        'new_training_checkpoints': 0, 'new_policy_exports': 0, 'confirmation_used': False,
        'model_promoted': False, 'navigation_qualified': False, 'seconds': time.monotonic()-started}
    assert result['native_python_comparisons'] == 7776 and result['seconds'] < 300
    atomic_json(a/'results.json', result)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(); parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--arm', choices=ARMS); parser.add_argument('--seed', type=int, choices=SEEDS)
    args = parser.parse_args(); torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
    if args.arm: run(args.archive, args.arm, args.seed)
    else: verify(args.archive)
