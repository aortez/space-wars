"""Scripted navigation reference: pursue the goal, keep clear by the rays, command the motor network.

This is the feasibility reference for the navigation stage, written without
reference to the native guidance code. It reads only the flight policy
observation the network would see, emits relative heading and speed commands
every `hold` ticks, and the exported motor network owns the actuators inside
the composed bridge. Passing the four task families here is the precondition
for fitting a navigation network to it.
"""
import argparse
import json
import math
from pathlib import Path
import time

from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from . import route

DEFAULT_BRIDGE = 'target/ci/examples/flight_navigation_bridge'
DEFAULT_MOTOR = 'data/training-experiments/learned-flight-motor-teacher-fit-v4-20261025/policy-final.json'
DISTANCE_SCALE = 300.
VELOCITY_SCALE = 70.
FAMILIES = ['clear', 'moving', 'obstructed', 'altered']
# Approach: speed falls linearly with distance to the goal and is zero inside the
# stop radius, where the nose is also held so the goal direction's swing past
# the ship cannot spin it; routing is skipped inside the hold radius, where the
# goal is sensed directly. Route choice: every candidate heading
# around the full circle is scored by how far from the goal its free path
# ends, with the free length read from the rays and capped at the lookahead,
# plus a penalty when that free length cannot absorb braking with margin. The
# command speed is what the chosen heading's clearance allows.
LAW = {
    'hold': 6, 'max_speed': 20., 'approach_gain': .6, 'approach_offset': 2., 'stop_radius': 3., 'hold_radius': 8.,
    'brake_acceleration': 20., 'margin': 8., 'half_width': 9., 'candidate_step': math.radians(7.5),
    'candidates': 24, 'lookahead': 60., 'blocked_weight': 30., 'route_margin': 9., 'route_lead': 12.,
}


def wrap(angle):
    return math.atan2(math.sin(angle), math.cos(angle))


def ray_heading(bearing):
    """Relative heading of a ray bearing: bearings run from body +x, headings from forward with left positive."""
    return wrap(bearing - math.pi / 2)


def clearance_by_heading(rays, bearings, headings, half_width):
    """Free distance along a corridor of the given half-width around each candidate heading.

    A ray hit at distance r and relative angle a blocks the corridor when its
    lateral offset r sin a is inside the half-width and it lies ahead; the
    corridor's clearance is the smallest along-track distance r cos a of such
    hits. Unhit rays do not limit it.
    """
    hits = [(ray_heading(b), r[1] * DISTANCE_SCALE) for b, r in zip(bearings, rays) if r[0] > 0.]
    result = []
    for c in headings:
        clearance = DISTANCE_SCALE
        for h, d in hits:
            a = wrap(h - c)
            if abs(a) < math.pi / 2 and d * abs(math.sin(a)) < half_width:
                clearance = min(clearance, d * math.cos(a))
        result.append(clearance)
    return result


def safe_speed(clearance, law=LAW):
    return math.sqrt(max(0., 2. * law['brake_acceleration'] * (clearance - law['margin'])))


def command(inputs, bearings, law=LAW, edges=None):
    """One relative heading and speed command from the flight policy observation.

    With the radial edges given, the heading candidates are scored toward the
    first waypoint of an A* route over the sensed cell map instead of the raw
    goal direction, so walls are rounded rather than approached.
    """
    gx, gy = inputs['self_goal'][0] * DISTANCE_SCALE, inputs['self_goal'][1] * DISTANCE_SCALE
    distance = math.hypot(gx, gy)
    aim = (gx, gy)
    if edges is not None and distance > law['hold_radius']:
        waypoint = route.plan(inputs['cells'], edges, (gx, gy), law['route_margin'], law['route_lead'])
        if waypoint is not None:
            aim = waypoint
    goal_heading = math.atan2(-aim[0], aim[1]) if math.hypot(*aim) > 1e-6 else 0.
    offsets = [k * law['candidate_step'] for k in range(-law['candidates'], law['candidates'] + 1)]
    headings = [wrap(goal_heading + o) for o in offsets]
    clearances = clearance_by_heading(inputs['rays'], bearings, headings, law['half_width'])
    goal_speed = min(law['max_speed'], law['approach_gain'] * max(0., distance - law['approach_offset']))
    best = None
    for offset, heading, clearance in zip(offsets, headings, clearances):
        reach = min(clearance - law['margin'], law['lookahead'], math.hypot(*aim))
        end = (-math.sin(heading) * reach, math.cos(heading) * reach)
        remaining = math.hypot(aim[0] - end[0], aim[1] - end[1])
        blocked = max(0., law['margin'] - clearance) / law['margin']
        cost = remaining + law['blocked_weight'] * blocked + .5 * abs(offset)
        if best is None or cost < best[0]:
            best = (cost, heading, clearance)
    _, heading, clearance = best
    speed = min(goal_speed, safe_speed(clearance, law))
    if distance < law['stop_radius']:
        speed = 0.
    if distance < law['stop_radius']:
        heading = 0.
    return {'heading_sin': math.sin(heading), 'heading_cos': math.cos(heading), 'speed': speed}, {'distance': distance, 'clearance': clearance, 'goal_speed': goal_speed}


