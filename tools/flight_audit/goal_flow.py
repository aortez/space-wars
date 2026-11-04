"""Frozen goal-path derivatives and first-minibatch gradients; no optimizer steps."""
import argparse
import copy
import gzip
import json
import math
from pathlib import Path
import random
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, identity, load
from tools.flight_ppo.model import tensor, VALUE_HEAD_ONLY
from tools.flight_ppo import exploration
from tools.flight_ppo.ppo import gae
from tools.flight_ppo.reward import reward
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.goal_response import read
from tools.flight_audit.hold_update import AuditBridge
from tools.flight_audit.signal import gradient_vector, gradient_summary, summary, heading_cos
from tools.flight_audit.recovery_response import trajectory

ARMS = ['schema', 'goal']
SEEDS = [17, 29, 43]
BEFORE = [0, 15]
FAMILIES = ['settled', 'position', 'velocity', 'spin']
DIRECTIONS = ['forward', 'left', 'backward', 'right']


def distribution(model, actor, previous, flat, indices, config):
    return exploration.distribution(model, actor, previous, flat['ar_previous_latents'][indices],
                                    flat['ar_previous_valid'][indices], config['exploration']['rho'],
                                    exploration.amplitude(config['exploration']))


def goal_mask(model):
    masks = []
    for name, p in model.named_parameters():
        m = torch.zeros_like(p, dtype=torch.bool)
        if name in ['query.weight', 'fusion1.weight']: m[:, :2] = True
        masks.append(m.flatten())
    return torch.cat(masks)


def cosine(a, b):
    denom = float(a.norm()*b.norm())
    return float(a.dot(b))/denom if denom > 1e-30 else None


def first_gradients(model, rollout, config, rng):
    flat = {k: v.flatten(0, 1) for k, v in rollout.items()}
    raw_adv = flat['advantages']; normalized = (raw_adv-raw_adv.mean())/raw_adv.std(unbiased=False).clamp_min(1e-8)
    parameters = list(model.parameters()); mask = goal_mask(model)
    with torch.random.fork_rng(devices=[]):
        torch.set_rng_state(rng)
        indices = torch.randperm(len(raw_adv))[:config['minibatch']]
        actor, value = model(flat['observations'][indices], value_gradient=VALUE_HEAD_ONLY)
        previous = model(flat['ar_previous_observations'][indices])[0]
        dist = distribution(model, actor, previous, flat, indices, config)
        logp = exploration.log_prob(model, dist, flat['ar_latents'][indices])
        entropy = exploration.entropy(model, dist).mean()
        ratio = (logp-flat['logp'][indices]).exp(); adv = normalized[indices]
        rows = -torch.minimum(ratio*adv, ratio.clamp(1-config['clip'], 1+config['clip'])*adv)
        policy = rows.mean()
        critic = config['value_weight']*.5*(value-flat['returns'][indices]).square().mean()
        entropy_loss = -config['entropy_weight']*entropy
        vectors = {name: gradient_vector(loss, parameters) for name, loss in
                   [('policy', policy), ('weighted_critic', critic), ('weighted_entropy', entropy_loss)]}
        families = {name: gradient_vector((rows*(indices % 4 == i)).mean(), parameters) for i, name in enumerate(FAMILIES)}
        torch.testing.assert_close(sum(families.values()), vectors['policy'], atol=2e-6, rtol=2e-5)
        direct = gradient_vector(policy+critic+entropy_loss, parameters, False)
        torch.testing.assert_close(sum(vectors.values()), direct, atol=2e-6, rtol=2e-5)
        assert torch.count_nonzero(vectors['weighted_critic'][mask]) == 0
        total = sum(vectors.values()); clip_scale = min(1., config['grad_clip']/(float(total.norm())+1e-6))
        stats = {'scope': 'Actual first minibatch only; family vectors retain the full minibatch denominator; raw gradients, not Adam steps.',
                 'minibatch_indices': indices.tolist(), 'all_parameters': gradient_summary(*vectors.values()),
                 'goal_columns': gradient_summary(*(v[mask] for v in vectors.values())),
                 'clipping_scale': clip_scale, 'clipped_goal_total_norm': float(total[mask].norm())*clip_scale,
                 'first_minibatch_kl': float(((ratio-1)-(logp-flat['logp'][indices])).mean().detach()),
                 'max_logp_error': float((logp.detach()-flat['logp'][indices]).abs().max()),
                 'losses': {'policy': float(policy.detach()), 'weighted_critic': float(critic.detach()), 'weighted_entropy': float(entropy_loss.detach())},
                 'families': {name: {'rows': int((indices % 4 == i).sum()), 'all_parameter_norm': float(v.norm()),
                                    'goal_column_norm': float(v[mask].norm()),
                                    'goal_cosine_with_total_policy': cosine(v[mask], vectors['policy'][mask])}
                              for i, (name, v) in enumerate(families.items())},
                 'position_vs_other_goal_cosine': cosine(families['position'][mask], sum(v for name, v in families.items() if name!='position')[mask])}
    with torch.no_grad(): means, values = model(flat['observations'])
    means = means.detach().requires_grad_()
    idx = torch.arange(len(means)); dist = distribution(model, means, means.detach(), flat, idx, config)
    lp = exploration.log_prob(model, dist, flat['ar_latents'])
    torch.testing.assert_close(lp.detach(), flat['logp'], atol=5e-6, rtol=5e-6)
    scores = torch.autograd.grad(lp.sum(), means)[0].detach()
    pressure = scores*normalized[:, None]
    saved = {'indices': indices, 'goal_column_mask': mask, 'parameter_names': [n for n, _ in model.named_parameters()],
             'components': vectors, 'policy_by_family': families, 'score_pressure': pressure, 'actor_means': means.detach()}
    return stats, saved, values.detach(), normalized, pressure


