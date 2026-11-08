"""LF-083: supervised fit of the navigation reference law into the C3 encoder, refined on the network's own states.

The law in `reference.py` labels every visited observation with the command it
would issue; the exported network issues every command in the refinement
rounds and the evaluations, with the motor network flying underneath. There is
no scripted mask, override, fallback or takeover, and the law is a training
target, not a ceiling.
"""
import argparse
import json
import math
import multiprocessing
from pathlib import Path
import random
import time

import torch
from torch.nn import functional as F
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from . import reference
from .model import COMMAND_CONTRACT, FORMAT_VERSION, MAX_SPEED, NavigationModel, commands, flatten, tensor

KIND = 'navigation-teacher-fit-v1'
STATIC_FAMILIES = ('clear', 'obstructed', 'altered')
# LF-082's development generation seed, a held-out seed never used for fitting,
# and one fresh training seed per collection round.
DEVELOPMENT_SEED = 7700002
HELD_OUT_SEED = 7700003
TRAINING_SEEDS = (7710001, 7710002, 7710003, 7710004)
HOLD = reference.LAW['hold']
# Random held commands spread the visited states beyond the law's own path; the
# labels stay the law's, so a burst never changes a target.
PERTURBATION = {'rate': 1. / 30., 'max_decisions': 6}
PARITY_LIMIT = 1e-4
SOURCES = ('fit.py', 'model.py', 'reference.py', 'route.py')


def static_rows(rows):
    rows = [r for r in rows if r['family'] in STATIC_FAMILIES]
    for index, row in enumerate(rows):
        row['index'] = index
    return rows


def label(inputs, bearings, edges):
    command, info = reference.command(inputs, bearings, reference.LAW, edges)
    return [command['heading_sin'], command['heading_cos'], command['speed']], command, info['distance']


def collect(bridge, rows, rng, driver='law', perturbation=PERTURBATION, log=None):
    """Observations visited under the perturbed law or under the loaded network, each labelled by the law.

    Returns float tensors: inputs, labels (pair and speed) and per-row family
    index and goal distance, plus the episode outcomes. A network command the
    bridge refuses ends that episode as `invalid_command`.
    """
    bearings = bridge.contract['schema']['ray_bearings']; edges = bridge.contract['schema']['radial_edges']
    x = []; y = []; meta = []; outcomes = {}
    for row in rows:
        item = bridge.call('reset', tasks=[row['task']])['items'][0]
        burst = 0; burst_command = None; outcome = None
        while item['progress']['outcome'] is None:
            target, command, distance = label(item['inputs'], bearings, edges)
            x.append(torch.tensor(flatten(item['inputs']), dtype=torch.float32)); y.append(target)
            meta.append([STATIC_FAMILIES.index(row['family']), distance])
            if driver == 'law' and perturbation and burst == 0 and rng.random() < perturbation['rate']:
                burst = rng.randint(1, perturbation['max_decisions']); angle = rng.uniform(-math.pi, math.pi)
                burst_command = {'heading_sin': math.sin(angle), 'heading_cos': math.cos(angle), 'speed': rng.uniform(0., MAX_SPEED)}
            if burst > 0:
                burst -= 1; item = bridge.call('step_command', commands=[burst_command], hold=HOLD)['items'][0]
            elif driver == 'law':
                item = bridge.call('step_command', commands=[command], hold=HOLD)['items'][0]
            else:
                try:
                    item = bridge.call('step_policy', hold=HOLD)['items'][0]
                except RuntimeError as error:
                    if 'invalid navigation command' not in str(error):
                        raise
                    outcome = 'invalid_command'; break
        outcome = outcome or item['progress']['outcome']; outcomes[outcome] = outcomes.get(outcome, 0) + 1
        if log:
            log('  %-10s task %3d %-9s %4d ticks' % (row['family'], row['index'], outcome, item['progress']['elapsed_ticks']))
    return torch.stack(x), torch.tensor(y, dtype=torch.float32), torch.tensor(meta, dtype=torch.float32), outcomes


def collect_shard(job):
    """Worker entry: one bridge per process, rows saved as a tensor shard."""
    torch.set_num_threads(1)
    with Bridge(job['bridge']) as bridge:
        bridge.call('load_motor_policy', path=job['motor'])
        if job.get('policy'):
            bridge.call('load_navigation_policy', path=job['policy'])
        x, y, meta, outcomes = collect(bridge, job['rows'], random.Random(job['seed']), 'policy' if job.get('policy') else 'law')
    torch.save({'x': x, 'y': y, 'meta': meta, 'outcomes': outcomes}, job['out'])
    return {'rows': len(x), 'outcomes': outcomes, 'shard': str(job['out'])}


