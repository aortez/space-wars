"""LF-059 physical gates, stopping-loss comparison and complete retention evidence."""
import argparse
from collections import Counter
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.motor_retention import seed_gate
from .audit import inspect as inspect_training,read,HEADING,SPIN
from .velocity_audit import inspect,task_matches
from .velocity import reward
from . import aligned


def gate(initial,final,parent):
    tracking=[v for v in final if v['suite']=='aligned_tracking'];stopping=[v for v in final if v['suite']=='aligned_stopping']
    braking=[v for v in final if v['suite']=='legacy_braking'];heading=[v for v in final if v['suite']=='legacy_heading']
    assert [len(v) for v in [tracking,stopping,braking,heading]]==[24,24,24,32]
    original={v['legacy_index']:v['retained'] for v in parent}
    heading_gate=seed_gate([{**v,'parent_success':original[v['legacy_index']]} for v in heading])
    def counts(rows,key):return {str(v):sum(r['retained'] for r in rows if r[key]==v) for v in sorted({r[key] for r in rows})}
    tracked=sum(v['retained'] for v in tracking);baseline=sum(v['retained'] for v in initial if v['suite']=='aligned_tracking')
    stopped=sum(v['retained'] for v in stopping);braked=sum(v['retained'] for v in braking)
    stationary=sum(v['retained'] for v in stopping if v['level']==0)
    failures=sum(v['outcome'] not in ['success','deadline'] for v in final)
    checks={'aligned_tracking':tracked>=20 and min(counts(tracking,'level').values())>=6 and min(counts(tracking,'mode').values())>=4,
            'tracking_gain':tracked-baseline>=12,'aligned_stopping':stopped>=20 and stationary>=7,
            'legacy_braking':braked>=20,'original_heading_retention':heading_gate['passed'],'physical_safety':failures==0}
    return {'passed':all(checks.values()),'movement_learning_passed':checks['aligned_tracking'] and checks['tracking_gain'] and checks['physical_safety'],
            'checks':checks,'tracking_retained':tracked,'initial_tracking_retained':baseline,'tracking_gain':tracked-baseline,
            'tracking_by_speed':counts(tracking,'level'),'tracking_by_mode':counts(tracking,'mode'),
            'aligned_stopping_retained':stopped,'aligned_stationary_retained':stationary,'legacy_braking_retained':braked,'heading':heading_gate,'physical_failures':failures}


def training_stats(trace):
    previous=trace['initial'];command=trace['command'];movement=command['speed']>0
    counts=Counter();peak=0.
    for step in trace['steps']:
        current=step['item'];v=previous['frame']['velocity'];h=command['heading'];action=step['action']
        forward=-math.sin(h)*v['x']+math.cos(h)*v['y'];lateral=abs(-math.cos(h)*v['x']-math.sin(h)*v['y'])
        e=previous['errors'];ready=e['heading']<=HEADING and e['spin']<=SPIN and lateral<=1
        improves=e['velocity']-current['errors']['velocity']>1e-4
        counts['movement_ticks' if movement else 'stopping_ticks']+=1
        if movement:
            counts['movement_heading_spin_good']+=int(e['heading']<=HEADING and e['spin']<=SPIN)
            under=command['speed']-forward>1
            counts['helpful_strict_push']+=int(ready and under and improves and action['thrust']>.5 and action['brake']<.1)
            counts['helpful_gentle_push']+=int(ready and under and improves and action['thrust']>.1 and action['brake']<.2)
        v=current['frame']['velocity'];peak=max(peak,math.hypot(v['x'],v['y']));previous=current
    return counts,peak


