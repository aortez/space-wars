"""Exact full/quarter-noise update audit; diagnostic branches are never flown."""
import argparse
import copy
import gzip
import json
import math
from pathlib import Path
import random
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, identity, load
from tools.flight_ppo.model import FlightModel, actions, tensor
from tools.flight_ppo import exploration
from tools.flight_ppo.ppo import gae, update
from tools.flight_ppo.reward import reward
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.signal import RecordingBridge, gradient_vector, gradient_summary, summary

SEEDS=[17,29,43]
BEFORE=[0,7,15]
ARMS=["full","quarter"]
VARIANTS={'full':{},'no_entropy':{'entropy_weight':0.},'no_value':{'value_weight':0.},'policy_only':{'value_weight':0.,'entropy_weight':0.}}
METRICS=['mean_thrust','mean_brake','mean_abs_steering']


from tools.flight_audit.hold_update import AuditBridge, action_metrics, probe, row_data



COMPONENTS=['policy','weighted_critic','weighted_entropy']


def parameter_group(name):
    if name=='log_std':return 'exploration'
    if name.startswith('actor.'):return 'actor_head'
    if name.startswith('critic.'):return 'critic_head'
    return 'shared_encoder'


def parameter_masks(model):
    return {group:torch.cat([torch.full((p.numel(),),parameter_group(name)==group,dtype=torch.bool)
            for name,p in model.named_parameters()])
            for group in ['shared_encoder','actor_head','critic_head','exploration']}


def projections(model,observations,vectors):
    actor,_=model(observations);metrics=action_metrics(actor);result={};gradients={}
    masks=parameter_masks(model)
    for name,q in metrics.items():
        gradient=gradient_vector(q,list(model.parameters()));gradients[name]=gradient
        result[name]={group:{key:-float(gradient[mask].dot(vectors[key][mask])) for key in COMPONENTS}
                      for group,mask in masks.items()}
        result[name]['all']={key:-float(gradient.dot(vectors[key])) for key in COMPONENTS}
        for key in COMPONENTS:
            assert math.isclose(result[name]['all'][key],sum(result[name][g][key] for g in masks),rel_tol=1e-4,abs_tol=1e-7)
    return result,gradients


def parameter_changes(model,initial):
    parts={g:[] for g in parameter_masks(model)}
    for name,p in model.named_parameters():parts[parameter_group(name)].append((p.detach()-initial[name]).flatten())
    return {g:float(torch.cat(v).norm()) for g,v in parts.items()}


def ratio_metrics(model,rollout,config):
    flat={k:v.flatten(0,1) for k,v in rollout.items()}
    with torch.no_grad():
        actor,_=model(flat['observations']);previous,_=model(flat['ar_previous_observations'])
        dist=exploration.distribution(model,actor,previous,flat['ar_previous_latents'],flat['ar_previous_valid'],
                                      config['exploration']['rho'],exploration.amplitude(config['exploration']))
        logr=exploration.log_prob(model,dist,flat['ar_latents'])-flat['logp'];ratio=logr.exp()
    return {'approx_kl':float(((ratio-1)-logr).mean()),
            'clip_fraction':float(((ratio-1).abs()>config['clip']).float().mean()),
            'ratio_min':float(ratio.min()),'ratio_max':float(ratio.max()),'ratio_mean':float(ratio.mean())}


def inspect_gradients(model, rollout, config, saved_rng):
    flat={k:v.flatten(0,1) for k,v in rollout.items()}
    advantages=flat['advantages'];normalized=(advantages-advantages.mean())/advantages.std(unbiased=False).clamp_min(1e-8)
    torch.set_rng_state(saved_rng)
    indices=torch.randperm(len(advantages))[:config['minibatch']]
    actor,value=model(flat['observations'][indices])
    previous=model(flat['ar_previous_observations'][indices])[0]
    dist=exploration.distribution(model,actor,previous,flat['ar_previous_latents'][indices],flat['ar_previous_valid'][indices],config['exploration']['rho'],exploration.amplitude(config['exploration']))
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
    dist=exploration.distribution(model,actor,actor.detach(),flat['ar_previous_latents'],flat['ar_previous_valid'],0.,exploration.amplitude(config['exploration']))
    lp=exploration.log_prob(model,dist,flat['ar_latents'])
    torch.testing.assert_close(lp.detach(),flat['logp'],atol=5e-6,rtol=5e-6)
    scores=torch.autograd.grad(lp.sum(),actor)[0].detach()
    pressure=scores*normalized[:,None]
    return result,{'indices':indices,'parameter_names':[name for name,_ in named],
                   'policy':vectors[0],'weighted_critic':vectors[1],'weighted_entropy':vectors[2]},values.detach(),normalized,pressure



