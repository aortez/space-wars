"""Planet-frame motor teacher v7: the v6 law with a thrust that waits for the nose and a committed half turn.

The v2 motor contract commands heading and speed relative to the pilot's planet
frame, the frame the physical brake decelerates toward, with gravity and planet
spin allowed and speeds to 60. The v5 teacher keeps every v4 term on the
relative velocities and adds two gravity terms: a thrust feedforward for the
gravity component along the nose, and at zero command a turn of the nose
against gravity so the ship hovers on thrust instead of sinking at the brake's
terminal rate. The v6 law (LF-091) leaves the overspeed the brake sheds, down
to crawling speeds, and a speed the nose already exceeds out of the attitude
error, so the nose no longer reverses for them. The v7 law (LF-099) holds the
thrust until the nose comes around to its attitude vector, so a ship on the
ground does not pin itself against the ground while it turns, and keeps a
turn near the half turn in the direction it started; `GAINS_V6` and
`GAINS_V5` keep the earlier laws. It is a training target only; the exported
network owns every control.
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
from .model import ARCHITECTURE, SENSE_ARCHITECTURE, WIDE_ARCHITECTURE, MotorModel

KIND = 'motor-teacher-fit-v2'
CONTRACT = 2
# The motor inputs of each command contract: the v3 contract adds the
# command's advisory vector, the turn sense first.
INPUTS = {2: 18, 3: 22}
SENSE = 18
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
# under gravity the nose still turns against it. thrust_gate: the angles
# between the nose and the attitude vector, (open, closed) in radians, over
# which the moving thrust fades from full to none, so the thrust waits for
# the nose: a ship lying on its side that thrusts while it turns pushes into
# the ground and the contact holds the turn (LF-099). thrust_lift: (up_open,
# up_closed, speed_full, speed_none), which reopens that gate under gravity
# while the nose lies within up_open to up_closed radians of gravity's
# opposite and the relative speed is under speed_full to speed_none, since
# there the thrust lifts the ship instead of pinning it: a climb at the
# launch band's edge whose nose is still turning, or a ship on its feet,
# otherwise waits on a turn that is slow or blocked (LF-099). half_turn: the width
# in radians of the band short of a half turn, of the heading error or of
# the attitude vector's angle from the nose, in which a saturated turn
# keeps the direction its previous steering gave it, half_turn_gain times
# that steering plus half_turn_bias toward the shorter turn, so a target
# behind the nose has one turn direction the fit can follow instead of a
# jump from full one way to full the other where the attitude vector
# crosses behind the nose, at which the fit passes through zero; gravity
# and the brake's share put that crossing short of the heading error's
# half turn (LF-098's seed 66 held a heading error of 2.7). brake_floor: the relative
# speeds (low, high) over which the moving brake fades in, so a ship at rest
# does not brake on the direction of its jitter: on the ground the brake
# holds the hull against the pivot that turns it upright. brake_share,
# brake_share_speed and attitude_forward make the v6 law, and thrust_gate and
# half_turn added to it the v7 law; LF-099 tried thrust_lift and brake_floor
# and left them out (docs/flight-motor-contact-v1.md). The rest are inert at
# their defaults.
GAINS_V5 = {'thrust': .1, 'attitude': .04, 'hold': 2., 'brake': .1, 'lateral': .2, 'lateral_deadband': .5, 'spin': 2.,
            'hover_start': .5, 'hover_full': 1.5}
# The v6 law: the v5 gains with the brake's share of the error left out of the
# attitude vector down to 2.5 units per second and no reversal for a speed the
# nose already exceeds. Judged with the navigation law flying it on the
# practice and transfer families (docs/flight-motor-steering-v1.md).
GAINS_V6 = dict(GAINS_V5, brake_share=1., brake_share_speed=2.5, attitude_forward=0.)
# The v7 law: the v6 gains with the thrust faded out as the nose turns from
# 0.35 to 0.7 radians off its attitude vector and the half-turn band 0.7
# radians wide. Judged on the contact sets and with the navigation law flying
# it (docs/flight-motor-contact-v1.md).
GAINS = dict(GAINS_V6, thrust_gate=(.35, .7), half_turn=.7)
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
    if x.ndim != 2 or x.shape[1] not in INPUTS.values():
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
    band = gains.get('half_turn', 0.)
    if band > 0.:
        # With the command short of a half turn behind the nose, a turn the
        # ordinary law saturates keeps the direction the previous steering
        # gave it, and only a ship that was not turning takes the shorter
        # turn. The band's outer half blends from the ordinary law into it,
        # and an attitude vector near the nose, a hover with a slow command
        # behind it, keeps the ordinary law.
        ramp = lambda angle: ((angle.abs() - (math.pi - band)) / (band / 2.)).clamp(0., 1.)
        behind = torch.maximum(ramp(delta), ramp(pursued))
        saturated = ((pursued.abs() - TURN_SPEED / gains['spin']) / .3).clamp(0., 1.)
        committed = (-gains.get('half_turn_gain', 3.) * x[:, 15]
                     + gains.get('half_turn_bias', .5) * torch.sign(pursued)).clamp(-1., 1.) * TURN_SPEED
        spin_moving = spin_moving + behind * saturated * (committed * magnitude - spin_moving)
    # Thrust is the projection of the required acceleration onto the nose: the
    # proportional term plus the gravity feedforward.
    soft = gains.get('thrust_soft', 0.)
    softness = torch.ones_like(ey) if soft <= 0. else (ey.abs() / soft).clamp(0., 1.)
    thrust_moving = (gains['thrust'] * softness * ey - gy / GRAVITY_SCALE).clamp(0., 1.)
    against = torch.atan2(gx, -gy)
    gate = gains.get('thrust_gate')
    if gate is not None:
        opened, closed = gate
        held = ((closed - pursued.abs()) / (closed - opened)).clamp(0., 1.)
        lift = gains.get('thrust_lift')
        if lift is not None:
            up_open, up_closed, speed_full, speed_none = lift
            upright = ((up_closed - against.abs()) / (up_closed - up_open)).clamp(0., 1.)
            slow = ((speed_none - torch.hypot(vx, vy)) / (speed_none - speed_full)).clamp(0., 1.)
            held = torch.maximum(held, upright * slow * hover)
        thrust_moving = thrust_moving * held
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
    floor_speeds = gains.get('brake_floor')
    if floor_speeds is not None:
        low, high = floor_speeds
        brake_moving = brake_moving * ((torch.hypot(vx, vy) - low) / (high - low)).clamp(0., 1.)
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
    thrust = torch.where(moving, thrust_moving, thrust_stopping)
    brake = torch.where(moving, brake_moving, brake_stopping)
    if x.shape[1] > SENSE:
        # The v3 contract's turn sense, the sign of the spin to turn with when
        # the ground blocks the shorter turn: the turn goes that way at the
        # limit, slowing in proportion to the angle still to turn that way so
        # it ends on the attitude vector, with the thrust still behind its
        # gate and the brake off, since the brake resists the pivot.
        sense = x[:, SENSE]
        advised = sense != 0.
        target = torch.where(moving, pursued, hovered)
        remaining = torch.where(torch.sign(target) == sense, target.abs(), 2 * math.pi - target.abs())
        spin = torch.where(advised, sense * (gains['spin'] * remaining).clamp(max=TURN_SPEED), spin)
        brake = torch.where(advised, torch.zeros_like(brake), brake)
    steering = (-spin / TURN_SPEED).clamp(-1., 1.)
    return torch.stack((steering, thrust, brake), dim=1)


def scalar_teacher_v2(inputs, gains=GAINS):
    steering, thrust, brake = teacher_v2(torch.tensor([inputs], dtype=torch.float32), gains)[0].tolist()
    return {'steering': steering, 'thrust': thrust, 'brake': brake}


def synthetic_inputs_v2(generator, count, contract=CONTRACT):
    """Open-wing v2 rows with gravity up to 10 in any direction; half concentrate on the aligned regime.

    Under the v3 contract the rows carry the advisory vector: a turn sense
    either way on a fifth of the rows each, so the fit learns to obey it
    everywhere, and zeros in the spare elements.
    """
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
    rows = torch.stack((delta.sin(), delta.cos(), command / SCALE, vx / SCALE, vy / SCALE, (vx + uniform(-.5, .5)) / SCALE,
                        (vy + uniform(-.5, .5)) / SCALE, spin / 2., gravity * gravity_direction.sin() / GRAVITY_SCALE,
                        gravity * gravity_direction.cos() / GRAVITY_SCALE, ones, ones, ones, ones, ones,
                        uniform(-1., 1.), uniform(0., 1.), uniform(0., 1.)), dim=1)
    if contract == 3:
        draw = torch.rand((count,), generator=generator)
        sense = torch.where(draw < .2, -ones, torch.where(draw < .4, ones, torch.zeros(count)))
        rows = torch.cat((rows, sense[:, None], torch.zeros((count, INPUTS[3] - SENSE - 1))), dim=1)
    return rows


def under_contract(rows, contract):
    """The task rows with their tasks under the given command contract."""
    for row in rows:
        row['task']['contract'] = contract
    return rows


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


def world_start(rng, bearing, offset, altitude, planet_spin, up, tangent, delta):
    """A gravity start whose velocity relative to the frame is given along the local up and its tangent instead of the command's axes."""
    s = start(rng, bearing, offset, altitude, True, planet_spin, 0., 0., 0., 0., delta)
    ux, uy = -math.sin(bearing), math.cos(bearing)
    s['relative_velocity'] = {'x': ux * up + uy * tangent, 'y': uy * up - ux * tangent}
    return s


