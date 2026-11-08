"""Supervised fit of the v2 navigation reference law into the C3 encoder, for the match envelope.

The LF-083 pipeline on the v2 contract: the law in `reference_v2.py` labels
every visited observation with the planet-frame command it would issue, the
exported network issues every command in the refinement rounds and the
evaluations, and the v2 motor network flies underneath. Tasks are the
match-transfer family on generated arenas plus the practice generator's four
families with their seeded gravity and spin, and optionally the long
transfers beyond 600 units, drawn from the same seeds, which cross the arena
as a mission's legs do. No scripted mask, override, fallback or takeover; the
law is a training target, not a ceiling.
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
from tools.flight_ppo.model import MATCH_SCALING, flatten
from . import fit as fit_v1, reference, reference_v2
from .model import CONTRACTS, SENSE_THRESHOLD, NavigationModel, commands, tensor

KIND = 'navigation-teacher-fit-v2'
CONTRACT = 2
MAX_SPEED = CONTRACTS[CONTRACT]['max_speed']
FAMILIES = ('transfer', 'clear', 'moving', 'obstructed', 'altered', 'long_transfer', 'grounded')
HOLD = reference_v2.LAW['hold']
PERTURBATION = {'rate': 1. / 30., 'max_decisions': 6}
# Transfer and practice seeds for the development, held-out and training sets.
SEEDS = {'development': {'transfer': 7700101, 'generated': 7700002},
         'held_out': {'transfer': 7700102, 'generated': 7700003},
         'training': [{'transfer': 7700201 + r, 'generated': 7710001 + r} for r in range(5)]}
SOURCES = ('fit_v2.py', 'fit.py', 'model.py', 'reference_v2.py', 'reference.py', 'route.py')


def label(inputs, bearings, edges, law=reference_v2.LAW):
    """The law's target, command, goal distance and whether its global route rounds a body; under the v3 law the target carries the turn sense."""
    trace = {}
    command, info = reference_v2.command(inputs, bearings, law, edges, trace)
    goal, routed = trace['goal'], trace['routed']
    detour = math.hypot(routed[0] - goal[0], routed[1] - goal[1]) > 1.
    target = [command['heading_sin'], command['heading_cos'], command['speed']]
    if 'advisory' in command:
        target.append(command['advisory'][0])
    return target, command, info['distance'], detour


def law_for(contract):
    return reference_v2.LAW_V3 if contract == 3 else reference_v2.LAW


def task_rows(bridge, seeds, transfers, per_family, long_transfers=0, grounded=0):
    rows = reference_v2.transfer_sets(bridge, seeds['transfer'], transfers) if transfers else []
    for row in reference_v2.generated_sets(bridge, seeds['generated'], per_family) if per_family else []:
        row['index'] = len(rows); rows.append(row)
    for row in reference_v2.transfer_sets(bridge, seeds['transfer'], long_transfers, distances='long') if long_transfers else []:
        row['index'] = len(rows); rows.append(row)
    for row in reference_v2.grounded_sets(seeds['generated'] + 7, grounded) if grounded else []:
        row['index'] = len(rows); rows.append(row)
    return rows


def collect(bridge, rows, rng, driver='law', perturbation=PERTURBATION, log=None, law=reference_v2.LAW):
    """Observations visited under the perturbed law or under the loaded network, each labelled by the law; a row marked to settle begins on the ground."""
    bearings = bridge.contract['schema']['ray_bearings']; edges = bridge.contract['schema']['radial_edges']
    x = []; y = []; meta = []; outcomes = {}
    for row in rows:
        item = bridge.call('reset', tasks=[row['task']])['items'][0]
        if row.get('settle'):
            item = reference_v2.settle(bridge, item)
        burst = 0; burst_command = None; outcome = None
        while item['progress']['outcome'] is None:
            target, command, distance, detour = label(item['inputs'], bearings, edges, law)
            x.append(torch.tensor(flatten(item['inputs']), dtype=torch.float32)); y.append(target)
            meta.append([FAMILIES.index(row['family']), distance, float(detour)])
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
    torch.set_num_threads(1)
    with Bridge(job['bridge']) as bridge:
        bridge.call('load_motor_policy', path=job['motor'])
        if job.get('policy'):
            bridge.call('load_navigation_policy', path=job['policy'])
        x, y, meta, outcomes = collect(bridge, job['rows'], random.Random(job['seed']), 'policy' if job.get('policy') else 'law',
                                       law=law_for(job.get('contract', CONTRACT)))
    torch.save({'x': x, 'y': y, 'meta': meta, 'outcomes': outcomes}, job['out'])
    return {'rows': len(x), 'outcomes': outcomes, 'shard': str(job['out'])}


