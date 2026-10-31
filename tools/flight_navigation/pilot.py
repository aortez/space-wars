"""LF-084: navigation polishing pilots at the command rate from the LF-083 fitted export.

PPO over six-tick commands with the per-tick horizon preserved by per-decision
discounting, a critic warm-up before any actor update, bounded potential
shaping on goal distance that is indifferent to the route taken, physical
terminal terms, and an optional supervised leash toward the reference law on
a sample of visited states. The motor network and the composed bridge are
those of LF-083. The navigation network issues every command: sampled during
collection, at its mean during evaluation, with no mask, override or fallback.
"""
import argparse
import json
import math
from pathlib import Path
import random
import time

import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from tools.flight_ppo.ppo import update
from . import fit, reference
from .model import NavigationModel, command_dicts, tensor

KIND = 'navigation-polish-pilot-v1'
SEEDS = [17, 29, 43]
SLOTS = 16
TRAINING_SEED = 7720001
WARM_CHECKPOINT = 'data/training-experiments/learned-flight-navigation-fit-v1-20261026/checkpoint-final.pt'
TERMINALS = {'success': 10., 'deadline': -2., 'collision': -10., 'damage': -10., 'ship_lost': -10.,
             'boundary_escape': -10., 'controller_error': -10., 'hold_broken': -10.}


HEADING_SAMPLERS = ('deterministic', 'gaussian')
ANCHOR_TERMS = ('heading', 'command')


def config(seed, anchor=0., updates=120, exploration_log_std=-1.2, learning_rate=1e-4, gamma=.998, decisions_per_slot=96, label_rate=.25,
           heading_sampler='deterministic', anchor_terms='heading'):
    """One pilot cell.

    The anchor weight leashes the mean command to the law on a labelled sample
    of visited states: its heading pair only, leaving the speed to PPO, or the
    whole command. The gaussian heading sampler explores the pair with a
    speed-scaled deviation; the deterministic sampler keeps the heading at its
    mean and explores the speed only.
    """
    if seed not in SEEDS or anchor < 0. or updates <= 0 or heading_sampler not in HEADING_SAMPLERS or anchor_terms not in ANCHOR_TERMS:
        raise ValueError('undeclared pilot cell')
    return {'kind': KIND, 'learner_seed': seed, 'anchor': anchor, 'anchor_terms': anchor_terms, 'heading_sampler': heading_sampler,
            'updates': updates, 'hold': fit.HOLD, 'slots': SLOTS,
            'decisions_per_slot': decisions_per_slot, 'exploration_log_std': exploration_log_std,
            'label_rate': label_rate if anchor > 0. else 0., 'critic_warmup_updates': 8, 'training_seed': TRAINING_SEED,
            'gamma': gamma, 'lambda': .95, 'epochs': 4, 'minibatch': 256, 'learning_rate': learning_rate, 'clip': .2,
            'value_weight': .5, 'entropy_weight': .001, 'grad_clip': .5, 'target_kl': .03,
            'reward': {'time_per_tick': -.001, 'potential_scale': 60., 'terminals': TERMINALS}}


def potential(item, scale):
    """Bounded goal-distance potential; zero at a terminal, as the C3 shaping treats terminals."""
    return -math.tanh(item['frame']['errors']['position'] / scale)


def decision_reward(before, after, ticks, gamma, spec):
    """Reward of one held command: the per-tick time cost discounted tick by tick, the terminal at the last tick, and potential shaping across the hold."""
    outcome = after['progress']['outcome']
    if outcome == 'environment_error':
        raise RuntimeError('environment error is not a learning terminal')
    time_cost = spec['time_per_tick'] * sum(gamma ** t for t in range(ticks))
    terminal = spec['terminals'][outcome] * gamma ** (ticks - 1) if outcome else 0.
    shaping = gamma ** ticks * (potential(after, spec['potential_scale']) if outcome is None else 0.) - potential(before, spec['potential_scale'])
    return {'time': time_cost, 'terminal': terminal, 'potential': shaping, 'total': time_cost + terminal + shaping}


def navigation_terms(model, actor, flat, indices, c):
    """Likelihood of the sampled three latents and the squashed entropy."""
    return model.log_prob(actor, flat['latent'][indices]), model.entropy(actor).mean()


def speed_terms(model, actor, flat, indices, c):
    """Likelihood and entropy over the sampled speed only, for a deterministic heading."""
    return model.speed_log_prob(actor, flat['latent'][indices]), model.speed_entropy(actor).mean()


def policy_terms(c):
    return speed_terms if c['heading_sampler'] == 'deterministic' else navigation_terms


