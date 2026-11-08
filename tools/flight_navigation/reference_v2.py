"""Scripted navigation reference for the match envelope: the v1 law in the goal's frame, commanding the v2 motor.

The v1 law pursues a static goal with a stationary-frame speed command. Here
the goal may move with an orbiting planet and the motor's command is relative
to the pilot's planet frame, so the law plans in the goal's own frame, where
the goal is still, and adds the goal's velocity relative to the planet frame
to every command. The sun's heat disk, read from the object record, blocks
route cells and limits speed like sensed geometry. Everything else is the
v1 law: A* on the polar cell map, corridor clearance by the rays, a linear
approach schedule and a nose hold inside the stop radius.
"""
import argparse
import json
import math
from multiprocessing import Pool
from pathlib import Path
import random
import time

from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from . import reference, route
from .reference import DISTANCE_SCALE, VELOCITY_SCALE, clearance_by_heading, safe_speed, wrap

CONTRACT = 2
SUN_RADIUS = 200.
SPIN_SCALE = 1.8
# The turn sense's thresholds, as the motor environment's reference sense in
# scenarios/spacewars/src/surface_sortie/flight_environment/motor.rs has them;
# the shared cases beside it hold the two to the same answers.
SENSE_TOUCH = 6.
SENSE_GATE = .35
SENSE_SPIN = .12
SENSE_STALLED = .06
SENSE_SATURATED = .9
SENSE_AHEAD = .3
# Speeds to 60 and a longer lookahead for transfers; the approach gain and
# braking model are the v1 values. sun_margin inflates the heat disk.
# escape_time/escape_speed: as the time to reach the nearest surface, falling
# under gravity with the closing speed the ship has, drops through that window
# the law adds a velocity away from the surface, up to that speed, as the
# mission's launch climbs before it transfers; sun_escape_depth/sun_escape_speed
# do the same out of the heat disk, by the depth inside it. curve_seconds: the
# clearance is read along the motion gravity will have bent the path to by
# then. motion_weight/motion_ramp: a moving ship's cost for turning away from
# the approach it already flies, that fraction of the aim's distance at a half
# turn, blended in over that speed range.
# speed_margins: extra route margin, corridor half-width and clearance margin
# at full strength, blended in over speed_ramp of the ship's own speed, since a
# fast ship needs room its straight-line clearance does not show; below the v1
# envelope's speeds the v1 margins hold. momentum_speed: the speed above which
# the motion the ship has must stop inside the clearance along it.
# global_range: beyond this goal distance the route runs around the planet and
# sun disks of the object record, with the mission's body margins, and the
# local map pursues that route's first waypoint. detour_commit_speed: the
# tangential speed around a body above which the ship is rounding it and keeps
# its side; below it the ship is drifting, and a drift that crosses the
# threshold must not take the route from one side to the other. A detour
# waypoint outside the enclosing edge by the body margin rounds the other way.
# sun_lead: the sun's detour waypoint lies at its circle's radius over the
# cosine of the lead, so the chase settles on the circle and not inside the
# heat disk, where every heading toward the waypoint is speed-limited (LF-095).
# heat_brake: whether the momentum brake reads the heat disk from outside it;
# off, a ship drifting toward the heat turns along it instead of stopping.
LAW = dict(reference.LAW, contract=CONTRACT, max_speed=60., lookahead=120., brake_acceleration=20., sun_margin=30.,
           escape_time=(2., 4.), escape_speed=25., escape_gravity=(1., 2.), escape_goal_fade=(30., 60.), curve_seconds=.5,
           sun_escape_depth=20., sun_escape_speed=30., speed_margins=(24., 10., 12.), speed_ramp=(20., 45.),
           momentum_speed=20., momentum_brake_acceleration=20., motion_weight=.3, motion_ramp=(5., 20.),
           global_range=240., body_margin=65., detour_margin=105., detour_turn=.5, detour_commit_speed=12., sun_lead=True, heat_brake=False, approach_gain=.8,
           speed_deadband=.5)
# The v3 law: the same commands with the advisory vector, the turn sense first.
LAW_V3 = dict(LAW, contract=3)


def turn_sense(nearest, error, spin, previous_steering):
    """The turn sense: the sign of the spin to turn with when the ground would block the shorter turn, 0 for the motor's own choice.

    With a surface against the hull, within SENSE_TOUCH of the origin, and the
    command outside the thrust gate: a turn under way is kept; a saturated steering whose turn has no spin is a turn the ground
    blocks, and the sense is the other way, the steering's own sign since
    steering is positive clockwise; otherwise, with the surface beside the
    ship, the nose turns away from it, whichever way is shorter. The rule has
    no state: the six-tick hold gives a turn time to build its spin before the
    next reading, a turn that builds it is kept, and one that does not is
    reversed, so a ship blocked both ways rocks between them instead of
    resting on one. A surface on the left, at a positive bearing, is left by
    a clockwise turn, a negative spin.
    """
    if nearest is None:
        return 0.
    distance, bearing = nearest
    if distance > SENSE_TOUCH or abs(error) <= SENSE_GATE:
        return 0.
    if abs(spin) > SENSE_SPIN:
        return math.copysign(1., spin)
    if abs(previous_steering) > SENSE_SATURATED and abs(spin) < SENSE_STALLED:
        return math.copysign(1., previous_steering)
    if SENSE_AHEAD < abs(bearing) < math.pi - SENSE_AHEAD:
        return -math.copysign(1., bearing)
    return 0.


