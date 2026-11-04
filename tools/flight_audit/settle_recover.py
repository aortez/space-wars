"""Paired near-goal recovery: legacy shaping versus a bounded state cost."""
import argparse
from collections import Counter
import copy
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.evaluate import evaluate
from tools.flight_ppo.execution import load_export
from tools.flight_ppo.model import actions, tensor
from tools.flight_ppo.reward import reward
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.continuous_approach import read
from tools.flight_audit.hold_position import comparison as hold_comparison
from tools.flight_audit.hold_robustness import LIMITS

SEEDS=[17,29,43]
ARMS=['legacy','dense']
UPDATES=64
FAMILIES=['settled','position','velocity','spin']


def inspect_trace(trace):
    task=trace['task'];criteria=task['arrival'];steps=trace['steps']
    if trace['version']!=2 or task.get('hold_failure') is not None:raise ValueError('wrong terminal contract')
    if any(criteria[k]!=v for k,v in LIMITS.items()):raise ValueError('changed physical tolerance')
    horizon=criteria['dwell_ticks'];deadline=task['deadline_ticks']
    if horizon not in [30,120,300] or deadline not in [30,120,300,900,1800] or not 0<len(steps)<=deadline:raise ValueError('wrong horizon/trace length')
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


def candidates(run, partition):
    batch=read(run/f'{partition}-admission.json');assert batch['complete']
    return [r['candidate'] for r in batch['attempts'] if r['admitted']]


