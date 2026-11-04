"""Planet-frame motor teacher v6: the v5 attitude law with the brake's share taken out of it.

The v2 motor contract commands heading and speed relative to the pilot's planet
frame, the frame the physical brake decelerates toward, with gravity and planet
spin allowed and speeds to 60. The v5 teacher keeps every v4 term on the
relative velocities and adds two gravity terms: a thrust feedforward for the
gravity component along the nose, and at zero command a turn of the nose
against gravity so the ship hovers on thrust instead of sinking at the brake's
terminal rate. The v6 law (LF-091) leaves the overspeed the brake sheds, down
to crawling speeds, and a speed the nose already exceeds out of the attitude
error, so the nose no longer reverses for them; `GAINS_V5` keeps the v5 law.
It is a training target only; the exported network owns every control.
"""
import argparse
import json
import math
from multiprocessing import Pool
from pathlib import Path
import random
import time

import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from . import aligned, teacher, velocity
from .model import ARCHITECTURE, WIDE_ARCHITECTURE, MotorModel

KIND = 'motor-teacher-fit-v2'
CONTRACT = 2
SCALE = 70.
MAX_SPEED = 60.
GRAVITY_SCALE = 45.
TURN_SPEED = teacher.TURN_SPEED
# hover_start/hover_full: gravity magnitudes between which a zero command turns
# the nose from the commanded heading to straight against gravity.
# attitude: the slower gain the nose steers by, so it tilts toward gravity's
# opposite while the error is still large; hold: the acceleration-sized term
# along the commanded course that settles the nose when nothing is demanded.
# hold_floor: the fraction of the course hold kept under full gravity, so a
# moving ship's rest attitude under weak gravity blends the course with
# gravity's opposite instead of swinging between them. attitude_ramp: the
# velocity-error magnitudes between which the error enters the attitude
# vector, so the nose does not chase an error the thrust can remove where it
# points; (0, 0) is the v5 law. brake_share: the fraction of the error's
# component against the relative velocity, the overspeed the brake removes,
# that is taken out of the attitude error, so the nose does not reverse to
# thrust against a speed the brake is already shedding; brake_share_speed:
# the relative speed below which that share fades out, since the brake's
# deceleration is proportional to the speed and a ship at rest in the frame
# must still turn its nose to a command behind it. attitude_forward: the
# fraction of a negative forward error, a speed the nose already exceeds,
# that enters the attitude vector; the thrust cut, the brake or gravity
# remove such a speed without a turn, so at 0 the nose does not reverse for
# it, while a positive forward error still flattens the tilt a lateral error
# asks for, as the thrust that removes it is lateral acceleration too.
# attitude_floor: the attitude vector magnitude under which the spin fades,
# since a vector near zero, an error that gravity is about to remove, has no
# direction worth turning to. gravity_share: the fraction of the error's
# component along gravity, the part gravity removes on its own, taken out of
# the attitude error, so the nose does not turn to thrust for it and then
# swing back against gravity once it is gone; gravity_share_time caps that
# share at what gravity removes in this many seconds, so a launch climb's
# large error toward the planet still turns the nose. attitude_relative:
# when set, the error's gain in the attitude vector is this many
# radians-worth per unit of speed relative to the rest bias, gravity's
# opposite plus the course hold, so the tilt a given error asks for is the
# same under weak gravity as in free space, where the fixed gain overpowers
# a weak bias and the nose chases its own thrust-driven drift around.
# thrust_soft: the forward error under
# which the thrust gain falls off linearly with the error, so a crawl at the
# goal does not overshoot its command by the thrust's first tick. stop_hold:
# when set, a stop in free space keeps the nose where it is instead of
# turning it to the commanded heading, which near the goal points anywhere;
# under gravity the nose still turns against it. brake_share,
# brake_share_speed and attitude_forward make the v6 law; the rest are
# inert at their defaults.
GAINS_V5 = {'thrust': .1, 'attitude': .04, 'hold': 2., 'brake': .1, 'lateral': .2, 'lateral_deadband': .5, 'spin': 2.,
            'hover_start': .5, 'hover_full': 1.5}
