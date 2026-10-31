"""LF-075: learning-signal pilots around the LF-074 positive control.

Grid: start in {warm (LF-074 weights), fresh} x recipe in {current, repeat, steep}.
The current recipe keeps the LF-069 settings on the plain format-1 actor: per-tick
60 Hz Gaussian exploration, 256 decisions per slot per update, six persistent
slots alternating speed-16 and stopping pools, gamma .99 per tick, lambda 1.
The repeat recipe holds each decision for six ticks and preserves the per-tick
horizon by accumulating rewards and discounts tick by tick. The steep recipe adds
an absolute velocity-error term so the cost keeps a constant slope inside the
tolerance band. Warm starts first fit the critic with the actor frozen and keep
steering at its learned mean, as LF-069 did; fresh starts sample steering too,
as LF-053 did, since deterministic steering cannot be learned from nothing. The
PPO update and likelihoods are the shared tools.flight_ppo.ppo implementation.
"""
import argparse
import copy
import json
import math
from pathlib import Path
import random
import time

import torch
from torch.nn import functional as F
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from tools.flight_ppo.model import actions
from tools.flight_ppo.ppo import update
from . import aligned, single_speed, teacher, velocity
from .model import MotorModel, tensor

KIND = 'motor-learning-signal-pilot-v1'
STARTS = ['warm', 'fresh']
RECIPES = ['current', 'repeat', 'steep']
SEEDS = [17, 29, 43]
WARM_CHECKPOINT = 'data/training-experiments/learned-flight-motor-teacher-fit-v1-20261025/checkpoint-final.pt'


CURRICULUM_SPEEDS = [4., 8., 12., 16.]


INITIALIZATIONS = {'standard': None, 'coast': {'thrust_logit_bias': -2., 'brake_logit_bias': -2.}}


MATRIX_TASK_SEED = 5510001  # The LF-055 training matrix seed; its development seed is 5510002.
POOLS = ['speed16', 'matrix']


def config(seed, start, recipe, updates=84, stage_updates=None, init='standard', steering_anchor=0., pools='speed16', extra_sets=False):
    """One grid cell; stage_updates adds a command-speed curriculum of four equal stages ending at 16.

    The coast initialization replaces the standard low-thrust prior (thrust bias -4,
    brake near .5) with thrust and brake logits at -2, so a fresh policy cruises near
    eleven units per second and produces target-band experience from the start.
    """
    if seed not in SEEDS or start not in STARTS or recipe not in RECIPES or init not in INITIALIZATIONS or pools not in POOLS:
        raise ValueError('undeclared pilot cell')
    curriculum = [[stage_updates, speed] for speed in CURRICULUM_SPEEDS] if stage_updates else None
    if curriculum:
        updates = stage_updates * len(CURRICULUM_SPEEDS)
    return {'kind': KIND, 'learner_seed': seed, 'start': start, 'recipe': recipe, 'updates': updates, 'curriculum': curriculum,
            'initialization': init, 'steering_anchor': steering_anchor, 'pools': pools, 'extra_sets': extra_sets,
            'decisions_per_slot': 256, 'hold': 6 if recipe == 'repeat' else 1, 'reward': 'steep' if recipe == 'steep' else 'current',
            'critic_warmup_updates': 8 if start == 'warm' else 0,
            'steering_sampler': 'deterministic' if start == 'warm' else 'gaussian',
            'gamma': .99, 'lambda': 1., 'epochs': 4, 'minibatch': 256, 'learning_rate': .0003, 'clip': .2,
            'value_weight': .5, 'entropy_weight': .001, 'grad_clip': .5, 'target_kl': .03}


def stage_speed(c, update_index):
    """Command speed of the curriculum stage containing this zero-based update, or None without a curriculum."""
    if not c.get('curriculum'):
        return None
    done = 0
    for count, speed in c['curriculum']:
        done += count
        if update_index < done:
            return speed
    return c['curriculum'][-1][1]


def retarget(pools, original_pools, speed):
    """Point every movement task at a command speed, scaling its start velocity from the speed-16 matrix in place."""
    for pool, original in zip(pools, original_pools):
        rows = []
        for row in original:
            row = copy.deepcopy(row)
            if row['family'] == 'track':
                factor = speed / row['task']['speed']
                row['task']['speed'] = speed
                row['task']['start']['relative_velocity'] = {k: v * factor for k, v in row['task']['start']['relative_velocity'].items()}
                row['level'] = speed
            rows.append(row)
        pool[:] = rows


