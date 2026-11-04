"""LF-064: paired physical action interventions, with immutable saved policies."""
import argparse
import copy
import math
from pathlib import Path
import shutil
import time

import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.ppo import gae
from tools.flight_ppo.tests import equal_tree
from tools.flight_motor import movement, storage
from tools.flight_motor.velocity import reward

ARMS = ['residual', 'movement']
SEEDS = [17, 29, 43]
HORIZONS = [1, 30, 120]
REPLICATES = 8
OFFSET = .5
GAMMA = .99
LAMBDA = .95
WALL_SECONDS = 900
RESTORE_LIMIT = 400000
VARIANTS = [{'name': 'baseline', 'duration': 0, 'channel': 1, 'offset': 0.}]
for duration in [1, 30]:
    for channel, name in [(1, 'throttle'), (2, 'brake')]:
        for sign in [-1, 1]:
            VARIANTS.append({'name': f'{name}-{sign:+d}-{duration}', 'duration': duration,
                             'channel': channel, 'offset': sign * OFFSET})
read = storage.read_json


def speed_errors(item):
    x = item['inputs']
    return (20 * (-x[0] * x[3] + x[1] * x[4] - x[2]),
            20 * abs(x[1] * x[3] + x[0] * x[4]))


def prefix(trace, tick):
    if not 0 <= tick <= len(trace['steps']):
        raise ValueError('invalid prefix tick')
    out = {**trace, 'steps': trace['steps'][:tick]}
    item = out['steps'][-1]['item'] if tick else out['initial']
    if item['progress']['outcome'] is not None:
        raise ValueError('terminal anchor')
    return out


def select_anchors(directory):
    """Earliest qualifying near-speed state; no return/action-based selection."""
    pool = {}
    for arm in ARMS:
        for seed in SEEDS:
            for base in [0, 16, 32]:
                for bearing in [0, 2]:
                    for offset in [0, 4, 8]:
                        case = base + bearing + offset
                        path = f'{arm}/seed-{seed}/case-{case:02}.json.gz'
                        pool[path] = read(directory / path)
    anchors = []
    for base, command in zip([0, 16, 32], [6, 12, 18]):
        for bearing in [0, 2]:
            for band, offset in [('under', 4), ('near', None), ('over', 8)]:
                if band == 'near':
                    eligible = []
                    for ai, arm in enumerate(ARMS):
                        for seed in SEEDS:
                            for offset in [0, 4, 8]:
                                case = base + bearing + offset
                                path = f'{arm}/seed-{seed}/case-{case:02}.json.gz'
                                for step in pool[path]['steps'][:600]:
                                    item = step['item']; error, lateral = speed_errors(item)
                                    if (abs(error) <= .25 and lateral <= 1
                                            and item['errors']['heading'] <= math.radians(5)
                                            and item['errors']['spin'] <= .2
                                            and item['progress']['outcome'] is None):
                                        eligible.append((item['progress']['elapsed_ticks'], ai, seed, case, path))
                                        break
                    if not eligible:
                        raise ValueError(f'missing near anchor {command}/{bearing}')
                    tick, ai, seed, case, path = min(eligible)
                    arm = ARMS[ai]
                else:
                    tick = 0; arm = 'residual'; seed = 17; case = base + bearing + offset
                    path = f'{arm}/seed-{seed}/case-{case:02}.json.gz'
                    for other in ARMS:
                        for s in SEEDS:
                            assert pool[f'{other}/seed-{s}/case-{case:02}.json.gz']['initial'] == pool[path]['initial']
                trace = prefix(pool[path], tick)
                item = trace['steps'][-1]['item'] if tick else trace['initial']
                error, lateral = speed_errors(item)
                assert (band == 'near' and abs(error) <= .25) or (band == 'under' and error < -1) or (band == 'over' and error > 1)
                anchors.append({'index': len(anchors), 'command': command, 'bearing': bearing // 2,
                                'band': band, 'source': path, 'source_arm': arm, 'source_seed': seed,
                                'case': case, 'tick': tick, 'forward_error': error, 'lateral_speed': lateral})
    return anchors, pool


def perturb(actor, sigma, innovation, variants, step):
    z = actor[:, 1:] + sigma * innovation
    for index, variant in enumerate(variants):
        if step < variant['duration']:
            z[index, variant['channel'] - 1] += variant['offset']
    return z