# The v6 law: the v5 gains with the brake's share of the error left out of the
# attitude vector down to 2.5 units per second and no reversal for a speed the
# nose already exceeds. Judged with the navigation law flying it on the
# practice and transfer families (docs/flight-motor-steering-v1.md).
GAINS = dict(GAINS_V5, brake_share=1., brake_share_speed=2.5, attitude_forward=0.)
DEFAULT_BRIDGE = 'target/ci/examples/flight_motor_bridge'
GRAVITY_ALTITUDES = (150., 100., 60., 30.)
# A deterministic matrix: gravity off or on at four altitudes, planet spin, the
# commanded speed, a heading delta, and the start velocity relative to the frame.
MATRIX_SPEEDS = (0., 10., 30., 60.)
MATRIX_DELTAS = (0., math.pi / 2)
MATRIX_STARTS = {'rest': (0., 0.), 'forward': (30., 0.), 'lateral': (0., 20.), 'backward': (-20., 0.)}


def teacher_v2(x, gains=GAINS):
    """Batched teacher actions (steering, thrust, brake) for v2 motor input rows.

    Moving: the required acceleration is the proportional velocity error minus
    gravity. The nose rests where it is, or against gravity when there is any,
    and turns toward the required vector as the error grows; the attitude uses
    a slower gain than the thrust, so the nose tilts toward gravity's opposite
    while the error is still large instead of swinging there after the error
    is gone. Thrust is the projection of the required acceleration onto the
    nose, the v4 proportional term plus the gravity feedforward, and the brake
    fires on the component of the error that braking removes. The nose never
    swings back to the commanded course, since the v2 gate has no heading
    requirement. Stopping: the v4 brake law, with the nose turned to the
    commanded heading or against gravity, and thrust holding the ship when
    gravity would otherwise sink it.
    """
    if x.ndim != 2 or x.shape[1] != 18:
        raise ValueError('wrong motor input shape')
    command = x[:, 2] * SCALE; vx = x[:, 3] * SCALE; vy = x[:, 4] * SCALE
    gx = x[:, 8] * GRAVITY_SCALE; gy = x[:, 9] * GRAVITY_SCALE
    delta = torch.atan2(x[:, 0], x[:, 1])
    moving = command > 0.
    # Velocity error in the body frame: x right, y forward.
    ex = -x[:, 0] * command - vx; ey = x[:, 1] * command - vy
    # The nose steers toward the attitude vector: a slow gain times the error,
    # minus gravity, plus a small hold term along the commanded course. With no
    # demand the nose settles on the course; under gravity it settles against
    # gravity; a real error turns it continuously, with no thresholds to hunt
    # across, and the course reference keeps a residual error from walking the
    # nose around in a circle.
    attitude = gains['attitude'] * GRAVITY_SCALE
    gravity = torch.hypot(gx, gy)
    hover = ((gravity - gains['hover_start']) / (gains['hover_full'] - gains['hover_start'])).clamp(0., 1.)
    # Under gravity the course hold fades out: gravity's opposite is then the
    # rest attitude for moving and stopping alike, so nothing flips between them.
    hold = gains['hold'] * (1. - (1. - gains.get('hold_floor', 0.)) * hover)
    low, high = gains.get('attitude_ramp', (0., 0.))
    error = torch.hypot(ex, ey)
    weight = torch.ones_like(error) if high <= low else ((error - low) / (high - low)).clamp(0., 1.)
    speed = torch.hypot(vx, vy).clamp_min(1e-6)
    overspeed = (-(ex * vx + ey * vy) / speed).clamp_min(0.)
    # The brake's share of the error, the overspeed along the relative
    # velocity, leaves the attitude error while the ship moves fast enough
    # for the brake to shed it; the nose then holds its course or its hover
    # attitude instead of reversing to thrust against it.
    share = gains.get('brake_share', 0.) * (speed / gains.get('brake_share_speed', 10.)).clamp(0., 1.)
    ax = ex + share * overspeed * vx / speed; ay = ey + share * overspeed * vy / speed
    ay = ay.clamp_min(0.) + gains.get('attitude_forward', 1.) * ay.clamp_max(0.)
    along = ((ax * gx + ay * gy) / gravity.clamp_min(1e-6)).clamp_min(0.)
    if gains.get('gravity_share_time', 0.) > 0.:
        along = torch.minimum(along, gravity * gains['gravity_share_time'])
    along = along * gains.get('gravity_share', 0.)
    ax = ax - along * gx / gravity.clamp_min(1e-6); ay = ay - along * gy / gravity.clamp_min(1e-6)
    bx = -gx - hold * x[:, 0]; by = -gy + hold * x[:, 1]
    relative = gains.get('attitude_relative', 0.)
    if relative > 0.:
        attitude = relative * torch.hypot(bx, by)
    tx = attitude * weight * ax + bx; ty = attitude * weight * ay + by
    pursued = torch.atan2(-tx, ty)
    floor = gains.get('attitude_floor', 0.)
    magnitude = torch.ones_like(pursued) if floor <= 0. else (torch.hypot(tx, ty) / floor).clamp(0., 1.)
    spin_moving = (gains['spin'] * pursued * magnitude).clamp(-TURN_SPEED, TURN_SPEED)
    # Thrust is the projection of the required acceleration onto the nose: the
    # proportional term plus the gravity feedforward.
    soft = gains.get('thrust_soft', 0.)
    softness = torch.ones_like(ey) if soft <= 0. else (ey.abs() / soft).clamp(0., 1.)
    thrust_moving = (gains['thrust'] * softness * ey - gy / GRAVITY_SCALE).clamp(0., 1.)
    against = torch.atan2(gx, -gy)
    reversed_ = (-x[:, 0] * vx + x[:, 1] * vy) < 0.
    # The v4 lateral damping: brake on relative velocity across the commanded
    # course beyond a deadband, which thrust along the course then restores.
    # It fades out under gravity, where drift across the course is gravity's
    # work and only the attitude tilt and thrust can undo it; braking there
    # throws away the speed along the course and lets the ship sink.
    across = (vx * x[:, 1] + vy * x[:, 0]).abs()
    lateral = (gains['lateral'] * (1. - hover) * (across - gains['lateral_deadband']).clamp_min(0.)).clamp(0., 1.)
    brake_moving = torch.where(reversed_, torch.ones_like(vy),
                               torch.maximum((gains['brake'] * overspeed).clamp(0., 1.), lateral))
    # Stopping: face the commanded heading or, under gravity, turn against it and hover.
    hovered = delta + hover * torch.atan2((against - delta).sin(), (against - delta).cos())
    hovered = torch.atan2(hovered.sin(), hovered.cos())
    feedforward = (-gy / GRAVITY_SCALE).clamp_min(0.)
    thrust_stopping = (gains['thrust'] * -vy * hover + feedforward).clamp(0., 1.)
    brake_stopping = (gains['brake'] * torch.hypot(vx, vy)).clamp(0., 1.)
    if gains.get('stop_hold', 0.) > 0.:
        # Only the turn against gravity remains; a free-space stop holds its heading.
        hovered = hover * torch.atan2(against.sin(), against.cos())
    spin_stopping = (gains['spin'] * hovered).clamp(-TURN_SPEED, TURN_SPEED)
    spin = torch.where(moving, spin_moving, spin_stopping).clamp(-TURN_SPEED, TURN_SPEED)
    steering = (-spin / TURN_SPEED).clamp(-1., 1.)
    thrust = torch.where(moving, thrust_moving, thrust_stopping)
    brake = torch.where(moving, brake_moving, brake_stopping)
    return torch.stack((steering, thrust, brake), dim=1)


