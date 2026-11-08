"""LF-065: change only GAE lambda on the protected residual training recipe."""
from . import sustained

ARMS = ['lambda095', 'lambda1']
SEEDS = sustained.SEEDS
RUN_KIND = 'motor-protected-gae-comparison-v1-gzip'


def config(seed, arm, preflight=False):
    if seed not in SEEDS or arm not in ARMS:
        raise ValueError('undeclared GAE comparison')
    c = sustained.config(seed, 'sustain', preflight)
    c.update(kind='motor-gae-comparison-v1', arm=arm)
    c['lambda'] = .95 if arm == 'lambda095' else 1.
    return c


training_candidates = sustained.training_candidates
candidates = sustained.candidates
initialize = sustained.initialize
update = sustained.update
assert_parent_actor = sustained.assert_parent_actor


def architecture(c):
    return sustained.ARCHITECTURE


def make_model(schema, c):
    return sustained.ResidualModel(schema)