def matrix_pools(pools, seed=MATRIX_TASK_SEED):
    """Replace the speed-16 movement pools with the LF-055 training matrix, commands 4/10/16 in eight start modes, retained to 900 ticks."""
    rows = [copy.deepcopy(r) for r in velocity.candidates(seed) if r['family'] == 'track']
    for row in rows:
        row['task']['retention_horizon_ticks'] = 900
    movement = [i for i, pool in enumerate(pools) if pool and pool[0]['family'] == 'track']
    for j, i in enumerate(movement):
        pools[i] = rows[j::len(movement)]
    return pools


def reward_total(item, kind):
    """Per-tick reward; the steep variant adds |velocity error| / 5 to the LF-069 cost."""
    terms = velocity.reward(item)
    if kind == 'current':
        return terms['total']
    if kind != 'steep':
        raise ValueError('unknown pilot reward')
    return terms['total'] - abs(item['errors']['velocity']) / 5 / 60


def sample(model, x, steering):
    """Sampled actuator latents; steering is sampled or held at its learned mean."""
    if steering == 'gaussian':
        return model.sample(x)
    if steering != 'deterministic':
        raise ValueError('unknown steering sampler')
    actor, value = model(x)
    _, actuators = model.distributions(actor)
    z = actuators.sample()
    return actor[:, 0], z, (actuators.log_prob(z) - model.actuator_log_jacobian(z)).sum(-1), value


def steering_anchor(model, actor, flat, indices, c):
    """Supervised leash: keep the mean steering on the teacher law's steering for the visited states.

    Deterministic steering gets no likelihood gradient in PPO, so without this
    term it drifts through the shared layers as a side effect of actuator updates.
    """
    target = teacher.teacher(flat['observations'][indices])[:, 0]
    return c['steering_anchor'] * F.mse_loss(torch.tanh(actor[:, 0]), target)


def actuator_terms(model, actor, flat, indices, c):
    """Likelihood and entropy over the sampled actuators only, for deterministic steering."""
    _, actuators = model.distributions(actor)
    z = flat['actuator_latents'][indices]
    logp = (actuators.log_prob(z) - model.actuator_log_jacobian(z)).sum(-1)
    entropy = (actuators.entropy() + model.actuator_log_jacobian(actuators.rsample())).sum(-1).mean()
    return logp, entropy


