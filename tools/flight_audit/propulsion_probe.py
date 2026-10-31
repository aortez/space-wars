"""Frozen channel-specific propulsion exploration probe; no training or policy mutation."""
import argparse
import copy
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo import exploration
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from tools.flight_ppo.evaluate import evaluate
from tools.flight_ppo.execution import load_export
from tools.flight_ppo.model import FlightModel, actions, tensor
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.goal_response import read
from tools.flight_audit.noise_probe import save_gzip, read_gzip
from tools.flight_audit.recovery_response import local_goal
from tools.flight_audit.settle_recover import parity

from tools.flight_audit.persistent_probe import (SEEDS, SUITES, INDICES, DIRECTIONS, REPEATS,
    BASE, AMPLITUDE, stream_seed, innovations, moments, pooled_moments, runs,
    metrics as persistent_metrics, group as persistent_group)

RHOS = {'quarter': .98, 'propulsion': .98}
CONDITIONS = ['native', 'mean', *RHOS]
CHANNEL_FACTORS = {'quarter': [1., 1., 1.], 'propulsion': [1., 8., 8.]}


def conditional(model, actor, previous_actor, previous_latent, valid, condition):
    if condition not in RHOS:
        raise ValueError('undeclared propulsion condition')
    base = exploration.distribution(model, actor, previous_actor, previous_latent, valid, RHOS[condition], AMPLITUDE)
    # Audit-only vector scaling. Production sampling contracts and defaults stay unchanged.
    return torch.distributions.Normal(base.loc, base.scale * torch.tensor(CHANNEL_FACTORS[condition]))


def command(model, actor, previous_actor, previous_latent, valid, condition, noise):
    if condition not in ['mean', *RHOS] or actor.shape != (1,3) or noise.shape != actor.shape:
        raise ValueError('undeclared condition or wrong latent shape')
    if not all(torch.isfinite(x).all() for x in [actor,previous_actor,previous_latent,noise]):
        raise ValueError('nonfinite latent parameter')
    if condition == 'mean':
        return actions(actor[:,0],actor[:,1:]), actor, None
    dist = conditional(model,actor,previous_actor,previous_latent,valid,condition)
    latent = dist.loc + dist.scale*noise
    return actions(latent[:,0],latent[:,1:]), latent, float(exploration.log_prob(model,dist,latent)[0])


def coverage(trace, suite):
    commands=[s['action'] for s in trace['steps']]
    joint=[a['thrust']>=.25 and a['brake']<=.25 for a in commands]
    out={'ticks':len(commands),'thrust_at_least_quarter_ticks':sum(a['thrust']>=.25 for a in commands),
         'brake_at_most_quarter_ticks':sum(a['brake']<=.25 for a in commands),
         'joint_ticks':sum(joint),'joint_longest_run':runs([int(v) for v in joint])['longest'],
         'first_joint_tick':next((i for i,v in enumerate(joint) if v),None),
         'controls':{k:{'sum':sum(a[k] for a in commands),'minimum':min(a[k] for a in commands),
                        'maximum':max(a[k] for a in commands)} for k in ['steering','thrust','brake']}}
    if suite=='position':
        before=[trace['initial']]+[s['frame'] for s in trace['steps'][:-1]]
        aligned=[];approaching=[]
        for f in before:
            dx=f['target_position']['x']-f['center_of_mass']['x'];dy=f['target_position']['y']-f['center_of_mass']['y']
            distance=math.hypot(dx,dy)
            aligned.append(distance>1e-8 and abs(math.atan2(*local_goal(f)))<=math.pi/6)
            projected=((f['velocity']['x']-f['target_velocity']['x'])*dx+
                       (f['velocity']['y']-f['target_velocity']['y'])*dy)/distance if distance>1e-8 else 0.
            approaching.append(projected>1e-6)
        out.update(aligned_joint_ticks=sum(j and b for j,b in zip(joint,aligned)),
                   approaching_aligned_joint_ticks=sum(j and b and v for j,b,v in zip(joint,aligned,approaching)))
    return out