def wrap(angle):
    return math.atan2(math.sin(angle), math.cos(angle))


def admitted(bridge, rng, make):
    """A task row from make(rng), drawn again while the bridge rejects its start, as an arena start can meet terrain or the other seat."""
    for _ in range(100):
        row = make(rng)
        if bridge is None:
            return row
        try:
            bridge.call('reset', tasks=[row['task']])
            return row
        except RuntimeError as error:
            if 'initial' not in str(error) and 'transfer' not in str(error):
                raise
    raise RuntimeError('no admitted start')


def contact_task(rng, world, attitude, tilt, speed, altitude, up=0., tangent=0., heading=None):
    """A contact-allowed v2 task under gravity: the ship's nose `attitude` radians from the local up, the command `tilt` from it, or `heading` from the up when given."""
    bearing = rng.uniform(-math.pi, math.pi)
    offset = tilt if heading is None else heading
    arena = world == 'arena'
    planet_spin = 0. if arena else rng.choice([-.015, 0., .015])
    task = {'start': world_start(rng, bearing, offset, altitude, planet_spin, up, tangent, wrap(offset - attitude)),
            'heading_delta': wrap(offset - attitude), 'speed': speed, 'deadline_ticks': 600, 'contract': CONTRACT,
            'contact_allowed': True}
    if arena:
        planet = rng.randrange(3)
        task['transfer'] = {'layout': ARENA_LAYOUT, 'start_planet': planet, 'goal_planet': (planet + 1) % 3}
    return task


