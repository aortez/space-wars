"""LF-070: error-region credit and exact private-copy update replay."""
import argparse
import copy
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.ppo import gae
from tools.flight_ppo.tests import equal_tree
from tools.flight_motor import aligned, sustained, storage
from tools.flight_motor.gae_audit import discounted_targets
from . import motor_feedback as feedback
from . import motor_gradient_math as math
from . import motor_error_math as region

SEEDS = [17, 29, 43]
UPDATES = [1, 16, 43, 64, 86]
read, load = storage.read_json, storage.load_torch


def replay(model, optimizer, rollout, c):
    names, params = math.parameters(model)
    account = math.AdamAccounting(optimizer, params, len(region.COMPONENTS))
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
            components = region.losses(surrogate, x) + [.5 * c['stopping_actor_weight'] * stop_loss,
                         entropy_loss, c['value_weight'] * value_loss]
            gradients = torch.stack([math.gradient(part, params) for part in components])
            optimizer.zero_grad(set_to_none=True); loss.backward()
            total_gradient = math.flatten([p.grad for p in params])
            torch.testing.assert_close(gradients.sum(0), total_gradient, atol=1e-6, rtol=1e-4)
            assert gradients[6].count_nonzero() == 0
            before = math.flatten(params).clone()
            norm = torch.nn.utils.clip_grad_norm_(model.parameters(), c['grad_clip'], error_if_nonfinite=True)
            clip = float((c['grad_clip'] / (norm + 1e-6)).clamp(max=1))
            optimizer.step()
            delta = math.flatten(params) - before
            allocation, moment_error = account.allocate(gradients, clip, delta)
            actor_norm = gradients[:8].sum(0).norm()
            traces.append({'epoch': epoch, 'indices': indices, 'gradients': gradients,
                'total_gradient': total_gradient, 'clip_factor': clip,
                'actor_only_clip_factor': float((c['grad_clip'] / (actor_norm + 1e-6)).clamp(max=1)),
                'delta': delta, 'allocation': allocation, 'moment_error': moment_error,
                'region_rows': [int(mask.sum()) for mask in region.regions(x)[1].values()]})
            logs.append({'policy_loss': float(policy.detach()), 'movement_policy_loss': float(move_loss.detach()),
                'stopping_policy_loss': float(stop_loss.detach()), 'value_loss': float(value_loss.detach()),
                'entropy_loss': float(entropy_loss.detach()), 'kl': float(kl), 'gradient_norm': float(norm),
                'clip_fraction': float(((ratio-1).abs() > c['clip']).float().mean().detach())})
    metrics = {'minibatches': len(logs), 'kl_early_stop': False, 'movement_rows': int(move.sum()),
        'stopping_rows': int((~move).sum()), 'movement_positive_advantages': int((adv[move] > 0).sum()),
        'stopping_positive_advantages': int((adv[~move] > 0).sum()),
        **{k: sum(row[k] for row in logs) / len(logs) for k in logs[0]}}
    return metrics, traces


def full_gradients(model, rollout):
    _, params = math.parameters(model)
    f = {k: v.flatten(0, 1) for k, v in rollout.items()}; x = f['observations']
    actor, _ = model(x)
    ratio = (model.log_prob(actor, f['latent'], f['actuator_latents'], x) - f['logp']).exp()
    variants = {'raw': f['advantages'], 'pooled': aligned.advantages(f['advantages'], x[:, 2] > 0)}
    return {name: torch.stack([math.gradient(loss, params) for loss in region.losses(ratio*adv, x)])
            for name, adv in variants.items()}


def pair_summary(vectors, masks):
    pairs = {'ready_under_over': (vectors[0], vectors[2]), 'unready_under_over': (vectors[3], vectors[5]),
             'all_under_over': (vectors[0]+vectors[3], vectors[2]+vectors[5]),
             'ready_unready': (vectors[:3].sum(0), vectors[3:6].sum(0))}
    return {kind: {name: math.cosine(left[mask], right[mask]) for name, (left, right) in pairs.items()}
            for kind, mask in masks.items()}


