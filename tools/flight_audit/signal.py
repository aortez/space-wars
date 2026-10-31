"""Read-only learning-signal reconstruction; no optimizer step or policy promotion."""
import argparse
from collections import defaultdict
import gzip
import hashlib
import json
import math
from pathlib import Path
import random
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import identity, load
from tools.flight_ppo import exploration
from tools.flight_ppo.model import actions, tensor
from tools.flight_ppo.reward import reward
from tools.flight_ppo.tests import equal_tree

UPDATES = [0, 8, 32, 64, 96, 127]


def write(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False)+'\n')


def summary(values):
    if not values:
        return {'count':0}
    return {'count':len(values),'mean':sum(values)/len(values),
            'minimum':min(values),'maximum':max(values),
            'positive_fraction':sum(x>0 for x in values)/len(values)}


def heading_cos(frame):
    dx=frame['target_position']['x']-frame['center_of_mass']['x']
    dy=frame['target_position']['y']-frame['center_of_mass']['y']
    distance=math.hypot(dx,dy)
    return (-math.sin(frame['angle'])*dx+math.cos(frame['angle'])*dy)/max(distance,1e-12)


def reward_components(before, after, gamma, weight):
    terminal=after['progress']['outcome'] is not None
    parts={}
    for name,scale,coefficient in [('position',100.,.5),('velocity',20.,weight),('spin',1.,.25)]:
        p=-coefficient*math.tanh(before['errors'][name]/scale)
        q=0. if terminal else -coefficient*math.tanh(after['frame']['errors'][name]/scale)
        parts[name]=gamma*q-p
    standard=reward({'frame':before},after,gamma,weight)
    parts.update(time=standard['time'],terminal=standard['terminal'],total=standard['total'])
    assert math.isclose(sum(parts[k] for k in ['position','velocity','spin','time','terminal']),parts['total'],abs_tol=1e-12)
    return parts


class RecordingBridge:
    def __init__(self, raw, limit=50000, wall_seconds=180):
        self.raw=raw;self.executable=raw.executable;self.contract=raw.contract
        self.limit=limit;self.ticks=0;self.rows=[];self.collector=None
        self.deadline=time.monotonic()+wall_seconds

    def call(self, op, **kwargs):
        if time.monotonic()>self.deadline:
            raise TimeoutError('diagnostic worker wall guard')
        ticks=sum(len(t['steps']) for t in kwargs['traces']) if op=='restore' else len(kwargs['actions']) if op=='step' else 0
        if self.ticks+ticks>self.limit:
            raise RuntimeError('diagnostic replay budget exceeded')
        self.ticks+=ticks
        before=None
        if op=='step' and self.collector is not None:
            before=[(e['id'],e['ticks'],item['frame']) for e,item in zip(self.collector.active,self.collector.items)]
        result=self.raw.call(op,**kwargs)
        if before is not None:
            for slot,((episode,tick,frame),action,item) in enumerate(zip(before,kwargs['actions'],result['items'])):
                self.rows.append({'slot':slot,'episode':episode,'tick':tick,'before':frame,'action':action,'after':item})
        return result


def labels(row):
    a=row['action'];before=row['before'];after=row['after']['frame']
    d=before['errors']['position'];change=d-after['errors']['position']
    turn=heading_cos(after)-heading_cos(before)
    return {'all':True,'thrust':a['thrust'],'no_thrust':not a['thrust'],
        'brake':a['brake'],'no_brake':not a['brake'],
        'thrust_only':a['thrust'] and not a['brake'],
        'brake_only':a['brake'] and not a['thrust'],
        'both':a['thrust'] and a['brake'],'neither':not a['thrust'] and not a['brake'],
        'thrust_closing':a['thrust'] and change>1e-6,
        'thrust_opening':a['thrust'] and change< -1e-6,
        'thrust_aligned':a['thrust'] and heading_cos(before)>=.8,
        'thrust_away':a['thrust'] and heading_cos(before)<=-.8,
        'turn_toward':abs(a['steering'])>.1 and turn>1e-6,
        'turn_away':abs(a['steering'])>.1 and turn< -1e-6,
        'brake_far':a['brake'] and d>3,'brake_near':a['brake'] and d<=3,
        'speed_decrease':before['errors']['velocity']-after['errors']['velocity']>1e-6,
        'spin_decrease':before['errors']['spin']-after['errors']['spin']>1e-6}


def gradient_vector(loss, parameters, retain_graph=True):
    gradients=torch.autograd.grad(loss,parameters,allow_unused=True,retain_graph=retain_graph)
    return torch.cat([(torch.zeros_like(p) if g is None else g).detach().flatten() for p,g in zip(parameters,gradients)])