def law_anchor(model, actor, flat, indices, c):
    """Supervised leash toward the law's command on the labelled rows of the minibatch: its heading pair, or the whole command."""
    mask = flat['labelled'][indices]
    if mask.sum() == 0.:
        return torch.zeros(())
    labels = flat['labels'][indices]
    rows = fit.loss_rows(actor, labels) if c['anchor_terms'] == 'command' else ((torch.tanh(actor[:, :2]) - labels[:, :2]) ** 2).sum(1)
    return c['anchor'] * (rows * mask).sum() / mask.sum()


def pools(rows, slots=SLOTS):
    """Interleaved task pools, one per persistent slot."""
    return [rows[i::slots] for i in range(slots)]


class NavigationCollector:
    """Persistent slots that all decide every hold; each decision is one macro-step with its own discount."""

    def __init__(self, bridge, pools, c, labeller=None, rng=None):
        self.bridge = bridge; self.pools = pools; self.c = c; self.cursors = [0] * len(pools)
        self.labeller = labeller; self.rng = rng or random.Random(c['learner_seed'])
        self.active = [self.next_task(i) for i in range(len(pools))]
        self.items = bridge.call('reset', tasks=[t['task'] for t in self.active])['items']
        self.outcomes = {}; self.episodes = 0; self.ticks = 0; self.physical_failures = 0
        self.window = {}

    def next_task(self, slot):
        pool = self.pools[slot]; task = pool[self.cursors[slot] % len(pool)]; self.cursors[slot] += 1
        return task

    def record(self, slot, outcome, ticks):
        key = '%s/%s' % (self.active[slot]['family'], outcome)
        self.outcomes[key] = self.outcomes.get(key, 0) + 1; self.episodes += 1
        self.physical_failures += outcome not in ('success', 'deadline')
        self.window['episodes'] = self.window.get('episodes', 0) + 1
        self.window[outcome] = self.window.get(outcome, 0) + 1
        if outcome == 'success':
            self.window.setdefault('success_ticks', []).append(ticks)

    def collect(self, model, decisions, lam):
        n = len(self.pools); c = self.c
        keys = ['obs', 'latent', 'logp', 'value', 'reward', 'done', 'gamma', 'label', 'labelled']
        seq = {k: [] for k in keys}
        self.window = {}
        sample = model.sample_speed if c['heading_sampler'] == 'deterministic' else model.sample
        for _ in range(decisions):
            x = tensor(self.items)
            with torch.no_grad():
                latent, logp, value = sample(x)
            labels = torch.zeros(n, 3); labelled = torch.zeros(n)
            if self.labeller is not None and c['label_rate'] > 0.:
                for i in range(n):
                    if self.rng.random() < c['label_rate']:
                        labels[i] = torch.tensor(self.labeller(self.items[i]['inputs'])); labelled[i] = 1.
            result = self.bridge.call('step_command', commands=command_dicts(latent), hold=c['hold'])
            after = result['items']; ticks = result['ticks']; self.ticks += sum(ticks)
            rewards = []; dones = []; gammas = []; resets = []
            for i in range(n):
                rewards.append(decision_reward(self.items[i], after[i], ticks[i], c['gamma'], c['reward'])['total'])
                gammas.append(c['gamma'] ** ticks[i])
                outcome = after[i]['progress']['outcome']; dones.append(1. if outcome else 0.)
                if outcome:
                    self.record(i, outcome, after[i]['progress']['elapsed_ticks']); resets.append(i)
            seq['obs'].append(x); seq['latent'].append(latent); seq['logp'].append(logp); seq['value'].append(value)
            seq['reward'].append(torch.tensor(rewards)); seq['done'].append(torch.tensor(dones)); seq['gamma'].append(torch.tensor(gammas))
            seq['label'].append(labels); seq['labelled'].append(labelled)
            self.items = after
            for i in resets:
                self.active[i] = self.next_task(i)
                self.items = self.bridge.call('reset_at', index=i, task=self.active[i]['task'])['items']
        with torch.no_grad():
            final_values = model(tensor(self.items))[1]
        values = torch.stack(seq['value']); rewards = torch.stack(seq['reward']); dones = torch.stack(seq['done']); gammas = torch.stack(seq['gamma'])
        advantages = torch.zeros_like(rewards); running = torch.zeros(n)
        for k in reversed(range(decisions)):
            next_value = final_values if k == decisions - 1 else values[k + 1]
            live = 1. - dones[k]
            delta = rewards[k] + gammas[k] * live * next_value - values[k]
            running = delta + gammas[k] * lam * live * running
            advantages[k] = running
        return {'observations': torch.stack(seq['obs']), 'latent': torch.stack(seq['latent']), 'logp': torch.stack(seq['logp']),
                'advantages': advantages, 'returns': advantages + values, 'labels': torch.stack(seq['label']), 'labelled': torch.stack(seq['labelled'])}


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
    model = NavigationModel(schema)
    model.load_state_dict(torch.load(warm_checkpoint, weights_only=True)['state'])
    with torch.no_grad():
        model.log_std.fill_(c['exploration_log_std'])
    return model


