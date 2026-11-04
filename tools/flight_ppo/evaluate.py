"""Physical evaluation with explicit action decoding and isolated sampling RNG."""
import argparse
from collections import Counter
from contextlib import nullcontext
import gzip
import hashlib
import json
from pathlib import Path
import time
import torch
from .bridge import Bridge
from .execution import DETERMINISTIC, SAMPLED, evaluation_spec, execution_identity, episode_seed, load_export, sampled_command
from .model import tensor


def summarize(results):
    def counts(rows):
        outcomes = dict(Counter(r['outcome'] for r in rows))
        return {'attempts': len(rows), 'outcomes': outcomes,
                'successes': outcomes.get('success', 0),
                'physical_failures': sum(n for k,n in outcomes.items() if k not in ['success','deadline','hold_broken']),
                **({'hold_failures': outcomes['hold_broken']} if 'hold_broken' in outcomes else {})}
    return {**counts(results), 'by_repeat': {
        str(repeat): counts([r for r in results if r.get('repeat', 0) == repeat])
        for repeat in sorted({r.get('repeat', 0) for r in results})},
        'by_recipe': {recipe: counts([r for r in results if r['recipe'] == recipe])
                      for recipe in sorted({r['recipe'] for r in results})}}


def evaluate(executable, policy, candidates, out, deadline=None, *, execution=None, learner_seed=None):
    spec = evaluation_spec(execution)
    sampled = spec['mode'] == SAMPLED
    candidates = list(candidates)
    if not candidates:
        raise ValueError('evaluation requires candidates')
    names = [f"{c['family']}-{c['index']}" for c in candidates]
    if len(set(names)) != len(names) or any(Path(n).name != n for n in names):
        raise ValueError('duplicate or unsafe evaluation trace names')
    if sampled:
        if len({c['index'] for c in candidates}) != len(candidates):
            raise ValueError('sampled evaluation requires unique candidate indices')
        for c in candidates:
            episode_seed(spec, learner_seed, spec['repeats']-1, c['index'])
    contract = execution_identity(spec)
    out = Path(out)
    out.mkdir()
    metadata = out/'metadata'
    metadata.mkdir()
    policy = Path(policy).resolve()
    (metadata/'execution.json').write_text(json.dumps({
        'specification': spec, 'contract': contract, 'learner_seed': learner_seed,
        'policy_sha256': hashlib.sha256(policy.read_bytes()).hexdigest(),
        'torch': str(torch.__version__), 'torch_threads': torch.get_num_threads(),
        'deterministic_algorithms': torch.are_deterministic_algorithms_enabled(), 'cpu_rng_isolated': sampled}, indent=2)+'\n')
    results = []
    # Includes export loading, every episode and exception unwinding. Evaluation
    # cannot consume the model initialization or collector's future random stream.
    context = torch.random.fork_rng(devices=[]) if sampled else nullcontext()
    with context, Bridge(executable) as bridge:
        try:
            bridge.call('load_policy', path=str(policy))
            model = load_export(policy, bridge.contract['schema']) if sampled else None
            for repeat in range(spec['repeats']):
                directory = out/f'repeat-{repeat}' if sampled else out
                if sampled:
                    directory.mkdir()
                    (directory/'policy').mkdir()
                repeat_results = []
                for candidate in candidates:
                    seed = episode_seed(spec, learner_seed, repeat, candidate['index']) if sampled else None
                    if sampled:
                        torch.manual_seed(seed)
                    item = bridge.call('reset', tasks=[candidate['task']])['items'][0]
                    first = item['frame']
                    policy_rows = []
                    while item['progress']['outcome'] is None:
                        if deadline is not None and time.monotonic() > deadline:
                            raise TimeoutError('evaluation exceeded declared wall budget')
                        if sampled:
                            with torch.inference_mode():
                                actor, _ = model(tensor([item]))
                                commands, latent = sampled_command(model, actor)
                                policy_rows.append({'actor': actor[0].tolist(),
                                    'mean_latent_controls': {'steering': float(actor[0,0].tanh()), 'thrust': float(actor[0,1].sigmoid()), 'brake': float(actor[0,2].sigmoid())},
                                    'latent_std': model.log_std.clamp(-5,2).exp().tolist(),
                                    'latent': latent[0], 'action': commands[0]})
                            item = bridge.call('step', actions=commands)['items'][0]
                        else:
                            item = bridge.call('step_policy')['items'][0]
                    if item['progress']['outcome'] == 'environment_error':
                        raise RuntimeError('evaluation infrastructure failure')
                    trace = bridge.call('state')['traces'][0]
                    name = f"{candidate['family']}-{candidate['index']}"
                    (directory/f'{name}.json').write_text(json.dumps(trace, allow_nan=False))
                    if sampled:
                        with gzip.open(directory/'policy'/f'{name}.json.gz','wt') as f:
                            json.dump(policy_rows, f, allow_nan=False)
                    row = {'family': candidate['family'], 'recipe': candidate['recipe'], 'index': candidate['index'],
                           'outcome': item['progress']['outcome'], 'ticks': item['progress']['elapsed_ticks'],
                           'initial_errors': first['errors'], 'final_errors': item['frame']['errors'],
                           'damage': any(s['evidence']['damage'] for s in trace['steps']),
                           'collision': any(s['evidence']['collision'] for s in trace['steps'])}
                    if sampled:
                        row.update(repeat=repeat, rng_seed=seed, execution_mode=SAMPLED)
                    results.append(row)
                    repeat_results.append(row)
                if sampled:
                    (directory/'results.json').write_text(json.dumps(repeat_results, indent=2)+'\n')
        except Exception as error:
            (metadata/'interruption.json').write_text(json.dumps({
                'error': str(error), 'completed_episodes': results, 'specification': spec}, indent=2)+'\n')
            raise
    (out/'results.json').write_text(json.dumps(results, indent=2)+'\n')
    (metadata/'summary.json').write_text(json.dumps(summarize(results), indent=2)+'\n')
    return results


def selection_score(results):
    # Both checkpoints use the same tasks/repeats. Reward is not a selection metric.
    return (sum(r['outcome'] == 'success' for r in results),
            -sum(r['outcome'] not in ('success', 'deadline', 'hold_broken') for r in results),
            -sum(r['final_errors']['position'] for r in results)/len(results))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bridge', required=True)
    parser.add_argument('--policy', type=Path, required=True)
    parser.add_argument('--admission', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--mode', choices=[DETERMINISTIC, SAMPLED], default=DETERMINISTIC)
    parser.add_argument('--repeats', type=int, default=1)
    parser.add_argument('--seed-base', type=int)
    parser.add_argument('--learner-seed', type=int)
    parser.add_argument('--wall-seconds', type=float, default=1200.)
    args = parser.parse_args()
    if not 0 < args.wall_seconds < float('inf'):
        parser.error('wall seconds must be positive and finite')
    spec = {'mode': args.mode, 'repeats': args.repeats}
    if args.seed_base is not None:
        spec['seed_base'] = args.seed_base
    spec = evaluation_spec(spec)
    batch = json.loads(args.admission.read_text())
    if not batch['complete']:
        raise ValueError('incomplete task admission')
    candidates = [r['candidate'] for r in batch['attempts'] if r['admitted']]
    torch.set_num_threads(1)
    torch.use_deterministic_algorithms(True)
    results = evaluate(args.bridge, args.policy, candidates, args.out,
                       time.monotonic()+args.wall_seconds, execution=spec, learner_seed=args.learner_seed)
    print(json.dumps(summarize(results)))


if __name__ == '__main__':
    main()