GROUND_ATTITUDES = (0., 1., -1., math.pi / 2, -math.pi / 2, 2.4, -2.4, math.pi)
CLIMB_SPEEDS = (20., 40., 57.)


def ground_tasks(seed, bridge=None):
    """Climbs from the ground, 96 tasks: the ship falls from altitude 15 with its nose at one of eight attitudes and the controls neutral until it rests, then a command up within 0.4 radians at a climb speed.

    The launch climb begins on the ship's feet and a climb that came down
    begins on its side or nose; the motor's other tasks end on contact.
    """
    rng = random.Random(seed); rows = []
    for world in ('practice', 'arena'):
        for attitude in GROUND_ATTITUDES:
            for speed in CLIMB_SPEEDS:
                for _ in range(2):
                    row = admitted(bridge, rng, lambda r: {'task': contact_task(r, world, attitude, r.uniform(-.4, .4), speed, 15.)})
                    rows.append({'index': len(rows), 'family': 'ground_' + world, 'mode': 'attitude%+.1f' % attitude,
                                 'level': 'speed%d' % speed, 'direction': 'up', 'settle': True, 'task': row['task']})
    return rows


def falling_tasks(seed, bridge=None):
    """Climbs begun low and falling with the nose turned away, 80 tasks: altitude 18 or 30, the nose 1.6 to 3.14 radians from the local up, falling at 5 or 12, a command up within 0.4 radians."""
    rng = random.Random(seed); rows = []
    for world in ('practice', 'arena'):
        for altitude in (18., 30.):
            for attitude in (1.6, -1.6, 2.4, -2.4, math.pi):
                for fall in (5., 12.):
                    for speed in (20., 57.):
                        row = admitted(bridge, rng, lambda r: {'task': contact_task(r, world, attitude, r.uniform(-.4, .4), speed, altitude, up=-fall)})
                        rows.append({'index': len(rows), 'family': 'falling_' + world, 'mode': 'attitude%+.1f' % attitude,
                                     'level': 'speed%d' % speed, 'direction': 'altitude%d_fall%d' % (altitude, fall), 'task': row['task']})
    return rows