def scalar_teacher_v2(inputs, gains=GAINS):
    steering, thrust, brake = teacher_v2(torch.tensor([inputs], dtype=torch.float32), gains)[0].tolist()
    return {'steering': steering, 'thrust': thrust, 'brake': brake}


def synthetic_inputs_v2(generator, count):
    """Open-wing v2 rows with gravity up to 10 in any direction; half concentrate on the aligned regime."""
    def uniform(low, high):
        return torch.rand((count,), generator=generator) * (high - low) + low
    near = torch.rand((count,), generator=generator) < .5
    delta = torch.where(near, uniform(-.3, .3), uniform(-math.pi, math.pi))
    command = torch.where(torch.rand((count,), generator=generator) < .25, torch.zeros(count), uniform(0., MAX_SPEED))
    speed = uniform(0., 70.); direction = uniform(-math.pi, math.pi)
    vx = torch.where(near, uniform(-2., 2.), speed * direction.sin())
    vy = torch.where(near, command + uniform(-6., 6.), speed * direction.cos())
    spin = torch.where(near, uniform(-.3, .3), uniform(-1.8, 1.8))
    gravity = torch.where(torch.rand((count,), generator=generator) < .4, torch.zeros(count), uniform(0., 18.))
    gravity_direction = uniform(-math.pi, math.pi)
    ones = torch.ones(count)
    return torch.stack((delta.sin(), delta.cos(), command / SCALE, vx / SCALE, vy / SCALE, (vx + uniform(-.5, .5)) / SCALE,
                        (vy + uniform(-.5, .5)) / SCALE, spin / 2., gravity * gravity_direction.sin() / GRAVITY_SCALE,
                        gravity * gravity_direction.cos() / GRAVITY_SCALE, ones, ones, ones, ones, ones,
                        uniform(-1., 1.), uniform(0., 1.), uniform(0., 1.)), dim=1)


