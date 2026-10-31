"""PPO-Clip with continuous action likelihoods and terminal-aware GAE."""
import torch
from . import exploration
from .model import VALUE_HEAD_ONLY, value_gradient_mode

GLOBAL_CENTERING = "global-mean-v1"
FAMILY_CENTERING = "family-mean-v1"
RECOVERY_FAMILIES = ("settled", "position", "velocity", "spin")


def advantage_centering(spec=None):
    spec = GLOBAL_CENTERING if spec is None else spec
    if spec not in (GLOBAL_CENTERING, FAMILY_CENTERING):
        raise ValueError("unsupported advantage centering")
    return spec


def normalize_advantages(adv, family_ids=None, mode=None):
    mode = advantage_centering(mode)
    divisor = adv.std(unbiased=False).clamp_min(1e-8)
    if mode == GLOBAL_CENTERING:
        return (adv - adv.mean()) / divisor
    if (family_ids is None or family_ids.shape != adv.shape
            or family_ids.dtype != torch.int64
            or set(family_ids.tolist()) != set(range(4))):
        raise ValueError("family centering requires labels for all four recovery families")
    means = torch.stack([adv[family_ids == i].mean() for i in range(4)])
    return (adv - means[family_ids]) / divisor


def advantage_diagnostics(rollout, config):
    """Read-only evidence, saved separately from optimizer/checkpoint history."""
    raw = rollout['advantages'].flatten().detach()
    labels = rollout['family_ids'].flatten()
    mode = advantage_centering(config.get('advantage_centering'))
    normalized = normalize_advantages(raw, labels, mode)
    groups = {}
    for i, name in enumerate(RECOVERY_FAMILIES):
        selected = labels == i
        r, n = raw[selected], normalized[selected]
        groups[name] = {'rows': int(selected.sum()), 'raw_mean': float(r.mean()),
                        'normalized_mean': float(n.mean()),
                        'raw_positive_rows': int((r > 0).sum()),
                        'normalized_positive_rows': int((n > 0).sum())}
    return {'mode': mode, 'raw_mean': float(raw.mean()), 'raw_std': float(raw.std(unbiased=False)),
            'divisor': float(raw.std(unbiased=False).clamp_min(1e-8)),
            'raw_advantages': raw.tolist(), 'family_ids': labels.tolist(),
            'normalized_advantages': normalized.tolist(), 'families': groups}


def gae(rewards, dones, values, final_value, gamma, lam):
    result = torch.zeros_like(rewards)
    running = torch.zeros_like(final_value)
    for t in reversed(range(len(rewards))):
        next_value = final_value if t == len(rewards) - 1 else values[t + 1]
        live = 1.0 - dones[t]
        delta = rewards[t] + gamma * live * next_value - values[t]
        running = delta + gamma * lam * live * running
        result[t] = running
    return result, result + values


def update(model, optimizer, rollout, config, policy_terms=None, auxiliary=None):
    """One PPO update; auxiliary(model, actor, flat, indices, config) adds a supervised loss term when given."""
    value_gradient = value_gradient_mode(config.get("value_gradient"))
    std_multiplier = exploration.amplitude(config["exploration"]) if "exploration" in config else 1.
    flat = {k: v.flatten(0, 1) for k, v in rollout.items()}
    adv = flat["advantages"]
    flat["advantages"] = normalize_advantages(adv, flat.get("family_ids"), config.get("advantage_centering"))
    logs = []
    stop = False
    for epoch in range(config["epochs"]):
        for indices in torch.randperm(len(adv)).split(config["minibatch"]):
            if value_gradient == VALUE_HEAD_ONLY:
                actor, value = model(flat["observations"][indices], value_gradient=value_gradient)
            else:
                actor, value = model(flat["observations"][indices])
            if policy_terms is not None:
                logp, entropy = policy_terms(model, actor, flat, indices, config)
            elif "exploration" in config:
                previous_actor = model(flat["ar_previous_observations"][indices])[0]
                dist = exploration.distribution(model, actor, previous_actor,
                    flat["ar_previous_latents"][indices], flat["ar_previous_valid"][indices],
                    config["exploration"]["rho"], std_multiplier)
                logp = exploration.log_prob(model, dist, flat["ar_latents"][indices])
                entropy = exploration.entropy(model, dist).mean()
            else:
                logp = model.log_prob(actor, flat["latent"][indices], flat["actuator_latents"][indices])
                entropy = model.entropy(actor).mean()
            log_ratio = logp - flat["logp"][indices]
            ratio = log_ratio.exp()
            advantage = flat["advantages"][indices]
            policy_loss = -torch.minimum(ratio * advantage, ratio.clamp(1-config["clip"], 1+config["clip"]) * advantage).mean()
            value_loss = 0.5 * (value - flat["returns"][indices]).square().mean()
            loss = policy_loss + config["value_weight"] * value_loss - config["entropy_weight"] * entropy
            extra = auxiliary(model, actor, flat, indices, config) if auxiliary is not None else torch.zeros(())
            loss = loss + extra
            if not torch.isfinite(loss):
                raise RuntimeError("non-finite PPO loss")
            kl = ((ratio - 1) - log_ratio).mean().detach()
            if kl > config["target_kl"]:
                stop = True
                break
            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            norm = torch.nn.utils.clip_grad_norm_(model.parameters(), config["grad_clip"], error_if_nonfinite=True)
            optimizer.step()
            logs.append({"policy_loss": float(policy_loss.detach()), "value_loss": float(value_loss.detach()),
                         "entropy": float(entropy.detach()), "kl": float(kl), "gradient_norm": float(norm), "auxiliary": float(extra.detach()),
                         "clip_fraction": float(((ratio - 1).abs() > config["clip"]).float().mean().detach())})
        if stop:
            break
    if not logs:
        raise RuntimeError("no PPO optimizer step completed")
    return {"minibatches": len(logs), "kl_early_stop": stop,
            **{key: sum(row[key] for row in logs) / len(logs) for key in logs[0]}}
