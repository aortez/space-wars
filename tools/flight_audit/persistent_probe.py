"""Frozen continuous-policy AR(1) exploration probe; no training or policy mutation."""
import argparse
from collections import Counter
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
from tools.flight_ppo.reward import reward
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.goal_response import read
from tools.flight_audit.noise_probe import save_gzip, read_gzip, metrics as physical_metrics
from tools.flight_audit.recovery_response import local_goal, wrapped
from tools.flight_audit.settle_recover import SEEDS, parity

RHOS = {'independent': 0., 'persistent': .98}
CONDITIONS = ['native', 'mean', *RHOS]
SUITES = ['position', 'hold']
INDICES = {'position': [1, 5, 9, 13], 'hold': list(range(16))}
DIRECTIONS = ['forward', 'left', 'backward', 'right']
REPEATS = 4
BASE = 4910000
AMPLITUDE = .25


def stream_seed(seed, suite, repeat, index):
    if seed not in SEEDS or suite not in SUITES or repeat not in range(REPEATS) or index not in INDICES[suite]:
        raise ValueError('outside frozen stream matrix')
    return BASE + seed*100000 + SUITES.index(suite)*10000 + repeat*100 + index


def innovations(seed, suite, repeat, index, horizon):
    if horizon != {'position': 900, 'hold': 1800}[suite]:
        raise ValueError('wrong deadline')
    return torch.randn(horizon, 3, generator=torch.Generator().manual_seed(stream_seed(seed,suite,repeat,index)))


def command(model, actor, previous_actor, previous_latent, valid, condition, noise):
    if condition not in ['mean', *RHOS] or actor.shape != (1,3) or noise.shape != actor.shape:
        raise ValueError('undeclared condition or wrong latent shape')
    if not all(torch.isfinite(x).all() for x in [actor,previous_actor,previous_latent,noise]):
        raise ValueError('nonfinite latent parameter')
    if condition == 'mean':
        return actions(actor[:,0],actor[:,1:]), actor, None
    dist = exploration.distribution(model,actor,previous_actor,previous_latent,valid,RHOS[condition],AMPLITUDE)
    latent = dist.loc + dist.scale*noise
    logp = exploration.log_prob(model,dist,latent)
    return actions(latent[:,0],latent[:,1:]), latent, float(logp[0])


def runs(values, epsilon=0.):
    lengths=[];previous=0;length=0
    for value in values:
        sign = 1 if value > epsilon else -1 if value < -epsilon else 0
        if sign != previous or sign == 0:
            if length: lengths.append(length)
            length = 0
        if sign: length += 1
        previous = sign
    if length: lengths.append(length)
    return {'runs':len(lengths),'ticks':sum(lengths),'longest':max(lengths,default=0),
            'mean':sum(lengths)/max(1,len(lengths))}


def moments(values):
    x=torch.tensor(values,dtype=torch.float64);left=x[:-1];right=x[1:]
    return {'pairs':len(left),'sx':float(left.sum()),'sy':float(right.sum()),
            'sxx':float(left.square().sum()),'syy':float(right.square().sum()),'sxy':float((left*right).sum()),
            'values':len(x),'sum':float(x.sum()),'sum_sq':float(x.square().sum())}


def pooled_moments(rows):
    sums={k:sum(r[k] for r in rows) for k in ['pairs','sx','sy','sxx','syy','sxy','values','sum','sum_sq']}
    n=sums['pairs'];count=sums['values'];mean=sums['sum']/max(count,1)
    denominator=math.sqrt(max(0.,sums['sxx']-sums['sx']**2/max(n,1))*max(0.,sums['syy']-sums['sy']**2/max(n,1)))
    correlation=(sums['sxy']-sums['sx']*sums['sy']/max(n,1))/denominator if denominator else 0.
    return {**sums,'lag1_correlation':correlation,'mean':mean,'std':math.sqrt(max(0.,sums['sum_sq']/max(count,1)-mean*mean))}


def movement(frames, suite):
    turns=[wrapped(b['angle']-a['angle']) for a,b in zip(frames,frames[1:])]
    out={'signed_turn_radians':sum(turns),'absolute_turn_radians':sum(abs(v) for v in turns),
         'turn_runs':runs(turns,1e-5)}
    if suite == 'position':
        distances=[f['errors']['position'] for f in frames]
        bearings=[abs(math.atan2(*local_goal(f))) for f in frames]
        out.update(initial_distance=distances[0],minimum_distance=min(distances),
                   distance_reduction=distances[0]-min(distances),initial_abs_bearing=bearings[0],
                   minimum_abs_bearing=min(bearings),bearing_reduction=bearings[0]-min(bearings),
                   first_radius_entry_tick=next((i for i,d in enumerate(distances) if d<=3.),None),
                   first_useful_turn_tick=next((i for i,(b,d) in enumerate(zip(bearings,distances))
                       if bearings[0]-b>=math.pi/9 and distances[0]-d>=1.),None))
    return out