def metrics(trace, suite, config):
    return {**persistent_metrics(trace,suite,config),'propulsion_coverage':coverage(trace,suite)}


def residual_metrics(actor, latent, std, condition):
    scale=std*AMPLITUDE*torch.tensor(CHANNEL_FACTORS[condition])
    residual=(latent-actor)/scale
    channels={k:moments(residual[:,i].tolist()) for i,k in enumerate(['steering','thrust','brake'])}
    return {'steering_residual_moments':channels['steering'],'channel_residual_moments':channels,
            'residual_runs':runs(residual[:,0].tolist())}


def group(rows,suite):
    out=persistent_group(rows,suite)
    cover=[r['propulsion_coverage'] for r in rows];ticks=sum(v['ticks'] for v in cover)
    keys=['thrust_at_least_quarter_ticks','brake_at_most_quarter_ticks','joint_ticks']
    if suite=='position':keys+=['aligned_joint_ticks','approaching_aligned_joint_ticks']
    out['propulsion_coverage']={k:sum(v[k] for v in cover) for k in keys}
    out['propulsion_coverage'].update(ticks=ticks,joint_longest_run=max(v['joint_longest_run'] for v in cover),
        controls={k:{'mean':sum(v['controls'][k]['sum'] for v in cover)/ticks,
                     'minimum':min(v['controls'][k]['minimum'] for v in cover),
                     'maximum':max(v['controls'][k]['maximum'] for v in cover)} for k in ['steering','thrust','brake']})
    sampled=[r for r in rows if 'channel_residual_moments' in r]
    if sampled:out['channel_residuals']={k:pooled_moments([v['channel_residual_moments'][k] for v in sampled]) for k in ['steering','thrust','brake']}
    return out