def window_summary(window):
    ticks = window.get('success_ticks', [])
    return {'episodes': window.get('episodes', 0), 'success': window.get('success', 0), 'collision': window.get('collision', 0),
            'deadline': window.get('deadline', 0), 'mean_success_ticks': sum(ticks) / len(ticks) if ticks else None}


def log_line(line):
    print(line, flush=True)


def run(c, out, bridge_path, warm_checkpoint, archive, threads=3, log=log_line):
    torch.set_num_threads(threads); torch.use_deterministic_algorithms(True)
    torch.manual_seed(c['learner_seed']); random.seed(c['learner_seed'])
    out.mkdir(parents=True, exist_ok=False); started = time.monotonic()
    bridge_path = str(Path(bridge_path).resolve()); motor = str(Path(reference.DEFAULT_MOTOR).resolve())
    atomic_json(out / 'config.json', {**c, 'bridge_sha256': sha(bridge_path), 'motor_sha256': sha(motor), 'warm_checkpoint_sha256': sha(warm_checkpoint)})
    history = []
    with Bridge(bridge_path) as bridge:
        bridge.call('load_motor_policy', path=motor)
        bearings = bridge.contract['schema']['ray_bearings']; edges = bridge.contract['schema']['radial_edges']
        model = make_model(bridge.contract['schema'], c, warm_checkpoint)
        development = fit.development_rows(bridge, archive)
        held_out = fit.static_rows(reference.task_sets(bridge, fit.HELD_OUT_SEED, 24))
        training = fit.static_rows(reference.task_sets(bridge, c['training_seed'], 120))
        atomic_json(out / 'tasks-training.json', training)
        export = out / 'policy-initial.json'; model.export(export); bridge.call('load_navigation_policy', path=str(export.resolve()))
        initial = fit.summarize(fit.evaluate(bridge, model, development, out / 'evaluation-initial'))
        log('initial: development %d/%d (%.0f s)' % (initial['passed'], initial['cases'], time.monotonic() - started))
        labeller = (lambda inputs: fit.label(inputs, bearings, edges)[0]) if c['anchor'] > 0. else None
        collector = NavigationCollector(bridge, pools(training, c['slots']), c, labeller)
        if c['critic_warmup_updates']:
            critic = torch.optim.Adam([p for name, p in model.named_parameters() if name.startswith('critic')], lr=c['learning_rate'] * 3, eps=1e-5)
            for i in range(c['critic_warmup_updates']):
                rollout = collector.collect(model, c['decisions_per_slot'], c['lambda'])
                loss = value_update(model, critic, rollout, c)
                history.append({'phase': 'critic-warmup', 'update': i + 1, 'value_loss': loss, 'rows': rollout['returns'].numel(), 'window': window_summary(collector.window)})
                log('critic warm-up %d/%d value loss %.4f window %s' % (i + 1, c['critic_warmup_updates'], loss, json.dumps(window_summary(collector.window))))
        optimizer = torch.optim.Adam(model.parameters(), lr=c['learning_rate'], eps=1e-5)
        auxiliary = law_anchor if c['anchor'] > 0. else None
        for i in range(c['updates']):
            rollout = collector.collect(model, c['decisions_per_slot'], c['lambda'])
            try:
                metrics = update(model, optimizer, rollout, c, policy_terms=policy_terms(c), auxiliary=auxiliary)
            except RuntimeError as error:
                if 'no PPO optimizer step' not in str(error):
                    raise
                metrics = {'minibatches': 0, 'kl_early_stop': True}
            metrics.update(phase='ppo', update=i + 1, rows=rollout['returns'].numel(), ticks=collector.ticks, episodes=collector.episodes,
                           physical_failures=collector.physical_failures, outcomes=dict(collector.outcomes), window=window_summary(collector.window),
                           log_std=model.log_std.detach().tolist(), labelled_rows=int(rollout['labelled'].sum()))
            history.append(metrics)
            if (i + 1) % 10 == 0 or i + 1 == c['updates']:
                log('update %d/%d kl %.4f entropy %.3f aux %.4f window %s failures %d (%.0f s)' % (
                    i + 1, c['updates'], metrics.get('kl', float('nan')), metrics.get('entropy', float('nan')), metrics.get('auxiliary', 0.),
                    json.dumps(window_summary(collector.window)), collector.physical_failures, time.monotonic() - started))
            atomic_json(out / 'history.json', history)
        export = out / 'policy-final.json'; model.export(export); bridge.call('load_navigation_policy', path=str(export.resolve()))
        torch.save({'kind': KIND, 'config': c, 'state': model.state_dict()}, out / 'checkpoint-final.pt')
        final = fit.summarize(fit.evaluate(bridge, model, development, out / 'evaluation-final'))
        log('final: development %d/%d (%.0f s)' % (final['passed'], final['cases'], time.monotonic() - started))
        final_held_out = fit.summarize(fit.evaluate(bridge, model, held_out, out / 'evaluation-final-held-out'))
        log('final: held out %d/%d (%.0f s)' % (final_held_out['passed'], final_held_out['cases'], time.monotonic() - started))
    summary = {'config': c, 'initial': initial, 'final': final, 'final_held_out': final_held_out,
               'training': {'episodes': collector.episodes, 'ticks': collector.ticks, 'physical_failures': collector.physical_failures,
                            'outcomes': collector.outcomes, 'kl_early_stops': sum(1 for h in history if h.get('kl_early_stop'))},
               'seconds': time.monotonic() - started}
    atomic_json(out / 'summary.json', summary)
    return summary


