"""LF-056: frozen motor policies, paired actuator exploration, no optimization."""
import argparse
import copy
import math
from pathlib import Path
import random
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json, sha
from tools.flight_ppo.model import actions
from tools.flight_ppo.tests import equal_tree
from tools.flight_motor.audit import read, qualified, HEADING, SPIN
from tools.flight_motor.task import SEEDS
from tools.flight_motor.velocity import PARENT_SHA
from tools.flight_motor.velocity_audit import task_matches
from .motor_retention import model_from_input, parity

HORIZON = 360
DEADLINE = 180
VARIANTS = {
    'baseline': {'multiplier': 1., 'rho': 0.},
    'wide4': {'multiplier': 4., 'rho': 0.},
    'wide8': {'multiplier': 8., 'rho': 0.},
    'wide8_persistent': {'multiplier': 8., 'rho': .95},
}


def candidates():
    rows = []
    starts = [('accelerate', speed, 0., sign) for speed in [6.,12.,18.] for sign in [-1,1]]
    starts += [('brake', speed, sign*speed, sign) for speed in [8.,24.] for sign in [-1,1]]
    for case, (family, level, initial_speed, sign) in enumerate(starts):
        rng = random.Random(5610001 + case); bearing = rng.uniform(-math.pi,math.pi)
        offset = sign*math.pi/6; heading = bearing+offset
        task = {'start': {'seed':5610001+case,'bearing':bearing,'altitude':150.,'heading_offset':offset,
                         'relative_velocity':{'x':-math.sin(heading)*initial_speed,'y':math.cos(heading)*initial_speed},
                         'spin':0.,'gravity':False,'planet_spin':0.},
                'heading_delta':0.,'speed':level if family=='accelerate' else 0.,
                'deadline_ticks':DEADLINE,'retention_horizon_ticks':HORIZON}
        for repeat in range(4):
            index = len(rows)
            rows.append({'index':index,'case':case,'repeat':repeat,'family':family,'level':level,
                         'initial_speed':initial_speed,'noise_seed':5620000+index,'task':copy.deepcopy(task)})
    return rows


def draws(rows):
    # Private generators; identical innovations across model seeds and variants.
    return {str(r['index']):torch.randn(HORIZON,2,generator=torch.Generator().manual_seed(r['noise_seed'])).tolist() for r in rows}


class Sampler:
    def __init__(self, variant, std):
        if variant not in VARIANTS: raise ValueError('undeclared exploration variant')
        self.variant = variant; self.std = torch.as_tensor(std,dtype=torch.float32).clone()
        if self.std.shape != (2,) or not torch.isfinite(self.std).all() or not (self.std>0).all():
            raise ValueError('invalid actuator deviation')
        self.residual = torch.zeros(2); self.tick = 0

    def step(self, actor, innovation):
        actor = torch.as_tensor(actor,dtype=torch.float32); epsilon = torch.as_tensor(innovation,dtype=torch.float32)
        if actor.shape != (3,) or epsilon.shape != (2,) or not torch.isfinite(actor).all() or not torch.isfinite(epsilon).all():
            raise ValueError('invalid exploration input')
        spec = VARIANTS[self.variant]; rho = spec['rho']
        self.residual = epsilon.clone() if self.tick == 0 else rho*self.residual + math.sqrt(1-rho*rho)*epsilon
        latent = actor.clone(); latent[1:] += self.std*spec['multiplier']*self.residual; self.tick += 1
        action = actions(latent[:1],latent[None,1:])[0]
        return {'actor':actor.tolist(),'innovation':epsilon.tolist(),'residual':self.residual.tolist(),'latent':latent.tolist(),'action':action}

    def state(self):
        return {'variant':self.variant,'std':self.std.tolist(),'residual':self.residual.tolist(),'tick':self.tick}

    @classmethod
    def restore(cls, state, variant, std, innovations):
        obj = cls(variant,std)
        if not 0 <= state['tick'] <= HORIZON: raise ValueError('invalid sampler cursor')
        for epsilon in innovations[:state['tick']]: obj.step([0.,0.,0.],epsilon)
        if obj.state() != state: raise ValueError('sampler continuation mismatch')
        return obj


