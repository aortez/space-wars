"""LF-058: saved trajectory, GAE, conditional-score and mean-update audit.

No environment stepping, optimizer construction/step, or changed saved weights.
"""
import argparse
import copy
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.ppo import normalize_advantages
from tools.flight_ppo.tests import equal_tree
from tools.flight_motor.model import MotorModel
from tools.flight_motor.audit import read, inspect as inspect_training, HEADING, SPIN
from tools.flight_motor.velocity import reward
from tools.flight_motor import persistent
from .motor_retention import parity

SEEDS = [17,29,43]
ARMS = ['control','persistent']
UPDATES = list(range(1,129))
GRADIENT_UPDATES = [1,10,20,24,32,64,96,128]
ANCHORS = [(24,'late'),(40,'late'),(56,'late'),(96,'late'),(28,'initial'),(44,'initial'),(60,'initial')]
FAMILIES = ['brake','track','turn','hold']


def rebuild_advantages(rewards, dones, values, last, gamma, lam, deadline_values=None):
    next_values=torch.cat((values[1:],last[None]),dim=0)
    delta=rewards+gamma*(1-dones)*next_values-values
    if deadline_values is not None:delta=delta+gamma*deadline_values
    result=torch.empty_like(delta);running=torch.zeros_like(last)
    for t in range(len(delta)-1,-1,-1):
        running=delta[t]+gamma*lam*(1-dones[t])*running;result[t]=running
    return result,result+values,delta


def episode_arrays(trace, family, episode_id):
    steps=trace['steps'];n=len(steps);items=[trace['initial']]+[s['item'] for s in steps]
    before=items[:-1];after=items[1:];command=trace['command'];h=command['heading'];speed=command['speed']
    forward=[-math.sin(h)*x['frame']['velocity']['x']+math.cos(h)*x['frame']['velocity']['y'] for x in before]
    lateral=[abs(-math.cos(h)*x['frame']['velocity']['x']-math.sin(h)*x['frame']['velocity']['y']) for x in before]
    out={'observations':torch.tensor([x['inputs'] for x in before],dtype=torch.float32),
         'after_observations':torch.tensor([x['inputs'] for x in after],dtype=torch.float32),
         'actions':torch.tensor([[s['action'][k] for k in ['steering','thrust','brake']] for s in steps]),
         'reward':torch.tensor([reward(x)['total'] for x in after]),
         'done':torch.tensor([float(x['progress']['outcome'] is not None) for x in after]),
         'deadline':torch.tensor([x['progress']['outcome']=='deadline' for x in after]),
         'family':torch.full((n,),FAMILIES.index(family),dtype=torch.int64),
         'episode':torch.full((n,),episode_id,dtype=torch.int64),
         'remaining':torch.tensor([trace['task']['deadline_ticks']-x['progress']['elapsed_ticks'] for x in before]),
         'speed':torch.full((n,),speed,dtype=torch.float32),'forward':torch.tensor(forward),'lateral':torch.tensor(lateral),
         'heading':torch.tensor([x['errors']['heading'] for x in before]),'spin':torch.tensor([x['errors']['spin'] for x in before]),
         'error_before':torch.tensor([x['errors']['velocity'] for x in before]),
         'error_after':torch.tensor([x['errors']['velocity'] for x in after])}
    for name in ['heading','velocity','spin']:
        if name=='heading':values=[-(1-math.cos(x['errors'][name]))/60 for x in after]
        elif name=='velocity':values=[-(x['errors'][name]/5)**2/60 for x in after]
        else:values=[-.1*(x['errors'][name]/1.8)**2/60 for x in after]
        out['cost_'+name]=torch.tensor(values)
    out['terminal_reward']=torch.tensor([reward(x)['terminal'] for x in after])
    rewards=[reward(x)['total'] for x in after];returns=[];g=0.
    for r in reversed(rewards):g=r+.99*g;returns.append(g)
    out['observed_episode_return']=torch.tensor(list(reversed(returns)))
    out['episode_return_valid']=torch.full((n,),bool(n and after[-1]['progress']['outcome'] is not None),dtype=torch.bool)
    for horizon in [30,60]:
        valid=torch.arange(n)+horizon<=n
        gain=torch.zeros(n);discounted=torch.zeros(n)
        for t in range(max(0,n-horizon+1)):
            gain[t]=before[t]['errors']['velocity']-after[t+horizon-1]['errors']['velocity']
            discounted[t]=sum(.99**k*rewards[t+k] for k in range(horizon))
        out[f'future_valid_{horizon}']=valid;out[f'velocity_gain_{horizon}']=gain;out[f'observed_reward_{horizon}']=discounted
    return out


