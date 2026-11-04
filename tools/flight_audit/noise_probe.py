"""Frozen-policy paired exploration-amplitude probe; no training or weight mutation."""
import argparse
from collections import Counter
import copy
import gzip
import hashlib
import json
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json,sha
from tools.flight_ppo.evaluate import evaluate
from tools.flight_ppo.execution import load_export
from tools.flight_ppo.model import actions,tensor
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.continuous_approach import read
from tools.flight_audit.settle_recover import inspect_trace,parity,LIMITS,FAMILIES,SEEDS

SCALES={'full':1.,'half':.5,'quarter':.25}
SUITES=['recovery','hold']
REPEATS=4
BASE=4610000


def stream_seed(seed,suite,repeat,index):
    if seed not in SEEDS or suite not in SUITES or not 0<=repeat<REPEATS or not 0<=index<16:
        raise ValueError('outside frozen stream matrix')
    return BASE+seed*100000+SUITES.index(suite)*10000+repeat*100+index


def innovations(seed,suite,repeat,index,horizon):
    if horizon not in [900,1800]:raise ValueError('wrong deadline')
    return torch.randn(horizon,3,generator=torch.Generator().manual_seed(stream_seed(seed,suite,repeat,index)))


def command(actor,std,scale,noise):
    if scale not in [0.,.25,.5,1.] or not math.isfinite(scale):raise ValueError('undeclared amplitude')
    if actor.shape!=(1,3) or std.shape!=(3,) or noise.shape!=(1,3):raise ValueError('wrong latent shape')
    if not all(torch.isfinite(x).all() for x in [actor,std,noise]) or not (std>0).all():raise ValueError('invalid latent parameters')
    latent=actor+(std*scale)*noise
    return actions(latent[:,0],latent[:,1:]),latent


def save_gzip(path,data):
    path.parent.mkdir(parents=True,exist_ok=True)
    with gzip.open(path,'wt') as f:json.dump(data,f,allow_nan=False,separators=(',',':'))


def read_gzip(path):
    with gzip.open(path,'rt') as f:return json.load(f)


def metrics(trace):
    m=inspect_trace(trace);previous=trace['initial'];eligible=0;first_exit_errors={k:previous['errors'][k] for k,v in LIMITS.items() if previous['errors'][k]>v} or None
    for tick,s in enumerate(trace['steps'],1):
        eligible+=all(previous['errors'][k]<=v for k,v in LIMITS.items())
        if first_exit_errors is None and any(s['frame']['errors'][k]>v for k,v in LIMITS.items()):
            first_exit_errors={k:s['frame']['errors'][k] for k,v in LIMITS.items() if s['frame']['errors'][k]>v}
        previous=s['frame']
    m.update(eligible_before=eligible,immediate_hold=m['outcome']=='success' and m['ticks']==30,first_exit_errors=first_exit_errors)
    return m


def group(rows,suite):
    return {'episodes':len(rows),'successes':sum(r['outcome']=='success' for r in rows),
            'immediate_holds':sum(r['immediate_hold'] for r in rows),'physical_failures':sum(r['physical_failure'] for r in rows),
            'ticks':sum(r['ticks'] for r in rows),'eligible_before':sum(r['eligible_before'] for r in rows),
            'maximum_dwell':max(r['maximum_dwell'] for r in rows),
            'outcomes':dict(Counter(r['outcome'] for r in rows)),
            'mean_final_errors':{k:sum(r['final_errors'][k] for r in rows)/len(rows) for k in LIMITS},
            ('by_family' if suite=='recovery' else 'by_heading_stratum'):{f:sum(r['outcome']=='success' for r in rows if r['index']%4==j) for j,f in enumerate(FAMILIES if suite=='recovery' else ['0','1','2','3'])}}