def layer_tape(model, observations):
    values = {}; hooks = []
    def before(name):
        def hook(module, args): values[name] = args[0]
        return hook
    def after(name):
        def hook(module, args, output): values[name] = output
        return hook
    hooks += [model.fusion1.register_forward_pre_hook(before('joined')), model.fusion1.register_forward_hook(after('z1'))]
    hooks += [model.fusion2.register_forward_pre_hook(before('h1')), model.fusion2.register_forward_hook(after('z2'))]
    hooks += [model.actor.register_forward_pre_hook(before('h2'))]
    try:
        actor, _ = model(observations)
        values['actor'] = actor
        values['controls'] = torch.cat((actor[:, :1].tanh(), actor[:, 1:].sigmoid()), dim=1)
    finally:
        for hook in hooks: hook.remove()
    return values


def tensor_stats(x):
    x = x.detach().flatten()
    return {'minimum': float(x.min()), 'median': float(x.median()), 'mean': float(x.mean()), 'maximum': float(x.max())}


def layer_flow(model, observations):
    """Derivatives are per physical goal unit; each observation is independent."""
    names = ['joined', 'z1', 'h1', 'z2', 'h2', 'actor', 'controls']
    tape = layer_tape(model, observations); derivatives = {k: [] for k in names}
    for axis in range(2):
        tangent = torch.zeros_like(observations); tangent[:, axis] = 1/300.
        _, directional = torch.autograd.functional.jvp(lambda x: layer_tape_values(model, x), observations, tangent)
        for k, d in zip(names, directional): derivatives[k].append(d.detach())
    derivatives = {k: torch.stack(v, dim=-1) for k, v in derivatives.items()}
    slope1 = 1-tape['h1'].detach().square(); slope2 = 1-tape['h2'].detach().square()
    decoded = tape['controls'].detach()
    output_slope = torch.cat((1-decoded[:, :1].square(), decoded[:, 1:]*(1-decoded[:, 1:])), dim=1)
    def linear(weight, d): return torch.einsum('oi,big->bog', weight.detach(), d)
    z1 = linear(model.fusion1.weight, derivatives['joined']); h1 = z1*slope1[:, :, None]
    z2 = linear(model.fusion2.weight, h1); h2 = z2*slope2[:, :, None]
    actor = linear(model.actor.weight, h2); controls = actor*output_slope[:, :, None]
    for name, predicted in [('z1', z1), ('h1', h1), ('z2', z2), ('h2', h2), ('actor', actor), ('controls', controls)]:
        torch.testing.assert_close(predicted, derivatives[name], atol=2e-7, rtol=2e-4)
    # Slope-one derivatives are algebraic comparisons, not changed forward policies.
    bypass = {}
    for name, one, two in [('fusion1_slope_one', True, False), ('fusion2_slope_one', False, True), ('both_slopes_one', True, True)]:
        d1 = z1 if one else h1
        d2 = linear(model.fusion2.weight, d1)
        if not two: d2 = d2*slope2[:, :, None]
        bypass[name] = linear(model.actor.weight, d2)*output_slope[:, :, None]
    metrics = {'rows': len(observations), 'units': 'Jacobian per one physical ship-local goal unit; norms cover both goal axes.',
               'tanh': {name: {'slope': tensor_stats(s), 'fraction_slope_at_most_0_01': float((s <= .01).float().mean()),
                               'max_abs_preactivation': float(tape[z].detach().abs().max())}
                        for name, s, z in [('fusion1', slope1, 'z1'), ('fusion2', slope2, 'z2')]},
               'goal_jacobian_norms': {name: tensor_stats(d.flatten(1).norm(dim=1)) for name, d in derivatives.items()},
               'control_goal_jacobian_norms': {name: tensor_stats(derivatives['controls'][:, i].norm(dim=1)) for i, name in enumerate(['steering','thrust','brake'])},
               'tanh_attenuation': {name: tensor_stats(after.flatten(1).norm(dim=1)/before.flatten(1).norm(dim=1).clamp_min(1e-30))
                                    for name, before, after in [('fusion1',z1,h1),('fusion2',z2,h2)]},
               'slope_one_control_norms': {name: tensor_stats(d.flatten(1).norm(dim=1)) for name, d in bypass.items()}}
    saved = {'layers': {k: v.detach() for k, v in tape.items()}, 'goal_jacobians': derivatives,
             'tanh_slopes': {'fusion1': slope1, 'fusion2': slope2}, 'slope_one_control_jacobians': bypass}
    return metrics, saved