def verify_run(a,arm,seed,preflight):
    phase='preflight' if preflight else 'pilot';run=a/f'{phase}/{arm}/seed-{seed}'
    repeat=a/f"{'preflight-resumed' if preflight else 'resumed'}/{arm}/seed-{seed}"
    c=aligned.config(seed,arm,preflight);end=c['updates'];s=read(run/'summary.json')
    assert s['completed_updates']==end and s['environment_steps']==end*1024 and s['parameter_count']==3463 and s['effective_trainable_parameters']==1765 and s['total_seconds']<c['wall_seconds']
    final=torch.load(run/f'checkpoint-{end:04}.pt',weights_only=True);again=torch.load(repeat/f'checkpoint-{end:04}.pt',weights_only=True)
    assert equal_tree(final,again) and equal_tree(torch.load(run/f'rollout-{end:04}.pt',weights_only=True),torch.load(repeat/f'rollout-{end:04}.pt',weights_only=True))
    zero=torch.load(run/'checkpoint-0000.pt',weights_only=True);parent_path=a/f'inputs/seed-{seed}/checkpoint-0064.pt'
    parent=torch.load(parent_path,weights_only=True)
    with torch.random.fork_rng(devices=[]):model=aligned.ReadoutModel(final['contract']['schema']);aligned.initialize(model,parent_path,c)
    assert equal_tree(zero['model'],model.state_dict()) and zero['optimizer']['state']=={} and zero['history']==[] and zero['collector']['steps']==0
    aligned.assert_fixed_steering(parent['model'],zero['model'])
    assert final['contract']['config']==c and not equal_tree(zero['model'],final['model'])
    assert read(run/'training-candidates.json')==aligned.training_candidates(c)==read(a/f"tasks/{'preflight' if preflight else 'training'}.json")
    assert read(run/'development-candidates.json')==aligned.candidates(c['development_task_seed'],True)==read(a/'tasks/development.json')
    state=final['collector'];coverage=Counter(e['candidate']['index'] for e in state['completed'])
    assert set(coverage)==set(v['index'] for v in aligned.training_candidates(c)) and min(coverage.values())>=(1 if preflight else 7)
    counts=Counter();outcomes={family:Counter() for family in ['track','brake']};peaks={k:0. for k in outcomes};total=0
    for e in state['completed']:
        trace=read(run/f"training-traces/episode-{e['id']:06}.json");inspection=inspect_training(trace,reward)
        assert inspection['ticks']==e['ticks'] and inspection['outcome']==e['outcome'] and abs(inspection['reward']-e['reward'])<1e-9
        stats,peak=training_stats(trace);counts.update(stats);peaks[e['candidate']['family']]=max(peaks[e['candidate']['family']],peak)
        outcomes[e['candidate']['family']][e['outcome']]+=1;total+=inspection['ticks']
    for trace in state['traces']:
        total+=inspect_training(trace,reward)['ticks'];stats,_=training_stats(trace);counts.update(stats)
    assert total==end*1024 and counts['movement_ticks']==counts['stopping_ticks']==end*512
    previous=torch.load(run/f'checkpoint-{end-1:04}.pt',weights_only=True)
    restore_ticks=sum(len(t['steps']) for t in previous['collector']['traces'])
    max_error=0.;action_counts=Counter();model.requires_grad_(False)
    for u in range(1,end+1):
        before=torch.load(run/f'checkpoint-{u-1:04}.pt',weights_only=True);after=torch.load(run/f'checkpoint-{u:04}.pt',weights_only=True)
        aligned.assert_fixed_steering(parent['model'],before['model']);aligned.assert_fixed_steering(parent['model'],after['model'])
        assert 'exploration' not in before['collector']
        model.load_state_dict(before['model']);rollout=torch.load(run/f'rollout-{u:04}.pt',weights_only=True);flat={k:v.flatten(0,1) for k,v in rollout.items()}
        with torch.no_grad():actor,_=model(flat['observations']);logp=model.log_prob(actor,flat['latent'],flat['actuator_latents'])
        torch.testing.assert_close(logp,flat['logp'],atol=1e-4,rtol=1e-5);torch.testing.assert_close(actor[:,0],flat['latent'],atol=1e-6,rtol=1e-5)
        max_error=max(max_error,float((logp-flat['logp']).abs().max()));move=flat['observations'][:,2]>0;assert int(move.sum())==512
        act=flat['actuator_latents'][move].sigmoid();action_counts['movement_thrust_above_point1']+=int((act[:,0]>.1).sum());action_counts['movement_brake_below_point2']+=int((act[:,1]<.2).sum())
    return {'arm':arm,'seed':seed,'training_coverage':dict(sorted(coverage.items())),'training_outcomes':{k:dict(v) for k,v in outcomes.items()},
        'training_stats':dict(counts),'training_peak_speeds':peaks,'action_counts':dict(action_counts),'likelihood_comparisons':total,'max_log_probability_error':max_error,
        'restore_ticks':restore_ticks,'neutral_setups':state['started']+4+state['started']-previous['collector']['started'],
        'optimizer_steps':sum(v['minibatches'] for v in s['history']),'repeated_optimizer_steps':s['history'][-1]['minibatches'],
        'kl_stops':sum(v['kl_early_stop'] for v in s['history']),'main_seconds':s['total_seconds'],'fixed_steering_checkpoints':end+1}


