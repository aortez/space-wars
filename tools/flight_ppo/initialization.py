"""Explicit weights-only initialization, distinct from checkpoint continuation."""
import hashlib
from pathlib import Path
import torch
from .execution import load_export
from .model import SCHEMA_SCALING, GOAL_SCALING, ARCHITECTURE, GOAL_RESIDUAL_ARCHITECTURE


def initialize_policy(model, config, path):
    expected = config.get('warm_start_sha256')
    if expected is None:
        if path is not None:raise ValueError('warm start requires a frozen policy hash')
        return
    if path is None:raise ValueError('frozen warm-start policy required')
    if model.input_scaling not in (SCHEMA_SCALING, GOAL_SCALING):
        raise ValueError('warm start requires schema or function-preserving goal parameterization')
    if hashlib.sha256(Path(path).read_bytes()).hexdigest() != expected:
        raise ValueError('warm-start policy hash mismatch')
    with torch.random.fork_rng(devices=[]):
        source = load_export(path, model.schema)
        state = source.state_dict()
        if source.architecture != model.architecture:
            if source.architecture != ARCHITECTURE or model.architecture != GOAL_RESIDUAL_ARCHITECTURE:
                raise ValueError('unsupported warm-start architecture conversion')
            # Only an untouched zero-output branch may extend a legacy policy.
            if torch.count_nonzero(model.goal_output.weight) or torch.count_nonzero(model.goal_output.bias):
                raise ValueError('legacy warm start requires a zero-output goal branch')
            state = {**model.state_dict(), **state}
        model.load_state_dict(state, strict=True)
        if model.input_scaling == GOAL_SCALING:
            # Both consumers receive the scaled goal; preserve the exported function.
            with torch.no_grad():
                model.query.weight[:, :2].div_(32.)
                model.fusion1.weight[:, :2].div_(32.)
