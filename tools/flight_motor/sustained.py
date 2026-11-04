"""LF-060: learned actuator residual with exact original zero-command behavior."""
import copy
import torch
from torch import nn
from torch.distributions import Normal
from tools.flight_ppo.checkpoint import atomic_json
from .model import MotorModel
from . import aligned,velocity

ARCHITECTURE='motor18-protected-actuator20x32x32-v2'
ARMS=['acquire','sustain']
SEEDS=aligned.SEEDS


def config(seed,arm,preflight=False):
    if arm not in ARMS:raise ValueError('undeclared sustained comparison')
    c=aligned.config(seed,'joint',False)
    c.update(kind='motor-protected-sustain-v1',arm=arm,preflight=preflight,
        actor_scope='frozen-parent-actor-with-zero-speed-gated-actuator-residual',
        actuator_initialization={'residual_bias':[2.,-2.],'residual_output_weights':'zero','residual_log_std':0.},
        movement_retention_ticks=900 if arm=='sustain' else None,
        residual_inputs='motor18-plus-local-velocity-error2',residual_gate='speed/(speed+1)',
        evidence_storage='lossless-gzip-v1')
    if preflight:c.update(updates=4,wall_seconds=180)
    return c


def training_candidates(c):
    rows=aligned.candidates(c['train_task_seed'])
    if c['movement_retention_ticks'] is not None:
        for r in rows:
            if r['task']['speed']>0:r['task']['retention_horizon_ticks']=c['movement_retention_ticks']
    return [rows[i] for i in [0,1,40,33]] if c['preflight'] else rows


def candidates(seed,development=True):
    if not development:raise ValueError('training uses its arm-specific recipe')
    return aligned.candidates(seed,True)


def residual_features(x):
    error=torch.stack((-x[:,0]*x[:,2]-x[:,3],x[:,1]*x[:,2]-x[:,4]),dim=1)
    return torch.cat((x,error),dim=1)


def gate(x):
    if (x[:,2]<0).any():raise ValueError('negative command speed')
    speed=x[:,2]*20
    return speed/(speed+1)


class ResidualModel(MotorModel):
    def __init__(self,schema):
        super().__init__(schema)
        for layer in [self.actor1,self.actor2,self.actor]:layer.requires_grad_(False)
        self.log_std.requires_grad_(False)
        self.residual1=nn.Linear(20,32);self.residual2=nn.Linear(32,32);self.residual=nn.Linear(32,2)
        self.residual_log_std=nn.Parameter(torch.zeros(2))
        for layer in [self.residual1,self.residual2]:nn.init.orthogonal_(layer.weight,2**.5);nn.init.zeros_(layer.bias)
        nn.init.zeros_(self.residual.weight)
        with torch.no_grad():self.residual.bias.copy_(torch.tensor([2.,-2.]))

    def forward(self,x):
        actor,value=super().forward(x)
        delta=self.residual(torch.tanh(self.residual2(torch.tanh(self.residual1(residual_features(x))))))
        return torch.cat((actor[:,:1],actor[:,1:]+gate(x)[:,None]*delta),dim=1),value

    def distribution(self,actor,x):
        sigma=(self.log_std[1:]+gate(x)[:,None]*self.residual_log_std).clamp(-5,2).exp()
        return Normal(actor[:,1:],sigma)

    def log_prob(self,actor,latent,z,x):
        return (self.distribution(actor,x).log_prob(z)-self.actuator_log_jacobian(z)).sum(-1)

    def entropy(self,actor,x):
        dist=self.distribution(actor,x)
        return (dist.entropy()+self.actuator_log_jacobian(dist.rsample())).sum(-1)

    def sample(self,x):
        actor,value=self(x);z=self.distribution(actor,x).sample()
        return actor[:,0],z,self.log_prob(actor,actor[:,0],z,x),value

    def export(self,path):
        atomic_json(path,{'format_version':2,'architecture':ARCHITECTURE,'observation_schema':self.schema,
            'weights':{k:{'shape':list(v.shape),'values':v.detach().flatten().tolist()} for k,v in self.state_dict().items()}})


def initialize(model,path,c):
    with torch.random.fork_rng(devices=[]):base=MotorModel(model.schema)
    velocity.initialize(base,path,c)
    missing,unexpected=model.load_state_dict(base.state_dict(),strict=False)
    assert set(missing)=={name for name in model.state_dict() if name.startswith('residual')} and not unexpected


def assert_parent_actor(parent,current):
    for name,value in parent.items():
        if name.startswith('actor') or name=='log_std':assert torch.equal(value,current[name]),name


def terms(model,actor,flat,indices,c):
    x=flat['observations'][indices]
    return model.log_prob(actor,flat['latent'][indices],flat['actuator_latents'][indices],x),model.entropy(actor,x)


def update(model,optimizer,rollout,c):
    return aligned.update(model,optimizer,rollout,c,policy_terms=terms)