def actor_of(bridge, item):
    out = bridge.call('infer',inputs=[item['inputs']])['outputs'][0]
    return [out[k] for k in ['steering_latent','thrust_logit','brake_logit']]


def exposure(item, command):
    frame = item['frame']; h = command['heading']; v = frame['velocity']
    forward = -math.sin(h)*v['x'] + math.cos(h)*v['y']
    lateral = abs(-math.cos(h)*v['x'] - math.sin(h)*v['y'])
    heading_ok = item['errors']['heading'] <= HEADING and item['errors']['spin'] <= SPIN
    useful = heading_ok and abs(lateral) <= 1. and .5*command['speed'] <= forward <= 1.5*command['speed'] and command['speed']>0
    return {'forward':forward,'lateral':lateral,'heading_ok':heading_ok,'useful':useful}


def inspect(trace):
    task = trace['task']; assert trace['version']==2 and task['deadline_ticks']==DEADLINE and task['retention_horizon_ticks']==HORIZON
    previous = trace['initial']; command = trace['command']; prev_motion = exposure(previous,command)
    assert previous['retention']=={'first_success_tick':None,'first_exit_tick':None}
    dwell = 0; first = exit_tick = None; useful_dwell = max_useful = 0; useful_first = None
    heading_ticks = 0; useful_ticks = 0; motion = []; thrust = []; brake = []
    for tick, step in enumerate(trace['steps'],1):
        item = step['item']; frame = item['frame']; e = item['errors']; m = exposure(item,command)
        delta = command['heading']-frame['angle']; angle = abs(math.atan2(math.sin(delta),math.cos(delta)))
        desired = (-math.sin(command['heading'])*command['speed'],math.cos(command['heading'])*command['speed'])
        velocity = math.hypot(frame['velocity']['x']-desired[0],frame['velocity']['y']-desired[1])
        assert abs(e['heading']-angle)<2e-6 and abs(e['velocity']-velocity)<2e-5 and e['spin']==abs(frame['spin'])
        assert frame['world_tick']==trace['initial']['frame']['world_tick']+tick
        assert len(item['inputs'])==18 and all(math.isfinite(v) for v in item['inputs'])
        assert item['inputs'][-3:]==[step['action'][k] for k in ['steering','thrust','brake']]
        dwell = dwell+1 if qualified(previous['errors']) and qualified(e) else 0
        if first is None and dwell>=30:first=tick
        if first is not None and not qualified(e) and exit_tick is None:exit_tick=tick
        outcome = None
        for key in ['ship_lost','collision','damage','boundary_escape']:
            if item['evidence'][key]:outcome=key;break
        if outcome is None and tick>=HORIZON:outcome='success' if first is not None and first<=DEADLINE and exit_tick is None else 'deadline'
        assert item['progress']=={'elapsed_ticks':tick,'dwell_ticks':dwell,'outcome':outcome}
        assert item['retention']=={'first_success_tick':first,'first_exit_tick':exit_tick}
        if outcome is not None:assert tick==len(trace['steps'])
        heading_ticks += int(prev_motion['heading_ok'] and m['heading_ok'])
        useful_dwell = useful_dwell+1 if prev_motion['useful'] and m['useful'] else 0
        useful_ticks += int(prev_motion['useful'] and m['useful']); max_useful=max(max_useful,useful_dwell)
        if useful_dwell>=30 and useful_first is None:useful_first=tick
        thrust.append(step['action']['thrust']);brake.append(step['action']['brake']);motion.append(m)
        previous=item;prev_motion=m
    n=len(trace['steps']);assert n>0
    return {'ticks':n,'outcome':previous['progress']['outcome'],'motor_acquired_tick':first,'motor_first_exit_tick':exit_tick,
            'motor_retained':previous['progress']['outcome']=='success','exposed':useful_first is not None,'first_exposure_tick':useful_first,
            'longest_exposure_intervals':max_useful,'useful_intervals':useful_ticks,'heading_intervals':heading_ticks,
            'maximum_forward_speed':max(m['forward'] for m in motion),'maximum_lateral_speed':max(m['lateral'] for m in motion),
            'maximum_heading_degrees':max(math.degrees(s['item']['errors']['heading']) for s in trace['steps']),
            'final_errors':previous['errors'],'final_forward_speed':motion[-1]['forward'],
            'thrust_mean':sum(thrust)/n,'brake_mean':sum(brake)/n,'thrust_above_point2':sum(t>.2 for t in thrust),
            'thrust_above_point5':sum(t>.5 for t in thrust),'brake_below_point1':sum(b<.1 for b in brake),
            'strong_thrust_released_brake':sum(t>.5 and b<.1 for t,b in zip(thrust,brake)),
            'overspeed_intervals':sum(m['forward']>1.5*command['speed'] for m in motion) if command['speed'] else 0}


