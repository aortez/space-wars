"""Frozen counterfactual goal/velocity probe; no optimization or new flight trials."""
import argparse
import copy
import gzip
import json
import math
from pathlib import Path
import statistics
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.execution import load_export
from tools.flight_ppo.model import actions, flatten
from tools.flight_ppo.tests import equal_tree

SEEDS = [17,29,43]
DISTANCES = [0.,1.5,3.,6.,12.,24.,60.]
SPEEDS = [-4.,-1.,0.,.5,1.,2.,4.,8.]


def read(p):
    with (gzip.open if p.suffix=='.gz' else open)(p,'rt') as f:return json.load(f)


def f32(x):
    import struct
    return struct.unpack('f',struct.pack('f',x))[0]


def validate_schema(s):
    expected={0:'goal_com_x',1:'goal_com_y',2:'target_minus_com_velocity_x',3:'target_minus_com_velocity_y',
        4:'com_world_velocity_x',5:'com_world_velocity_y',6:'origin_world_velocity_x',7:'origin_world_velocity_y',
        13:'control_frame_velocity_x',14:'control_frame_velocity_y',16:'thrust_acceleration',
        19:'brake_acceleration',20:'brake_gain',21:'cruise_speed'}
    assert s['version']==2 and s['action_contract']=='continuous-flight-v2'
    assert all(s['self_features'][i]==name for i,name in expected.items())
    assert s['scales']['local_distance']==300 and s['scales']['velocity']==70
    assert s['ray_features'][2:]==['surface_velocity_x','surface_velocity_y']
    assert s['nearest_features'][4:]==['surface_velocity_x','surface_velocity_y']
    assert s['object_features'][7:9]==['center_velocity_x','center_velocity_y']


def intervene(inputs, goal, velocity):
    """Keep pose/spin/world fixed; shift translational velocity in every encoding."""
    if not all(math.isfinite(x) for x in [*goal,*velocity]):raise ValueError('nonfinite intervention')
    out=copy.deepcopy(inputs);own=out['self_goal'];before=inputs['self_goal']
    assert abs(before[2]+before[4])<2e-7 and abs(before[3]+before[5])<2e-7, 'stationary target required'
    new=[f32(v/70.) for v in velocity];delta=[new[i]-before[4+i] for i in range(2)]
    for i in range(2):
        own[i]=f32(goal[i]/300.);own[2+i]=-new[i];own[4+i]=new[i]
        own[6+i]=f32(before[6+i]+delta[i])
    for ray in out['rays']:
        if ray[0]:
            for i in range(2):ray[2+i]=f32(ray[2+i]-delta[i])
    if out['nearest'][0]:
        for i in range(2):out['nearest'][4+i]=f32(out['nearest'][4+i]-delta[i])
    for obj in out['objects']:
        if obj[0]:
            for i in range(2):obj[7+i]=f32(obj[7+i]-delta[i])
    return out


def grid():
    points=[('origin',0.,0.)]
    for distance in DISTANCES[1:]:
        points += [(name,distance*x,distance*y) for name,x,y in [('forward',0,1),('back',0,-1),('right',1,0),('left',-1,0)]]
    return [{'direction':name,'goal':[x,y],'forward_speed':v} for name,x,y in points for v in SPEEDS]


def forward_acceleration(inputs, command):
    own=inputs['self_goal']
    relative=[(own[6+i]-own[13+i])*70 for i in range(2)]
    speed=math.hypot(*relative);cruise=own[21]*70
    governor=max(0.,min(1.,(cruise-relative[1])/10.))
    thrust=command['thrust']*own[16]*45*governor
    braking=command['brake']*min(speed*own[20]*4,own[19]*45)
    return thrust-(braking*relative[1]/speed if speed else 0.)


def stats(values):
    return {'minimum':min(values),'median':statistics.median(values),'mean':statistics.mean(values),'maximum':max(values)}


