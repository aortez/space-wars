"""Exact continuous hold-update replay and matched immediate-loss coefficient probes."""
import argparse
import copy
import gzip
import json
import math
from pathlib import Path
import random
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, identity, load
from tools.flight_ppo.model import actions, tensor
from tools.flight_ppo.ppo import gae, update
from tools.flight_ppo.reward import reward
from tools.flight_ppo.tests import equal_tree
from tools.flight_audit.signal import RecordingBridge, gradient_vector, inspect_gradients, summary

SEEDS=[17,29,43]
BEFORE=[0,32,64,127]
VARIANTS={'full':{},'no_entropy':{'entropy_weight':0.},'no_value':{'value_weight':0.},'policy_only':{'value_weight':0.,'entropy_weight':0.}}
METRICS=['mean_thrust','mean_brake','mean_abs_steering']


class AuditBridge(RecordingBridge):
    def __init__(self,*args,**kwargs):
        super().__init__(*args,**kwargs);self.setup_ticks=0
    def call(self,op,**kwargs):
        self.setup_ticks+=len(kwargs['traces']) if op=='restore' else len(kwargs['tasks']) if op=='reset' else 1 if op=='reset_at' else 0
        return super().call(op,**kwargs)


def action_metrics(actor):
    return {'mean_thrust':actor[:,1].sigmoid().mean(),'mean_brake':actor[:,2].sigmoid().mean(),
            'mean_abs_steering':actor[:,0].tanh().abs().mean()}


def probe(model,observations):
    with torch.no_grad():actor,value=model(observations)
    return {'metrics':{k:float(v) for k,v in action_metrics(actor).items()},
            'raw_actor':actor.tolist(),'actions':actions(actor[:,0],actor[:,1:]),'value':value.tolist()}


def labels(row):
    e=row['before']['errors'];after=row['after']['frame']['errors'];a=row['action']
    return {'all':True,'first_30_ticks':row['tick']<30,'near_goal':e['position']<=3,
            'within_all_tolerances':e['position']<=3 and e['velocity']<=1 and abs(e['spin'])<=0.20000000298023224,
            'low_throttle':a['thrust']<=.05,'high_throttle':a['thrust']>=.5,
            'speed_decrease':after['velocity']<e['velocity']-1e-6,'speed_increase':after['velocity']>e['velocity']+1e-6,
            'spin_decrease':after['spin']<e['spin']-1e-6,'spin_increase':after['spin']>e['spin']+1e-6}


def row_data(rows,rollout,values,normalized,pressure,config):
    compact=[];groups={}
    for i,row in enumerate(rows):
        terms=reward({'frame':row['before']},row['after'],config['gamma'],config['velocity_shaping_weight'],config['deadline_distance_weight'])
        components={}
        for name,scale,coefficient in [('position',100.,.5),('velocity',20.,config['velocity_shaping_weight']),('spin',1.,.25)]:
            p=-coefficient*math.tanh(row['before']['errors'][name]/scale)
            q=0. if row['after']['progress']['outcome'] is not None else -coefficient*math.tanh(row['after']['frame']['errors'][name]/scale)
            components[name]=config['gamma']*q-p
        assert math.isclose(sum(components.values()),terms['potential'],abs_tol=1e-12)
        item={'slot':row['slot'],'episode':row['episode'],'tick':row['tick'],'action':row['action'],
              'before_errors':row['before']['errors'],'after_errors':row['after']['frame']['errors'],
              'outcome':row['after']['progress']['outcome'],'reward':{**terms,**components},
              'advantage':float(rollout['advantages'].flatten()[i]),'normalized_advantage':float(normalized[i]),
              'critic_value':float(values[i]),'critic_target':float(rollout['returns'].flatten()[i]),
              'score_pressure':pressure[i].tolist(),'latent':rollout['ar_latents'].flatten(0,1)[i].tolist(),
              'labels':[k for k,on in labels(row).items() if on]}
        compact.append(item)
        for label in item['labels']:groups.setdefault(label,[]).append(item)
    grouped={label:{'count':len(items),
        **{k:summary([v[k] for v in items]) for k in ['advantage','normalized_advantage']},
        'score_pressure':{name:summary([v['score_pressure'][i] for v in items]) for i,name in enumerate(['steering','thrust','brake'])},
        'reward_means':{k:sum(v['reward'][k] for v in items)/len(items) for k in compact[0]['reward']}}
        for label,items in groups.items()}
    return compact,grouped