def sense_reading(inputs, error):
    """What the turn sense is read from: the nearest surface within touch as a distance and a bearing from the nose, positive to the left, the motor command's heading error, the spin and the steering applied on the previous tick."""
    n = inputs['nearest']; g = inputs['self_goal']
    nearest = None
    if n[0] > 0. and n[1] * DISTANCE_SCALE <= SENSE_TOUCH:
        nearest = (n[1] * DISTANCE_SCALE, math.atan2(-n[2] * DISTANCE_SCALE, n[3] * DISTANCE_SCALE))
    return nearest, error, g[10] * SPIN_SCALE, g[29]


def advised(command, relative, goal_velocity, inputs, law):
    """The command dict with, under the v3 law, the advisory vector: the turn sense read against the motor's command, the relative command plus the goal's velocity, which the host adds."""
    if law.get('contract') != 3:
        return command
    absolute = (relative[0] + goal_velocity[0], relative[1] + goal_velocity[1])
    heading = math.atan2(command['heading_sin'], command['heading_cos'])
    error = math.atan2(-absolute[0], absolute[1]) if math.hypot(*absolute) >= 1e-6 else heading
    return dict(command, advisory=[turn_sense(*sense_reading(inputs, error)), 0., 0., 0.])


def relative_velocity(inputs):
    """The ship's velocity relative to the pilot's planet frame, in body coordinates."""
    g = inputs['self_goal']
    return (g[4] * VELOCITY_SCALE - g[13] * VELOCITY_SCALE, g[5] * VELOCITY_SCALE - g[14] * VELOCITY_SCALE)


def world_velocity(inputs):
    """The ship's center-of-mass world velocity in body coordinates."""
    g = inputs['self_goal']
    return (g[4] * VELOCITY_SCALE, g[5] * VELOCITY_SCALE)


def geometry_velocity(inputs):
    """The world velocity, in body coordinates, of the surface the rays mostly see: the nearest surface's, or the frame's when nothing is near.

    The pilot's planet frame is a planet the pilot belongs to, not always the
    one the ship is near, and planets orbit at up to twenty units a second;
    a surface's own motion is what the ship must clear and stop against.
    """
    n = inputs['nearest']
    g = inputs['self_goal']
    if n[0] > 0.:
        return (n[4] * VELOCITY_SCALE + g[6] * VELOCITY_SCALE, n[5] * VELOCITY_SCALE + g[7] * VELOCITY_SCALE)
    return (g[13] * VELOCITY_SCALE, g[14] * VELOCITY_SCALE)


def geometry_relative_velocity(inputs):
    """The ship's velocity relative to the surface the rays mostly see, in body coordinates."""
    v = world_velocity(inputs)
    sx, sy = geometry_velocity(inputs)
    return (v[0] - sx, v[1] - sy)


def with_speed_margins(inputs, law):
    """The law with its route margin, corridor half-width and clearance margin widened by the ship's speed over the surfaces."""
    weight = ramp(math.hypot(*geometry_relative_velocity(inputs)), *law['speed_ramp'])
    if weight <= 0.:
        return law
    route_margin, half_width, margin = law['speed_margins']
    return dict(law, route_margin=law['route_margin'] + weight * route_margin, half_width=law['half_width'] + weight * half_width,
                margin=law['margin'] + weight * margin)


class Disks(list):
    """The body disks with the sun's index, which the routing cannot tell from a disk alone."""
    sun = None


def bodies(inputs, law=LAW):
    """Planet and sun disks in body units with their world velocities, each inflated by the mission's body margin, or empty without a sun to fix the scale."""
    sun_row = next((row for row in inputs['objects'] if row[0] > 0. and row[2] > 0. and row[12] > 1e-6), None)
    if sun_row is None:
        return Disks()
    scale = SUN_RADIUS / sun_row[12]
    g = inputs['self_goal']
    disks = Disks()
    for row in inputs['objects']:
        if row[0] <= 0. or not (row[1] > 0. or row[2] > 0.):
            continue
        radius = max(row[12], row[13]) * scale + (law['sun_margin'] if row[2] > 0. else law['body_margin'])
        velocity = (row[7] * VELOCITY_SCALE + g[6] * VELOCITY_SCALE, row[8] * VELOCITY_SCALE + g[7] * VELOCITY_SCALE)
        if row[2] > 0.:
            disks.sun = len(disks)
        disks.append(((row[5] * scale, row[6] * scale), radius, velocity))
    return disks


