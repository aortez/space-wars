"""Shared encoder with v2 continuous controls; independent Rust export inference."""
import json
import math
from pathlib import Path

import torch
from torch import nn
from torch.nn import functional as F
from torch.distributions import Normal

ACTION_CONTRACT = "continuous-flight-v2"

ARCHITECTURE = "polar-conv8x8-coordinates-set-attention32-mlp64x64-v1"
GOAL_RESIDUAL_ARCHITECTURE = "polar-conv8x8-coordinates-set-attention32-mlp64x64-goal16-v2"


def architecture(spec=None):
    spec = ARCHITECTURE if spec is None else spec
    if spec not in (ARCHITECTURE, GOAL_RESIDUAL_ARCHITECTURE):
        raise ValueError("unsupported flight architecture")
    return spec


def export_version(spec):
    return 3 if architecture(spec) == GOAL_RESIDUAL_ARCHITECTURE else 2


def flatten(inputs):
    if inputs is None:
        return [0.0] * 4009
    return (inputs["self_goal"] + [v for row in inputs["cells"] for v in row]
            + [v for row in inputs["rays"] for v in row] + inputs["nearest"]
            + [v for row in inputs["objects"] for v in row])


def tensor(items):
    return torch.tensor([flatten(item["inputs"]) for item in items], dtype=torch.float32)


SCHEMA_SCALING = "schema-v1"
TASK_SCALING = "task-local-v1"
GOAL_SCALING = "goal-only32-v1"
# Milder goal and relative-velocity factors for match transfers, whose goals
# lie hundreds of units away and whose speeds run to 60.
MATCH_SCALING = "match-local-v1"
VALUE_SHARED = "shared-encoder-v1"
VALUE_HEAD_ONLY = "head-only-v1"


def value_gradient_mode(spec=None):
    spec = VALUE_SHARED if spec is None else spec
    if spec not in (VALUE_SHARED, VALUE_HEAD_ONLY):
        raise ValueError("unsupported value-gradient mode")
    return spec


def input_scaling(spec=None):
    spec = SCHEMA_SCALING if spec is None else spec
    if spec not in (SCHEMA_SCALING, TASK_SCALING, GOAL_SCALING, MATCH_SCALING):
        raise ValueError("unsupported training input scaling")
    return spec