def projections(model,observations,vectors):
    actor,_=model(observations);metrics=action_metrics(actor);result={};gradients={}
    for name,q in metrics.items():
        gradient=gradient_vector(q,list(model.parameters()))
        gradients[name]=gradient
        result[name]={key:-float(gradient.dot(vectors[key])) for key in ['policy','weighted_critic','weighted_entropy']}
    return result,gradients


def case(a,budget,starts,remaining):
    seed=budget['seed'];before_n=budget['before'];after_n=budget['after'];run=a/f'inputs/seed-{seed}'
    out=a/f'seed-{seed}/update-{after_n:04}';out.mkdir(parents=True)
    started=time.monotonic();expected=torch.load(run/f'checkpoint-{after_n:04}.pt',weights_only=True)
    config=expected['contract']['config']
    with Bridge(a/'runtime/flight_ppo_bridge') as raw:
        bridge=AuditBridge(raw,limit=budget['restore_ticks']+1024,wall_seconds=min(180,remaining))
        model,optimizer,collector,completed,history=load(run/f'checkpoint-{before_n:04}.pt',identity(config,raw),bridge)
        assert completed==before_n and config['exploration']['rho']==0 and config['task_recipe']=='hold-position-v1'
        initial_model=copy.deepcopy(model.state_dict());initial_optimizer=copy.deepcopy(optimizer.state_dict())
        bridge.collector=collector
        rollout=collector.collect(model,config,bridge.deadline)
        rng=torch.get_rng_state().clone();python_rng=random.getstate();reconstructed=collector.state()
        assert equal_tree(reconstructed,expected['collector']) and len(bridge.rows)==1024
        assert bridge.ticks==budget['restore_ticks']+1024 and bridge.setup_ticks==budget['neutral_setup_ticks']
        torch.save(rollout,out/'rollout.pt');torch.save(reconstructed,out/'collector.pt')
        torch.save({'torch_rng':rng,'python_rng':python_rng},out/'post-collection-rng.pt')
        rewards=torch.tensor([reward({'frame':r['before']},r['after'],config['gamma'],config['velocity_shaping_weight'],config['deadline_distance_weight'])['total'] for r in bridge.rows]).reshape(256,4)
        dones=torch.tensor([r['after']['progress']['outcome'] is not None for r in bridge.rows],dtype=torch.float32).reshape(256,4)
        with torch.no_grad():values=torch.stack([model(x)[1] for x in rollout['observations']]);last=model(tensor(collector.items))[1]
        advantages,returns=gae(rewards,dones,values,last,config['gamma'],config['lambda'])
        assert torch.equal(advantages,rollout['advantages']) and torch.equal(returns,rollout['returns'])
        torch.save({'rewards':rewards,'dones':dones,'values':values,'final_value':last,'advantages':advantages,'returns':returns},out/'targets.pt')
        observations={'hold_starts':starts,'rollout':rollout['observations'].flatten(0,1)}
        baseline={name:probe(model,x) for name,x in observations.items()};atomic_json(out/'before-probes.json',baseline)
        gradients,vectors,batched_values,normalized,pressure=inspect_gradients(model,rollout,config,rng)
        projection={};q_gradients={}
        for name,x in observations.items():projection[name],q_gradients[name]=projections(model,x,vectors)
        vectors['observable_gradients']=q_gradients;torch.save(vectors,out/'gradient-vectors.pt')
        gradients['observable_projections']=projection
        gradients['projection_scope']='negative raw loss-gradient dot observable-gradient at the first actual minibatch; unit gradient-descent direction, not Adam attribution'
        rows,groups=row_data(bridge.rows,rollout,values.flatten(),normalized,pressure,config)
        with gzip.open(out/'rows.json.gz','wt') as f:json.dump(rows,f,allow_nan=False)
        assert equal_tree(model.state_dict(),initial_model) and equal_tree(optimizer.state_dict(),initial_optimizer)
        branches={}
        for name,overrides in VARIANTS.items():
            if time.monotonic()>bridge.deadline:raise TimeoutError('case analysis budget')
            model.load_state_dict(initial_model);optimizer.load_state_dict(copy.deepcopy(initial_optimizer))
            torch.set_rng_state(rng);random.setstate(python_rng)
            metrics=update(model,optimizer,rollout,dict(config,**overrides))
            state={'model':copy.deepcopy(model.state_dict()),'optimizer':copy.deepcopy(optimizer.state_dict()),
                   'torch_rng':torch.get_rng_state(),'python_rng':random.getstate(),'metrics':metrics,'coefficient_overrides':overrides}
            torch.save(state,out/f'{name}.pt')
            if name=='full':
                assert equal_tree(state['model'],expected['model']) and equal_tree(state['optimizer'],expected['optimizer'])
                assert torch.equal(state['torch_rng'],expected['torch_rng']) and state['python_rng']==expected['python_rng']
                assert all(metrics[k]==expected['history'][-1][k] for k in metrics) and history==expected['history'][:-1]
            probes={key:probe(model,x) for key,x in observations.items()};atomic_json(out/f'{name}-probes.json',probes)
            branches[name]={'overrides':overrides,'metrics':metrics,'after':{key:v['metrics'] for key,v in probes.items()},
                            'change':{key:{m:v['metrics'][m]-baseline[key]['metrics'][m] for m in METRICS} for key,v in probes.items()}}
        record={'seed':seed,'before_update':before_n,'after_update':after_n,'budget':budget,
                'before':{key:v['metrics'] for key,v in baseline.items()},'branches':branches,'gradients':gradients,'groups':groups,
                'exact_production_update':True,'exact_collector':True,'exact_gae':True,'action_ticks':bridge.ticks,'neutral_setup_ticks':bridge.setup_ticks,
                'diagnostic_optimizer_steps':sum(v['metrics']['minibatches'] for v in branches.values()),'seconds':time.monotonic()-started}
        assert record['seconds']<=180 and record['diagnostic_optimizer_steps']<=64
        atomic_json(out/'results.json',record);return record


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True);args=parser.parse_args();a=args.archive
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True);started=time.monotonic()
    budgets=json.loads((a/'replay-budget.json').read_text())
    assert [(r['seed'],r['before'],r['after']) for r in budgets]==[(s,n,n+1) for s in SEEDS for n in BEFORE]
    starts={}
    for seed in SEEDS:
        fixture=json.loads((a/f'inputs/seed-{seed}/parity-initial.json').read_text());items=fixture['items']
        assert len(items)==16 and all(i['frame']['errors']=={'position':0.,'velocity':0.,'spin':0.} for i in items)
        starts[seed]=tensor(items)
    records=[]
    for budget in budgets:
        remaining=1200-(time.monotonic()-started)
        if remaining<=0:raise TimeoutError('global audit budget')
        records.append(case(a,budget,starts[budget['seed']],remaining))
        atomic_json(a/'progress.json',{'cases':len(records),'action_ticks':sum(r['action_ticks'] for r in records),'optimizer_steps':sum(r['diagnostic_optimizer_steps'] for r in records)})
    result={'experiment_id':a.name,'cases':records,'selected_before_updates':BEFORE,'seeds':SEEDS,
            'action_ticks':sum(r['action_ticks'] for r in records),'neutral_setup_ticks':sum(r['neutral_setup_ticks'] for r in records),
            'diagnostic_optimizer_steps':sum(r['diagnostic_optimizer_steps'] for r in records),'seconds':time.monotonic()-started,
            'new_training_experiment':False,'new_flight_evaluation':False,'confirmation_used':False,'model_promoted':False}
    assert result['action_ticks']==44724 and result['neutral_setup_ticks']==55 and result['diagnostic_optimizer_steps']<=768 and result['seconds']<=1200
    atomic_json(a/'results.json',result)


if __name__=='__main__':main()
