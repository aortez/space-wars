"""Retrospective, exact-update replay and fixed-batch coefficient probes."""
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
from tools.flight_ppo.checkpoint import identity, load, atomic_json
from tools.flight_ppo.model import FlightModel, tensor
from tools.flight_ppo.ppo import gae, update
from tools.flight_ppo.reward import reward
from tools.flight_ppo.tests import equal_tree
from .signal import RecordingBridge, gradient_vector, inspect_gradients, labels, summary

PARENT = Path('data/training-experiments/learned-flight-mixed-duration-v1-20261023')
VARIANTS = {'full': {}, 'no_value': {'value_weight': 0.},
            'no_entropy': {'entropy_weight': 0.},
            'policy_only': {'value_weight': 0., 'entropy_weight': 0.}}


def probe(model, observations):
    with torch.no_grad():
        actor, value = model(observations)
    return {'mean_thrust_logit': float(actor[:, 1].mean()),
            'mean_thrust_probability': float(actor[:, 1].sigmoid().mean()),
            'thrust_logits': actor[:, 1].tolist(), 'thrust_probabilities': actor[:, 1].sigmoid().tolist(),
            'brake_probabilities': actor[:, 2].sigmoid().tolist(), 'values': value.tolist()}


def select_updates(scan):
    if [r['update'] for r in scan] != list(range(128, 257)):
        raise ValueError('complete ordered 128–256 scan required')
    deltas = [{'update': b['update'], 'delta': b['mean_thrust_logit']-a['mean_thrust_logit']}
              for a, b in zip(scan, scan[1:])]
    if not all(math.isfinite(r['delta']) for r in deltas):
        raise ValueError('nonfinite scan')
    down = min(deltas, key=lambda r: (r['delta'], r['update']))
    up = min(deltas, key=lambda r: (-r['delta'], r['update']))
    return {'largest_drop': down, 'largest_rise': up, 'fixed_endpoint': 256,
            'updates': sorted({down['update'], up['update'], 256})}


def initial_probes(bridge, admission):
    candidates = [r['candidate'] for r in admission['attempts'] if r['admitted']]
    if not admission['complete'] or admission['seed'] != 4210002 or len(candidates) != 16:
        raise ValueError('wrong probe partition')
    items = [bridge.call('reset', tasks=[c['task']])['items'][0] for c in candidates]
    return candidates, items, tensor(items)


def grouped_rows(rows, rollout, values, normalized, pressure, config):
    compact = [];groups = {}
    for i, row in enumerate(rows):
        before, after = row['before'], row['after']
        terms = reward({'frame': before}, after, config['gamma'],
                       config['velocity_shaping_weight'], config['deadline_distance_weight'])
        components = {}
        for name, scale, coefficient in [('position', 100., .5), ('velocity', 20., config['velocity_shaping_weight']), ('spin', 1., .25)]:
            p = -coefficient*math.tanh(before['errors'][name]/scale)
            q = 0. if after['progress']['outcome'] is not None else -coefficient*math.tanh(after['frame']['errors'][name]/scale)
            components[name] = config['gamma']*q-p
        assert math.isclose(sum(components.values()), terms['potential'], abs_tol=1e-12)
        assert abs(terms['total']-float(rollout['rewards_audit'].flatten()[i])) < 1e-6
        item = {'slot': row['slot'], 'episode': row['episode'], 'tick': row['tick'],
                'action': row['action'], 'before_errors': before['errors'],
                'after_errors': after['frame']['errors'], 'outcome': after['progress']['outcome'],
                'reward': {**terms, **components}, 'advantage': float(rollout['advantages'].flatten()[i]),
                'normalized_advantage': float(normalized[i]), 'critic_value': float(values[i]),
                'critic_target': float(rollout['returns'].flatten()[i]),
                'thrust_score_pressure': float(pressure[i, 1]), 'brake_score_pressure': float(pressure[i, 2]),
                'thrust_latent': float(rollout['ar_latents'].flatten(0, 1)[i, 1]),
                'labels': [k for k, enabled in labels(row).items() if enabled]}
        compact.append(item)
        for label in item['labels']:
            groups.setdefault(label, []).append(item)
    grouped = {label: {'count': len(items),
        **{key: summary([item[key] for item in items]) for key in ['advantage', 'normalized_advantage', 'thrust_score_pressure', 'brake_score_pressure']},
        'reward_means': {key: sum(item['reward'][key] for item in items)/len(items) for key in compact[0]['reward']}}
        for label, items in groups.items()}
    return grouped, compact