def probes(model, x, metadata, rollout):
    _, params = math.parameters(model)
    rows = rollout['observations'].flatten(0, 1); error, masks = region.regions(rows)
    ready = masks['ready_under'] | masks['ready_near'] | masks['ready_over']
    targets = feedback.alter(rows[ready], [16]*int(ready.sum()), [16]*int(ready.sum()))
    all_x = torch.cat((x, rows, targets)); raw, _ = model(all_x)
    forces = feedback.acceleration(all_x, raw[:, 1:].sigmoid()); groups = []
    for panel in ['anchors_mean', 'anchor_51']:
        for command in [0, 16]:
            for error_value in [-6, -1, 0, 1, 6]:
                mask = (metadata[:, 1] == command) & (metadata[:, 2] == error_value)
                if panel == 'anchor_51': mask &= metadata[:, 0] == 51
                groups.append(({'panel': panel, 'command': command, 'error': error_value}, mask.nonzero().flatten()))
    for name in region.REGIONS:
        groups.append(({'panel': 'saved_region', 'region': name, 'command': 16}, masks[name].nonzero().flatten()+len(x)))
    for side, mask in [('under', ready & (error >= -3) & (error < 0)), ('over', ready & (error > 0) & (error <= 3))]:
        groups.append(({'panel': 'saved_ready_near', 'side': side, 'command': 16}, mask.nonzero().flatten()+len(x)))
    groups.append(({'panel': 'saved_ready_target', 'command': 16}, torch.arange(len(targets))+len(x)+len(rows)))
    records, jacobians = [], []
    for label, indices in groups:
        value = forces[indices].mean() if len(indices) else forces.sum()*0
        jac = math.gradient(value, params)
        if label['command'] == 0: assert not jac.count_nonzero()
        records.append({**label, 'count': len(indices), 'indices': indices.tolist(),
                        'force': float(value.detach()) if len(indices) else None})
        jacobians.append(jac)
    assert len(records) == 29
    return {'inputs': all_x, 'groups': records, 'jacobians': torch.stack(jacobians),
            'logits': raw.detach(), 'forces': forces.detach()}


def verify_targets(model, before, after, rollout, evidence):
    assert torch.equal(evidence['observations'], rollout['observations'])
    torch.testing.assert_close(evidence['actions'], rollout['actuator_latents'].sigmoid(), atol=1e-6, rtol=1e-6)
    c = before['contract']['config']
    assert c['lambda'] == 1
    with torch.no_grad():
        values = torch.stack([model(x)[1] for x in rollout['observations']])
        last = model(torch.tensor([i['inputs'] for i in after['collector']['items']]))[1]
        adv, returns = gae(evidence['rewards'], evidence['dones'], values, last, c['gamma'], c['lambda'])
        expected = discounted_targets(evidence['rewards'], evidence['dones'], last, c['gamma'])
    assert torch.equal(adv, rollout['advantages']) and torch.equal(returns, rollout['returns'])
    torch.testing.assert_close(returns, expected, atol=1e-4, rtol=1e-5)
    return float((returns-expected).abs().max())