def join_training(run, final):
    state=final['collector'];episodes=[]
    for e in state['completed']:episodes.append((e,run/f"training-traces/episode-{e['id']:06}.json",None))
    for e,t in zip(state['active'],state['traces']):episodes.append((e,None,t))
    episodes.sort(key=lambda x:x[0]['id']);counts=[0]*4;data=None
    for e,path,active_trace in episodes:
        trace=read(path) if path else active_trace;inspection=inspect_training(trace,reward)
        assert inspection['ticks']==e['ticks'] and abs(inspection['reward']-e['reward'])<1e-9
        if path:assert inspection['outcome']==e['outcome']
        n=e['ticks']
        if n==0:continue
        slot=e['candidate']['index']%4;arrays=episode_arrays(trace,e['candidate']['family'],e['id'])
        if data is None:
            data={k:torch.empty((32768,4)+tuple(v.shape[1:]),dtype=v.dtype) for k,v in arrays.items()}
        start=counts[slot];assert start+n<=32768
        for k,v in arrays.items():data[k][start:start+n,slot]=v
        counts[slot]+=n
    assert counts==[32768]*4
    # Future observations must remain in the same behavior-policy rollout.
    offset=torch.arange(32768)%256
    for h in [30,60]:data[f'future_valid_{h}'] &= (offset+h<=256)[:,None]
    return data


def groups(c):
    movement=c['speed']>0;under=c['speed']-c['forward']>1;over=c['forward']-c['speed']>1
    ready=(c['heading']<=HEADING)&(c['spin']<=SPIN)&(c['lateral']<=1)
    push=(c['actions'][:,1]>.5)&(c['actions'][:,2]<.1)
    improves=c['error_before']-c['error_after']>1e-4;near=c['remaining']<=30
    helpful=movement&under&ready&push&improves
    return {'all':torch.ones_like(movement),'movement':movement,'stopping':~movement,
            'underspeed':movement&under,'overspeed':movement&over,'near_forward_speed':movement&~under&~over,
            'ready_underspeed':movement&under&ready,'ready_overspeed':movement&over&ready,
            'movement_push':movement&push,'ready_under_push':movement&under&ready&push,
            'helpful_push':helpful,'helpful_push_interior':helpful&~near,'helpful_push_deadline_near':helpful&near,
            'ready_under_no_push':movement&under&ready&~push,
            'helpful_brake':movement&over&ready&(c['actions'][:,2]>.5)&improves,
            'movement_improves':movement&improves,'movement_worsens':movement&(c['error_after']-c['error_before']>1e-4),
            'movement_deadline_near':movement&near,'movement_interior':movement&~near,
            'terminal_deadline':c['deadline'],'movement_not_ready':movement&~ready,
            **{f'command_{s}':c['speed']==s for s in [4,10,16]},
            **{f'family_{f}':c['family']==i for i,f in enumerate(FAMILIES)}}


METRICS=['reward','cost_heading','cost_velocity','cost_spin','terminal_reward','error_before','error_after',
         'advantage','normalized_advantage','critic_value','td_error','critic_target',
         'counterfactual_advantage','counterfactual_normalized_advantage',
         'thrust_before','thrust_after','brake_before','brake_after','thrust_delta','brake_delta',
         'thrust_logit_delta','brake_logit_delta','log_ratio_after','surrogate_gain',
         'thrust_bias_pressure','brake_bias_pressure','raw_thrust_bias_pressure','raw_brake_bias_pressure',
         'counterfactual_thrust_bias_pressure','counterfactual_brake_bias_pressure']