def metrics(trace, suite, config):
    out=physical_metrics(trace);frames=[trace['initial']]+[s['frame'] for s in trace['steps']]
    out.update(movement(frames,suite));discounted={};undiscounted={};previous=trace['initial']
    for tick,s in enumerate(trace['steps']):
        part=reward({'frame':previous},{'frame':s['frame'],'progress':s['progress']},config['gamma'],
                    config['velocity_shaping_weight'],config['deadline_distance_weight'],config['regulation_weight'])
        for k,v in part.items():
            undiscounted[k]=undiscounted.get(k,0.)+v
            discounted[k]=discounted.get(k,0.)+config['gamma']**tick*v
        previous=s['frame']
    out.update(reward=undiscounted,discounted_reward=discounted)
    return out


def group(rows,suite):
    out={'episodes':len(rows),'successes':sum(r['outcome']=='success' for r in rows),
         'immediate_holds':sum(r['immediate_hold'] for r in rows),'physical_failures':sum(r['physical_failure'] for r in rows),
         'ticks':sum(r['ticks'] for r in rows),'maximum_dwell':max(r['maximum_dwell'] for r in rows),
         'outcomes':dict(Counter(r['outcome'] for r in rows)),
         'mean_discounted_return':sum(r['discounted_reward']['total'] for r in rows)/len(rows),
         'mean_absolute_turn_radians':sum(r['absolute_turn_radians'] for r in rows)/len(rows),
         'mean_turn_run_ticks':sum(r['turn_runs']['ticks'] for r in rows)/max(1,sum(r['turn_runs']['runs'] for r in rows)),
         'longest_turn_run_ticks':max(r['turn_runs']['longest'] for r in rows)}
    sampled=[r for r in rows if 'steering_residual_moments' in r]
    if sampled:
        out['steering_residual']=pooled_moments([r['steering_residual_moments'] for r in sampled])
        out['mean_residual_sign_run_ticks']=sum(r['residual_runs']['ticks'] for r in sampled)/max(1,sum(r['residual_runs']['runs'] for r in sampled))
    if suite=='position':
        out['directions']={d:{'episodes':sum(r['index']==i for r in rows),
            'successes':sum(r['index']==i and r['outcome']=='success' for r in rows),
            'radius_entries':sum(r['index']==i and r['first_radius_entry_tick'] is not None for r in rows),
            'useful_turns':sum(r['index']==i and r['first_useful_turn_tick'] is not None for r in rows),
            'mean_distance_reduction':sum(r['distance_reduction'] for r in rows if r['index']==i)/sum(r['index']==i for r in rows)}
            for d,i in zip(DIRECTIONS,INDICES[suite])}
    else:
        out['heading_strata']={str(i):{'episodes':sum(r['index']%4==i for r in rows),
            'successes':sum(r['index']%4==i and r['outcome']=='success' for r in rows),
            'immediate_holds':sum(r['index']%4==i and r['immediate_hold'] for r in rows)} for i in range(4)}
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
        for condition in ['mean',*RHOS]:
            for repeat in range(1 if condition=='mean' else REPEATS):
                for candidate in cs:
                    index=candidate['index'];noise=streams[repeat,index];policy_rows=[];tick=0
                    item=bridge.call('reset',tasks=[candidate['task']])['items'][0]
                    previous_actor=torch.zeros(1,3);previous_latent=torch.zeros(1,3);valid=torch.zeros(1)
                    while item['progress']['outcome'] is None:
                        if time.monotonic()>deadline:raise TimeoutError('frozen persistence-probe budget exhausted')
                        with torch.inference_mode():
                            actor,_=model(tensor([item]))
                            commands,latent,logp=command(model,actor,previous_actor,previous_latent,valid,condition,noise[tick:tick+1])
                        policy_rows.append({'actor':actor[0].tolist(),'latent':latent[0].tolist(),'action':commands[0],'logp':logp})
                        previous_actor=actor;previous_latent=latent;valid=torch.ones(1)
                        item=bridge.call('step',actions=commands)['items'][0];tick+=1
                    trace=bridge.call('state')['traces'][0];m=metrics(trace,suite,config)
                    if condition in RHOS:
                        residual=((torch.tensor([p['latent'][0] for p in policy_rows])-torch.tensor([p['actor'][0] for p in policy_rows]))/(std[0]*AMPLITUDE)).tolist()
                        m.update(steering_residual_moments=moments(residual),residual_runs=runs(residual))
                    suffix=f'{condition}/repeat-{repeat}/clear-{index}.json.gz'
                    save_gzip(out/'traces'/suffix,trace);save_gzip(out/'policy'/suffix,policy_rows)
                    rows.append(dict(m,condition=condition,repeat=repeat,index=index))
                    atomic_json(out/'progress.json',{'condition':condition,'episodes':len(rows),'ticks':sum(r['ticks'] for r in rows)})
            assert equal_tree(weights,model.state_dict())
    assert torch.equal(initial_rng,torch.get_rng_state()) and sha(policy)==policy_hash and sha(checkpoint)==checkpoint_hash
    result={'seed':seed,'suite':suite,'rows':rows,'summary':{c:group([r for r in rows if r['condition']==c],suite) for c in CONDITIONS},
            'parity':par,'seconds':time.monotonic()-start,'weights_unchanged':True,'global_rng_preserved':True,
            'native_baselines_exact':True,'latent_std':std.tolist(),'policy_sha256':policy_hash}
    assert len(rows)==10*len(cs) and result['seconds']<1800
    atomic_json(out/'results.json',result)


