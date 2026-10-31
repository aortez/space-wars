"""LF-071: matched action returns at LF-070's saved single-speed learning states."""
import argparse
import copy
from pathlib import Path
import shutil
import time

import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from tools.flight_motor import aligned, storage, sustained
from . import motor_action_return as base
from . import motor_error_math as region

SEEDS = [17, 29, 43]
UPDATES = [1, 16, 43, 64, 86]
HORIZONS = [1, 30, 120, 256]
REPLICATES = 8
GAMMA = .99
LAMBDA = 1.
WALL_SECONDS = 1200
RESTORE_LIMIT = 5000000
VARIANTS = base.VARIANTS
read, load = storage.read_json, storage.load_torch


def make_model(state):
    c = state['contract']['config']
    assert c['gamma'] == GAMMA and c['lambda'] == LAMBDA
    assert c['actuator_sampler'] == 'independent-gaussian' and 'motor_exploration' not in c
    with torch.random.fork_rng(devices=[]):
        model = sustained.ResidualModel(state['contract']['schema'])
    model.load_state_dict(state['model']); model.requires_grad_(False)
    return model


def interval_sources(source, before, after, rollout):
    """Map every saved row to its original episode and exact pre-action prefix."""
    b, a = before['collector'], after['collector']
    assert a['completed'][:len(b['completed'])] == b['completed']
    completed = a['completed'][len(b['completed']):]
    sources = {}
    for slot, pool in enumerate(b['pools']):
        indices = {r['index'] for r in pool}
        episodes = [(e, read(source/f"training-traces/episode-{e['id']:06}.json.gz"))
                    for e in completed if e['candidate']['index'] in indices]
        episodes.append((a['active'][slot], a['traces'][slot]))
        assert episodes[0][0]['id'] == b['active'][slot]['id']
        offset = 0
        for episode, trace in episodes:
            lo = b['active'][slot]['ticks'] if episode['id'] == b['active'][slot]['id'] else 0
            assert len(trace['steps']) == episode['ticks']
            for tick in range(lo, len(trace['steps'])):
                item = trace['initial'] if tick == 0 else trace['steps'][tick-1]['item']
                assert torch.equal(torch.tensor(item['inputs']), rollout['observations'][offset, slot])
                sources[offset*len(b['pools'])+slot] = (trace, tick, episode['id'], episode['candidate']['index'])
                offset += 1
        assert offset == 256
    assert len(sources) == 1536
    return sources


