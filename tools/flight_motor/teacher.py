"""LF-074: supervised fit of an independent proportional teacher into the plain motor actor.

The teacher is a hand-written proportional law over the 18 motor inputs. It is a
training target only. The exported network owns every control during evaluation,
with no scripted mask, override, fallback or takeover. Native physical outcomes
remain the success criteria, and the teacher is not a ceiling for later learning.
"""
import argparse
import json
import math
from pathlib import Path
import random
import time

import torch
from torch.nn import functional as F
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from . import aligned, single_speed, velocity
from .model import ARCHITECTURE, MotorModel, tensor

# Throttle and brake per unit of speed error, desired spin per radian of heading error,
# and brake per unit of lateral speed at nonzero commands. GAINS_V1 is the LF-074 law,
# which left lateral velocity undamped whenever a speed was commanded.
GAINS_V1 = {'thrust': .1, 'brake': .1, 'spin': 2.}
GAINS_V2 = {'thrust': .1, 'brake': .1, 'spin': 2., 'lateral': .2}
# Crab: while the body-frame velocity error exceeds pursuit_start and calls for
# acceleration, the nose target turns from the commanded heading by crab radians
# per unit of lateral velocity error, capped at crab_cap and faded in fully at
# pursuit_full, so thrust removes lateral error early and no residual is left for
# the brake to grind down near the target. Overspeed errors are left to the brake.
# The lateral brake term ignores residuals below lateral_deadband, which the
# tolerance absorbs, so the fitted network has a flat zero to land on at the target.
GAINS_V3 = {'thrust': .1, 'brake': .1, 'spin': 2., 'lateral': .2, 'lateral_deadband': .5, 'pursuit_start': 1., 'pursuit_full': 6.}
GAINS = {'thrust': .1, 'brake': .1, 'spin': 2., 'lateral': .2, 'lateral_deadband': .5, 'pursuit_start': 1., 'pursuit_full': 6., 'crab': .15, 'crab_cap': .8}
TURN_SPEED = 1.8
DEVELOPMENT_TASK_SEED = 5910002
DEFAULT_BRIDGE = 'data/training-experiments/learned-flight-motor-effective-batch-v1-20261025/runtime/flight_motor_bridge'
KIND = 'motor-teacher-fit-v1'


def wrap(angle):
    return math.atan2(math.sin(angle), math.cos(angle))


def scalar_teacher(inputs, gains=GAINS):
    """Reference scalar form of the teacher; the batched form must agree with it."""
    ds, dc, command = inputs[0], inputs[1], inputs[2] * 20.
    vx, vy = inputs[3] * 20., inputs[4] * 20.
    delta = math.atan2(ds, dc)
    if command > 0. and 'pursuit_full' in gains:
        ex, ey = -ds * command - vx, dc * command - vy
        if ey > 0.:
            blend = max(0., min(1., (math.hypot(ex, ey) - gains['pursuit_start']) / (gains['pursuit_full'] - gains['pursuit_start'])))
            if 'crab' in gains:
                delta = wrap(delta + blend * max(-gains['crab_cap'], min(gains['crab_cap'], gains['crab'] * -ex)))
            else:
                delta = wrap(delta + blend * wrap(math.atan2(-ex, ey) - delta))
    spin = max(-TURN_SPEED, min(TURN_SPEED, gains['spin'] * delta))
    steering = max(-1., min(1., -spin / TURN_SPEED))
    if command == 0.:
        return {'steering': steering, 'thrust': 0., 'brake': max(0., min(1., gains['brake'] * math.hypot(vx, vy)))}
    forward_error = dc * command - vy
    thrust = max(0., min(1., gains['thrust'] * forward_error))
    lateral = min(1., gains.get('lateral', 0.) * max(0., abs(vx) - gains.get('lateral_deadband', 0.)))
    brake = 1. if vy < 0. else max(max(0., min(1., gains['brake'] * -forward_error)), lateral)
    return {'steering': steering, 'thrust': thrust, 'brake': brake}