def summarize(c):
    out={}
    for name,mask in groups(c).items():
        n=int(mask.sum());r={'count':n}
        if n:
            r.update({k:float(c[k][mask].double().mean()) for k in METRICS})
            r.update(advantage_positive=int((c['advantage'][mask]>0).sum()),normalized_positive=int((c['normalized_advantage'][mask]>0).sum()),
                counterfactual_normalized_positive=int((c['counterfactual_normalized_advantage'][mask]>0).sum()),
                normalized_sign_flips=int(((c['advantage'][mask]>0)!=(c['normalized_advantage'][mask]>0)).sum()),
                deadline_bootstrap_sign_flips=int(((c['normalized_advantage'][mask]>0)!=(c['counterfactual_normalized_advantage'][mask]>0)).sum()),
                thrust_increases=int((c['thrust_delta'][mask]>0).sum()),brake_decreases=int((c['brake_delta'][mask]<0).sum()),
                positive_advantage_likelihood_increases=int(((c['normalized_advantage'][mask]>0)&(c['log_ratio_after'][mask]>0)).sum()))
        complete=mask&c['episode_return_valid'];r['complete_episode_return_count']=int(complete.sum())
        if complete.any():r['observed_episode_return']=float(c['observed_episode_return'][complete].double().mean())
        for h in [30,60]:
            valid=mask&c[f'future_valid_{h}'];m=int(valid.sum());r[f'future_count_{h}']=m
            if m:
                r[f'velocity_gain_{h}']=float(c[f'velocity_gain_{h}'][valid].double().mean())
                r[f'observed_reward_{h}']=float(c[f'observed_reward_{h}'][valid].double().mean())
                r[f'future_improves_{h}']=int((c[f'velocity_gain_{h}'][valid]>1e-4).sum())
        out[name]=r
    return out


def likelihood(model, actor, flat, config):
    latents=torch.cat((flat['latent'][:,None],flat['actuator_latents']),1)
    if 'motor_exploration' in config:
        previous=model(flat['ar_previous_observations'])[0]
        dist=persistent.distribution(model,actor,previous,flat['ar_previous_latents'],flat['ar_previous_valid'],flat['observations'],config['motor_exploration'])
        rho=torch.zeros_like(actor);rho[:,1:]=((flat['observations'][:,2]>0)*flat['ar_previous_valid'])[:,None]*.95
        score=(latents-dist.loc)/dist.scale.square()*(1-rho)
        return persistent.log_prob(model,dist,latents),score
    sigma=model.log_std.clamp(-5,2).exp()
    return model.log_prob(actor,flat['latent'],flat['actuator_latents']),(latents-actor)/sigma.square()


def anchor_probe(model, inputs):
    with torch.no_grad():actor,value=model(inputs)
    return {'actor':actor.tolist(),'steering':actor[:,0].tanh().tolist(),'thrust':actor[:,1].sigmoid().tolist(),
            'brake':actor[:,2].sigmoid().tolist(),'value':value.tolist()}


def first_minibatch_gradients(model, before, rollout, config, output):
    original=copy.deepcopy(model.state_dict());model.requires_grad_(True)
    with torch.random.fork_rng(devices=[]):
        torch.set_rng_state(before['torch_rng'])
        with torch.no_grad():
            for t,x in enumerate(rollout['observations']):
                if 'motor_exploration' in config:
                    actor=model(x)[0];previous=model(rollout['ar_previous_observations'][t])[0]
                    dist=persistent.distribution(model,actor,previous,rollout['ar_previous_latents'][t],rollout['ar_previous_valid'][t],x,config['motor_exploration'])
                    assert torch.equal(dist.sample(),rollout['ar_latents'][t])
                else:
                    latent,actuators,_,_=model.sample(x)
                    assert torch.equal(latent,rollout['latent'][t]) and torch.equal(actuators,rollout['actuator_latents'][t])
        flat={k:v.flatten(0,1) for k,v in rollout.items()};adv=normalize_advantages(flat['advantages'])
        indices=torch.randperm(len(adv))[:config['minibatch']];actor,value=model(flat['observations'][indices])
        if 'motor_exploration' in config:logp,entropy=persistent.terms(model,actor,flat,indices,config)
        else:logp=model.log_prob(actor,flat['latent'][indices],flat['actuator_latents'][indices]);entropy=model.entropy(actor).mean()
        ratio=(logp-flat['logp'][indices]).exp();advantage=adv[indices]
        policy=-torch.minimum(ratio*advantage,ratio.clamp(.8,1.2)*advantage).mean()
        value_loss=.5*(value-flat['returns'][indices]).square().mean()
        losses={'policy':policy,'weighted_value':config['value_weight']*value_loss,'weighted_entropy':-config['entropy_weight']*entropy}
        params=list(model.parameters());names=[n for n,_ in model.named_parameters()]
        vectors={}
        for name,loss in {**losses,'combined':sum(losses.values())}.items():
            grad=torch.autograd.grad(loss,params,retain_graph=True,allow_unused=True)
            vectors[name]=torch.cat([(g if g is not None else torch.zeros_like(p)).flatten() for g,p in zip(grad,params)]).detach()
        torch.testing.assert_close(vectors['combined'],sum(vectors[k] for k in losses),atol=1e-6,rtol=1e-5)
        offsets={};cursor=0
        for name,p in zip(names,params):offsets[name]=[cursor,cursor+p.numel()];cursor+=p.numel()
        bias=offsets['actor.bias'];norm=float(vectors['combined'].norm())
        result={'minibatch_indices':indices.tolist(),'losses':{k:float(v.detach()) for k,v in losses.items()},
                'gradient_norms':{k:float(v.norm()) for k,v in vectors.items()},'combined_clip_factor':min(1.,config['grad_clip']/(norm+1e-6)),
                'actor_bias_ascent':{k:(-v[bias[0]:bias[1]]).tolist() for k,v in vectors.items()},
                'sampler_draws_exact':1024,'gradient_scope':'first actual minibatch before Adam; raw gradients, not optimizer attribution'}
        torch.save({'vectors':vectors,'parameter_offsets':offsets,'indices':indices},output)
    assert equal_tree(original,model.state_dict());model.requires_grad_(False)
    return result


