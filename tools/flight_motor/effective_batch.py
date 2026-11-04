"""LF-073: matched physics, larger averaged PPO batches, unchanged return horizon."""
import torch
from . import single_speed

ARMS = ['current', 'large']
SEEDS = single_speed.SEEDS
RUN_KIND = 'motor-effective-batch-v1-gzip'


def config(seed, arm, preflight=False):
    if arm not in ARMS:
        raise ValueError('undeclared effective-batch arm')
    c = single_speed.config(seed, 'speed16', preflight)
    segments = 4 if arm == 'large' else 1
    c.update(arm=arm, collection_segments=segments, return_horizon_ticks=256,
             rollout_ticks=256*segments, minibatch=256*segments,
             updates=(4 if preflight else 84)//segments)
    return c


def collect(collector, model, c, deadline):
    # Each segment bootstraps separately. Concatenation must not extend GAE.
    assert c['rollout_ticks'] == c['return_horizon_ticks']*c['collection_segments']
    segment_config = {**c, 'rollout_ticks': c['return_horizon_ticks']}
    rows = [collector.collect(model, segment_config, deadline)
            for _ in range(c['collection_segments'])]
    return {k: torch.cat([r[k] for r in rows], dim=0) for k in rows[0]}


def target_updates(c):
    return list(range(1, c['updates']+1)) if c['preflight'] else (
        [1, 4, 11, 16, 21] if c['arm'] == 'large' else [1, 16, 44, 64, 84])


training_candidates = single_speed.training_candidates
collector_pools = single_speed.collector_pools
candidates = single_speed.candidates
initialize = single_speed.initialize
update = single_speed.update
assert_parent_actor = single_speed.assert_parent_actor
architecture = single_speed.architecture
make_model = single_speed.make_model
