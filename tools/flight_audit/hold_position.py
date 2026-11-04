"""Frozen zero-error hold task: physical gates, complete-interval evidence and parity."""
import argparse
from collections import Counter
import copy
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.curriculum import HOLD_RECIPE
from tools.flight_ppo.execution import DETERMINISTIC, load_export
from tools.flight_ppo.model import actions, tensor
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.continuous_approach import CRITERIA, read, trace_metrics, combine, training_inspection

SEEDS=[17,29,43]
RECIPES=[f'{HOLD_RECIPE}/stratum-{i}' for i in range(4)]


def physical(rows):
    if len(rows)!=16 or len({r['index'] for r in rows})!=16:raise ValueError('sixteen unique hold cases required')
    if any(sum(r['recipe']==name for r in rows)!=4 for name in RECIPES):raise ValueError('hold heading coverage required')
    for r in rows:
        if r['outcome'] not in ['success','deadline','collision','damage','ship_lost','boundary_escape']:raise ValueError('invalid physical outcome')
        if r['outcome']=='success' and r['ticks']<30:raise ValueError('incomplete hold interval')
    successes=sum(r['outcome']=='success' for r in rows)
    immediate=[r for r in rows if r['outcome']=='success' and r['ticks']==30]
    return {'attempts':16,'successes':successes,'immediate_holds':len(immediate),
            'immediate_holds_by_stratum':[sum(r['recipe']==name for r in immediate) for name in RECIPES],
            'outcomes':dict(Counter(r['outcome'] for r in rows)),
            'physical_failures':sum(r['outcome'] not in ['success','deadline'] or r['collision'] or r['damage'] for r in rows)}


def comparison(initial,final):
    before,after=physical(initial),physical(final)
    keyed=[{r['index']:r for r in rows} for rows in [initial,final]]
    if keyed[0].keys()!=keyed[1].keys():raise ValueError('different tasks')
    for index,old in keyed[0].items():
        new=keyed[1][index]
        if any(old[k]!=new[k] for k in ['recipe','initial_errors']):raise ValueError('different starts')
    gain=after['successes']-before['successes'];hold_gain=after['immediate_holds']-before['immediate_holds']
    return {'initial':before,'final':after,'arrival_gain':gain,'immediate_hold_gain':hold_gain,
            'learning_evidence':after['successes']>=4 and gain>=4,
            'scoped_hold':after['immediate_holds']>=12 and hold_gain>=8 and min(after['immediate_holds_by_stratum'])>=2 and after['physical_failures']==0}


def aggregate(records):
    if sorted(r['seed'] for r in records)!=SEEDS:raise ValueError('three declared seeds required')
    return {key:sum(r['mean'][key] for r in records)>=2 for key in ['learning_evidence','scoped_hold']}


def hold_metrics(trace, allow_active=False):
    task=trace['task'];start=task['start']
    assert task['goal_offset']==task['goal_velocity']==start['relative_velocity']=={'x':0.,'y':0.}
    assert start['spin']==0 and not start['gravity'] and start['planet_spin']==0
    assert not task['obstacles'] and not task['terrain_edits']
    assert trace['initial']['errors']=={'position':0.,'velocity':0.,'spin':0.}
    metrics=trace_metrics(trace,allow_active)
    first=None
    for tick,s in enumerate(trace['steps'],1):
        e=s['frame']['errors'];failed=[k for k in ['position','velocity','spin'] if abs(e[k])>CRITERIA[k]]
        if failed:first={'tick':tick,'violations':failed,'errors':e};break
    metrics['first_tolerance_exit']=first
    metrics['immediate_hold']=metrics['outcome']=='success' and metrics['ticks']==30
    return metrics


def inspect_evaluation(directory):
    rows=read(directory/'results.json');physical(rows)
    assert read(directory/'metadata/execution.json')['specification']=={'mode':DETERMINISTIC,'repeats':1}
    traces=[]
    for row in rows:
        trace=read(directory/f"clear-{row['index']}.json");metrics=hold_metrics(trace)
        assert metrics['outcome']==row['outcome'] and metrics['ticks']==row['ticks']
        assert metrics['final_errors']==row['final_errors'] and trace['initial']['errors']==row['initial_errors']
        for flag in ['collision','damage']:assert row[flag]==any(s['evidence'][flag] for s in trace['steps'])
        traces.append(dict(metrics,index=row['index'],recipe=row['recipe']))
    return {'summary':combine(traces),'episodes':traces,'immediate_holds':sum(t['immediate_hold'] for t in traces)}


