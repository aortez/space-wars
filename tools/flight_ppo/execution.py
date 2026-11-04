"""Versioned evaluation decoding, distinct from the collector's exploration law."""
import hashlib
import json
from pathlib import Path
import torch
from . import exploration
from .model import FlightModel, actions, architecture, export_version

DETERMINISTIC = 'continuous-mean-v2'
SAMPLED = 'continuous-sampled-v2'
DESCRIPTOR = Path(__file__).with_name('continuous-execution-v2.json')
DESCRIPTOR_SHA256 = 'b062e9795ace1138dd1857175c8cd2ef8f637ce6f2a6068ef46e74adadae2b4d'


def evaluation_spec(spec=None):
    if spec is None:
        return {'mode': DETERMINISTIC, 'repeats': 1}
    if not isinstance(spec, dict):
        raise ValueError('evaluation must be an object')
    mode = spec.get('mode')
    if (mode == DETERMINISTIC and type(spec.get('repeats')) is int
            and spec == {'mode': DETERMINISTIC, 'repeats': 1}):
        return dict(spec)
    if mode != SAMPLED or set(spec) != {'mode', 'repeats', 'seed_base'}:
        raise ValueError('unknown or incomplete evaluation specification')
    if type(spec['repeats']) is not int or not 1 <= spec['repeats'] <= 10:
        raise ValueError('sampled evaluation requires 1–10 repeats')
    if type(spec['seed_base']) is not int or not 0 <= spec['seed_base'] < 2**63:
        raise ValueError('invalid evaluation seed base')
    return dict(spec)


def execution_identity(spec):
    spec = evaluation_spec(spec)
    if spec['mode'] == DETERMINISTIC:
        return {'mode': DETERMINISTIC, 'implementation': 'native Rust policy action'}
    data = DESCRIPTOR.read_bytes()
    descriptor = json.loads(data)
    if (hashlib.sha256(data).hexdigest() != DESCRIPTOR_SHA256
            or descriptor['id'] != SAMPLED or descriptor['version'] != 2):
        raise ValueError('unsupported continuous execution descriptor')
    return {'mode': SAMPLED, 'descriptor': descriptor,
            'descriptor_sha256': hashlib.sha256(data).hexdigest()}


def episode_seed(spec, learner_seed, repeat, index):
    if type(learner_seed) is not int or learner_seed < 0:
        raise ValueError('sampled evaluation requires a nonnegative integer learner seed')
    if type(index) is not int or not 0 <= index < 1000:
        raise ValueError('sampled evaluation candidate indices must be unique integers in [0, 1000)')
    if type(repeat) is not int or not 0 <= repeat < spec['repeats']:
        raise ValueError('evaluation repeat outside specification')
    seed = spec['seed_base'] + learner_seed*10000 + repeat*1000 + index
    if seed >= 2**63:
        raise ValueError('evaluation episode seed exceeds supported range')
    return seed


def load_export(path, schema):
    """Read an inference export without relaxing checkpoint continuation checks.

    The caller must isolate the CPU RNG because FlightModel construction initializes
    temporary parameters before all tensors are replaced with exported values.
    """
    record = json.loads(Path(path).read_text())
    spec = architecture(record['architecture'])
    if (record['format_version'] != export_version(spec)
            or record['observation_schema'] != schema):
        raise ValueError('policy export architecture/schema mismatch')
    model = FlightModel(schema, architecture_spec=spec)
    expected = model.state_dict()
    if record['weights'].keys() != expected.keys():
        raise ValueError('policy export tensor names mismatch')
    state = {}
    for name, parameter in expected.items():
        value = record['weights'][name]
        if value['shape'] != list(parameter.shape) or len(value['values']) != parameter.numel():
            raise ValueError(f'policy export tensor shape mismatch: {name}')
        tensor = torch.tensor(value['values'], dtype=parameter.dtype).reshape(parameter.shape)
        if not torch.isfinite(tensor).all():
            raise ValueError(f'nonfinite policy export tensor: {name}')
        state[name] = tensor
    model.load_state_dict(state, strict=True)
    model.eval()
    model.requires_grad_(False)
    return model


def sampled_command(model, actor):
    distribution = exploration.distribution(model, actor, torch.zeros_like(actor),
        torch.zeros_like(actor), actor.new_zeros(len(actor)), 0.)
    latent = distribution.sample()
    return actions(latent[:, 0], latent[:, 1:]), latent.tolist()