def probe(model, command=16., speeds=None):
    """Fixed-state net acceleration of the mean policy at the previous-action fixed point.

    Speeds default to one unit either side of the command; the result keys keep
    their original names, so for command 16 they read exactly as before.
    """
    speeds = (command - 1., command, command + 1.) if speeds is None else speeds
    forces = []
    for speed in speeds:
        previous = torch.zeros(3)
        for _ in range(30):
            x = torch.tensor([[0., 1., command / 20., 0., speed / 20., 0., speed / 20., 0., 0., 0., 1., 1., 1., 1., 1., *previous.tolist()]])
            with torch.no_grad():
                a = model(x)[0][0]
            previous = torch.stack((a[0].tanh(), a[1].sigmoid(), a[2].sigmoid()))
        forces.append(45. * previous[1].item() - min(4. * speed, 40.) * previous[2].item())
    return {'slope_at_16': (forces[-1] - forces[0]) / (speeds[-1] - speeds[0]), 'bias_at_16': forces[len(speeds) // 2]}


class PilotCollector:
    """Six persistent slots; each slot decides whenever its held action expires or its episode resets."""

    def __init__(self, bridge, pools, reward_kind, hold, gamma, steering='gaussian'):
        self.bridge = bridge; self.pools = pools; self.cursors = [0] * len(pools)
        self.reward_kind = reward_kind; self.hold = hold; self.gamma = gamma; self.steering = steering
        self.active = [self.next_task(i) for i in range(len(pools))]
        self.items = bridge.call('reset', tasks=[t['task'] for t in self.active])['items']
        self.counters = [0] * len(pools)
        self.outcomes = {}; self.episodes = 0; self.ticks = 0; self.physical_failures = 0

    def next_task(self, slot):
        pool = self.pools[slot]; task = pool[self.cursors[slot] % len(pool)]; self.cursors[slot] += 1
        return task

    def record(self, slot, outcome):
        key = '%s/%s' % (self.active[slot]['family'], outcome)
        self.outcomes[key] = self.outcomes.get(key, 0) + 1; self.episodes += 1
        self.physical_failures += outcome not in ['success', 'deadline']

    def collect(self, model, ticks, lam):
        n = len(self.pools)
        keys = ['obs', 'latent', 'actuator', 'logp', 'value', 'reward', 'done', 'gamma']
        sequences = [{k: [] for k in keys} for _ in range(n)]
        held = [None] * n; open_index = [None] * n
        # A hold left open by the previous window was already bootstrapped at its
        # end, so every slot starts this window with a fresh decision.
        self.counters = [0] * n
        for _ in range(ticks):
            deciding = [i for i in range(n) if self.counters[i] == 0]
            if deciding:
                x = tensor([self.items[i] for i in deciding])
                with torch.no_grad():
                    latent, actuator, logp, value = sample(model, x, self.steering)
                chosen = actions(latent, actuator)
                for j, i in enumerate(deciding):
                    s = sequences[i]
                    s['obs'].append(x[j]); s['latent'].append(latent[j]); s['actuator'].append(actuator[j])
                    s['logp'].append(logp[j]); s['value'].append(value[j])
                    s['reward'].append(0.); s['done'].append(0.); s['gamma'].append(1.)
                    held[i] = chosen[j]; open_index[i] = len(s['reward']) - 1; self.counters[i] = self.hold
            after = self.bridge.call('step', actions=held)['items']; self.ticks += n
            resets = []
            for i in range(n):
                s = sequences[i]; k = open_index[i]; elapsed = self.hold - self.counters[i]
                s['reward'][k] += self.gamma ** elapsed * reward_total(after[i], self.reward_kind)
                s['gamma'][k] = self.gamma ** (elapsed + 1); self.counters[i] -= 1
                outcome = after[i]['progress']['outcome']
                if outcome is not None:
                    s['done'][k] = 1.; self.record(i, outcome); resets.append(i); self.counters[i] = 0
            self.items = after
            for i in resets:
                self.active[i] = self.next_task(i)
                self.items = self.bridge.call('reset_at', index=i, task=self.active[i]['task'])['items']
        with torch.no_grad():
            final_values = model(tensor(self.items))[1]
        rollout = {k: [] for k in ['observations', 'latent', 'actuator_latents', 'logp', 'advantages', 'returns']}
        for i, s in enumerate(sequences):
            count = len(s['reward'])
            if count == 0:
                continue
            values = torch.stack(s['value']); advantages = torch.zeros(count); running = 0.
            for k in reversed(range(count)):
                next_value = final_values[i] if k == count - 1 else values[k + 1]
                live = 1. - s['done'][k]
                delta = s['reward'][k] + s['gamma'][k] * live * next_value - values[k]
                running = delta + s['gamma'][k] * lam * live * running
                advantages[k] = running
            rollout['observations'].append(torch.stack(s['obs'])); rollout['latent'].append(torch.stack(s['latent']))
            rollout['actuator_latents'].append(torch.stack(s['actuator'])); rollout['logp'].append(torch.stack(s['logp']))
            rollout['advantages'].append(advantages); rollout['returns'].append(advantages + values)
        return {k: torch.cat(v)[:, None] if torch.cat(v).ndim == 1 else torch.cat(v)[:, None, :] for k, v in rollout.items()}


def value_update(model, optimizer, rollout, c):
    flat = {k: v.flatten(0, 1) for k, v in rollout.items()}
    losses = []
    for _ in range(c['epochs']):
        for index in torch.randperm(len(flat['returns'])).split(c['minibatch']):
            value = model(flat['observations'][index])[1]
            loss = .5 * (value - flat['returns'][index]).square().mean()
            optimizer.zero_grad(set_to_none=True); loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), c['grad_clip']); optimizer.step(); losses.append(loss.item())
    return sum(losses) / len(losses)


def make_model(schema, c, warm_checkpoint):
    model = MotorModel(schema)
    if c['start'] == 'warm':
        model.load_state_dict(torch.load(warm_checkpoint, weights_only=True)['state'])
    elif INITIALIZATIONS[c.get('initialization', 'standard')]:
        biases = INITIALIZATIONS[c['initialization']]
        with torch.no_grad():
            model.actor.bias[1] = biases['thrust_logit_bias']; model.actor.bias[2] = biases['brake_logit_bias']
    return model