def worker(a,seed,suite):
    start=time.monotonic();deadline=start+1800;out=a/f'runs/seed-{seed}/{suite}';out.mkdir(parents=True)
    inputs=a/f'inputs/seed-{seed}';policy=inputs/'policy-0000.json';policy_hash=sha(policy);checkpoint_hash=sha(inputs/'checkpoint-0000.pt')
    batch=read(a/f'inputs/{suite}-admission.json');assert batch['complete']
    cs=[r['candidate'] for r in batch['attempts'] if r['admitted']];assert sorted(c['index'] for c in cs)==list(range(16))
    with Bridge(a/'runtime/flight_ppo_bridge') as b:
        admissions=[b.call('inspect',candidate=c) for c in cs];assert all(r['admitted'] for r in admissions)
    atomic_json(out/'admission.json',admissions)
    par=parity(a,inputs,0,cs,out/'parity.json')
    native=evaluate(a/'runtime/flight_ppo_bridge',policy,cs,out/'native',deadline)
    all_rows=[]
    for r in native:
        trace=read(out/f"native/clear-{r['index']}.json");assert trace==read(inputs/f"{suite}-baseline/clear-{r['index']}.json")
        all_rows.append(dict(metrics(trace),condition='native',repeat=0,index=r['index']))
    torch.manual_seed(0);initial_rng=torch.get_rng_state().clone()
    with torch.random.fork_rng(devices=[]),Bridge(a/'runtime/flight_ppo_bridge') as b:
        model=load_export(policy,b.contract['schema']);weights=copy.deepcopy(model.state_dict());std=model.log_std.clamp(-5,2).exp().clone()
        streams={}
        for c in cs:
            for repeat in range(REPEATS):
                noise=innovations(seed,suite,repeat,c['index'],c['task']['deadline_ticks']);streams[(repeat,c['index'])]=noise
                save_gzip(out/f"streams/repeat-{repeat}/clear-{c['index']}.json.gz",{'seed':stream_seed(seed,suite,repeat,c['index']),'values':noise.tolist()})
        for condition,scale,repeats in [('mean',0.,1)]+[(k,v,REPEATS) for k,v in SCALES.items()]:
            for repeat in range(repeats):
                for c in cs:
                    index=c['index'];item=b.call('reset',tasks=[c['task']])['items'][0];policy_rows=[];tick=0
                    noise=streams[(repeat,index)]
                    while item['progress']['outcome'] is None:
                        if time.monotonic()>deadline:raise TimeoutError('frozen noise-probe budget exhausted')
                        with torch.inference_mode():
                            actor,_=model(tensor([item]));commands,latent=command(actor,std,scale,noise[tick:tick+1])
                        policy_rows.append({'actor':actor[0].tolist(),'latent':latent[0].tolist(),'action':commands[0]})
                        item=b.call('step',actions=commands)['items'][0];tick+=1
                    trace=b.call('state')['traces'][0];m=metrics(trace)
                    suffix=f'{condition}/repeat-{repeat}/clear-{index}.json.gz'
                    save_gzip(out/'traces'/suffix,trace);save_gzip(out/'policy'/suffix,policy_rows)
                    all_rows.append(dict(m,condition=condition,repeat=repeat,index=index))
            assert equal_tree(weights,model.state_dict())
            atomic_json(out/'progress.json',{'completed_condition':condition,'episodes':len(all_rows),'ticks':sum(r['ticks'] for r in all_rows)})
        assert equal_tree(model.state_dict(),torch.load(inputs/'checkpoint-0000.pt',weights_only=True)['model'])
    assert torch.equal(initial_rng,torch.get_rng_state()) and sha(policy)==policy_hash and sha(inputs/'checkpoint-0000.pt')==checkpoint_hash
    summaries={k:group([r for r in all_rows if r['condition']==k],suite) for k in ['native','mean']+list(SCALES)}
    result={'seed':seed,'suite':suite,'rows':all_rows,'summary':summaries,'parity':par,'seconds':time.monotonic()-start,
            'weights_unchanged':True,'global_rng_preserved':True,'native_baselines_exact':True,'latent_std':std.tolist(),'policy_sha256':policy_hash}
    assert len(all_rows)==224 and result['seconds']<1800
    atomic_json(out/'results.json',result)


def gates(records):
    comparisons=[]
    for seed in SEEDS:
        hold=next(r for r in records if r['seed']==seed and r['suite']=='hold')['summary']
        recovery=next(r for r in records if r['seed']==seed and r['suite']=='recovery')['summary']
        for scale in ['half','quarter']:
            h=hold[scale];f=hold['full'];r=recovery[scale];base=recovery['full']
            comparisons.append({'seed':seed,'condition':scale,'hold_gain':h['immediate_holds']-f['immediate_holds'],
                'position_gain':r['by_family']['position']-base['by_family']['position'],
                'holding_noise_effect':h['immediate_holds']-f['immediate_holds']>=16 and h['physical_failures']==0,
                'position_noise_effect':r['by_family']['position']>=8 and r['by_family']['position']-base['by_family']['position']>=4 and r['physical_failures']==0,
                'broad_recovery':r['successes']>=48 and min(r['by_family'].values())>=12 and r['physical_failures']==0})
    return comparisons,{scale:{k:sum(r[k] for r in comparisons if r['condition']==scale)>=2 for k in ['holding_noise_effect','position_noise_effect','broad_recovery']} for scale in ['half','quarter']}