def run(a, arm, seed):
    start=time.monotonic();deadline=start+900;source=a/f'inputs/{arm}/seed-{seed}';out=a/f'audit/{arm}/seed-{seed}';out.mkdir()
    final=torch.load(source/'checkpoint-0128.pt',weights_only=True);data=join_training(source,final)
    config=final['contract']['config'];assert config==persistent.config(seed,arm)
    rng=torch.get_rng_state().clone()
    with torch.random.fork_rng(devices=[]):model=MotorModel(final['contract']['schema'])
    model.requires_grad_(False);anchor_rows=read(a/'anchors.json');anchor_inputs=torch.tensor([r['inputs'] for r in anchor_rows])
    collected=[];timeline=[];probes=[];gradient_records=[];max_logp=0.;exact_gae=0
    model.load_state_dict(torch.load(source/'checkpoint-0000.pt',weights_only=True)['model']);probes.append({'update':0,**anchor_probe(model,anchor_inputs)})
    for update in UPDATES:
        if time.monotonic()>deadline:raise TimeoutError('motor credit audit budget')
        before=torch.load(source/f'checkpoint-{update-1:04}.pt',weights_only=True)
        after=torch.load(source/f'checkpoint-{update:04}.pt',weights_only=True);rollout=torch.load(source/f'rollout-{update:04}.pt',weights_only=True)
        lo=(update-1)*256;hi=update*256;c={k:v[lo:hi].clone() for k,v in data.items()}
        assert torch.equal(c['observations'],rollout['observations'])
        actual=torch.cat((rollout['latent'][:,:,None].tanh(),rollout['actuator_latents'].sigmoid()),2)
        torch.testing.assert_close(actual,c['actions'],atol=1e-7,rtol=1e-6)
        model.load_state_dict(before['model'])
        with torch.no_grad():
            outputs=[model(x) for x in rollout['observations']]
            actors=torch.stack([v[0] for v in outputs]);values=torch.stack([v[1] for v in outputs])
            last=model(torch.tensor([x['inputs'] for x in after['collector']['items']]))[1]
            advantage,returns,delta=rebuild_advantages(c['reward'],c['done'],values,last,config['gamma'],config['lambda'])
            assert torch.equal(advantage,rollout['advantages']) and torch.equal(returns,rollout['returns']);exact_gae+=1024
            terminal_values=torch.zeros_like(values)
            if c['deadline'].any():terminal_values[c['deadline']]=model(c['after_observations'][c['deadline']])[1]
            cf,_,_=rebuild_advantages(c['reward'],c['done'],values,last,config['gamma'],config['lambda'],terminal_values)
            flat={k:v.flatten(0,1) for k,v in rollout.items()};old_actor=actors.flatten(0,1)
            old_logp,score=likelihood(model,old_actor,flat,config)
            torch.testing.assert_close(old_logp,flat['logp'],atol=1e-4,rtol=1e-5);max_logp=max(max_logp,float((old_logp-flat['logp']).abs().max()))
        if update in GRADIENT_UPDATES:
            gradient_records.append({'update':update,**first_minibatch_gradients(model,before,rollout,config,out/f'gradients-{update:04}.pt')})
        model.load_state_dict(after['model'])
        with torch.no_grad():new_actor=model(flat['observations'])[0];new_logp,_=likelihood(model,new_actor,flat,config)
        c={k:v.flatten(0,1) for k,v in c.items()};adv=advantage.flatten();normalized=normalize_advantages(adv);ratio=(new_logp-flat['logp']).exp()
        cf_normalized=normalize_advantages(cf.flatten())
        c.update(advantage=adv,normalized_advantage=normalized,critic_value=values.flatten(),td_error=delta.flatten(),critic_target=returns.flatten(),
                 counterfactual_advantage=cf.flatten(),counterfactual_normalized_advantage=cf_normalized,
                 thrust_before=old_actor[:,1].sigmoid(),thrust_after=new_actor[:,1].sigmoid(),
                 brake_before=old_actor[:,2].sigmoid(),brake_after=new_actor[:,2].sigmoid(),
                 thrust_logit_delta=new_actor[:,1]-old_actor[:,1],brake_logit_delta=new_actor[:,2]-old_actor[:,2],
                 log_ratio_after=new_logp-flat['logp'],surrogate_gain=torch.minimum(ratio*normalized,ratio.clamp(.8,1.2)*normalized)-normalized,
                 thrust_bias_pressure=score[:,1]*normalized,brake_bias_pressure=score[:,2]*normalized,
                 raw_thrust_bias_pressure=score[:,1]*adv,raw_brake_bias_pressure=score[:,2]*adv,
                 thrust_bias_score=score[:,1],brake_bias_score=score[:,2],
                 counterfactual_thrust_bias_pressure=score[:,1]*cf_normalized,counterfactual_brake_bias_pressure=score[:,2]*cf_normalized)
        c['thrust_delta']=c['thrust_after']-c['thrust_before'];c['brake_delta']=c['brake_after']-c['brake_before']
        c['update']=torch.full((1024,),update,dtype=torch.int64)
        # Full raw observations/actions remain in copied evidence; retain compact audit columns.
        del c['observations'];del c['after_observations'];collected.append(c)
        timeline.append({'update':update,'groups':summarize(c)})
        probes.append({'update':update,**anchor_probe(model,anchor_inputs)})
    combined={k:torch.cat([c[k] for c in collected]) for k in collected[0]};torch.save(combined,out/'rows.pt')
    comparisons=[]
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        for update in [0,128]:
            state=torch.load(source/f'checkpoint-{update:04}.pt',weights_only=True);model.load_state_dict(state['model'])
            bridge.call('load_policy',path=str((source/f'policy-{update:04}.json').resolve()))
            comparisons.append({'update':update,**parity(bridge,model,anchor_inputs.tolist())})
    assert torch.equal(rng,torch.get_rng_state())
    atomic_json(out/'parity.json',comparisons);atomic_json(out/'timeline.json',timeline);atomic_json(out/'anchors.json',probes)
    result={'arm':arm,'seed':seed,'groups':summarize(combined),'gradients':gradient_records,
            'exact_gae_rows':exact_gae,'joined_rows':len(combined['reward']),'native_python_comparisons':14,
            'mean_anchor_rows':129*7,'exact_sampler_replay_rows':8192,'max_logp_error':max_logp,
            'parameters_not_optimized_rng_unchanged':True,'seconds':time.monotonic()-start}
    assert result['seconds']<900;atomic_json(out/'results.json',result)