def worker(a,seed,suite):
    start=time.monotonic();deadline=start+1800;out=a/f'runs/seed-{seed}/{suite}';out.mkdir(parents=True)
    inputs=a/f'inputs/seed-{seed}';policy=inputs/'policy-0000.json';checkpoint=inputs/'checkpoint-0000.pt'
    policy_hash=sha(policy);checkpoint_hash=sha(checkpoint);config=read(inputs/'config.json')
    saved=torch.load(checkpoint,weights_only=True);assert config==saved['contract']['config']
    batch=read(a/f'inputs/{suite}-admission.json');assert batch['complete']
    cs=[r['candidate'] for r in batch['attempts'] if r['admitted'] and r['candidate']['index'] in INDICES[suite]]
    assert sorted(c['index'] for c in cs)==INDICES[suite]
    with Bridge(a/'runtime/flight_ppo_bridge') as bridge:
        admissions=[bridge.call('inspect',candidate=c) for c in cs];assert all(r['admitted'] for r in admissions)
    atomic_json(out/'admission.json',admissions)
    par=parity(a,inputs,0,cs,out/'parity.json')
    native=evaluate(a/'runtime/flight_ppo_bridge',policy,cs,out/'native',deadline)
    rows=[]
    for row in native:
        trace=read(out/f"native/clear-{row['index']}.json")
        assert trace==read(inputs/f"{suite}-baseline/clear-{row['index']}.json")
        rows.append(dict(metrics(trace,suite,config),condition='native',repeat=0,index=row['index']))
    initial_rng=torch.get_rng_state().clone()
    with torch.random.fork_rng(devices=[]),Bridge(a/'runtime/flight_ppo_bridge') as bridge:
        model=load_export(policy,bridge.contract['schema']);weights=copy.deepcopy(model.state_dict())
        assert equal_tree(weights,saved['model']);std=model.log_std.clamp(-5,2).exp().clone();streams={}
        for candidate in cs:
            for repeat in range(REPEATS):
                index=candidate['index'];noise=innovations(seed,suite,repeat,index,candidate['task']['deadline_ticks'])
                streams[repeat,index]=noise
                save_gzip(out/f'streams/repeat-{repeat}/clear-{index}.json.gz',{'seed':stream_seed(seed,suite,repeat,index),'values':noise.tolist()})
                assert torch.equal(noise,torch.tensor(read_gzip(inputs/f'{suite}-reference/streams/repeat-{repeat}/clear-{index}.json.gz')['values']))
        for condition in ['mean',*RHOS]:
            for repeat in range(1 if condition=='mean' else REPEATS):
                for candidate in cs:
                    index=candidate['index'];noise=streams[repeat,index];policy_rows=[];tick=0
                    item=bridge.call('reset',tasks=[candidate['task']])['items'][0]
                    previous_actor=torch.zeros(1,3);previous_latent=torch.zeros(1,3);valid=torch.zeros(1)
                    while item['progress']['outcome'] is None:
                        if time.monotonic()>deadline:raise TimeoutError('frozen propulsion-probe budget exhausted')
                        with torch.inference_mode():
                            actor,_=model(tensor([item]))
                            commands,latent,logp=command(model,actor,previous_actor,previous_latent,valid,condition,noise[tick:tick+1])
                        policy_rows.append({'actor':actor[0].tolist(),'latent':latent[0].tolist(),'action':commands[0],'logp':logp})
                        previous_actor=actor;previous_latent=latent;valid=torch.ones(1)
                        item=bridge.call('step',actions=commands)['items'][0];tick+=1
                    trace=bridge.call('state')['traces'][0];m=metrics(trace,suite,config)
                    if condition in RHOS:
                        m.update(residual_metrics(torch.tensor([p['actor'] for p in policy_rows]),torch.tensor([p['latent'] for p in policy_rows]),std,condition))
                    suffix=f'{condition}/repeat-{repeat}/clear-{index}.json.gz'
                    save_gzip(out/'traces'/suffix,trace);save_gzip(out/'policy'/suffix,policy_rows)
                    if condition in ['mean','quarter']:
                        reference=f"{condition}/repeat-{repeat}/clear-{index}.json.gz"
                        assert trace==read_gzip(inputs/f'{suite}-reference/traces'/reference)
                        assert policy_rows==read_gzip(inputs/f'{suite}-reference/policy'/reference)
                    rows.append(dict(m,condition=condition,repeat=repeat,index=index))
                    atomic_json(out/'progress.json',{'condition':condition,'episodes':len(rows),'ticks':sum(r['ticks'] for r in rows)})
            assert equal_tree(weights,model.state_dict())
    assert torch.equal(initial_rng,torch.get_rng_state()) and sha(policy)==policy_hash and sha(checkpoint)==checkpoint_hash
    result={'seed':seed,'suite':suite,'rows':rows,'summary':{c:group([r for r in rows if r['condition']==c],suite) for c in CONDITIONS},
            'parity':par,'seconds':time.monotonic()-start,'weights_unchanged':True,'global_rng_preserved':True,
            'native_baselines_exact':True,'python_reference_traces_exact':True,'latent_std':std.tolist(),'policy_sha256':policy_hash}
    assert len(rows)==10*len(cs) and result['seconds']<1800
    atomic_json(out/'results.json',result)