def flight(a, seed):
    start=time.monotonic();deadline=start+600;out=a/f'flight/seed-{seed}';out.mkdir()
    rows=read(a/'tasks/cases.json');noise=read(a/'tasks/draws.json');assert rows==candidates()
    rng=torch.get_rng_state().clone();records=[];comparisons=[];snapshots=[]
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        assert sha(a/f'inputs/seed-{seed}/checkpoint-0064.pt')==PARENT_SHA[seed]
        model=model_from_input(a,seed,bridge.contract['schema']);weights=copy.deepcopy(model.state_dict())
        std=model.log_std.clamp(-5,2).exp()[1:].detach().tolist()
        bridge.call('load_policy',path=str((a/f'inputs/seed-{seed}/policy-0064.json').resolve()))
        for variant in VARIANTS:
            directory=out/variant;directory.mkdir()
            for row in rows:
                index=row['index'];sampler=Sampler(variant,std);innovations=noise[str(index)]
                item=bridge.call('reset',tasks=[row['task']])['items'][0];samples=[]
                comparisons.append({'variant':variant,'index':index,'phase':'initial',**parity(bridge,model,[item['inputs']])})
                while item['progress']['outcome'] is None:
                    if time.monotonic()>deadline:raise TimeoutError('motor exploration flight budget')
                    sample=sampler.step(actor_of(bridge,item),innovations[sampler.tick]);samples.append(sample)
                    item=bridge.call('step',actions=[sample['action']])['items'][0]
                    if index==0 and sampler.tick==180:
                        snapshot={'variant':variant,'index':index,'sampler':sampler.state(),'trace':bridge.call('state')['traces'][0],
                                  'input_policy_sha256':sha(a/f'inputs/seed-{seed}/policy-0064.json'),
                                  'runtime_sha256':sha(a/'runtime/flight_motor_bridge'),'draws_sha256':sha(a/'tasks/draws.json')}
                        atomic_json(directory/'sampler-0180.json',snapshot);snapshots.append(snapshot)
                comparisons.append({'variant':variant,'index':index,'phase':'final',**parity(bridge,model,[item['inputs']])})
                trace=bridge.call('state')['traces'][0];atomic_json(directory/f'case-{index:02}.json',trace)
                atomic_json(directory/f'samples-{index:02}.json',samples)
                records.append({**row,'variant':variant,**inspect(trace)})
            # Paired continuation of the first acceleration case. This is not training.
            snapshot=read(directory/'sampler-0180.json');assert snapshot==snapshots[-1]
            assert snapshot['input_policy_sha256']==sha(a/f'inputs/seed-{seed}/policy-0064.json')
            assert snapshot['runtime_sha256']==sha(a/'runtime/flight_motor_bridge') and snapshot['draws_sha256']==sha(a/'tasks/draws.json')
            item=bridge.call('restore',traces=[snapshot['trace']])['items'][0]
            sampler=Sampler.restore(snapshot['sampler'],variant,std,noise['0']);expected=read(directory/'samples-00.json')
            while item['progress']['outcome'] is None:
                if time.monotonic()>deadline:raise TimeoutError('motor exploration continuation budget')
                sample=sampler.step(actor_of(bridge,item),noise['0'][sampler.tick]);assert sample==expected[sampler.tick-1]
                item=bridge.call('step',actions=[sample['action']])['items'][0]
            trace=bridge.call('state')['traces'][0];assert trace==read(directory/'case-00.json');atomic_json(directory/'continued-case-00.json',trace)
        assert equal_tree(weights,model.state_dict()) and torch.equal(rng,torch.get_rng_state())
    atomic_json(out/'parity.json',comparisons)
    atomic_json(out/'results.json',{'seed':seed,'records':records,'seconds':time.monotonic()-start,
        'parameters_rng_unchanged':True,'native_python_comparisons':len(comparisons),'exact_continuations':4,
        'evaluation_ticks':sum(r['ticks'] for r in records),'restore_ticks':720,'continuation_ticks':720,'neutral_setups':164})