class FlightModel(nn.Module):
    def __init__(self, schema, scaling=None, architecture_spec=None):
        super().__init__()
        if schema.get("version") != 2 or schema.get("action_contract") != ACTION_CONTRACT:
            raise ValueError("continuous policy requires the v2 observation/action contract")
        self.schema = schema
        self.architecture = architecture(architecture_spec)
        self.input_scaling = input_scaling(scaling)
        # Training parameterization only: exports fold these factors into both
        # linear consumers of the self/goal vector, preserving the native schema.
        factors = ([32., 32.] + [1.] * 33 if self.input_scaling == GOAL_SCALING
                   else [8., 8., 6., 6.] + [1.] * 31 if self.input_scaling == MATCH_SCALING
                   else [25., 25., 17.5, 17.5] + [1.] * 31)
        self.register_buffer("task_input_factors", torch.tensor(factors), persistent=False)
        edges = torch.tensor(schema["radial_edges"], dtype=torch.float32) / 300.0
        self.register_buffer("coordinates", torch.stack((edges[:-1], edges[1:]))[:, :, None].expand(2, 23, 64), persistent=False)
        self.conv1 = nn.Conv2d(4, 8, 3, stride=2)
        self.conv2 = nn.Conv2d(8, 8, 3, stride=2)
        self.map_fc = nn.Linear(192, 32)
        self.rays_fc = nn.Linear(512, 32)
        self.object_fc1 = nn.Linear(15, 32)
        self.object_fc2 = nn.Linear(32, 32)
        self.query = nn.Linear(35, 32)
        self.fusion1 = nn.Linear(137, 64)
        self.fusion2 = nn.Linear(64, 64)
        self.actor = nn.Linear(64, 3)
        self.critic = nn.Linear(64, 1)
        self.log_std = nn.Parameter(torch.tensor([-0.5, -0.5, -0.5]))
        for layer in self.modules():
            if isinstance(layer, (nn.Conv2d, nn.Linear)):
                nn.init.orthogonal_(layer.weight, math.sqrt(2))
                nn.init.zeros_(layer.bias)
        nn.init.orthogonal_(self.actor.weight, 0.01)
        nn.init.orthogonal_(self.critic.weight, 1.0)
        if self.architecture == GOAL_RESIDUAL_ARCHITECTURE:
            # Keep the base initialization and subsequent sampler RNG unchanged.
            with torch.random.fork_rng(devices=[]):
                self.goal_hidden = nn.Linear(2, 16)
                self.goal_output = nn.Linear(16, 3)
                nn.init.orthogonal_(self.goal_hidden.weight, math.sqrt(2))
                nn.init.zeros_(self.goal_hidden.bias)
                nn.init.zeros_(self.goal_output.weight)
                nn.init.zeros_(self.goal_output.bias)

    @staticmethod
    def polar_conv(layer, x):
        x = F.pad(x, (1, 1, 0, 0), mode="circular")
        return torch.tanh(layer(F.pad(x, (0, 0, 1, 1))))

    def forward(self, x, *, value_gradient=VALUE_SHARED):
        value_gradient = value_gradient_mode(value_gradient)
        if x.ndim != 2 or x.shape[1] != 4009:
            raise ValueError("wrong policy tensor shape")
        own = x[:, :35]
        if self.input_scaling != SCHEMA_SCALING:
            own = own * self.task_input_factors
        cells = x[:, 35:2979].reshape(-1, 23, 64, 2).permute(0, 3, 1, 2)
        cells = torch.cat((cells, self.coordinates.expand(len(x), -1, -1, -1)), dim=1)
        cells = self.polar_conv(self.conv2, self.polar_conv(self.conv1, cells))
        cells = torch.tanh(self.map_fc(F.avg_pool2d(cells, 2).flatten(1)))
        rays = torch.tanh(self.rays_fc(x[:, 2979:3491]))
        objects = x[:, 3497:].reshape(-1, 32, 16)
        mask = objects[:, :, 0]
        encoded = torch.tanh(self.object_fc2(torch.tanh(self.object_fc1(objects[:, :, 1:] * mask[:, :, None]))))
        scores = (encoded * self.query(own)[:, None, :]).sum(-1) / math.sqrt(32)
        weights = scores.masked_fill(mask == 0, -1e9).softmax(-1) * mask
        weights = weights / weights.sum(-1, keepdim=True).clamp_min(1e-8)
        pooled = (encoded * weights[:, :, None]).sum(1)
        joined = torch.cat((own, cells, rays, x[:, 3491:3497], pooled), dim=1)
        hidden = torch.tanh(self.fusion2(torch.tanh(self.fusion1(joined))))
        value_features = hidden.detach() if value_gradient == VALUE_HEAD_ONLY else hidden
        actor = self.actor(hidden)
        if self.architecture == GOAL_RESIDUAL_ARCHITECTURE:
            # Schema goal /300 -> task-local goal /12, independent of main scaling.
            goal = torch.tanh(self.goal_hidden(x[:, :2] * 25.))
            actor = actor + self.goal_output(goal)
        return actor, self.critic(value_features).squeeze(-1)

    def distributions(self, actor):
        std = self.log_std.clamp(-5, 2).exp()
        return Normal(actor[:, 0], std[0]), Normal(actor[:, 1:], std[1:])

    @staticmethod
    def log_jacobian(latent):
        return 2 * (math.log(2) - latent - F.softplus(-2 * latent))

    @staticmethod
    def actuator_log_jacobian(latent):
        # log(sigmoid(z) * (1-sigmoid(z))), stable even in saturated tails.
        return -F.softplus(-latent) - F.softplus(latent)

    def log_prob(self, actor, latent, actuator_latents):
        steering, actuators = self.distributions(actor)
        return (steering.log_prob(latent) - self.log_jacobian(latent)
                + (actuators.log_prob(actuator_latents)
                   - self.actuator_log_jacobian(actuator_latents)).sum(-1))

    def sample(self, x):
        actor, value = self(x)
        steering, actuators = self.distributions(actor)
        latent, actuator_latents = steering.sample(), actuators.sample()
        return latent, actuator_latents, self.log_prob(actor, latent, actuator_latents), value

    def entropy(self, actor):
        steering, actuators = self.distributions(actor)
        return (steering.entropy() + self.log_jacobian(steering.rsample())
                + (actuators.entropy() + self.actuator_log_jacobian(actuators.rsample())).sum(-1))

    def inference_state_dict(self):
        state = {name: p.detach().clone() for name, p in self.state_dict().items()}
        if self.input_scaling != SCHEMA_SCALING:
            state["query.weight"] *= self.task_input_factors[None, :]
            state["fusion1.weight"][:, :35] *= self.task_input_factors[None, :]
        return state

    def export(self, path):
        record = {"format_version": export_version(self.architecture), "architecture": self.architecture,
                  "observation_schema": self.schema,
                  "weights": {name: {"shape": list(p.shape), "values": p.detach().flatten().tolist()}
                              for name, p in self.inference_state_dict().items()}}
        Path(path).write_text(json.dumps(record, allow_nan=False) + "\n")


def actions(latent, actuator_latents):
    return [{"steering": float(u), "thrust": float(a[0]), "brake": float(a[1])}
            for u, a in zip(latent.tanh().tolist(), actuator_latents.sigmoid().tolist())]