def teacher(x, gains=GAINS):
    """Batched teacher actions (steering, thrust, brake) for motor input rows."""
    if x.ndim != 2 or x.shape[1] != 18:
        raise ValueError('wrong motor input shape')
    command = x[:, 2] * 20.; vx = x[:, 3] * 20.; vy = x[:, 4] * 20.
    delta = torch.atan2(x[:, 0], x[:, 1])
    forward_error = x[:, 1] * command - vy
    moving = command > 0.
    if 'pursuit_full' in gains:
        ex = -x[:, 0] * command - vx; ey = forward_error
        blend = ((torch.hypot(ex, ey) - gains['pursuit_start']) / (gains['pursuit_full'] - gains['pursuit_start'])).clamp(0., 1.)
        if 'crab' in gains:
            turn = (gains['crab'] * -ex).clamp(-gains['crab_cap'], gains['crab_cap'])
        else:
            turn = torch.atan2(-ex, ey) - delta
            turn = torch.atan2(turn.sin(), turn.cos())
        pursued = delta + blend * turn
        delta = torch.where(moving & (ey > 0.), torch.atan2(pursued.sin(), pursued.cos()), delta)
    spin = (gains['spin'] * delta).clamp(-TURN_SPEED, TURN_SPEED)
    steering = (-spin / TURN_SPEED).clamp(-1., 1.)
    thrust = torch.where(moving, (gains['thrust'] * forward_error).clamp(0., 1.), torch.zeros_like(vy))
    stopping = (gains['brake'] * torch.hypot(vx, vy)).clamp(0., 1.)
    lateral = (gains.get('lateral', 0.) * (vx.abs() - gains.get('lateral_deadband', 0.)).clamp_min(0.)).clamp(0., 1.)
    tracking = torch.where(vy < 0., torch.ones_like(vy), torch.maximum((gains['brake'] * -forward_error).clamp(0., 1.), lateral))
    return torch.stack((steering, thrust, torch.where(moving, tracking, stopping)), dim=1)


def synthetic_inputs(generator, count):
    """Zero-gravity, open-wing motor rows; half concentrate on the aligned regulation regime."""
    def uniform(low, high):
        return torch.rand((count,), generator=generator) * (high - low) + low
    near = torch.rand((count,), generator=generator) < .5
    delta = torch.where(near, uniform(-.3, .3), uniform(-math.pi, math.pi))
    command = torch.where(torch.rand((count,), generator=generator) < .25, torch.zeros(count), uniform(0., 20.))
    speed = uniform(0., 32.); direction = uniform(-math.pi, math.pi)
    vx = torch.where(near, uniform(-2., 2.), speed * direction.sin())
    vy = torch.where(near, command + uniform(-6., 6.), speed * direction.cos())
    spin = torch.where(near, uniform(-.3, .3), uniform(-1.8, 1.8))
    zeros = torch.zeros(count); ones = torch.ones(count)
    return torch.stack((delta.sin(), delta.cos(), command / 20., vx / 20., vy / 20., (vx + uniform(-.5, .5)) / 20.,
                        (vy + uniform(-.5, .5)) / 20., spin / 2., zeros, zeros, ones, ones, ones, ones, ones,
                        uniform(-1., 1.), uniform(0., 1.), uniform(0., 1.)), dim=1)


def random_task(rng):
    """A zero-gravity task whose commanded heading points within 60 degrees of radial up."""
    bearing = rng.uniform(-math.pi, math.pi); offset = rng.uniform(-math.pi / 3, math.pi / 3)
    heading = bearing + offset
    forward, lateral = rng.choice([(0., 0.), (1., 0.), (-1., 0.), (0., 1.), (0., -1.), (.7, .7), (.7, -.7), (-.7, .7), (-.7, -.7)])
    magnitude = rng.uniform(0., 30.)
    delta = 0. if rng.random() < .6 else rng.uniform(-math.pi / 2, math.pi / 2)
    speed = 0. if rng.random() < .3 else rng.uniform(2., 20.)
    velocity = {'x': (-math.sin(heading) * forward - math.cos(heading) * lateral) * magnitude,
                'y': (math.cos(heading) * forward - math.sin(heading) * lateral) * magnitude}
    return {'start': {'seed': rng.randrange(1 << 31), 'bearing': bearing, 'altitude': 150., 'heading_offset': offset - delta,
                      'relative_velocity': velocity, 'spin': rng.choice([-.6, 0., .6]), 'gravity': False, 'planet_spin': 0.},
            'heading_delta': delta, 'speed': speed, 'deadline_ticks': 360, 'retention_horizon_ticks': 600}