def seed_gate(rows, baseline):
    accel=[r for r in rows if r['family']=='accelerate'];stop=[r for r in rows if r['family']=='brake']
    assert len(accel)==24 and len(stop)==16
    counts={str(v):sum(r['exposed'] for r in accel if r['level']==v) for v in [6.,12.,18.]}
    exposed=sum(r['exposed'] for r in accel);gain=exposed-sum(r['exposed'] for r in baseline if r['family']=='accelerate')
    heading=sum(r['heading_intervals'] for r in accel)/(24*HORIZON)
    failures=sum(r['outcome'] not in ['success','deadline'] for r in rows)
    checks={'exposure':exposed>=8 and min(counts.values())>=2,'gain':gain>=6,'heading':heading>=.9,'physical_safety':failures==0}
    return {'passed':all(checks.values()),'checks':checks,'exposed':exposed,'exposed_by_speed':counts,'exposure_gain':gain,
            'heading_fraction':heading,'physical_failures':failures,
            'brake_acquired_on_time':sum(r['motor_acquired_tick'] is not None and r['motor_acquired_tick']<=DEADLINE for r in stop),
            'brake_retained':sum(r['motor_retained'] for r in stop),'motor_tracking_retained':sum(r['motor_retained'] for r in accel)}


def verify(a):
    start=time.monotonic();noise=read(a/'tasks/draws.json');rows=read(a/'tasks/cases.json');assert noise==draws(rows) and rows==candidates()
    records=[];evaluation_ticks=0
    for seed in SEEDS:
        out=a/f'flight/seed-{seed}';r=read(out/'results.json')
        assert r['seconds']<600 and r['parameters_rng_unchanged'] and r['native_python_comparisons']==320 and r['exact_continuations']==4
        assert len(read(out/'parity.json'))==320 and len(r['records'])==160
        state=torch.load(a/f'inputs/seed-{seed}/checkpoint-0064.pt',weights_only=True);std=state['model']['log_std'].clamp(-5,2).exp()[1:].tolist()
        for record in r['records']:
            index=record['index'];variant=record['variant'];directory=out/variant
            trace=read(directory/f'case-{index:02}.json');assert task_matches(trace['task'],rows[index]['task'])
            assert all(record[k]==v for k,v in inspect(trace).items())
            samples=read(directory/f'samples-{index:02}.json');assert len(samples)==len(trace['steps'])
            sampler=Sampler(variant,std)
            for i,(sample,step) in enumerate(zip(samples,trace['steps'])):
                assert sample==sampler.step(sample['actor'],noise[str(index)][i]) and sample['action']==step['action']
                assert sample['latent'][0]==sample['actor'][0]
            evaluation_ticks+=record['ticks']
        baseline=[v for v in r['records'] if v['variant']=='baseline']
        gates={variant:seed_gate([v for v in r['records'] if v['variant']==variant],baseline) for variant in VARIANTS}
        records.append({**r,'gates':gates})
    passing=[v for v in VARIANTS if v!='baseline' and sum(r['gates'][v]['passed'] for r in records)>=2]
    result={'experiment_id':a.name,'records':records,'passing_variants':passing,'passed':bool(passing),
            'training_ticks':0,'optimizer_steps':0,'new_policy_exports':0,'new_training_checkpoints':0,'sampler_snapshots':12,
            'evaluation_episodes':480,'evaluation_ticks':evaluation_ticks,'restore_ticks':2160,'continuation_ticks':2160,
            'neutral_setup_ticks':492,'native_python_comparisons':960,'exact_continuations':12,
            'confirmation_used':False,'model_promoted':False,'navigation_qualified':False,'seconds':time.monotonic()-start}
    assert evaluation_ticks<=172800 and result['seconds']<300;atomic_json(a/'results.json',result)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--flight',type=int,choices=SEEDS);args=p.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.flight:flight(args.archive,args.flight)
    else:verify(args.archive)
