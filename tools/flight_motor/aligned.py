"""LF-059 aligned actuator readout: matched inclusion/exclusion of stopping actor loss."""
import copy
import math
import random
import torch
from torch.distributions import Normal
from .model import MotorModel
from . import velocity
from .task import SEEDS

ARMS = ['joint','movement_only']
MODES = {'rest':0.,'slow':.5,'fast':1.5,'reverse':-1.}


def candidates(seed, development=False):
    move=[];stop=[]
    def add(rows,speed,forward,lateral,mode,level,repeat):
        index=len(rows);offset=0 if rows is move else 100
        rng=random.Random(seed*1000+offset+index);bearing=rng.uniform(-math.pi,math.pi)
        heading_offset=rng.uniform(-math.pi/3,math.pi/3);h=bearing+heading_offset
        rows.append({'family':'track' if rows is move else 'brake','suite':'aligned_tracking' if rows is move else 'aligned_stopping',
            'direction':'left' if repeat==0 else 'right','mode':mode,'level':level,
            'task':{'start':{'seed':seed*1000+offset+index,'bearing':bearing,'altitude':150.,'heading_offset':heading_offset,
                            'relative_velocity':{'x':-math.sin(h)*forward-math.cos(h)*lateral,'y':math.cos(h)*forward-math.sin(h)*lateral},
                            'spin':0.,'gravity':False,'planet_spin':0.},'heading_delta':0.,'speed':speed,'deadline_ticks':360}})
    for speed in ([6,12,18] if development else [4,10,16]):
        for mode,factor in MODES.items():
            for repeat in range(2):add(move,speed,speed*factor,0.,mode,speed,repeat)
    for speed in ([0,8,18] if development else [0,6,14]):
        for mode,(forward,lateral) in {'forward':(1,0),'reverse':(-1,0),'left':(0,1),'right':(0,-1)}.items():
            for repeat in range(2):add(stop,0.,speed*forward,speed*lateral,mode,speed,repeat)
    rows=[row for pair in zip(move,stop) for row in pair]
    if development:
        for original in velocity.candidates(5510002,True):
            if original['family']=='track':continue
            r=copy.deepcopy(original);r['suite']='legacy_braking' if r['family']=='brake' else 'legacy_heading'
            r['legacy_index']=r['index'];rows.append(r)
        for r in rows:r['task']['retention_horizon_ticks']=900
    for i,r in enumerate(rows):r['index']=i
    return rows


def config(seed,arm,preflight=False):
    if seed not in SEEDS or arm not in ARMS:raise ValueError('undeclared aligned experiment')
    c={**velocity.config(seed),'kind':'motor-aligned-readout-v1','arm':arm,'preflight':preflight,
       'train_task_seed':5910001,'development_task_seed':5910002,
       'stopping_actor_weight':1. if arm=='joint' else 0.,
       'actor_scope':'frozen-features-and-steering-actuator-readout-only',
       'actuator_initialization':{'thrust':.25,'brake':.1,'log_std':-.7},
       'steering_sampler':'deterministic-learned-mean','actuator_sampler':'independent-gaussian',
       'advantage_centering':'separate-movement-stopping','actor_group_coefficient':.5}
    if preflight:c.update(updates=2,wall_seconds=120)
    return c


def training_candidates(c):
    rows=candidates(c['train_task_seed'])
    return [rows[i] for i in [0,1,40,33]] if c['preflight'] else rows


class ReadoutModel(MotorModel):
    def __init__(self,schema):
        super().__init__(schema)
        self.actor1.requires_grad_(False);self.actor2.requires_grad_(False)
        self.actor.weight.register_hook(lambda g:g*torch.tensor([0.,1.,1.])[:,None])
        self.actor.bias.register_hook(lambda g:g*torch.tensor([0.,1.,1.]))
        self.log_std.register_hook(lambda g:g*torch.tensor([0.,1.,1.]))

    def sample(self,x):
        actor,value=self(x);sample=Normal(actor[:,1:],self.log_std[1:].clamp(-5,2).exp()).sample()
        return actor[:,0],sample,self.log_prob(actor,actor[:,0],sample),value

    def log_prob(self,actor,latent,actuators):
        dist=Normal(actor[:,1:],self.log_std[1:].clamp(-5,2).exp())
        return (dist.log_prob(actuators)-self.actuator_log_jacobian(actuators)).sum(-1)

    def entropy(self,actor):
        dist=Normal(actor[:,1:],self.log_std[1:].clamp(-5,2).exp())
        return (dist.entropy()+self.actuator_log_jacobian(dist.rsample())).sum(-1)


