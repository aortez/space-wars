"""LF-060: sustained reward semantics, exact stopping preservation and physical gates."""
import argparse
from collections import Counter
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.motor_retention import inspect as retention_inspect
from .audit import inspect as acquisition_inspect
from .model import MotorModel
from .velocity import reward
from .velocity_audit import inspect,task_matches
from .aligned_audit import gate,training_stats
from . import sustained,storage

read=storage.read_json
load=storage.load_torch
PHYSICAL_FAILURES={'ship_lost','collision','damage','boundary_escape'}
FAILURE_POLICIES=['strict-v1','record-physical-failures-v1']


def training_failures(completed):
    failures=[]
    for episode in completed:
        outcome=episode['outcome']
        if outcome in ['success','deadline']:continue
        if outcome not in PHYSICAL_FAILURES:raise ValueError('unknown training outcome')
        task=episode['candidate']['task'];ticks=episode['ticks'];horizon=task.get('retention_horizon_ticks')
        failures.append({'episode':episode['id'],'case':episode['candidate']['index'],
            'speed':task['speed'],'ticks':ticks,'outcome':outcome,
            'forgone_continuation_ticks':max(0,horizon-max(task['deadline_ticks'],ticks)) if horizon and task['speed']>0 else 0})
    return failures


def preflight_checks(record,failure_policy):
    if failure_policy not in FAILURE_POLICIES:raise ValueError('unknown training failure policy')
    for outcomes in record['training_outcomes'].values():
        if set(outcomes)-PHYSICAL_FAILURES-{'success','deadline'}:raise ValueError('unknown training outcome')
    failures=record['training_physical_failures']
    assert len(failures)==sum(n for outcomes in record['training_outcomes'].values() for k,n in outcomes.items() if k in PHYSICAL_FAILURES)
    observed=record['training_stats'].get('movement_ticks_after_360',0)
    forgone=sum(v['forgone_continuation_ticks'] for v in failures) if failure_policy=='record-physical-failures-v1' else 0
    checks={'throttle_coverage':record['action_counts']['movement_thrust_above_point05']>=512,
            'brake_coverage':record['action_counts']['movement_brake_below_point4']>=512,
            'continuation_exposure_accounted':record['arm']!='sustain' or observed+forgone>=1080}
    if failure_policy=='strict-v1':checks['physical_safety']=not failures
    return checks


def inspect_training(trace):
    if trace['version']==1:return acquisition_inspect(trace,reward)
    result=retention_inspect(trace)
    result['reward']=sum(reward(s['item'])['total'] for s in trace['steps'])
    for step in trace['steps']:
        if step['item']['progress']['elapsed_ticks']<900 and step['item']['progress']['outcome'] is None:
            assert reward(step['item'])['terminal']==0.
    return result


def extra_training_stats(trace):
    out=Counter()
    if trace['command']['speed']>0:
        out['movement_ticks_after_360']=max(0,len(trace['steps'])-360)
        if trace['version']==2:
            last=trace['steps'][-1]['item'] if trace['steps'] else trace['initial'];ret=last['retention']
            if ret['first_success_tick'] is not None:
                out['movement_episodes_acquired']+=1
                out['post_acquisition_movement_ticks']+=len(trace['steps'])-ret['first_success_tick']
                out['movement_episodes_breached']+=int(ret['first_exit_tick'] is not None)
    return out