def layer_tape_values(model, observations):
    tape = layer_tape(model, observations)
    return tuple(tape[k] for k in ['joined', 'z1', 'h1', 'z2', 'h2', 'actor', 'controls'])


def row_metrics(rows, rollout, expected, normalized, pressure, values, means):
    episodes = {e['id']: e for e in expected['collector']['completed']+expected['collector']['active']}
    output = []; groups = {}
    for i, r in enumerate(rows):
        candidate = episodes[r['episode']]['candidate']; index = candidate['index']; family = FAMILIES[index % 4]
        assert i % 4 == r['slot'] == index % 4
        before = r['before']; after = r['after']['frame']; alignment = heading_cos(after)-heading_cos(before)
        item = {'family': family, 'case_index': index, 'direction': DIRECTIONS[(index//4)%4] if family=='position' else None,
                'episode': r['episode'], 'tick': r['tick'], 'action': r['action'], 'actor_mean': means[i].tolist(),
                'before_errors': before['errors'], 'after_errors': after['errors'], 'outcome': r['after']['progress']['outcome'],
                'heading_cos_before': heading_cos(before), 'alignment_change': alignment,
                'distance_decrease': before['errors']['position']-after['errors']['position'],
                'raw_advantage': float(rollout['advantages'].flatten()[i]), 'normalized_advantage': float(normalized[i]),
                'value': float(values[i]), 'return': float(rollout['returns'].flatten()[i]), 'score_pressure': pressure[i].tolist()}
        output.append(item)
        keys = [family]
        if family=='position':
            keys += ['position_'+item['direction']]
            if alignment > 1e-6: keys += ['position_alignment_improves']
            elif alignment < -1e-6: keys += ['position_alignment_worsens']
            if item['distance_decrease'] > 1e-6: keys += ['position_closing']
            elif item['distance_decrease'] < -1e-6: keys += ['position_opening']
        for key in keys: groups.setdefault(key, []).append(item)
    summaries = {key: {'rows': len(items), 'raw_advantage': summary([v['raw_advantage'] for v in items]),
                       'normalized_advantage': summary([v['normalized_advantage'] for v in items]),
                       'steering': summary([v['action']['steering'] for v in items]),
                       'steering_score_pressure': summary([v['score_pressure'][0] for v in items]),
                       'sampled_successes': sum(v['outcome']=='success' for v in items)} for key, items in groups.items()}
    return output, summaries


def case(a, budget, remaining):
    arm, seed, before = budget['arm'], budget['seed'], budget['before']; after = before+1
    directory = a/f'inputs/{arm}/seed-{seed}'; out = a/f'{arm}/seed-{seed}/update-{after:04}'; out.mkdir(parents=True)
    start = time.monotonic(); expected = torch.load(directory/f'checkpoint-{after:04}.pt', weights_only=True); config = expected['contract']['config']
    with Bridge(a/'runtime/flight_ppo_bridge') as raw:
        bridge = AuditBridge(raw, limit=budget['restore_ticks']+1024, wall_seconds=min(180, remaining))
        model, optimizer, collector, completed, history = load(directory/f'checkpoint-{before:04}.pt', identity(config, raw), bridge)
        assert completed == before and config['value_gradient'] == VALUE_HEAD_ONLY and config['exploration']['rho'] == 0
        weights = copy.deepcopy(model.state_dict()); adam = copy.deepcopy(optimizer.state_dict()); old_history = copy.deepcopy(history)
        bridge.collector = collector; rollout = collector.collect(model, config, bridge.deadline)
        reconstructed = collector.state(); assert equal_tree(reconstructed, expected['collector']) and history == expected['history'][:-1]
        assert len(bridge.rows) == 1024 and bridge.ticks == budget['restore_ticks']+1024 and bridge.setup_ticks == budget['neutral_setup_ticks']
        rng = torch.get_rng_state().clone(); py_rng = random.getstate()
        rewards = torch.tensor([reward({'frame':r['before']}, r['after'], config['gamma'], config['velocity_shaping_weight'], config['deadline_distance_weight'], config['regulation_weight'])['total'] for r in bridge.rows]).reshape(256,4)
        dones = torch.tensor([r['after']['progress']['outcome'] is not None for r in bridge.rows], dtype=torch.float32).reshape(256,4)
        with torch.no_grad(): values = torch.stack([model(x)[1] for x in rollout['observations']]); last = model(tensor(collector.items))[1]
        advantages, returns = gae(rewards, dones, values, last, config['gamma'], config['lambda'])
        assert torch.equal(advantages, rollout['advantages']) and torch.equal(returns, rollout['returns'])
        torch.save(rollout, out/'rollout.pt'); torch.save(reconstructed, out/'collector.pt')
        torch.save({'rewards':rewards, 'dones':dones, 'values':values, 'final_value':last, 'advantages':advantages, 'returns':returns}, out/'targets.pt')
        torch.save({'torch_rng':rng, 'python_rng':py_rng}, out/'post-collection-rng.pt')
        gradients, vectors, batched_values, normalized, pressure = first_gradients(model, rollout, config, rng)
        assert gradients['first_minibatch_kl'] <= config['target_kl']
        torch.save(vectors, out/'gradient-vectors.pt')
        rows, groups = row_metrics(bridge.rows, rollout, expected, normalized, pressure, batched_values, vectors['actor_means'])
        with gzip.open(out/'rows.json.gz','wt') as f: json.dump(rows,f,allow_nan=False)
        flow, layers = layer_flow(model, rollout['observations'][:,1,:])
        torch.save(layers, out/'goal-path.pt')
        assert equal_tree(weights, model.state_dict()) and equal_tree(adam, optimizer.state_dict()) and history == old_history
        assert torch.equal(rng, torch.get_rng_state()) and py_rng == random.getstate()
        assert equal_tree(reconstructed, collector.state())
        result = {'arm':arm,'seed':seed,'before_update':before,'after_update':after,'budget':budget,
                  'gradients':gradients,'groups':groups,'goal_path':flow,'exact_collector':True,'exact_gae':True,
                  'model_optimizer_history_rng_unchanged':True,'action_ticks':bridge.ticks,'neutral_setup_ticks':bridge.setup_ticks,
                  'optimizer_steps':0,'seconds':time.monotonic()-start}
        assert result['seconds'] < 180
        atomic_json(out/'results.json',result); return result


def training_paths(a):
    records=[]
    for arm in ARMS:
        for seed in SEEDS:
            directory=a/f'inputs/{arm}/seed-{seed}';state=torch.load(directory/'checkpoint-0016.pt',weights_only=True)
            for e in state['collector']['completed']:
                if e['candidate']['index'] % 4 != 1: continue
                t=read(directory/f"training-traces/episode-{e['id']:06}.json")
                assert t['task']==e['candidate']['task']
                records.append({'arm':arm,'seed':seed,'episode':e['id'],'case_index':e['candidate']['index'],
                                'direction':DIRECTIONS[(e['candidate']['index']//4)%4],**trajectory(t)})
    return records


def main():
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);a=p.parse_args().archive
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True);start=time.monotonic();budgets=read(a/'replay-budget.json')
    assert [(b['arm'],b['seed'],b['before']) for b in budgets]==[(arm,s,u) for arm in ARMS for s in SEEDS for u in BEFORE]
    records=[]
    for b in budgets:
        remaining=1200-(time.monotonic()-start)
        if remaining<=0:raise TimeoutError('global goal-flow wall budget')
        records.append(case(a,b,remaining));atomic_json(a/'progress.json',{'cases':len(records),'action_ticks':sum(r['action_ticks'] for r in records)})
    paths=training_paths(a)
    result={'experiment_id':a.name,'cases':records,'completed_position_paths':paths,'replayed_rollout_ticks':12288,
            'restore_ticks':sum(b['restore_ticks'] for b in budgets),'action_ticks':sum(r['action_ticks'] for r in records),
            'neutral_setup_ticks':sum(r['neutral_setup_ticks'] for r in records),'optimizer_steps':0,
            'position_rows_for_layer_analysis':3072,'first_minibatches':12,'seconds':time.monotonic()-start,
            'new_training_experiment':False,'new_flight_evaluation':False,'confirmation_used':False,'model_promoted':False}
    assert result['action_ticks']==12288+sum(b['restore_ticks'] for b in budgets) and result['seconds']<1200
    atomic_json(a/'results.json',result)


if __name__=='__main__':main()