def verify(a):
    start=time.monotonic();records=[]
    for arm in ARMS:
        for seed in SEEDS:
            out=a/f'audit/{arm}/seed-{seed}';r=read(out/'results.json');rows=torch.load(out/'rows.pt',weights_only=True)
            assert r['groups']==summarize(rows) and r['exact_gae_rows']==r['joined_rows']==131072
            assert r['parameters_not_optimized_rng_unchanged'] and [g['update'] for g in r['gradients']]==GRADIENT_UPDATES
            assert len(read(out/'anchors.json'))==129 and len(read(out/'timeline.json'))==128
            assert len(read(out/'parity.json'))==2;records.append(r)
    result={'experiment_id':a.name,'records':records,'audit_complete':True,'joined_rows':786432,'exact_gae_rows':786432,
            'native_python_comparisons':84,'mean_anchor_rows':5418,'gradient_minibatches':48,'exact_sampler_replay_rows':49152,
            'training_ticks':0,'physics_ticks':0,'optimizer_steps':0,'new_policy_exports':0,'new_training_checkpoints':0,
            'confirmation_used':False,'model_promoted':False,'navigation_qualified':False,'seconds':time.monotonic()-start}
    assert result['seconds']<300;atomic_json(a/'results.json',result)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--arm',choices=ARMS);p.add_argument('--seed',type=int,choices=SEEDS);args=p.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.arm:
        if args.seed is None:raise ValueError('seed required')
        run(args.archive,args.arm,args.seed)
    else:verify(args.archive)