def gradient_summary(policy, critic, entropy):
    total=policy+critic+entropy
    def norm(x):return float(x.norm())
    def cosine(a,b):
        denom=float(a.norm()*b.norm())
        return float(a.dot(b))/denom if denom>1e-20 else None
    return {'policy_norm':norm(policy),'weighted_critic_norm':norm(critic),
        'weighted_entropy_norm':norm(entropy),'total_norm':norm(total),
        'critic_to_policy_norm_ratio':norm(critic)/max(norm(policy),1e-20),
        'policy_critic_cosine':cosine(policy,critic),'policy_total_cosine':cosine(policy,total)}


def inspect_gradients(model, rollout, config, saved_rng):
    flat={k:v.flatten(0,1) for k,v in rollout.items()}
    advantages=flat['advantages'];normalized=(advantages-advantages.mean())/advantages.std(unbiased=False).clamp_min(1e-8)
    torch.set_rng_state(saved_rng)
    indices=torch.randperm(len(advantages))[:config['minibatch']]
    actor,value=model(flat['observations'][indices])
    previous=model(flat['ar_previous_observations'][indices])[0]
    dist=exploration.distribution(model,actor,previous,flat['ar_previous_latents'][indices],flat['ar_previous_valid'][indices],config['exploration']['rho'])
    logp=exploration.log_prob(model,dist,flat['ar_latents'][indices])
    entropy=exploration.entropy(model,dist).mean()
    ratio=(logp-flat['logp'][indices]).exp();adv=normalized[indices]
    policy=-torch.minimum(ratio*adv,ratio.clamp(1-config['clip'],1+config['clip'])*adv).mean()
    critic=config['value_weight']*.5*(value-flat['returns'][indices]).square().mean()
    entropy_loss=-config['entropy_weight']*entropy
    named=list(model.named_parameters());parameters=[p for _,p in named]
    vectors=[gradient_vector(loss,parameters) for loss in [policy,critic,entropy_loss]]
    direct=gradient_vector(policy+critic+entropy_loss,parameters,False)
    torch.testing.assert_close(sum(vectors),direct,atol=2e-6,rtol=2e-5)
    mask=torch.cat([torch.full((p.numel(),),not name.startswith(('actor.','critic.','log_std')),dtype=torch.bool) for name,p in named])
    full=gradient_summary(*vectors);full['clipping_scale']=min(1.,config['grad_clip']/(full['total_norm']+1e-6))
    result={'scope':'actual first minibatch at archived pre-update weights; no optimizer step',
        'all_parameters':full,'shared_parameters':gradient_summary(*(v[mask] for v in vectors)),
        'losses':{'policy':float(policy.detach()),'weighted_critic':float(critic.detach()),'weighted_entropy':float(entropy_loss.detach())},
        'maximum_abs_logp_error':float((logp.detach()-flat['logp'][indices]).abs().max())}
    # Direct score pressure on the actor's outputs, excluding shared critic gradients.
    with torch.no_grad():actor,values=model(flat['observations'])
    actor=actor.detach().requires_grad_()
    dist=exploration.distribution(model,actor,actor.detach(),flat['ar_previous_latents'],flat['ar_previous_valid'],0.)
    lp=exploration.log_prob(model,dist,flat['ar_latents'])
    torch.testing.assert_close(lp.detach(),flat['logp'],atol=5e-6,rtol=5e-6)
    scores=torch.autograd.grad(lp.sum(),actor)[0].detach()
    pressure=scores*normalized[:,None]
    return result,{'indices':indices,'parameter_names':[name for name,_ in named],
                   'policy':vectors[0],'weighted_critic':vectors[1],'weighted_entropy':vectors[2]},values.detach(),normalized,pressure


def inspect_rows(rows, rollout, values, normalized, pressure, config):
    raw=rollout['advantages'].flatten();target=rollout['returns'].flatten()
    groups=defaultdict(list);compact=[]
    for i,row in enumerate(rows):
        parts=reward_components(row['before'],row['after'],config['gamma'],config['velocity_shaping_weight'])
        lab=labels(row)
        item={'slot':row['slot'],'episode':row['episode'],'tick':row['tick'],
            'action':row['action'],'before_errors':row['before']['errors'],'after_errors':row['after']['frame']['errors'],
            'outcome':row['after']['progress']['outcome'],'heading_cos_before':heading_cos(row['before']),
            'heading_cos_after':heading_cos(row['after']['frame']), 'reward':parts,
            'advantage':float(raw[i]),'normalized_advantage':float(normalized[i]),
            'critic_value':float(values[i]),'critic_target':float(target[i]),
            'thrust_score_pressure':float(pressure[i,1]),'brake_score_pressure':float(pressure[i,2]),
            'labels':[name for name,on in lab.items() if on]}
        compact.append(item)
        for name,on in lab.items():
            if on:groups[name].append(item)
    result={}
    for name,items in groups.items():
        result[name]={'count':len(items), **{key:summary([v[key] for v in items]) for key in ['advantage','normalized_advantage','thrust_score_pressure','brake_score_pressure']},
            'reward_means':{key:sum(v['reward'][key] for v in items)/len(items) for key in compact[0]['reward']}}
    return result,compact


