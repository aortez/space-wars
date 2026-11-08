"""Navigation network: the C3 flight encoder with its three heads read as a direction pair and a speed.

The weights, observation schema and Rust inference are the flight policy's;
only the output mapping and the export version differ. The exported file is
format 4, which the flight policy loader refuses and the navigation loader
requires, so neither side can misread the other's heads.
"""
import json
import math
from pathlib import Path

import torch
from torch import nn
from torch.distributions import Normal
from tools.flight_ppo.model import ARCHITECTURE, FlightModel, TASK_SCALING, flatten

COMMAND_CONTRACT = 'navigation-command-v1'
FORMAT_VERSION = 4
MAX_SPEED = 20.
# The v2 contract: the velocity relative to the goal as a heading and a speed
# to 60, for the planet-frame motor, with the host adding the goal's velocity
# in the pilot's planet frame and speeds under the deadband read as stops;
# exported as format 5.
# The v3 contract: the v2 command with the advisory vector of the motor's v3
# contract, whose turn sense a fourth head sets, through tanh and a threshold
# of a half either way; exported as format 6.
CONTRACTS = {1: {'name': COMMAND_CONTRACT, 'format_version': FORMAT_VERSION, 'max_speed': MAX_SPEED, 'speed_deadband': 0.},
             2: {'name': 'navigation-command-v2', 'format_version': 5, 'max_speed': 60., 'speed_deadband': .5},
             3: {'name': 'navigation-command-v3', 'format_version': 6, 'max_speed': 60., 'speed_deadband': .5}}
SENSE_THRESHOLD = .5


class NavigationModel(FlightModel):
    """Training parameterization uses task-local input scaling; exports fold it into the weights."""

    def __init__(self, schema, scaling=TASK_SCALING, contract=1):
        super().__init__(schema, scaling, ARCHITECTURE)
        self.contract = contract
        self.max_speed = CONTRACTS[contract]['max_speed']
        self.speed_deadband = CONTRACTS[contract]['speed_deadband']
        if contract == 3:
            # The fourth head, the turn sense, beside the direction pair and the speed.
            self.actor = nn.Linear(64, 4)
            nn.init.orthogonal_(self.actor.weight, .01); nn.init.zeros_(self.actor.bias)
            self.log_std = nn.Parameter(torch.full((4,), -.5))

    def export(self, path):
        record = {'format_version': CONTRACTS[self.contract]['format_version'], 'architecture': self.architecture, 'observation_schema': self.schema,
                  'weights': {name: {'shape': list(p.shape), 'values': p.detach().flatten().tolist()}
                              for name, p in self.inference_state_dict().items()}}
        Path(path).write_text(json.dumps(record, allow_nan=False) + '\n')

    # Exploration: one Gaussian over the three latents, squashed afterwards as
    # the command mapping squashes them, so the likelihood is of the executed
    # command. The heading pair's deviation scales with the commanded speed,
    # floored at a tenth, so a stopping command keeps the nose still enough for
    # the arrival dwell while a cruising command explores its route. The scale
    # carries no gradient: it is an exploration schedule, not a lever.
    HEADING_NOISE_FLOOR = .1

    def distributions(self, actor):
        if self.contract == 3:
            # The fourth head is a threshold decision, not a squashed
            # actuator: a Gaussian over its latent is not a likelihood of the
            # sense the command carries. The v3 heads are fitted by imitation,
            # and only the speed is sampled (sample_speed).
            raise ValueError('the v3 contract samples its speed alone; the turn sense is not sampled')
        std = self.log_std.clamp(-5, 2).exp()
        scale = torch.sigmoid(actor[:, 2:3]).detach().clamp_min(self.HEADING_NOISE_FLOOR)
        return Normal(actor, torch.cat((std[:2] * scale, std[2:].expand(len(actor), 1)), dim=1))

    def log_prob(self, actor, latent):
        return (self.distributions(actor).log_prob(latent).sum(-1)
                - self.log_jacobian(latent[:, :2]).sum(-1) - self.actuator_log_jacobian(latent[:, 2]))

    def sample(self, x):
        actor, value = self(x)
        latent = self.distributions(actor).sample()
        return latent, self.log_prob(actor, latent), value

    def entropy(self, actor):
        distribution = self.distributions(actor); latent = distribution.rsample()
        return (distribution.entropy().sum(-1) + self.log_jacobian(latent[:, :2]).sum(-1)
                + self.actuator_log_jacobian(latent[:, 2]))

    # Speed-only exploration: the heading stays at its mean, as the motor stage
    # kept steering, so a stopping command never jitters the nose and the
    # likelihood keeps the speed deviation's moderate sensitivity everywhere.
    # A fourth head, the turn sense, stays at its mean too.
    def speed_distribution(self, actor):
        return Normal(actor[:, 2], self.log_std.clamp(-5, 2).exp()[2])

    def speed_log_prob(self, actor, latent):
        return self.speed_distribution(actor).log_prob(latent[:, 2]) - self.actuator_log_jacobian(latent[:, 2])

    def sample_speed(self, x):
        actor, value = self(x)
        speed = self.speed_distribution(actor).sample()
        latent = torch.cat((actor[:, :2], speed[:, None], actor[:, 3:]), dim=1)
        return latent, self.speed_log_prob(actor, latent), value

    def speed_entropy(self, actor):
        distribution = self.speed_distribution(actor)
        return distribution.entropy() + self.actuator_log_jacobian(distribution.rsample())


def commands(actor, max_speed=MAX_SPEED):
    """Direction pair through tanh and speed through a sigmoid scaled to the cap, as the Rust loader maps them, and a fourth head's sense through tanh."""
    mapped = (torch.tanh(actor[:, :2]), max_speed * torch.sigmoid(actor[:, 2:3]))
    if actor.shape[1] > 3:
        mapped += (torch.tanh(actor[:, 3:4]),)
    return torch.cat(mapped, dim=1)


def command_dicts(latents, minimum_norm=.125, max_speed=MAX_SPEED, speed_deadband=0.):
    """Bridge commands from latents; a pair under the floor keeps its direction at the floor, forward if it has none.

    The floor sits above the bridge's minimum of a tenth because the pair is
    rescaled in double precision and parsed natively in single precision. A
    speed under the contract's deadband is a stop, as the native loader reads it.
    """
    rows = []
    for values in commands(latents, max_speed).tolist():
        sin, cos, speed = values[:3]
        norm = math.hypot(sin, cos)
        if norm < minimum_norm:
            sin, cos = (0., minimum_norm) if norm == 0. else (sin * minimum_norm / norm, cos * minimum_norm / norm)
        speed = min(max_speed, max(0., speed))
        command = {'heading_sin': sin, 'heading_cos': cos, 'speed': 0. if speed < speed_deadband else speed}
        if len(values) > 3:
            # The sense through its threshold, as the native loader reads it.
            command['advisory'] = [1. if values[3] > SENSE_THRESHOLD else -1. if values[3] < -SENSE_THRESHOLD else 0., 0., 0., 0.]
        rows.append(command)
    return rows


def tensor(items):
    return torch.tensor([flatten(item['inputs']) for item in items], dtype=torch.float32)
