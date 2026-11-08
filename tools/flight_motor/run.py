"""Bounded motor PPO using real native dynamics, export evaluation and strict resumes."""
import argparse
import json
from pathlib import Path
import platform
import random
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, save, sha
from tools.flight_ppo.ppo import update
from tools.flight_ppo.model import actions
from .model import MotorModel, ARCHITECTURE, tensor
from .task import candidates, config as declared_config
from .collector import Collector
from . import velocity, persistent


def identity(config, bridge):
    sources = {str(p):sha(p) for directory in ['tools/flight_motor','tools/flight_ppo'] for p in sorted(Path(directory).glob('*.py'))}
    return {'kind':'motor-checkpoint-v1','architecture':ARCHITECTURE,'config':config,'schema':bridge.contract['schema'],
            'sources':sources,'bridge_sha256':sha(bridge.executable),'python':platform.python_version(),'torch':str(torch.__version__),
            'threads':torch.get_num_threads(),'device':'cpu'}


def restore(path, expected, bridge, directory):
    s=torch.load(path,weights_only=True)
    if s['contract'] != expected:raise ValueError('motor checkpoint contract/runtime/source mismatch')
    model=MotorModel(expected['schema']);model.load_state_dict(s['model'])
    optimizer=torch.optim.Adam(model.parameters(),lr=expected['config']['learning_rate'],eps=1e-5);optimizer.load_state_dict(s['optimizer'])
    collector=Collector.restore(bridge,s['collector'],directory)
    if ('motor_exploration' in expected['config']) != (collector.exploration is not None):
        raise ValueError('motor exploration checkpoint state mismatch')
    torch.set_rng_state(s['torch_rng']);random.setstate(s['python_rng'])
    return model,optimizer,collector,s['update'],s['history']


def evaluate(executable, policy, model, rows, directory, deadline):
    directory.mkdir(); results=[]; parity=[]
    with Bridge(executable) as bridge:
        bridge.call('load_policy',path=str(policy.resolve()))
        for row in rows:
            item=bridge.call('reset',tasks=[row['task']])['items'][0]
            native=bridge.call('infer',inputs=[item['inputs']])['outputs'][0]
            with torch.no_grad(): actor,value=model(tensor([item]))
            expected=torch.cat((actor,value[:,None]),dim=1)[0]
            actual=torch.tensor([native[k] for k in ['steering_latent','thrust_logit','brake_logit','value']])
            torch.testing.assert_close(actual,expected,atol=1e-5,rtol=1e-5)
            command=actions(actor[:,0],actor[:,1:])[0]
            assert max(abs(command[k]-native['action'][k]) for k in command)<1e-5
            parity.append({'index':row['index'],'inputs':item['inputs'],'python_raw':expected.tolist(),'native':native})
            while item['progress']['outcome'] is None:
                if time.monotonic()>deadline:raise TimeoutError('motor evaluation wall budget')
                item=bridge.call('step_policy')['items'][0]
            trace=bridge.call('state')['traces'][0];atomic_json(directory/f"case-{row['index']:02}.json",trace)
            results.append({'index':row['index'],'family':row['family'],'direction':row['direction'],
                            'outcome':item['progress']['outcome'],'ticks':item['progress']['elapsed_ticks'],'final_errors':item['errors']})
    atomic_json(directory/'results.json',results);atomic_json(directory/'parity.json',parity)
    return results


