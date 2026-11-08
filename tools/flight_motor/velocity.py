"""LF-055: frozen braking/tracking matrix and explicit weights-only stage transition."""
import copy
import math
import random
import torch
from tools.flight_ppo.checkpoint import sha
from . import task
from .model import ARCHITECTURE

PARENT_SHA = {
    17: 'd508b5b4104623f28ed9c2d251734e3e0edd87b3cc7faf064b5edbb87e69d7be',
    29: '4ca09c8bc0e05798a7a126718cab6fb4a634c9c7bc2c62a3b29c0816c427bad0',
    43: '763a38e9b0aba2d38113c2e6bde29692fc9b4ae443ec2cc1e99f15d5444ce921',
}
MODES = {'rest': (0, 0), 'slow': (.5, 0), 'fast': (1.5, 0), 'reverse': (-1, 0),
         'left': (0, 1), 'right': (0, -1), 'forward_left': (1, 1), 'forward_right': (1, -1)}


def candidates(seed, development=False):
    rows = []
    turn = 60 if development else 45
    def add(family, speed, forward, lateral, mode, level, sign):
        index = len(rows); rng = random.Random(seed * 1000 + index)
        bearing = rng.uniform(-math.pi, math.pi)
        # Point the desired velocity into clear space, within 60 degrees of radial up.
        # Absolute world bearing still covers the full circle. Spin starts at zero.
        offset = rng.uniform(-math.pi / 3, math.pi / 3)
        heading = bearing + offset; delta = math.radians(sign * turn)
        velocity = {'x': -math.sin(heading)*forward - math.cos(heading)*lateral,
                    'y': math.cos(heading)*forward - math.sin(heading)*lateral}
        rows.append({'index': index, 'family': family, 'direction': 'left' if sign > 0 else 'right',
                     'mode': mode, 'level': level,
                     'task': {'start': {'seed': seed * 1000 + index, 'bearing': bearing, 'altitude': 150.,
                                       'heading_offset': offset - delta, 'relative_velocity': velocity,
                                       'spin': 0., 'gravity': False, 'planet_spin': 0.},
                              'heading_delta': delta, 'speed': speed, 'deadline_ticks': 360}})
    for speed in ([8, 18, 30] if development else [6, 14, 26]):
        for mode, (forward, lateral) in {'forward': (1, 0), 'reverse': (-1, 0), 'left': (0, 1), 'right': (0, -1)}.items():
            for sign in [1, -1]: add('brake', 0., speed*forward, speed*lateral, mode, speed, sign)
    for speed in ([6, 12, 18] if development else [4, 10, 16]):
        for mode, (forward, lateral) in MODES.items():
            for sign in [1, -1]: add('track', speed, speed*forward, speed*lateral, mode, speed, sign)
    # Rehearse the entire original training matrix; evaluate the original unseen one.
    for original in task.candidates(5310002 if development else 5310001, development):
        row = copy.deepcopy(original); row['parent_index'] = row['index']; row['index'] = len(rows); rows.append(row)
    if development:
        for row in rows: row['task']['retention_horizon_ticks'] = 900
    return rows


def reward(item):
    e = item['errors']; outcome = item['progress']['outcome']
    tracking = -(1 - math.cos(e['heading']) + .1*(e['spin']/1.8)**2 + (e['velocity']/5)**2) / 60
    terminal = 1. if outcome == 'success' else -5. if outcome not in [None, 'deadline'] else 0.
    return {'tracking': tracking, 'terminal': terminal, 'total': tracking + terminal}


def config(seed):
    return {**task.config(seed), 'kind': 'motor-velocity-v1', 'updates': 128,
            'train_task_seed': 5510001, 'development_task_seed': 5510002,
            'initialization': {'kind': 'weights-only-fresh-optimizer-rng-collector', 'parent_sha256': PARENT_SHA[seed]}}


def initialize(model, path, config):
    if sha(path) != config['initialization']['parent_sha256']:
        raise ValueError('wrong motor stage parent hash')
    state = torch.load(path, weights_only=True); contract = state['contract']
    if (contract['architecture'] != ARCHITECTURE or contract['schema'] != model.schema
            or contract['config']['kind'] != 'motor-turn-settle-v1'
            or contract['config']['learner_seed'] != config['learner_seed'] or state['update'] != 64):
        raise ValueError('incompatible motor stage parent')
    model.load_state_dict(state['model'], strict=True)