def enclosing_edge(inputs, law=LAW):
    """The arena's enclosing edge as a disk in body units, from its object record, or None without one or a sun to fix the scale."""
    sun_row = next((row for row in inputs['objects'] if row[0] > 0. and row[2] > 0. and row[12] > 1e-6), None)
    edge_row = next((row for row in inputs['objects'] if row[0] > 0. and row[3] > 0. and row[12] > 1e-6), None)
    if sun_row is None or edge_row is None:
        return None
    scale = SUN_RADIUS / sun_row[12]
    return (edge_row[5] * scale, edge_row[6] * scale), edge_row[12] * scale


def route_aim(goal, disks, law=LAW, motion=(0., 0.), edge=None):
    """The mission's planet routing on the object record: the first disk the leg to the aim crosses is rounded by a waypoint beside it, rechecked once per body.

    `motion` is the ship's world velocity in body coordinates: behind a body
    the side to round is a toss-up that a ship moving around the body has
    settled, so it keeps rounding the way it already moves. `edge` is the
    enclosing edge's disk; a waypoint beyond it, less the body margin, is no
    way around. A ship chasing a waypoint that keeps a fixed lead ahead on a
    circle settles at the circle's radius times the cosine of the lead,
    which for the sun lies inside the heat disk the speed limit keeps clear;
    with sun_lead the sun's waypoint lies at the circle's radius over that
    cosine, so the chase settles on the circle.
    """
    aim = goal
    visited = []
    for _ in range(len(disks)):
        length = math.hypot(*aim)
        if length < 1e-6:
            break
        direction = (aim[0] / length, aim[1] / length)

        def encounter(center, radius):
            offset = center
            along = offset[0] * direction[0] + offset[1] * direction[1]
            if math.hypot(*offset) < radius and along <= 0.:
                return math.inf
            # Distance from the center to the leg from the ship to the aim.
            t = max(0., min(1., along / length))
            closest = (aim[0] * t, aim[1] * t)
            if math.hypot(center[0] - closest[0], center[1] - closest[1]) >= radius:
                return math.inf
            perpendicular = max(0., math.hypot(*offset) ** 2 - along * along)
            return max(0., along - math.sqrt(max(0., radius * radius - perpendicular)))
        candidates = [(encounter(c, r), i) for i, (c, r, _) in enumerate(disks) if i not in visited]
        candidates = [c for c in candidates if math.isfinite(c[0])]
        if not candidates:
            break
        _, index = min(candidates)
        center, radius, velocity = disks[index]
        radial = (-center[0], -center[1]); norm = math.hypot(*radial)
        if norm < 1e-6:
            break
        radial = (radial[0] / norm, radial[1] / norm)
        toward = (aim[0] - center[0], aim[1] - center[1]); tnorm = math.hypot(*toward)
        toward = (toward[0] / tnorm, toward[1] / tnorm) if tnorm > 1e-6 else radial
        angle = math.atan2(radial[0] * toward[1] - radial[1] * toward[0], radial[0] * toward[0] + radial[1] * toward[1])
        turn = law['detour_turn'] if abs(angle) < .01 else max(-law['detour_turn'], min(law['detour_turn'], angle))
        around = (motion[0] - velocity[0], motion[1] - velocity[1])
        tangential = radial[0] * around[1] - radial[1] * around[0]
        if abs(angle) > math.pi / 2 and abs(tangential) > law['detour_commit_speed']:
            turn = math.copysign(law['detour_turn'], tangential)
        reach = radius - (law['body_margin'] if radius > law['sun_margin'] else 0.) + law['detour_margin']
        lead = law.get('sun_lead', False) and index == getattr(disks, 'sun', None)

        def waypoint(turn):
            c, s_ = math.cos(turn), math.sin(turn)
            rotated = (radial[0] * c - radial[1] * s_, radial[0] * s_ + radial[1] * c)
            distance = reach / max(c, .5) if lead else reach
            return (center[0] + rotated[0] * distance, center[1] + rotated[1] * distance)

        def inside_another(point):
            return (any(j != index and math.hypot(point[0] - c[0], point[1] - c[1]) < r for j, (c, r, _) in enumerate(disks))
                    or (edge is not None and math.hypot(point[0] - edge[0][0], point[1] - edge[0][1]) > edge[1] - law['body_margin']))

        # A waypoint beside this body that lies inside another body's disk,
        # the sun's beyond a planet at the heat's edge, or beyond the enclosing
        # edge, rounds the other way.
        aim = waypoint(turn)
        if inside_another(aim) and not inside_another(waypoint(-turn)):
            aim = waypoint(-turn)
        visited.append(index)
    return aim
STATIC_FAMILIES = ('clear', 'obstructed', 'altered', 'moving')


def goal_frame(inputs):
    """The goal in body units, and the goal's velocity relative to the pilot's planet frame in body coordinates."""
    g = inputs['self_goal']
    goal = (g[0] * DISTANCE_SCALE, g[1] * DISTANCE_SCALE)
    target_minus_com = (g[2] * VELOCITY_SCALE, g[3] * VELOCITY_SCALE)
    com = (g[4] * VELOCITY_SCALE, g[5] * VELOCITY_SCALE)
    frame = (g[13] * VELOCITY_SCALE, g[14] * VELOCITY_SCALE)
    return goal, (target_minus_com[0] + com[0] - frame[0], target_minus_com[1] + com[1] - frame[1])