def collect_parallel(bridge_path, motor, policy, rows, seed, directory, workers, name):
    """Splits the tasks over worker processes; returns the concatenated tensors and merged outcomes."""
    directory.mkdir(parents=True, exist_ok=True)
    chunks = max(1, min(len(rows), workers * 3))
    jobs = [{'bridge': bridge_path, 'motor': motor, 'policy': policy, 'rows': rows[i::chunks], 'seed': seed * 1000 + i,
             'out': directory / f'{name}-{i:02}.pt'} for i in range(chunks) if rows[i::chunks]]
    with multiprocessing.get_context('spawn').Pool(min(workers, len(jobs))) as pool:
        reports = pool.map(collect_shard, jobs)
    shards = [torch.load(job['out'], weights_only=True) for job in jobs]
    outcomes = {}
    for report in reports:
        for key, count in report['outcomes'].items():
            outcomes[key] = outcomes.get(key, 0) + count
    return (torch.cat([s['x'] for s in shards]), torch.cat([s['y'] for s in shards]), torch.cat([s['meta'] for s in shards]), outcomes)


def loss_rows(actor, target):
    """Per-row squared error on the tanh direction pair plus cross-entropy on the speed logit against the normalized speed."""
    pair = ((torch.tanh(actor[:, :2]) - target[:, :2]) ** 2).sum(1)
    speed = F.binary_cross_entropy_with_logits(actor[:, 2], target[:, 2] / MAX_SPEED, reduction='none')
    return pair + speed


def loss_terms(actor, target):
    return loss_rows(actor, target).mean()


def command_error(actor, target):
    """Per-row absolute heading error in radians, wrapped, and absolute speed error in units."""
    predicted = commands(actor)
    heading = torch.atan2(predicted[:, 0], predicted[:, 1]) - torch.atan2(target[:, 0], target[:, 1])
    return torch.atan2(heading.sin(), heading.cos()).abs(), (predicted[:, 2] - target[:, 2]).abs()


def error_summary(model, x, y, meta, batch=4096):
    angles = []; speeds = []
    with torch.no_grad():
        for i in range(0, len(x), batch):
            angle, speed = command_error(model(x[i:i + batch])[0], y[i:i + batch]); angles.append(angle); speeds.append(speed)
    angle = torch.cat(angles); speed = torch.cat(speeds)

    def block(mask):
        return {'rows': int(mask.sum()), 'heading_degrees': math.degrees(angle[mask].mean().item()) if mask.any() else None,
                'speed': speed[mask].mean().item() if mask.any() else None}
    summary = block(torch.ones(len(x), dtype=torch.bool))
    summary['near_goal'] = block(meta[:, 1] < reference.LAW['hold_radius'])
    summary['by_family'] = {family: block(meta[:, 0] == index) for index, family in enumerate(STATIC_FAMILIES)}
    return summary


