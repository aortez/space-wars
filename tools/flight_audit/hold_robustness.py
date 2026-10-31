"""Frozen no-training hold-duration and perturbation audit of all three endpoints."""
import argparse
from collections import Counter
import copy
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from tools.flight_ppo.curriculum import hold_candidate
from tools.flight_ppo.evaluate import evaluate
from tools.flight_ppo.execution import DETERMINISTIC, load_export
from tools.flight_ppo.model import actions, tensor
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.continuous_approach import read

SEEDS = [17, 29, 43]
ANCHOR_SEED = 4410001
LIMITS = {'position': 3., 'velocity': 1., 'spin': 0.20000000298023224}
VARIANTS = [f'nominal-{n}' for n in [30, 120, 300]]
for scope in ['inside', 'recovery']:
    for field, axes in [('position', ['forward','right']), ('velocity',['forward','right']), ('spin',['angular'])]:
        for axis in axes:
            for sign in ['positive','negative']:
                VARIANTS.append(f'{scope}-{field}-{axis}-{sign}')


def candidate(anchor, variant):
    if anchor not in range(8) or variant not in VARIANTS:raise ValueError('unknown declared probe')
    c=hold_candidate(ANCHOR_SEED,anchor);task=c['task'];start=task['start']
    c['index']=VARIANTS.index(variant)*8+anchor
    c['recipe']=f'hold-robustness-v1/{variant}/stratum-{anchor%4}'
    if variant.startswith('nominal-'):
        horizon=int(variant.split('-')[1]);task['arrival']['dwell_ticks']=horizon;task['deadline_ticks']=horizon
    else:
        scope,field,axis,sign=variant.split('-');inside=scope=='inside'
        magnitude={'position':1.5 if inside else 4.,'velocity':.5 if inside else 1.5,'spin':.1 if inside else .3}[field]
        magnitude*=1 if sign=='positive' else -1
        task['arrival']['dwell_ticks']=300 if inside else 30;task['deadline_ticks']=300
        if field=='spin':start['spin']=magnitude
        else:
            angle=start['bearing']+start['heading_offset']
            x,y=(-math.sin(angle),math.cos(angle)) if axis=='forward' else (math.cos(angle),math.sin(angle))
            value={'x':magnitude*x,'y':magnitude*y}
            if field=='position':task['goal_offset']=value
            else:start['relative_velocity']=value
    return c


def assert_initial(item, c):
    e=item['frame']['errors'];task=c['task'];start=task['start']
    # The native neutral setup tick removes .025 rad/s of angular speed.
    expected={'position':math.hypot(*task['goal_offset'].values()),
              'velocity':math.hypot(*start['relative_velocity'].values()),'spin':max(abs(start['spin'])-.025,0.)}
    if any(abs(e[k]-v)>.002 for k,v in expected.items()):raise ValueError('setup did not preserve declared perturbation')
    frame=item['frame']
    if abs(frame['spin']-math.copysign(expected['spin'],start['spin']))>.002:raise ValueError('spin direction changed')
    for k in ['x','y']:
        if abs(frame['velocity'][k]-start['relative_velocity'][k])>.002:raise ValueError('velocity direction changed')
        if abs(frame['target_position'][k]-frame['center_of_mass'][k]-task['goal_offset'][k])>.002:raise ValueError('position direction changed')
    if item['progress']!={'elapsed_ticks':0,'dwell_ticks':0,'outcome':None}:raise ValueError('nonzero initial progress')
    return e