def half_turn_tasks(seed, bridge=None):
    """Commands behind the nose, 80 tasks: the command 2.6, 2.9 or 3.14 radians off the nose, up or along the tangent, at 20 or 57, from rest or falling at 10, at altitude 80 above the practice planet or 40 above an arena planet."""
    rng = random.Random(seed); rows = []
    for world, altitude in (('practice', 80.), ('arena', 40.)):
        for delta in (2.6, -2.6, 2.9, -2.9, math.pi):
            for direction in ('up', 'tangent'):
                for speed in (20., 57.):
                    for fall in (0., 10.):
                        def make(r):
                            heading = 0. if direction == 'up' else r.choice([-1., 1.]) * math.pi / 2
                            return {'task': contact_task(r, world, wrap(heading - delta), 0., speed, altitude, up=-fall, heading=heading)}
                        row = admitted(bridge, rng, make)
                        rows.append({'index': len(rows), 'family': 'half_turn_' + world, 'mode': 'delta%+.1f' % delta,
                                     'level': 'speed%d' % speed, 'direction': '%s_fall%d' % (direction, fall), 'task': row['task']})
    return rows


def drifting_turn_tasks(seed, bridge=None):
    """Climb commands behind a drifting nose, 64 tasks: a command up at 57 with the nose 2.4 to 3.0 radians off it, drifting along the tangent at 20 or 40 either way, level or falling at 5, at altitude 90 above the practice planet.

    The match's dead spot (LF-098, seed 66 seat 0) held a heading error of
    2.7 with the ship drifting sideways, where gravity and the brake's share
    put the teacher's attitude vector behind the nose; the half-turn set
    starts at rest or falling only.
    """
    rng = random.Random(seed); rows = []
    for delta in (2.4, 2.6, 2.8, 3.0):
        for side in (-1., 1.):
            for drift in (20., 40.):
                for direction in (-1., 1.):
                    for fall in (0., 5.):
                        row = admitted(bridge, rng, lambda r: {'task': contact_task(r, 'practice', wrap(-side * delta), 0., 57., 90., up=-fall,
                                                                                    tangent=direction * drift, heading=0.)})
                        rows.append({'index': len(rows), 'family': 'drifting_turn', 'mode': 'delta%+.1f' % (side * delta),
                                     'level': 'drift%+d' % (direction * drift), 'direction': 'fall%d' % fall, 'task': row['task']})
    return rows


def tilted_climb_tasks(seed, bridge=None):
    """Climbs begun hovering with the nose tilted, 48 tasks: at rest 70 units above the practice planet or an arena planet, the nose 0.5, 0.7 or 0.9 radians either side of the local up, a command up within 0.2 radians at 40 or 57.

    A pursuit climb at the launch band's edge with the opponent far away
    releases as soon as the ship rises above 70 and starts again below it,
    so each attempt lasts a few ticks with the nose still turning: a thrust
    that waits for the nose leaves the ship hovering at the edge (LF-099,
    seed 41 seat 0). The recovered tick measures how soon the climb starts.
    """
    rng = random.Random(seed); rows = []
    for world in ('practice', 'arena'):
        for attitude in (.5, -.5, .7, -.7, .9, -.9):
            for speed in (40., 57.):
                for _ in range(2):
                    row = admitted(bridge, rng, lambda r: {'task': contact_task(r, world, attitude, r.uniform(-.2, .2), speed, 70.)})
                    rows.append({'index': len(rows), 'family': 'tilted_climb_' + world, 'mode': 'attitude%+.1f' % attitude,
                                 'level': 'speed%d' % speed, 'direction': 'up', 'task': row['task']})
    return rows


BLOCKED_ATTITUDES = (2., -2., 2.3, -2.3, 2.6, -2.6, 2.9, -2.9)


def blocked_command(rng, attitude):
    """A command from the up on the side opposite a nose below the horizontal, the two angles summing past a half turn, so the shorter turn to it goes through the ground and the longer through the sky."""
    return -math.copysign(rng.uniform(max(.3, math.pi - abs(attitude) + .1), 1.3), attitude)


def blocked_tasks(seed, bridge=None):
    """Turns the ground blocks, 96 tasks: the ship falls from altitude 15 with its nose at one of eight attitudes below the horizontal and the controls neutral until it rests, then a command up across to the other side at a climb speed."""
    rng = random.Random(seed); rows = []
    for world in ('practice', 'arena'):
        for attitude in BLOCKED_ATTITUDES:
            for speed in CLIMB_SPEEDS:
                for _ in range(2):
                    row = admitted(bridge, rng, lambda r: {'task': contact_task(r, world, attitude, blocked_command(r, attitude), speed, 15.)})
                    rows.append({'index': len(rows), 'family': 'blocked_' + world, 'mode': 'attitude%+.1f' % attitude,
                                 'level': 'speed%d' % speed, 'direction': 'up_across', 'settle': True, 'task': row['task']})
    return rows


