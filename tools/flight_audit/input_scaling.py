"""Bounded paired scaling experiment: preflight, fixed-context response and physical report."""
import argparse
import copy
import json
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.execution import load_export
from tools.flight_ppo.model import FlightModel, actions, flatten
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.continuous_approach import comparison, evaluation_inspection, training_inspection
from tools.flight_audit.goal_response import read, grid, summarize, group, forward_acceleration

ARMS=['schema','task'];SEEDS=[17,29,43]


def models(run, update, schema):
    state=torch.load(run/f'checkpoint-{update:04}.pt',weights_only=True)
    model=FlightModel(schema,state['contract']['config']['input_scaling'])
    model.load_state_dict(state['model']);model.eval();model.requires_grad_(False)
    folded=load_export(run/f'policy-{update:04}.json',schema)
    assert equal_tree(model.inference_state_dict(),folded.state_dict())
    return model,folded,state


def infer(bridge, model, folded, inputs):
    x=torch.tensor([flatten(v) for v in inputs])
    with torch.no_grad():
        actor,value=model(x);fa,fv=folded(x)
    expected=torch.cat((actor,value[:,None]),dim=1)
    folded_raw=torch.cat((fa,fv[:,None]),dim=1)
    native=bridge.call('infer',inputs=inputs)['outputs']
    actual=torch.tensor([[v[k] for k in ['steering_latent','thrust_logit','brake_logit','value']] for v in native])
    torch.testing.assert_close(actual,expected,atol=1e-5,rtol=1e-5)
    torch.testing.assert_close(folded_raw,expected,atol=1e-5,rtol=1e-5)
    commands=actions(actor[:,0],actor[:,1:])
    command_error=max(abs(c[k]-n['action'][k]) for c,n in zip(commands,native) for k in c)
    assert command_error<=1e-5
    return expected.tolist(),commands,native,{'raw_error':float((actual-expected).abs().max()),'fold_error':float((folded_raw-expected).abs().max()),'action_error':command_error}


def preflight(a):
    result={};initial=[]
    for arm in ARMS:
        run=a/f'preflight/{arm}';resumed=a/f'preflight/{arm}-resumed'
        state=torch.load(run/'checkpoint-0002.pt',weights_only=True)
        assert equal_tree(state,torch.load(resumed/'checkpoint-0002.pt',weights_only=True))
        initial.append(torch.load(run/'checkpoint-0000.pt',weights_only=True)['model'])
        with Bridge(a/'runtime/flight_ppo_bridge') as bridge,torch.random.fork_rng(devices=[]):
            model,folded,_=models(run,2,bridge.contract['schema'])
            bridge.call('load_policy',path=str((run/'policy-0002.json').resolve()))
            inputs=read(a/'inputs/anchors/seed-17/start-0/inputs.json.gz')
            raw,commands,native,parity=infer(bridge,model,folded,inputs)
            atomic_json(a/f'preflight/{arm}-parity.json',{'python_raw':raw,'python_actions':commands,'native':native,'errors':parity})
        result[arm]={'exact_resume':True,'parity':parity,'fixtures':201,'training':training_inspection(run,2)}
    assert equal_tree(initial[0],initial[1])
    atomic_json(a/'preflight/verification.json',{'arms':result,'matched_initial_parameter_draws':True,'training_ticks':6144,'optimizer_steps_maximum':96})