def sun_disk(inputs, law=LAW):
    """The sun's heat disk in body units, from its object record; the solid radius fixes the world scale."""
    for row in inputs['objects']:
        if row[0] > 0. and row[2] > 0. and row[12] > 1e-6:
            scale = law['sun_radius'] / row[12] if 'sun_radius' in law else SUN_RADIUS / row[12]
            return (row[5] * scale, row[6] * scale), row[13] * scale + law['sun_margin']
    return None


def disk_clearance(center, radius, heading):
    """Free distance along a body heading before entering a disk, or the map range when the ray misses it.

    From inside the disk, a heading that leads outward is free and one that
    leads further in is blocked, so a ship caught at the heat's edge leaves it.
    """
    dx, dy = -math.sin(heading), math.cos(heading)
    along = center[0] * dx + center[1] * dy
    if math.hypot(*center) < radius:
        return DISTANCE_SCALE if along < 0. else 0.
    across = math.hypot(center[0] - along * dx, center[1] - along * dy)
    if across >= radius or along <= 0.:
        return DISTANCE_SCALE
    return min(DISTANCE_SCALE, max(0., along - math.sqrt(radius * radius - across * across)))


def with_sun_blocked(cells, edges, sun):
    """Occupancy cells with every cell whose center lies in the heat disk marked as touched."""
    if sun is None:
        return cells
    (cx, cy), radius = sun
    cells = [list(c) for c in cells]
    for band in range(len(edges) - 1):
        for sector in range(route.SECTORS):
            x, y = route.cell_center(edges, band, sector)
            if (x - cx) ** 2 + (y - cy) ** 2 < radius * radius:
                cells[band * route.SECTORS + sector][0] = 1.
    return cells


def ramp(value, low, high):
    return min(1., max(0., (value - low) / (high - low)))


def time_to_impact(distance, closing, gravity):
    """Seconds until a surface at `distance` is reached, closing at `closing` with `gravity` pulling toward it."""
    if gravity > 1e-6:
        return (-closing + math.sqrt(closing * closing + 2. * gravity * distance)) / gravity
    return distance / closing if closing > 1e-6 else math.inf


def escape(inputs, goal_distance, law=LAW):
    """The fall guard: a body-frame velocity away from the nearest surface, relative to that surface, and its weight in [0, 1].

    The weight rises as the time to reach the nearest surface, falling under
    the gravity there with the closing speed the ship has against it, drops
    through the escape window; it is zero away from gravity and near goals,
    which the admission screens keep clear. The pursuit is scaled down by
    the same weight, so near a surface under gravity the ship climbs before
    it pursues.
    """
    nearest = inputs['nearest']
    if nearest[0] <= 0.:
        return (0., 0.), 0.
    distance = nearest[1] * DISTANCE_SCALE
    gravity = math.hypot(inputs['self_goal'][11], inputs['self_goal'][12]) * 45.
    px, py = nearest[2] * DISTANCE_SCALE, nearest[3] * DISTANCE_SCALE
    norm = math.hypot(px, py)
    if norm < 1e-6:
        return (0., 0.), 0.
    vx, vy = geometry_relative_velocity(inputs)
    closing = (vx * px + vy * py) / norm
    weight = (ramp(gravity, *law['escape_gravity']) * ramp(goal_distance, *law['escape_goal_fade'])
              * (1. - ramp(time_to_impact(distance, closing, gravity), *law['escape_time'])))
    if weight <= 0.:
        return (0., 0.), 0.
    strength = law['escape_speed'] * weight
    return (-px / norm * strength, -py / norm * strength), weight


def sun_escape(sun, law=LAW):
    """A body-frame velocity out of the sun's inflated heat disk and its weight in [0, 1], rising with the depth inside."""
    if sun is None:
        return (0., 0.), 0.
    (cx, cy), radius = sun
    distance = math.hypot(cx, cy)
    weight = ramp(radius - distance, 0., law['sun_escape_depth'])
    if weight <= 0. or distance < 1e-6:
        return (0., 0.), 0.
    strength = law['sun_escape_speed'] * weight
    return (-cx / distance * strength, -cy / distance * strength), weight


def world_clearances(inputs, bearings, sun, vx, vy, law):
    """Clearances along a body-frame world velocity after gravity has bent it for the law's curve time.

    The sensed surfaces are read along the motion relative to them, since
    they move with their planet; the sun's heat disk is fixed in the world.
    """
    g = inputs['self_goal']
    bx, by = vx + g[11] * 45. * law['curve_seconds'], vy + g[12] * 45. * law['curve_seconds']
    sx, sy = geometry_velocity(inputs)
    rx, ry = bx - sx, by - sy
    surfaces = DISTANCE_SCALE
    if math.hypot(rx, ry) >= 1e-3:
        surfaces = clearance_by_heading(inputs['rays'], bearings, [math.atan2(-rx, ry)], law['half_width'])[0]
    heat = DISTANCE_SCALE
    if sun is not None and math.hypot(bx, by) >= 1e-3:
        heat = disk_clearance(sun[0], sun[1] + law['half_width'], math.atan2(-bx, by))
    return surfaces, heat


