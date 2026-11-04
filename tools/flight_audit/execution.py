"""Frozen-policy action execution comparison; no optimizer or training continuation."""
import argparse
from collections import Counter
import gzip
import json
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import identity, sha, atomic_json
from tools.flight_ppo.model import FlightModel, tensor, actions
from tools.flight_ppo import exploration

PARENT = Path('data/training-experiments/learned-flight-aligned-approach-v1-20261023')
REPEATS = 4


def episode_seed(learner, repeat, index):
    return 5310000 + learner*10000 + repeat*1000 + index


def load_frozen(parent, learner, bridge):
    run = parent/f'seed-{learner}'
    state = torch.load(run/'checkpoint-0128.pt', weights_only=True)
    if state['contract'] != identity(state['contract']['config'], bridge):
        raise ValueError('frozen policy source/runtime contract mismatch')
    assert state['update'] == 128
    assert state['contract']['config']['exploration'] == {'kind':'gaussian-latent-ar1-v1','rho':0.}
    model = FlightModel(bridge.contract['schema'])
    model.load_state_dict(state['model'], strict=True)
    model.eval(); model.requires_grad_(False)
    return model


def command(model, actor, mode):
    if mode == 'deterministic':
        return actions(actor[:,0], (actor[:,1:]>=0).float()), None
    if mode != 'sampled':raise ValueError('unknown execution mode')
    dist = exploration.distribution(model, actor, torch.zeros_like(actor), torch.zeros_like(actor),
                                    actor.new_zeros(len(actor)), 0.)
    latent = dist.sample()
    return actions(latent[:,0], (latent[:,1:]>=0).float()), latent.tolist()


def rollout(bridge, model, candidate, mode, rng_seed, deadline, tick_limit=1800):
    if tick_limit > 1800 or tick_limit < 1:raise ValueError('invalid rollout tick limit')
    torch.manual_seed(rng_seed)
    item = bridge.call('reset', tasks=[candidate['task']])['items'][0]
    rows=[]
    with torch.inference_mode():
        while item['progress']['outcome'] is None and len(rows)<tick_limit:
            if time.monotonic()>deadline:raise TimeoutError('frozen execution wall guard')
            actor,_ = model(tensor([item]))
            commands, latent = command(model,actor,mode)
            probabilities=actor[0,1:].sigmoid().clamp(1e-6,1-1e-6).tolist()
            rows.append({'actor':actor[0].tolist(), 'button_probabilities':probabilities,
                         'steering_std':float(model.log_std.clamp(-5,2).exp()[0]),
                         'latent':latent[0] if latent is not None else None, 'action':commands[0]})
            if mode == 'deterministic':
                item=bridge.call('step_policy')['items'][0]
            else:
                item=bridge.call('step', actions=commands)['items'][0]
    trace=bridge.call('state')['traces'][0]
    assert len(trace['steps'])==len(rows)
    for r,s in zip(rows,trace['steps']):
        assert r['action']['thrust']==s['action']['thrust'] and r['action']['brake']==s['action']['brake']
        assert abs(r['action']['steering']-s['action']['steering'])<1e-5
    if item['progress']['outcome']=='environment_error':raise RuntimeError('environment failure')
    return trace,rows


