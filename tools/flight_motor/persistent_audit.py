"""LF-057 preflight and paired training evidence, likelihoods and physical gates."""
import argparse
from collections import Counter
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from .model import MotorModel
from .audit import inspect as training_inspect, read, HEADING, SPIN
from .velocity_audit import inspect, gate, task_matches
from .velocity import candidates, reward
from .task import SEEDS
from . import persistent


def action_stats(parts):
    samples=torch.cat(parts);out={}
    for label,selected in [('movement',samples[:,0]>0),('stopping',samples[:,0]==0)]:
        x=samples[selected];n=len(x)
        out[label]={'ticks':n,'thrust_mean':float(x[:,1].mean()),'brake_mean':float(x[:,2].mean()),
                    'thrust_above_point2':int((x[:,1]>.2).sum()),'thrust_above_point5':int((x[:,1]>.5).sum()),
                    'brake_below_point1':int((x[:,2]<.1).sum()),
                    'strong_thrust_released_brake':int(((x[:,1]>.5)&(x[:,2]<.1)).sum())}
    return out


def verify_run(a,arm,seed,preflight):
    phase='preflight' if preflight else 'pilot';again_phase='preflight-resumed' if preflight else 'resumed'
    run=a/f'{phase}/{arm}/seed-{seed}';resumed=a/f'{again_phase}/{arm}/seed-{seed}'
    c=persistent.config(seed,arm,preflight);end=c['updates'];ticks=end*1024
    summary=read(run/'summary.json');assert summary['completed_updates']==end and summary['environment_steps']==ticks
    assert summary['parameter_count']==3463 and summary['total_seconds']<c['wall_seconds']
    final=torch.load(run/f'checkpoint-{end:04}.pt',weights_only=True)
    again=torch.load(resumed/f'checkpoint-{end:04}.pt',weights_only=True);assert equal_tree(final,again)
    assert equal_tree(torch.load(run/f'rollout-{end:04}.pt',weights_only=True),torch.load(resumed/f'rollout-{end:04}.pt',weights_only=True))
    zero=torch.load(run/'checkpoint-0000.pt',weights_only=True);parent=torch.load(a/f'inputs/seed-{seed}/checkpoint-0064.pt',weights_only=True)
    assert equal_tree(zero['model'],parent['model']) and zero['optimizer']['state']=={} and zero['update']==0 and zero['history']==[]
    assert zero['collector']['steps']==0 and not equal_tree(final['model'],zero['model'])
    assert read(run/'policy-0000.json')==read(a/f'inputs/seed-{seed}/policy-0064.json')
    assert final['contract']['config']==c
    assert read(run/'training-candidates.json')==persistent.training_candidates(c)==read(a/f"tasks/{'preflight' if preflight else 'training'}.json")
    assert read(run/'development-candidates.json')==candidates(c['development_task_seed'],True)==read(a/'tasks/development.json')
    state=final['collector'];coverage=Counter(e['candidate']['index'] for e in state['completed'])
    assert set(coverage)==set(r['index'] for r in persistent.training_candidates(c)) and min(coverage.values()) >= (1 if preflight else 3)
    total=0;outcomes=Counter();peaks={'track':0.,'brake':0.,'turn':0.,'hold':0.};heading_good=heading_total=0
    for episode in state['completed']:
        trace=read(run/f"training-traces/episode-{episode['id']:06}.json");r=training_inspect(trace,reward)
        assert r['ticks']==episode['ticks'] and r['outcome']==episode['outcome'] and abs(r['reward']-episode['reward'])<1e-9
        outcomes[episode['outcome']]+=1;total+=r['ticks']
        family=episode['candidate']['family']
        peaks[family]=max(peaks[family],max((step['item']['frame']['velocity']['x']**2+step['item']['frame']['velocity']['y']**2)**.5 for step in trace['steps']))
        if family=='track':
            heading_total+=len(trace['steps']);heading_good+=sum(s['item']['errors']['heading']<=HEADING and s['item']['errors']['spin']<=SPIN for s in trace['steps'])
    total+=sum(training_inspect(t,reward)['ticks'] for t in state['traces']);assert total==ticks
    previous=torch.load(run/f'checkpoint-{end-1:04}.pt',weights_only=True)
    restore_ticks=sum(len(t['steps']) for t in previous['collector']['traces'])
    setups=state['started']+4+state['started']-previous['collector']['started']
    # Recompute every behavior likelihood at its original pre-update weights.
    with torch.random.fork_rng(devices=[]):model=MotorModel(final['contract']['schema']);model.requires_grad_(False)
    parts=[];max_error=0.
    for update in range(1,end+1):
        before=torch.load(run/f'checkpoint-{update-1:04}.pt',weights_only=True);model.load_state_dict(before['model'])
        rollout=torch.load(run/f'rollout-{update:04}.pt',weights_only=True);flat={k:v.flatten(0,1) for k,v in rollout.items()}
        with torch.no_grad():
            actor=model(flat['observations'])[0]
            if arm=='persistent':
                previous_actor=model(flat['ar_previous_observations'])[0]
                dist=persistent.distribution(model,actor,previous_actor,flat['ar_previous_latents'],flat['ar_previous_valid'],flat['observations'],c['motor_exploration'])
                logp=persistent.log_prob(model,dist,flat['ar_latents'])
                initial=before['collector']['exploration'];persistent.validate_state(initial,before['collector']['active'])
                for name in ['previous_observations','previous_latents','previous_valid']:
                    assert torch.equal(rollout['ar_'+name][0],initial[name])
                valid=rollout['ar_previous_valid'][1:,:,None]
                assert torch.equal(rollout['ar_previous_observations'][1:],rollout['observations'][:-1]*valid)
                assert torch.equal(rollout['ar_previous_latents'][1:],rollout['ar_latents'][:-1]*valid)
                assert torch.equal(flat['ar_latents'],torch.cat((flat['latent'][:,None],flat['actuator_latents']),1))
            else:
                assert not any(k.startswith('ar_') for k in flat) and 'exploration' not in before['collector']
                logp=model.log_prob(actor,flat['latent'],flat['actuator_latents'])
        torch.testing.assert_close(logp,flat['logp'],atol=1e-4,rtol=1e-5)
        max_error=max(max_error,float((logp-flat['logp']).abs().max()))
        parts.append(torch.cat((flat['observations'][:,2:3],flat['actuator_latents'].sigmoid()),1))
    if arm=='persistent':persistent.validate_state(state['exploration'],state['active'])
    historical=False
    if not preflight and arm=='control':
        old=torch.load(a/f'inputs/seed-{seed}/historical-checkpoint-0128.pt',weights_only=True)
        assert equal_tree({k:v for k,v in final.items() if k!='contract'},{k:v for k,v in old.items() if k!='contract'})
        assert read(run/'policy-0128.json')==read(a/f'inputs/seed-{seed}/historical-policy-0128.json');historical=True
    result={'arm':arm,'seed':seed,'training_coverage':dict(sorted(coverage.items())),'training_outcomes':dict(outcomes),
            'training_peak_speeds':peaks,'completed_tracking_heading_endpoint_fraction':heading_good/heading_total,
            'action_statistics':action_stats(parts),'likelihood_comparisons':ticks,'max_log_probability_error':max_error,
            'restore_ticks':restore_ticks,'neutral_setups':setups,
            'optimizer_steps':sum(h['minibatches'] for h in summary['history']),
            'repeated_optimizer_steps':summary['history'][-1]['minibatches'],
            'kl_stops':sum(h['kl_early_stop'] for h in summary['history']),'main_seconds':summary['total_seconds'],
            'historical_control_exact_except_contract':historical}
    return result