def gates(records):
    paired=[]
    for seed in SEEDS:
        position=next(r['summary'] for r in records if r['seed']==seed and r['suite']=='position')
        hold=next(r['summary'] for r in records if r['seed']==seed and r['suite']=='hold')
        candidate=position['propulsion'];base=position['quarter'];h=hold['propulsion'];b=hold['quarter']
        entries=lambda r:sum(r['directions'][d]['radius_entries'] for d in DIRECTIONS[1:])
        successes=lambda r:sum(r['directions'][d]['successes'] for d in DIRECTIONS[1:])
        persistence=all(s[c]['channel_residuals'][k]['lag1_correlation']>=.9 for s in [position,hold] for c in RHOS for k in ['steering','thrust','brake'])
        coverage=entries(candidate)>=4 and entries(candidate)-entries(base)>=2 and successes(candidate)>=2 and successes(candidate)-successes(base)>=1 and sum(candidate['directions'][d]['radius_entries']>0 for d in DIRECTIONS[1:])>=2
        retained=h['successes']>=48 and h['successes']>=b['successes']-8 and h['immediate_holds']>=b['immediate_holds']-8 and min(v['successes'] for v in h['heading_strata'].values())>=8
        safe=candidate['physical_failures']==h['physical_failures']==0
        paired.append({'seed':seed,'residual_persistence':persistence,'nonforward_entries':entries(candidate),
                       'nonforward_entry_gain':entries(candidate)-entries(base),'nonforward_successes':successes(candidate),
                       'nonforward_success_gain':successes(candidate)-successes(base),'maneuver_exposure':coverage,
                       'holding_retained':retained,'physical_safety':safe,'joint_training_candidate':persistence and coverage and retained and safe})
    return {'per_seed':paired,'residual_persistence':all(r['residual_persistence'] for r in paired),
            'training_candidate':sum(r['joint_training_candidate'] for r in paired)>=2}