def run(a, seed):
    started = time.monotonic(); out = a/f'audit/seed-{seed}'; out.mkdir()
    source = a/f'inputs/speed16/seed-{seed}'
    x, metadata = feedback.grid(read(a/'anchors.json'), [0, 16], [-6, -1, 0, 1, 6])
    assert x.shape == (540, 18)
    first = load(source/'checkpoint-0000.pt.gz'); records, parity = [], []
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        for update in UPDATES:
            before = first if update == 1 else load(source/f'checkpoint-{update-1:04}.pt.gz')
            after = load(source/f'checkpoint-{update:04}.pt.gz'); rollout = load(source/f'rollout-{update:04}.pt.gz')
            c = before['contract']['config']
            with torch.random.fork_rng(devices=[]):
                model = sustained.ResidualModel(before['contract']['schema'])
                final = sustained.ResidualModel(before['contract']['schema'])
            model.load_state_dict(before['model']); final.load_state_dict(after['model'])
            names, params = math.parameters(model); pmasks = math.parameter_masks(names, params)
            assert sum(p.numel() for p in params) == 3493
            for state in [before, after]: sustained.assert_parent_actor(first['model'], state['model'])
            optimizer = torch.optim.Adam(model.parameters(), lr=c['learning_rate'], eps=1e-5)
            optimizer.load_state_dict(copy.deepcopy(before['optimizer']))
            evidence = region.reconstruct(source, before, after, c['rollout_ticks'])
            target_error = verify_targets(model, before, after, rollout, evidence)
            credit = region.credit(model, rollout, evidence); gradients = full_gradients(model, rollout)
            panel = probes(model, x, metadata, rollout)
            with torch.no_grad():
                final_raw = final(panel['inputs'])[0]
                final_force = feedback.acceleration(panel['inputs'], final_raw[:, 1:].sigmoid())
                stop = panel['inputs'][:, 2] == 0
                assert torch.equal(panel['logits'][stop], final_raw[stop])
                assert torch.equal(model.distribution(panel['logits'], panel['inputs']).scale[stop],
                                   final.distribution(final_raw, panel['inputs']).scale[stop])
            if update == 1: parity.append(feedback.native_parity(bridge, source/'policy-0000.json', model, x))
            if update == 86: parity.append(feedback.native_parity(bridge, source/'policy-0086.json', final, x))
            torch.set_rng_state(before['torch_rng'])
            with torch.no_grad():
                for t, rows in enumerate(rollout['observations']):
                    latent, z, logp, _ = model.sample(rows)
                    for actual, expected in [(latent, rollout['latent'][t]), (z, rollout['actuator_latents'][t]), (logp, rollout['logp'][t])]:
                        assert torch.equal(actual, expected)
            metrics, steps = replay(model, optimizer, rollout, c)
            assert len(steps) == 24 and equal_tree(model.state_dict(), after['model'])
            assert equal_tree(optimizer.state_dict(), after['optimizer']) and torch.equal(torch.get_rng_state(), after['torch_rng'])
            assert metrics == {k: after['history'][-1][k] for k in metrics}
            jac = panel['jacobians'].double(); allocation = torch.stack([s['allocation'] for s in steps]).sum(0)
            delta = math.flatten([after['model'][n]-before['model'][n] for n in names]).double()
            telescope = delta - torch.stack([s['delta'].double() for s in steps]).sum(0)
            assert telescope.abs().max() < 2e-6
            effects = jac @ allocation.T; linear = jac @ delta; summaries = []
            for i, group in enumerate(panel['groups']):
                ids, n = group['indices'], group['count']
                summaries.append({k: v for k, v in group.items() if k != 'indices'} | {
                    'actual_change': float((final_force[ids]-panel['forces'][ids]).mean()) if n else None,
                    'linear_change': float(linear[i]) if n else None,
                    'allocated_change': effects[i].tolist() if n else None,
                    'gradient_descent': {key: (-jac[i] @ value.double().T).tolist() if n else None for key, value in gradients.items()}})
            result = {'update': update, 'exact_replay': True, 'target_max_rounding_error': target_error,
                'metrics': metrics, 'credit': credit['stats'], 'full_rollout_pairs': {k: pair_summary(v, pmasks) for k, v in gradients.items()},
                'minibatch_pairs': [pair_summary(s['gradients'], pmasks) for s in steps],
                'clip_factors': [s['clip_factor'] for s in steps], 'actor_only_clip_factors': [s['actor_only_clip_factor'] for s in steps],
                'max_moment_error': max(s['moment_error'] for s in steps),
                'max_step_rounding': max(float(s['allocation'][-1].abs().max()) for s in steps),
                'max_telescope_rounding': float(telescope.abs().max()), 'probes': summaries}
            storage.dump_torch(out/f'update-{update:04}.pt.gz', {'parameter_names': names, 'parameter_masks': pmasks,
                'steps': steps, 'gradients': gradients, 'credit': credit, 'evidence': evidence,
                'probes': panel, 'final_forces': final_force, 'actual_delta': delta, 'allocation': allocation,
                'telescope_rounding': telescope, 'replay_model': model.state_dict(),
                'replay_optimizer': optimizer.state_dict(), 'replay_rng': torch.get_rng_state()})
            records.append(result)
            print(f'seed {seed} update {update}: exact targets and replay, 24 optimizer steps', flush=True)
            if time.monotonic()-started > 600: raise TimeoutError('error-gradient audit budget')
    atomic_json(out/'results.json', {'seed': seed, 'records': records, 'parity': parity, 'seconds': time.monotonic()-started})