def case(a,budget,starts,remaining):
    seed=budget['seed'];arm=budget['arm'];before_n=budget['before'];after_n=budget['after'];run=a/f'inputs/{arm}/seed-{seed}'
    out=a/f'{arm}/seed-{seed}/update-{after_n:04}';out.mkdir(parents=True)
    started=time.monotonic();expected=torch.load(run/f'checkpoint-{after_n:04}.pt',weights_only=True)
    config=expected['contract']['config']
    with Bridge(a/'runtime/flight_ppo_bridge') as raw:
        bridge=AuditBridge(raw,limit=budget['restore_ticks']+1024,wall_seconds=min(180,remaining))
        model,optimizer,collector,completed,history=load(run/f'checkpoint-{before_n:04}.pt',identity(config,raw),bridge)
        assert completed==before_n and config['exploration']['rho']==0 and config['exploration']['kind']==exploration.SCALED_KIND
        initial_model=copy.deepcopy(model.state_dict());initial_optimizer=copy.deepcopy(optimizer.state_dict())
        bridge.collector=collector
        rollout=collector.collect(model,config,bridge.deadline)
        rng=torch.get_rng_state().clone();python_rng=random.getstate();reconstructed=collector.state()
        assert equal_tree(reconstructed,expected['collector']) and len(bridge.rows)==1024
        assert bridge.ticks==budget['restore_ticks']+1024 and bridge.setup_ticks==budget['neutral_setup_ticks']
        torch.save(rollout,out/'rollout.pt');torch.save(reconstructed,out/'collector.pt')
        torch.save({'torch_rng':rng,'python_rng':python_rng},out/'post-collection-rng.pt')
        rewards=torch.tensor([reward({'frame':r['before']},r['after'],config['gamma'],config['velocity_shaping_weight'],config['deadline_distance_weight'])['total'] for r in bridge.rows]).reshape(256,4)
        dones=torch.tensor([r['after']['progress']['outcome'] is not None for r in bridge.rows],dtype=torch.float32).reshape(256,4)
        with torch.no_grad():values=torch.stack([model(x)[1] for x in rollout['observations']]);last=model(tensor(collector.items))[1]
        advantages,returns=gae(rewards,dones,values,last,config['gamma'],config['lambda'])
        assert torch.equal(advantages,rollout['advantages']) and torch.equal(returns,rollout['returns'])
        torch.save({'rewards':rewards,'dones':dones,'values':values,'final_value':last,'advantages':advantages,'returns':returns},out/'targets.pt')
        observations={**starts,'rollout':rollout['observations'].flatten(0,1)}
        baseline={name:probe(model,x) for name,x in observations.items()};atomic_json(out/'before-probes.json',baseline)
        gradients,vectors,batched_values,normalized,pressure=inspect_gradients(model,rollout,config,rng)
        projection={};q_gradients={}
        for name,x in observations.items():projection[name],q_gradients[name]=projections(model,x,vectors)
        vectors['observable_gradients']=q_gradients;torch.save(vectors,out/'gradient-vectors.pt')
        gradients['observable_projections']=projection
        gradients['parameter_groups']={g:gradient_summary(*(vectors[k][mask] for k in COMPONENTS)) for g,mask in parameter_masks(model).items()}
        gradients['projection_scope']='negative raw loss-gradient dot observable-gradient at the first actual minibatch; unit gradient-descent direction, not Adam attribution'
        rows,groups=row_data(bridge.rows,rollout,values.flatten(),normalized,pressure,config)
        with gzip.open(out/'rows.json.gz','wt') as f:json.dump(rows,f,allow_nan=False)
        assert equal_tree(model.state_dict(),initial_model) and equal_tree(optimizer.state_dict(),initial_optimizer)
        branches={}
        for name,overrides in VARIANTS.items():
            if time.monotonic()>bridge.deadline:raise TimeoutError('case analysis budget')
            model.load_state_dict(initial_model);optimizer.load_state_dict(copy.deepcopy(initial_optimizer))
            torch.set_rng_state(rng);random.setstate(python_rng)
            step_probes=[]
            def after_step(opt,args,kwargs):
                step_probes.append({key:probe(model,x)['metrics'] for key,x in starts.items()})
            hook=optimizer.register_step_post_hook(after_step)
            try:metrics=update(model,optimizer,rollout,dict(config,**overrides))
            finally:hook.remove()
            state={'model':copy.deepcopy(model.state_dict()),'optimizer':copy.deepcopy(optimizer.state_dict()),
                   'torch_rng':torch.get_rng_state(),'python_rng':random.getstate(),'metrics':metrics,'coefficient_overrides':overrides}
            torch.save(state,out/f'{name}.pt')
            if name=='full':
                assert equal_tree(state['model'],expected['model']) and equal_tree(state['optimizer'],expected['optimizer'])
                assert torch.equal(state['torch_rng'],expected['torch_rng']) and state['python_rng']==expected['python_rng']
                assert all(metrics[k]==expected['history'][-1][k] for k in metrics) and history==expected['history'][:-1]
            probes={key:probe(model,x) for key,x in observations.items()};atomic_json(out/f'{name}-probes.json',probes)
            branches[name]={'overrides':overrides,'step_probes':step_probes,
                            'parameter_changes':parameter_changes(model,initial_model),
                            'end_batch_ratios':ratio_metrics(model,rollout,config),'metrics':metrics,'after':{key:v['metrics'] for key,v in probes.items()},
                            'change':{key:{m:v['metrics'][m]-baseline[key]['metrics'][m] for m in METRICS} for key,v in probes.items()}}
        record={'seed':seed,'arm':arm,'before_update':before_n,'after_update':after_n,'budget':budget,
                'before':{key:v['metrics'] for key,v in baseline.items()},'branches':branches,'gradients':gradients,'groups':groups,
                'exact_production_update':True,'exact_collector':True,'exact_gae':True,'action_ticks':bridge.ticks,'neutral_setup_ticks':bridge.setup_ticks,
                'diagnostic_optimizer_steps':sum(v['metrics']['minibatches'] for v in branches.values()),'seconds':time.monotonic()-started}
        assert record['seconds']<=180 and record['diagnostic_optimizer_steps']<=64
        atomic_json(out/'results.json',record);return record


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);args=parser.parse_args();a=args.archive
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True);started=time.monotonic()
    budgets=json.loads((a/'replay-budget.json').read_text())
    assert [(r['arm'],r['seed'],r['before'],r['after']) for r in budgets]==[(arm,s,n,n+1) for arm in ARMS for s in SEEDS for n in BEFORE]
    baselines=[json.loads((a/f'inputs/hold-baseline/seed-17/clear-{i}.json').read_text()) for i in range(16)]
    with Bridge(a/'runtime/flight_ppo_bridge') as raw:
        hold=raw.call('reset',tasks=[b['task'] for b in baselines])['items']
    assert all(item['frame']==b['initial'] for item,b in zip(hold,baselines))
    atomic_json(a/'hold-fixture.json',{'items':hold,'parent_start_frames_exact':True})
    recovery=json.loads((a/'inputs/recovery-fixture.json').read_text())['items']
    assert len(hold)==len(recovery)==16
    assert all(i['frame']['errors']=={'position':0.,'velocity':0.,'spin':0.} for i in hold)
    starts={'hold_starts':tensor(hold),'recovery_starts':tensor(recovery)}
    curves=[]
    for arm in ARMS:
        for seed in SEEDS:
            for n in range(17):
                state=torch.load(a/f'inputs/{arm}/seed-{seed}/checkpoint-{n:04}.pt',weights_only=True)
                model=FlightModel(state['contract']['schema'],state['contract']['config'].get('input_scaling'))
                model.load_state_dict(state['model'])
                curves.append({'arm':arm,'seed':seed,'update':n,
                    'probes':{k:probe(model,x) for k,x in starts.items()}})
    atomic_json(a/'checkpoint-curves.json',curves)
    records=[]
    for budget in budgets:
        remaining=1200-(time.monotonic()-started)
        if remaining<=0:raise TimeoutError('global audit budget')
        records.append(case(a,budget,starts,remaining))
        atomic_json(a/'progress.json',{'cases':len(records),'action_ticks':sum(r['action_ticks'] for r in records),'optimizer_steps':sum(r['diagnostic_optimizer_steps'] for r in records)})
    result={'experiment_id':a.name,'cases':records,'selected_before_updates':BEFORE,'seeds':SEEDS,
            'action_ticks':sum(r['action_ticks'] for r in records),'neutral_setup_ticks':sum(r['neutral_setup_ticks'] for r in records),
            'diagnostic_optimizer_steps':sum(r['diagnostic_optimizer_steps'] for r in records),'seconds':time.monotonic()-started,
            'new_training_experiment':False,'new_flight_evaluation':False,'confirmation_used':False,'model_promoted':False}
    assert result['action_ticks']==sum(b['restore_ticks']+1024 for b in budgets)
    result['neutral_setup_ticks']+=16
    assert result['neutral_setup_ticks']==16+sum(b['neutral_setup_ticks'] for b in budgets)
    assert result['diagnostic_optimizer_steps']<=1152 and result['seconds']<=1200
    atomic_json(a/'results.json',result)


if __name__=='__main__':main()
