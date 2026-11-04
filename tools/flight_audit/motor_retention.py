"""Frozen-policy motor retention and matched-state heading-response audit."""
import argparse
import copy
import math
from pathlib import Path
import time
import torch
from tools.flight_ppo.bridge import Bridge
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.model import actions
from tools.flight_ppo.tests import equal_tree
from tools.flight_motor.model import MotorModel,tensor
from tools.flight_motor.audit import read,qualified
from tools.flight_motor.task import SEEDS

HORIZON=900
GRID=list(range(-45,46))
ANCHORS=[0,1,24]


def model_from_input(a,seed,schema):
    state=torch.load(a/f'inputs/seed-{seed}/checkpoint-0064.pt',weights_only=True)
    with torch.random.fork_rng(devices=[]):
        model=MotorModel(schema);model.load_state_dict(state['model']);model.eval();model.requires_grad_(False)
    policy=read(a/f'inputs/seed-{seed}/policy-0064.json')
    assert policy['observation_schema']==schema
    assert policy['weights']=={k:{'shape':list(v.shape),'values':v.flatten().tolist()} for k,v in model.state_dict().items()}
    return model


def parity(bridge,model,inputs):
    saved=copy.deepcopy(inputs)
    native=bridge.call('infer',inputs=inputs)['outputs']
    with torch.no_grad():actor,value=model(torch.tensor(inputs,dtype=torch.float32))
    expected=torch.cat((actor,value[:,None]),dim=1)
    actual=torch.tensor([[n[k] for k in ['steering_latent','thrust_logit','brake_logit','value']] for n in native])
    torch.testing.assert_close(actual,expected,atol=1e-5,rtol=1e-5)
    commands=actions(actor[:,0],actor[:,1:])
    error=max(abs(c[k]-n['action'][k]) for c,n in zip(commands,native) for k in c)
    assert error<=1e-5 and inputs==saved
    return {'inputs':inputs,'native':native,'python_raw':expected.tolist(),'python_actions':commands,
            'raw_error':float((actual-expected).abs().max()),'action_error':error}


def prefix_matches(trace,parent):
    assert trace['version']==2 and parent['version']==1
    task=copy.deepcopy(trace['task']);assert task.pop('retention_horizon_ticks')==HORIZON
    assert task==parent['task'] and trace['command']==parent['command']
    initial=copy.deepcopy(trace['initial']);initial.pop('retention');assert initial==parent['initial']
    assert len(trace['steps'])>=len(parent['steps'])
    for old,new in zip(parent['steps'],trace['steps']):
        projected=copy.deepcopy(new);projected['item'].pop('retention')
        projected['item']['progress']['outcome']=old['item']['progress']['outcome']
        assert projected==old
    first=trace['steps'][len(parent['steps'])-1]['item']['retention']['first_success_tick']
    if parent['steps'][-1]['item']['progress']['outcome']=='success':assert first==len(parent['steps'])
    else:assert first is None


def inspect(trace):
    assert trace['version']==2
    task=trace['task'];horizon=task['retention_horizon_ticks'];assert horizon==HORIZON and task['deadline_ticks']==360
    initial=trace['initial'];previous=initial;dwell=0;first=None;exit_tick=None;post=[];max_drift=0.
    assert initial['retention']=={'first_success_tick':None,'first_exit_tick':None}
    for tick,step in enumerate(trace['steps'],1):
        item=step['item'];frame=item['frame'];e=item['errors'];command=trace['command']
        difference=command['heading']-frame['angle'];angle=abs(math.atan2(math.sin(difference),math.cos(difference)))
        desired=(-math.sin(command['heading'])*command['speed'],math.cos(command['heading'])*command['speed'])
        velocity=math.hypot(frame['velocity']['x']-desired[0],frame['velocity']['y']-desired[1])
        assert abs(e['heading']-angle)<2e-6 and abs(e['velocity']-velocity)<1e-5 and e['spin']==abs(frame['spin'])
        assert frame['world_tick']==initial['frame']['world_tick']+tick
        assert len(item['inputs'])==18 and all(math.isfinite(v) for v in item['inputs'])
        assert item['inputs'][-3:]==[step['action'][k] for k in ['steering','thrust','brake']]
        dwell=dwell+1 if qualified(previous['errors']) and qualified(e) else 0
        if first is None and dwell>=30:first=tick
        if first is not None and not qualified(e) and exit_tick is None:exit_tick=tick
        if first is not None and tick>first:post.append(e)
        outcome=None
        for key in ['ship_lost','collision','damage','boundary_escape']:
            if item['evidence'][key]:outcome=key;break
        if outcome is None and tick>=horizon:
            outcome='success' if first is not None and first<=task['deadline_ticks'] and exit_tick is None else 'deadline'
        assert item['progress']=={'elapsed_ticks':tick,'dwell_ticks':dwell,'outcome':outcome}
        assert item['retention']=={'first_success_tick':first,'first_exit_tick':exit_tick}
        if outcome is not None:assert tick==len(trace['steps'])
        max_drift=max(max_drift,math.hypot(*(frame['center_of_mass'][k]-initial['frame']['center_of_mass'][k] for k in ['x','y'])))
        previous=item
    return {'ticks':len(trace['steps']),'outcome':previous['progress']['outcome'],'first_success_tick':first,'first_exit_tick':exit_tick,
            'acquired_on_time':first is not None and first<=task['deadline_ticks'],
            'retained':previous['progress']['outcome']=='success','post_acquisition_ticks':len(post),
            'maximum_post_errors':{k:max(e[k] for e in post) for k in ['heading','velocity','spin']} if post else None,
            'maximum_drift':max_drift,'final_errors':previous['errors'],
            'final_signed_heading_degrees':math.degrees(math.atan2(previous['inputs'][0],previous['inputs'][1]))}