def selected_rows(mask):
    ids = mask.nonzero().flatten().tolist()
    # Fixed tercile positions in time-major/slot-major order, not outcome-ranked.
    return sorted({ids[len(ids)//3], ids[2*len(ids)//3]}) if ids else []


def horizons(anchor):
    return sorted(set(HORIZONS + [anchor['rollout_remaining']]))


def prepare(a):
    anchors, coverage = [], []
    for seed in SEEDS:
        source = a/f'inputs/speed16/seed-{seed}'
        for update in UPDATES:
            before = load(source/f'checkpoint-{update-1:04}.pt.gz')
            after = load(source/f'checkpoint-{update:04}.pt.gz')
            rollout = load(source/f'rollout-{update:04}.pt.gz')
            sources = interval_sources(source, before, after, rollout)
            model = make_model(before)
            target = a/f'policies/seed-{seed}-before-{update:04}.json'
            assert not target.exists(); model.export(target)
            x = rollout['observations'].flatten(0, 1)
            error, masks = region.regions(x)
            pooled = aligned.advantages(rollout['advantages'].flatten(), x[:, 2] > 0)
            for side in ['under', 'over']:
                group = 'ready_' + side
                ids = selected_rows(masks[group])
                coverage.append({'seed': seed, 'update': update, 'region': group,
                                 'available_rows': int(masks[group].sum()), 'selected_rows': ids})
                for index in ids:
                    t, slot = divmod(index, 6)
                    trace, tick, episode_id, candidate = sources[index]
                    anchor = {'index': len(anchors), 'seed': seed, 'update': update, 'region': group,
                        'band': side, 'row': index, 'rollout_t': t, 'slot': slot,
                        'rollout_remaining': 256-t, 'tick': tick, 'episode_id': episode_id,
                        'candidate': candidate, 'command': 16, 'forward_error': float(error[index]),
                        'original_advantage': float(rollout['advantages'][t, slot]),
                        'original_pooled_advantage': float(pooled[index]),
                        'original_latents': rollout['actuator_latents'][t, slot].tolist()}
                    common = base.prefix(trace, tick)
                    original = {**trace, 'steps': trace['steps'][:min(len(trace['steps']), tick+256-t)]}
                    assert len(original['steps']) > tick
                    storage.dump_json(a/f"anchors/anchor-{len(anchors):02}.json.gz", common)
                    storage.dump_json(a/f"anchors/source-{len(anchors):02}.json.gz", original)
                    anchors.append(anchor)
    atomic_json(a/'anchors.json', anchors); atomic_json(a/'coverage.json', coverage)
    g = torch.Generator().manual_seed(710001)
    storage.dump_torch(a/'innovations.pt.gz', torch.randn(len(anchors), REPLICATES, max(HORIZONS), 2, generator=g))
    return anchors, coverage


def protected_rows(model, parent, rollout, bridge):
    x = rollout['observations'].flatten(0, 1)
    x = x[x[:, 2] == 0]
    raw = model(x)[0]; old = parent(x)[0]
    assert torch.equal(raw, old)
    assert torch.equal(model.distribution(raw, x).scale, parent.distribution(old, x).scale)
    native = bridge.call('infer', inputs=x.tolist())['outputs']
    actual = torch.tensor([[n[k] for k in ['steering_latent', 'thrust_logit', 'brake_logit']] for n in native])
    torch.testing.assert_close(actual, raw, atol=1e-5, rtol=1e-5)
    return len(x)


def run(a, seed):
    started = time.monotonic(); deadline = started + WALL_SECONDS
    out = a/f'probe/seed-{seed}'; out.mkdir()
    anchors = read(a/'anchors.json'); noise = load(a/'innovations.pt.gz')
    source = a/f'inputs/speed16/seed-{seed}'
    parent_state = load(source/'checkpoint-0000.pt.gz'); parent = make_model(parent_state)
    rng = torch.get_rng_state().clone()
    counts = {k: 0 for k in ['matched_initial_states', 'restore_ticks', 'neutral_setups', 'probe_ticks',
        'native_python_comparisons', 'baseline_replays', 'baseline_replay_ticks', 'source_replays',
        'source_replay_ticks', 'protected_stopping_rows']}
    records = []
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        for update in UPDATES:
            state = load(source/f'checkpoint-{update-1:04}.pt.gz'); model = make_model(state)
            fixed = copy.deepcopy(model.state_dict())
            sustained.assert_parent_actor(parent_state['model'], fixed)
            bridge.call('load_policy', path=str((a/f'policies/seed-{seed}-before-{update:04}.json').resolve()))
            counts['protected_stopping_rows'] += protected_rows(model, parent, load(source/f'rollout-{update:04}.pt.gz'), bridge)
            for anchor in [v for v in anchors if v['seed'] == seed and v['update'] == update]:
                if shutil.disk_usage(out).free < 512*1024**2: raise RuntimeError('probe retention reserve')
                index, tick = anchor['index'], anchor['tick']
                trace = read(a/f'anchors/anchor-{index:02}.json.gz')
                original = read(a/f'anchors/source-{index:02}.json.gz')
                restored = bridge.call('restore', traces=[original])['items'][0]
                assert restored == original['steps'][-1]['item']
                counts['source_replays'] += 1; counts['source_replay_ticks'] += len(original['steps'])
                counts['neutral_setups'] += 1
                for replicate in [-1, *range(REPLICATES)]:
                    mode = 'mean' if replicate == -1 else 'sampled'
                    try:
                        branches = base.batch(bridge, model, trace, None if replicate == -1 else noise[index, replicate],
                            counts, deadline, horizons=horizons(anchor), variants=VARIANTS,
                            gamma=GAMMA, lam=LAMBDA, restore_limit=RESTORE_LIMIT)
                    except Exception as error:
                        storage.dump_json(out/'interruption.json.gz', {'error': str(error), 'anchor': index,
                            'replicate': replicate, 'counts': counts, 'partial_traces': bridge.call('state')['traces']})
                        raise
                    name = f'anchor-{index:02}-{mode}-{replicate:02}.json.gz'
                    storage.dump_json(out/name, {'anchor': anchor, 'mode': mode, 'replicate': replicate, 'branches': branches})
                    if mode == 'mean':
                        baseline = branches[0]['trace']
                        final = bridge.call('restore', traces=[baseline])['items'][0]
                        assert final == baseline['steps'][-1]['item']
                        counts['baseline_replays'] += 1; counts['baseline_replay_ticks'] += len(baseline['steps'])
                        counts['neutral_setups'] += 1
                    records.append({'file': name, 'anchor': index, 'mode': mode, 'replicate': replicate,
                        'branches': [{k: b[k] for k in ['variant', 'first', 'metrics']} for b in branches]})
                print({'seed': seed, 'update': update, 'anchor': index, 'probe_ticks': counts['probe_ticks']}, flush=True)
            assert equal_tree(fixed, model.state_dict())
    assert torch.equal(rng, torch.get_rng_state()) and counts['restore_ticks'] <= RESTORE_LIMIT
    assert counts['probe_ticks'] <= sum(v['seed'] == seed for v in anchors)*9*9*256
    seconds = time.monotonic()-started; assert seconds < WALL_SECONDS
    atomic_json(out/'results.json', {'seed': seed, 'records': records, 'counts': counts,
        'weights_rng_unchanged': True, 'training_ticks': 0, 'optimizer_steps': 0, 'seconds': seconds})


def verify(a):
    started = time.monotonic(); results, counts = [], {}
    anchors = read(a/'anchors.json'); noise = load(a/'innovations.pt.gz')
    verified_rows = 0
    for seed in SEEDS:
        out = a/f'probe/seed-{seed}'; result = read(out/'results.json')
        source = a/f'inputs/speed16/seed-{seed}'
        models = {u: make_model(load(source/f'checkpoint-{u-1:04}.pt.gz')) for u in UPDATES}
        assert result['weights_rng_unchanged'] and result['seconds'] < WALL_SECONDS
        expected_records = {(v['index'], r) for v in anchors if v['seed'] == seed for r in [-1, *range(REPLICATES)]}
        assert len(result['records']) == len(expected_records)
        assert {(v['anchor'], v['replicate']) for v in result['records']} == expected_records
        observed, failures, terminals = 0, 0, {}
        for rec in result['records']:
            data = read(out/rec['file']); anchor = anchors[rec['anchor']]; tick = anchor['tick']
            assert data['anchor'] == anchor and data['replicate'] == rec['replicate'] and data['mode'] == rec['mode']
            assert rec['mode'] == ('mean' if rec['replicate'] == -1 else 'sampled')
            common = read(a/f"anchors/anchor-{anchor['index']:02}.json.gz")
            model = models[anchor['update']]; assert len(data['branches']) == len(VARIANTS)
            for i, b in enumerate(data['branches']):
                trace = b['trace']; assert base.prefix(trace, tick) == common and b['variant'] == VARIANTS[i]
                assert b['metrics'] == base.credits(trace, tick, b['values'], horizons=horizons(anchor), gamma=GAMMA, lam=LAMBDA)
                assert {k: b[k] for k in ['variant', 'first', 'metrics']} == rec['branches'][i]
                steps = trace['steps'][tick:]; n = len(steps); assert 1 <= n <= 256
                before = [common['steps'][-1]['item'] if tick else common['initial']] + [s['item'] for s in steps[:-1]]
                x = torch.tensor([v['inputs'] for v in before]); actor, value = model(x)
                sigma = model.distribution(actor, x).scale
                draws = torch.zeros(n, 2) if rec['mode'] == 'mean' else noise[anchor['index'], rec['replicate'], :n]
                z = actor[:, 1:] + sigma*draws
                variant = VARIANTS[i]; z[:min(n, variant['duration']), variant['channel']-1] += variant['offset']
                expected = torch.cat((actor[:, :1].tanh(), z.sigmoid()), 1)
                actual = torch.tensor([[s['action'][k] for k in ['steering', 'thrust', 'brake']] for s in steps])
                torch.testing.assert_close(actual, expected, atol=1e-5, rtol=1e-5)
                torch.testing.assert_close(value, torch.tensor(b['values'][:-1]), atol=1e-5, rtol=1e-5)
                final = model(torch.tensor([steps[-1]['item']['inputs']]))[1][0]
                torch.testing.assert_close(final, torch.tensor(b['values'][-1]), atol=1e-5, rtol=1e-5)
                for m in b['metrics']:
                    assert abs(m['gae_first']-(m['bootstrapped_return']-b['values'][0])) < 1e-4
                assert all(s['item']['progress']['outcome'] is None for s in steps[:-1])
                terminal = steps[-1]['item']['progress']['outcome']
                assert terminal in [None, 'success', 'deadline', 'collision', 'damage', 'ship_lost', 'boundary_escape']
                assert n == 256 or terminal is not None
                failures += int(terminal not in [None, 'success', 'deadline'])
                terminals[str(terminal)] = terminals.get(str(terminal), 0)+1
                observed += n
            assert all(b['values'][0] == data['branches'][0]['values'][0] for b in data['branches'])
            if time.monotonic()-started > 600: raise TimeoutError('error-return verification budget')
        assert observed == result['counts']['probe_ticks']
        verified_rows += observed; result['physical_failures'] = failures; result['terminal_counts'] = terminals
        for k, v in result['counts'].items(): counts[k] = counts.get(k, 0)+v
        results.append(result)
    assert counts['matched_initial_states'] == len(anchors)*81
    assert counts['baseline_replays'] == counts['source_replays'] == len(anchors)
    assert counts['protected_stopping_rows'] == 11520
    atomic_json(a/'results.json', {'experiment_id': a.name, 'verification_passed': True,
        'records': results, 'counts': counts, 'verified_action_value_rows': verified_rows,
        'physical_failures': sum(r['physical_failures'] for r in results),
        'training_ticks': 0, 'optimizer_steps': 0, 'new_checkpoints': 0,
        'inference_exports_of_existing_weights': 15, 'new_trained_policies': 0,
        'confirmation_used': False, 'model_promoted': False, 'navigation_qualified': False,
        'seconds': time.monotonic()-started})


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--archive', type=Path, required=True)
    p.add_argument('--seed', type=int, choices=SEEDS); p.add_argument('--verify', action='store_true')
    args = p.parse_args()
    torch.set_num_threads(1); torch.use_deterministic_algorithms(True); torch.set_grad_enabled(False)
    if args.verify: verify(args.archive)
    else: run(args.archive, args.seed)