def start(rng, bearing, offset, altitude, gravity, planet_spin, forward, lateral, magnitude, spin, delta):
    heading = bearing + offset
    velocity = {'x': (-math.sin(heading) * forward - math.cos(heading) * lateral) * magnitude,
                'y': (math.cos(heading) * forward - math.sin(heading) * lateral) * magnitude}
    return {'seed': rng.randrange(1 << 31), 'bearing': bearing, 'altitude': altitude, 'heading_offset': offset - delta,
            'relative_velocity': velocity, 'spin': spin, 'gravity': gravity, 'planet_spin': planet_spin}


def horizons(speed):
    """Deadline and retention horizon in ticks: fast commands travel far, so their horizons shrink to keep the path inside the practice world."""
    if speed == 0.:
        return 360, 600
    if speed <= 20.:
        return 480, 720
    if speed <= 30.:
        return 300, 420
    return 300, 360


ARENA_LAYOUT = {'sun': True, 'planets': 3, 'edge_margin': 200}


def random_task_v2(rng):
    """A v2 task: gravity on half the time at a random altitude, a third of those above an arena planet for match-strength gravity, spin a third of the time, speeds to 60."""
    bearing = rng.uniform(-math.pi, math.pi)
    offset = rng.choice([-1., 1.]) * rng.uniform(math.pi / 3, 2 * math.pi / 3)
    gravity = rng.random() < .5
    arena = gravity and rng.random() < .35
    altitude = rng.uniform(15., 60.) if arena else rng.uniform(15., 150.) if gravity else 150.
    planet_spin = 0. if arena else rng.choice([-.015, 0., 0., .015])
    forward, lateral = rng.choice([(0., 0.), (1., 0.), (-1., 0.), (0., 1.), (0., -1.), (.7, .7), (.7, -.7), (-.7, .7), (-.7, -.7)])
    magnitude = rng.uniform(0., 60.)
    delta = 0. if rng.random() < .6 else rng.uniform(-math.pi / 2, math.pi / 2)
    speed = 0. if rng.random() < .3 else rng.uniform(2., MAX_SPEED)
    deadline, retention = horizons(speed)
    task = {'start': start(rng, bearing, offset, altitude, gravity, planet_spin, forward, lateral, magnitude, rng.choice([-.6, 0., .6]), delta),
            'heading_delta': delta, 'speed': speed, 'deadline_ticks': deadline, 'retention_horizon_ticks': retention, 'contract': CONTRACT}
    if arena:
        planet = rng.randrange(3)
        task['transfer'] = {'layout': ARENA_LAYOUT, 'start_planet': planet, 'goal_planet': (planet + 1) % 3}
    return task