def world_clearance(inputs, bearings, sun, vx, vy, law):
    """The smaller of the surface and heat clearances along a body-frame world velocity."""
    return min(world_clearances(inputs, bearings, sun, vx, vy, law))


def momentum_brake(inputs, bearings, sun, law):
    """Whether the motion the ship has cannot stop inside the clearance along it.

    Speed limits on the commanded heading assume the ship already moves that
    way. Above the v1 envelope's speeds it may not, so the motion it has must
    also stop inside the clearance it has; when it cannot, the law commands
    a stop, which the motor brakes toward at its full authority. Outside the
    heat disk, without heat_brake, the brake reads the surfaces alone: a
    ship drifting toward the heat turns along it rather than stopping before
    it, and inside the disk the sun escape takes over.
    """
    world = world_velocity(inputs)
    against = geometry_relative_velocity(inputs)
    if not law.get('heat_brake', True) and sun is not None and math.hypot(*sun[0]) > sun[1]:
        sun = None
    surfaces, heat = world_clearances(inputs, bearings, sun, world[0], world[1], law)
    braking = dict(law, brake_acceleration=law['momentum_brake_acceleration'])
    return any(speed >= law['momentum_speed'] and speed > safe_speed(clearance, braking) + 2.
               for speed, clearance in ((math.hypot(*against), surfaces), (math.hypot(*world), heat)))


def motion_clearance(inputs, bearings, sun, approach, speed, goal_velocity, law):
    """Clearance along the direction the ship would actually move in the world under this approach.

    The approach is planned in the goal's frame, but the sensed geometry is
    fixed in the world, so the clearance that limits speed is the one along
    the world velocity the command produces, after gravity has bent that
    velocity for the law's curve time.
    """
    g = inputs['self_goal']
    frame = (g[13] * VELOCITY_SCALE, g[14] * VELOCITY_SCALE)
    return world_clearance(inputs, bearings, sun,
                           -math.sin(approach) * speed + goal_velocity[0] + frame[0],
                           math.cos(approach) * speed + goal_velocity[1] + frame[1], law)


