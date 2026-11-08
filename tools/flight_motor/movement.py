"""LF-063: matched protected residual versus trainable movement actor."""
import torch
from torch.nn import functional as F
from tools.flight_ppo.checkpoint import atomic_json
from .model import MotorModel
from . import sustained

ARCHITECTURE='motor18-protected-movement20x32x32-v3'
ARMS=['residual','movement'];SEEDS=sustained.SEEDS


def config(seed,arm,preflight=False):
    if arm not in ARMS:raise ValueError('undeclared movement comparison')
    c=sustained.config(seed,'sustain',preflight)
    c.update(kind='motor-movement-comparison-v1',arm=arm,
             movement_actor_mode='additive-residual' if arm=='residual' else 'protected-movement-blend',
             movement_initialization='original-residual' if arm=='residual' else 'copied-original-actor-zero-error-weights')
    if arm=='movement':
        c.update(actor_scope='frozen-parent-steering-and-protected-trainable-movement-actor',
                 actuator_initialization={'features':'copied-original-actor','error_weights':'zero','fixed_offset':[2.,-2.],'residual_log_std':0.})
    return c


training_candidates=sustained.training_candidates
candidates=sustained.candidates
update=sustained.update
assert_parent_actor=sustained.assert_parent_actor


def architecture(c):return sustained.ARCHITECTURE if c['arm']=='residual' else ARCHITECTURE


class MovementModel(sustained.ResidualModel):
    def forward(self,x):
        actor,value=MotorModel.forward(self,x)
        features=sustained.residual_features(x)
        # Split the projection to preserve the copied 18-input computation exactly.
        first=F.linear(x,self.residual1.weight[:,:18].contiguous(),self.residual1.bias)
        first=first+F.linear(features[:,18:],self.residual1.weight[:,18:])
        hidden=self.residual2(first.tanh()).tanh()
        # Keep the original 3-row matmul shape; the steering row is frozen/unused.
        weights=torch.cat((self.actor.weight[:1],self.residual.weight),dim=0)
        bias=torch.cat((self.actor.bias[:1],self.residual.bias),dim=0)
        moving=F.linear(hidden,weights,bias)[:,1:]
        offset=x.new_tensor([2.,-2.])
        correction=(moving-actor[:,1:])+offset
        return torch.cat((actor[:,:1],actor[:,1:]+sustained.gate(x)[:,None]*correction),dim=1),value

    def export(self,path):
        atomic_json(path,{'format_version':3,'architecture':ARCHITECTURE,'observation_schema':self.schema,
            'weights':{k:{'shape':list(v.shape),'values':v.detach().flatten().tolist()} for k,v in self.state_dict().items()}})


def make_model(schema,c):
    return sustained.ResidualModel(schema) if c['arm']=='residual' else MovementModel(schema)


def initialize(model,path,c):
    sustained.initialize(model,path,c)
    if c['arm']=='movement':
        with torch.no_grad():
            model.residual1.weight[:,:18].copy_(model.actor1.weight)
            model.residual1.weight[:,18:]=0
            model.residual1.bias.copy_(model.actor1.bias)
            model.residual2.load_state_dict(model.actor2.state_dict())
            model.residual.weight.copy_(model.actor.weight[1:])
            model.residual.bias.copy_(model.actor.bias[1:])