def random_contact_task(rng, contract=CONTRACT):
    """A rollout row from the contact families: a climb from the ground, a low falling climb or a command behind the nose, and under the v3 contract a turn the ground blocks, with random parameters."""
    kind = rng.choice(('ground', 'falling', 'half_turn') + (('blocked',) if contract == 3 else ()))
    world = 'arena' if rng.random() < .5 else 'practice'
    speed = rng.uniform(10., MAX_SPEED)
    if kind == 'ground':
        attitude = rng.uniform(-math.pi, math.pi)
        return {'task': contact_task(rng, world, attitude, rng.uniform(-.6, .6), speed, 15.), 'settle': True}
    if kind == 'blocked':
        attitude = rng.choice([-1., 1.]) * rng.uniform(2., 3.)
        return {'task': contact_task(rng, world, attitude, blocked_command(rng, attitude), speed, 15.), 'settle': True}
    if kind == 'falling':
        attitude = rng.choice([-1., 1.]) * rng.uniform(1., math.pi)
        altitude = rng.uniform(16., 40.)
        return {'task': contact_task(rng, world, attitude, rng.uniform(-.6, .6), speed, altitude, up=-rng.uniform(2., 15.))}
    delta = rng.choice([-1., 1.]) * rng.uniform(2.2, math.pi)
    altitude = rng.uniform(20., 60.) if world == 'arena' else rng.uniform(30., 150.)
    heading = rng.uniform(-2 * math.pi / 3, 2 * math.pi / 3)
    return {'task': contact_task(rng, world, wrap(heading - delta), 0., speed, altitude, up=-rng.uniform(0., 12.), heading=heading)}


SETTLE_LIMIT = 150
SETTLE_REST = 10


def settle(bridge, item):
    """Neutral controls until the ship has rested in contact for ten ticks, at most 150 ticks; returns the item and the ticks spent."""
    ticks = 0; resting = 0
    while item['progress']['outcome'] is None and resting < SETTLE_REST and ticks < SETTLE_LIMIT:
        item = bridge.call('step', actions=[{'steering': 0., 'thrust': 0., 'brake': 0.}])['items'][0]; ticks += 1
        speed = math.hypot(item['inputs'][3], item['inputs'][4]) * SCALE
        resting = resting + 1 if item['evidence']['collision'] and speed < 1. else 0
    return item, ticks


RECOVERY_AIRBORNE = 30


class Recovery:
    """The tick a contact task recovers: thirty ticks out of contact with half the commanded speed along the command, the climb under way.

    The motor's own success needs the velocity within one unit for thirty
    ticks, which a fast climb above an arena planet can miss after it has
    left the ground, as the gravity and the frame change under it.
    """
    def __init__(self):
        self.airborne = 0; self.tick = None; self.ticks = 0

    def update(self, item):
        self.ticks += 1
        x = item['inputs']
        along = (-x[0] * x[3] + x[1] * x[4]) * SCALE
        self.airborne = 0 if item['evidence']['collision'] else self.airborne + 1
        if self.tick is None and self.airborne >= RECOVERY_AIRBORNE and along >= .5 * x[2] * SCALE:
            self.tick = self.ticks


def contact_sets(seed, bridge=None, contract=CONTRACT):
    sets = {'ground': ground_tasks(seed, bridge), 'falling': falling_tasks(seed, bridge), 'half_turn': half_turn_tasks(seed, bridge)}
    if contract == 3:
        sets['blocked'] = blocked_tasks(seed, bridge)
    return {name: under_contract(rows, contract) for name, rows in sets.items()}


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


def collect_rollouts(bridge, rng, episodes, driver='teacher', perturb=True, gains=GAINS, contact_share=0., contract=CONTRACT):
    """Observed inputs from closed-loop episodes driven by the teacher or the loaded network, under the command contract.

    Random held actions spread the visited states; the recorded rows are still
    labelled by the teacher afterwards, so perturbation never changes a target.
    A contact_share of the episodes come from the contact families, whose
    settling ticks are not recorded; at zero the draws are the LF-091 ones.
    """
    rows = []; outcomes = {}
    for _ in range(episodes):
        contact = contact_share > 0. and rng.random() < contact_share
        while True:
            # An arena start can meet terrain or the other seat; the sampler draws again, as the admission screens do.
            row = random_contact_task(rng, contract) if contact else {'task': random_task_v2(rng)}
            row['task']['contract'] = contract
            try:
                item = bridge.call('reset', tasks=[row['task']])['items'][0]
                break
            except RuntimeError as error:
                if 'initial' not in str(error) and 'transfer' not in str(error):
                    raise
                outcomes['rejected_start'] = outcomes.get('rejected_start', 0) + 1
        if row.get('settle'):
            item, _ = settle(bridge, item)
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
    x[mask, 15:18] = noise[mask]
    return x, outcomes


