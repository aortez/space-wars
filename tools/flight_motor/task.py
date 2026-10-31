"""Fixed procedural matrices and dense command-tracking cost; native outcomes."""
import math
import random

SEEDS = [17, 29, 43]


def candidates(seed, development=False):
    angles = [30, 60, 120, 165] if development else [15, 45, 90, 150]
    spins = [-.45, 0., .45] if development else [-.6, 0., .6]
    rows = []
    for index in range(32):
        rng = random.Random(seed * 1000 + index)
        turn = index < 24
        magnitude = angles[(index // 2) % 4] if turn else 0
        sign = -1 if index % 2 else 1
        spin = spins[index // 8] if turn else 0.
        rows.append({'index': index, 'family': 'turn' if turn else 'hold', 'direction': 'left' if sign > 0 else 'right',
            'task': {'start': {'seed': seed * 1000 + index, 'bearing': rng.uniform(-math.pi, math.pi),
                              'altitude': 150., 'heading_offset': rng.uniform(-math.pi, math.pi),
                              'relative_velocity': {'x': 0., 'y': 0.}, 'spin': spin, 'gravity': False, 'planet_spin': 0.},
                     'heading_delta': math.radians(sign * magnitude), 'speed': 0., 'deadline_ticks': 360}})
    return rows


def reward(item):
    e = item['errors']; outcome = item['progress']['outcome']
    # Smooth dense error cost; no goal-distance potential or inherited arrival reward.
    tracking = -(1 - math.cos(e['heading']) + .1 * (e['spin'] / 1.8) ** 2 + .2 * (e['velocity'] / 5) ** 2) / 60
    terminal = 1. if outcome == 'success' else -5. if outcome not in [None, 'deadline'] else 0.
    return {'tracking': tracking, 'terminal': terminal, 'total': tracking + terminal}


def config(seed):
    return {'kind': 'motor-turn-settle-v1', 'learner_seed': seed, 'updates': 64, 'rollout_ticks': 256,
            'epochs': 4, 'minibatch': 256, 'learning_rate': .0003, 'gamma': .99, 'lambda': .95,
            'clip': .2, 'value_weight': .5, 'entropy_weight': .001, 'grad_clip': .5, 'target_kl': .03,
            'train_task_seed': 5310001, 'development_task_seed': 5310002, 'wall_seconds': 900}