def command(inputs, bearings, law=LAW, edges=None, trace=None):
    """One relative heading and speed command for the v2 motor from the flight policy observation.

    A dict passed as `trace` receives the decision's internals: the routed aim,
    the local waypoint and every candidate's cost.
    """
    law = with_speed_margins(inputs, law)
    goal, goal_velocity = goal_frame(inputs)
    distance = math.hypot(*goal)
    sun = sun_disk(inputs, law)
    climb, climb_weight = escape(inputs, distance, law)
    outward, outward_weight = sun_escape(sun, law)
    escape_weight = max(climb_weight, outward_weight)
    g = inputs['self_goal']
    frame = (g[13] * VELOCITY_SCALE, g[14] * VELOCITY_SCALE)
    world = world_velocity(inputs)
    surface = geometry_velocity(inputs)
    # The ways out are world velocities: a climb rides the surface it leaves,
    # the way out of the heat is fixed in the world; the pursuit fades out as
    # they fade in. The command is relative to the goal, so a world velocity
    # takes the frame's and the goal's velocities away.
    ways = (climb_weight * surface[0] + climb[0] + outward[0], climb_weight * surface[1] + climb[1] + outward[1])

    def goal_relative(target):
        return (target[0] - frame[0] - goal_velocity[0], target[1] - frame[1] - goal_velocity[1])

    relative = relative_velocity(inputs)
    aim = goal
    if distance > law['global_range']:
        # Far goals are routed around the bodies of the object record first.
        aim = route_aim(goal, bodies(inputs, law), law, world, enclosing_edge(inputs, law))
    if trace is not None:
        trace.update(goal=goal, routed=aim, waypoint=None)
    if edges is not None and distance > law['hold_radius']:
        waypoint = route.plan(with_sun_blocked(inputs['cells'], edges, sun), edges, aim, law['route_margin'], law['route_lead'])
        if waypoint is not None:
            aim = waypoint
            if trace is not None:
                trace['waypoint'] = waypoint
    goal_heading = math.atan2(-aim[0], aim[1]) if math.hypot(*aim) > 1e-6 else 0.
    offsets = [k * law['candidate_step'] for k in range(-law['candidates'], law['candidates'] + 1)]
    headings = [wrap(goal_heading + o) for o in offsets]
    # The approach crawls into the stop radius under the arrival velocity
    # tolerance, as the v1 law does: a network's blurred copy of the stop
    # then creeps within tolerance instead of overshooting or circling.
    goal_speed = min(law['max_speed'], law['approach_gain'] * max(0., distance - law['approach_offset']))
    # Candidates are scored by their free reach toward the aim in the goal's
    # frame; the clearance of each is read along its actual world motion. A
    # moving ship also pays for turning away from the approach it already
    # flies, a fraction of the aim's distance at a half turn, so two detours of
    # equal reach do not take turns from one decision to the next while a
    # clear preference still wins.
    approach = (relative[0] - goal_velocity[0], relative[1] - goal_velocity[1])
    commitment = law['motion_weight'] * ramp(math.hypot(*approach), *law['motion_ramp']) * math.hypot(*aim) / math.pi
    motion_heading = math.atan2(-approach[0], approach[1]) if commitment > 0. else 0.
    best = None
    scored = []
    for offset, heading in zip(offsets, headings):
        clearance = motion_clearance(inputs, bearings, sun, heading, goal_speed, goal_velocity, law)
        reach = min(clearance - law['margin'], law['lookahead'], math.hypot(*aim))
        end = (-math.sin(heading) * reach, math.cos(heading) * reach)
        remaining = math.hypot(aim[0] - end[0], aim[1] - end[1])
        blocked = max(0., law['margin'] - clearance) / law['margin']
        cost = remaining + law['blocked_weight'] * blocked + .5 * abs(offset) + commitment * abs(wrap(heading - motion_heading))
        scored.append((cost, heading, clearance, remaining))
        if best is None or cost < best[0]:
            best = (cost, heading, clearance)
    _, heading, clearance = best
    if trace is not None:
        trace.update(candidates=sorted(scored)[:4], commitment=commitment, motion_heading=motion_heading, goal_speed=goal_speed, away=ways)
    if momentum_brake(inputs, bearings, sun, law):
        # Stop against the surfaces the ship moves toward before pursuing
        # anything, with the nose turning toward the free way on; a climb or
        # the way out of the heat is kept.
        fade = 1. - escape_weight
        command = goal_relative((surface[0] * fade + ways[0], surface[1] * fade + ways[1]))
        command_speed = min(law['max_speed'], math.hypot(*command))
        if command_speed >= law['speed_deadband']:
            command_heading = math.atan2(-command[0], command[1])
        else:
            command_heading, command_speed = (heading if clearance > law['margin'] else 0.), 0.
        relative = (-math.sin(command_heading) * command_speed, math.cos(command_heading) * command_speed)
        return (advised({'heading_sin': math.sin(command_heading), 'heading_cos': math.cos(command_heading), 'speed': command_speed},
                        relative, goal_velocity, inputs, law),
                {'distance': distance, 'clearance': 0., 'goal_speed': 0., 'goal_velocity': math.hypot(*goal_velocity), 'braking': True})
    speed = min(goal_speed, safe_speed(clearance, law))
    if speed < goal_speed:
        # A slower approach moves differently through the world; read its clearance again.
        clearance = min(clearance, motion_clearance(inputs, bearings, sun, heading, speed, goal_velocity, law))
        speed = min(speed, safe_speed(clearance, law))
    # The approach is planned in the goal's frame and is continuous through
    # the stop radius, where its speed is already zero. The pursuit, the
    # approach carried by the goal, fades out as the ways out fade in; the
    # v2 command is the result relative to the goal, which the host adds
    # back exactly. A speed under the contract's deadband is a stop.
    fade = 1. - escape_weight
    pursuit = (-math.sin(heading) * speed + goal_velocity[0] + frame[0], math.cos(heading) * speed + goal_velocity[1] + frame[1])
    desired = goal_relative((pursuit[0] * fade + ways[0], pursuit[1] * fade + ways[1]))
    command_speed = min(law['max_speed'], math.hypot(*desired))
    if command_speed < law['speed_deadband']:
        command_heading, command_speed = (0. if distance < law['stop_radius'] else heading), 0.
    else:
        command_heading = math.atan2(-desired[0], desired[1])
    relative = (-math.sin(command_heading) * command_speed, math.cos(command_heading) * command_speed)
    return (advised({'heading_sin': math.sin(command_heading), 'heading_cos': math.cos(command_heading), 'speed': command_speed},
                    relative, goal_velocity, inputs, law),
            {'distance': distance, 'clearance': clearance, 'goal_speed': goal_speed, 'goal_velocity': math.hypot(*goal_velocity)})


def transfer_sets(bridge, seed, count, max_attempts=None, distances='standard'):
    """Admitted match-transfer tasks from the generator, in attempt order: the standard range to 600 units, or the long range beyond it as the long_transfer family."""
    long = distances == 'long'
    batch = bridge.call('generate_transfers', seed=seed, count=count, max_attempts=max_attempts or count * (16 if long else 8), range=distances)
    family = 'long_transfer' if long else 'transfer'
    rows = []
    for attempt in batch['attempts']:
        if attempt['admitted']:
            rows.append({'index': len(rows), 'family': family, 'recipe': family,
                         'candidate_index': attempt['candidate']['index'], 'distance': attempt['distance'],
                         'reference_time_seconds': attempt['reference_time_seconds'], 'task': attempt['candidate']['task']})
    return rows


