"""Independent source-observation audit of the fixed brake-or-native selector."""
import math


def select(observation, diagnostic, actions, screen):
    f = observation['local']['combat']['recovery']['flight']
    p, flight = f['pilot'], f['flight']
    controls = actions[0]['Scenario']['payload']
    result = dict(action='observe', reason=None, warnings=[])

    def refuse(reason):
        result['reason'] = reason
        return result

    if diagnostic['unavailable_shells'] or diagnostic['shells_in_range'] > diagnostic['capacity']:
        return refuse('incomplete projectile sample')
    limits = flight['limits']; ship = p['ship']; frame = p['planet']['motion']
    values = [flight['sweep'], p['landing']['assist_strength'], ship['angle'], frame['spin'],
              *limits.values(), *ship['position'].values(), *ship['velocity'].values(),
              *frame['position'].values(), *frame['velocity'].values(), diagnostic['observer_radius']]
    values += [v for q in diagnostic['projectiles']
               for v in [q['collision_radius'], *q['relative_position'].values(), *q['relative_velocity'].values()]]
    if not all(math.isfinite(v) for v in values) or any(limits[k] <= 0 for k in
            ['thrust_acceleration', 'brake_acceleration', 'brake_gain']):
        return refuse('unsupported motor observation')
    if flight['sweep'] != 0 or flight['wings_closed'] or actions[1]['Scenario']['payload'][1]:
        return refuse('wing transition or closed command')
    if p['landing']['assist_strength'] != 0:
        return refuse('landing assist active')
    if controls[6]:
        return refuse('native braking already active')
    warnings = [(q, screen(q['relative_position'], q['relative_velocity'],
                           diagnostic['observer_radius'] + q['collision_radius'])['entry_seconds'])
                for q in diagnostic['projectiles']]
    warnings = [(q, t) for q, t in warnings if t is not None]
    assert warnings, 'selection requires the first warning'
    if min(t for _, t in warnings) < .5:
        return refuse('warning shorter than pulse')
    offset = [ship['position'][k] - frame['position'][k] for k in 'xy']
    frame_velocity = [frame['velocity']['x'] - frame['spin'] * offset[1],
                      frame['velocity']['y'] + frame['spin'] * offset[0]]
    relative = [ship['velocity'][k] - frame_velocity[i] for i, k in enumerate('xy')]
    speed = math.hypot(*relative)
    brake = [-x / speed * min(speed * limits['brake_gain'], limits['brake_acceleration'])
             for x in relative] if speed else [0., 0.]
    forward = [-math.sin(ship['angle']), math.cos(ship['angle'])]
    governor = max(0., min(1., (limits['cruise_speed'] - sum(x*y for x,y in zip(relative,forward))) / 10.))
    native = [x * limits['thrust_acceleration'] * governor * controls[4] for x in forward]
    for q, t in warnings:
        position = [q['relative_position'][k] for k in 'xy']; distance = math.hypot(*position)
        toward = sum(x*y for x,y in zip(brake,position)) / distance
        change = sum((b-n)*r for b,n,r in zip(brake,native,position)) / distance
        result['warnings'].append(dict(id=q['id'], entry_seconds=t, brake_toward=toward, change_toward=change))
    if not all(q['brake_toward'] < 0 and q['change_toward'] < 0 for q in result['warnings']):
        return refuse('braking does not move away from every warning')
    result.update(action='brake', reason='pulse fits and braking initially moves away')
    return result
