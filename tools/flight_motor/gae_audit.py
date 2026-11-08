"""LF-065: strict physical comparison, saved GAE checks and exact stopping protection."""
import argparse
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.ppo import gae
from tools.flight_ppo.tests import equal_tree
from .sustained_audit import read,load,verify_run,preflight_checks,inspect,task_matches,gate
from .movement_audit import feedback,verify_control
from .velocity import reward
from . import gae_comparison as comparison


def discounted_targets(rewards, dones, last, gamma):
    """Independent lambda-1 target recurrence, cutting at every terminal/reset."""
    rows = torch.empty_like(rewards); running = last.clone()
    for t in reversed(range(len(rewards))):
        running = rewards[t] + gamma * (1-dones[t]) * running
        rows[t] = running
    return rows


def compare_first(a, phase, seed):
    runs = [a/f'{phase}/{arm}/seed-{seed}' for arm in comparison.ARMS]
    starts = [load(p/'checkpoint-0000.pt.gz') for p in runs]
    assert equal_tree({k:v for k,v in starts[0].items() if k!='contract'},
                      {k:v for k,v in starts[1].items() if k!='contract'})
    first = [load(p/'rollout-0001.pt.gz') for p in runs]
    assert equal_tree({k:v for k,v in first[0].items() if k not in ['advantages','returns']},
                      {k:v for k,v in first[1].items() if k not in ['advantages','returns']})
    assert equal_tree(load(runs[0]/'checkpoint-0001.pt.gz')['collector'],
                      load(runs[1]/'checkpoint-0001.pt.gz')['collector'])
    assert not torch.equal(first[0]['advantages'],first[1]['advantages'])
    assert not torch.equal(first[0]['returns'],first[1]['returns'])
    groups = {}
    moving = first[0]['observations'][:,:,2]>0
    for name,mask in [('movement',moving),('stopping',~moving)]:
        groups[name] = {arm:{'advantage_mean':float(r['advantages'][mask].mean()),
                            'advantage_std':float(r['advantages'][mask].std(unbiased=False)),
                            'return_mean':float(r['returns'][mask].mean()),
                            'return_std':float(r['returns'][mask].std(unbiased=False))}
                       for arm,r in zip(comparison.ARMS,first)}
    return {'seed':seed,'behavior_exact':True,'collector_exact':True,'targets_differ':True,'groups':groups}


def verify_targets(a, arm, seed, preflight, stage=comparison):
    phase='preflight' if preflight else 'pilot'; run=a/f'{phase}/{arm}/seed-{seed}'
    c=stage.config(seed,arm,preflight); end=c['updates']; final=load(run/f'checkpoint-{end:04}.pt.gz')
    state=final['collector']; slots={r['index']:i for i,pool in enumerate(state['pools']) for r in pool}
    count_slots=len(state['pools']);horizon=c['rollout_ticks'];batch=count_slots*horizon
    episodes=[(e,read(run/f"training-traces/episode-{e['id']:06}.json.gz")) for e in state['completed']]
    episodes += list(zip(state['active'],state['traces'])); episodes.sort(key=lambda pair:pair[0]['id'])
    count=[0]*count_slots; observations=torch.empty(end*horizon,count_slots,18); rewards=torch.empty(end*horizon,count_slots); dones=torch.empty_like(rewards)
    for e,trace in episodes:
        n=len(trace['steps']); assert n==e['ticks']
        if not n: continue
        slot=slots[e['candidate']['index']]; lo=count[slot]; hi=lo+n
        assert hi<=end*horizon
        observations[lo:hi,slot]=torch.tensor([trace['initial']['inputs']]+[s['item']['inputs'] for s in trace['steps'][:-1]])
        rewards[lo:hi,slot]=torch.tensor([reward(s['item'])['total'] for s in trace['steps']])
        dones[lo:hi,slot]=torch.tensor([float(s['item']['progress']['outcome'] is not None) for s in trace['steps']])
        count[slot]=hi
    assert count==[end*horizon]*count_slots
    with torch.random.fork_rng(devices=[]):model=stage.make_model(final['contract']['schema'],c)
    model.requires_grad_(False); records=[]; max_error=0.
    selected=list(range(1,end+1)) if preflight else getattr(stage,'TARGET_UPDATES',[1,16,64,128])
    for u in selected:
        before=load(run/f'checkpoint-{u-1:04}.pt.gz'); after=load(run/f'checkpoint-{u:04}.pt.gz')
        rollout=load(run/f'rollout-{u:04}.pt.gz'); lo=(u-1)*horizon; hi=u*horizon
        assert torch.equal(observations[lo:hi],rollout['observations'])
        model.load_state_dict(before['model'])
        with torch.no_grad():
            values=torch.stack([model(x)[1] for x in rollout['observations']])
            last=model(torch.tensor([i['inputs'] for i in after['collector']['items']]))[1]
            advantages,returns=gae(rewards[lo:hi],dones[lo:hi],values,last,c['gamma'],c['lambda'])
        assert torch.equal(advantages,rollout['advantages']) and torch.equal(returns,rollout['returns'])
        if c['lambda']==1:
            expected=discounted_targets(rewards[lo:hi],dones[lo:hi],last,c['gamma'])
            torch.testing.assert_close(returns,expected,atol=1e-4,rtol=1e-5)
            max_error=max(max_error,float((returns-expected).abs().max()))
        records.append({'update':u,'rows':batch,'advantage_std':float(advantages.std(unbiased=False)),
                        'return_std':float(returns.std(unbiased=False))})
    result={'rows':len(selected)*batch,'lambda1_rows':len(selected)*batch if c['lambda']==1 else 0,
            'max_telescoping_rounding_error':max_error,'checks':records}
    atomic_json(run/'target-verification.json',result)
    return result