def replay_case(run, executable, destination, analyzed_update, observations, wall_seconds=600):
    destination.mkdir()
    start = time.monotonic()
    expected = torch.load(run/f'checkpoint-{analyzed_update:04}.pt', weights_only=True)
    config = expected['contract']['config']
    with Bridge(executable) as raw:
        contract = identity(config, raw)
        bridge = RecordingBridge(raw, limit=8300, wall_seconds=wall_seconds)
        model, optimizer, collector, completed, history = load(
            run/f'checkpoint-{analyzed_update-1:04}.pt', contract, bridge)
        assert completed == analyzed_update-1 and config['exploration']['rho'] == 0.
        initial_model = copy.deepcopy(model.state_dict())
        initial_optimizer = copy.deepcopy(optimizer.state_dict())
        before = probe(model, observations)
        bridge.collector = collector
        rollout = collector.collect(model, config, bridge.deadline)
        rng = torch.get_rng_state().clone();python_rng = random.getstate()
        reconstructed = collector.state()
        assert equal_tree(reconstructed, expected['collector']), 'collector replay differs'
        assert len(bridge.rows) == 1024
        torch.save(rollout, destination/'rollout.pt')
        torch.save({'torch_rng': rng, 'python_rng': python_rng}, destination/'post-collection-rng.pt')
        torch.save(reconstructed, destination/'collector.pt')
        # The instrumentation receives the actual reward samples separately; it
        # never changes the production PPO batch or its return/advantage targets.
        audited = dict(rollout, rewards_audit=torch.tensor([
            reward({'frame': row['before']}, row['after'], config['gamma'],
                   config['velocity_shaping_weight'], config['deadline_distance_weight'])['total']
            for row in bridge.rows]).reshape(256, 4))
        dones = torch.tensor([row['after']['progress']['outcome'] is not None for row in bridge.rows],
                             dtype=torch.float32).reshape(256, 4)
        with torch.no_grad():
            replay_values = torch.stack([model(x)[1] for x in rollout['observations']])
            final_value = model(tensor(collector.items))[1]
        replay_adv, replay_returns = gae(audited['rewards_audit'], dones, replay_values,
                                        final_value, config['gamma'], config['lambda'])
        assert torch.equal(replay_adv, rollout['advantages']) and torch.equal(replay_returns, rollout['returns'])
        gradients, vectors, values, normalized, pressure = inspect_gradients(model, rollout, config, rng)
        q = model(observations)[0][:, 1].mean()
        q_gradient = gradient_vector(q, list(model.parameters()), False)
        gradients['fixed_start_raw_gradient_projection'] = {
            key: -float(q_gradient.dot(vectors[key])) for key in ['policy', 'weighted_critic', 'weighted_entropy']}
        gradients['projection_scope'] = 'first-minibatch direction for mean starting thrust logit under raw gradient descent; not an Adam attribution'
        vectors['fixed_start_logit_gradient'] = q_gradient
        grouped, rows = grouped_rows(bridge.rows, audited, replay_values.flatten(), normalized, pressure, config)
        torch.save(vectors, destination/'gradient-vectors.pt')
        with gzip.open(destination/'rows.json.gz', 'wt') as stream:
            json.dump(rows, stream, allow_nan=False)
        assert equal_tree(model.state_dict(), initial_model) and equal_tree(optimizer.state_dict(), initial_optimizer)
        branches = {}
        for name, overrides in VARIANTS.items():
            if time.monotonic() > bridge.deadline:
                raise TimeoutError('update attribution wall guard')
            model.load_state_dict(initial_model)
            optimizer.load_state_dict(copy.deepcopy(initial_optimizer))
            torch.set_rng_state(rng);random.setstate(python_rng)
            result = update(model, optimizer, rollout, dict(config, **overrides))
            after = probe(model, observations)
            state = {'model': copy.deepcopy(model.state_dict()), 'optimizer': copy.deepcopy(optimizer.state_dict()),
                     'torch_rng': torch.get_rng_state(), 'python_rng': random.getstate(),
                     'coefficient_overrides': overrides, 'metrics': result}
            torch.save(state, destination/f'{name}.pt')
            if name == 'full':
                assert equal_tree(state['model'], expected['model']), 'model update differs'
                assert equal_tree(state['optimizer'], expected['optimizer']), 'optimizer update differs'
                assert torch.equal(state['torch_rng'], expected['torch_rng']) and state['python_rng'] == expected['python_rng']
                archived_metrics = expected['history'][-1]
                assert all(result[k] == archived_metrics[k] for k in result), 'optimizer metrics differ'
                assert history == expected['history'][:-1]
            branches[name] = {'coefficient_overrides': overrides, 'after': after,
                'mean_logit_change': after['mean_thrust_logit']-before['mean_thrust_logit'],
                'mean_probability_change': after['mean_thrust_probability']-before['mean_thrust_probability'],
                'metrics': result}
        record = {'update': analyzed_update, 'before': before, 'branches': branches,
                  'exact_archive_replay': True, 'collector_reconstruction_exact': True,
                  'rewards_values_gae_targets_reconstructed_exactly': True,
                  'gradient_analysis': gradients, 'groups': grouped,
                  'replay_ticks_including_restore': bridge.ticks,
                  'diagnostic_optimizer_steps': sum(b['metrics']['minibatches'] for b in branches.values()),
                  'seconds': time.monotonic()-start,
                  'scope': 'one original update per branch on the identical recorded batch; inherited Adam state and fixed original advantages; no branch continued or promoted'}
        atomic_json(destination/'results.json', record)
        return record


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--archive', type=Path, required=True)
    args = parser.parse_args();archive = args.archive
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    start = time.monotonic();records = []
    with Bridge(archive/'runtime/flight_ppo_bridge') as bridge:
        admission = json.loads((archive/'inputs/development-admission.json').read_text())
        candidates, items, observations = initial_probes(bridge, admission)
        atomic_json(archive/'probe-inputs.json', {'candidates': candidates, 'items': items})
        torch.save(observations, archive/'probe-inputs.pt')
        config = json.loads((archive/'inputs/contract.json').read_text())['config']
        contract = identity(config, bridge)
        model = FlightModel(bridge.contract['schema']);model.eval()
        scan = []
        for number in range(128, 257):
            if time.monotonic()-start > 1200:
                raise TimeoutError('audit wall guard')
            state = torch.load(archive/f'inputs/checkpoint-{number:04}.pt', weights_only=True)
            assert state['contract'] == contract and state['update'] == number
            model.load_state_dict(state['model'])
            scan.append({'update': number, **probe(model, observations)})
        selection = select_updates(scan)
        atomic_json(archive/'checkpoint-scan.json', {'seed': 43, 'scan': scan, 'selection': selection})
    for number in selection['updates']:
        remaining = 1200-(time.monotonic()-start)
        if remaining <= 0:
            raise TimeoutError('audit wall guard')
        records.append(replay_case(archive/'inputs', archive/'runtime/flight_ppo_bridge',
                                  archive/f'update-{number:04}', number, observations, min(600, remaining)))
        assert records[-1]['before'] == {k: v for k, v in scan[number-129].items() if k != 'update'}
        assert records[-1]['branches']['full']['after'] == {k: v for k, v in scan[number-128].items() if k != 'update'}
        assert sum(r['replay_ticks_including_restore'] for r in records) <= 25000
        atomic_json(archive/'progress.json', {'completed_cases': len(records),
            'replay_ticks': sum(r['replay_ticks_including_restore'] for r in records)})
    atomic_json(archive/'results.json', {'experiment_id': archive.name, 'seed': 43,
        'selection': selection, 'scan': scan, 'cases': records,
        'replay_ticks_including_restore': sum(r['replay_ticks_including_restore'] for r in records),
        'diagnostic_optimizer_steps': sum(r['diagnostic_optimizer_steps'] for r in records),
        'seconds': time.monotonic()-start, 'confirmation_used': False, 'model_promoted': False,
        'new_training_experiment': False})


if __name__ == '__main__':
    main()