def preflight(a):
    start=time.monotonic();records=[]
    for arm in persistent.ARMS:
        for seed in SEEDS:
            r=verify_run(a,arm,seed,True);records.append(r)
            assert all(k in ['success','deadline'] for k in r['training_outcomes'])
            if arm=='persistent':
                movement=r['action_statistics']['movement'];assert movement['thrust_above_point2']/movement['ticks']>=.01
                assert movement['brake_below_point1']/movement['ticks']>=.01
    result={'passed':True,'records':records,'training_ticks':18432,'exact_resumes':6,'parent_initializations_exact':6,
            'likelihood_comparisons':12288,'restore_ticks':sum(r['restore_ticks'] for r in records),
            'neutral_setups':sum(r['neutral_setups'] for r in records),
            'optimizer_steps':sum(r['optimizer_steps']+r['repeated_optimizer_steps'] for r in records),'seconds':time.monotonic()-start}
    assert result['restore_ticks']<=6144 and result['optimizer_steps']<=288 and result['seconds']<300
    atomic_json(a/'preflight-results.json',result)


def verify(a):
    start=time.monotonic();pre=read(a/'preflight-results.json');assert pre['passed'];records=[];evaluation_ticks=0
    for arm in persistent.ARMS:
        for seed in SEEDS:
            r=verify_run(a,arm,seed,False);run=a/f'pilot/{arm}/seed-{seed}';phases={}
            for phase in ['initial','final']:
                directory=run/f'evaluation-{phase}';rows=read(directory/'results.json');assert len(rows)==104
                assert len(read(directory/'parity.json'))==104;phases[phase]=[]
                for row,candidate in zip(rows,candidates(5510002,True)):
                    assert row['index']==candidate['index'];trace=read(directory/f"case-{row['index']:02}.json")
                    assert task_matches(trace['task'],candidate['task']);inspection=inspect(trace)
                    assert all(row[k]==inspection[k] for k in ['ticks','outcome','final_errors'])
                    if phase=='initial':
                        if 'parent_index' in candidate:assert trace==read(a/f"inputs/seed-{seed}/reference/case-{candidate['parent_index']:02}.json")
                        if arm=='persistent':assert trace==read(a/f"pilot/control/seed-{seed}/evaluation-initial/case-{candidate['index']:02}.json")
                    phases[phase].append({**candidate,**inspection});evaluation_ticks+=inspection['ticks']
            r['gate']=gate(phases['initial'],phases['final']);r['trajectories']=phases;records.append(r)
    qualified={arm:sum(r['gate']['passed'] for r in records if r['arm']==arm) for arm in persistent.ARMS}
    comparisons=[]
    for seed in SEEDS:
        control=next(r for r in records if r['arm']=='control' and r['seed']==seed)
        experiment=next(r for r in records if r['arm']=='persistent' and r['seed']==seed)
        comparisons.append({'seed':seed,'tracking_gain_over_control':experiment['gate']['tracking_retained']-control['gate']['tracking_retained'],
                            'braking_gain_over_control':experiment['gate']['braking_retained']-control['gate']['braking_retained'],
                            'experimental_joint_gate':experiment['gate']['passed']})
    result={'experiment_id':a.name,'records':records,'paired_comparisons':comparisons,'qualified_seeds':qualified,
            'passed':qualified['persistent']>=2,'training_ticks':811008,'main_training_ticks':786432,'main_restart_ticks':6144,
            'preflight_training_ticks':pre['training_ticks'],'optimizer_steps':pre['optimizer_steps']+sum(r['optimizer_steps']+r['repeated_optimizer_steps'] for r in records),
            'restore_ticks':pre['restore_ticks']+sum(r['restore_ticks'] for r in records),'evaluation_ticks':evaluation_ticks,
            'evaluation_episodes':1248,'native_python_comparisons':1248,'likelihood_comparisons':798720,
            'neutral_setup_ticks':pre['neutral_setups']+sum(r['neutral_setups'] for r in records)+1248,
            'exact_resumes':12,'historical_controls_exact_except_contract':3,'exact_parent_initializations':12,'exact_parent_retention_traces':192,
            'paired_initial_traces_exact':312,'confirmation_used':False,'model_promoted':False,'navigation_qualified':False,'seconds':time.monotonic()-start}
    assert result['optimizer_steps']<=12672 and result['restore_ticks']<=14760 and evaluation_ticks<=1123200 and result['seconds']<300
    atomic_json(a/'results.json',result)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--preflight',action='store_true');args=p.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.preflight:preflight(args.archive)
    else:verify(args.archive)