def parity(a,run,update,candidates,destination):
    with Bridge(a/'runtime/flight_ppo_bridge') as bridge,torch.random.fork_rng(devices=[]):
        model=load_export(run/f'policy-{update:04}.json',bridge.contract['schema'])
        state=torch.load(run/f'checkpoint-{update:04}.pt',weights_only=True)
        assert state['contract']['config']['input_scaling']=='schema-v1' and equal_tree(model.state_dict(),state['model'])
        weights=copy.deepcopy(model.state_dict());rng=torch.get_rng_state().clone()
        items=bridge.call('reset',tasks=[c['task'] for c in candidates])['items']
        for item in items:assert item['frame']['errors']=={'position':0.,'velocity':0.,'spin':0.} and item['progress']=={'elapsed_ticks':0,'dwell_ticks':0,'outcome':None}
        bridge.call('load_policy',path=str((run/f'policy-{update:04}.json').resolve()))
        native=bridge.call('infer',inputs=[i['inputs'] for i in items])['outputs']
        with torch.no_grad():actor,value=model(tensor(items))
        expected=torch.cat((actor,value[:,None]),dim=1)
        actual=torch.tensor([[n[k] for k in ['steering_latent','thrust_logit','brake_logit','value']] for n in native])
        torch.testing.assert_close(actual,expected,atol=1e-5,rtol=1e-5)
        commands=actions(actor[:,0],actor[:,1:]);error=max(abs(c[k]-n['action'][k]) for c,n in zip(commands,native) for k in c)
        assert error<=1e-5 and equal_tree(weights,model.state_dict()) and torch.equal(rng,torch.get_rng_state())
        result={'fixtures':len(items),'raw_error':float((actual-expected).abs().max()),'action_error':error}
        atomic_json(destination,{'items':items,'python_raw':expected.tolist(),'python_actions':commands,'native':native,'summary':result})
        return result


def preflight(a):
    run=a/'preflight/original';resumed=a/'preflight/resumed'
    state=torch.load(run/'checkpoint-0002.pt',weights_only=True)
    assert equal_tree(state,torch.load(resumed/'checkpoint-0002.pt',weights_only=True))
    admission=read(run/'training-admission.json');candidates=[v['candidate'] for v in admission['attempts'] if v['admitted']][:16]
    assert len(candidates)==16
    p=parity(a,run,2,candidates,a/'preflight/parity.json')
    inspection=training_inspection(run,2)
    for trace in state['collector']['traces']:
        if trace['steps']:hold_metrics(trace,True)
    atomic_json(a/'preflight/verification.json',{'exact_resume':True,'parity':p,'training':inspection,'total_training_ticks':3072,'maximum_optimizer_steps':48})


def report(a):
    start=time.monotonic();records=[];train_admissions=[];dev_admissions=[]
    for seed in SEEDS:
        run=a/f'seed-{seed}';s=read(run/'summary.json')
        assert s['completed_updates']==128 and s['environment_steps']==131072 and s['total_seconds']<=1800
        state=torch.load(run/'checkpoint-0128.pt',weights_only=True);assert state['history']==s['history']
        for trace in state['collector']['traces']:
            if trace['steps']:hold_metrics(trace,True)
        for path in sorted((run/'training-traces').glob('episode-*.json')):hold_metrics(read(path))
        train_admissions.append(read(run/'training-admission.json'));dev_admissions.append(read(run/'development-admission.json'))
        candidates=[v['candidate'] for v in dev_admissions[-1]['attempts'] if v['admitted']]
        assert len(candidates)==16 and len({c['index'] for c in candidates})==16
        evaluation={phase:inspect_evaluation(run/f'evaluation-{phase}') for phase in ['initial','final']}
        parities={phase:parity(a,run,n,candidates,run/f'parity-{phase}.json') for phase,n in [('initial',0),('final',128)]}
        records.append({'seed':seed,'mean':comparison(s['initial_evaluation'],s['final_evaluation']),
                        'evaluation':evaluation,'training':training_inspection(run,128),'parity':parities,
                        'training_seconds':s['training_seconds_including_checkpoints'],'total_seconds':s['total_seconds'],
                        'parameter_count':s['parameter_count'],'optimizer_steps':sum(h['minibatches'] for h in s['history'])})
        if time.monotonic()-start>300:raise TimeoutError('hold report budget')
    assert all(v==train_admissions[0] for v in train_admissions) and all(v==dev_admissions[0] for v in dev_admissions)
    for n in [0,2]:assert equal_tree(torch.load(a/f'preflight/original/checkpoint-{n:04}.pt',weights_only=True),torch.load(a/f'seed-17/checkpoint-{n:04}.pt',weights_only=True))
    statuses=read(a/'validation/process-status.json');assert all(v['exit_code']==0 for v in statuses)
    r={'experiment_id':a.name,'records':records,'gates':aggregate(records),'training_ticks':sum(v['training']['ticks'] for v in records),
       'evaluation_ticks':sum(v['evaluation'][phase]['summary']['ticks'] for v in records for phase in ['initial','final']),
       'preflight_training_ticks':3072,'parity_fixtures':96,'preflight_parity_fixtures':16,'parity_setup_ticks':112,
       'report_seconds':time.monotonic()-start,'matched_admissions':True,'exact_resume_and_preflight_main':True,
       'confirmation_used':False,'model_promoted':False,'approach_or_navigation_claim':False}
    assert r['training_ticks']==393216 and r['evaluation_ticks']<=172800 and r['report_seconds']<=300
    atomic_json(a/'results.json',r)


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);parser.add_argument('--preflight',action='store_true');args=parser.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.preflight:preflight(args.archive)
    else:report(args.archive)


if __name__=='__main__':main()
