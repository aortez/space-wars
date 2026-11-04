"""Protected speed-control comparison with complete compressed checkpoints/traces."""
import argparse
import copy
from pathlib import Path
import random
import shutil
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json,sha
from tools.flight_ppo.model import actions
from .collector import Collector
from .model import tensor
from .run import identity as base_identity
from . import sustained,storage


class CompressedCollector(Collector):
    def save_trace(self,episode_id,trace):storage.dump_json(self.directory/f'episode-{episode_id:06}.json.gz',trace)


def identity(c,bridge):
    return {**base_identity(c,bridge),'kind':'motor-protected-checkpoint-v2-gzip','architecture':sustained.ARCHITECTURE}


def save(path,contract,model,optimizer,collector,u,history):
    state={'contract':contract,'model':model.state_dict(),'optimizer':optimizer.state_dict(),'collector':collector.state(),
           'update':u,'history':history,'torch_rng':torch.get_rng_state(),'python_rng':random.getstate()}
    storage.dump_torch(path,state);atomic_json(path.parent/'latest.json',{'checkpoint':path.name,'sha256':sha(path),'update':u})


def restore(path,expected,bridge,directory):
    s=storage.load_torch(path)
    if s['contract']!=expected:raise ValueError('protected checkpoint contract/runtime/source mismatch')
    model=sustained.ResidualModel(expected['schema']);model.load_state_dict(s['model'])
    optimizer=torch.optim.Adam(model.parameters(),lr=expected['config']['learning_rate'],eps=1e-5);optimizer.load_state_dict(s['optimizer'])
    collector=CompressedCollector.restore(bridge,s['collector'],directory)
    if collector.exploration is not None:raise ValueError('unexpected persistent noise')
    torch.set_rng_state(s['torch_rng']);random.setstate(s['python_rng'])
    return model,optimizer,collector,s['update'],s['history']


def evaluate(executable,policy,model,rows,directory,deadline):
    directory.mkdir();results=[];parity=[]
    with Bridge(executable) as bridge:
        bridge.call('load_policy',path=str(policy.resolve()))
        for row in rows:
            item=bridge.call('reset',tasks=[row['task']])['items'][0]
            native=bridge.call('infer',inputs=[item['inputs']])['outputs'][0]
            with torch.no_grad():actor,value=model(tensor([item]))
            expected=torch.cat((actor,value[:,None]),dim=1)[0]
            actual=torch.tensor([native[k] for k in ['steering_latent','thrust_logit','brake_logit','value']])
            torch.testing.assert_close(actual,expected,atol=1e-5,rtol=1e-5)
            command=actions(actor[:,0],actor[:,1:])[0];assert max(abs(command[k]-native['action'][k]) for k in command)<1e-5
            parity.append({'index':row['index'],'inputs':item['inputs'],'python_raw':expected.tolist(),'native':native})
            while item['progress']['outcome'] is None:
                if time.monotonic()>deadline:raise TimeoutError('protected evaluation budget')
                item=bridge.call('step_policy')['items'][0]
            storage.dump_json(directory/f"case-{row['index']:02}.json.gz",bridge.call('state')['traces'][0])
            results.append({'index':row['index'],'family':row['family'],'direction':row['direction'],'outcome':item['progress']['outcome'],
                            'ticks':item['progress']['elapsed_ticks'],'final_errors':item['errors']})
    atomic_json(directory/'results.json',results);atomic_json(directory/'parity.json',parity)
    return results


def main():
    p=argparse.ArgumentParser();p.add_argument('--bridge',required=True);p.add_argument('--config',type=Path,required=True)
    p.add_argument('--out',type=Path,required=True);p.add_argument('--resume',type=Path);p.add_argument('--initialize-from',type=Path)
    p.add_argument('--skip-evaluation',action='store_true');args=p.parse_args();c=storage.read_json(args.config)
    if c!=sustained.config(c['learner_seed'],c['arm'],c['preflight']):raise ValueError('not frozen sustained config')
    if bool(args.resume)==bool(args.initialize_from):raise ValueError('choose initialization or exact resume')
    if c['preflight'] and not args.skip_evaluation:raise ValueError('preflight has no scored evaluation')
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True);torch.manual_seed(c['learner_seed']);random.seed(c['learner_seed'])
    args.out.mkdir();start=time.monotonic();deadline=start+c['wall_seconds']
    atomic_json(args.out/'run-plan.json',{'config':c,'resume':str(args.resume) if args.resume else None,'initialize_from':str(args.initialize_from) if args.initialize_from else None,'evaluation':not args.skip_evaluation})
    with Bridge(args.bridge) as bridge:
        contract=identity(c,bridge);atomic_json(args.out/'contract.json',contract)
        if args.resume:model,optimizer,collector,completed,history=restore(args.resume,contract,bridge,args.out/'training-traces')
        else:
            rows=sustained.training_candidates(c);atomic_json(args.out/'training-candidates.json',rows)
            collector=CompressedCollector(bridge,rows,args.out/'training-traces');model=sustained.ResidualModel(bridge.contract['schema'])
            sustained.initialize(model,args.initialize_from,c)
            optimizer=torch.optim.Adam(model.parameters(),lr=c['learning_rate'],eps=1e-5);completed=0;history=[]
            save(args.out/'checkpoint-0000.pt.gz',contract,model,optimizer,collector,0,history)
        fixed=copy.deepcopy(model.state_dict());initial=args.out/f'policy-{completed:04}.json';model.export(initial)
        rows=sustained.candidates(c['development_task_seed'],True);atomic_json(args.out/'development-candidates.json',rows)
        baseline=evaluate(args.bridge,initial,model,rows,args.out/'evaluation-initial',deadline) if not args.skip_evaluation else []
        try:
            while completed<c['updates']:
                if shutil.disk_usage(args.out).free<512*1024**2:raise RuntimeError('retention disk reserve reached')
                rollout=collector.collect(model,c,deadline);storage.dump_torch(args.out/f'rollout-{completed+1:04}.pt.gz',rollout)
                metrics=sustained.update(model,optimizer,rollout,c);sustained.assert_parent_actor(fixed,model.state_dict());completed+=1
                metrics.update(update=completed,environment_steps=collector.steps,completed_episodes=len(collector.completed));history.append(metrics)
                save(args.out/f'checkpoint-{completed:04}.pt.gz',contract,model,optimizer,collector,completed,history)
                atomic_json(args.out/'progress.json',{'updates':history,'started':collector.started,'completed':collector.completed});print(metrics,flush=True)
            final=args.out/f'policy-{completed:04}.json';model.export(final)
            final_rows=evaluate(args.bridge,final,model,rows,args.out/'evaluation-final',deadline) if not args.skip_evaluation else []
            atomic_json(args.out/'summary.json',{'completed_updates':completed,'environment_steps':collector.steps,'started_episodes':collector.started,'total_seconds':time.monotonic()-start,
                'parameter_count':sum(p.numel() for p in model.parameters()),'trainable_parameters':sum(p.numel() for p in model.parameters() if p.requires_grad),
                'history':history,'initial_evaluation':baseline,'final_evaluation':final_rows,'promoted':False})
        except Exception as error:
            storage.dump_json(args.out/'interruption.json.gz',{'error':str(error),'completed_updates':completed,'partial_traces':bridge.call('state')['traces']});raise


if __name__=='__main__':main()