def gates(records):
    paired=[]
    for seed in SEEDS:
        position=next(r['summary'] for r in records if r['seed']==seed and r['suite']=='position')
        hold=next(r['summary'] for r in records if r['seed']==seed and r['suite']=='hold')
        candidate=position['persistent'];base=position['independent'];h=hold['persistent'];b=hold['independent']
        entries=lambda r:sum(r['directions'][d]['radius_entries'] for d in DIRECTIONS[1:])
        successes=lambda r:sum(r['directions'][d]['successes'] for d in DIRECTIONS[1:])
        persistence=all(s['persistent']['steering_residual']['lag1_correlation']>=.9 and
            s['persistent']['steering_residual']['lag1_correlation']-s['independent']['steering_residual']['lag1_correlation']>=.7 for s in [position,hold])
        coverage=entries(candidate)>=4 and entries(candidate)-entries(base)>=2 and successes(candidate)>=2 and successes(candidate)-successes(base)>=1 and sum(candidate['directions'][d]['radius_entries']>0 for d in DIRECTIONS[1:])>=2
        retained=h['successes']>=48 and h['successes']>=b['successes']-8 and h['immediate_holds']>=b['immediate_holds']-8 and min(v['successes'] for v in h['heading_strata'].values())>=8
        safe=candidate['physical_failures']==h['physical_failures']==0
        paired.append({'seed':seed,'persistence_manipulation':persistence,'nonforward_entries':entries(candidate),
                       'nonforward_entry_gain':entries(candidate)-entries(base),'nonforward_successes':successes(candidate),
                       'nonforward_success_gain':successes(candidate)-successes(base),'maneuver_exposure':coverage,
                       'holding_retained':retained,'physical_safety':safe,'joint_training_candidate':persistence and coverage and retained and safe})
    return {'per_seed':paired,'persistence_manipulation':all(r['persistence_manipulation'] for r in paired),
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
                        center=actor+correlation*previous;scale=std*AMPLITUDE*(1-correlation.square()).sqrt()
                        assert torch.equal(latent,center+scale*noise)
                        dist=torch.distributions.Normal(center,scale)
                        expected_logp=exploration.log_prob(FlightModel,dist,latent)
                        torch.testing.assert_close(expected_logp,torch.tensor([p['logp'] for p in policy]),atol=1e-5,rtol=1e-5)
                    decoded=actions(latent[:,0],latent[:,1:])
                    for p,s,expected in zip(policy,trace['steps'],decoded):
                        assert p['action']==s['action'] and all(abs(p['action'][k]-expected[k])<=1e-7 for k in expected)
                assert trace['task']==baselines[index]['task'] and trace['initial']==baselines[index]['initial']
                m=metrics(trace,suite,config)
                if condition in RHOS:
                    residual=((latent[:,0]-actor[:,0])/(std[0]*AMPLITUDE)).tolist()
                    m.update(steering_residual_moments=moments(residual),residual_runs=runs(residual))
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
                    assert firsts['independent',repeat,index]==firsts['persistent',repeat,index]
                    independent=lookup['independent',repeat,index];persistent=lookup['persistent',repeat,index]
                    paired.append({'seed':seed,'suite':suite,'repeat':repeat,'index':index,
                        'outcomes':[independent['outcome'],persistent['outcome']],
                        'persistent_minus_independent_return':persistent['discounted_reward']['total']-independent['discounted_reward']['total'],
                        'persistent_minus_mean_return':persistent['discounted_reward']['total']-mean['discounted_reward']['total']})
            records.append(record)
    result={'experiment_id':a.name,'records':records,'gates':gates(records),'paired_returns':paired,
            'native_python_mean_episode_differences':differences,'normal_stream_count':stream_count,
            'evaluation_episodes':sum(len(r['rows']) for r in records),'evaluation_ticks':sum(r['ticks'] for v in records for r in v['rows']),
            'admission_setups':60,'parity_comparisons':60,'neutral_setup_ticks':720,'training_ticks':0,'optimizer_steps':0,
            'weights_unchanged':True,'first_samples_paired':True,'confirmation_used':False,'model_promoted':False,'general_navigation_claim':False,'report_seconds':time.monotonic()-start}
    assert result['evaluation_episodes']==600 and result['evaluation_ticks']<=972000 and stream_count==240 and len(paired)==240 and result['report_seconds']<600
    atomic_json(a/'results.json',result)


def main():
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--seed',type=int,choices=SEEDS);p.add_argument('--suite',choices=SUITES)
    args=p.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.seed:worker(args.archive,args.seed,args.suite)
    else:verify(args.archive)


if __name__=='__main__':main()