def matrix_tasks(seed):
    """The deterministic v2 reference matrix, 320 tasks."""
    rng = random.Random(seed)
    rows = []
    for gravity in (None, *GRAVITY_ALTITUDES):
        for planet_spin in (0., .015):
            for speed in MATRIX_SPEEDS:
                for delta in MATRIX_DELTAS:
                    for mode, (forward, lateral) in MATRIX_STARTS.items():
                        # Commands within thirty degrees of the tangent: neither into the planet nor out of the world.
                        bearing = rng.uniform(-math.pi, math.pi)
                        offset = rng.choice([-1., 1.]) * rng.uniform(math.pi / 3, 2 * math.pi / 3)
                        magnitude = math.hypot(forward, lateral)
                        f, l = (forward / magnitude, lateral / magnitude) if magnitude > 0. else (0., 0.)
                        altitude = gravity if gravity is not None else 150.
                        family = 'free' if gravity is None else 'gravity%d' % gravity
                        deadline, retention = horizons(speed)
                        rows.append({'index': len(rows), 'family': family, 'mode': mode,
                                     'level': 'speed%d' % speed, 'direction': 'turn' if delta else 'ahead',
                                     'planet_spin': planet_spin,
                                     'task': {'start': start(rng, bearing, offset, altitude, gravity is not None, planet_spin, f, l, magnitude, 0., delta),
                                              'heading_delta': delta, 'speed': speed, 'deadline_ticks': deadline,
                                              'retention_horizon_ticks': retention, 'contract': CONTRACT}})
    return rows


CRUISE_SPEEDS = (40., 55.)
CRUISE_RATES = (-.3, -.15, .15, .3)
CRUISE_ALTITUDES = (80., 140.)


def cruise_tasks(seed, bridge=None):
    """The turning cruise family: a command whose heading rotates at a steady rate while the ship holds speed under gravity for 25 seconds, the match's transfer regime.

    The command turns the way a route around a planet or the sun turns it;
    the matrix's single fixed commands never ask the nose to track a target
    that keeps moving. Half the tasks fly above an arena planet for the
    match's gravity and orbit; the rest above the practice planet with spin.
    With a bridge, an arena start that meets terrain or the other seat is
    drawn again, as the rollout sampler draws its starts.
    """
    rng = random.Random(seed)
    rows = []
    for arena in (False, True):
        for altitude in CRUISE_ALTITUDES:
            for speed in CRUISE_SPEEDS:
                for rate in CRUISE_RATES:
                    for mode, (forward, lateral) in (('forward', (1., 0.)), ('lateral', (0., 1.))):
                        for _ in range(16):
                            bearing = rng.uniform(-math.pi, math.pi)
                            offset = rng.choice([-1., 1.]) * rng.uniform(math.pi / 3, 2 * math.pi / 3)
                            task = {'start': start(rng, bearing, offset, altitude, True, 0. if arena else .015, forward, lateral, speed * .8, 0., 0.),
                                    'heading_delta': 0., 'speed': speed, 'deadline_ticks': 300, 'retention_horizon_ticks': 1500,
                                    'heading_rate': rate, 'contract': CONTRACT}
                            if arena:
                                planet = rng.randrange(3)
                                task['transfer'] = {'layout': ARENA_LAYOUT, 'start_planet': planet, 'goal_planet': (planet + 1) % 3}
                            if bridge is None:
                                break
                            try:
                                bridge.call('reset', tasks=[task])
                                break
                            except RuntimeError as error:
                                if 'initial' not in str(error) and 'transfer' not in str(error):
                                    raise
                        else:
                            raise RuntimeError('no admitted cruise start')
                        rows.append({'index': len(rows), 'family': 'cruise_arena' if arena else 'cruise_practice', 'mode': mode,
                                     'level': 'speed%d' % speed, 'direction': 'rate%+.2f' % rate, 'planet_spin': 0. if arena else .015, 'task': task})
    return rows