def collect_rollouts(bridge, rng, episodes, driver='teacher', perturb=True):
    """Observed inputs from closed-loop episodes driven by the teacher or the loaded network.

    Random held actions spread the visited states; the recorded rows are still
    labelled by the teacher afterwards, so perturbation never changes a target.
    """
    rows = []; outcomes = {}
    for _ in range(episodes):
        item = bridge.call('reset', tasks=[random_task(rng)])['items'][0]
        hold = 0; action = None
        while item['progress']['outcome'] is None:
            rows.append(item['inputs'])
            if perturb and hold == 0 and rng.random() < 1 / 90:
                hold = rng.randint(1, 30)
                action = {'steering': rng.uniform(-1., 1.), 'thrust': rng.random(), 'brake': rng.random()}
            if hold > 0:
                hold -= 1; item = bridge.call('step', actions=[action])['items'][0]
            elif driver == 'policy':
                item = bridge.call('step_policy')['items'][0]
            else:
                item = bridge.call('step', actions=[scalar_teacher(item['inputs'])])['items'][0]
        outcome = item['progress']['outcome']; outcomes[outcome] = outcomes.get(outcome, 0) + 1
    x = torch.tensor(rows, dtype=torch.float32)
    # The teacher ignores previous actions; randomizing half of them stops the fit from copying them.
    mask = torch.rand((len(x),)) < .5
    noise = torch.stack((torch.rand(len(x)) * 2 - 1, torch.rand(len(x)), torch.rand(len(x))), dim=1)
    x[mask, 15:] = noise[mask]
    return x, outcomes


def loss_terms(actor, target):
    """Steering by squared error on the tanh output; actuators by cross-entropy on the sigmoid logits."""
    steering = F.mse_loss(torch.tanh(actor[:, 0]), target[:, 0])
    actuators = F.binary_cross_entropy_with_logits(actor[:, 1:], target[:, 1:])
    return steering + actuators


def action_error(actor, target):
    predicted = torch.cat((torch.tanh(actor[:, :1]), torch.sigmoid(actor[:, 1:])), dim=1)
    return (predicted - target).abs().mean(0)