GROUNDED_ALTITUDE = 15.
GROUNDED_FALLS = (0., 8., 12.)
# The settle of up to 150 ticks counts toward the deadline, and the law itself
# needs up to 850 ticks on the family, so 900 raced the networks against it.
GROUNDED_DEADLINE = 1200
GROUNDED_CLIMB = 85.


def grounded_sets(seed, count):
    """Climbs from the ground above the practice planet, the grounded family: the ship starts at the task contract's lowest altitude with its nose at any angle, at rest or falling, and comes down while the goal waits up the radial, with contact allowed, so the law's turn sense is read where the ground blocks a turn (LF-100)."""
    rng = random.Random(seed); rows = []
    for index in range(count):
        attitude = rng.uniform(-math.pi, math.pi); bearing = rng.uniform(-math.pi, math.pi); fall = rng.choice(GROUNDED_FALLS)
        task = {'start': {'seed': rng.randrange(1 << 31), 'bearing': bearing, 'altitude': GROUNDED_ALTITUDE, 'heading_offset': attitude,
                          'relative_velocity': {'x': math.sin(bearing) * fall, 'y': -math.cos(bearing) * fall}, 'spin': 0., 'gravity': True,
                          'planet_spin': rng.choice([-.015, 0., .015])},
                'goal_offset': {'x': -math.sin(bearing) * GROUNDED_CLIMB, 'y': math.cos(bearing) * GROUNDED_CLIMB}, 'goal_velocity': {'x': 0., 'y': 0.},
                'arrival': {'position': 3., 'velocity': 1., 'spin': .2, 'dwell_ticks': 30}, 'deadline_ticks': GROUNDED_DEADLINE,
                'obstacles': [], 'terrain_edits': [], 'contact_allowed': True}
        rows.append({'index': index, 'family': 'grounded', 'recipe': 'attitude%+.2f' % attitude, 'settle': True, 'task': task})
    return rows


def generated_sets(bridge, seed, per_family, max_rounds=4000):
    """The practice generator's four families with their seeded gravity and spin kept."""
    batch = bridge.call('generate', seed=seed, per_family=per_family, max_rounds=max_rounds)
    if not batch['complete']:
        raise RuntimeError('task generation quota incomplete')
    rows = []
    for attempt in batch['attempts']:
        if attempt['admitted']:
            candidate = attempt['candidate']
            rows.append({'index': len(rows), 'family': candidate['family'], 'recipe': candidate['recipe'],
                         'candidate_index': candidate['index'], 'task': candidate['task']})
    return rows


SETTLE_LIMIT = 150
SETTLE_REST = 10
NEUTRAL = {'steering': 0., 'thrust': 0., 'brake': 0.}
STOP = {'heading_sin': 0., 'heading_cos': 1., 'speed': 0.}


def settle(bridge, item):
    """Neutral controls until the ship has rested in contact for ten ticks, at most 150: the grounded family begins on the ground, as the motor's contact sets do; a stop is held meanwhile, since the bridge steps actions under a held command."""
    bridge.call('set_command', commands=[STOP])
    ticks = 0; resting = 0
    while item['progress']['outcome'] is None and resting < SETTLE_REST and ticks < SETTLE_LIMIT:
        item = bridge.call('step_actions', actions=[NEUTRAL])['items'][0]; ticks += 1
        motor = item['motor_inputs'] or [0.] * 5
        speed = math.hypot(motor[3], motor[4]) * VELOCITY_SCALE
        resting = resting + 1 if item['evidence'] and item['evidence']['collision'] and speed < 1. else 0
    return item


def run_row(bridge, row, law, bearings, edges):
    item = bridge.call('reset', tasks=[row['task']])['items'][0]
    if row.get('settle'):
        item = settle(bridge, item)
    decisions = 0; min_clearance = DISTANCE_SCALE; max_speed = 0.
    while item['progress']['outcome'] is None:
        action, info = command(item['inputs'], bearings, law, edges)
        min_clearance = min(min_clearance, info['clearance']); max_speed = max(max_speed, action['speed'])
        item = bridge.call('step_command', commands=[action], hold=law['hold'])['items'][0]
        decisions += 1
    return finish_row(bridge, row, item, decisions, min_clearance, max_speed)