def flight(a,seed):
    start=time.monotonic();deadline=start+600;out=a/f'flight/seed-{seed}';out.mkdir();records=[];comparisons=[]
    rng=torch.get_rng_state().clone()
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        model=model_from_input(a,seed,bridge.contract['schema']);weights=copy.deepcopy(model.state_dict())
        bridge.call('load_policy',path=str((a/f'inputs/seed-{seed}/policy-0064.json').resolve()))
        for row in read(a/'tasks/retention.json'):
            index=row['index'];item=bridge.call('reset',tasks=[row['task']])['items'][0]
            comparisons.append({'index':index,'phase':'initial',**parity(bridge,model,[item['inputs']])})
            while item['progress']['outcome'] is None:
                if time.monotonic()>deadline:raise TimeoutError('motor retention flight budget')
                item=bridge.call('step_policy')['items'][0]
            comparisons.append({'index':index,'phase':'final',**parity(bridge,model,[item['inputs']])})
            trace=bridge.call('state')['traces'][0];atomic_json(out/f'case-{index:02}.json',trace)
            parent=read(a/f'inputs/seed-{seed}/reference/case-{index:02}.json');prefix_matches(trace,parent)
            records.append({'index':index,'family':row['family'],'direction':row['direction'],
                            'parent_success':parent['steps'][-1]['item']['progress']['outcome']=='success',**inspect(trace)})
        # One declared active-trace restart per model, after tick 450 of the stationary case.
        original=read(out/'case-24.json');assert len(original['steps'])==HORIZON
        prefix=copy.deepcopy(original);prefix['steps']=prefix['steps'][:450]
        item=bridge.call('restore',traces=[prefix])['items'][0];assert item==prefix['steps'][-1]['item']
        while item['progress']['outcome'] is None:
            if time.monotonic()>deadline:raise TimeoutError('motor retention continuation budget')
            item=bridge.call('step_policy')['items'][0]
        replay=bridge.call('state')['traces'][0];assert replay==original;atomic_json(out/'continued-case-24.json',replay)
        assert equal_tree(weights,model.state_dict()) and torch.equal(rng,torch.get_rng_state())
    atomic_json(out/'parity.json',comparisons)
    result={'seed':seed,'records':records,'seconds':time.monotonic()-start,'parameters_rng_unchanged':True,
            'parent_prefixes_exact':32,'native_python_comparisons':64,'exact_native_continuation':True,
            'evaluation_ticks':sum(r['ticks'] for r in records),'repeated_continuation_ticks':450,'restore_ticks':450,'neutral_setups':33}
    assert result['seconds']<600;atomic_json(out/'results.json',result)


def heading_input(inputs,degrees):
    out=list(inputs);angle=math.radians(degrees)
    out[:2]=torch.tensor([math.sin(angle),math.cos(angle)],dtype=torch.float32).tolist()
    return out


def response_summary(record):
    values=record['native'][1:];samples=[{'degrees':d,'action':n['action']} for d,n in zip(GRID,values)]
    roots=[]
    for left,right in zip(samples,samples[1:]):
        if left['action']['steering']*right['action']['steering']<0:roots.append([left['degrees'],right['degrees']])
    roots.extend([[s['degrees'],s['degrees']] for s in samples if s['action']['steering']==0])
    return {'zero_heading_action':samples[GRID.index(0)]['action'],'steering_zero_brackets_degrees':roots,
            'original_heading_degrees':math.degrees(math.atan2(record['inputs'][0][0],record['inputs'][0][1])),
            'original_action':record['native'][0]['action'],'samples':samples}


