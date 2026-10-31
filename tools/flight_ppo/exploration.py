"""Versioned continuous Gaussian exploration with optional AR(1) residuals.

Every sampled latent is executed: tanh steering and sigmoid throttle/brake.
"""
import math
import torch
from torch.distributions import Normal

KIND = "continuous-gaussian-ar1-v2"
SCALED_KIND = "continuous-gaussian-scaled-ar1-v3"


def validate(spec):
    if spec.get("kind") == KIND:
        if set(spec) != {"kind", "rho"} or not 0 <= spec["rho"] < 1:
            raise ValueError("invalid exploration contract")
    elif spec.get("kind") == SCALED_KIND:
        if set(spec) != {"kind", "rho", "std_multiplier"} or not 0 <= spec["rho"] < 1:
            raise ValueError("invalid scaled exploration contract")
        validate_multiplier(spec["std_multiplier"])
    else:
        raise ValueError("invalid exploration contract")


def validate_multiplier(value):
    if type(value) not in (int, float) or not math.isfinite(value) or not .01 <= value <= 1.:
        raise ValueError("exploration standard-deviation multiplier must be finite and in [.01, 1]")


def amplitude(spec):
    validate(spec)
    return spec.get("std_multiplier", 1.)


def means(actor):
    return actor


def distribution(model, actor, previous_actor, previous_latent, valid, rho, std_multiplier=1.):
    validate_multiplier(std_multiplier)
    correlation = valid[:, None] * rho
    center = means(actor) + correlation * (previous_latent - means(previous_actor))
    scale = model.log_std.clamp(-5, 2).exp()
    if std_multiplier != 1.:
        scale = scale * std_multiplier
    return Normal(center, scale * (1-correlation.square()).sqrt())


def log_prob(model, dist, latent):
    return (dist.log_prob(latent).sum(-1) - model.log_jacobian(latent[:, 0])
            - model.actuator_log_jacobian(latent[:, 1:]).sum(-1))


def entropy(model, dist):
    sample = dist.rsample()
    return (dist.entropy().sum(-1) + model.log_jacobian(sample[:, 0])
            + model.actuator_log_jacobian(sample[:, 1:]).sum(-1))


def initial_state(batch, spec):
    validate(spec)
    return {"spec": dict(spec), "previous_observations": torch.zeros(batch, 4009),
            "previous_latents": torch.zeros(batch, 3), "previous_valid": torch.zeros(batch)}


def advance(state, observations, latents, ended):
    valid = torch.tensor([not value for value in ended], dtype=torch.float32)
    state["previous_observations"] = observations.clone() * valid[:, None]
    state["previous_latents"] = latents.clone() * valid[:, None]
    state["previous_valid"] = valid