def fit(model, x, y, steps, batch, learning_rate, generator, log=print):
    """Supervised regression of the actor only; the critic and deviations stay at initialization."""
    parameters = [p for name, p in model.named_parameters() if name.startswith('actor')]
    optimizer = torch.optim.Adam(parameters, lr=learning_rate)
    schedule = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, steps)
    for step in range(1, steps + 1):
        index = torch.randint(len(x), (batch,), generator=generator)
        actor, _ = model(x[index])
        loss = loss_terms(actor, y[index])
        optimizer.zero_grad(); loss.backward(); optimizer.step(); schedule.step()
        if step % max(steps // 10, 1) == 0 or step == steps:
            with torch.no_grad():
                error = action_error(model(x)[0], y)
            log('  fit step %d/%d loss %.5f mean |error| steering %.4f thrust %.4f brake %.4f'
                % (step, steps, loss.item(), *error.tolist()))
    with torch.no_grad():
        return action_error(model(x)[0], y).tolist()


def reference_results(bridge, rows, gains=GAINS, hold=1):
    """Closed-loop results of the scripted law itself: the standing feasibility reference."""
    results = []
    for row in rows:
        item = bridge.call('reset', tasks=[row['task']])['items'][0]
        post_error = 0.
        while item['progress']['outcome'] is None:
            action = scalar_teacher(item['inputs'], gains)
            for _ in range(hold):
                item = bridge.call('step', actions=[action])['items'][0]
                retention = item.get('retention') or {}
                if retention.get('first_success_tick') is not None:
                    post_error = max(post_error, item['errors']['velocity'])
                if item['progress']['outcome'] is not None:
                    break
        retention = item.get('retention') or {}
        results.append({'index': row['index'], 'family': row['family'], 'mode': row.get('mode'), 'level': row.get('level'),
                        'direction': row.get('direction'), 'outcome': item['progress']['outcome'], 'ticks': item['progress']['elapsed_ticks'],
                        'first_success_tick': retention.get('first_success_tick'), 'first_exit_tick': retention.get('first_exit_tick'),
                        'final_errors': item['errors'], 'max_post_acquisition_velocity_error': post_error})
    return results


def probe_forces(bridge, speeds=(0, 8, 12, 15, 16, 17, 20, 24), command=16.):
    """Fixed-state actuator outputs of the loaded policy at its own previous-action fixed point."""
    rows = []
    for speed in speeds:
        previous = (0., 0., 0.)
        for _ in range(30):
            inputs = [0., 1., command / 20., 0., speed / 20., 0., speed / 20., 0., 0., 0., 1., 1., 1., 1., 1., *previous]
            action = bridge.call('infer', inputs=[inputs])['outputs'][0]['action']
            previous = (action['steering'], action['thrust'], action['brake'])
        rows.append({'speed': speed, 'thrust': action['thrust'], 'brake': action['brake'],
                     'net_acceleration': 45. * action['thrust'] - min(4. * speed, 40.) * action['brake']})
    by_speed = {r['speed']: r['net_acceleration'] for r in rows}
    return {'rows': rows, 'slope_at_16': (by_speed[17] - by_speed[15]) / 2, 'bias_at_16': by_speed[16]}


def evaluate(bridge, model, rows, directory, hold=1):
    """Closed-loop development evaluation of the loaded policy with native/Python parity on first inputs.

    With hold above one, each native decision is applied for that many ticks,
    matching a training recipe that decides at a lower rate than the physics.
    """
    directory.mkdir(); results = []
    for row in rows:
        item = bridge.call('reset', tasks=[row['task']])['items'][0]
        native = bridge.call('infer', inputs=[item['inputs']])['outputs'][0]
        with torch.no_grad():
            actor = model(tensor([item]))[0][0]
        parity = max(abs(a - b) for a, b in zip(actor.tolist(), [native['steering_latent'], native['thrust_logit'], native['brake_logit']]))
        if parity > 1e-4:
            raise ValueError('native and Python inference disagree')
        start = item['frame']['center_of_mass']; post_error = 0.
        while item['progress']['outcome'] is None:
            if hold == 1:
                items = [bridge.call('step_policy')['items'][0]]
            else:
                action = bridge.call('infer', inputs=[item['inputs']])['outputs'][0]['action']
                items = []
                for _ in range(hold):
                    items.append(bridge.call('step', actions=[action])['items'][0])
                    if items[-1]['progress']['outcome'] is not None:
                        break
            for item in items:
                retention = item.get('retention') or {}
                if retention.get('first_success_tick') is not None:
                    post_error = max(post_error, item['errors']['velocity'])
        end = item['frame']['center_of_mass']; retention = item.get('retention') or {}
        atomic_json(directory / f"case-{row['index']:03}.json", bridge.call('state')['traces'][0])
        results.append({'index': row['index'], 'family': row['family'], 'mode': row.get('mode'), 'level': row.get('level'),
                        'direction': row.get('direction'), 'outcome': item['progress']['outcome'], 'ticks': item['progress']['elapsed_ticks'],
                        'first_success_tick': retention.get('first_success_tick'), 'first_exit_tick': retention.get('first_exit_tick'),
                        'final_errors': item['errors'], 'max_post_acquisition_velocity_error': post_error,
                        'displacement': math.hypot(end['x'] - start['x'], end['y'] - start['y']), 'parity': parity})
    atomic_json(directory / 'results.json', results)
    return results


def summarize(results):
    groups = {}
    for r in results:
        key = '%s/%s/%s' % (r['family'], r['mode'], r['level'])
        g = groups.setdefault(key, {'cases': 0, 'passed': 0, 'outcomes': {}, 'acquisition_ticks': [], 'max_post_acquisition_velocity_error': 0.})
        g['cases'] += 1; g['passed'] += r['outcome'] == 'success'
        g['outcomes'][r['outcome']] = g['outcomes'].get(r['outcome'], 0) + 1
        if r['first_success_tick'] is not None:
            g['acquisition_ticks'].append(r['first_success_tick'])
        g['max_post_acquisition_velocity_error'] = max(g['max_post_acquisition_velocity_error'], r['max_post_acquisition_velocity_error'])
    for g in groups.values():
        ticks = g.pop('acquisition_ticks')
        g['acquisition_ticks'] = [min(ticks), max(ticks)] if ticks else None
    return {'cases': len(results), 'passed': sum(r['outcome'] == 'success' for r in results), 'groups': dict(sorted(groups.items()))}


def development_rows(archive=None):
    rows = single_speed.candidates(DEVELOPMENT_TASK_SEED)
    if archive is not None:
        expected = json.loads(Path(archive).read_text())
        if [r['task'] for r in expected] != [r['task'] for r in rows]:
            raise ValueError('regenerated development tasks differ from the archived LF-073 set')
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--bridge', default=DEFAULT_BRIDGE); p.add_argument('--out', type=Path, required=True)
    p.add_argument('--seed', type=int, default=1); p.add_argument('--synthetic', type=int, default=300_000)
    p.add_argument('--episodes', type=int, default=300); p.add_argument('--steps', type=int, default=20_000)
    p.add_argument('--batch', type=int, default=2048); p.add_argument('--learning-rate', type=float, default=1e-3)
    p.add_argument('--rounds', type=int, default=2); p.add_argument('--round-episodes', type=int, default=200)
    p.add_argument('--round-steps', type=int, default=10_000); p.add_argument('--threads', type=int, default=4)
    p.add_argument('--archived-development', type=Path)
    args = p.parse_args()
    torch.set_num_threads(args.threads); torch.manual_seed(args.seed); random.seed(args.seed)
    generator = torch.Generator().manual_seed(args.seed); rng = random.Random(args.seed)
    args.out.mkdir(parents=False); started = time.monotonic()
    plan = {'kind': KIND, 'gains': GAINS, 'architecture': ARCHITECTURE, 'bridge_sha256': sha(args.bridge),
            'arguments': {k: (str(v) if isinstance(v, Path) else v) for k, v in vars(args).items()}}
    atomic_json(args.out / 'plan.json', plan)
    log_lines = []
    def log(line):
        print(line, flush=True); log_lines.append(line)
    with Bridge(args.bridge) as bridge:
        model = MotorModel(bridge.contract['schema'])
        x = synthetic_inputs(generator, args.synthetic)
        rollouts, outcomes = collect_rollouts(bridge, rng, args.episodes)
        log('teacher rollouts: %d rows from %d episodes, outcomes %s' % (len(rollouts), args.episodes, outcomes))
        x = torch.cat((x, rollouts)); y = teacher(x)
        log('round 0: fitting %d rows' % len(x))
        errors = [fit(model, x, y, args.steps, args.batch, args.learning_rate, generator, log)]
        rounds = [{'rows': len(x), 'teacher_outcomes': outcomes, 'mean_absolute_error': errors[-1]}]
        for round_index in range(1, args.rounds + 1):
            export = args.out / f'policy-round-{round_index - 1}.json'; model.export(export)
            bridge.call('load_policy', path=str(export.resolve()))
            visited, outcomes = collect_rollouts(bridge, rng, args.round_episodes, driver='policy')
            log('round %d: network rollouts %d rows, outcomes %s' % (round_index, len(visited), outcomes))
            x = torch.cat((x, visited)); y = teacher(x)
            errors.append(fit(model, x, y, args.round_steps, args.batch, args.learning_rate / 2, generator, log))
            rounds.append({'rows': len(x), 'network_outcomes': outcomes, 'mean_absolute_error': errors[-1]})
        export = args.out / 'policy-final.json'; model.export(export)
        torch.save({'kind': KIND, 'state': model.state_dict(), 'gains': GAINS}, args.out / 'checkpoint-final.pt')
        bridge.call('load_policy', path=str(export.resolve()))
        forces = probe_forces(bridge)
        log('fixed-state probe at command 16: slope %+.4f per unit speed, bias %+.4f' % (forces['slope_at_16'], forces['bias_at_16']))
        development = development_rows(args.archived_development)
        speed16 = summarize(evaluate(bridge, model, development, args.out / 'development-speed16'))
        log('LF-073 development set: %d/%d passed' % (speed16['passed'], speed16['cases']))
        broad = summarize(evaluate(bridge, model, aligned.candidates(DEVELOPMENT_TASK_SEED, True), args.out / 'development-aligned'))
        log('aligned 6/12/18 development set: %d/%d passed' % (broad['passed'], broad['cases']))
        lateral = summarize(evaluate(bridge, model, velocity.candidates(DEVELOPMENT_TASK_SEED, True), args.out / 'development-lateral'))
        log('lateral 6/12/18 matrix with 60-degree turns: %d/%d passed' % (lateral['passed'], lateral['cases']))
    summary = {'kind': KIND, 'gains': GAINS, 'rounds': rounds, 'fixed_state_probe': forces, 'development_speed16': speed16,
               'development_aligned': broad, 'development_lateral': lateral, 'seconds': time.monotonic() - started, 'policy_sha256': sha(export)}
    atomic_json(args.out / 'summary.json', summary); (args.out / 'log.txt').write_text('\n'.join(log_lines) + '\n')
    log('done in %.1f s' % summary['seconds'])


if __name__ == '__main__':
    main()