def credits(trace, tick, values, *, horizons=None, gamma=GAMMA, lam=LAMBDA):
    horizons = HORIZONS if horizons is None else horizons
    steps = trace['steps'][tick:]
    assert len(values) == len(steps) + 1 and steps
    terms = [reward(s['item']) for s in steps]
    rows = []
    for horizon in horizons:
        n = min(horizon, len(steps))
        done = steps[n - 1]['item']['progress']['outcome'] is not None
        assert n == horizon or done
        rewards = torch.tensor([[t['total']] for t in terms[:n]], dtype=torch.float32)
        dones = torch.zeros(n, 1); dones[-1] = float(done)
        before = torch.tensor(values[:n], dtype=torch.float32)[:, None]
        after = torch.tensor([values[n]], dtype=torch.float32)
        advantages, _ = gae(rewards, dones, before, after, gamma, lam)
        observed = sum(gamma ** i * terms[i]['total'] for i in range(n))
        bootstrap = 0. if done else gamma ** n * values[n]
        item = steps[n - 1]['item']
        rows.append({'horizon': horizon, 'performed_ticks': n, 'terminal': done,
                     'observed_return': observed, 'bootstrap': bootstrap,
                     'bootstrapped_return': observed + bootstrap,
                     'gae_first': float(advantages[0, 0]),
                     'td_first': terms[0]['total'] + gamma * (steps[0]['item']['progress']['outcome'] is None) * values[1] - values[0],
                     'velocity_error': item['errors']['velocity'],
                     'forward_error': speed_errors(item)[0], 'outcome': item['progress']['outcome']})
    return rows


def batch(bridge, model, anchor_trace, noise, counts, deadline, *, horizons=None,
          variants=None, gamma=GAMMA, lam=LAMBDA, restore_limit=RESTORE_LIMIT):
    horizons = HORIZONS if horizons is None else horizons
    variants = VARIANTS if variants is None else variants
    tick = len(anchor_trace['steps']); expected = anchor_trace['steps'][-1]['item'] if tick else anchor_trace['initial']
    items = bridge.call('restore', traces=[anchor_trace] * len(variants))['items']
    assert all(i == expected for i in items)
    counts['matched_initial_states'] += len(items)
    counts['restore_ticks'] += tick * len(items); counts['neutral_setups'] += len(items)
    active = list(range(len(variants))); traces = {}; values = [[] for _ in variants]; first = {}
    for step in range(max(horizons)):
        if time.monotonic() > deadline or counts['restore_ticks'] > restore_limit:
            raise TimeoutError('action-return probe budget')
        native = bridge.call('infer', inputs=[i['inputs'] for i in items])['outputs']
        actor = torch.tensor([[n[k] for k in ['steering_latent', 'thrust_logit', 'brake_logit']] for n in native])
        x = torch.tensor([i['inputs'] for i in items]); sigma = model.distribution(actor, x).scale
        if step in {0, 29, 119, max(horizons)-1}:
            raw, value = model(x)
            torch.testing.assert_close(raw, actor, atol=1e-5, rtol=1e-5)
            torch.testing.assert_close(value, torch.tensor([n['value'] for n in native]), atol=1e-5, rtol=1e-5)
            counts['native_python_comparisons'] += len(items)
        innovations = torch.zeros(len(items), 2) if noise is None else noise[step].expand(len(items), 2)
        z = perturb(actor, sigma, innovations, [variants[i] for i in active], step)
        actuators = z.sigmoid(); actions = []
        for slot, variant in enumerate(active):
            action = dict(native[slot]['action'])
            if noise is not None or step < variants[variant]['duration']:
                action.update(thrust=float(actuators[slot, 0]), brake=float(actuators[slot, 1]))
            actions.append(action); values[variant].append(native[slot]['value'])
            if step == 0:
                first[variant] = {'actor': actor[slot].tolist(), 'sigma': sigma[slot].tolist(),
                                  'latent': z[slot].tolist(), 'action': action}
        items = bridge.call('step', actions=actions)['items']
        counts['probe_ticks'] += len(items)
        ended = [i['progress']['outcome'] is not None for i in items]
        if any(ended) or step == max(horizons) - 1:
            current = bridge.call('state')['traces']
            kept = []; next_active = []; next_items = []
            for slot, variant in enumerate(active):
                if ended[slot] or step == max(horizons) - 1:
                    traces[variant] = current[slot]
                else:
                    kept.append(current[slot]); next_active.append(variant); next_items.append(items[slot])
            if not kept:
                break
            items = bridge.call('restore', traces=kept)['items']
            assert items == next_items
            counts['restore_ticks'] += sum(len(t['steps']) for t in kept)
            counts['neutral_setups'] += len(kept); active = next_active
    final = bridge.call('infer', inputs=[traces[i]['steps'][-1]['item']['inputs'] for i in range(len(variants))])['outputs']
    final_x = torch.tensor([traces[i]['steps'][-1]['item']['inputs'] for i in range(len(variants))])
    torch.testing.assert_close(model(final_x)[1], torch.tensor([n['value'] for n in final]), atol=1e-5, rtol=1e-5)
    counts['native_python_comparisons'] += len(final)
    rows = []
    for index, variant in enumerate(variants):
        trace = traces[index]; assert prefix(trace, tick) == anchor_trace
        values[index].append(final[index]['value'])
        rows.append({'variant': variant, 'trace': trace, 'values': values[index], 'first': first[index],
                     'metrics': credits(trace, tick, values[index], horizons=horizons, gamma=gamma, lam=lam)})
    return rows