def inspect_trace(trace):
    task=trace['task'];criteria=task['arrival'];steps=trace['steps']
    if trace['version']!=2 or task.get('hold_failure') is not None:raise ValueError('wrong terminal contract')
    if any(criteria[k]!=v for k,v in LIMITS.items()):raise ValueError('changed physical tolerance')
    horizon=criteria['dwell_ticks'];deadline=task['deadline_ticks']
    if horizon not in [30,120,300] or deadline not in [30,120,300,1800] or not 0<len(steps)<=deadline:raise ValueError('wrong horizon/trace length')
    def qualified(frame):return all(abs(frame['errors'][k])<=v for k,v in LIMITS.items())
    previous=trace['initial'];dwell=0;first_exit=None if qualified(previous) else 0;first_entry=0 if qualified(previous) else None
    failures=[];flips=0;max_dwell=0
    for tick,s in enumerate(steps,1):
        frame=s['frame'];action=s['action'];evidence=s['evidence']
        if s.get('error') or not frame or not action:raise ValueError('invalid environment/controller state')
        if not all(math.isfinite(v) for v in frame['errors'].values()):raise ValueError('nonfinite error')
        if any(type(action[k]) not in [int,float] or not math.isfinite(action[k]) or not low<=action[k]<=1 for k,low in [('steering',-1),('thrust',0),('brake',0)]):raise ValueError('invalid actuator')
        valid=qualified(frame)
        if not valid and first_exit is None:first_exit=tick
        if valid and first_entry is None:first_entry=tick
        dwell=dwell+1 if qualified(previous) and valid else 0;max_dwell=max(max_dwell,dwell)
        physical=[k for k in ['ship_lost','collision','damage','boundary_escape'] if evidence[k]]
        outcome=physical[0] if physical else 'success' if dwell>=horizon else 'deadline' if tick==deadline else None
        if s['progress']!={'elapsed_ticks':tick,'dwell_ticks':dwell,'outcome':outcome}:raise ValueError('outcome/dwell reconstruction differs')
        if outcome is not None and tick!=len(steps):raise ValueError('advanced past terminal')
        if tick>1:flips+=action['steering']*steps[tick-2]['action']['steering']<0
        failures+=physical;previous=frame
    if outcome is None:raise ValueError('unfinished evaluation')
    return {'outcome':outcome,'ticks':len(steps),'horizon':horizon,'first_exit_tick':first_exit,'first_entry_tick':first_entry,
            'maximum_dwell':max_dwell,'physical_failure':bool(failures),'final_errors':steps[-1]['frame']['errors'],
            'maximum_errors':{k:max([trace['initial']['errors'][k]]+[s['frame']['errors'][k] for s in steps]) for k in LIMITS},
            'steering_sign_changes':flips,'controls':{k:{'mean':sum(s['action'][k] for s in steps)/len(steps),
                 'mean_abs':sum(abs(s['action'][k]) for s in steps)/len(steps)} for k in ['steering','thrust','brake']}}


def groups(episodes):
    if len(episodes)!=184 or len({e['index'] for e in episodes})!=184:raise ValueError('missing/duplicate probe')
    result={}
    for variant in VARIANTS:
        rows=[e for e in episodes if e['variant']==variant]
        if len(rows)!=8 or sorted(e['anchor'] for e in rows)!=list(range(8)):raise ValueError('missing anchor coverage')
        success=sum(e['outcome']=='success' for e in rows);fail=sum(e['physical_failure'] for e in rows)
        strata=[sum(e['outcome']=='success' and e['anchor']%4==i for e in rows) for i in range(4)]
        result[variant]={'attempts':8,'successes':success,'physical_failures':fail,'successes_by_stratum':strata,
                         'outcomes':dict(Counter(e['outcome'] for e in rows)),
                         'passes':success>=6 and min(strata)>=1 and fail==0}
    gates={'fresh_half_second':result['nominal-30']['passes'],
           'nominal_long_hold':all(result[f'nominal-{n}']['passes'] for n in [120,300]),
           'inside_perturbations':all(v['passes'] for k,v in result.items() if k.startswith('inside-')),
           'recovery':all(v['passes'] for k,v in result.items() if k.startswith('recovery-'))}
    return result,gates


def prepare(a):
    start=time.monotonic();base=[r['candidate'] for r in read(a/'inputs/development-admission.json')['attempts'] if r['admitted']]
    assert len(base)==16
    declared=[candidate(i,v) for v in VARIANTS for i in range(8)];reports=[]
    with Bridge(a/'runtime/flight_ppo_bridge') as bridge:
        for c in base+declared:
            report=bridge.call('inspect',candidate=c)
            if not report['admitted']:raise ValueError(f"admission failed {c['recipe']}: {report['rejection']}")
            reports.append(report)
    atomic_json(a/'admissions.json',{'baseline':reports[:16],'probes':reports[16:],'complete':True,
                                  'neutral_setup_ticks':200,'seconds':time.monotonic()-start})
    assert time.monotonic()-start<180


def parity(a,seed,fixtures,out):
    with Bridge(a/'runtime/flight_ppo_bridge') as bridge,torch.random.fork_rng(devices=[]):
        model=load_export(a/f'inputs/seed-{seed}/policy.json',bridge.contract['schema'])
        state=torch.load(a/f'inputs/seed-{seed}/checkpoint.pt',weights_only=True)
        assert equal_tree(model.state_dict(),state['model'])
        weights=copy.deepcopy(model.state_dict());rng=torch.get_rng_state().clone();batches=[];raw=0.;action_error=0.
        bridge.call('load_policy',path=str((a/f'inputs/seed-{seed}/policy.json').resolve()))
        for start in range(0,len(fixtures),12):
            cs=fixtures[start:start+12];items=bridge.call('reset',tasks=[c['task'] for c in cs])['items']
            for item,c in zip(items,cs):assert_initial(item,c)
            native=bridge.call('infer',inputs=[i['inputs'] for i in items])['outputs']
            with torch.no_grad():actor,value=model(tensor(items))
            expected=torch.cat((actor,value[:,None]),dim=1)
            actual=torch.tensor([[n[k] for k in ['steering_latent','thrust_logit','brake_logit','value']] for n in native])
            torch.testing.assert_close(actual,expected,atol=1e-5,rtol=1e-5)
            commands=actions(actor[:,0],actor[:,1:]);error=max(abs(c[k]-n['action'][k]) for c,n in zip(commands,native) for k in c)
            assert error<=1e-5;raw=max(raw,float((actual-expected).abs().max()));action_error=max(action_error,error)
            batches.append({'candidates':cs,'items':items,'python_raw':expected.tolist(),'python_actions':commands,'native':native})
        assert equal_tree(weights,model.state_dict()) and torch.equal(rng,torch.get_rng_state())
        result={'fixtures':len(fixtures),'maximum_raw_error':raw,'maximum_action_error':action_error}
        atomic_json(out,{'batches':batches,'summary':result});return result


