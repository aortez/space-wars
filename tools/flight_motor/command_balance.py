"""LF-067: six-slot cyclic versus command-stratified protected lambda-1 PPO."""
from . import gae_comparison, sustained

ARMS = ['cyclic', 'balanced']
SEEDS = sustained.SEEDS
RUN_KIND = 'motor-command-balance-v1-gzip'
TARGET_UPDATES = [1, 16, 43, 86]


def config(seed, arm, preflight=False):
    if seed not in SEEDS or arm not in ARMS:
        raise ValueError('undeclared command-balance comparison')
    c = gae_comparison.config(seed, 'lambda1', preflight)
    c.update(kind='motor-command-balance-v1', arm=arm, collector_slots=6,
             collector_schedule=arm, updates=4 if preflight else 86)
    return c


def training_candidates(c):
    # Preserve the task definitions and indices; only their slot assignment differs.
    rows = sustained.training_candidates({**c, 'preflight': False})
    return [rows[i] for i in [0, 1, 16, 17, 32, 33]] if c['preflight'] else rows


def collector_pools(c):
    rows = training_candidates(c)
    if c['preflight']:
        return [[row] for row in rows]
    if c['arm'] == 'cyclic':
        return [rows[i::6] for i in range(6)]
    movement = [[r for r in rows if r['task']['speed'] == speed] for speed in [4, 10, 16]]
    stopping = [rows[i::6] for i in [1, 3, 5]]
    return [pool for pair in zip(movement, stopping) for pool in pair]


candidates = sustained.candidates
initialize = sustained.initialize
update = sustained.update
assert_parent_actor = sustained.assert_parent_actor
architecture = gae_comparison.architecture
make_model = gae_comparison.make_model