def run_law(bridge, row, gains):
    item = bridge.call('reset', tasks=[row['task']])['items'][0]
    settled = 0
    if row.get('settle'):
        item, settled = settle(bridge, item)
    post_error = 0.; ticks = 0; saturated = 0; spinning = 0; velocity_error = 0.; contact = 0; recovery = Recovery()
    while item['progress']['outcome'] is None:
        action = scalar_teacher_v2(item['inputs'], gains)
        ticks += 1; saturated += abs(action['steering']) > .95; spinning += abs(item['inputs'][7] * 2.) > 1.5
        item = bridge.call('step', actions=[action])['items'][0]
        velocity_error += item['errors']['velocity']; contact += item['evidence']['collision']; recovery.update(item)
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
            'mean_velocity_error': velocity_error / max(1, ticks), 'settle_ticks': settled, 'law_ticks': ticks,
            'contact_ticks': contact, 'recovered_tick': recovery.tick}


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
    # The contact families: the ticks flown to success after any settling,
    # and the share of flown ticks in contact.
    flown = [r['law_ticks'] for r in results if r['outcome'] == 'success' and 'law_ticks' in r]
    if flown:
        summary['mean_success_ticks'] = sum(flown) / len(flown)
    if any('contact_ticks' in r for r in results):
        summary['contact_share'] = sum(r.get('contact_ticks', 0) for r in results) / max(1, sum(r.get('law_ticks', 0) for r in results))
    recovered = [r['recovered_tick'] for r in results if r.get('recovered_tick') is not None]
    if any('recovered_tick' in r for r in results):
        summary['recovered'] = len(recovered)
        summary['mean_recovered_tick'] = sum(recovered) / max(1, len(recovered))
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


RECOVERY_SETS = ('ground', 'falling', 'half_turn', 'blocked')


def evaluate_sets(bridge, model, out, hold=1, contact=False, contract=CONTRACT):
    """Closed-loop evaluation of the loaded policy on the development matrix, a held-out matrix and the v1 sets, and with contact the contact sets on seed 1, under the command contract."""
    summaries = {}
    sets = [('matrix-development', matrix_tasks(1)), ('matrix-held-out', matrix_tasks(2)), ('v1-sets', v1_sets_as_v2())]
    sets = [(name, under_contract(rows, contract)) for name, rows in sets]
    if contact:
        sets += [(name, rows) for name, rows in contact_sets(1, bridge, contract).items()]
    for name, rows in sets:
        results = teacher.evaluate(bridge, model, rows, out / f'evaluation-{name}', hold, prepare=settle,
                                   recovered=(lambda item: Recovery()) if name in RECOVERY_SETS else None)
        summaries[name] = summarize(results)
        summaries[name]['max_parity'] = max(r['parity'] for r in results)
    return summaries


def load_export(path, schema=None):
    """A motor export's actor and critic in a MotorModel, for evaluating an export the run did not fit; the file's own schema unless one is given."""
    data = json.loads(Path(path).read_text())
    model = MotorModel(schema or data['observation_schema'], 64 if data['format_version'] in (4, 5) else 32)
    model.load_state_dict({k: torch.tensor(v['values'], dtype=torch.float32).reshape(v['shape']) for k, v in data['weights'].items()})
    return model


EVALUATION_SETS = ('matrix-development', 'matrix-held-out', 'v1-sets', 'ground', 'falling', 'half_turn', 'drifting_turn', 'tilted_climb',
                   'blocked')