def verify(a):
    start=time.monotonic();records=[];baseline_differences=[]
    for seed in SEEDS:
        for suite in SUITES:
            out=a/f'runs/seed-{seed}/{suite}';r=read(out/'results.json');assert r['seconds']<1800
            seen=set();std=torch.tensor(r['latent_std'])
            baselines={i:read(out/f'native/clear-{i}.json') for i in range(16)}
            for row in r['rows']:
                key=(row['condition'],row['repeat'],row['index']);assert key not in seen;seen.add(key)
                condition,repeat,index=key
                if condition=='native':trace=read(out/f'native/clear-{index}.json')
                else:
                    suffix=f'{condition}/repeat-{repeat}/clear-{index}.json.gz';trace=read_gzip(out/'traces'/suffix)
                    policy_rows=read_gzip(out/'policy'/suffix);assert len(policy_rows)==len(trace['steps'])
                    noise=innovations(seed,suite,repeat,index,trace['task']['deadline_ticks'])
                    stored=read_gzip(out/f'streams/repeat-{repeat}/clear-{index}.json.gz')
                    assert stored['seed']==stream_seed(seed,suite,repeat,index) and torch.equal(torch.tensor(stored['values']),noise)
                    actor=torch.tensor([p['actor'] for p in policy_rows])
                    latent=actor+(std*(0. if condition=='mean' else SCALES[condition]))*noise[:len(policy_rows)]
                    assert torch.equal(latent,torch.tensor([p['latent'] for p in policy_rows]))
                    decoded=actions(latent[:,0],latent[:,1:])
                    for p,s,expected in zip(policy_rows,trace['steps'],decoded):
                        assert p['action']==s['action']
                        # Vectorized transcendental kernels may differ by one float ulp.
                        assert all(abs(p['action'][k]-expected[k])<=1e-7 for k in expected)
                assert trace['task']==baselines[index]['task'] and trace['initial']==baselines[index]['initial']
                m=metrics(trace);assert all(row[k]==v for k,v in m.items())
            for condition in ['native','mean']+list(SCALES):
                rows=[x for x in r['rows'] if x['condition']==condition]
                assert len(rows)==(16 if condition in ['native','mean'] else 64)
                assert all(sum(x['index']==i for x in rows)==(1 if condition in ['native','mean'] else 4) for i in range(16))
                assert group(rows,suite)==r['summary'][condition]
            native={v['index']:v for v in r['rows'] if v['condition']=='native'}
            for row in [v for v in r['rows'] if v['condition']=='mean']:
                n=native[row['index']]
                if (n['outcome'],n['ticks'])!=(row['outcome'],row['ticks']):baseline_differences.append({'seed':seed,'suite':suite,'index':row['index'],'native':[n['outcome'],n['ticks']],'python':[row['outcome'],row['ticks']]})
            records.append(r)
    paired,g=gates(records)
    result={'experiment_id':a.name,'records':records,'paired':paired,'gates':g,'native_python_mean_episode_differences':baseline_differences,
            'evaluation_episodes':sum(len(r['rows']) for r in records),'evaluation_ticks':sum(row['ticks'] for r in records for row in r['rows']),
            'admission_setups':96,'parity_comparisons':96,'normal_stream_count':384,'training_ticks':0,'optimizer_steps':0,
            'weights_unchanged':True,'confirmation_used':False,'model_promoted':False,'general_navigation_claim':False,'report_seconds':time.monotonic()-start}
    assert result['evaluation_episodes']==1344 and result['evaluation_ticks']<=1814400 and result['report_seconds']<600
    atomic_json(a/'results.json',result)


def main():
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--seed',type=int,choices=SEEDS);p.add_argument('--suite',choices=SUITES)
    args=p.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.seed:worker(args.archive,args.seed,args.suite)
    else:verify(args.archive)

if __name__=='__main__':main()