def metrics(candidate, trace, rows, mode, repeat, rng_seed):
    steps=trace['steps'];frames=[trace['initial']]+[s['frame'] for s in steps]
    initial=frames[0]['errors']['position'];final=frames[-1]['errors']
    entered=next((i for i,s in enumerate(steps) if s['frame']['errors']['position']<=3),None)
    near=[i for i,f in enumerate(frames[:-1]) if f['errors']['position']<=3]
    def probability_stats(indices):
        if not indices:return {'ticks':0}
        return {'ticks':len(indices),
            'mean_thrust_probability':sum(rows[i]['button_probabilities'][0] for i in indices)/len(indices),
            'mean_brake_probability':sum(rows[i]['button_probabilities'][1] for i in indices)/len(indices),
            'thrust_probability_below_half_fraction':sum(rows[i]['button_probabilities'][0]<.5 for i in indices)/len(indices),
            'executed_thrust_fraction':sum(steps[i]['action']['thrust'] for i in indices)/len(indices),
            'executed_brake_fraction':sum(steps[i]['action']['brake'] for i in indices)/len(indices),
            'mean_abs_spin':sum(abs(frames[i]['errors']['spin']) for i in indices)/len(indices),
            'speed_within_tolerance_fraction':sum(frames[i]['errors']['velocity']<=1 for i in indices)/len(indices),
            'spin_within_tolerance_fraction':sum(abs(frames[i]['errors']['spin'])<=.2 for i in indices)/len(indices)}
    outcome=steps[-1]['progress']['outcome']
    if outcome=='success':
        assert steps[-1]['progress']['dwell_ticks']==30
        assert all(s['frame']['errors']['position']<=3 and s['frame']['errors']['velocity']<=1 and abs(s['frame']['errors']['spin'])<=.200001 for s in steps[-30:])
    return {'index':candidate['index'],'recipe':candidate['recipe'],'mode':mode,'repeat':repeat,'rng_seed':rng_seed,
        'outcome':outcome,'ticks':len(steps),'initial_distance':initial,'final_errors':final,
        'minimum_distance':min(f['errors']['position'] for f in frames),
        'final_distance_reduction':initial-final['position'], 'entered_goal_radius':entered is not None,
        'first_entry_errors':steps[entered]['frame']['errors'] if entered is not None else None,
        'met_all_instantaneous_tolerances':any(s['progress']['dwell_ticks']>0 for s in steps),
        'maximum_dwell_ticks':max(s['progress']['dwell_ticks'] for s in steps),
        'collision':any(s['evidence']['collision'] for s in steps),'damage':any(s['evidence']['damage'] for s in steps),
        'all_ticks':probability_stats(range(len(rows))),'inside_radius':probability_stats(near),
        'initial_button_probabilities':rows[0]['button_probabilities'],'steering_std':rows[0]['steering_std']}


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--archive',type=Path,required=True)
    parser.add_argument('--seed',type=int,choices=[17,29,43],required=True)
    args=parser.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    out=args.archive/f'seed-{args.seed}';out.mkdir()
    (out/'traces').mkdir();(out/'policy').mkdir()
    start=time.monotonic();deadline=start+1200;records=[];ticks=0
    parent=args.archive/'inputs'
    run=parent/f'seed-{args.seed}'
    with Bridge(args.archive/'runtime/flight_ppo_bridge') as bridge:
        model=load_frozen(parent,args.seed,bridge)
        before={k:v.clone() for k,v in model.state_dict().items()}
        bridge.call('load_policy',path=str((run/'policy-0128.json').resolve()))
        batch=json.loads((run/'development-admission.json').read_text())
        candidates=[r['candidate'] for r in batch['attempts'] if r['admitted']]
        assert len(candidates)==16 and batch['seed']==4210002
        for mode,repeats in [('deterministic',[-1]),('sampled',range(REPEATS))]:
            for repeat in repeats:
                for c in candidates:
                    rng_seed=episode_seed(args.seed,repeat,c['index'])
                    trace,rows=rollout(bridge,model,c,mode,rng_seed,deadline)
                    if trace['steps'][-1]['progress']['outcome'] is None:raise RuntimeError('incomplete episode')
                    ticks+=len(rows);assert ticks<=144000
                    name=f"{mode}-r{repeat}-clear-{c['index']}"
                    with gzip.open(out/'traces'/f'{name}.json.gz','wt') as f:json.dump(trace,f,allow_nan=False)
                    with gzip.open(out/'policy'/f'{name}.json.gz','wt') as f:json.dump(rows,f,allow_nan=False)
                    record=metrics(c,trace,rows,mode,repeat,rng_seed)
                    if mode=='deterministic':
                        original=json.loads((run/'evaluation-final'/f"clear-{c['index']}.json").read_text())
                        assert trace==original,'deterministic replay differs from parent'
                        record['matches_parent_trace']=True
                    records.append(record)
                    atomic_json(out/'progress.json',{'physics_ticks':ticks,'episodes':len(records),'outcomes':dict(Counter(r['outcome'] for r in records)), 'last':{k:record[k] for k in ['mode','repeat','index','outcome']}})
        assert all(torch.equal(v,before[k]) for k,v in model.state_dict().items())
    atomic_json(out/'results.json',{'seed':args.seed,'episodes':records,'physics_ticks':ticks,
                'seconds':time.monotonic()-start,'model_unchanged':True,'optimizer_steps':0,
                'checkpoint_sha256':sha(run/'checkpoint-0128.pt')})


if __name__=='__main__':main()