def stationary_zero_gravity(candidate):
    """The v1 envelope: the admitted geometry with gravity off and the planet stationary."""
    row = json.loads(json.dumps(candidate))
    row['task']['start']['gravity'] = False
    row['task']['start']['planet_spin'] = 0.
    return row


def task_sets(bridge, seed, per_family, max_rounds=4000):
    batch = bridge.call('generate', seed=seed, per_family=per_family, max_rounds=max_rounds)
    if not batch['complete']:
        raise RuntimeError('task generation quota incomplete')
    rows = [stationary_zero_gravity(a['candidate']) for a in batch['attempts'] if a['admitted']]
    for index, row in enumerate(rows):
        row['index'] = index
    return rows


def run(bridge, rows, law=LAW, log=None):
    bearings = bridge.contract['schema']['ray_bearings']; edges = bridge.contract['schema']['radial_edges']
    results = []
    for row in rows:
        item = bridge.call('reset', tasks=[row['task']])['items'][0]
        decisions = 0; min_clearance = DISTANCE_SCALE; max_speed = 0.
        while item['progress']['outcome'] is None:
            action, info = command(item['inputs'], bearings, law, edges)
            min_clearance = min(min_clearance, info['clearance']); max_speed = max(max_speed, action['speed'])
            item = bridge.call('step_command', commands=[action], hold=law['hold'])['items'][0]
            decisions += 1
        f = item['frame']
        results.append({'index': row['index'], 'family': row['family'], 'recipe': row['recipe'], 'outcome': item['progress']['outcome'],
                        'ticks': item['progress']['elapsed_ticks'], 'decisions': decisions,
                        'final_distance': math.hypot(f['target_position']['x'] - f['center_of_mass']['x'], f['target_position']['y'] - f['center_of_mass']['y']),
                        'final_relative_speed': math.hypot(f['velocity']['x'] - f['target_velocity']['x'], f['velocity']['y'] - f['target_velocity']['y']),
                        'min_clearance': min_clearance, 'max_command_speed': max_speed, 'deadline_ticks': row['task']['deadline_ticks']})
        if log:
            log('%-10s %-28s %-9s ticks %4d of %4d  final distance %6.1f  min clearance %6.1f' % (
                row['family'], row['recipe'][:28], results[-1]['outcome'], results[-1]['ticks'], row['task']['deadline_ticks'], results[-1]['final_distance'], min_clearance))
    return results


def summarize(results):
    groups = {}
    for r in results:
        g = groups.setdefault(r['family'], {'cases': 0, 'passed': 0, 'outcomes': {}, 'ticks': []})
        g['cases'] += 1; g['passed'] += r['outcome'] == 'success'; g['outcomes'][r['outcome']] = g['outcomes'].get(r['outcome'], 0) + 1
        if r['outcome'] == 'success':
            g['ticks'].append(r['ticks'])
    for g in groups.values():
        ticks = g.pop('ticks'); g['mean_success_ticks'] = sum(ticks) / len(ticks) if ticks else None
    return {'cases': len(results), 'passed': sum(r['outcome'] == 'success' for r in results), 'families': groups}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=Path, required=True); p.add_argument('--bridge', default=DEFAULT_BRIDGE); p.add_argument('--motor', default=DEFAULT_MOTOR)
    p.add_argument('--seed', type=int, default=7700002); p.add_argument('--per-family', type=int, default=24)
    args = p.parse_args()
    args.out.mkdir(parents=True, exist_ok=False); started = time.monotonic()
    with Bridge(args.bridge) as bridge:
        bridge.call('load_motor_policy', path=str(Path(args.motor).resolve()))
        rows = task_sets(bridge, args.seed, args.per_family)
        atomic_json(args.out / 'tasks.json', rows)
        results = run(bridge, rows, log=lambda line: print(line, flush=True))
    summary = {'law': LAW, 'seed': args.seed, 'motor': str(args.motor), 'summary': summarize(results), 'seconds': time.monotonic() - started}
    atomic_json(args.out / 'results.json', results); atomic_json(args.out / 'summary.json', summary)
    print(json.dumps(summary['summary'], indent=1))


if __name__ == '__main__':
    main()