def v1_sets_as_v2():
    """The LF-078 motor development sets, zero gravity and speeds to 20, judged under the v2 contract."""
    rows = []
    for family, source in (('v1_speed16', teacher.development_rows()),
                           ('v1_aligned', aligned.candidates(teacher.DEVELOPMENT_TASK_SEED, True)),
                           ('v1_lateral', velocity.candidates(teacher.DEVELOPMENT_TASK_SEED, True))):
        for row in source:
            task = json.loads(json.dumps(row['task'])); task['contract'] = CONTRACT
            rows.append({'index': len(rows), 'family': family, 'mode': row.get('mode'), 'level': row.get('level'),
                         'direction': row.get('direction'), 'planet_spin': 0., 'task': task})
    return rows


def collect_rollouts(bridge, rng, episodes, driver='teacher', perturb=True, gains=GAINS):
    """Observed v2 inputs from closed-loop episodes driven by the teacher or the loaded network.

    Random held actions spread the visited states; the recorded rows are still
    labelled by the teacher afterwards, so perturbation never changes a target.
    """
    rows = []; outcomes = {}
    for _ in range(episodes):
        while True:
            # An arena start can meet terrain or the other seat; the sampler draws again, as the admission screens do.
            try:
                item = bridge.call('reset', tasks=[random_task_v2(rng)])['items'][0]
                break
            except RuntimeError as error:
                if 'initial' not in str(error) and 'transfer' not in str(error):
                    raise
                outcomes['rejected_start'] = outcomes.get('rejected_start', 0) + 1
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
                item = bridge.call('step', actions=[scalar_teacher_v2(item['inputs'], gains)])['items'][0]
        outcome = item['progress']['outcome']; outcomes[outcome] = outcomes.get(outcome, 0) + 1
    x = torch.tensor(rows, dtype=torch.float32)
    # The teacher ignores previous actions; randomizing half of them stops the fit from copying them.
    mask = torch.rand((len(x),)) < .5
    noise = torch.stack((torch.rand(len(x)) * 2 - 1, torch.rand(len(x)), torch.rand(len(x))), dim=1)
    x[mask, 15:] = noise[mask]
    return x, outcomes


def run_law(bridge, row, gains):
    item = bridge.call('reset', tasks=[row['task']])['items'][0]
    post_error = 0.; ticks = 0; saturated = 0; spinning = 0; velocity_error = 0.
    while item['progress']['outcome'] is None:
        action = scalar_teacher_v2(item['inputs'], gains)
        ticks += 1; saturated += abs(action['steering']) > .95; spinning += abs(item['inputs'][7] * 2.) > 1.5
        item = bridge.call('step', actions=[action])['items'][0]
        velocity_error += item['errors']['velocity']
        retention = item.get('retention') or {}
        if retention.get('first_success_tick') is not None:
            post_error = max(post_error, item['errors']['velocity'])
    retention = item.get('retention') or {}
    return {'index': row['index'], 'family': row['family'], 'mode': row['mode'], 'level': row['level'],
            'direction': row['direction'], 'planet_spin': row.get('planet_spin'), 'outcome': item['progress']['outcome'],
            'ticks': item['progress']['elapsed_ticks'], 'first_success_tick': retention.get('first_success_tick'),
            'first_exit_tick': retention.get('first_exit_tick'), 'final_errors': item['errors'],
            'max_post_acquisition_velocity_error': post_error,
            'steering_saturation': saturated / max(1, ticks), 'spin_saturation': spinning / max(1, ticks),
            'mean_velocity_error': velocity_error / max(1, ticks)}


def _reference_shard(job):
    bridge_path, rows, gains = job
    with Bridge(bridge_path) as bridge:
        return [run_law(bridge, row, gains) for row in rows]


def reference_results(bridge_path, rows, gains=GAINS, workers=8):
    """Closed-loop results of the scripted law on every row, over a pool of bridges."""
    chunks = [rows[i::workers] for i in range(workers)]
    with Pool(workers) as pool:
        shards = pool.map(_reference_shard, [(bridge_path, chunk, gains) for chunk in chunks if chunk])
    results = [r for shard in shards for r in shard]
    results.sort(key=lambda r: r['index'])
    return results