def verify_run(a,arm,seed,preflight,stage=sustained):
    phase='preflight' if preflight else 'pilot';run=a/f'{phase}/{arm}/seed-{seed}'
    repeated=a/f"{'preflight-resumed' if preflight else 'resumed'}/{arm}/seed-{seed}";c=stage.config(seed,arm,preflight);end=c['updates'];s=read(run/'summary.json')
    slots=c.get('collector_slots',4);batch=slots*c['rollout_ticks'];group=batch//2
    assert s['completed_updates']==end and s['environment_steps']==end*batch and s['parameter_count']==5259 and s['trainable_parameters']==3493 and s['total_seconds']<c['wall_seconds']
    final=load(run/f'checkpoint-{end:04}.pt.gz');again=load(repeated/f'checkpoint-{end:04}.pt.gz')
    assert equal_tree(final,again) and equal_tree(load(run/f'rollout-{end:04}.pt.gz'),load(repeated/f'rollout-{end:04}.pt.gz'))
    zero=load(run/'checkpoint-0000.pt.gz');parent_path=a/f'inputs/seed-{seed}/checkpoint-0064.pt';parent=load(parent_path)
    with torch.random.fork_rng(devices=[]):
        torch.manual_seed(seed);model=sustained.ResidualModel(final['contract']['schema']) if stage is sustained else stage.make_model(final['contract']['schema'],c);stage.initialize(model,parent_path,c)
        base=MotorModel(model.schema);base.load_state_dict(parent['model'])
    assert equal_tree(zero['model'],model.state_dict()) and zero['optimizer']['state']=={} and zero['history']==[] and zero['collector']['steps']==0
    sustained.assert_parent_actor(parent['model'],zero['model']);assert final['contract']['config']==c and not equal_tree(zero['model'],final['model'])
    assert read(run/'training-candidates.json')==stage.training_candidates(c)==read(a/f"tasks/{'preflight' if preflight else 'training'}-{arm}.json")
    assert read(run/'development-candidates.json')==stage.candidates(c['development_task_seed'],True)==read(a/'tasks/development.json')
    state=final['collector'];coverage=Counter(e['candidate']['index'] for e in state['completed'])
    if hasattr(stage,'collector_pools'):assert state['pools']==zero['collector']['pools']==stage.collector_pools(c)
    assert set(coverage)==set(v['index'] for v in stage.training_candidates(c))
    for row in stage.training_candidates(c):
        minimum=1 if preflight else 3 if row['task'].get('retention_horizon_ticks') else 7
        assert coverage[row['index']]>=minimum
    counts=Counter();outcomes={'track':Counter(),'brake':Counter()};peaks={k:0. for k in outcomes};total=0
    for e in state['completed']:
        trace=read(run/f"training-traces/episode-{e['id']:06}.json.gz");inspection=inspect_training(trace)
        assert inspection['ticks']==e['ticks'] and inspection['outcome']==e['outcome'] and abs(inspection['reward']-e['reward'])<1e-9
        stats,peak=training_stats(trace);counts.update(stats);counts.update(extra_training_stats(trace));peaks[e['candidate']['family']]=max(peaks[e['candidate']['family']],peak)
        outcomes[e['candidate']['family']][e['outcome']]+=1;total+=inspection['ticks']
    for trace in state['traces']:
        total+=inspect_training(trace)['ticks'];stats,_=training_stats(trace);counts.update(stats);counts.update(extra_training_stats(trace))
    assert total==end*batch and counts['movement_ticks']==counts['stopping_ticks']==end*group
    previous=load(run/f'checkpoint-{end-1:04}.pt.gz');restore_ticks=sum(len(t['steps']) for t in previous['collector']['traces'])
    max_error=0.;action_counts=Counter();model.requires_grad_(False);base.requires_grad_(False)
    for u in range(1,end+1):
        before=load(run/f'checkpoint-{u-1:04}.pt.gz');after=load(run/f'checkpoint-{u:04}.pt.gz')
        sustained.assert_parent_actor(parent['model'],before['model']);sustained.assert_parent_actor(parent['model'],after['model']);assert 'exploration' not in before['collector']
        model.load_state_dict(before['model']);rollout=load(run/f'rollout-{u:04}.pt.gz');flat={k:v.flatten(0,1) for k,v in rollout.items()};x=flat['observations'];movement=x[:,2]>0
        with torch.no_grad():
            actor,_=model(x);original_actor,_=base(x);logp=model.log_prob(actor,flat['latent'],flat['actuator_latents'],x)
            assert torch.equal(actor[~movement],original_actor[~movement]);assert torch.equal(actor[:,0],original_actor[:,0])
            assert torch.equal(model.distribution(actor,x).scale[~movement],base.log_std[1:].clamp(-5,2).exp().expand(group,2))
        torch.testing.assert_close(logp,flat['logp'],atol=1e-4,rtol=1e-5);torch.testing.assert_close(actor[:,0],flat['latent'],atol=1e-6,rtol=1e-5)
        max_error=max(max_error,float((logp-flat['logp']).abs().max()));assert int(movement.sum())==group
        act=flat['actuator_latents'][movement].sigmoid();action_counts['movement_thrust_above_point05']+=int((act[:,0]>.05).sum());action_counts['movement_brake_below_point4']+=int((act[:,1]<.4).sum())
    return {'arm':arm,'seed':seed,'training_coverage':dict(sorted(coverage.items())),'training_outcomes':{k:dict(v) for k,v in outcomes.items()},
        'training_physical_failures':training_failures(state['completed']),
        'training_stats':dict(counts),'training_peak_speeds':peaks,'action_counts':dict(action_counts),'likelihood_comparisons':total,'max_log_probability_error':max_error,
        'protected_stopping_rows':total//2,'restore_ticks':restore_ticks,'neutral_setups':state['started']+slots+state['started']-previous['collector']['started'],
        'optimizer_steps':sum(v['minibatches'] for v in s['history']),'repeated_optimizer_steps':s['history'][-1]['minibatches'],
        'kl_stops':sum(v['kl_early_stop'] for v in s['history']),'main_seconds':s['total_seconds'],'frozen_parent_checkpoints':end+1}