def frozen_continuations(bridge, model, state, config, arm, seed, update, out):
    if update not in [32,96]:return []
    records=[];bridge.collector=None
    for slot in [0,3]:
        trace=state['traces'][slot]
        for repeat in [0,1]:
            diagnostic_seed=70000000+(['control','long_credit'].index(arm))*10000000+seed*100000+update*100+slot*10+repeat
            torch.manual_seed(diagnostic_seed);random.seed(diagnostic_seed)
            item=bridge.call('restore',traces=[trace])['items'][0]
            assert item==state['items'][slot]
            with torch.no_grad():prediction=float(model(tensor([item]))[1][0])
            initial=item;rows=[];discount=1.;ret=0.
            while item['progress']['outcome'] is None:
                before=item
                with torch.no_grad():
                    actor,_=model(tensor([item]))
                    dist=exploration.distribution(model,actor,actor,torch.zeros(1,3),torch.zeros(1),0.)
                    latent=dist.sample()
                command=actions(latent[:,0],(latent[:,1:]>=0).float())
                item=bridge.call('step',actions=command)['items'][0]
                terms=reward(before,item,config['gamma'],config['velocity_shaping_weight'])
                ret+=discount*terms['total'];discount*=config['gamma']
                rows.append({'action':command[0],'reward':terms,'errors':item['frame']['errors']})
                assert len(trace['steps'])+len(rows)<=1800
            saved=bridge.call('state')['traces'][0]
            filename=f'frozen-slot-{slot}-repeat-{repeat}.json.gz'
            with gzip.open(out/filename,'wt') as f:json.dump(saved,f,allow_nan=False)
            records.append({'slot':slot,'repeat':repeat,'diagnostic_seed':diagnostic_seed,
                'prefix_ticks':len(trace['steps']),'continuation_ticks':len(rows),
                'initial_errors':initial['frame']['errors'],'predicted_value':prediction,
                'realized_discounted_return':ret,'prediction_minus_return':prediction-ret,
                'outcome':item['progress']['outcome'],'trace':filename})
    return records


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--parent',type=Path,required=True);parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--arm',choices=['control','long_credit'],required=True);parser.add_argument('--seed',type=int,required=True)
    args=parser.parse_args();args.out.mkdir()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    run=args.parent/args.arm/f'seed-{args.seed}';config=json.loads((run/'contract.json').read_text())['config']
    assert config['exploration']['rho']==0.
    started=time.monotonic();total_ticks=0;reports=[]
    with Bridge(args.parent/'runtime/flight_ppo_bridge') as raw:
        contract=identity(config,raw)
        for update in UPDATES:
            case=args.out/f'update-{update:04}';case.mkdir()
            bridge=RecordingBridge(raw,limit=50000,wall_seconds=180)
            model,optimizer,collector,completed,history=load(run/f'checkpoint-{update:04}.pt',contract,bridge)
            before_parameters={k:v.clone() for k,v in model.state_dict().items()}
            before_optimizer=optimizer.state_dict()
            bridge.collector=collector
            rollout=collector.collect(model,config,bridge.deadline)
            rng=torch.get_rng_state();state=collector.state()
            expected=torch.load(run/f'checkpoint-{update+1:04}.pt',weights_only=True)
            assert equal_tree(state,expected['collector']),'reconstructed collector differs from archive'
            assert len(bridge.rows)==1024
            gradients,vectors,values,normalized,pressure=inspect_gradients(model,rollout,config,rng)
            grouped,rows=inspect_rows(bridge.rows,rollout,values,normalized,pressure,config)
            torch.save(rollout,case/'rollout.pt');torch.save(vectors,case/'gradients.pt')
            with gzip.open(case/'rows.json.gz','wt') as f:json.dump(rows,f,allow_nan=False)
            frozen=frozen_continuations(bridge,model,state,config,args.arm,args.seed,update,case)
            assert equal_tree(before_parameters,model.state_dict())
            assert equal_tree(before_optimizer,optimizer.state_dict())
            report={'update_before':update,'update_analyzed':update+1,'rows':len(rows),
                'collector_reconstruction_exact':True,'model_optimizer_unchanged':True,
                'groups':grouped,'gradients':gradients,'frozen_continuations':frozen,'replay_ticks_including_restore':bridge.ticks}
            write(case/'results.json',report);reports.append(report);total_ticks+=bridge.ticks
            assert total_ticks<=65000
            write(args.out/'progress.json',{'completed_cases':len(reports),'replay_ticks':total_ticks})
            print(json.dumps({'arm':args.arm,'seed':args.seed,'update':update,'replay_ticks':bridge.ticks}),flush=True)
        write(args.out/'results.json',{'arm':args.arm,'seed':args.seed,'cases':reports,'replay_ticks':total_ticks,'elapsed_seconds':time.monotonic()-started})


if __name__=='__main__':main()