def verify(a):
    started = time.monotonic(); records = []
    for seed in SEEDS:
        source = a/f'inputs/speed16/seed-{seed}'; out = a/f'audit/seed-{seed}'; r = read(out/'results.json')
        assert r['seconds'] < 600 and len(r['records']) == 5
        for result, update in zip(r['records'], UPDATES):
            v = load(out/f'update-{update:04}.pt.gz'); saved = load(source/f'checkpoint-{update:04}.pt.gz')
            before = load(source/f'checkpoint-{update-1:04}.pt.gz'); rollout = load(source/f'rollout-{update:04}.pt.gz')
            assert result['update'] == update and result['exact_replay']
            assert equal_tree(saved['model'], v['replay_model']) and equal_tree(saved['optimizer'], v['replay_optimizer'])
            assert torch.equal(saved['torch_rng'], v['replay_rng'])
            evidence = region.reconstruct(source, before, saved, 256)
            assert equal_tree(evidence, v['evidence']) and torch.equal(evidence['observations'], rollout['observations'])
            assert v['probes']['jacobians'].shape == (29, 3493) and len(v['steps']) == 24
            for step in v['steps']:
                torch.testing.assert_close(step['gradients'].sum(0), step['total_gradient'], atol=1e-6, rtol=1e-4)
                torch.testing.assert_close(step['allocation'].sum(0), step['delta'].double(), atol=1e-12, rtol=1e-10)
                assert not step['gradients'][6].count_nonzero()
                assert not step['gradients'][8][v['parameter_masks']['actor']].count_nonzero()
                assert step['actor_only_clip_factor']+1e-6 >= step['clip_factor']
                indices = step['indices']; x = rollout['observations'].flatten(0, 1)[indices]
                assert step['region_rows'] == [int(mask.sum()) for mask in region.regions(x)[1].values()]
            torch.testing.assert_close(v['allocation'].sum(0)+v['telescope_rounding'], v['actual_delta'], atol=1e-12, rtol=1e-10)
            jac = v['probes']['jacobians'].double(); linear = jac @ v['actual_delta']
            for i, group in enumerate(v['probes']['groups']):
                if not group['count']: continue
                actual = float((v['final_forces'][group['indices']]-v['probes']['forces'][group['indices']]).mean())
                assert result['probes'][i]['actual_change'] == actual
                assert abs(result['probes'][i]['linear_change']-float(linear[i])) < 1e-12
                if group['command'] == 0: assert actual == 0 and not jac[i].count_nonzero()
            assert sum(x['count'] for x in result['credit']) == 768
        records.append(r)
    result = {'experiment_id': a.name, 'verification_passed': True, 'records': records,
        'component_names': region.COMPONENTS, 'allocation_names': region.ALLOCATIONS,
        'source_policy_runs': 3, 'selected_updates': UPDATES, 'exact_update_replays': 15,
        'diagnostic_optimizer_steps': 360, 'replayed_minibatch_rows': 92160,
        'saved_sampling_rows': 23040, 'verified_target_rows': 23040, 'protected_stopping_rows': 11520,
        'zero_command_probe_rows': 4050, 'frozen_parent_checkpoints': 30,
        'native_python_comparisons': sum(p['count'] for r in records for p in r['parity']),
        'training_ticks': 0, 'physics_ticks': 0, 'new_training_checkpoints': 0, 'new_policy_exports': 0,
        'confirmation_used': False, 'model_promoted': False, 'navigation_qualified': False, 'seconds': time.monotonic()-started}
    assert result['native_python_comparisons'] == 3240 and result['seconds'] < 300
    atomic_json(a/'results.json', result)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(); parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--seed', type=int, choices=SEEDS); args = parser.parse_args()
    torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
    if args.seed: run(args.archive, args.seed)
    else: verify(args.archive)

