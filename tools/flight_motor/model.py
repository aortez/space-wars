"""Independent small actor and critic; all actuators remain learned."""
import math
import torch
from torch import nn
from tools.flight_ppo.model import FlightModel
from tools.flight_ppo.checkpoint import atomic_json

ARCHITECTURE = 'motor18-actor32x32-critic32x32-v1'
# A 64-unit actor for laws the 32-unit actor cannot fit closely enough; exported as format 4.
WIDE_ARCHITECTURE = 'motor18-actor64x64-critic32x32-v4'
# The wide actor over the v3 contract's twenty-two inputs, the command's advisory vector among them; exported as format 5.
SENSE_ARCHITECTURE = 'motor22-actor64x64-critic32x32-v5'


class MotorModel(FlightModel):
    def __init__(self, schema, width=32):
        nn.Module.__init__(self)
        inputs = len(schema['features'])
        if (schema['version'], inputs) not in ((1, 18), (2, 18), (3, 22)):
            raise ValueError('incompatible motor schema')
        if width not in (32, 64) or (inputs == 22 and width != 64):
            raise ValueError('unsupported motor actor width')
        self.schema = schema; self.width = width; self.inputs = inputs
        self.actor1 = nn.Linear(inputs, width); self.actor2 = nn.Linear(width, width); self.actor = nn.Linear(width, 3)
        self.critic1 = nn.Linear(inputs, 32); self.critic2 = nn.Linear(32, 32); self.critic = nn.Linear(32, 1)
        self.log_std = nn.Parameter(torch.full((3,), -.7))
        for layer in self.modules():
            if isinstance(layer, nn.Linear):
                nn.init.orthogonal_(layer.weight, math.sqrt(2)); nn.init.zeros_(layer.bias)
        nn.init.orthogonal_(self.actor.weight, .01); nn.init.orthogonal_(self.critic.weight, 1.)
        with torch.no_grad(): self.actor.bias[1] = -4.  # Explicit low-thrust initial distribution, not an action override.

    def forward(self, x):
        if x.ndim != 2 or x.shape[1] != self.inputs: raise ValueError('wrong motor input shape')
        actor = self.actor(torch.tanh(self.actor2(torch.tanh(self.actor1(x)))))
        value = self.critic(torch.tanh(self.critic2(torch.tanh(self.critic1(x))))).squeeze(-1)
        return actor, value

    def export(self, path):
        format_version, architecture = ((5, SENSE_ARCHITECTURE) if self.inputs == 22 else (4, WIDE_ARCHITECTURE) if self.width == 64
                                        else (1, ARCHITECTURE))
        atomic_json(path, {'format_version': format_version, 'architecture': architecture, 'observation_schema': self.schema,
            'weights': {k: {'shape': list(v.shape), 'values': v.detach().flatten().tolist()} for k, v in self.state_dict().items()}})


def tensor(items):
    return torch.tensor([i['inputs'] for i in items], dtype=torch.float32)