def run(c, out, bridge_path, warm_checkpoint, log=print):
    torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
    torch.manual_seed(c['learner_seed']); random.seed(c['learner_seed'])
    out.mkdir(parents=True, exist_ok=False); started = time.monotonic()
    atomic_json(out / 'config.json', {**c, 'bridge_sha256': sha(bridge_path), 'warm_checkpoint_sha256': sha(warm_checkpoint) if c['start'] == 'warm' else None})
    pools = single_speed.collector_pools(single_speed.config(c['learner_seed'], 'speed16'))
    if c.get('pools') == 'matrix':
        pools = matrix_pools(pools)
    development = single_speed.candidates(teacher.DEVELOPMENT_TASK_SEED)
    history = []
    with Bridge(bridge_path) as bridge:
        model = make_model(bridge.contract['schema'], c, warm_checkpoint)
        export = out / 'policy-initial.json'; model.export(export); bridge.call('load_policy', path=str(export.resolve()))
        initial = teacher.summarize(teacher.evaluate(bridge, model, development, out / 'evaluation-initial', c['hold']))
        initial_probe = probe(model)
        log('initial: %d/104 passed, probe %s' % (initial['passed'], initial_probe))
        collector = PilotCollector(bridge, pools, c['reward'], c['hold'], c['gamma'], c['steering_sampler'])
        terms = actuator_terms if c['steering_sampler'] == 'deterministic' else None
        auxiliary = steering_anchor if c.get('steering_anchor', 0.) > 0. else None
        ticks = c['decisions_per_slot'] * c['hold']
        if c['critic_warmup_updates']:
            critic = torch.optim.Adam([p for name, p in model.named_parameters() if name.startswith('critic')], lr=c['learning_rate'], eps=1e-5)
            for i in range(c['critic_warmup_updates']):
                rollout = collector.collect(model, ticks, c['lambda'])
                loss = value_update(model, critic, rollout, c)
                history.append({'phase': 'critic-warmup', 'update': i + 1, 'value_loss': loss, 'rows': len(rollout['returns']), **probe(model)})
                log('critic warm-up %d/%d value loss %.4f' % (i + 1, c['critic_warmup_updates'], loss))
        optimizer = torch.optim.Adam(model.parameters(), lr=c['learning_rate'], eps=1e-5)
        original_pools = copy.deepcopy(collector.pools); current_speed = None
        for i in range(c['updates']):
            speed = stage_speed(c, i)
            if speed is not None and speed != current_speed:
                retarget(collector.pools, original_pools, speed); current_speed = speed
                log('curriculum stage: movement command speed %g from update %d' % (speed, i + 1))
            rollout = collector.collect(model, ticks, c['lambda'])
            try:
                metrics = update(model, optimizer, rollout, c, policy_terms=terms, auxiliary=auxiliary)
            except RuntimeError as error:
                if 'no PPO optimizer step' not in str(error):
                    raise
                metrics = {'minibatches': 0, 'kl_early_stop': True}
            metrics.update(phase='ppo', update=i + 1, rows=len(rollout['returns']), ticks=collector.ticks, episodes=collector.episodes,
                           physical_failures=collector.physical_failures, outcomes=dict(collector.outcomes), **probe(model))
            if current_speed is not None:
                stage = probe(model, command=current_speed)
                metrics.update(stage_speed=current_speed, slope_at_stage=stage['slope_at_16'], bias_at_stage=stage['bias_at_16'])
            history.append(metrics)
            if (i + 1) % 12 == 0 or i + 1 == c['updates']:
                log('update %d/%d slope %+.3f bias %+.3f kl %.4f episodes %d failures %d' % (
                    i + 1, c['updates'], metrics['slope_at_16'], metrics['bias_at_16'], metrics.get('kl', float('nan')), collector.episodes, collector.physical_failures))
            atomic_json(out / 'history.json', history)
        export = out / 'policy-final.json'; model.export(export); bridge.call('load_policy', path=str(export.resolve()))
        torch.save({'kind': KIND, 'config': c, 'state': model.state_dict()}, out / 'checkpoint-final.pt')
        final = teacher.summarize(teacher.evaluate(bridge, model, development, out / 'evaluation-final', c['hold']))
        extra = bool(c.get('curriculum')) or bool(c.get('extra_sets'))
        final_aligned = (teacher.summarize(teacher.evaluate(bridge, model, aligned.candidates(teacher.DEVELOPMENT_TASK_SEED, True), out / 'evaluation-final-aligned', c['hold']))
                         if extra else None)
        final_lateral = (teacher.summarize(teacher.evaluate(bridge, model, velocity.candidates(teacher.DEVELOPMENT_TASK_SEED, True), out / 'evaluation-final-lateral', c['hold']))
                         if c.get('extra_sets') else None)
    summary = {'config': c, 'initial': initial, 'final': final, 'final_aligned': final_aligned, 'final_lateral': final_lateral, 'initial_probe': initial_probe, 'final_probe': probe(model),
               'training': {'episodes': collector.episodes, 'ticks': collector.ticks, 'physical_failures': collector.physical_failures,
                            'outcomes': collector.outcomes, 'kl_early_stops': sum(1 for h in history if h.get('kl_early_stop'))},
               'seconds': time.monotonic() - started}
    atomic_json(out / 'summary.json', summary)
    log('final: %d/104 passed, tracking %s, probe %s, %.0f s' % (final['passed'], tracking(final), summary['final_probe'], summary['seconds']))
    return summary