def initialize(model,path,c):
    velocity.initialize(model,path,c)
    with torch.no_grad():
        model.actor.weight[1:]=0.
        model.actor.bias[1:]=torch.tensor([math.log(.25/.75),math.log(.1/.9)])
        model.log_std[1:]=-.7


def assert_fixed_steering(before,after):
    for name in ['actor1.weight','actor1.bias','actor2.weight','actor2.bias']:
        assert torch.equal(before[name],after[name]),name
    for name in ['actor.weight','actor.bias','log_std']:assert torch.equal(before[name][0],after[name][0]),name


def advantages(raw,movement):
    out=torch.zeros_like(raw)
    for mask in [movement,~movement]:
        if not mask.any():raise ValueError('both command groups required')
        part=raw[mask];out[mask]=(part-part.mean())/part.std(unbiased=False).clamp_min(1e-8)
    return out


def actor_losses(ratio,adv,entropy,movement,c):
    surrogate=torch.minimum(ratio*adv,ratio.clamp(1-c['clip'],1+c['clip'])*adv)
    # A fixed coefficient keeps movement's contribution unchanged between arms.
    # Absent groups contribute zero; all actual minibatches normally contain both.
    def mean(x,mask):return x[mask].mean() if mask.any() else x.sum()*0.
    move=-mean(surrogate,movement);stop=-mean(surrogate,~movement)
    policy=.5*(move+c['stopping_actor_weight']*stop)
    entropy_loss=-c['entropy_weight']*.5*(mean(entropy,movement)+c['stopping_actor_weight']*mean(entropy,~movement))
    return policy,entropy_loss,move,stop


def update(model,optimizer,rollout,c,policy_terms=None):
    flat={k:v.flatten(0,1) for k,v in rollout.items()};movement=flat['observations'][:,2]>0
    assert int(movement.sum())==len(movement)//2
    adv=advantages(flat['advantages'],movement);logs=[];stop=False
    for epoch in range(c['epochs']):
        for indices in torch.randperm(len(adv)).split(c['minibatch']):
            actor,value=model(flat['observations'][indices]);group=movement[indices]
            if policy_terms is None:
                logp=model.log_prob(actor,flat['latent'][indices],flat['actuator_latents'][indices]);entropy=model.entropy(actor)
            else:logp,entropy=policy_terms(model,actor,flat,indices,c)
            logratio=logp-flat['logp'][indices];ratio=logratio.exp()
            policy,entropy_loss,move_loss,stop_loss=actor_losses(ratio,adv[indices],entropy,group,c)
            value_loss=.5*(value-flat['returns'][indices]).square().mean();loss=policy+c['value_weight']*value_loss+entropy_loss
            if not torch.isfinite(loss):raise RuntimeError('non-finite aligned PPO loss')
            kl=((ratio-1)-logratio).mean().detach()
            if kl>c['target_kl']:stop=True;break
            optimizer.zero_grad(set_to_none=True);loss.backward()
            norm=torch.nn.utils.clip_grad_norm_(model.parameters(),c['grad_clip'],error_if_nonfinite=True);optimizer.step()
            logs.append({'policy_loss':float(policy.detach()),'movement_policy_loss':float(move_loss.detach()),'stopping_policy_loss':float(stop_loss.detach()),
                         'value_loss':float(value_loss.detach()),'entropy_loss':float(entropy_loss.detach()),'kl':float(kl),'gradient_norm':float(norm),
                         'clip_fraction':float(((ratio-1).abs()>c['clip']).float().mean().detach())})
        if stop:break
    if not logs:raise RuntimeError('no aligned optimizer step')
    return {'minibatches':len(logs),'kl_early_stop':stop,'movement_rows':int(movement.sum()),'stopping_rows':int((~movement).sum()),
            'movement_positive_advantages':int((adv[movement]>0).sum()),'stopping_positive_advantages':int((adv[~movement]>0).sum()),
            **{k:sum(row[k] for row in logs)/len(logs) for k in logs[0]}}
