"""Initial-versus-trained evaluation under one explicit mixed execution contract."""
import argparse
from collections import Counter
import gzip
import json
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import identity,sha,atomic_json
from tools.flight_ppo.model import FlightModel
from tools.flight_ppo import exploration
from .execution import episode_seed,metrics
from .steering import rollout

PARENT=Path('data/training-experiments/learned-flight-steering-ablation-v1-20261023')
INITIAL=Path('data/training-experiments/learned-flight-aligned-approach-v1-20261023')
CONTRACT=Path(__file__).with_name('mixed-execution-v1.json')


def validate_contract(path):
    actual=json.loads(Path(path).read_text());expected=json.loads(CONTRACT.read_text())
    if actual!=expected:raise ValueError('unsupported execution contract')
    if actual['id']!='mean-steering-sampled-buttons-v1' or actual['version']!=1:raise ValueError('unsupported contract version')
    return actual


def load_checkpoint(inputs,seed,update,bridge):
    if update not in [0,128]:raise ValueError('undeclared checkpoint')
    state=torch.load(inputs/f'seed-{seed}'/f'checkpoint-{update:04}.pt',weights_only=True)
    if state['contract']!=identity(state['contract']['config'],bridge):raise ValueError('checkpoint source/runtime mismatch')
    assert state['update']==update
    assert state['contract']['config']['exploration']=={'kind':'gaussian-latent-ar1-v1','rho':0.}
    model=FlightModel(bridge.contract['schema']);model.load_state_dict(state['model'],strict=True)
    model.eval();model.requires_grad_(False)
    return model


def paired_innovations(left,right):
    count=min(len(left),len(right))
    if count<1:raise ValueError('empty pairing')
    def standardized(rows):
        rows=rows[:count];actor=torch.tensor([r['actor'] for r in rows]);latent=torch.tensor([r['latent'] for r in rows])
        scale=torch.tensor([[r['steering_std'],1.,1.] for r in rows])
        return (latent-exploration.means(actor))/scale
    a,b=standardized(left),standardized(right)
    torch.testing.assert_close(a,b,atol=2e-5,rtol=0.)
    return {'shared_ticks':count,'maximum_abs_standardized_innovation_error':float((a-b).abs().max())}


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);parser.add_argument('--seed',type=int,choices=[17,29,43],required=True)
    args=parser.parse_args();torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    contract=validate_contract(args.archive/'execution-contract.json')
    out=args.archive/f'seed-{args.seed}';out.mkdir();(out/'traces').mkdir();(out/'policy').mkdir()
    start=time.monotonic();deadline=start+1200;records=[];ticks=0
    run=args.archive/f'inputs/seed-{args.seed}'
    with Bridge(args.archive/'runtime/flight_ppo_bridge') as bridge:
        model=load_checkpoint(args.archive/'inputs',args.seed,0,bridge)
        before={k:v.clone() for k,v in model.state_dict().items()}
        admission=json.loads((run/'development-admission.json').read_text());assert admission['seed']==4210002
        candidates=[r['candidate'] for r in admission['attempts'] if r['admitted']];assert len(candidates)==16
        for repeat in range(4):
            for c in candidates:
                seed=episode_seed(args.seed,repeat,c['index'])
                trace,rows=rollout(bridge,model,c,seed,deadline)
                assert trace['steps'][-1]['progress']['outcome'] is not None
                ticks+=len(rows);assert ticks<=115200
                stem=f"mean_steering-r{repeat}-clear-{c['index']}"
                base=args.archive/f'trained/seed-{args.seed}'
                with gzip.open(base/'policy'/f'{stem}.json.gz','rt') as f:trained_rows=json.load(f)
                with gzip.open(base/'traces'/f'{stem}.json.gz','rt') as f:trained_trace=json.load(f)
                assert trace['initial']==trained_trace['initial']
                pairing=paired_innovations(rows,trained_rows)
                name=f"initial-r{repeat}-clear-{c['index']}"
                for folder,data in [('traces',trace),('policy',rows)]:
                    with gzip.open(out/folder/f'{name}.json.gz','wt') as f:json.dump(data,f,allow_nan=False)
                record=metrics(c,trace,rows,'initial',repeat,seed);record['pairing']=pairing;records.append(record)
                atomic_json(out/'progress.json',{'physics_ticks':ticks,'episodes':len(records),'outcomes':dict(Counter(r['outcome'] for r in records)),
                    'last':{k:record[k] for k in ['repeat','index','outcome']}})
        assert all(torch.equal(v,before[k]) for k,v in model.state_dict().items())
    atomic_json(out/'results.json',{'seed':args.seed,'episodes':records,'physics_ticks':ticks,'seconds':time.monotonic()-start,
        'model_unchanged':True,'optimizer_steps':0,'checkpoint_sha256':sha(run/'checkpoint-0000.pt'),'execution_contract':contract['id']})


if __name__=='__main__':main()
