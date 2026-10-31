"""LF-063 strict evaluation and original-control reproduction."""
import argparse
from pathlib import Path
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from tools.flight_ppo.bridge import Bridge
from tools.flight_audit.motor_feedback import alter,acceleration
from tools.flight_audit.motor_retention import parity
from .sustained_audit import read,load,verify_run,preflight_checks,inspect,task_matches,gate
from . import movement


def verify_control(a,run,seed):
    reference=a/f'inputs/control/seed-{seed}'
    for update in [0,1,16,64,128]:
        current=load(run/f'checkpoint-{update:04}.pt.gz');old=load(reference/f'checkpoint-{update:04}.pt.gz')
        assert equal_tree({k:v for k,v in current.items() if k!='contract'},{k:v for k,v in old.items() if k!='contract'})
    for update in [1,128]:assert equal_tree(load(run/f'rollout-{update:04}.pt.gz'),load(reference/f'rollout-{update:04}.pt.gz'))
    for update in [0,128]:assert read(run/f'policy-{update:04}.json')==read(reference/f'policy-{update:04}.json')


def feedback(a,stage=movement):
    inputs=[];metadata=[]
    for anchor in read(a/'inputs/feedback-anchors.json'):
        x=torch.tensor([anchor['inputs']],dtype=torch.float32)
        for command in [6,12,18]:
            for error in [-6,-.25,0,.25,6]:
                inputs.append(alter(x,[command],[command+error])[0]);metadata.append({'anchor':anchor['index'],'command':command,'error':error})
    x=torch.stack(inputs);records=[];initial={}
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        for arm in stage.ARMS:
            for seed in stage.SEEDS:
                c=stage.config(seed,arm);run=a/f'pilot/{arm}/seed-{seed}'
                for phase,update in [('initial',0),('final',c['updates'])]:
                    state=load(run/f'checkpoint-{update:04}.pt.gz')
                    with torch.random.fork_rng(devices=[]):model=stage.make_model(state['contract']['schema'],c)
                    model.load_state_dict(state['model']);model.requires_grad_(False)
                    bridge.call('load_policy',path=str((run/f'policy-{update:04}.json').resolve()))
                    checks=[parity(bridge,model,part.tolist()) for part in x.split(256)]
                    with torch.no_grad():raw=model(x)[0];actions=raw[:,1:].sigmoid();force=acceleration(x,actions)
                    if phase=='initial':
                        if arm==stage.ARMS[0]:initial[seed]=raw.clone()
                        else:assert torch.equal(raw,initial[seed])
                    slopes=[]
                    for index in range(0,len(x),5):
                        slope=(actions[index+3]-actions[index+1])/.5
                        slopes.append({'anchor':metadata[index]['anchor'],'command':metadata[index]['command'],
                            'throttle_per_speed':float(slope[0]),'brake_per_speed':float(slope[1]),
                            'acceleration_at_target':float(force[index+2]),'acceleration_at_overspeed6':float(force[index+4])})
                    records.append({'arm':arm,'seed':seed,'phase':phase,'actions':actions.tolist(),'logits':raw[:,1:].tolist(),
                                    'acceleration':force.tolist(),'slopes':slopes,'parity':checks})
    atomic_json(a/'feedback.json',{'metadata':metadata,'inputs':x.tolist(),'records':records,'native_python_comparisons':len(x)*12,
        'paired_initial_outputs_exact':len(x)*3,'new_physics_ticks':0,'optimizer_steps':0})
    assert len(x)*12==9720


