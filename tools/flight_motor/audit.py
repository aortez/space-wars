"""Native motor trajectory reconstruction, complete exposure and exact resumes."""
import argparse
import json
import math
from pathlib import Path
import struct
import time
import torch
from tools.flight_ppo.checkpoint import atomic_json
from tools.flight_ppo.tests import equal_tree
from .task import SEEDS, reward


def read(path):return json.loads(Path(path).read_text())
def f32(x):return struct.unpack('f',struct.pack('f',x))[0]
HEADING = f32(f32(5.) * f32(math.pi/180))
SPIN = f32(.2)


def qualified(errors):
    return errors['heading'] <= HEADING and errors['velocity'] <= 1 and errors['spin'] <= SPIN


def inspect(trace, cost=reward):
    assert trace['version']==1 and trace['task']['deadline_ticks']==360
    initial=trace['initial'];previous=initial;dwell=0;total=0.;max_drift=0.;first=None
    for tick,step in enumerate(trace['steps'],1):
        item=step['item'];frame=item['frame'];e=item['errors'];command=trace['command']
        difference=command['heading']-frame['angle'];angle=abs(math.atan2(math.sin(difference),math.cos(difference)))
        desired=(-math.sin(command['heading'])*command['speed'],math.cos(command['heading'])*command['speed'])
        velocity=math.hypot(frame['velocity']['x']-desired[0],frame['velocity']['y']-desired[1])
        assert abs(e['heading']-angle)<2e-6 and abs(e['velocity']-velocity)<1e-5 and e['spin']==abs(frame['spin'])
        assert frame['world_tick']==initial['frame']['world_tick']+tick
        assert all(math.isfinite(v) for v in item['inputs']) and len(item['inputs'])==18
        assert item['inputs'][-3:]==[step['action'][k] for k in ['steering','thrust','brake']]
        dwell=dwell+1 if qualified(previous['errors']) and qualified(e) else 0
        evidence=item['evidence'];outcome=None
        for key in ['ship_lost','collision','damage','boundary_escape']:
            if evidence[key]:outcome=key;break
        if outcome is None:
            if dwell>=30:outcome='success'
            elif tick>=trace['task']['deadline_ticks']:outcome='deadline'
        assert item['progress']=={'elapsed_ticks':tick,'dwell_ticks':dwell,'outcome':outcome}
        if outcome is not None:assert tick==len(trace['steps'])
        if qualified(e) and first is None:first=tick
        max_drift=max(max_drift,math.hypot(*(frame['center_of_mass'][k]-initial['frame']['center_of_mass'][k] for k in ['x','y'])))
        total+=cost(item)['total'];previous=item
    return {'ticks':len(trace['steps']),'outcome':previous['progress']['outcome'],'reward':total,'maximum_drift':max_drift,
            'first_qualified_tick':first,'maximum_dwell':max([s['item']['progress']['dwell_ticks'] for s in trace['steps']]+[0]),
            'final_errors':previous['errors']}


def physical(rows):
    assert len(rows)==32 and sorted(r['index'] for r in rows)==list(range(32))
    return {'turn_successes':sum(r['outcome']=='success' and r['family']=='turn' for r in rows),
            'left':sum(r['outcome']=='success' and r['family']=='turn' and r['direction']=='left' for r in rows),
            'right':sum(r['outcome']=='success' and r['family']=='turn' and r['direction']=='right' for r in rows),
            'immediate_holds':sum(r['outcome']=='success' and r['family']=='hold' and r['ticks']==30 for r in rows),
            'physical_failures':sum(r['outcome'] not in ['success','deadline'] for r in rows)}


def gate(initial, final):
    return {'turn_competence':final['turn_successes']>=20 and min(final['left'],final['right'])>=9,
            'learning_gain':final['turn_successes']>=initial['turn_successes']+12,
            'stationary_retention':final['immediate_holds']>=7,'physical_safety':final['physical_failures']==0}


def verify(archive):
    start=time.monotonic();rows=[];restore_ticks=0;optimizer_steps=0;evaluation_ticks=0
    for seed in SEEDS:
        run=archive/f'pilot/seed-{seed}';resume=archive/f'resumed/seed-{seed}'
        s=read(run/'summary.json');assert s['completed_updates']==64 and s['environment_steps']==65536 and s['parameter_count']==3463 and s['total_seconds']<900
        final=torch.load(run/'checkpoint-0064.pt',weights_only=True);again=torch.load(resume/'checkpoint-0064.pt',weights_only=True)
        assert equal_tree(final,again)
        assert equal_tree(torch.load(run/'rollout-0064.pt',weights_only=True),torch.load(resume/'rollout-0064.pt',weights_only=True))
        checkpoint63=torch.load(run/'checkpoint-0063.pt',weights_only=True)
        restore_ticks+=sum(len(t['steps']) for t in checkpoint63['collector']['traces'])
        optimizer_steps+=sum(h['minibatches'] for h in s['history'])+s['history'][-1]['minibatches']
        state=final['collector'];seen={e['candidate']['index'] for e in state['completed']};assert seen==set(range(32))
        counts={};training_ticks=0
        for e in state['completed']:
            inspection=inspect(read(run/f"training-traces/episode-{e['id']:06}.json"))
            assert inspection['outcome']==e['outcome'] and inspection['ticks']==e['ticks'] and abs(inspection['reward']-e['reward'])<1e-9
            counts[e['outcome']]=counts.get(e['outcome'],0)+1;training_ticks+=e['ticks']
        for t in state['traces']:training_ticks+=inspect(t)['ticks']
        assert training_ticks==65536
        evaluations={};trajectories={}
        for phase in ['initial','final']:
            results=read(run/f'evaluation-{phase}/results.json');evaluations[phase]=physical(results);trajectories[phase]=[]
            assert len(read(run/f'evaluation-{phase}/parity.json'))==32
            for row in results:
                item=inspect(read(run/f"evaluation-{phase}/case-{row['index']:02}.json"))
                assert item['outcome']==row['outcome'] and item['ticks']==row['ticks'] and item['final_errors']==row['final_errors']
                evaluation_ticks+=item['ticks'];trajectories[phase].append({**row,**item})
        checks=gate(evaluations['initial'],evaluations['final'])
        zero=torch.load(run/'checkpoint-0000.pt',weights_only=True);assert not zero['optimizer']['state']
        assert not equal_tree(zero['model'],final['model'])
        rows.append({'seed':seed,'physical':evaluations,'gates':checks,'qualified':all(checks.values()),'trajectories':trajectories,
                     'training_outcomes':counts,'training_cases_completed':sorted(seen),'started_episodes':state['started'],
                     'optimizer_steps':sum(h['minibatches'] for h in s['history']),'kl_stops':sum(h['kl_early_stop'] for h in s['history']),
                     'main_seconds':s['total_seconds']})
    result={'experiment_id':archive.name,'records':rows,'passed':sum(r['qualified'] for r in rows)>=2,
            'training_ticks':199680,'restore_ticks':restore_ticks,'optimizer_steps':optimizer_steps,'evaluation_ticks':evaluation_ticks,
            'evaluation_episodes':192,'native_python_comparisons':192,'exact_resumes':3,'report_seconds':time.monotonic()-start,
            'confirmation_used':False,'model_promoted':False,'navigation_qualified':False}
    assert restore_ticks<=4308 and optimizer_steps<=3120 and evaluation_ticks<=69120 and result['report_seconds']<300
    atomic_json(archive/'results.json',result)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--archive',type=Path,required=True);args=p.parse_args()
    torch.set_num_threads(1);verify(args.archive)
