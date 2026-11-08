"""Lift a v2 flight stack to the v3 contracts with a free turn sense.

The motor export gains four inputs of zero weight for the command's advisory
vector and the navigation export a fourth head of zero weight, so the lifted
pair issues the v2 commands and actions exactly and must fly the v2 flights
identically: the gate on the v3 plumbing before any network is fitted.
Usage: python -m tools.flight_motor.lift_v3 MOTOR_IN NAVIGATION_IN MOTOR_OUT NAVIGATION_OUT [--bridge MOTOR_BRIDGE].
"""
import argparse
import json
from pathlib import Path

from tools.flight_ppo.bridge import Bridge
from .model import SENSE_ARCHITECTURE, WIDE_ARCHITECTURE
from .teacher_v2 import DEFAULT_BRIDGE, INPUTS

NAVIGATION_FORMAT_V2 = 5
NAVIGATION_FORMAT_V3 = 6


def lift_motor(record, schema_v3):
    """A wide v2 motor export under the v3 contract, the four advisory inputs weightless."""
    if record['format_version'] != 4 or record['architecture'] != WIDE_ARCHITECTURE:
        raise ValueError('only a wide v2 motor export lifts')
    weights = {name: {'shape': list(t['shape']), 'values': list(t['values'])} for name, t in record['weights'].items()}
    for name in ('actor1.weight', 'critic1.weight'):
        tensor = weights[name]
        rows, columns = tensor['shape']
        if columns != INPUTS[2]:
            raise ValueError('wrong motor input width')
        values = tensor['values']
        tensor['values'] = [v for row in range(rows) for v in values[row * columns:(row + 1) * columns] + [0.] * (INPUTS[3] - INPUTS[2])]
        tensor['shape'] = [rows, INPUTS[3]]
    return {'format_version': 5, 'architecture': SENSE_ARCHITECTURE, 'observation_schema': schema_v3, 'weights': weights}


def lift_navigation(record):
    """A v2 navigation export under the v3 contract, its fourth head weightless."""
    if record['format_version'] != NAVIGATION_FORMAT_V2:
        raise ValueError('only a v2 navigation export lifts')
    weights = {name: {'shape': list(t['shape']), 'values': list(t['values'])} for name, t in record['weights'].items()}
    actor = weights['actor.weight']
    if actor['shape'] != [3, 64]:
        raise ValueError('wrong navigation head count')
    actor['shape'] = [4, 64]; actor['values'] += [0.] * 64
    for name in ('actor.bias', 'log_std'):
        weights[name]['shape'] = [4]; weights[name]['values'].append(0.)
    return dict(record, format_version=NAVIGATION_FORMAT_V3, weights=weights)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('motor_in', type=Path); p.add_argument('navigation_in', type=Path)
    p.add_argument('motor_out', type=Path); p.add_argument('navigation_out', type=Path)
    p.add_argument('--bridge', default=DEFAULT_BRIDGE, help='the motor bridge, for the v3 schema')
    args = p.parse_args()
    with Bridge(args.bridge) as bridge:
        schema_v3 = bridge.contract['schema_v3']
    args.motor_out.write_text(json.dumps(lift_motor(json.loads(args.motor_in.read_text()), schema_v3)) + '\n')
    args.navigation_out.write_text(json.dumps(lift_navigation(json.loads(args.navigation_in.read_text()))) + '\n')


if __name__ == '__main__':
    main()