def training_inspection(run, update):
    state=torch.load(run/f'checkpoint-{update:04}.pt',weights_only=True);collector=state['collector'];config=state['contract']['config']
    active={e['id']:t for e,t in zip(collector['active'],collector['traces'])};rows=[]
    coverage={name:{'ticks':0,'eligible_before':0,'correction_rows':0,'indices':set(),'directions':set()} for name in FAMILIES}
    for e in collector['completed']+collector['active']:
        c=e['candidate'];family=FAMILIES[c['index']%4];cov=coverage[family]
        cov['indices'].add(c['index']);cov['directions'].add((c['index']//4)%(2 if family=='spin' else 4))
        if e['ticks']==0:continue
        trace=active[e['id']] if e['id'] in active else read(run/f"training-traces/episode-{e['id']:06}.json")
        # Rollout cuts keep their nonterminal status during independent validation.
        if trace['steps'][-1]['progress']['outcome'] is None:
            m={'outcome':None,'ticks':len(trace['steps']),'maximum_dwell':max(s['progress']['dwell_ticks'] for s in trace['steps'])}
            validate_active(trace)
        else:m=inspect_trace(trace)
        assert m['ticks']==e['ticks']
        if e['id'] not in active:assert m['outcome']==e['outcome']
        previous=trace['initial'];terms={}
        for s in trace['steps']:
            cov['ticks']+=1;cov['eligible_before']+=all(previous['errors'][k]<=v for k,v in LIMITS.items())
            key={'settled':'position','position':'position','velocity':'velocity','spin':'spin'}[family]
            cov['correction_rows']+=previous['errors'][key]>LIMITS[key]
            part=reward({'frame':previous},{'frame':s['frame'],'progress':s['progress']},config['gamma'],config['velocity_shaping_weight'],config['deadline_distance_weight'],config['regulation_weight'])
            for k,v in part.items():terms[k]=terms.get(k,0.)+v
            previous=s['frame']
        for k,v in e['reward'].items():assert abs(terms.get(k,0.)-v)<1e-6
        rows.append(dict(m,family=family))
    ticks=sum(r['ticks'] for r in rows);assert ticks==collector['steps']==1024*update
    for cov in coverage.values():
        cov['indices']=sorted(cov['indices']);cov['directions']=sorted(cov['directions'])
    return {'ticks':ticks,'outcomes':dict(Counter(r['outcome'] or 'budget_censored' for r in rows)),
            'maximum_dwell':max(r['maximum_dwell'] for r in rows),'coverage':coverage,
            'latent_std':state['model']['log_std'].clamp(-5,2).exp().tolist()}


def validate_active(trace):
    # The shared physical rules are identical; active rollout cuts are not terminals.
    task=trace['task'];assert trace['version']==2 and task.get('hold_failure') is None
    assert task['deadline_ticks']==900 and task['arrival']==dict(LIMITS,dwell_ticks=300)
    previous=trace['initial'];dwell=0
    for tick,s in enumerate(trace['steps'],1):
        assert all(math.isfinite(v) for v in s['frame']['errors'].values()) and not s.get('error')
        assert all(not s['evidence'][k] for k in ['collision','damage','ship_lost','boundary_escape'])
        assert all(math.isfinite(s['action'][k]) and low<=s['action'][k]<=1 for k,low in [('steering',-1),('thrust',0),('brake',0)])
        eligible=lambda f:all(f['errors'][k]<=v for k,v in LIMITS.items())
        dwell=dwell+1 if eligible(previous) and eligible(s['frame']) else 0
        assert dwell<300 and tick<900 and s['progress']=={'elapsed_ticks':tick,'dwell_ticks':dwell,'outcome':None}
        previous=s['frame']


def physical(rows):
    assert len(rows)==16 and len({r['index'] for r in rows})==16
    assert all(r['outcome'] in ['success','deadline','collision','damage','ship_lost','boundary_escape'] for r in rows)
    by=[sum(r['outcome']=='success' and r['index']%4==i for r in rows) for i in range(4)]
    assert all(sum(r['index']%4==i for r in rows)==4 for i in range(4))
    return {'successes':sum(by),'by_family':dict(zip(FAMILIES,by)),
            'physical_failures':sum(r['outcome'] not in ['success','deadline'] for r in rows),
            'outcomes':dict(Counter(r['outcome'] for r in rows))}


def inspect_evaluation(folder):
    rows=read(folder/'results.json');metrics=[]
    for row in rows:
        t=read(folder/f"clear-{row['index']}.json");m=inspect_trace(t)
        assert m['outcome']==row['outcome'] and m['ticks']==row['ticks'] and m['final_errors']==row['final_errors']
        assert t['initial']['errors']==row['initial_errors']
        for flag in ['collision','damage']:assert row[flag]==any(s['evidence'][flag] for s in t['steps'])
        metrics.append(dict(m,index=row['index']))
    return {'ticks':sum(m['ticks'] for m in metrics),'episodes':metrics}


def parity(a,run,update,fixtures,destination):
    with Bridge(a/'runtime/flight_ppo_bridge') as bridge,torch.random.fork_rng(devices=[]):
        model=load_export(run/f'policy-{update:04}.json',bridge.contract['schema']);state=torch.load(run/f'checkpoint-{update:04}.pt',weights_only=True)
        assert equal_tree(model.state_dict(),state['model']);weights=copy.deepcopy(model.state_dict());rng=torch.get_rng_state().clone()
        items=bridge.call('reset',tasks=[c['task'] for c in fixtures])['items'];bridge.call('load_policy',path=str((run/f'policy-{update:04}.json').resolve()))
        native=bridge.call('infer',inputs=[i['inputs'] for i in items])['outputs']
        with torch.no_grad():actor,value=model(tensor(items))
        expected=torch.cat((actor,value[:,None]),dim=1);actual=torch.tensor([[n[k] for k in ['steering_latent','thrust_logit','brake_logit','value']] for n in native])
        torch.testing.assert_close(actual,expected,atol=1e-5,rtol=1e-5)
        commands=actions(actor[:,0],actor[:,1:]);error=max(abs(c[k]-n['action'][k]) for c,n in zip(commands,native) for k in c)
        assert error<1e-5 and equal_tree(weights,model.state_dict()) and torch.equal(rng,torch.get_rng_state())
        result={'fixtures':len(fixtures),'raw_error':float((actual-expected).abs().max()),'action_error':error}
        atomic_json(destination,{'items':items,'native':native,'python_raw':expected.tolist(),'python_actions':commands,'summary':result});return result


def preflight(a):
    records=[]
    for arm in ARMS:
        run=a/f'preflight/{arm}';resumed=a/f'preflight/{arm}-resumed'
        assert equal_tree(torch.load(run/'checkpoint-0016.pt',weights_only=True),torch.load(resumed/'checkpoint-0016.pt',weights_only=True))
        training=training_inspection(run,16)
        for family,c in training['coverage'].items():
            assert c['ticks']==4096 and len(c['directions'])==(2 if family=='spin' else 4)
            if family!='settled':assert c['correction_rows']>=256
        assert training['coverage']['settled']['eligible_before']>0
        p=parity(a,run,16,candidates(run,'training')[:16],a/f'preflight/{arm}-parity.json')
        records.append({'arm':arm,'training':training,'parity':p,'exact_resume':True})
    atomic_json(a/'preflight/verification.json',{'records':records,'training_ticks':34816,'maximum_optimizer_steps':544,'coverage_gate':True,'incentive_checks':incentive_checks()})


def extra(a,arm,seed):
    start=time.monotonic();deadline=start+1200;run=a/f'runs/{arm}/seed-{seed}';cs=candidates(run,'development')
    short=copy.deepcopy(cs)
    for c in short:c['task']['arrival']['dwell_ticks']=30
    with Bridge(a/'runtime/flight_ppo_bridge') as bridge:
        admissions=[bridge.call('inspect',candidate=c) for c in short];assert all(r['admitted'] for r in admissions)
    atomic_json(run/'matched-short-admission.json',admissions)
    old=[r['candidate'] for r in read(a/'inputs/legacy-admission.json')['attempts'] if r['admitted']]
    parities={}
    for phase,n in [('initial',0),('final',64)]:
        evaluate(a/'runtime/flight_ppo_bridge',run/f'policy-{n:04}.json',short,run/f'matched-short-{phase}',deadline)
        rows=evaluate(a/'runtime/flight_ppo_bridge',run/f'policy-{n:04}.json',old,run/f'legacy-hold-{phase}',deadline)
        if phase=='initial':
            for row in rows:assert read(run/f"legacy-hold-initial/clear-{row['index']}.json")==read(a/f"inputs/seed-{seed}/baseline/clear-{row['index']}.json")
        parities[phase]=parity(a,run,n,cs,run/f'parity-{phase}.json')
    atomic_json(run/'extra-summary.json',{'seconds':time.monotonic()-start,'parity':parities,'initial_legacy_traces_exact':True})


def report(a):
    start=time.monotonic();records=[]
    for partition in ['training','development']:
        batches=[read(a/f'runs/{arm}/seed-{seed}/{partition}-admission.json') for arm in ARMS for seed in SEEDS]
        assert all(b==batches[0] for b in batches)
    for seed in SEEDS:
        initial=[];admissions=[]
        for arm in ARMS:
            run=a/f'runs/{arm}/seed-{seed}';s=read(run/'summary.json');assert s['completed_updates']==64 and s['environment_steps']==65536 and s['total_seconds']<1800
            zero=torch.load(run/'checkpoint-0000.pt',weights_only=True);initial.append(zero)
            parent=torch.load(a/f'inputs/seed-{seed}/checkpoint.pt',weights_only=True);assert equal_tree(zero['model'],parent['model']) and not zero['optimizer']['state']
            admissions.append(read(run/'training-admission.json'))
            if seed==17:
                for n in [0,16]:assert equal_tree(torch.load(run/f'checkpoint-{n:04}.pt',weights_only=True),torch.load(a/f'preflight/{arm}/checkpoint-{n:04}.pt',weights_only=True))
            training=training_inspection(run,64);assert all(len(c['indices'])==16 for c in training['coverage'].values())
            primary={phase:physical(s[phase+'_evaluation']) for phase in ['initial','final']}
            gain=primary['final']['successes']-primary['initial']['successes']
            legacy=hold_comparison(read(run/'legacy-hold-initial/results.json'),read(run/'legacy-hold-final/results.json'))
            gates={'learning_evidence':primary['final']['successes']>=4 and gain>=4,
                   'sustained_recovery':primary['final']['successes']>=12 and gain>=8 and min(primary['final']['by_family'].values())>=3 and primary['final']['physical_failures']==0,
                   'short_hold_retained':legacy['final']['immediate_holds']>=12 and min(legacy['final']['immediate_holds_by_stratum'])>=2 and legacy['final']['physical_failures']==0}
            evaluation={suite:{phase:inspect_evaluation(run/f'{prefix}-{phase}') for phase in ['initial','final']} for suite,prefix in [('primary','evaluation'),('matched_short','matched-short'),('legacy_hold','legacy-hold')]}
            extra_summary=read(run/'extra-summary.json');assert extra_summary['seconds']<1200
            records.append({'seed':seed,'arm':arm,'primary':primary,'gain':gain,'gates':gates,'legacy_hold':legacy,
                            'matched_short':{phase:physical(read(run/f'matched-short-{phase}/results.json')) for phase in ['initial','final']},
                            'training':training,'evaluation':evaluation,'parity':extra_summary['parity'],'main_seconds':s['total_seconds'],
                            'extra_seconds':extra_summary['seconds'],'optimizer_steps':sum(h['minibatches'] for h in s['history'])})
        assert admissions[0]==admissions[1]
        for key in ['model','optimizer','torch_rng','python_rng','collector']:assert equal_tree(initial[0][key],initial[1][key])
        pair=[r for r in records if r['seed']==seed];assert pair[0]['primary']['initial']==pair[1]['primary']['initial']
    paired=[{'seed':seed,'dense_minus_legacy':next(v for v in records if v['seed']==seed and v['arm']=='dense')['primary']['final']['successes']-next(v for v in records if v['seed']==seed and v['arm']=='legacy')['primary']['final']['successes']} for seed in SEEDS]
    r={'experiment_id':a.name,'records':records,'gates':{arm:{k:sum(v['gates'][k] for v in records if v['arm']==arm)>=2 for k in records[0]['gates']} for arm in ARMS},
       'paired':paired,'dense_improvement':sum(v['dense_minus_legacy']>=4 for v in paired)>=2,
       'training_ticks':393216,'preflight_training_ticks':34816,'evaluation_ticks':sum(v['evaluation'][suite][phase]['ticks'] for v in records for suite in ['primary','matched_short','legacy_hold'] for phase in ['initial','final']),
       'evaluation_episodes':576,'parity_comparisons':224,'matched_short_admission_setups':96,'report_seconds':time.monotonic()-start,
       'exact_resume':True,'parent_weights_exact':True,'matched_initial_states':True,'all_training_cases_exposed':True,
       'confirmation_used':False,'model_promoted':False,'general_navigation_claim':False}
    assert r['evaluation_ticks']<=691200 and r['report_seconds']<900
    atomic_json(a/'results.json',r)


def incentive_checks():
    # Synthetic error paths audit reward incentives, not physical reachability.
    gamma=.9995
    def frame(d=0.,v=0.,s=0.):return {'errors':{'position':d,'velocity':v,'spin':s}}
    paths={'idle':[frame(6.)]*900,
           'approach_then_hold':[frame(6.*(1-t/180),2.*math.sin(math.pi*t/180)) for t in range(1,181)]+[frame()]*300,
           'inside_quiet':[frame()]*300,'inside_offset':[frame(1.5)]*300,
           'inside_velocity':[frame(0.,.5)]*300,'inside_spin':[frame(0.,0.,.1)]*300}
    scores={}
    for name,path in paths.items():
        scores[name]={}
        for arm,w in [('legacy',0.),('dense',.003)]:
            before={'frame':frame(6.) if name in ['idle','approach_then_hold'] else frame()};total=0.
            for t,f in enumerate(path):
                after={'frame':f,'progress':{'outcome':('deadline' if name=='idle' else 'success') if t==len(path)-1 else None}}
                total+=gamma**t*reward(before,after,gamma,.25,4.,w)['total'];before=after
            scores[name][arm]=total
    assert all(scores['approach_then_hold'][a]>scores['idle'][a] for a in ARMS)
    assert all(scores['inside_quiet']['dense']>scores[k]['dense'] for k in ['inside_offset','inside_velocity','inside_spin'])
    # Remove the shared -initial potential: discounted shaping telescopes to zero.
    conservative_deadline_lower_bound=-900*(.001+.003)-6.
    assert conservative_deadline_lower_bound>-10.
    return {'synthetic_discounted_returns':scores,'nonphysical_deadline_lower_bound_without_initial_potential':conservative_deadline_lower_bound,
            'immediate_physical_failure_upper_bound_without_initial_potential':-10.,
            'synthetic_paths_are_not_physics_validation':True}


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True)
    parser.add_argument('--preflight',action='store_true');parser.add_argument('--extra',choices=ARMS);parser.add_argument('--seed',type=int,choices=SEEDS)
    args=parser.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.preflight:preflight(args.archive)
    elif args.extra:extra(args.archive,args.extra,args.seed)
    else:report(args.archive)


if __name__=='__main__':main()