def main():
    p=argparse.ArgumentParser();p.add_argument('--bridge',required=True);p.add_argument('--config',type=Path,required=True)
    p.add_argument('--out',type=Path,required=True);p.add_argument('--resume',type=Path);p.add_argument('--stop-after',type=int)
    p.add_argument('--initialize-from',type=Path)
    p.add_argument('--skip-evaluation',action='store_true');args=p.parse_args()
    config=json.loads(args.config.read_text())
    stage = persistent if config.get('kind') == 'motor-persistent-comparison-v1' else velocity if config.get('kind') == 'motor-velocity-v1' else None
    expected_config = persistent.config(config['learner_seed'],config['arm'],config['preflight']) if stage is persistent else stage.config(config['learner_seed']) if stage else declared_config(config['learner_seed'])
    if config != expected_config:raise ValueError('not the frozen motor config')
    if stage is persistent and config['preflight'] and not args.skip_evaluation:raise ValueError('preflight has no scored evaluation')
    if args.stop_after is not None and not 1 <= args.stop_after <= config['updates']:raise ValueError('invalid motor update limit')
    if stage and not args.resume and not args.initialize_from:raise ValueError('velocity stage requires declared parent weights')
    if args.initialize_from and (not stage or args.resume):raise ValueError('initialization and exact resume are distinct')
    make_rows = stage.candidates if stage else candidates
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True);torch.manual_seed(config['learner_seed']);random.seed(config['learner_seed'])
    args.out.mkdir();start=time.monotonic();deadline=start+config['wall_seconds']
    atomic_json(args.out/'run-plan.json',{'config':config,'resume':str(args.resume) if args.resume else None,'stop_after':args.stop_after,'evaluation':not args.skip_evaluation,'initialize_from':str(args.initialize_from) if args.initialize_from else None})
    with Bridge(args.bridge) as bridge:
        contract=identity(config,bridge);atomic_json(args.out/'contract.json',contract)
        if args.resume:model,optimizer,collector,completed,history=restore(args.resume,contract,bridge,args.out/'training-traces')
        else:
            rows=persistent.training_candidates(config) if stage is persistent else make_rows(config['train_task_seed']);atomic_json(args.out/'training-candidates.json',rows)
            collector=Collector(bridge,rows,args.out/'training-traces');model=MotorModel(bridge.contract['schema'])
            if 'motor_exploration' in config:collector.exploration=persistent.initial_state(4)
            if args.initialize_from:stage.initialize(model,args.initialize_from,config)
            optimizer=torch.optim.Adam(model.parameters(),lr=config['learning_rate'],eps=1e-5);completed=0;history=[]
            save(args.out/'checkpoint-0000.pt',contract,model,optimizer,collector,0,history)
        initial=args.out/f'policy-{completed:04}.json';model.export(initial)
        rows=make_rows(config['development_task_seed'],True);atomic_json(args.out/'development-candidates.json',rows)
        baseline=evaluate(args.bridge,initial,model,rows,args.out/'evaluation-initial',deadline) if not args.skip_evaluation else []
        end=min(config['updates'],args.stop_after or config['updates'])
        try:
            while completed<end:
                rollout=collector.collect(model,config,deadline)
                torch.save(rollout,args.out/f'rollout-{completed+1:04}.pt')
                metrics=update(model,optimizer,rollout,config,policy_terms=persistent.terms if 'motor_exploration' in config else None);completed+=1
                metrics.update(update=completed,environment_steps=collector.steps,completed_episodes=len(collector.completed));history.append(metrics)
                save(args.out/f'checkpoint-{completed:04}.pt',contract,model,optimizer,collector,completed,history)
                atomic_json(args.out/'progress.json',{'updates':history,'started':collector.started,'completed':collector.completed})
                print(json.dumps(metrics),flush=True)
            final=args.out/f'policy-{completed:04}.json';model.export(final)
            final_rows=evaluate(args.bridge,final,model,rows,args.out/'evaluation-final',deadline) if not args.skip_evaluation else []
            atomic_json(args.out/'summary.json',{'completed_updates':completed,'environment_steps':collector.steps,'started_episodes':collector.started,
                'total_seconds':time.monotonic()-start,'parameter_count':sum(p.numel() for p in model.parameters()),'history':history,
                'initial_evaluation':baseline,'final_evaluation':final_rows,'promoted':False})
        except Exception as error:
            atomic_json(args.out/'interruption.json',{'error':str(error),'completed_updates':completed,'partial_traces':bridge.call('state')['traces']});raise


if __name__=='__main__':main()
