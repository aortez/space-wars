"""Frozen deterministic-steering ablation with paired latent innovations."""
import argparse
from collections import Counter
import gzip
import json
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from tools.flight_ppo.model import tensor
from tools.flight_ppo import exploration
from .execution import command, episode_seed, load_frozen, metrics

PARENT=Path('data/training-experiments/learned-flight-action-execution-v1-20261023')
MODE='mean_steering'


def steering_command(model, actor):
    # Consume all three Gaussian draws, including the unused steering draw, so
    # the two button innovation streams stay paired with the full sampler.
    commands,latent=command(model,actor,'sampled')
    for i,c in enumerate(commands):c['steering']=float(actor[i,0].tanh())
    return commands,latent


def innovation_error(model, left, right):
    count=min(len(left),len(right))
    if count==0:raise ValueError('empty pairing')
    def innovations(rows):
        actor=torch.tensor([r['actor'] for r in rows[:count]])
        latent=torch.tensor([r['latent'] for r in rows[:count]])
        scale=torch.cat((model.log_std.clamp(-5,2).exp(),actor.new_ones(2)))
        return (latent-exploration.means(actor))/scale
    a,b=innovations(left),innovations(right)
    torch.testing.assert_close(a,b,atol=2e-5,rtol=0.)
    return {'shared_ticks':count,'maximum_abs_standardized_innovation_error':float((a-b).abs().max())}


def rollout(bridge,model,candidate,rng_seed,deadline,tick_limit=1800):
    if not 1<=tick_limit<=1800:raise ValueError('invalid tick limit')
    torch.manual_seed(rng_seed)
    item=bridge.call('reset',tasks=[candidate['task']])['items'][0];rows=[]
    with torch.inference_mode():
        while item['progress']['outcome'] is None and len(rows)<tick_limit:
            if time.monotonic()>deadline:raise TimeoutError('steering ablation wall guard')
            actor,_=model(tensor([item]));commands,latent=steering_command(model,actor)
            rows.append({'actor':actor[0].tolist(),'button_probabilities':actor[0,1:].sigmoid().clamp(1e-6,1-1e-6).tolist(),
                'steering_std':float(model.log_std.clamp(-5,2).exp()[0]),'latent':latent[0],'action':commands[0]})
            item=bridge.call('step',actions=commands)['items'][0]
    trace=bridge.call('state')['traces'][0]
    assert len(trace['steps'])==len(rows)
    for r,s in zip(rows,trace['steps']):
        assert r['action']['thrust']==s['action']['thrust'] and r['action']['brake']==s['action']['brake']
        assert abs(r['action']['steering']-s['action']['steering'])<1e-5
    if item['progress']['outcome']=='environment_error':raise RuntimeError('environment failure')
    return trace,rows


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);parser.add_argument('--seed',type=int,choices=[17,29,43],required=True)
    args=parser.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    out=args.archive/f'seed-{args.seed}';out.mkdir();(out/'traces').mkdir();(out/'policy').mkdir()
    start=time.monotonic();deadline=start+1200;records=[];ticks=0
    run=args.archive/f'inputs/seed-{args.seed}'
    with Bridge(args.archive/'runtime/flight_ppo_bridge') as bridge:
        model=load_frozen(args.archive/'inputs',args.seed,bridge)
        before={k:v.clone() for k,v in model.state_dict().items()}
        admission=json.loads((run/'development-admission.json').read_text())
        candidates=[r['candidate'] for r in admission['attempts'] if r['admitted']]
        assert len(candidates)==16 and admission['seed']==4210002
        for repeat in range(4):
            for c in candidates:
                seed=episode_seed(args.seed,repeat,c['index'])
                trace,rows=rollout(bridge,model,c,seed,deadline)
                assert trace['steps'][-1]['progress']['outcome'] is not None
                ticks+=len(rows);assert ticks<=115200
                stem=f"sampled-r{repeat}-clear-{c['index']}"
                base=args.archive/f'baseline/seed-{args.seed}'
                with gzip.open(base/'policy'/f'{stem}.json.gz','rt') as f:old_rows=json.load(f)
                with gzip.open(base/'traces'/f'{stem}.json.gz','rt') as f:old_trace=json.load(f)
                assert trace['initial']==old_trace['initial']
                assert rows[0]['actor']==old_rows[0]['actor']
                pairing=innovation_error(model,rows,old_rows)
                name=f"{MODE}-r{repeat}-clear-{c['index']}"
                for folder,data in [('traces',trace),('policy',rows)]:
                    with gzip.open(out/folder/f'{name}.json.gz','wt') as f:json.dump(data,f,allow_nan=False)
                record=metrics(c,trace,rows,MODE,repeat,seed);record['pairing']=pairing
                records.append(record)
                atomic_json(out/'progress.json',{'physics_ticks':ticks,'episodes':len(records),'outcomes':dict(Counter(r['outcome'] for r in records)),
                    'last':{k:record[k] for k in ['repeat','index','outcome']}})
        assert all(torch.equal(v,before[k]) for k,v in model.state_dict().items())
    atomic_json(out/'results.json',{'seed':args.seed,'episodes':records,'physics_ticks':ticks,'seconds':time.monotonic()-start,
        'model_unchanged':True,'optimizer_steps':0,'checkpoint_sha256':sha(run/'checkpoint-0128.pt')})


if __name__=='__main__':main()
