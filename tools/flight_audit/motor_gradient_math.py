"""LF-068 gradient accounting, including Adam's shared actual-path denominator."""
import torch
from tools.flight_motor import aligned

COMMANDS = [4, 10, 16]
COMPONENTS = ['command_4', 'command_10', 'command_16', 'stopping', 'entropy', 'value']
ALLOCATIONS = COMPONENTS + ['carried_momentum', 'rounding_residual']


def parameters(model):
    named = [(n, p) for n, p in model.named_parameters() if p.requires_grad]
    return [n for n, _ in named], [p for _, p in named]


def flatten(values):
    return torch.cat([v.detach().reshape(-1) for v in values])


def gradient(loss, params):
    values = torch.autograd.grad(loss, params, retain_graph=True, allow_unused=True)
    return flatten([torch.zeros_like(p) if g is None else g for p, g in zip(params, values)])


def masks_by_command(x):
    return [(x[:, 2] * 20 - command).abs() < 1e-5 for command in COMMANDS]


def command_losses(surrogate, x):
    # Preserve the actual pooled movement denominator, including unequal groups.
    count = (x[:, 2] > 0).sum().clamp_min(1)
    return [-.5 * surrogate[mask].sum() / count for mask in masks_by_command(x)]


def normalized_variants(raw, x):
    move = x[:, 2] > 0
    std = raw[move].std(unbiased=False).clamp_min(1e-8)
    pooled = aligned.advantages(raw, move)
    own = pooled.clone()
    for mask in masks_by_command(x):
        if mask.any():
            own[mask] = (raw[mask] - raw[mask].mean()) / std
    return {'raw': raw, 'raw_common_scale': raw / std, 'pooled': pooled,
            'own_center_common_scale': own}


def cosine(a, b):
    a, b = a.double(), b.double()
    denominator = a.norm() * b.norm()
    if min(float(a.norm()), float(b.norm())) <= 1e-12:
        return None
    return float((a @ b / denominator).clamp(-1, 1))


def parameter_masks(names, params):
    return {kind: torch.cat([torch.full((p.numel(),), n.startswith('residual') and
             (kind == 'actor' or n != 'residual_log_std'), dtype=torch.bool)
             for n, p in zip(names, params)]) for kind in ['actor', 'mean_action']}


class AdamAccounting:
    """Additive first-moment allocation conditional on actual clipping/variance.

    This is not the counterfactual optimizer path after removing a component.
    Float64 bookkeeping leaves float32 optimizer/parameter rounding explicit.
    """
    def __init__(self, optimizer, params, component_count=None):
        assert len(optimizer.param_groups) == 1
        group = optimizer.param_groups[0]
        assert not any(group[k] for k in ['weight_decay', 'amsgrad', 'maximize',
                                         'capturable', 'differentiable'])
        self.optimizer, self.params, self.group = optimizer, params, group
        count = len(COMPONENTS) if component_count is None else component_count
        self.moments = torch.zeros(count + 1, sum(p.numel() for p in params), dtype=torch.float64)
        self.moments[-1] = flatten([optimizer.state[p].get('exp_avg', torch.zeros_like(p)) for p in params]).double()

    def allocate(self, gradients, clip_factor, actual_delta):
        b1, b2 = self.group['betas']
        self.moments *= b1
        self.moments[:-1] += (1 - b1) * clip_factor * gradients.double()
        actual_moment = flatten([self.optimizer.state[p]['exp_avg'] for p in self.params]).double()
        torch.testing.assert_close(self.moments.sum(0), actual_moment, atol=2e-6, rtol=1e-4)
        scales = []
        for p in self.params:
            state = self.optimizer.state[p]
            step = float(state['step'])
            denominator = (state['exp_avg_sq'].sqrt() / (1 - b2 ** step) ** .5).add(self.group['eps'])
            scales.append((-self.group['lr'] / (1 - b1 ** step)) / denominator.double())
        allocated = self.moments * flatten(scales)
        residual = actual_delta.double() - allocated.sum(0)
        assert float(residual.abs().max()) <= 2e-6
        return torch.cat((allocated, residual[None])), float((self.moments.sum(0) - actual_moment).abs().max())