def verify(a,preflight,failure_policy='record-physical-failures-v1'):
    assert failure_policy==read(a/'experiment-plan.json').get('training_failure_policy','strict-v1')
    start=time.monotonic();records=[];evaluated=0;parent_exact=zero_retained=0
    for arm in comparison.ARMS:
        for seed in comparison.SEEDS:
            r=verify_run(a,arm,seed,preflight,stage=comparison);r['target_verification']=verify_targets(a,arm,seed,preflight);records.append(r)
            if preflight:
                r['preflight_checks']=preflight_checks({**r,'arm':'sustain'},failure_policy)
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
                    if phase=='initial' and arm=='lambda1':assert trace==read(a/f"pilot/lambda095/seed-{seed}/evaluation-initial/case-{candidate['index']:02}.json.gz")
                    if phase=='final' and candidate['task']['speed']==0:
                        assert trace==read(run/f"evaluation-initial/case-{candidate['index']:02}.json.gz");zero_retained+=1
                    phases[phase].append({**candidate,**inspection});evaluated+=inspection['ticks']
            r['gate']=gate(phases['initial'],phases['final'],parent);r['trajectories']=phases
            if arm=='lambda095':verify_control(a,run,seed)
    phase='preflight' if preflight else 'pilot'
    first_rollouts=[compare_first(a,phase,seed) for seed in comparison.SEEDS]
    if preflight:
        result={'passed':True,'records':records,'training_ticks':30720,'optimizer_steps':sum(r['optimizer_steps']+r['repeated_optimizer_steps'] for r in records),
                'restore_ticks':sum(r['restore_ticks'] for r in records),'neutral_setups':sum(r['neutral_setups'] for r in records),'exact_resumes':6,'paired_first_behavior_rollouts_exact':3,'seconds':time.monotonic()-start}
        assert result['optimizer_steps']<=480 and result['restore_ticks']<=11070;target='preflight-results.json'
    else:
        pre=read(a/'preflight-results.json');assert pre['passed'] and pre['training_failure_policy']==failure_policy;comparisons=[]
        for seed in comparison.SEEDS:
            one=next(r for r in records if r['arm']=='lambda095' and r['seed']==seed);two=next(r for r in records if r['arm']=='lambda1' and r['seed']==seed)
            first=load(a/f'pilot/lambda095/seed-{seed}/checkpoint-0000.pt.gz');second=load(a/f'pilot/lambda1/seed-{seed}/checkpoint-0000.pt.gz')
            for k in ['model','optimizer','collector','update','history','torch_rng','python_rng']:assert equal_tree(first[k],second[k])
            assert first['optimizer']['state']==second['optimizer']['state']=={}
            comparisons.append({'seed':seed,'tracking_gain_over_lambda095':two['gate']['tracking_retained']-one['gate']['tracking_retained'],
                                'lambda1_joint_gate':two['gate']['passed'],'lambda1_learning':two['gate']['movement_learning_passed']})
        qualified={arm:sum(r['gate']['passed'] for r in records if r['arm']==arm) for arm in comparison.ARMS};learned={arm:sum(r['gate']['movement_learning_passed'] for r in records if r['arm']==arm) for arm in comparison.ARMS}
        assert parent_exact==672 and zero_retained==480
        feedback(a,stage=comparison)
        result={'experiment_id':a.name,'records':records,'paired_comparisons':comparisons,'qualified_seeds':qualified,'movement_learning_seeds':learned,
            'passed':qualified['lambda1']>=2,'lambda1_improvement':sum(r['tracking_gain_over_lambda095']>=4 for r in comparisons)>=2,
            'training_ticks':823296,'main_training_ticks':786432,'main_movement_ticks':393216,'main_stopping_ticks':393216,
            'optimizer_steps':pre['optimizer_steps']+sum(r['optimizer_steps']+r['repeated_optimizer_steps'] for r in records),
            'restore_ticks':pre['restore_ticks']+sum(r['restore_ticks'] for r in records),'evaluation_ticks':evaluated,'evaluation_episodes':1248,
            'native_python_comparisons':10968,'evaluation_inference_comparisons':1248,'feedback_inference_comparisons':9720,'paired_initial_feedback_outputs_exact':2430,
            'likelihood_comparisons':811008,'protected_stopping_rows':405504,'frozen_parent_checkpoints':816,
            'neutral_setup_ticks':pre['neutral_setups']+sum(r['neutral_setups'] for r in records)+1248,'exact_resumes':12,
            'declared_parent_initializations':12,'paired_first_behavior_rollouts_exact':6,'original_control_checkpoints_exact':15,'original_control_rollouts_exact':6,'original_control_exports_exact':6,'paired_initial_traces_exact':312,'original_parent_traces_exact':672,'zero_command_initial_final_traces_exact':480,
            'confirmation_used':False,'model_promoted':False,'navigation_qualified':False,'seconds':time.monotonic()-start}
        assert result['optimizer_steps']<=12864 and result['restore_ticks']<=24000 and evaluated<=1123200;target='results.json'
    result['first_rollout_comparisons']=first_rollouts
    result['verified_gae_rows']=sum(r['target_verification']['rows'] for r in records)+(0 if preflight else pre['verified_gae_rows'])
    result['lambda1_telescoping_rows']=sum(r['target_verification']['lambda1_rows'] for r in records)+(0 if preflight else pre['lambda1_telescoping_rows'])
    result['training_failure_policy']=failure_policy
    assert result['seconds']<600;atomic_json(a/target,result)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--preflight',action='store_true');args=p.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True);verify(args.archive,args.preflight)