def collect_parallel(bridge_path, motor, policy, rows, seed, directory, workers, name, contract=CONTRACT):
    directory.mkdir(parents=True, exist_ok=True)
    chunks = max(1, min(len(rows), workers * 3))
    jobs = [{'bridge': bridge_path, 'motor': motor, 'policy': policy, 'rows': rows[i::chunks], 'seed': seed * 1000 + i,
             'out': directory / f'{name}-{i:02}.pt', 'contract': contract} for i in range(chunks) if rows[i::chunks]]
    with multiprocessing.get_context('spawn').Pool(min(workers, len(jobs))) as pool:
        reports = pool.map(collect_shard, jobs)
    shards = [torch.load(job['out'], weights_only=True) for job in jobs]
    outcomes = {}
    for report in reports:
        for key, count in report['outcomes'].items():
            outcomes[key] = outcomes.get(key, 0) + count
    return (torch.cat([s['x'] for s in shards]), torch.cat([s['y'] for s in shards]), torch.cat([s['meta'] for s in shards]), outcomes)


# Speed errors in units are squared over this scale, so a unit of error near
# a goal weighs as much as a few degrees of heading; a stop needs only a speed
# under half the contract's deadband.
SPEED_LOSS_SCALE = 10.
STOP_TARGET = .25


def loss_rows(actor, target):
    """Squared error on the tanh pair, cross-entropy on the speed logit against speed over the cap, the speed error in units and, with a fourth head, squared error on the tanh sense."""
    pair = (torch.tanh(actor[:, :2]) - target[:, :2]).square().sum(-1)
    logit = F.binary_cross_entropy_with_logits(actor[:, 2], target[:, 2] / MAX_SPEED, reduction='none')
    predicted = MAX_SPEED * torch.sigmoid(actor[:, 2])
    units = torch.where(target[:, 2] <= 0., F.relu(predicted - STOP_TARGET), predicted - target[:, 2]) / SPEED_LOSS_SCALE
    loss = pair + logit + units.square()
    if actor.shape[1] > 3:
        # The advised rows are a few percent of the data; unweighted, the
        # head sits at free and the threshold never trips.
        weight = torch.where(target[:, 3] != 0., SENSE_WEIGHT, 1.)
        loss = loss + weight * (torch.tanh(actor[:, 3]) - target[:, 3]).square()
    return loss


def sense_agreement(actor, target):
    """Whether the fourth head's sense, through its threshold, is the target's."""
    sense = torch.tanh(actor[:, 3])
    predicted = torch.where(sense > SENSE_THRESHOLD, 1., torch.where(sense < -SENSE_THRESHOLD, -1., 0.))
    return predicted == target[:, 3]


def command_error(actor, target):
    predicted = commands(actor, MAX_SPEED)
    heading = torch.atan2(predicted[:, 0], predicted[:, 1]) - torch.atan2(target[:, 0], target[:, 1])
    return torch.atan2(heading.sin(), heading.cos()).abs(), (predicted[:, 2] - target[:, 2]).abs()


def error_summary(model, x, y, meta, batch=4096):
    angles = []; speeds = []; agreements = []
    with torch.no_grad():
        for i in range(0, len(x), batch):
            actor = model(x[i:i + batch])[0]
            angle, speed = command_error(actor, y[i:i + batch]); angles.append(angle); speeds.append(speed)
            if actor.shape[1] > 3:
                agreements.append(sense_agreement(actor, y[i:i + batch]))
    angle = torch.cat(angles); speed = torch.cat(speeds)

    def block(mask):
        return {'rows': int(mask.sum()), 'heading_degrees': math.degrees(angle[mask].mean().item()) if mask.any() else None,
                'speed': speed[mask].mean().item() if mask.any() else None}
    summary = block(torch.ones(len(x), dtype=torch.bool))
    if agreements:
        agreement = torch.cat(agreements); advised = y[:, 3] != 0.
        summary['sense'] = {'rows_advised': int(advised.sum()), 'agreement': agreement.float().mean().item(),
                            'agreement_advised': agreement[advised].float().mean().item() if advised.any() else None}
    summary['near_goal'] = block(meta[:, 1] < reference_v2.LAW['hold_radius'])
    summary['routed'] = block(meta[:, 2] > 0.)
    summary['by_family'] = {family: block(meta[:, 0] == index) for index, family in enumerate(FAMILIES)}
    return summary


# Rows where the law sets a sense weigh this much more in the sense term.
SENSE_WEIGHT = 12.