def verify(a,preflight,failure_policy='record-physical-failures-v1'):
    assert failure_policy==read(a/'experiment-plan.json').get('training_failure_policy','strict-v1')
    start=time.monotonic();records=[];evaluated=0;parent_exact=zero_retained=0
    for arm in movement.ARMS:
        for seed in movement.SEEDS:
            r=verify_run(a,arm,seed,preflight,stage=movement);records.append(r)
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
                    if phase=='initial' and arm=='movement':assert trace==read(a/f"pilot/residual/seed-{seed}/evaluation-initial/case-{candidate['index']:02}.json.gz")
                    if phase=='final' and candidate['task']['speed']==0:
                        assert trace==read(run/f"evaluation-initial/case-{candidate['index']:02}.json.gz");zero_retained+=1
                    phases[phase].append({**candidate,**inspection});evaluated+=inspection['ticks']
            r['gate']=gate(phases['initial'],phases['final'],parent);r['trajectories']=phases
            if arm=='residual':verify_control(a,run,seed)
    for seed in movement.SEEDS:
        phase='preflight' if preflight else 'pilot'
        assert equal_tree(load(a/f'{phase}/residual/seed-{seed}/rollout-0001.pt.gz'),load(a/f'{phase}/movement/seed-{seed}/rollout-0001.pt.gz'))
    if preflight:
        result={'passed':True,'records':records,'training_ticks':30720,'optimizer_steps':sum(r['optimizer_steps']+r['repeated_optimizer_steps'] for r in records),
                'restore_ticks':sum(r['restore_ticks'] for r in records),'neutral_setups':sum(r['neutral_setups'] for r in records),'exact_resumes':6,'paired_first_rollouts_exact':3,'seconds':time.monotonic()-start}
        assert result['optimizer_steps']<=480 and result['restore_ticks']<=11070;target='preflight-results.json'
    else:
        pre=read(a/'preflight-results.json');assert pre['passed'] and pre['training_failure_policy']==failure_policy;comparisons=[]
        for seed in movement.SEEDS:
            one=next(r for r in records if r['arm']=='residual' and r['seed']==seed);two=next(r for r in records if r['arm']=='movement' and r['seed']==seed)
            first=load(a/f'pilot/residual/seed-{seed}/checkpoint-0000.pt.gz');second=load(a/f'pilot/movement/seed-{seed}/checkpoint-0000.pt.gz')
            for k in ['update','history','torch_rng','python_rng']:assert equal_tree(first[k],second[k])
            assert first['optimizer']['state']==second['optimizer']['state']=={}
            comparisons.append({'seed':seed,'tracking_gain_over_residual':two['gate']['tracking_retained']-one['gate']['tracking_retained'],
                                'movement_joint_gate':two['gate']['passed'],'movement_learning':two['gate']['movement_learning_passed']})
        qualified={arm:sum(r['gate']['passed'] for r in records if r['arm']==arm) for arm in movement.ARMS};learned={arm:sum(r['gate']['movement_learning_passed'] for r in records if r['arm']==arm) for arm in movement.ARMS}
        assert parent_exact==672 and zero_retained==480
        feedback(a)
        result={'experiment_id':a.name,'records':records,'paired_comparisons':comparisons,'qualified_seeds':qualified,'movement_learning_seeds':learned,
            'passed':qualified['movement']>=2,'movement_improvement':sum(r['tracking_gain_over_residual']>=4 for r in comparisons)>=2,
            'training_ticks':823296,'main_training_ticks':786432,'main_movement_ticks':393216,'main_stopping_ticks':393216,
            'optimizer_steps':pre['optimizer_steps']+sum(r['optimizer_steps']+r['repeated_optimizer_steps'] for r in records),
            'restore_ticks':pre['restore_ticks']+sum(r['restore_ticks'] for r in records),'evaluation_ticks':evaluated,'evaluation_episodes':1248,
            'native_python_comparisons':10968,'evaluation_inference_comparisons':1248,'feedback_inference_comparisons':9720,'paired_initial_feedback_outputs_exact':2430,
            'likelihood_comparisons':811008,'protected_stopping_rows':405504,'frozen_parent_checkpoints':816,
            'neutral_setup_ticks':pre['neutral_setups']+sum(r['neutral_setups'] for r in records)+1248,'exact_resumes':12,
            'declared_parent_initializations':12,'paired_first_rollouts_exact':6,'original_control_checkpoints_exact':15,'original_control_rollouts_exact':6,'original_control_exports_exact':6,'paired_initial_traces_exact':312,'original_parent_traces_exact':672,'zero_command_initial_final_traces_exact':480,
            'confirmation_used':False,'model_promoted':False,'navigation_qualified':False,'seconds':time.monotonic()-start}
        assert result['optimizer_steps']<=12864 and result['restore_ticks']<=24000 and evaluated<=1123200;target='results.json'
    result['training_failure_policy']=failure_policy
    assert result['seconds']<300;atomic_json(a/target,result)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--preflight',action='store_true');args=p.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True);verify(args.archive,args.preflight)