def responses(a):
    start=time.monotonic();records=[];rng=torch.get_rng_state().clone()
    with Bridge(a/'runtime/flight_motor_bridge') as bridge:
        for seed in SEEDS:
            model=model_from_input(a,seed,bridge.contract['schema']);weights=copy.deepcopy(model.state_dict())
            bridge.call('load_policy',path=str((a/f'inputs/seed-{seed}/policy-0064.json').resolve()))
            for index in ANCHORS:
                trace=read(a/f'inputs/seed-17/reference/case-{index:02}.json')
                for phase,item in [('initial',trace['initial']),('late',trace['steps'][-1]['item'])]:
                    x=item['inputs'];inputs=[list(x)]+[heading_input(x,d) for d in GRID]
                    result=parity(bridge,model,inputs)
                    if seed==17:
                        expected=(trace['steps'][0]['action'] if phase=='initial' else read(a/f'flight/seed-17/case-{index:02}.json')['steps'][360]['action'])
                        assert result['native'][0]['action']==expected
                    record={'seed':seed,'anchor_case':index,'anchor_phase':phase,**result};record['summary']=response_summary(record);records.append(record)
                    if time.monotonic()-start>300:raise TimeoutError('motor response budget')
            assert equal_tree(weights,model.state_dict())
    assert torch.equal(rng,torch.get_rng_state())
    atomic_json(a/'responses.json',{'records':records,'native_python_comparisons':1656,'parameters_rng_inputs_unchanged':True,
                                  'new_physics_ticks':0,'original_action_checks':6,'seconds':time.monotonic()-start})


def seed_gate(records):
    turn=[v for v in records if v['family']=='turn'];hold=[v for v in records if v['family']=='hold']
    assert len(turn)==24 and len(hold)==8
    left=sum(v['retained'] and v['direction']=='left' for v in turn);right=sum(v['retained'] and v['direction']=='right' for v in turn)
    failures=sum(v['outcome'] not in ['success','deadline'] for v in records)
    lost=[v['index'] for v in records if v['parent_success'] and not v['retained']]
    stationary=sum(v['retained'] and v['first_success_tick']==30 for v in hold)
    return {'turns_retained':left+right,'left':left,'right':right,'stationary_retained':stationary,'physical_failures':failures,
            'lost_parent_successes':lost,'passed':left+right>=20 and min(left,right)>=9 and stationary>=7 and not failures and not lost}


def verify(a):
    start=time.monotonic();records=[]
    for seed in SEEDS:
        r=read(a/f'flight/seed-{seed}/results.json');assert r['parent_prefixes_exact']==32 and r['exact_native_continuation'] and r['parameters_rng_unchanged']
        for row in r['records']:
            trace=read(a/f"flight/seed-{seed}/case-{row['index']:02}.json")
            prefix_matches(trace,read(a/f"inputs/seed-{seed}/reference/case-{row['index']:02}.json"))
            assert all(row[k]==v for k,v in inspect(trace).items())
        records.append({**r,'gate':seed_gate(r['records'])})
    response=read(a/'responses.json');assert response['native_python_comparisons']==1656 and response['parameters_rng_inputs_unchanged']
    result={'experiment_id':a.name,'records':records,'passed':sum(v['gate']['passed'] for v in records)>=2,
            'response_summaries':[{k:v[k] for k in ['seed','anchor_case','anchor_phase','summary']} for v in response['records']],
            'evaluation_episodes':96,'evaluation_ticks':sum(v['evaluation_ticks'] for v in records),'continuation_ticks':1350,
            'restore_ticks':1350,'neutral_setup_ticks':99,'native_python_comparisons':1848,'parent_prefixes_exact':96,
            'exact_native_continuations':3,'training_ticks':0,'optimizer_steps':0,'new_checkpoints':0,
            'confirmation_used':False,'model_promoted':False,'navigation_qualified':False,'seconds':time.monotonic()-start}
    assert result['evaluation_ticks']<=86400 and result['seconds']<300;atomic_json(a/'results.json',result)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);p.add_argument('--flight',type=int,choices=SEEDS);p.add_argument('--responses',action='store_true');args=p.parse_args()
    torch.set_num_threads(1);torch.use_deterministic_algorithms(True)
    if args.flight:flight(args.archive,args.flight)
    elif args.responses:responses(args.archive)
    else:verify(args.archive)