def summarize(rows):
    lookup={(tuple(r['case']['goal']),r['case']['forward_speed']):r for r in rows if r['case']['kind']=='intervention'}
    def get(x,y,v):return lookup[((float(x),float(y)),float(v))]
    def pair(a,b):
        delta={k:a['python_action'][k]-b['python_action'][k] for k in ['steering','thrust','brake']}
        return {'delta':delta,'material_throttle_or_brake':max(abs(delta['thrust']),abs(delta['brake']))>=.05,
                'forward_acceleration_delta':a['forward_actuator_acceleration']-b['forward_actuator_acceleration']}
    near,far=get(0,3,2),get(0,12,2)
    direction=get(12,0,0)['python_action']['steering']-get(-12,0,0)['python_action']['steering']
    return {'distance_near_minus_far':pair(near,far), 'velocity_fast_minus_slow':pair(get(0,3,4),get(0,3,1)),
        'forward_distance_spans_at_speed_2':{k:max(get(0,d,2)['python_action'][k] for d in DISTANCES)-min(get(0,d,2)['python_action'][k] for d in DISTANCES) for k in ['steering','thrust','brake']},
        'right_minus_left_steering':direction,'material_direction_response':abs(direction)>=.05,
        'arrival':{f'goal_{d}_speed_{v}':{'action':get(0,d,v)['python_action'],'forward_acceleration':get(0,d,v)['forward_actuator_acceleration']} for d in [0,3] for v in [0,1]},
        'original_action':rows[0]['python_action']}