# Rows inside the hold radius weigh this much more: the arrival rule is
# decided there, by commands the deadband and the tolerances leave little
# room for.
NEAR_GOAL_WEIGHT = 3.


def fit(model, x, y, meta, steps, batch, learning_rate, generator, log=print, routed_weight=1.):
    """Rows whose global route rounds a body weigh routed_weight, since they are few and the law's side there is the hardest command to copy."""
    parameters = [p for name, p in model.named_parameters() if not name.startswith('critic') and name != 'log_std']
    optimizer = torch.optim.Adam(parameters, lr=learning_rate)
    schedule = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, steps)
    probe = torch.randperm(len(x), generator=generator)[:8192]
    near = torch.where(meta[:, 1] < reference_v2.LAW['hold_radius'], NEAR_GOAL_WEIGHT, 1.)
    near = torch.where(meta[:, 2] > 0., routed_weight, near)
    for step in range(1, steps + 1):
        index = torch.randint(len(x), (batch,), generator=generator)
        actor, _ = model(x[index])
        loss = (loss_rows(actor, y[index]) * near[index]).mean()
        optimizer.zero_grad(); loss.backward(); optimizer.step(); schedule.step()
        if step % max(steps // 10, 1) == 0 or step == steps:
            with torch.no_grad():
                angle, speed = command_error(model(x[probe])[0], y[probe])
            log('  fit step %d/%d loss %.5f probe heading %.2f deg speed %.3f' % (step, steps, loss.item(), math.degrees(angle.mean()), speed.mean()))
    return error_summary(model, x, y, meta)


def main():
    global SENSE_WEIGHT
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, required=True); p.add_argument('--bridge', default=reference.DEFAULT_BRIDGE)
    p.add_argument('--motor', required=True, help='the v2 planet-frame motor export')
    p.add_argument('--seed', type=int, default=1); p.add_argument('--scaling', default=MATCH_SCALING)
    p.add_argument('--transfers', type=int, default=96); p.add_argument('--per-family', type=int, default=72)
    p.add_argument('--rounds', type=int, default=4); p.add_argument('--round-transfers', type=int, default=48); p.add_argument('--round-per-family', type=int, default=36)
    p.add_argument('--evaluation-transfers', type=int, default=24); p.add_argument('--evaluation-per-family', type=int, default=12)
    p.add_argument('--long-transfers', type=int, default=0); p.add_argument('--round-long-transfers', type=int, default=0)
    p.add_argument('--evaluation-long-transfers', type=int, default=0)
    p.add_argument('--grounded', type=int, default=0, help='climbs from the ground in the first training round, the family the turn sense is read on')
    p.add_argument('--round-grounded', type=int, default=0); p.add_argument('--evaluation-grounded', type=int, default=0)
    p.add_argument('--routed-weight', type=float, default=1.)
    p.add_argument('--sense-weight', type=float, default=SENSE_WEIGHT, help='the weight of the fourth head on the rows where the law sets a sense')
    p.add_argument('--steps', type=int, default=8000); p.add_argument('--round-steps', type=int, default=5000)
    p.add_argument('--batch', type=int, default=512); p.add_argument('--learning-rate', type=float, default=1e-3)
    p.add_argument('--workers', type=int, default=12); p.add_argument('--threads', type=int, default=12)
    p.add_argument('--contract', type=int, choices=(2, 3), default=CONTRACT,
                   help='the command contract: 3 adds the advisory vector with the turn sense, over a v3 motor')
    args = p.parse_args()
    SENSE_WEIGHT = args.sense_weight
    contract = args.contract; law = law_for(contract)
    torch.set_num_threads(args.threads); torch.manual_seed(args.seed); random.seed(args.seed)
    generator = torch.Generator().manual_seed(args.seed)
    args.out.mkdir(parents=False); started = time.monotonic()
    bridge_path = str(Path(args.bridge).resolve()); motor = str(Path(args.motor).resolve())
    plan = {'kind': KIND, 'contract': CONTRACTS[contract], 'law': law, 'hold': HOLD, 'perturbation': PERTURBATION,
            'seeds': SEEDS, 'scaling': args.scaling, 'bridge_sha256': sha(bridge_path), 'motor_sha256': sha(motor),
            'sources': {name: sha(Path(__file__).parent / name) for name in SOURCES},
            'arguments': {k: (str(v) if isinstance(v, Path) else v) for k, v in vars(args).items()}}
    atomic_json(args.out / 'plan.json', plan)
    log_lines = []

    def log(line):
        print(line, flush=True); log_lines.append(line)
    with Bridge(bridge_path) as bridge:
        bridge.call('load_motor_policy', path=motor)
        model = NavigationModel(bridge.contract['schema'], args.scaling, contract)
        development = task_rows(bridge, SEEDS['development'], args.evaluation_transfers, args.evaluation_per_family, args.evaluation_long_transfers,
                                args.evaluation_grounded)
        held_out = task_rows(bridge, SEEDS['held_out'], args.evaluation_transfers, args.evaluation_per_family, args.evaluation_long_transfers,
                             args.evaluation_grounded)
        training = [task_rows(bridge, SEEDS['training'][r], args.transfers if r == 0 else args.round_transfers,
                              args.per_family if r == 0 else args.round_per_family,
                              args.long_transfers if r == 0 else args.round_long_transfers,
                              args.grounded if r == 0 else args.round_grounded) for r in range(args.rounds + 1)]
        atomic_json(args.out / 'tasks-development.json', development); atomic_json(args.out / 'tasks-held-out.json', held_out)
        for r, rows in enumerate(training):
            atomic_json(args.out / f'tasks-training-round-{r}.json', rows)
        log('tasks: development %d, held out %d, training %s (%.0f s)' % (len(development), len(held_out), [len(t) for t in training], time.monotonic() - started))
        x, y, meta, outcomes = collect_parallel(bridge_path, motor, None, training[0], args.seed, args.out / 'shards', args.workers, 'round-0', contract)
        log('round 0: %d rows from %d perturbed law episodes, outcomes %s (%.0f s)' % (len(x), len(training[0]), outcomes, time.monotonic() - started))
        errors = fit(model, x, y, meta, args.steps, args.batch, args.learning_rate, generator, log, args.routed_weight)
        rounds = [{'rows': len(x), 'episodes': len(training[0]), 'law_outcomes': outcomes, 'errors': errors}]
        log('round 0 errors: %s' % json.dumps(errors))
        for r in range(1, args.rounds + 1):
            export = args.out / f'policy-round-{r - 1}.json'; model.export(export)
            visited, labels, visited_meta, outcomes = collect_parallel(bridge_path, motor, str(export.resolve()), training[r], args.seed + r, args.out / 'shards', args.workers, f'round-{r}', contract)
            log('round %d: %d rows from %d network episodes, outcomes %s (%.0f s)' % (r, len(visited), len(training[r]), outcomes, time.monotonic() - started))
            x = torch.cat((x, visited)); y = torch.cat((y, labels)); meta = torch.cat((meta, visited_meta))
            errors = fit(model, x, y, meta, args.round_steps, args.batch, args.learning_rate / 2, generator, log, args.routed_weight)
            rounds.append({'rows': len(x), 'episodes': len(training[r]), 'network_outcomes': outcomes, 'errors': errors})
            log('round %d errors: %s' % (r, json.dumps(errors)))
        export = args.out / 'policy-final.json'; model.export(export)
        torch.save({'kind': KIND, 'state': model.state_dict(), 'law': law, 'input_scaling': model.input_scaling}, args.out / 'checkpoint-final.pt')
        bridge.call('load_navigation_policy', path=str(export.resolve()))
        development_results = fit_v1.summarize(fit_v1.evaluate(bridge, model, development, args.out / 'evaluation-development', log, prepare=reference_v2.settle))
        log('development set: %d/%d passed, max parity %.2e (%.0f s)' % (development_results['passed'], development_results['cases'], development_results['max_parity'], time.monotonic() - started))
        held_out_results = fit_v1.summarize(fit_v1.evaluate(bridge, model, held_out, args.out / 'evaluation-held-out', log, prepare=reference_v2.settle))
        log('held-out set: %d/%d passed (%.0f s)' % (held_out_results['passed'], held_out_results['cases'], time.monotonic() - started))
    law_results = reference_v2.run(bridge_path, motor, held_out, law, args.workers)
    atomic_json(args.out / 'reference-held-out.json', law_results); law_summary = reference.summarize(law_results)
    log('law on the held-out set: %d/%d passed (%.0f s)' % (law_summary['passed'], law_summary['cases'], time.monotonic() - started))
    summary = {'kind': KIND, 'contract': CONTRACTS[contract], 'input_scaling': model.input_scaling, 'rounds': rounds,
               'development': development_results, 'held_out': held_out_results, 'law_held_out': law_summary,
               'seconds': time.monotonic() - started, 'policy_sha256': sha(export)}
    atomic_json(args.out / 'summary.json', summary); (args.out / 'log.txt').write_text('\n'.join(log_lines) + '\n')
    log('done in %.1f s' % summary['seconds'])


if __name__ == '__main__':
    main()