def evaluate_export(args):
    """Closed-loop evaluation of a motor export on the named sets, the contact sets on seed 1 as the fit evaluates them."""
    names = args.sets.split(',')
    if not set(names) <= set(EVALUATION_SETS):
        raise ValueError('unknown evaluation set')
    started = time.monotonic(); summaries = {}
    with Bridge(args.bridge) as bridge:
        model = load_export(args.policy)
        contract = model.schema['version']
        bridge.call('load_policy', path=str(Path(args.policy).resolve()))
        contact = contact_sets(1, bridge, contract)
        contact['drifting_turn'] = drifting_turn_tasks(1, bridge)
        contact['tilted_climb'] = tilted_climb_tasks(1, bridge)
        if 'blocked' not in contact:
            # The blocked set judges a v2 export too, under its own contract.
            contact['blocked'] = blocked_tasks(1, bridge)
        for name in names:
            rows = {'matrix-development': lambda: matrix_tasks(1), 'matrix-held-out': lambda: matrix_tasks(2),
                    'v1-sets': v1_sets_as_v2}.get(name, lambda: contact[name])()
            rows = under_contract(rows, contract)
            results = teacher.evaluate(bridge, model, rows, args.out / f'evaluation-{name}', prepare=settle,
                                       recovered=(lambda item: Recovery()) if name in contact else None)
            summaries[name] = summarize(results)
            line = '%s: %d/%d passed' % (name, summaries[name]['passed'], summaries[name]['cases'])
            if 'recovered' in summaries[name]:
                line += ', recovered %d (%.0f ticks), contact share %.2f' % (summaries[name]['recovered'], summaries[name]['mean_recovered_tick'],
                                                                           summaries[name]['contact_share'])
            print(line, flush=True)
    atomic_json(args.out / 'summary.json', {'kind': 'motor-export-evaluation-v2', 'policy': str(args.policy), 'policy_sha256': sha(args.policy),
                                            'bridge_sha256': sha(args.bridge), 'evaluations': summaries,
                                            'seconds': time.monotonic() - started})


def reference(args):
    gains = dict(GAINS, **(args.gains or {}))
    started = time.monotonic()
    if args.set == 'cruise':
        with Bridge(args.bridge) as bridge:
            rows = cruise_tasks(args.seed, bridge)
    elif args.set in ('ground', 'falling', 'half_turn', 'drifting_turn', 'tilted_climb', 'blocked'):
        with Bridge(args.bridge) as bridge:
            rows = {'ground': ground_tasks, 'falling': falling_tasks, 'half_turn': half_turn_tasks,
                    'drifting_turn': drifting_turn_tasks, 'tilted_climb': tilted_climb_tasks, 'blocked': blocked_tasks}[args.set](args.seed, bridge)
    else:
        rows = matrix_tasks(args.seed) if args.set == 'matrix' else v1_sets_as_v2()
    rows = under_contract(rows, args.contract)
    atomic_json(args.out / 'plan.json', {'kind': 'motor-teacher-v2-reference', 'set': args.set, 'gains': gains, 'contract': args.contract,
                                         'bridge_sha256': sha(args.bridge), 'seed': args.seed, 'tasks': len(rows)})
    atomic_json(args.out / 'tasks.json', rows)
    results = reference_results(args.bridge, rows, gains, args.workers)
    atomic_json(args.out / 'results.json', results)
    summary = summarize(results); summary['seconds'] = time.monotonic() - started
    atomic_json(args.out / 'summary.json', summary)
    print(json.dumps({'passed': summary['passed'], 'cases': summary['cases'], 'by_family': summary['by_family'],
                      'mean_success_ticks': summary.get('mean_success_ticks'), 'contact_share': summary.get('contact_share'),
                      'recovered': summary.get('recovered'), 'mean_recovered_tick': summary.get('mean_recovered_tick'),
                      'seconds': round(summary['seconds'], 1)}, indent=1))


