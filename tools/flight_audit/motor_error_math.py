"""LF-070 error-region partitions, saved rewards and local actuator credit."""
import torch
from tools.flight_motor import aligned, storage
from tools.flight_motor.velocity import reward
from . import motor_feedback as feedback

REGIONS = ['ready_under', 'ready_near', 'ready_over', 'unready_under', 'unready_near', 'unready_over']
COMPONENTS = REGIONS + ['stopping', 'entropy', 'value']
ALLOCATIONS = COMPONENTS + ['carried_momentum', 'rounding_residual']


def regions(x):
    error, known = feedback.training_groups(x)
    ready = known['ready_under'] | known['ready_near'] | known['ready_over']
    parts = {}
    for prefix, mask in [('ready', ready), ('unready', known['movement'] & ~ready)]:
        for side, selected in [('under', error < -1), ('near', error.abs() <= 1), ('over', error > 1)]:
            parts[f'{prefix}_{side}'] = mask & selected
    assert torch.equal(torch.stack(list(parts.values())).sum(0), known['movement'].long())
    return error, parts


def losses(surrogate, x):
    _, groups = regions(x); count = (x[:, 2] > 0).sum().clamp_min(1)
    return [-.5 * surrogate[groups[name]].sum() / count for name in REGIONS]


def reconstruct(source, before, after, horizon):
    """Recover exactly the selected interval, including completed/reset episodes."""
    b, a = before['collector'], after['collector']
    assert a['completed'][:len(b['completed'])] == b['completed']
    completed = a['completed'][len(b['completed']):]
    columns = []
    for slot, pool in enumerate(b['pools']):
        indices = {row['index'] for row in pool}
        episodes = [(e, storage.read_json(source/f"training-traces/episode-{e['id']:06}.json.gz"))
                    for e in completed if e['candidate']['index'] in indices]
        episodes.append((a['active'][slot], a['traces'][slot]))
        assert episodes[0][0]['id'] == b['active'][slot]['id']
        rows = []
        for episode, trace in episodes:
            lo = b['active'][slot]['ticks'] if episode['id'] == b['active'][slot]['id'] else 0
            assert len(trace['steps']) == episode['ticks']
            for t in range(lo, len(trace['steps'])):
                previous = trace['initial'] if t == 0 else trace['steps'][t-1]['item']
                step = trace['steps'][t]; item = step['item']; terms = reward(item)
                rows.append({'observations': previous['inputs'], 'rewards': terms['total'],
                    'tracking_rewards': terms['tracking'], 'terminal_rewards': terms['terminal'],
                    'dones': float(item['progress']['outcome'] is not None),
                    'elapsed_ticks': item['progress']['elapsed_ticks'], 'candidate': episode['candidate']['index'],
                    'velocity_error_before': previous['errors']['velocity'], 'velocity_error_after': item['errors']['velocity'],
                    'actions': [step['action']['thrust'], step['action']['brake']]})
        assert len(rows) == horizon, (slot, len(rows), horizon)
        columns.append(rows)
    return {key: torch.tensor([[columns[s][t][key] for s in range(len(columns))] for t in range(horizon)], dtype=torch.float32)
            for key in columns[0][0]}


def credit(model, rollout, evidence):
    f = {k: v.flatten(0, 1) for k, v in rollout.items()}; x = f['observations']
    actor, values = model(x); actions = actor[:, 1:].sigmoid()
    mean_force = feedback.acceleration(x, actions)
    force_action_derivative = torch.autograd.grad(mean_force.sum(), actions, retain_graph=True)[0]
    force_logit_derivative = force_action_derivative * actions * (1-actions)
    dist = model.distribution(actor, x)
    score = (f['actuator_latents'] - actor[:, 1:]) / dist.scale.square()
    local = (force_logit_derivative * score).sum(1)
    adv = aligned.advantages(f['advantages'], x[:, 2] > 0)
    error, groups = regions(x)
    correction_sign = -error.sign()
    sample_force = feedback.acceleration(x, f['actuator_latents'].sigmoid())
    fields = {'error': error, 'raw_advantage': f['advantages'], 'advantage': adv, 'value': values,
        'return': f['returns'], 'mean_force': mean_force, 'sample_force': sample_force,
        'sample_corrective_delta': correction_sign*(sample_force-mean_force),
        # Independent per-row logit ascent; no network sharing or optimizer scaling.
        'raw_independent_logit_correction': correction_sign*local*f['advantages'],
        'independent_logit_correction': correction_sign*local*adv,
        'throttle_score': adv*score[:, 0], 'brake_score': adv*score[:, 1],
        **{k: evidence[k].flatten(0, 1) for k in ['rewards', 'tracking_rewards', 'terminal_rewards', 'elapsed_ticks',
            'velocity_error_before', 'velocity_error_after', 'dones']}}
    stats = []
    for name, mask in groups.items():
        n = int(mask.sum()); more = mask & (fields['sample_corrective_delta'] > 0)
        less = mask & (fields['sample_corrective_delta'] < 0)
        stats.append({'region': name, 'count': n,
            'sums': {k: float(v[mask].detach().double().sum()) for k, v in fields.items()},
            'positive_advantages': int((adv[mask] > 0).sum()),
            'positive_local_corrections': int((fields['independent_logit_correction'][mask] > 0).sum()),
            'more_corrective_samples': int(more.sum()), 'less_corrective_samples': int(less.sum()),
            'more_corrective_advantage_sum': float(adv[more].double().sum()),
            'less_corrective_advantage_sum': float(adv[less].double().sum())})
    return {'fields': {k: v.detach() for k, v in fields.items()}, 'stats': stats, 'masks': groups}
