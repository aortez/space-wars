"""LF-073 strict flight gates, segmented targets, matched starts and exact resumes."""
import argparse
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.ppo import gae
from tools.flight_ppo.tests import equal_tree
from . import effective_batch as stage
from .gae_audit import discounted_targets
from .velocity import reward
from .sustained_audit import read, load, verify_run, preflight_checks, inspect, task_matches, gate

def verify_targets(a, arm, seed, preflight, stage=stage):
    phase='preflight' if preflight else 'pilot'; run=a/f'{phase}/{arm}/seed-{seed}'
    c=stage.config(seed,arm,preflight); end=c['updates']; final=load(run/f'checkpoint-{end:04}.pt.gz')
    state=final['collector']; slots={r['index']:i for i,pool in enumerate(state['pools']) for r in pool}
    count_slots=len(state['pools']);horizon=c['rollout_ticks'];batch=count_slots*horizon
    episodes=[(e,read(run/f"training-traces/episode-{e['id']:06}.json.gz")) for e in state['completed']]
    episodes += list(zip(state['active'],state['traces'])); episodes.sort(key=lambda pair:pair[0]['id'])
    count=[0]*count_slots; observations=torch.empty(end*horizon,count_slots,18); rewards=torch.empty(end*horizon,count_slots); dones=torch.empty_like(rewards)
    for e,trace in episodes:
        n=len(trace['steps']); assert n==e['ticks']
        if not n: continue
        slot=slots[e['candidate']['index']]; lo=count[slot]; hi=lo+n
        assert hi<=end*horizon
        observations[lo:hi,slot]=torch.tensor([trace['initial']['inputs']]+[s['item']['inputs'] for s in trace['steps'][:-1]])
        rewards[lo:hi,slot]=torch.tensor([reward(s['item'])['total'] for s in trace['steps']])
        dones[lo:hi,slot]=torch.tensor([float(s['item']['progress']['outcome'] is not None) for s in trace['steps']])
        count[slot]=hi
    assert count==[end*horizon]*count_slots
    with torch.random.fork_rng(devices=[]):model=stage.make_model(final['contract']['schema'],c)
    model.requires_grad_(False); records=[]; max_error=0.
    selected=stage.target_updates(c)
    for u in selected:
        before=load(run/f'checkpoint-{u-1:04}.pt.gz'); after=load(run/f'checkpoint-{u:04}.pt.gz')
        rollout=load(run/f'rollout-{u:04}.pt.gz'); lo=(u-1)*horizon; hi=u*horizon
        assert torch.equal(observations[lo:hi],rollout['observations'])
        model.load_state_dict(before['model'])
        advantages = torch.empty_like(rollout['advantages']); returns = torch.empty_like(rollout['returns'])
        segment = c['return_horizon_ticks']
        for offset in range(0, horizon, segment):
            stop = offset+segment
            next_x = rollout['observations'][stop] if stop < horizon else torch.tensor([i['inputs'] for i in after['collector']['items']])
            with torch.no_grad():
                values = torch.stack([model(x)[1] for x in rollout['observations'][offset:stop]])
                last = model(next_x)[1]
                aa, rr = gae(rewards[lo+offset:lo+stop], dones[lo+offset:lo+stop], values, last, c['gamma'], c['lambda'])
            advantages[offset:stop] = aa; returns[offset:stop] = rr
            expected = discounted_targets(rewards[lo+offset:lo+stop], dones[lo+offset:lo+stop], last, c['gamma'])
            torch.testing.assert_close(rr, expected, atol=1e-4, rtol=1e-5)
            max_error=max(max_error,float((rr-expected).abs().max()))
        assert torch.equal(advantages,rollout['advantages']) and torch.equal(returns,rollout['returns'])
        records.append({'update':u,'rows':batch,'advantage_std':float(advantages.std(unbiased=False)),
                        'return_std':float(returns.std(unbiased=False))})
    result={'rows':len(selected)*batch,'lambda1_rows':len(selected)*batch if c['lambda']==1 else 0,
            'max_telescoping_rounding_error':max_error,'checks':records}
    atomic_json(run/'target-verification.json',result)
    return result