def report(a):
    started=time.monotonic();records=[];all_response=[];all_comparisons=0;admissions=[];training_admissions=[]
    cases=[{'kind':'original'}]+[{'kind':'intervention',**c} for c in grid()]
    for arm in ARMS:
        for seed in SEEDS:
            run=a/f'runs/{arm}/seed-{seed}';s=read(run/'summary.json')
            assert s['completed_updates']==128 and s['environment_steps']==131072 and s['total_seconds']<=1800
            state=torch.load(run/'checkpoint-0128.pt',weights_only=True)
            assert state['history']==s['history']
            if seed==17:
                for n in [0,2]:assert equal_tree(torch.load(a/f'preflight/{arm}/checkpoint-{n:04}.pt',weights_only=True),torch.load(run/f'checkpoint-{n:04}.pt',weights_only=True))
            admissions.append(read(run/'development-admission.json'));training_admissions.append(read(run/'training-admission.json'))
            mean=comparison(s['initial_evaluation'],s['final_evaluation'],seed)
            evaluation={phase:evaluation_inspection(run/f'evaluation-{phase}',seed,False) for phase in ['initial','final']}
            record={'arm':arm,'seed':seed,'mean':mean,'evaluation':evaluation,'training':training_inspection(run,128),
                    'training_seconds':s['training_seconds_including_checkpoints'],'total_seconds':s['total_seconds'],
                    'optimizer_steps':sum(h['minibatches'] for h in s['history']),'parameter_count':s['parameter_count']}
            if arm=='schema':
                parent=torch.load(a/f'inputs/parent-checkpoint-{seed}.pt',weights_only=True)
                record['reproduces_parent_parameters_optimizer_rng']=all(equal_tree(state[k],parent[k]) for k in ['model','optimizer','torch_rng','python_rng'])
            with Bridge(a/'runtime/flight_ppo_bridge') as bridge,torch.random.fork_rng(devices=[]):
                response={}
                for phase,update in [('initial',0),('final',128)]:
                    model,folded,_=models(run,update,bridge.contract['schema']);weights=copy.deepcopy(model.state_dict());rng=torch.get_rng_state().clone()
                    bridge.call('load_policy',path=str((run/f'policy-{update:04}.json').resolve()))
                    anchors=[]
                    for directory in sorted((a/f'inputs/anchors/seed-{seed}').iterdir()):
                        if time.monotonic()-started>900:raise TimeoutError('report/probe wall budget')
                        anchor=read(directory/'anchor.json');inputs=read(directory/'inputs.json.gz');assert len(inputs)==201
                        raw,commands,native,parity=infer(bridge,model,folded,inputs)
                        rows=[{'case':c,'python_raw':o,'python_action':cmd,'native':n,'forward_actuator_acceleration':forward_acceleration(x,cmd)} for c,x,o,cmd,n in zip(cases,inputs,raw,commands,native)]
                        out=a/f'response/{arm}/seed-{seed}/{phase}'/directory.name;out.mkdir(parents=True)
                        atomic_json(out/'outputs.json',rows)
                        rec={'arm':arm,'seed':seed,'phase':phase,'kind':anchor['kind'],'index':anchor['index'],'parity':parity,'summary':summarize(rows)}
                        atomic_json(out/'summary.json',rec);anchors.append(rec);all_response.append(rec);all_comparisons+=201
                        atomic_json(a/'response-progress.json',{'anchor_batches':len(all_response),'comparisons':all_comparisons})
                    response[phase]={kind:group([v for v in anchors if v['kind']==kind]) for kind in ['start','near']}
                    assert equal_tree(weights,model.state_dict()) and torch.equal(rng,torch.get_rng_state())
                record['response']=response
            records.append(record)
    assert all(v==admissions[0] for v in admissions) and all(v==training_admissions[0] for v in training_admissions)
    for seed in SEEDS:
        states=[torch.load(a/f'runs/{arm}/seed-{seed}/checkpoint-0000.pt',weights_only=True) for arm in ARMS]
        assert equal_tree(states[0]['model'],states[1]['model'])
    assert all_comparisons==74772 and len(all_response)==372
    exits=read(a/'validation/process-status.json');assert all(v['exit_code']==0 for v in exits)
    by_arm={arm:{'learning_evidence':sum(v['mean']['learning_evidence'] for v in records if v['arm']==arm)>=2,
                 'scoped_progression':sum(v['mean']['scoped_progression'] for v in records if v['arm']==arm)>=2} for arm in ARMS}
    paired=[]
    for seed in SEEDS:
        control=next(v for v in records if v['arm']=='schema' and v['seed']==seed);task=next(v for v in records if v['arm']=='task' and v['seed']==seed)
        paired.append({'seed':seed,'final_arrival_gain':task['mean']['final']['successes']-control['mean']['final']['successes'],
                       'initial_arrival_gain':task['mean']['initial']['successes']-control['mean']['initial']['successes']})
    r={'experiment_id':a.name,'records':records,'gates':by_arm,'paired':paired,
       'scaling_arrival_improvement':sum(v['final_arrival_gain']>=4 for v in paired)>=2,
       'training_ticks':sum(v['training']['ticks'] for v in records),'evaluation_ticks':sum(v['evaluation'][p]['summary']['ticks'] for v in records for p in ['initial','final']),
       'preflight_training_ticks':6144,'response_comparisons':all_comparisons,'response_anchor_batches':len(all_response),
       'maximum_native_raw_error':max(v['parity']['raw_error'] for v in all_response),
       'maximum_fold_error':max(v['parity']['fold_error'] for v in all_response),
       'maximum_action_error':max(v['parity']['action_error'] for v in all_response),
       'seconds':time.monotonic()-started,'all_processes_exit_zero':True,'matched_initial_parameters_and_admissions':True,
       'parameters_and_rng_unchanged_during_response':True,'confirmation_used':False,'model_promoted':False,'general_flight_claim':False}
    assert r['training_ticks']==786432 and r['evaluation_ticks']<=345600 and r['seconds']<=900
    atomic_json(a/'results.json',r)


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True)
    parser.add_argument('--preflight',action='store_true');args=parser.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.preflight:preflight(args.archive)
    else:report(args.archive)


if __name__=='__main__':main()
