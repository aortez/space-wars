"""LF-059 bounded runner; frozen original steering, learned actuator readout."""
import argparse
import copy
from pathlib import Path
import random
import shutil
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json,save
from .audit import read
from .collector import Collector
from .run import identity,evaluate
from . import aligned


def restore(path,expected,bridge,directory):
    s=torch.load(path,weights_only=True)
    if s['contract']!=expected:raise ValueError('aligned checkpoint contract/runtime/source mismatch')
    model=aligned.ReadoutModel(expected['schema']);model.load_state_dict(s['model'])
    optimizer=torch.optim.Adam(model.parameters(),lr=expected['config']['learning_rate'],eps=1e-5);optimizer.load_state_dict(s['optimizer'])
    collector=Collector.restore(bridge,s['collector'],directory)
    if collector.exploration is not None:raise ValueError('unexpected persistent-noise state')
    torch.set_rng_state(s['torch_rng']);random.setstate(s['python_rng'])
    return model,optimizer,collector,s['update'],s['history']


def main():
    p=argparse.ArgumentParser();p.add_argument('--bridge',required=True);p.add_argument('--config',type=Path,required=True)
    p.add_argument('--out',type=Path,required=True);p.add_argument('--resume',type=Path);p.add_argument('--initialize-from',type=Path)
    p.add_argument('--skip-evaluation',action='store_true');args=p.parse_args();c=read(args.config)
    if c!=aligned.config(c['learner_seed'],c['arm'],c['preflight']):raise ValueError('not frozen aligned config')
    if bool(args.resume)==bool(args.initialize_from):raise ValueError('choose initialization or exact resume')
    if c['preflight'] and not args.skip_evaluation:raise ValueError('preflight has no scored evaluation')
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True);torch.manual_seed(c['learner_seed']);random.seed(c['learner_seed'])
    args.out.mkdir();start=time.monotonic();deadline=start+c['wall_seconds']
    atomic_json(args.out/'run-plan.json',{'config':c,'resume':str(args.resume) if args.resume else None,'initialize_from':str(args.initialize_from) if args.initialize_from else None,'evaluation':not args.skip_evaluation})
    with Bridge(args.bridge) as bridge:
        contract=identity(c,bridge);atomic_json(args.out/'contract.json',contract)
        if args.resume:model,optimizer,collector,completed,history=restore(args.resume,contract,bridge,args.out/'training-traces')
        else:
            rows=aligned.training_candidates(c);atomic_json(args.out/'training-candidates.json',rows)
            collector=Collector(bridge,rows,args.out/'training-traces');model=aligned.ReadoutModel(bridge.contract['schema'])
            aligned.initialize(model,args.initialize_from,c)
            optimizer=torch.optim.Adam(model.parameters(),lr=c['learning_rate'],eps=1e-5);completed=0;history=[]
            save(args.out/'checkpoint-0000.pt',contract,model,optimizer,collector,0,history)
        fixed=copy.deepcopy(model.state_dict());initial=args.out/f'policy-{completed:04}.json';model.export(initial)
        rows=aligned.candidates(c['development_task_seed'],True);atomic_json(args.out/'development-candidates.json',rows)
        baseline=evaluate(args.bridge,initial,model,rows,args.out/'evaluation-initial',deadline) if not args.skip_evaluation else []
        try:
            while completed<c['updates']:
                if shutil.disk_usage(args.out).free<512*1024**2:raise RuntimeError('retention disk reserve reached')
                rollout=collector.collect(model,c,deadline);torch.save(rollout,args.out/f'rollout-{completed+1:04}.pt')
                metrics=aligned.update(model,optimizer,rollout,c);aligned.assert_fixed_steering(fixed,model.state_dict());completed+=1
                metrics.update(update=completed,environment_steps=collector.steps,completed_episodes=len(collector.completed));history.append(metrics)
                save(args.out/f'checkpoint-{completed:04}.pt',contract,model,optimizer,collector,completed,history)
                atomic_json(args.out/'progress.json',{'updates':history,'started':collector.started,'completed':collector.completed})
                print(metrics,flush=True)
            final=args.out/f'policy-{completed:04}.json';model.export(final)
            final_rows=evaluate(args.bridge,final,model,rows,args.out/'evaluation-final',deadline) if not args.skip_evaluation else []
            atomic_json(args.out/'summary.json',{'completed_updates':completed,'environment_steps':collector.steps,'started_episodes':collector.started,'total_seconds':time.monotonic()-start,
                'parameter_count':sum(p.numel() for p in model.parameters()),'effective_trainable_parameters':1765,'history':history,
                'initial_evaluation':baseline,'final_evaluation':final_rows,'promoted':False})
        except Exception as error:
            atomic_json(args.out/'interruption.json',{'error':str(error),'completed_updates':completed,'partial_traces':bridge.call('state')['traces']});raise


if __name__=='__main__':main()