def verify(a, preflight):
    start = time.monotonic(); records = []; evaluated = parent_exact = zero_retained = 0
    tasks = read(a/'tasks/development.json'); phase = 'preflight' if preflight else 'pilot'
    assert read(a/'experiment-plan.json')['training_failure_policy'] == 'record-physical-failures-v1'
    for arm in stage.ARMS:
        for seed in stage.SEEDS:
            c = stage.config(seed, arm, preflight); end = c['updates']; run = a/f'{phase}/{arm}/seed-{seed}'
            r = verify_run(a, arm, seed, preflight, stage=stage)
            r['target_verification'] = verify_targets(a, arm, seed, preflight); records.append(r)
            for u in range(1, end+1):
                x = load(run/f'rollout-{u:04}.pt.gz')['observations']
                assert x.shape == (c['rollout_ticks'], 6, 18)
                assert torch.equal(x[:, [0, 2, 4], 2], torch.full((c['rollout_ticks'], 3), .8))
                assert not x[:, [1, 3, 5], 2].count_nonzero()
            if preflight:
                checks = preflight_checks({**r, 'arm': 'sustain'}, 'record-physical-failures-v1')
                exposure = r['training_stats'].get('movement_ticks_after_360', 0) + sum(v['forgone_continuation_ticks'] for v in r['training_physical_failures'])
                checks['three_slot_continuation_exposure'] = exposure >= 1620
                r['preflight_checks'] = checks; assert all(checks.values())
                continue
            phases = {}; parent = []
            for task in tasks:
                if task['suite'].startswith('legacy'):
                    parent.append({**task, **inspect(read(a/f"inputs/seed-{seed}/reference/case-{task['legacy_index']:02}.json.gz"))})
            for label in ['initial', 'final']:
                directory = run/f'evaluation-{label}'; rows = read(directory/'results.json'); phases[label] = []
                assert len(rows) == len(read(directory/'parity.json')) == len(tasks) == 104
                for row, task in zip(rows, tasks):
                    assert row['index'] == task['index']
                    trace = read(directory/f"case-{row['index']:02}.json.gz")
                    assert task_matches(trace['task'], task['task'])
                    found = inspect(trace)
                    assert all(row[k] == found[k] for k in ['ticks', 'outcome', 'final_errors'])
                    if task['suite'].startswith('legacy'):
                        assert trace == read(a/f"inputs/seed-{seed}/reference/case-{task['legacy_index']:02}.json.gz"); parent_exact += 1
                    if label == 'final' and task['task']['speed'] == 0:
                        assert trace == read(run/f"evaluation-initial/case-{task['index']:02}.json.gz"); zero_retained += 1
                    if label == 'initial' and arm == 'large':
                        assert trace == read(a/f"pilot/current/seed-{seed}/evaluation-initial/case-{task['index']:02}.json.gz")
                    phases[label].append({**task, **found}); evaluated += found['ticks']
            r['gate'] = gate(phases['initial'], phases['final'], parent); r['trajectories'] = phases
    comparisons = []
    for seed in stage.SEEDS:
        dirs = [a/f'{phase}/{arm}/seed-{seed}' for arm in stage.ARMS]
        starts = [load(p/'checkpoint-0000.pt.gz') for p in dirs]
        for key in ['model', 'optimizer', 'collector', 'update', 'history', 'torch_rng', 'python_rng']:
            assert equal_tree(starts[0][key], starts[1][key])
        first = [load(p/'rollout-0001.pt.gz') for p in dirs]
        assert equal_tree(first[0], {k:v[:256] for k,v in first[1].items()})
        # The current arm must reproduce LF-069 at the matched endpoint, including history.
        old = a/'inputs/control-reference'/('preflight' if preflight else 'pilot')/f'seed-{seed}'
        old_final = load(old/f"checkpoint-{stage.config(seed,'current',preflight)['updates']:04}.pt.gz")
        now = load(dirs[0]/f"checkpoint-{old_final['update']:04}.pt.gz")
        for key in ['model', 'optimizer', 'collector', 'update', 'history', 'torch_rng', 'python_rng']:
            assert equal_tree(now[key], old_final[key]), (seed, key)
        comparisons.append({'seed': seed, 'initial_states_exact': True, 'first_256_ticks_exact': True, 'current_matches_LF069': True})
    result = {'experiment_id': a.name, 'records': records, 'paired_comparisons': comparisons,
        'verification_passed': True, 'exact_resumes': 6,
        'training_ticks': sum((stage.config(r['seed'], r['arm'], preflight)['updates']+1)*stage.config(r['seed'], r['arm'], preflight)['rollout_ticks']*6 for r in records),
        'optimizer_steps': sum(r['optimizer_steps']+r['repeated_optimizer_steps'] for r in records),
        'restore_ticks': sum(r['restore_ticks'] for r in records),
        'neutral_setup_ticks': sum(r['neutral_setups'] for r in records)+(0 if preflight else 1248),
        'likelihood_comparisons': sum(r['likelihood_comparisons'] for r in records),
        'protected_stopping_rows': sum(r['protected_stopping_rows'] for r in records),
        'verified_gae_rows': sum(r['target_verification']['rows'] for r in records),
        'evaluation_ticks': evaluated, 'evaluation_episodes': 0 if preflight else 1248,
        'original_parent_traces_exact': parent_exact, 'zero_command_traces_exact': zero_retained,
        'confirmation_used': False, 'model_promoted': False, 'navigation_qualified': False}
    if preflight:
        result['passed'] = True; target = 'preflight-results.json'
        assert result['training_ticks'] == 59904 and result['optimizer_steps'] <= 504
    else:
        pre = read(a/'preflight-results.json'); assert pre['passed']
        result['main_training_ticks'] = 774144
        result['qualified_seeds'] = {arm: sum(r['gate']['passed'] for r in records if r['arm'] == arm) for arm in stage.ARMS}
        result['passed'] = result['qualified_seeds']['large'] >= 2
        gains = []
        for seed in stage.SEEDS:
            one, two = [next(r for r in records if r['seed'] == seed and r['arm'] == arm) for arm in stage.ARMS]
            gains.append({'seed': seed, 'retained_gain': two['gate']['tracking_retained']-one['gate']['tracking_retained'],
                          'physical_failure_change': two['gate']['physical_failures']-one['gate']['physical_failures']})
        result['paired_flight_gains'] = gains
        result['meaningful_retention_improvement'] = sum(v['retained_gain'] >= 4 and v['physical_failure_change'] <= 0 for v in gains) >= 2
        assert parent_exact == 672 and zero_retained == 480 and evaluated <= 1123200
        for key in ['training_ticks', 'optimizer_steps', 'restore_ticks', 'neutral_setup_ticks', 'likelihood_comparisons', 'protected_stopping_rows', 'verified_gae_rows', 'exact_resumes']:
            result[key] += pre[key]
        assert result['training_ticks'] == 857088 and result['optimizer_steps'] <= 8208 and result['restore_ticks'] <= 48000
        target = 'results.json'
    result['seconds'] = time.monotonic()-start; assert result['seconds'] < 600
    atomic_json(a/target, result)


if __name__ == '__main__':
    p = argparse.ArgumentParser(); p.add_argument('--archive', type=Path, required=True); p.add_argument('--preflight', action='store_true')
    args = p.parse_args(); torch.set_num_threads(1); torch.use_deterministic_algorithms(True)
    verify(args.archive, args.preflight)