def run(a, arm, seed):
    start = time.monotonic(); deadline = start + WALL_SECONDS
    out = a / f'probe/{arm}/seed-{seed}'; out.mkdir()
    checkpoint = storage.load_torch(a / f'inputs/{arm}/seed-{seed}/checkpoint-0128.pt.gz')
    c = checkpoint['contract']['config']
    assert c['actuator_sampler'] == 'independent-gaussian' and 'motor_exploration' not in c
    rng = torch.get_rng_state().clone()
    with torch.random.fork_rng(devices=[]):
        model = movement.make_model(checkpoint['contract']['schema'], c)
    model.load_state_dict(checkpoint['model']); model.requires_grad_(False)
    fixed = copy.deepcopy(model.state_dict())
    noises = storage.load_torch(a / 'innovations.pt.gz')
    anchors = read(a / 'anchors.json'); counts = {k: 0 for k in [
        'matched_initial_states', 'restore_ticks', 'neutral_setups', 'probe_ticks',
        'native_python_comparisons', 'baseline_replays', 'baseline_replay_ticks', 'source_suffix_matches']}
    records = []
    with Bridge(a / 'runtime/flight_motor_bridge') as bridge:
        bridge.call('load_policy', path=str((a / f'inputs/{arm}/seed-{seed}/policy-0128.json').resolve()))
        for anchor in anchors:
            if shutil.disk_usage(out).free < 512 * 1024**2:
                raise RuntimeError('action-return retention reserve')
            index = anchor['index']; tick = anchor['tick']; trace = read(a / f'anchors/anchor-{index:02}.json.gz')
            for replicate in [-1, *range(REPLICATES)]:
                mode = 'mean' if replicate == -1 else 'sampled'
                try:
                    rows = batch(bridge, model, trace, None if replicate == -1 else noises[index, replicate], counts, deadline)
                except Exception as error:
                    storage.dump_json(out / 'interruption.json.gz', {'error': str(error), 'anchor': index,
                        'replicate': replicate, 'counts': counts, 'partial_traces': bridge.call('state')['traces']})
                    raise
                name = f'anchor-{index:02}-{mode}-{replicate:02}.json.gz'
                storage.dump_json(out / name, {'anchor': anchor, 'mode': mode, 'replicate': replicate, 'branches': rows})
                if mode == 'mean':
                    baseline = rows[0]['trace']
                    restored = bridge.call('restore', traces=[baseline])['items'][0]
                    assert restored == baseline['steps'][-1]['item']
                    counts['baseline_replays'] += 1; counts['baseline_replay_ticks'] += len(baseline['steps'])
                    counts['neutral_setups'] += 1
                    if tick == 0 or (arm == anchor['source_arm'] and seed == anchor['source_seed']):
                        original = read(a / f"inputs/{arm}/seed-{seed}/case-{anchor['case']:02}.json.gz")
                        assert baseline['steps'] == original['steps'][:len(baseline['steps'])]
                        counts['source_suffix_matches'] += 1
                records.append({'file': name, 'anchor': index, 'mode': mode, 'replicate': replicate,
                                'branches': [{k: r[k] for k in ['variant', 'first', 'metrics']} for r in rows]})
            print({'arm': arm, 'seed': seed, 'anchor': index, 'ticks': counts['probe_ticks']}, flush=True)
    assert equal_tree(fixed, model.state_dict()) and torch.equal(rng, torch.get_rng_state())
    assert counts['probe_ticks'] <= 174960 and counts['restore_ticks'] <= RESTORE_LIMIT
    result = {'arm': arm, 'seed': seed, 'counts': counts, 'records': records,
              'weights_rng_unchanged': True, 'training_ticks': 0, 'optimizer_steps': 0,
              'seconds': time.monotonic() - start}
    assert result['seconds'] < WALL_SECONDS
    atomic_json(out / 'results.json', result)