def group(records):
    return {'anchors':len(records),
        **{name:{'material_count':sum(r['summary'][name]['material_throttle_or_brake'] for r in records),
            'delta':{k:stats([r['summary'][name]['delta'][k] for r in records]) for k in ['steering','thrust','brake']},
            'forward_acceleration_delta':stats([r['summary'][name]['forward_acceleration_delta'] for r in records])}
            for name in ['distance_near_minus_far','velocity_fast_minus_slow']},
        'forward_distance_spans_at_speed_2':{k:stats([r['summary']['forward_distance_spans_at_speed_2'][k] for r in records]) for k in ['steering','thrust','brake']},
        'right_minus_left_steering':stats([r['summary']['right_minus_left_steering'] for r in records]),
        'material_direction_response_count':sum(r['summary']['material_direction_response'] for r in records),
        'correct_direction_sign_count':sum(r['summary']['right_minus_left_steering']>0 for r in records),
        'arrival':{key:{'positive_forward_acceleration_count':sum(r['summary']['arrival'][key]['forward_acceleration']>0 for r in records),
            'forward_acceleration':stats([r['summary']['arrival'][key]['forward_acceleration'] for r in records]),
            'thrust':stats([r['summary']['arrival'][key]['action']['thrust'] for r in records]),
            'brake':stats([r['summary']['arrival'][key]['action']['brake'] for r in records])}
            for key in ['goal_0_speed_0','goal_0_speed_1','goal_3_speed_0','goal_3_speed_1']}}


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--archive',type=Path,required=True)
    args=parser.parse_args();a=args.archive;start=time.monotonic();deadline=start+600
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    candidates=[r['candidate'] for r in read(a/'inputs/development-admission.json')['attempts'] if r['admitted']]
    assert len(candidates)==16
    cases=[{'kind':'original'}]+[{'kind':'intervention',**c} for c in grid()]
    assert len(cases)==201
    total_ticks=0;total_outputs=0;records=[];missing=[];base_rng=torch.get_rng_state().clone()
    for seed in SEEDS:
        with Bridge(a/'runtime/flight_ppo_bridge') as bridge, torch.random.fork_rng(devices=[]):
            validate_schema(bridge.contract['schema'])
            model=load_export(a/f'inputs/seed-{seed}/policy-0128.json',bridge.contract['schema'])
            state=torch.load(a/f'inputs/seed-{seed}/checkpoint-0128.pt',weights_only=True)
            assert equal_tree(model.state_dict(),state['model'])
            weights=copy.deepcopy(model.state_dict());rng=torch.get_rng_state().clone()
            bridge.call('load_policy',path=str((a/f'inputs/seed-{seed}/policy-0128.json').resolve()))
            anchors=[]
            for c in candidates:
                anchors.append({'kind':'start','index':c['index'],'task':c['task'],'prefix':None})
                trace=read(a/f"inputs/seed-{seed}/clear-{c['index']}.json")
                assert trace['task']['goal_velocity']=={'x':0.,'y':0.}
                first=next((i+1 for i,s in enumerate(trace['steps']) if s['frame']['errors']['position']<=3),None)
                if first:
                    prefix=dict(trace,steps=trace['steps'][:first]);assert prefix['steps'][-1]['progress']['outcome'] is None
                    anchors.append({'kind':'near','index':c['index'],'task':c['task'],'prefix':prefix})
                else:missing.append({'seed':seed,'index':c['index'],'reason':'never entered goal radius'})
            for anchor in anchors:
                if time.monotonic()>deadline:raise TimeoutError('goal-response wall budget')
                if anchor['prefix']:
                    item=bridge.call('restore',traces=[anchor['prefix']])['items'][0]
                    assert item['frame']==anchor['prefix']['steps'][-1]['frame']
                    assert item['inputs']['self_goal'][29:32]==[anchor['prefix']['steps'][-1]['action'][k] for k in ['steering','thrust','brake']]
                    replay_ticks=len(anchor['prefix']['steps']);total_ticks+=replay_ticks
                else:
                    item=bridge.call('reset',tasks=[anchor['task']])['items'][0];replay_ticks=0
                assert total_ticks<=5555
                assert item['frame']['target_velocity']=={'x':0.,'y':0.} and item['frame']['gravity']=={'x':0.,'y':0.}
                original=item['inputs'];original_copy=copy.deepcopy(original)
                inputs=[original]+[intervene(original,c['goal'],[0.,c['forward_speed']]) for c in cases[1:]]
                with torch.no_grad():actor,value=model(torch.tensor([flatten(x) for x in inputs]))
                py_raw=torch.cat((actor,value[:,None]),dim=1);commands=actions(actor[:,0],actor[:,1:])
                native=bridge.call('infer',inputs=inputs)['outputs']
                native_raw=torch.tensor([[r[k] for k in ['steering_latent','thrust_logit','brake_logit','value']] for r in native])
                torch.testing.assert_close(native_raw,py_raw,atol=1e-5,rtol=1e-5)
                command_error=max(abs(command[k]-result['action'][k]) for command,result in zip(commands,native) for k in command)
                assert command_error<=1e-5 and original==original_copy
                rows=[{'case':c,'python_raw':raw,'python_action':command,'native':result,'forward_actuator_acceleration':forward_acceleration(x,command)}
                      for c,x,raw,command,result in zip(cases,inputs,py_raw.tolist(),commands,native)]
                directory=a/f"seed-{seed}/{anchor['kind']}-{anchor['index']}";directory.mkdir(parents=True)
                atomic_json(directory/'anchor.json',{'kind':anchor['kind'],'index':anchor['index'],'item':item,'replay_ticks':replay_ticks})
                with gzip.open(directory/'inputs.json.gz','wt') as f:json.dump(inputs,f,allow_nan=False)
                atomic_json(directory/'outputs.json',rows)
                record={'seed':seed,'kind':anchor['kind'],'index':anchor['index'],'replay_ticks':replay_ticks,
                        'raw_parity_error':float((native_raw-py_raw).abs().max()),'action_parity_error':command_error,'summary':summarize(rows)}
                atomic_json(directory/'summary.json',record);records.append(record);total_outputs+=len(rows)
                atomic_json(a/'progress.json',{'anchors':len(records),'comparisons':total_outputs,'replay_ticks':total_ticks})
            assert equal_tree(weights,model.state_dict()) and torch.equal(rng,torch.get_rng_state())
    assert torch.equal(base_rng,torch.get_rng_state()) and len(records)==93 and total_ticks==5555 and total_outputs==18693
    result={'experiment_id':a.name,'anchor_count':len(records),'replay_action_ticks':total_ticks,'neutral_setup_ticks':len(records),'native_python_comparisons':total_outputs,
        'seconds':time.monotonic()-start,'missing_near_anchors':missing,'records':records,
        'groups':{str(seed):{kind:group([r for r in records if r['seed']==seed and r['kind']==kind]) for kind in ['start','near']} for seed in SEEDS},
        'maximum_raw_parity_error':max(r['raw_parity_error'] for r in records),'maximum_action_parity_error':max(r['action_parity_error'] for r in records),
        'parameters_and_rng_unchanged':True,'optimizer_steps':0,'new_flight_trials':0,'confirmation_used':False,'model_promoted':False}
    assert result['seconds']<=600
    atomic_json(a/'results.json',result)


if __name__=='__main__':main()