def tracking(summary):
    groups = [g for key, g in summary['groups'].items() if key.startswith('track/')]
    return '%d/%d' % (sum(g['passed'] for g in groups), sum(g['cases'] for g in groups))


def summarize_grid(root):
    rows = []
    for path in sorted(Path(root).glob('*/summary.json')):
        s = json.loads(path.read_text()); c = s['config']
        rows.append({'start': c['start'], 'recipe': c['recipe'], 'seed': c['learner_seed'], 'initial_passed': s['initial']['passed'],
                     'final_passed': s['final']['passed'], 'final_tracking': tracking(s['final']), 'initial_slope': s['initial_probe']['slope_at_16'],
                     'final_slope': s['final_probe']['slope_at_16'], 'initial_bias': s['initial_probe']['bias_at_16'], 'final_bias': s['final_probe']['bias_at_16'],
                     'track_outcomes': {k.split('/')[1]: v for k, v in s['training']['outcomes'].items() if k.startswith('track/')},
                     'physical_failures': s['training']['physical_failures'], 'kl_early_stops': s['training']['kl_early_stops'], 'seconds': round(s['seconds']),
                     'final_aligned': s['final_aligned']['passed'] if s.get('final_aligned') else None, 'final_lateral': s['final_lateral']['passed'] if s.get('final_lateral') else None})
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, required=True); p.add_argument('--seed', type=int); p.add_argument('--start', choices=STARTS)
    p.add_argument('--recipe', choices=RECIPES); p.add_argument('--updates', type=int, default=84)
    p.add_argument('--stage-updates', type=int, help='command-speed curriculum 4/8/12/16 with this many updates per stage')
    p.add_argument('--init', choices=list(INITIALIZATIONS), default='standard')
    p.add_argument('--steering-anchor', type=float, default=0., help='weight of the supervised steering leash toward the teacher law')
    p.add_argument('--pools', choices=POOLS, default='speed16'); p.add_argument('--extra-sets', action='store_true', help='also evaluate the aligned and lateral sets at the end')
    p.add_argument('--bridge', default=teacher.DEFAULT_BRIDGE); p.add_argument('--warm-checkpoint', default=WARM_CHECKPOINT)
    p.add_argument('--summarize', action='store_true')
    args = p.parse_args()
    if args.summarize:
        rows = summarize_grid(args.out); atomic_json(args.out / 'grid-summary.json', rows)
        for r in rows:
            print('%-5s %-7s seed %2d  passed %3d -> %3d  tracking %-5s  aligned %s  lateral %s  slope %+.3f -> %+.3f  bias %+.3f -> %+.3f  track %s  failures %d  kl stops %d  %ds' % (
                r['start'], r['recipe'], r['seed'], r['initial_passed'], r['final_passed'], r['final_tracking'], r['final_aligned'], r['final_lateral'], r['initial_slope'], r['final_slope'],
                r['initial_bias'], r['final_bias'], r['track_outcomes'], r['physical_failures'], r['kl_early_stops'], r['seconds']))
        return
    run(config(args.seed, args.start, args.recipe, args.updates, args.stage_updates, args.init, args.steering_anchor, args.pools, args.extra_sets),
        args.out, args.bridge, args.warm_checkpoint)


if __name__ == '__main__':
    main()