def verify(a):
    start=time.monotonic();records=[];differences=[];paired=[];stream_count=0
    for seed in SEEDS:
        for suite in SUITES:
            out=a/f'runs/seed-{seed}/{suite}';record=read(out/'results.json');assert record['seconds']<1800
            config=read(a/f'inputs/seed-{seed}/config.json');std=torch.tensor(record['latent_std']);seen=set();firsts={}
            baselines={i:read(out/f'native/clear-{i}.json') for i in INDICES[suite]}
            for repeat in range(REPEATS):
                for index in INDICES[suite]:
                    stored=read_gzip(out/f'streams/repeat-{repeat}/clear-{index}.json.gz')
                    assert stored['seed']==stream_seed(seed,suite,repeat,index)
                    assert torch.equal(torch.tensor(stored['values']),innovations(seed,suite,repeat,index,baselines[index]['task']['deadline_ticks']))
                    assert stored==read_gzip(a/f'inputs/seed-{seed}/{suite}-reference/streams/repeat-{repeat}/clear-{index}.json.gz')
                    stream_count+=1
            for row in record['rows']:
                condition,repeat,index=row['condition'],row['repeat'],row['index'];key=(condition,repeat,index)
                assert key not in seen;seen.add(key)
                if condition=='native':trace=baselines[index]
                else:
                    suffix=f'{condition}/repeat-{repeat}/clear-{index}.json.gz'
                    trace=read_gzip(out/'traces'/suffix);policy=read_gzip(out/'policy'/suffix)
                    assert len(policy)==len(trace['steps'])
                    actor=torch.tensor([p['actor'] for p in policy]);latent=torch.tensor([p['latent'] for p in policy])
                    assert torch.isfinite(actor).all() and torch.isfinite(latent).all()
                    if condition=='mean':assert torch.equal(actor,latent) and all(p['logp'] is None for p in policy)
                    else:
                        firsts[condition,repeat,index]=policy[0]
                        noise=innovations(seed,suite,repeat,index,trace['task']['deadline_ticks'])[:len(policy)]
                        correlation=torch.full((len(policy),1),RHOS[condition]);correlation[0]=0
                        previous=torch.cat((torch.zeros(1,3),latent[:-1]-actor[:-1]))
                        center=actor+correlation*previous;scale=(std*AMPLITUDE*(1-correlation.square()).sqrt())*torch.tensor(CHANNEL_FACTORS[condition])
                        assert torch.equal(latent,center+scale*noise)
                        dist=torch.distributions.Normal(center,scale)
                        expected_logp=exploration.log_prob(FlightModel,dist,latent)
                        torch.testing.assert_close(expected_logp,torch.tensor([p['logp'] for p in policy]),atol=1e-5,rtol=1e-5)
                    decoded=actions(latent[:,0],latent[:,1:])
                    for p,s,expected in zip(policy,trace['steps'],decoded):
                        assert p['action']==s['action'] and all(abs(p['action'][k]-expected[k])<=1e-7 for k in expected)
                assert trace['task']==baselines[index]['task'] and trace['initial']==baselines[index]['initial']
                if condition=='native':
                    assert trace==read(a/f'inputs/seed-{seed}/{suite}-baseline/clear-{index}.json')
                elif condition in ['mean','quarter']:
                    reference=a/f'inputs/seed-{seed}/{suite}-reference'
                    assert trace==read_gzip(reference/'traces'/suffix)
                    assert policy==read_gzip(reference/'policy'/suffix)
                m=metrics(trace,suite,config)
                if condition in RHOS:
                    m.update(residual_metrics(actor,latent,std,condition))
                assert all(row[k]==v for k,v in m.items())
            for condition in CONDITIONS:
                rows=[r for r in record['rows'] if r['condition']==condition]
                repeats=1 if condition in ['native','mean'] else REPEATS
                assert len(rows)==len(INDICES[suite])*repeats
                assert {(r['repeat'],r['index']) for r in rows}=={(j,i) for j in range(repeats) for i in INDICES[suite]}
                assert group(rows,suite)==record['summary'][condition]
            lookup={(r['condition'],r['repeat'],r['index']):r for r in record['rows']}
            for index in INDICES[suite]:
                native=lookup['native',0,index];mean=lookup['mean',0,index]
                if (native['outcome'],native['ticks'])!=(mean['outcome'],mean['ticks']):
                    differences.append({'seed':seed,'suite':suite,'index':index,'native':[native['outcome'],native['ticks']],'python':[mean['outcome'],mean['ticks']]})
                for repeat in range(REPEATS):
                    left=firsts['quarter',repeat,index];right=firsts['propulsion',repeat,index]
                    assert left['actor']==right['actor'] and left['latent'][0]==right['latent'][0] and left['action']['steering']==right['action']['steering']
                    baseline=lookup['quarter',repeat,index];candidate=lookup['propulsion',repeat,index]
                    paired.append({'seed':seed,'suite':suite,'repeat':repeat,'index':index,
                        'outcomes':[baseline['outcome'],candidate['outcome']],
                        'propulsion_minus_quarter_return':candidate['discounted_reward']['total']-baseline['discounted_reward']['total'],
                        'propulsion_minus_mean_return':candidate['discounted_reward']['total']-mean['discounted_reward']['total']})
            records.append(record)
    result={'experiment_id':a.name,'records':records,'gates':gates(records),'paired_returns':paired,
            'native_python_mean_episode_differences':differences,'normal_stream_count':stream_count,
            'evaluation_episodes':sum(len(r['rows']) for r in records),'evaluation_ticks':sum(r['ticks'] for v in records for r in v['rows']),
            'admission_setups':60,'parity_comparisons':60,'neutral_setup_ticks':720,'training_ticks':0,'optimizer_steps':0,
            'weights_unchanged':True,'first_actors_and_steering_paired':True,'channel_scale_factors':CHANNEL_FACTORS,'rho':.98,'confirmation_used':False,'model_promoted':False,'general_navigation_claim':False,'report_seconds':time.monotonic()-start}
    assert result['evaluation_episodes']==600 and result['evaluation_ticks']<=972000 and stream_count==240 and len(paired)==240 and result['report_seconds']<600
    atomic_json(a/'results.json',result)


def main():
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--seed',type=int,choices=SEEDS);p.add_argument('--suite',choices=SUITES)
    args=p.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.seed:worker(args.archive,args.seed,args.suite)
    else:verify(args.archive)


if __name__=='__main__':main()
