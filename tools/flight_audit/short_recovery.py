"""Gated short recovery entry stage, with exact control replay and hold retention."""
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
from tools.flight_ppo.reward import reward
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.continuous_approach import read
from tools.flight_audit.hold_position import comparison as hold_comparison
from tools.flight_audit.settle_recover import SEEDS,FAMILIES,LIMITS,candidates,parity,physical,inspect_trace,inspect_evaluation


def training_evidence(run,update):
    state=torch.load(run/f'checkpoint-{update:04}.pt',weights_only=True);collector=state['collector'];config=state['contract']['config']
    active={e['id']:t for e,t in zip(collector['active'],collector['traces'])}
    coverage={f:{'ticks':0,'eligible_before':0,'correction_rows':0,'indices':set(),'directions':set(),'successes':0,'maximum_dwell':0} for f in FAMILIES}
    outcomes=Counter()
    for e in collector['completed']+collector['active']:
        c=e['candidate'];f=FAMILIES[c['index']%4];cov=coverage[f]
        cov['indices'].add(c['index']);cov['directions'].add((c['index']//4)%(2 if f=='spin' else 4))
        if e['ticks']==0:continue
        trace=active[e['id']] if e['id'] in active else read(run/f"training-traces/episode-{e['id']:06}.json")
        assert trace['task']==c['task'] and trace['version']==2 and trace['task'].get('hold_failure') is None
        horizon=trace['task']['arrival']['dwell_ticks'];assert horizon==config.get('recovery_dwell_ticks',300)
        assert trace['task']['arrival']==dict(LIMITS,dwell_ticks=horizon) and trace['task']['deadline_ticks']==900
        outcome=trace['steps'][-1]['progress']['outcome']
        if outcome is not None:inspect_trace(trace);assert outcome==e['outcome']
        else:
            # Active rollout cuts retain their nonterminal status during validation.
            assert e['id'] in active
        previous=trace['initial'];dwell=0;terms={}
        for tick,s in enumerate(trace['steps'],1):
            assert all(math.isfinite(v) and v>=0 for v in s['frame']['errors'].values())
            assert all(math.isfinite(s['action'][k]) and low<=s['action'][k]<=1 for k,low in [('steering',-1),('thrust',0),('brake',0)])
            before_ok=all(previous['errors'][k]<=v for k,v in LIMITS.items())
            after_ok=all(s['frame']['errors'][k]<=v for k,v in LIMITS.items())
            dwell=dwell+1 if before_ok and after_ok else 0
            cov['maximum_dwell']=max(cov['maximum_dwell'],dwell)
            if outcome is None:
                assert not s.get('error') and all(not s['evidence'][k] for k in ['ship_lost','collision','damage','boundary_escape'])
                assert dwell<horizon and tick<900 and s['progress']=={'elapsed_ticks':tick,'dwell_ticks':dwell,'outcome':None}
            cov['ticks']+=1;cov['eligible_before']+=before_ok
            key={'settled':'position','position':'position','velocity':'velocity','spin':'spin'}[f]
            cov['correction_rows']+=previous['errors'][key]>LIMITS[key]
            part=reward({'frame':previous},{'frame':s['frame'],'progress':s['progress']},config['gamma'],config['velocity_shaping_weight'],config['deadline_distance_weight'],config['regulation_weight'])
            for k,v in part.items():terms[k]=terms.get(k,0.)+v
            previous=s['frame']
        assert len(trace['steps'])==e['ticks']
        for k,v in e['reward'].items():assert abs(terms.get(k,0.)-v)<1e-6
        outcomes[outcome or 'budget_censored']+=1;cov['successes']+=outcome=='success'
    for cov in coverage.values():
        cov['indices']=sorted(cov['indices']);cov['directions']=sorted(cov['directions'])
        assert cov['ticks']==256*update
    return {'ticks':1024*update,'coverage':coverage,'outcomes':dict(outcomes),'latent_std':state['model']['log_std'].clamp(-5,2).exp().tolist()}


def retention(old):
    f=old['final']
    return f['immediate_holds']>=12 and min(f['immediate_holds_by_stratum'])>=2 and f['physical_failures']==0


def pilot_gate(records):
    assert sorted(r['seed'] for r in records)==SEEDS
    exposure=all(all(len(c['directions'])==(2 if f=='spin' else 4) and (f=='settled' or c['correction_rows']>=256) for f,c in r['training']['coverage'].items()) for r in records)
    successes=all(r['training']['outcomes'].get('success',0)>=1 for r in records)
    position=sum(r['training']['coverage']['position']['eligible_before']>=30 for r in records)>=2
    retained=sum(retention(r['legacy_hold']) for r in records)>=2
    checks={'signed_exposure':exposure,'sampled_success_each_seed':successes,'position_eligible_30_rows_two_seeds':position,'short_hold_retention_two_seeds':retained}
    return {'checks':checks,'passed':all(checks.values())}


def progression(records):
    per_seed=[]
    for r in records:
        f=r['primary']['final'];gain=f['successes']-r['primary']['initial']['successes']
        correction=f['successes']>=12 and min(f['by_family'].values())>=3 and f['physical_failures']==0
        per_seed.append({'seed':r['seed'],'correction':correction,'retention':retention(r['legacy_hold']),'gain':gain,'combined':correction and retention(r['legacy_hold'])})
    return {'per_seed':per_seed,'passed':sum(r['combined'] for r in per_seed)>=2 and sum(r['gain']>=2 for r in per_seed)>=2}


def extra(a,phase,seed):
    start=time.monotonic();deadline=start+600;run=a/f'{phase}/seed-{seed}';update=16 if phase=='pilot' else 64
    old=[r['candidate'] for r in read(a/'inputs/legacy-admission.json')['attempts'] if r['admitted']]
    parities={}
    for name,n in [('initial',0),('final',update)]:
        rows=evaluate(a/'runtime/flight_ppo_bridge',run/f'policy-{n:04}.json',old,run/f'legacy-hold-{name}',deadline)
        if name=='initial':
            for row in rows:assert read(run/f"legacy-hold-initial/clear-{row['index']}.json")==read(a/f"inputs/seed-{seed}/baseline/clear-{row['index']}.json")
        parities[name]=parity(a,run,n,candidates(run,'development'),run/f'parity-{name}.json')
        for row in read(run/'evaluation-initial/results.json'):
            assert read(run/f"evaluation-initial/clear-{row['index']}.json")==read(a/f"inputs/control-{seed}/short-initial/clear-{row['index']}.json")
    atomic_json(run/'extra-summary.json',{'seconds':time.monotonic()-start,'parity':parities,'initial_traces_exact':True})


def inspect_stage(a,phase):
    n=16 if phase=='pilot' else 64;records=[]
    for seed in SEEDS:
        run=a/f'{phase}/seed-{seed}';s=read(run/'summary.json');assert s['completed_updates']==n and s['environment_steps']==1024*n and s['total_seconds']<1800
        state=torch.load(run/'checkpoint-0000.pt',weights_only=True);parent=torch.load(a/f'inputs/seed-{seed}/checkpoint.pt',weights_only=True)
        assert equal_tree(state['model'],parent['model']) and not state['optimizer']['state']
        if phase=='pilot':
            assert equal_tree(torch.load(run/'checkpoint-0016.pt',weights_only=True),torch.load(a/f'resumed/seed-{seed}/checkpoint-0016.pt',weights_only=True))
        else:
            for u in [0,16]:assert equal_tree(torch.load(run/f'checkpoint-{u:04}.pt',weights_only=True),torch.load(a/f'pilot/seed-{seed}/checkpoint-{u:04}.pt',weights_only=True))
        long_config=read(a/f'inputs/control-{seed}/config.json');assert s['parameter_count']==39447
        actual=copy.deepcopy(state['contract']['config']);assert actual.pop('recovery_dwell_ticks')==30 and actual==long_config
        for partition in ['training','development']:
            short=candidates(run,partition);long=candidates(a/f'inputs/control-{seed}',partition)
            assert len(short)==len(long)
            for x,y in zip(short,long):
                match=copy.deepcopy(x);match['task']['arrival']['dwell_ticks']=300;assert match==y
        training=training_evidence(run,n)
        primary={key:physical(s[key+'_evaluation']) for key in ['initial','final']}
        legacy=hold_comparison(read(run/'legacy-hold-initial/results.json'),read(run/'legacy-hold-final/results.json'))
        evaluation={suite:{key:inspect_evaluation(run/f'{prefix}-{key}') for key in ['initial','final']} for suite,prefix in [('primary','evaluation'),('legacy_hold','legacy-hold')]}
        control=physical(read(a/f'inputs/control-{seed}/short-final/results.json'))
        records.append({'seed':seed,'updates':n,'primary':primary,'legacy_hold':legacy,'training':training,'evaluation':evaluation,
                        'long64_short_final':control,'difference_from_long64':primary['final']['successes']-control['successes'],
                        'main_seconds':s['total_seconds'],'extra':read(run/'extra-summary.json'),'optimizer_steps':sum(h['minibatches'] for h in s['history'])})
    return records


def verify(a,phase):
    start=time.monotonic();records=inspect_stage(a,phase)
    if phase=='pilot':
        current=torch.load(a/'control/checkpoint-0016.pt',weights_only=True);old=torch.load(a/'inputs/control-17/checkpoint-0016.pt',weights_only=True)
        for k in current:
            if k!='contract':assert equal_tree(current[k],old[k]),k
        assert current['contract']['config']==old['contract']['config']
        training_evidence(a/'control',16)
        parity(a,a/'control',16,candidates(a/'control','training')[:16],a/'validation/control-parity.json')
    result={'experiment_id':a.name,'phase':phase,'records':records,'pilot_gate':pilot_gate(records) if phase=='pilot' else read(a/'pilot-results.json')['pilot_gate'],
            'progression':progression(records),'progression_qualified':phase=='main' and progression(records)['passed'],'report_seconds':time.monotonic()-start,'confirmation_used':False,'model_promoted':False,
            'evaluation_ticks':sum(v['ticks'] for r in records for suite in r['evaluation'].values() for v in suite.values()),'evaluation_episodes':192,
            'same_task_starts_only_dwell_differs':True,'initial_parent_traces_exact':True,'control16_replay_exact_except_source_identity':True}
    assert result['report_seconds']<300
    atomic_json(a/f'{phase}-results.json',result)


def main():
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--phase',choices=['pilot','main'],required=True);p.add_argument('--extra',type=int,choices=SEEDS)
    args=p.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.extra:extra(args.archive,args.phase,args.extra)
    else:verify(args.archive,args.phase)

if __name__=='__main__':main()