def mean_ticks(summary):
    return {family: g['mean_success_ticks'] for family, g in summary['families'].items()}


def summarize_grid(root):
    rows = []
    for path in sorted(Path(root).glob('*/summary.json')):
        s = json.loads(path.read_text()); c = s['config']
        rows.append({'cell': path.parent.name, 'seed': c['learner_seed'], 'anchor': c['anchor'], 'anchor_terms': c.get('anchor_terms', 'command'),
                     'heading_sampler': c.get('heading_sampler', 'gaussian'), 'updates': c['updates'],
                     'initial_passed': s['initial']['passed'], 'final_passed': s['final']['passed'], 'held_out_passed': s['final_held_out']['passed'],
                     'initial_ticks': mean_ticks(s['initial']), 'final_ticks': mean_ticks(s['final']), 'held_out_ticks': mean_ticks(s['final_held_out']),
                     'training_outcomes': s['training']['outcomes'], 'physical_failures': s['training']['physical_failures'],
                     'kl_early_stops': s['training']['kl_early_stops'], 'seconds': round(s['seconds'])})
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, required=True); p.add_argument('--seed', type=int); p.add_argument('--anchor', type=float, default=0.)
    p.add_argument('--updates', type=int, default=120); p.add_argument('--exploration-log-std', type=float, default=-1.2)
    p.add_argument('--learning-rate', type=float, default=1e-4); p.add_argument('--gamma', type=float, default=.998)
    p.add_argument('--decisions-per-slot', type=int, default=96); p.add_argument('--threads', type=int, default=3)
    p.add_argument('--heading', choices=HEADING_SAMPLERS, default='deterministic'); p.add_argument('--anchor-terms', choices=ANCHOR_TERMS, default='heading')
    p.add_argument('--bridge', default=reference.DEFAULT_BRIDGE); p.add_argument('--warm-checkpoint', default=WARM_CHECKPOINT)
    p.add_argument('--archived-development', type=Path, default=Path('data/training-experiments/learned-flight-navigation-reference-v3-20261025/tasks.json'))
    p.add_argument('--summarize', action='store_true')
    args = p.parse_args()
    if args.summarize:
        rows = summarize_grid(args.out); atomic_json(args.out / 'grid-summary.json', rows)
        for r in rows:
            print('%-24s seed %2d %-13s anchor %.1f %-8s development %3d -> %3d  held out %3d  ticks %s -> %s  held out %s  outcomes %s  failures %d  kl stops %d  %ds' % (
                r['cell'], r['seed'], r['heading_sampler'], r['anchor'], r['anchor_terms'], r['initial_passed'], r['final_passed'], r['held_out_passed'],
                {k: round(v) for k, v in r['initial_ticks'].items() if v}, {k: round(v) for k, v in r['final_ticks'].items() if v},
                {k: round(v) for k, v in r['held_out_ticks'].items() if v}, r['training_outcomes'], r['physical_failures'], r['kl_early_stops'], r['seconds']))
        return
    c = config(args.seed, args.anchor, args.updates, args.exploration_log_std, args.learning_rate, args.gamma, args.decisions_per_slot,
               heading_sampler=args.heading, anchor_terms=args.anchor_terms)
    run(c, args.out, args.bridge, args.warm_checkpoint, args.archived_development, args.threads)


if __name__ == '__main__':
    main()
