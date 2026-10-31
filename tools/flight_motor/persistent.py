"""LF-057 frozen paired configs and command-conditioned actuator AR policy."""
import torch
from torch.distributions import Normal
from tools.flight_ppo.exploration import log_prob, entropy
from .velocity import candidates, initialize, reward, config as velocity_config

ARMS = ['control', 'persistent']
SPEC = {'kind':'motor-command-conditioned-ar-v1', 'movement_multiplier':8., 'movement_rho':.95,
        'stopping_multiplier':1., 'stopping_rho':0., 'steering_multiplier':1., 'steering_rho':0.}


def config(seed, arm, preflight=False):
    if arm not in ARMS: raise ValueError('undeclared motor comparison arm')
    c = {**velocity_config(seed), 'kind':'motor-persistent-comparison-v1','arm':arm,'preflight':preflight}
    if arm == 'persistent': c['motor_exploration'] = dict(SPEC)
    if preflight: c.update(updates=2,wall_seconds=120)
    return c


def training_candidates(config):
    rows = candidates(config['train_task_seed'])
    return [rows[i] for i in [0,24,40,56]] if config['preflight'] else rows


def initial_state(batch):
    return {'spec':dict(SPEC),'previous_observations':torch.zeros(batch,18),
            'previous_latents':torch.zeros(batch,3),'previous_valid':torch.zeros(batch)}


def distribution(model, actor, previous_actor, previous_latents, valid, observations, spec):
    if spec != SPEC: raise ValueError('undeclared motor exploration contract')
    movement = observations[:,2] > 0
    rho = torch.zeros_like(actor); rho[:,1:] = movement[:,None]*SPEC['movement_rho']
    rho *= valid[:,None]
    multiplier = torch.ones_like(actor); multiplier[:,1:] = torch.where(movement,8.,1.)[:,None]
    center = actor + rho*(previous_latents-previous_actor)
    scale = model.log_std.clamp(-5,2).exp()*multiplier*torch.sqrt(1-rho.square())
    return Normal(center,scale)


def terms(model, actor, flat, indices, config):
    previous = model(flat['ar_previous_observations'][indices])[0]
    dist = distribution(model,actor,previous,flat['ar_previous_latents'][indices],
                        flat['ar_previous_valid'][indices],flat['observations'][indices],config['motor_exploration'])
    return log_prob(model,dist,flat['ar_latents'][indices]), entropy(model,dist).mean()


def advance(state, observations, latents, ended):
    valid = torch.tensor([not value for value in ended],dtype=torch.float32)
    state['previous_observations'] = observations.clone()*valid[:,None]
    state['previous_latents'] = latents.clone()*valid[:,None]
    state['previous_valid'] = valid


def validate_state(state, active):
    if state['spec'] != SPEC: raise ValueError('motor exploration state contract mismatch')
    n=len(active)
    for name,shape in [('previous_observations',(n,18)),('previous_latents',(n,3)),('previous_valid',(n,))]:
        value=state[name]
        if value.shape!=shape or not torch.isfinite(value).all():raise ValueError('invalid motor exploration state tensor')
    for i,episode in enumerate(active):
        expected=float(episode['ticks']>0)
        if state['previous_valid'][i] != expected: raise ValueError('motor exploration episode cursor mismatch')
        if not expected and (state['previous_observations'][i].count_nonzero() or state['previous_latents'][i].count_nonzero()):
            raise ValueError('motor exploration state leaked across reset')
