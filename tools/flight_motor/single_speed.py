"""LF-069: fixed speed-16 learning with the original stopping policy protected."""
from . import command_balance, sustained

ARMS = ['speed16']
SEEDS = sustained.SEEDS
RUN_KIND = 'motor-single-speed-v1-gzip'
TARGET_UPDATES = [1, 16, 43, 86]


def config(seed, arm, preflight=False):
    if seed not in SEEDS or arm not in ARMS:
        raise ValueError('undeclared single-speed diagnostic')
    c = command_balance.config(seed, 'balanced', preflight)
    c.update(kind='motor-single-speed-v1', arm=arm, collector_schedule='single-speed16')
    return c


def training_candidates(c):
    rows = sustained.training_candidates({**c, 'preflight': False})
    if c['preflight']:
        return [rows[i] for i in [32, 1, 40, 17, 44, 33]]
    return [r for r in rows if r['task']['speed'] in [0, 16]]


def collector_pools(c):
    rows = training_candidates(c)
    if c['preflight']:
        return [[r] for r in rows]
    moving = [r for r in rows if r['task']['speed'] == 16]
    # Keep every original stopping pool and its order; each movement case occurs once.
    previous = command_balance.collector_pools(command_balance.config(c['learner_seed'], 'balanced'))
    return [pool for i in range(3) for pool in [moving[i::3], previous[2*i+1]]]


def candidates(seed, development=True):
    if not development:
        raise ValueError('training uses its declared single-speed pools')
    rows = sustained.candidates(seed, True)
    for row in rows:
        if row['family'] != 'track':
            continue
        old_speed = row['task']['speed']
        row['start_variant'] = [6, 12, 18].index(old_speed)
        row['level'] = 16
        row['task']['speed'] = 16
        velocity = row['task']['start']['relative_velocity']
        row['task']['start']['relative_velocity'] = {k: v * (16 / old_speed) for k, v in velocity.items()}
    return rows


initialize = sustained.initialize
update = sustained.update
assert_parent_actor = sustained.assert_parent_actor
architecture = command_balance.architecture
make_model = command_balance.make_model