def verify(a,preflight,failure_policy='strict-v1'):
    assert failure_policy==read(a/'experiment-plan.json').get('training_failure_policy','strict-v1')
    start=time.monotonic();records=[];evaluated=0;parent_exact=zero_retained=0
    for arm in sustained.ARMS:
        for seed in sustained.SEEDS:
            r=verify_run(a,arm,seed,preflight);records.append(r)
            if preflight:
                r['preflight_checks']=preflight_checks(r,failure_policy)
                assert all(r['preflight_checks'].values())
                continue
            run=a/f'pilot/{arm}/seed-{seed}';phases={};parent=[]
            for candidate in read(a/'tasks/development.json'):
                if candidate['suite'].startswith('legacy'):
                    old=read(a/f"inputs/seed-{seed}/reference/case-{candidate['legacy_index']:02}.json.gz");parent.append({**candidate,**inspect(old)})
            for phase in ['initial','final']:
                directory=run/f'evaluation-{phase}';rows=read(directory/'results.json');assert len(rows)==len(read(directory/'parity.json'))==104;phases[phase]=[]
                for row,candidate in zip(rows,read(a/'tasks/development.json')):
                    assert row['index']==candidate['index'];trace=read(directory/f"case-{row['index']:02}.json.gz");assert task_matches(trace['task'],candidate['task'])
                    inspection=inspect(trace);assert all(row[k]==inspection[k] for k in ['ticks','outcome','final_errors'])
                    if candidate['suite'].startswith('legacy'):
                        assert trace==read(a/f"inputs/seed-{seed}/reference/case-{candidate['legacy_index']:02}.json.gz");parent_exact+=1
                    if phase=='initial' and arm=='sustain':assert trace==read(a/f"pilot/acquire/seed-{seed}/evaluation-initial/case-{candidate['index']:02}.json.gz")
                    if phase=='final' and candidate['task']['speed']==0:
                        assert trace==read(run/f"evaluation-initial/case-{candidate['index']:02}.json.gz");zero_retained+=1
                    phases[phase].append({**candidate,**inspection});evaluated+=inspection['ticks']
            r['gate']=gate(phases['initial'],phases['final'],parent);r['trajectories']=phases
    if preflight:
        result={'passed':True,'records':records,'training_ticks':30720,'optimizer_steps':sum(r['optimizer_steps']+r['repeated_optimizer_steps'] for r in records),
                'restore_ticks':sum(r['restore_ticks'] for r in records),'neutral_setups':sum(r['neutral_setups'] for r in records),'exact_resumes':6,'seconds':time.monotonic()-start}
        assert result['optimizer_steps']<=480 and result['restore_ticks']<=11070;target='preflight-results.json'
    else:
        pre=read(a/'preflight-results.json');assert pre['passed'] and pre['training_failure_policy']==failure_policy;comparisons=[]
        for seed in sustained.SEEDS:
            one=next(r for r in records if r['arm']=='acquire' and r['seed']==seed);two=next(r for r in records if r['arm']=='sustain' and r['seed']==seed)
            first=load(a/f'pilot/acquire/seed-{seed}/checkpoint-0000.pt.gz');second=load(a/f'pilot/sustain/seed-{seed}/checkpoint-0000.pt.gz')
            for k in ['model','optimizer','update','history','torch_rng','python_rng']:assert equal_tree(first[k],second[k])
            comparisons.append({'seed':seed,'tracking_gain_over_acquisition':two['gate']['tracking_retained']-one['gate']['tracking_retained'],
                                'sustained_joint_gate':two['gate']['passed'],'sustained_movement_learning':two['gate']['movement_learning_passed']})
        qualified={arm:sum(r['gate']['passed'] for r in records if r['arm']==arm) for arm in sustained.ARMS};learned={arm:sum(r['gate']['movement_learning_passed'] for r in records if r['arm']==arm) for arm in sustained.ARMS}
        assert parent_exact==672 and zero_retained==480
        result={'experiment_id':a.name,'records':records,'paired_comparisons':comparisons,'qualified_seeds':qualified,'movement_learning_seeds':learned,
            'passed':qualified['sustain']>=2,'sustained_improvement':sum(r['tracking_gain_over_acquisition']>=4 for r in comparisons)>=2,
            'training_ticks':823296,'main_training_ticks':786432,'main_movement_ticks':393216,'main_stopping_ticks':393216,
            'optimizer_steps':pre['optimizer_steps']+sum(r['optimizer_steps']+r['repeated_optimizer_steps'] for r in records),
            'restore_ticks':pre['restore_ticks']+sum(r['restore_ticks'] for r in records),'evaluation_ticks':evaluated,'evaluation_episodes':1248,
            'native_python_comparisons':1248,'likelihood_comparisons':811008,'protected_stopping_rows':405504,'frozen_parent_checkpoints':816,
            'neutral_setup_ticks':pre['neutral_setups']+sum(r['neutral_setups'] for r in records)+1248,'exact_resumes':12,
            'declared_parent_initializations':12,'paired_initial_traces_exact':312,'original_parent_traces_exact':672,'zero_command_initial_final_traces_exact':480,
            'confirmation_used':False,'model_promoted':False,'navigation_qualified':False,'seconds':time.monotonic()-start}
        assert result['optimizer_steps']<=12864 and result['restore_ticks']<=24000 and evaluated<=1123200;target='results.json'
    result['training_failure_policy']=failure_policy
    assert result['seconds']<300;atomic_json(a/target,result)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--preflight',action='store_true')
    p.add_argument('--training-failure-policy',choices=FAILURE_POLICIES,default='strict-v1');args=p.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True);verify(args.archive,args.preflight,args.training_failure_policy)