def verify(a):
    start = time.monotonic(); results = []; counts = {}; action_rows = 0
    noises = storage.load_torch(a / 'innovations.pt.gz')
    anchors = read(a / 'anchors.json')
    for arm in ARMS:
        for seed in SEEDS:
            out = a / f'probe/{arm}/seed-{seed}'; result = read(out / 'results.json')
            state = storage.load_torch(a / f'inputs/{arm}/seed-{seed}/checkpoint-0128.pt.gz')
            with torch.random.fork_rng(devices=[]):
                model = movement.make_model(state['contract']['schema'], state['contract']['config'])
            model.load_state_dict(state['model']); model.requires_grad_(False)
            assert result['weights_rng_unchanged'] and result['seconds'] < WALL_SECONDS
            assert len(result['records']) == 18 * 9
            observed_ticks = 0; failures = 0
            for record in result['records']:
                data = read(out / record['file']); anchor = anchors[record['anchor']]; tick = anchor['tick']
                assert data['anchor'] == anchor and data['mode'] == record['mode'] and data['replicate'] == record['replicate']
                assert len(data['branches']) == 9
                common = read(a / f"anchors/anchor-{anchor['index']:02}.json.gz")
                for i, branch in enumerate(data['branches']):
                    trace = branch['trace']; assert prefix(trace, tick) == common and branch['variant'] == VARIANTS[i]
                    assert branch['metrics'] == credits(trace, tick, branch['values'])
                    assert {k: branch[k] for k in ['variant', 'first', 'metrics']} == record['branches'][i]
                    steps = trace['steps'][tick:]; n = len(steps)
                    before = [common['steps'][-1]['item'] if tick else common['initial']] + [s['item'] for s in steps[:-1]]
                    x = torch.tensor([s['inputs'] for s in before]); actor, value = model(x)
                    sigma = model.distribution(actor, x).scale
                    innovation = torch.zeros(n, 2) if record['mode'] == 'mean' else noises[anchor['index'], record['replicate'], :n]
                    z = actor[:, 1:] + sigma * innovation
                    v = VARIANTS[i]; z[:min(n, v['duration']), v['channel'] - 1] += v['offset']
                    actual = torch.tensor([[s['action'][k] for k in ['steering', 'thrust', 'brake']] for s in steps])
                    expected = torch.cat((actor[:, :1].tanh(), z.sigmoid()), dim=1)
                    torch.testing.assert_close(actual, expected, atol=1e-5, rtol=1e-5)
                    torch.testing.assert_close(value, torch.tensor(branch['values'][:-1]), atol=1e-5, rtol=1e-5)
                    final_value = model(torch.tensor([steps[-1]['item']['inputs']]))[1][0]
                    torch.testing.assert_close(final_value, torch.tensor(branch['values'][-1]), atol=1e-5, rtol=1e-5)
                    observed_ticks += n; action_rows += n
                    terminal = steps[-1]['item']['progress']['outcome']
                    assert terminal in [None, 'success', 'deadline', 'collision', 'damage', 'ship_lost', 'boundary_escape']
                    assert all(s['item']['progress']['outcome'] is None for s in steps[:-1])
                    failures += int(terminal not in [None, 'success', 'deadline'])
            assert observed_ticks == result['counts']['probe_ticks']
            result['physical_failures'] = failures
            for k, v in result['counts'].items(): counts[k] = counts.get(k, 0) + v
            results.append(result)
            if time.monotonic() - start > 600: raise TimeoutError('action-return verification budget')
    assert counts['matched_initial_states'] == 8748 and counts['baseline_replays'] == 108
    assert counts['source_suffix_matches'] == 78 and counts['probe_ticks'] <= 1049760
    result = {'experiment_id': a.name, 'verification_passed': True, 'records': results, 'counts': counts,
              'verified_action_value_rows': action_rows, 'physical_failures': sum(r['physical_failures'] for r in results),
              'training_ticks': 0, 'optimizer_steps': 0, 'new_checkpoints': 0, 'new_exports': 0,
              'confirmation_used': False, 'model_promoted': False, 'navigation_qualified': False,
              'seconds': time.monotonic() - start}
    atomic_json(a / 'results.json', result)


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--archive', type=Path, required=True)
    p.add_argument('--arm', choices=ARMS); p.add_argument('--seed', type=int, choices=SEEDS)
    p.add_argument('--verify', action='store_true'); args = p.parse_args()
    torch.set_num_threads(1); torch.use_deterministic_algorithms(True); torch.set_grad_enabled(False)
    if args.verify: verify(args.archive)
    else: run(args.archive, args.arm, args.seed)