def verify(a,preflight):
    start=time.monotonic();records=[];evaluated=0
    for arm in aligned.ARMS:
        for seed in aligned.SEEDS:
            r=verify_run(a,arm,seed,preflight);records.append(r)
            if preflight:
                assert all(k in ['success','deadline'] for v in r['training_outcomes'].values() for k in v)
                assert r['action_counts']['movement_thrust_above_point1']>=256 and r['action_counts']['movement_brake_below_point2']>=256
                continue
            run=a/f'pilot/{arm}/seed-{seed}';phases={};parent=[]
            for candidate in read(a/'tasks/development.json'):
                if candidate['suite'].startswith('legacy'):
                    old=read(a/f"inputs/seed-{seed}/reference/case-{candidate['legacy_index']:02}.json")
                    parent.append({**candidate,**inspect(old)})
            for phase in ['initial','final']:
                directory=run/f'evaluation-{phase}';rows=read(directory/'results.json');assert len(rows)==len(read(directory/'parity.json'))==104;phases[phase]=[]
                for row,candidate in zip(rows,read(a/'tasks/development.json')):
                    assert row['index']==candidate['index'];trace=read(directory/f"case-{row['index']:02}.json");assert task_matches(trace['task'],candidate['task'])
                    inspection=inspect(trace);assert all(row[k]==inspection[k] for k in ['ticks','outcome','final_errors'])
                    if phase=='initial':
                        if candidate['suite'].startswith('legacy'):
                            old=read(a/f"inputs/seed-{seed}/reference/case-{candidate['legacy_index']:02}.json");assert trace['initial']==old['initial']
                        if arm=='movement_only':assert trace==read(a/f"pilot/joint/seed-{seed}/evaluation-initial/case-{candidate['index']:02}.json")
                    phases[phase].append({**candidate,**inspection});evaluated+=inspection['ticks']
            r['gate']=gate(phases['initial'],phases['final'],parent);r['trajectories']=phases
    if preflight:
        result={'passed':True,'records':records,'training_ticks':18432,'optimizer_steps':sum(r['optimizer_steps']+r['repeated_optimizer_steps'] for r in records),
                'restore_ticks':sum(r['restore_ticks'] for r in records),'neutral_setups':sum(r['neutral_setups'] for r in records),'exact_resumes':6,'seconds':time.monotonic()-start}
        assert result['optimizer_steps']<=288 and result['restore_ticks']<=6144;target='preflight-results.json'
    else:
        pre=read(a/'preflight-results.json');assert pre['passed'];comparisons=[]
        for seed in aligned.SEEDS:
            joint=next(r for r in records if r['arm']=='joint' and r['seed']==seed);isolate=next(r for r in records if r['arm']=='movement_only' and r['seed']==seed)
            # First trajectories/rollout start identically; only actor stopping coefficient differs.
            for u in [0]:
                first=torch.load(a/f'pilot/joint/seed-{seed}/checkpoint-{u:04}.pt',weights_only=True);second=torch.load(a/f'pilot/movement_only/seed-{seed}/checkpoint-{u:04}.pt',weights_only=True)
                assert equal_tree({k:v for k,v in first.items() if k!='contract'},{k:v for k,v in second.items() if k!='contract'})
            assert equal_tree(torch.load(a/f'pilot/joint/seed-{seed}/rollout-0001.pt',weights_only=True),torch.load(a/f'pilot/movement_only/seed-{seed}/rollout-0001.pt',weights_only=True))
            comparisons.append({'seed':seed,'tracking_gain_over_joint':isolate['gate']['tracking_retained']-joint['gate']['tracking_retained'],
                                'isolated_joint_gate':isolate['gate']['passed'],'isolated_movement_learning':isolate['gate']['movement_learning_passed']})
        qualified={arm:sum(r['gate']['passed'] for r in records if r['arm']==arm) for arm in aligned.ARMS}
        learned={arm:sum(r['gate']['movement_learning_passed'] for r in records if r['arm']==arm) for arm in aligned.ARMS}
        result={'experiment_id':a.name,'records':records,'paired_comparisons':comparisons,'qualified_seeds':qualified,'movement_learning_seeds':learned,
            'passed':qualified['movement_only']>=2,'isolation_improvement':sum(r['tracking_gain_over_joint']>=4 for r in comparisons)>=2,
            'training_ticks':811008,'main_training_ticks':786432,'main_movement_ticks':393216,'main_stopping_ticks':393216,
            'optimizer_steps':pre['optimizer_steps']+sum(r['optimizer_steps']+r['repeated_optimizer_steps'] for r in records),
            'restore_ticks':pre['restore_ticks']+sum(r['restore_ticks'] for r in records),'evaluation_ticks':evaluated,'evaluation_episodes':1248,
            'native_python_comparisons':1248,'likelihood_comparisons':798720,'fixed_steering_checkpoints':804,
            'neutral_setup_ticks':pre['neutral_setups']+sum(r['neutral_setups'] for r in records)+1248,'exact_resumes':12,
            'declared_parent_initializations':12,'paired_initial_traces_exact':312,'paired_initial_rollouts_exact':3,'parent_initial_states_exact':336,
            'confirmation_used':False,'model_promoted':False,'navigation_qualified':False,'seconds':time.monotonic()-start}
        assert result['optimizer_steps']<=12672 and result['restore_ticks']<=14760 and evaluated<=1123200;target='results.json'
    assert result['seconds']<300;atomic_json(a/target,result)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--preflight',action='store_true');args=p.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True);verify(args.archive,args.preflight)