def finish_row(bridge, row, item, decisions, min_clearance, max_speed):
    f = item['frame']
    # The motor's share of ticks spinning above 1.5 radians per second and at
    # the steering limit, from the environment's own trace, to compare the law
    # with the navigation network flying the same motor.
    steps = bridge.call('state')['traces'][0]['steps']
    spinning = sum(1 for s in steps if s['frame'] and abs(s['frame']['spin']) > 1.5)
    saturated = sum(1 for s in steps if s['action'] and abs(s['action']['steering']) > .95)
    # The first tick within 50 units of the goal, the mission's reach of a
    # handoff, since a long leg is judged by its approach as much as by its
    # arrival.
    within_50 = next((i + 1 for i, s in enumerate(steps) if s['frame'] and s['frame']['errors']['position'] <= 50.), None)
    return {'index': row['index'], 'family': row['family'], 'recipe': row['recipe'], 'outcome': item['progress']['outcome'],
            'ticks': item['progress']['elapsed_ticks'], 'decisions': decisions, 'ticks_within_50': within_50,
            'final_distance': math.hypot(f['target_position']['x'] - f['center_of_mass']['x'], f['target_position']['y'] - f['center_of_mass']['y']),
            'final_relative_speed': math.hypot(f['velocity']['x'] - f['target_velocity']['x'], f['velocity']['y'] - f['target_velocity']['y']),
            'final_spin': abs(f['spin']), 'min_clearance': min_clearance, 'max_command_speed': max_speed,
            'deadline_ticks': row['task']['deadline_ticks'],
            'spinning_share': spinning / max(1, len(steps)), 'steering_saturation': saturated / max(1, len(steps))}


def run_row_teacher(bridge, row, law, bearings, edges, gains):
    """The law's commands flown by the scripted motor teacher instead of the motor export.

    The loaded motor export only lends its contract: the bridge holds each
    command and the teacher acts on the motor inputs tick by tick, so the
    motor network's imitation drops out of the comparison.
    """
    from tools.flight_motor import teacher_v2
    gains = dict(teacher_v2.GAINS, **gains)
    item = bridge.call('reset', tasks=[row['task']])['items'][0]
    decisions = 0; min_clearance = DISTANCE_SCALE; max_speed = 0.
    while item['progress']['outcome'] is None:
        action, info = command(item['inputs'], bearings, law, edges)
        min_clearance = min(min_clearance, info['clearance']); max_speed = max(max_speed, action['speed'])
        item = bridge.call('set_command', commands=[action])['items'][0]
        for _ in range(law['hold']):
            if item['progress']['outcome'] is not None:
                break
            motor_action = teacher_v2.scalar_teacher_v2(item['motor_inputs'], gains)
            item = bridge.call('step_actions', actions=[motor_action])['items'][0]
        decisions += 1
    return finish_row(bridge, row, item, decisions, min_clearance, max_speed)


def _shard(job):
    bridge_path, motor, rows, law, teacher = job
    with Bridge(bridge_path) as bridge:
        bridge.call('load_motor_policy', path=motor)
        bearings = bridge.contract['schema']['ray_bearings']; edges = bridge.contract['schema']['radial_edges']
        if teacher is None:
            return [run_row(bridge, row, law, bearings, edges) for row in rows]
        return [run_row_teacher(bridge, row, law, bearings, edges, teacher) for row in rows]


def run(bridge_path, motor, rows, law=LAW, workers=8, teacher=None):
    chunks = [rows[i::workers] for i in range(workers)]
    with Pool(workers) as pool:
        shards = pool.map(_shard, [(bridge_path, motor, chunk, law, teacher) for chunk in chunks if chunk])
    results = [r for shard in shards for r in shard]
    results.sort(key=lambda r: r['index'])
    return results


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, required=True); p.add_argument('--bridge', default=reference.DEFAULT_BRIDGE)
    p.add_argument('--motor', required=True, help='a v2 planet-frame motor export')
    p.add_argument('--set', choices=('transfer', 'long_transfer', 'generated'), default='transfer')
    p.add_argument('--seed', type=int, default=7700101); p.add_argument('--count', type=int, default=24)
    p.add_argument('--workers', type=int, default=8); p.add_argument('--law', type=json.loads, default=None)
    p.add_argument('--teacher', type=json.loads, default=None,
                   help='fly the commands with the scripted motor teacher under these JSON gain overrides instead of the motor export')
    args = p.parse_args()
    law = dict(LAW, **(args.law or {}))
    args.out.mkdir(parents=True, exist_ok=False); started = time.monotonic()
    motor = str(Path(args.motor).resolve())
    with Bridge(args.bridge) as bridge:
        if args.set == 'generated':
            rows = generated_sets(bridge, args.seed, args.count)
        else:
            rows = transfer_sets(bridge, args.seed, args.count, distances='long' if args.set == 'long_transfer' else 'standard')
    atomic_json(args.out / 'tasks.json', rows)
    results = run(args.bridge, motor, rows, law, args.workers, args.teacher)
    summary = {'law': law, 'set': args.set, 'seed': args.seed, 'motor': motor, 'motor_sha256': sha(motor), 'teacher': args.teacher,
               'bridge_sha256': sha(args.bridge), 'summary': reference.summarize(results), 'seconds': time.monotonic() - started}
    # Tick-weighted shares over every case, as the stack harness traces report them.
    ticks = sum(r['ticks'] for r in results)
    for key in ('spinning_share', 'steering_saturation'):
        summary['summary'][key] = sum(r[key] * r['ticks'] for r in results) / max(1, ticks)
    atomic_json(args.out / 'results.json', results); atomic_json(args.out / 'summary.json', summary)
    print(json.dumps(summary['summary'], indent=1))


if __name__ == '__main__':
    main()