def summarize(results):
    summary = teacher.summarize(results)
    by_family = {}
    for r in results:
        g = by_family.setdefault(r['family'], {'cases': 0, 'passed': 0})
        g['cases'] += 1; g['passed'] += r['outcome'] == 'success'
    summary['by_family'] = by_family
    # The share of ticks at the steering limit and at a spin above 1.5, over
    # the cases that recorded them, for the law's and the fit's steering.
    for key in ('steering_saturation', 'spin_saturation', 'mean_velocity_error'):
        values = [r[key] for r in results if key in r]
        if values:
            summary[key] = sum(values) / len(values)
    return summary


def fit_actor(model, x, y, steps, batch, learning_rate, generator, log=print, steering_weight=1.):
    """Supervised regression of the actor only, with the steering term weighted; the critic and deviations stay at initialization."""
    parameters = [p for name, p in model.named_parameters() if name.startswith('actor')]
    optimizer = torch.optim.Adam(parameters, lr=learning_rate)
    schedule = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, steps)
    for step in range(1, steps + 1):
        index = torch.randint(len(x), (batch,), generator=generator)
        actor, _ = model(x[index])
        steering = torch.nn.functional.mse_loss(torch.tanh(actor[:, 0]), y[index][:, 0])
        actuators = torch.nn.functional.binary_cross_entropy_with_logits(actor[:, 1:], y[index][:, 1:])
        loss = steering_weight * steering + actuators
        optimizer.zero_grad(); loss.backward(); optimizer.step(); schedule.step()
        if step % max(steps // 10, 1) == 0 or step == steps:
            with torch.no_grad():
                error = teacher.action_error(model(x)[0], y)
            log('  fit step %d/%d loss %.5f mean |error| steering %.4f thrust %.4f brake %.4f'
                % (step, steps, loss.item(), *error.tolist()))
    with torch.no_grad():
        return teacher.action_error(model(x)[0], y).tolist()


def evaluate_sets(bridge, model, out, hold=1):
    """Closed-loop evaluation of the loaded policy on the development matrix, a held-out matrix and the v1 sets."""
    summaries = {}
    for name, rows in (('matrix-development', matrix_tasks(1)), ('matrix-held-out', matrix_tasks(2)), ('v1-sets', v1_sets_as_v2())):
        results = teacher.evaluate(bridge, model, rows, out / f'evaluation-{name}', hold)
        summaries[name] = summarize(results)
        summaries[name]['max_parity'] = max(r['parity'] for r in results)
    return summaries


def reference(args):
    gains = dict(GAINS, **(args.gains or {}))
    started = time.monotonic()
    if args.set == 'cruise':
        with Bridge(args.bridge) as bridge:
            rows = cruise_tasks(args.seed, bridge)
    else:
        rows = matrix_tasks(args.seed) if args.set == 'matrix' else v1_sets_as_v2()
    atomic_json(args.out / 'plan.json', {'kind': 'motor-teacher-v2-reference', 'set': args.set, 'gains': gains, 'contract': CONTRACT,
                                         'bridge_sha256': sha(args.bridge), 'seed': args.seed, 'tasks': len(rows)})
    atomic_json(args.out / 'tasks.json', rows)
    results = reference_results(args.bridge, rows, gains, args.workers)
    atomic_json(args.out / 'results.json', results)
    summary = summarize(results); summary['seconds'] = time.monotonic() - started
    atomic_json(args.out / 'summary.json', summary)
    print(json.dumps({'passed': summary['passed'], 'cases': summary['cases'], 'by_family': summary['by_family'],
                      'seconds': round(summary['seconds'], 1)}, indent=1))


def fit(args):
    """The LF-078 fit pipeline on the v2 contract: synthetic rows plus teacher rollouts, then rounds on the network's own states."""
    gains = dict(GAINS, **(args.gains or {}))
    torch.set_num_threads(args.threads); torch.manual_seed(args.seed); random.seed(args.seed)
    generator = torch.Generator().manual_seed(args.seed); rng = random.Random(args.seed)
    started = time.monotonic()
    plan = {'kind': KIND, 'contract': CONTRACT, 'gains': gains, 'architecture': WIDE_ARCHITECTURE if args.width == 64 else ARCHITECTURE,
            'steering_weight': args.steering_weight, 'bridge_sha256': sha(args.bridge),
            'arguments': {k: (str(v) if isinstance(v, Path) else v) for k, v in vars(args).items()}}
    atomic_json(args.out / 'plan.json', plan)
    log_lines = []
    def log(line):
        print(line, flush=True); log_lines.append(line)
    with Bridge(args.bridge) as bridge:
        model = MotorModel(bridge.contract['schema_v2'], args.width)
        x = synthetic_inputs_v2(generator, args.synthetic)
        rollouts, outcomes = collect_rollouts(bridge, rng, args.episodes, gains=gains)
        log('teacher rollouts: %d rows from %d episodes, outcomes %s' % (len(rollouts), args.episodes, outcomes))
        x = torch.cat((x, rollouts)); y = teacher_v2(x, gains)
        log('round 0: fitting %d rows' % len(x))
        errors = [fit_actor(model, x, y, args.steps, args.batch, args.learning_rate, generator, log, args.steering_weight)]
        rounds = [{'rows': len(x), 'teacher_outcomes': outcomes, 'mean_absolute_error': errors[-1]}]
        for round_index in range(1, args.rounds + 1):
            export = args.out / f'policy-round-{round_index - 1}.json'; model.export(export)
            bridge.call('load_policy', path=str(export.resolve()))
            visited, outcomes = collect_rollouts(bridge, rng, args.round_episodes, driver='policy', gains=gains)
            log('round %d: network rollouts %d rows, outcomes %s' % (round_index, len(visited), outcomes))
            x = torch.cat((x, visited)); y = teacher_v2(x, gains)
            errors.append(fit_actor(model, x, y, args.round_steps, args.batch, args.learning_rate / 2, generator, log, args.steering_weight))
            rounds.append({'rows': len(x), 'network_outcomes': outcomes, 'mean_absolute_error': errors[-1]})
        export = args.out / 'policy-final.json'; model.export(export)
        bridge.call('load_policy', path=str(export.resolve()))
        summaries = evaluate_sets(bridge, model, args.out)
    for name, summary in summaries.items():
        log('%s: %d/%d passed, parity %.2e' % (name, summary['passed'], summary['cases'], summary['max_parity']))
        for family, group in summary['by_family'].items():
            log('  %s %d/%d' % (family, group['passed'], group['cases']))
    atomic_json(args.out / 'summary.json', {'kind': KIND, 'contract': CONTRACT, 'gains': gains, 'rounds': rounds,
                                            'evaluations': summaries, 'seconds': time.monotonic() - started,
                                            'policy_sha256': sha(export)})
    (args.out / 'log.txt').write_text('\n'.join(log_lines) + '\n')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('mode', choices=('reference', 'fit'))
    p.add_argument('--bridge', default=DEFAULT_BRIDGE); p.add_argument('--out', type=Path, required=True)
    p.add_argument('--seed', type=int, default=1); p.add_argument('--workers', type=int, default=8)
    p.add_argument('--set', choices=('matrix', 'v1', 'cruise'), default='matrix')
    p.add_argument('--gains', type=json.loads, default=None, help='JSON overrides of the law gains')
    p.add_argument('--synthetic', type=int, default=400_000); p.add_argument('--episodes', type=int, default=300)
    p.add_argument('--steps', type=int, default=40_000); p.add_argument('--batch', type=int, default=4096)
    p.add_argument('--learning-rate', type=float, default=1e-3); p.add_argument('--rounds', type=int, default=3)
    p.add_argument('--round-episodes', type=int, default=200); p.add_argument('--round-steps', type=int, default=20_000)
    p.add_argument('--threads', type=int, default=8)
    p.add_argument('--width', type=int, default=32, help='actor hidden width, 32 or 64')
    p.add_argument('--steering-weight', type=float, default=1., help='weight of the steering term in the fit loss')
    args = p.parse_args()
    args.out.mkdir(parents=False)
    if args.mode == 'reference':
        reference(args)
    else:
        fit(args)


if __name__ == '__main__':
    main()