def fit(model, x, y, meta, steps, batch, learning_rate, generator, log=print):
    """Supervised regression of the encoder and actor; the critic and deviations stay at initialization."""
    parameters = [p for name, p in model.named_parameters() if not name.startswith('critic') and name != 'log_std']
    optimizer = torch.optim.Adam(parameters, lr=learning_rate)
    schedule = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, steps)
    probe = torch.randperm(len(x), generator=generator)[:8192]
    for step in range(1, steps + 1):
        index = torch.randint(len(x), (batch,), generator=generator)
        actor, _ = model(x[index])
        loss = loss_terms(actor, y[index])
        optimizer.zero_grad(); loss.backward(); optimizer.step(); schedule.step()
        if step % max(steps // 10, 1) == 0 or step == steps:
            with torch.no_grad():
                angle, speed = command_error(model(x[probe])[0], y[probe])
            log('  fit step %d/%d loss %.5f probe heading %.2f deg speed %.3f' % (step, steps, loss.item(), math.degrees(angle.mean()), speed.mean()))
    return error_summary(model, x, y, meta)


def evaluate(bridge, model, rows, directory, log=None, prepare=None):
    """Closed-loop evaluation of the loaded network with native/Python parity on each task's first observation; a row marked to settle is first brought to rest by `prepare`."""
    directory.mkdir(); results = []
    for row in rows:
        item = bridge.call('reset', tasks=[row['task']])['items'][0]
        if prepare is not None and row.get('settle'):
            item = prepare(bridge, item)
        native = bridge.call('infer_navigation', inputs=[item['inputs']])['outputs'][0]
        with torch.no_grad():
            actor = model(tensor([item]))[0][0]
        latents = native['latents'] + ([native['sense_latent']] if native.get('sense_latent') is not None else [])
        if len(latents) != len(actor):
            raise ValueError('native and Python head counts differ')
        parity = max(abs(a - b) for a, b in zip(actor.tolist(), latents))
        if parity > PARITY_LIMIT:
            raise ValueError('native and Python inference disagree')
        decisions = 0; max_speed = 0.; outcome = None
        while item['progress']['outcome'] is None:
            try:
                result = bridge.call('step_policy', hold=HOLD)
            except RuntimeError as error:
                if 'invalid navigation command' not in str(error):
                    raise
                outcome = 'invalid_command'; break
            item = result['items'][0]; decisions += 1; max_speed = max(max_speed, result['commands'][0]['speed'])
        f = item['frame']; outcome = outcome or item['progress']['outcome']
        atomic_json(directory / f"case-{row['index']:03}.json", bridge.call('state')['traces'][0])
        results.append({'index': row['index'], 'family': row['family'], 'recipe': row['recipe'], 'outcome': outcome,
                        'ticks': item['progress']['elapsed_ticks'], 'decisions': decisions,
                        'final_distance': math.hypot(f['target_position']['x'] - f['center_of_mass']['x'], f['target_position']['y'] - f['center_of_mass']['y']),
                        'final_relative_speed': math.hypot(f['velocity']['x'] - f['target_velocity']['x'], f['velocity']['y'] - f['target_velocity']['y']),
                        'max_command_speed': max_speed, 'deadline_ticks': row['task']['deadline_ticks'], 'parity': parity})
        if log:
            log('  %-10s %-28s %-9s ticks %4d of %4d  final distance %6.1f' % (
                row['family'], row['recipe'][:28], outcome, results[-1]['ticks'], row['task']['deadline_ticks'], results[-1]['final_distance']))
    atomic_json(directory / 'results.json', results)
    return results


def summarize(results):
    summary = reference.summarize(results)
    summary['max_parity'] = max((r.get('parity', 0.) for r in results), default=0.)
    return summary


def development_rows(bridge, archive):
    """LF-082's static development tasks, regenerated and checked against the archived set."""
    rows = static_rows(reference.task_sets(bridge, DEVELOPMENT_SEED, 24))
    expected = [r['task'] for r in json.loads(Path(archive).read_text()) if r['family'] in STATIC_FAMILIES]
    if [r['task'] for r in rows] != expected:
        raise ValueError('regenerated development tasks differ from the archived LF-082 set')
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, required=True); p.add_argument('--bridge', default=reference.DEFAULT_BRIDGE)
    p.add_argument('--motor', default=reference.DEFAULT_MOTOR)
    p.add_argument('--archived-development', type=Path, default=Path('data/training-experiments/learned-flight-navigation-reference-v3-20261025/tasks.json'))
    p.add_argument('--seed', type=int, default=1); p.add_argument('--per-family', type=int, default=120)
    p.add_argument('--rounds', type=int, default=2); p.add_argument('--round-per-family', type=int, default=60)
    p.add_argument('--steps', type=int, default=8000); p.add_argument('--round-steps', type=int, default=4000)
    p.add_argument('--batch', type=int, default=512); p.add_argument('--learning-rate', type=float, default=1e-3)
    p.add_argument('--workers', type=int, default=12); p.add_argument('--threads', type=int, default=12)
    args = p.parse_args()
    torch.set_num_threads(args.threads); torch.manual_seed(args.seed); random.seed(args.seed)
    generator = torch.Generator().manual_seed(args.seed)
    args.out.mkdir(parents=False); started = time.monotonic()
    bridge_path = str(Path(args.bridge).resolve()); motor = str(Path(args.motor).resolve())
    plan = {'kind': KIND, 'command_contract': COMMAND_CONTRACT, 'format_version': FORMAT_VERSION, 'law': reference.LAW, 'hold': HOLD,
            'perturbation': PERTURBATION, 'seeds': {'development': DEVELOPMENT_SEED, 'held_out': HELD_OUT_SEED, 'training': TRAINING_SEEDS[:args.rounds + 1]},
            'bridge_sha256': sha(bridge_path), 'motor_sha256': sha(motor),
            'sources': {name: sha(Path(__file__).parent / name) for name in SOURCES},
            'arguments': {k: (str(v) if isinstance(v, Path) else v) for k, v in vars(args).items()}}
    atomic_json(args.out / 'plan.json', plan)
    log_lines = []

    def log(line):
        print(line, flush=True); log_lines.append(line)
    with Bridge(bridge_path) as bridge:
        bridge.call('load_motor_policy', path=motor)
        model = NavigationModel(bridge.contract['schema'])
        development = development_rows(bridge, args.archived_development)
        held_out = static_rows(reference.task_sets(bridge, HELD_OUT_SEED, 24))
        training = [static_rows(reference.task_sets(bridge, TRAINING_SEEDS[r], args.per_family if r == 0 else args.round_per_family))
                    for r in range(args.rounds + 1)]
        atomic_json(args.out / 'tasks-development.json', development); atomic_json(args.out / 'tasks-held-out.json', held_out)
        for r, rows in enumerate(training):
            atomic_json(args.out / f'tasks-training-round-{r}.json', rows)
        log('tasks: development %d, held out %d, training %s (%.0f s)' % (len(development), len(held_out), [len(t) for t in training], time.monotonic() - started))
        x, y, meta, outcomes = collect_parallel(bridge_path, motor, None, training[0], args.seed, args.out / 'shards', args.workers, 'round-0')
        log('round 0: %d rows from %d perturbed law episodes, outcomes %s (%.0f s)' % (len(x), len(training[0]), outcomes, time.monotonic() - started))
        errors = fit(model, x, y, meta, args.steps, args.batch, args.learning_rate, generator, log)
        rounds = [{'rows': len(x), 'episodes': len(training[0]), 'law_outcomes': outcomes, 'errors': errors}]
        log('round 0 errors: %s' % json.dumps(errors))
        for r in range(1, args.rounds + 1):
            export = args.out / f'policy-round-{r - 1}.json'; model.export(export)
            visited, labels, visited_meta, outcomes = collect_parallel(bridge_path, motor, str(export.resolve()), training[r], args.seed + r, args.out / 'shards', args.workers, f'round-{r}')
            log('round %d: %d rows from %d network episodes, outcomes %s (%.0f s)' % (r, len(visited), len(training[r]), outcomes, time.monotonic() - started))
            x = torch.cat((x, visited)); y = torch.cat((y, labels)); meta = torch.cat((meta, visited_meta))
            errors = fit(model, x, y, meta, args.round_steps, args.batch, args.learning_rate / 2, generator, log)
            rounds.append({'rows': len(x), 'episodes': len(training[r]), 'network_outcomes': outcomes, 'errors': errors})
            log('round %d errors: %s' % (r, json.dumps(errors)))
        export = args.out / 'policy-final.json'; model.export(export)
        torch.save({'kind': KIND, 'state': model.state_dict(), 'law': reference.LAW, 'input_scaling': model.input_scaling}, args.out / 'checkpoint-final.pt')
        bridge.call('load_navigation_policy', path=str(export.resolve()))
        development_results = summarize(evaluate(bridge, model, development, args.out / 'evaluation-development', log))
        log('development set: %d/%d passed, max parity %.2e (%.0f s)' % (development_results['passed'], development_results['cases'], development_results['max_parity'], time.monotonic() - started))
        held_out_results = summarize(evaluate(bridge, model, held_out, args.out / 'evaluation-held-out', log))
        log('held-out set: %d/%d passed (%.0f s)' % (held_out_results['passed'], held_out_results['cases'], time.monotonic() - started))
        law_results = reference.run(bridge, held_out)
        atomic_json(args.out / 'reference-held-out.json', law_results); law_summary = reference.summarize(law_results)
        log('law on the held-out set: %d/%d passed (%.0f s)' % (law_summary['passed'], law_summary['cases'], time.monotonic() - started))
    summary = {'kind': KIND, 'input_scaling': model.input_scaling, 'rounds': rounds, 'development': development_results,
               'held_out': held_out_results, 'law_held_out': law_summary, 'seconds': time.monotonic() - started, 'policy_sha256': sha(export)}
    atomic_json(args.out / 'summary.json', summary); (args.out / 'log.txt').write_text('\n'.join(log_lines) + '\n')
    log('done in %.1f s' % summary['seconds'])


if __name__ == '__main__':
    main()