def fit(args):
    """The LF-078 fit pipeline on the v2 contract: synthetic rows plus teacher rollouts, then rounds on the network's own states."""
    gains = dict(GAINS, **(args.gains or {}))
    torch.set_num_threads(args.threads); torch.manual_seed(args.seed); random.seed(args.seed)
    generator = torch.Generator().manual_seed(args.seed); rng = random.Random(args.seed)
    started = time.monotonic()
    contract = args.contract
    plan = {'kind': KIND, 'contract': contract, 'gains': gains,
            'architecture': SENSE_ARCHITECTURE if contract == 3 else WIDE_ARCHITECTURE if args.width == 64 else ARCHITECTURE,
            'steering_weight': args.steering_weight, 'bridge_sha256': sha(args.bridge),
            'arguments': {k: (str(v) if isinstance(v, Path) else v) for k, v in vars(args).items()}}
    atomic_json(args.out / 'plan.json', plan)
    log_lines = []
    def log(line):
        print(line, flush=True); log_lines.append(line)
    with Bridge(args.bridge) as bridge:
        model = MotorModel(bridge.contract['schema_v3' if contract == 3 else 'schema_v2'], args.width)
        x = synthetic_inputs_v2(generator, args.synthetic, contract)
        rollouts, outcomes = collect_rollouts(bridge, rng, args.episodes, gains=gains, contact_share=args.contact_share, contract=contract)
        log('teacher rollouts: %d rows from %d episodes, outcomes %s' % (len(rollouts), args.episodes, outcomes))
        x = torch.cat((x, rollouts)); y = teacher_v2(x, gains)
        log('round 0: fitting %d rows' % len(x))
        errors = [fit_actor(model, x, y, args.steps, args.batch, args.learning_rate, generator, log, args.steering_weight)]
        rounds = [{'rows': len(x), 'teacher_outcomes': outcomes, 'mean_absolute_error': errors[-1]}]
        for round_index in range(1, args.rounds + 1):
            export = args.out / f'policy-round-{round_index - 1}.json'; model.export(export)
            bridge.call('load_policy', path=str(export.resolve()))
            visited, outcomes = collect_rollouts(bridge, rng, args.round_episodes, driver='policy', gains=gains,
                                                 contact_share=args.contact_share, contract=contract)
            log('round %d: network rollouts %d rows, outcomes %s' % (round_index, len(visited), outcomes))
            x = torch.cat((x, visited)); y = teacher_v2(x, gains)
            errors.append(fit_actor(model, x, y, args.round_steps, args.batch, args.learning_rate / 2, generator, log, args.steering_weight))
            rounds.append({'rows': len(x), 'network_outcomes': outcomes, 'mean_absolute_error': errors[-1]})
        export = args.out / 'policy-final.json'; model.export(export)
        bridge.call('load_policy', path=str(export.resolve()))
        summaries = evaluate_sets(bridge, model, args.out, contact=args.contact_share > 0., contract=contract)
    for name, summary in summaries.items():
        log('%s: %d/%d passed, parity %.2e' % (name, summary['passed'], summary['cases'], summary['max_parity']))
        for family, group in summary['by_family'].items():
            log('  %s %d/%d' % (family, group['passed'], group['cases']))
    atomic_json(args.out / 'summary.json', {'kind': KIND, 'contract': contract, 'gains': gains, 'rounds': rounds,
                                            'evaluations': summaries, 'seconds': time.monotonic() - started,
                                            'policy_sha256': sha(export)})
    (args.out / 'log.txt').write_text('\n'.join(log_lines) + '\n')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('mode', choices=('reference', 'fit', 'evaluate'))
    p.add_argument('--bridge', default=DEFAULT_BRIDGE); p.add_argument('--out', type=Path, required=True)
    p.add_argument('--seed', type=int, default=1); p.add_argument('--workers', type=int, default=8)
    p.add_argument('--set', choices=('matrix', 'v1', 'cruise', 'ground', 'falling', 'half_turn', 'drifting_turn', 'tilted_climb', 'blocked'),
                   default='matrix')
    p.add_argument('--contract', type=int, choices=(2, 3), default=CONTRACT,
                   help='the motor command contract: 3 adds the advisory vector with the turn sense')
    p.add_argument('--gains', type=json.loads, default=None, help='JSON overrides of the law gains')
    p.add_argument('--synthetic', type=int, default=400_000); p.add_argument('--episodes', type=int, default=300)
    p.add_argument('--steps', type=int, default=40_000); p.add_argument('--batch', type=int, default=4096)
    p.add_argument('--learning-rate', type=float, default=1e-3); p.add_argument('--rounds', type=int, default=3)
    p.add_argument('--round-episodes', type=int, default=200); p.add_argument('--round-steps', type=int, default=20_000)
    p.add_argument('--threads', type=int, default=8)
    p.add_argument('--width', type=int, default=32, help='actor hidden width, 32 or 64')
    p.add_argument('--steering-weight', type=float, default=1., help='weight of the steering term in the fit loss')
    p.add_argument('--contact-share', type=float, default=0.,
                   help='share of rollout episodes from the contact families, which the evaluation then adds')
    p.add_argument('--policy', type=Path, help='the motor export the evaluate mode flies')
    p.add_argument('--sets', default='ground,falling,half_turn', help='comma-separated evaluation sets for the evaluate mode')
    args = p.parse_args()
    args.out.mkdir(parents=False)
    if args.mode == 'reference':
        reference(args)
    elif args.mode == 'evaluate':
        evaluate_export(args)
    else:
        fit(args)


if __name__ == '__main__':
    main()