def worker(a,seed):
    start=time.monotonic();deadline=start+1800;out=a/f'seed-{seed}';out.mkdir()
    admissions=read(a/'admissions.json');base=[r['candidate'] for r in admissions['baseline']];probes=[r['candidate'] for r in admissions['probes']]
    policy=a/f'inputs/seed-{seed}/policy.json';before=sha(policy);checkpoint_hash=sha(a/f'inputs/seed-{seed}/checkpoint.pt')
    p=parity(a,seed,[base[0]]+[probes[8*j] for j in range(23)],out/'parity.json')
    baseline=evaluate(a/'runtime/flight_ppo_bridge',policy,base,out/'baseline',deadline)
    baseline_metrics=[]
    for row in baseline:
        path=out/f"baseline/clear-{row['index']}.json";trace=read(path)
        assert trace==read(a/f"inputs/seed-{seed}/baseline/clear-{row['index']}.json"), 'baseline replay changed'
        m=inspect_trace(trace);assert m['outcome']=='success' and m['ticks']==30;baseline_metrics.append(m)
    rows=evaluate(a/'runtime/flight_ppo_bridge',policy,probes,out/'probes',deadline);episodes=[]
    for c,row in zip(probes,rows):
        assert row['index']==c['index'] and row['recipe']==c['recipe']
        trace=read(out/f"probes/clear-{row['index']}.json")
        assert_initial({'frame':trace['initial'],'progress':{'elapsed_ticks':0,'dwell_ticks':0,'outcome':None}},c)
        m=inspect_trace(trace);assert m['outcome']==row['outcome'] and m['ticks']==row['ticks'] and m['final_errors']==row['final_errors']
        for k in ['collision','damage']:assert row[k]==any(s['evidence'][k] for s in trace['steps'])
        episodes.append(dict(m,index=c['index'],variant=VARIANTS[c['index']//8],anchor=c['index']%8))
    group,gates=groups(episodes)
    assert before==sha(policy) and checkpoint_hash==sha(a/f'inputs/seed-{seed}/checkpoint.pt')
    seconds=time.monotonic()-start;assert seconds<1800
    result={'seed':seed,'baseline_exact':True,'baseline_ticks':sum(m['ticks'] for m in baseline_metrics),
            'probe_ticks':sum(m['ticks'] for m in episodes),'parity':p,'groups':group,'gates':gates,'episodes':episodes,
            'seconds':seconds,'weights_unchanged':True,'optimizer_steps':0}
    assert result['probe_ticks']<=51600 and result['baseline_ticks']==480
    atomic_json(out/'results.json',result)


def report(a):
    start=time.monotonic();records=[read(a/f'seed-{seed}/results.json') for seed in SEEDS]
    r={'experiment_id':a.name,'records':records,'gates':{k:sum(v['gates'][k] for v in records)>=2 for k in records[0]['gates']},
       'baseline_episodes':48,'probe_episodes':552,'baseline_ticks':sum(v['baseline_ticks'] for v in records),
       'probe_ticks':sum(v['probe_ticks'] for v in records),'admission_setup_ticks':200,'parity_setup_ticks':72,
       'evaluation_setup_ticks':600,'native_parity_comparisons':72,'optimizer_steps':0,'new_checkpoints':0,
       'new_policy_exports':0,'confirmation_used':False,'model_promoted':False,'general_flight_claim':False,
       'report_seconds':time.monotonic()-start}
    assert r['baseline_ticks']==1440 and r['probe_ticks']<=154800 and r['report_seconds']<180
    atomic_json(a/'results.json',r)


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True)
    mode=parser.add_mutually_exclusive_group(required=True);mode.add_argument('--prepare',action='store_true');mode.add_argument('--seed',type=int,choices=SEEDS);mode.add_argument('--report',action='store_true')
    args=parser.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.prepare:prepare(args.archive)
    elif args.seed:worker(args.archive,args.seed)
    else:report(args.archive)


if __name__=='__main__':main()
