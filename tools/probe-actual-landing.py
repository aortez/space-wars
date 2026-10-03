#!/usr/bin/env python3
"""Detached actual-hatch job inspection with exact live replay parity."""
import argparse
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
import csv
import importlib.util
import json
import math
from pathlib import Path
import subprocess
import struct
import traceback

spec=importlib.util.spec_from_file_location('handoff',Path(__file__).with_name('validate-covered-handoff.py'))
H=importlib.util.module_from_spec(spec);spec.loader.exec_module(H)
I,C,F=H.I,H.C,H.F
MODEL='detached_actual_request_v1'


def assert_handoff_parity(current,retained):
    # JSON object keys are strings even when the native auditor counted seats
    # using integers. Preserve every value while comparing serialized receipts.
    assert json.loads(json.dumps(current))==retained


def plan():
    return [dict(name='shared-armed-world1-p1-powered',
                 ticks=[7635,7682,7683,7803,7804,7924,7925,8045,8046,8116]),
            dict(name='shared-armed-world0-p2-powered',ticks=[18199,18200,18240,18288,18289])]


def command(old,binary,root,ticks):
    cmd=list(old['command']);assert '--probe-actual-landing-ticks' not in cmd
    cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(root)
    if ticks:cmd+=['--probe-actual-landing-seat',str(old['item']['seat']),
                   '--probe-actual-landing-ticks',','.join(map(str,ticks))]
    return cmd


def local(p,point):
    x,y=I.A.J.vector(point);cx,cy=I.A.J.vector(p['planet']['motion']['position'])
    a=p['planet']['motion']['angle'];co,si=math.cos(a),math.sin(a)
    return ((x-cx)*co+(y-cy)*si,-(x-cx)*si+(y-cy)*co)


def angular_change(source,current):
    f32=lambda x:struct.unpack('f',struct.pack('f',x))[0]
    angles=[f32(p['ship']['angle']-p['planet']['motion']['angle']) for p in [source,current]]
    return abs(f32(math.sin(f32(f32(angles[0]-angles[1])*.5))))


def stability(rows,first,last):
    generations={};invalidations=[];phase_changes=[];previous=None
    for tick,row in sorted(rows.items()):
        if not first<=tick<=last:continue
        p=row['pilot'];e=row.get('objective_evidence') or {};cap=row.get('capture') or {}
        state=(p['landing']['phase'],p['transfer'],cap.get('goal'),
               (cap.get('acquisition') or {}).get('reason'))
        if state!=previous:
            phase_changes.append(dict(tick=tick,landing_phase=state[0],transfer=state[1],
                capture_goal=state[2],reason=state[3],hull=p['ship_health'],actions=row['actions']))
        previous=state
        source_tick=e.get('request_tick')
        if source_tick not in rows or p['landing']['phase']!='landed':continue
        source=rows[source_tick]['pilot']
        if source['landing']['phase']!='landed' or not source['hatch'] or not p['hatch']:continue
        delta=dict(vehicle=math.dist(local(source,source['ship']['position']),local(p,p['ship']['position'])),
            exit=math.dist(local(source,source['hatch']),local(p,p['hatch'])),
            angular_half_sine=angular_change(source,p))
        r=generations.setdefault(e['generation'],dict(generation=e['generation'],source_tick=source_tick,
            first_observed=tick,last_observed=tick,publications=0,maximum_motion={k:0. for k in delta}))
        r['last_observed']=tick;r['publications']+=int(row.get('landing_objective') is not None)
        for k,v in delta.items():r['maximum_motion'][k]=max(r['maximum_motion'][k],v)
        if e['invalidated_by']:
            event=dict(tick=tick,generation=e['generation'],source_tick=source_tick,
                reason=e['invalidated_by'],motion=delta,row=row)
            if e['invalidated_by']=='expired':assert tick==e['measurement_tick']+121
            invalidations.append(event)
    return dict(generations=list(generations.values()),invalidations=invalidations,
        phase_changes=phase_changes,
        motion_scope='Planet-local translation reconstructed in Python double precision; angular subtraction and sine rounded to f32. Translation is descriptive near the native 0.002-unit boundary; recorded native invalidation remains authoritative.')


def audit_work(probe):
    work=probe['work'];steps=probe['steps'];cap=probe['max_steps']
    assert 0<=steps<=cap<=1_000_000 and steps==sum(work.values())
    assert set(work)=={'graph','physics_queries'} and all(v>=0 for v in work.values())
    totals=defaultdict(lambda:dict(graph=0,physics_queries=0))
    transitions=probe['transitions'];assert transitions and transitions[0]['steps']==0
    assert transitions[0]['work']==dict(graph=0,physics_queries=0)
    for index,t in enumerate(transitions):
        next_work=transitions[index+1]['work'] if index+1<len(transitions) else work
        next_steps=transitions[index+1]['steps'] if index+1<len(transitions) else steps
        assert sum(t['work'].values())==t['steps']<=next_steps<=steps
        for k in work:
            delta=next_work[k]-t['work'][k];assert delta>=0
            totals[t['phase']][k]+=delta
    # Zero-step terminal stages need no charge.
    expected={k:v for k,v in totals.items() if sum(v.values())}
    assert expected==probe['work_by_phase']
    assert {k:sum(v[k] for v in expected.values()) for k in work}==work
    stop=probe['stop'];route=probe['positive_actual']
    assert stop in ['actual_positive','complete_without_positive_actual','work_limit']
    if stop=='actual_positive':
        assert route and route['site'] is None
        assert all(route[k] and route[k]['failure'] is None and not route[k]['partial']
                   for k in ['outbound','returning'])
    else:
        assert route is None
        if stop=='work_limit':assert steps==cap
        else:assert probe['complete_survey'] is not None
    return dict(stop=stop,extra_work=work,
        minimum_additional_dispatch_ticks=max(math.ceil(work['graph']/4),math.ceil(work['physics_queries']/384)),
        final_phase=probe['final_phase'],source_measurements=probe['source_measurements'],
        final_measurements=probe['final_measurements'],final_flights=probe['final_flights'],
        phases=probe['work_by_phase'],positive_actual=route)


def audit_probe(root,item,ticks):
    document=json.loads((root/'actual-landing-probe.json').read_text())
    assert document['model']==MODEL and document['seat']==item['seat']
    assert document['requested_ticks']==ticks and not document['unreached_ticks']
    assert [r['tick'] for r in document['rows']]==ticks
    charge=defaultdict(list)
    with (root/'live-planning.csv').open() as f:
        for r in csv.DictReader(f):
            if r['task']=='landing_objective' and int(r['actor'])==item['seat']:
                charge[int(r['generation'])].append((int(r['tick']),int(r['graph']),int(r['queries'])))
    wanted=set(ticks)|{r['probe']['request_tick'] for r in document['rows'] if r['probe']}
    dense={r['pilot']['tick']:r for r in F.rows(root/'capture-evidence.jsonl')
           if r['seat']==item['seat'] and (r['pilot']['tick'] in wanted or min(ticks)<=r['pilot']['tick']<=max(ticks))}
    records=[];witnesses=[]
    for r in document['rows']:
        tick=r['tick'];row=dense[tick];p=row['pilot'];probe=r['probe']
        I.witness_observation(dict(row,initial_cover=dict(observation=r['observation'])))
        if probe is None:
            records.append(dict(tick=tick,probe=None));continue
        assert probe['actor']==item['seat'] and probe['observed_tick']==tick
        assert probe['measurement_tick']<=probe['request_tick']<=tick<=probe['measurement_tick']+120
        source=dense[probe['request_tick']]['pilot']
        assert source['landing']['phase']=='landed' and source['hatch'] is not None
        pose=probe['source_pose']
        for name,value in [('vehicle',source['ship']['position']),('exit',source['hatch'])]:
            assert math.dist(I.A.J.vector(pose[name]),local(source,value))<.00025
        for native,value in zip(pose['boarding_hatches'],source['boarding_hatches']):
            assert (native is None)==(value is None)
            if native is not None:assert math.dist(I.A.J.vector(native),local(source,value))<.00025
        assert abs(pose['angle']-(source['ship']['angle']-source['planet']['motion']['angle']))<.000001
        prior=[w for w in charge[probe['generation']] if w[0]<tick]
        assert probe['live_charged']==dict(graph=sum(w[1] for w in prior),physics_queries=sum(w[2] for w in prior))
        analysis=audit_work(probe)
        records.append(dict(tick=tick,generation=probe['generation'],source=probe['measurement_tick'],
            live_charged=probe['live_charged'],publication_deadline_tick=probe['measurement_tick']+120,
            available_dispatch_ticks=probe['measurement_tick']+120-tick,**analysis))
        witnesses.append(dict(tick=tick,source=dense[probe['request_tick']],row=row))
    path=root/'actual-landing-witnesses.json'
    motion=stability(dense,min(ticks),max(ticks))
    F.D.write(path,dict(schema=1,records=records,rows=witnesses,stability=motion))
    return dict(records=records,stability=motion,witness_sha256=F.E.digest(path),probe_sha256=F.E.digest(root/'actual-landing-probe.json'))


def run(entry,old,binary,out,enabled):
    root=out/(entry['name']+('-probe' if enabled else '-retained'))
    cmd=command(old,binary,root,entry['ticks'] if enabled else [])
    result=dict(item=old['item'],command=cmd)
    try:
        with (out/(root.name+'.log')).open('x') as log:
            subprocess.run(cmd,check=True,stdout=log,stderr=log,timeout=1800)
        result.update(C.analyze(root,old['item']))
        result['initial_cover']=I.audit_initial(root,old['item'],True)
        result['handoff']=H.audit_handoffs(root,old['item'],True)
        result['replay_parity']=C.P.replay_parity(old,result)
        assert result['initial_cover']==old['initial_cover']
        assert_handoff_parity(result['handoff'],old['handoff'])
        if enabled:result['actual_probe']=audit_probe(root,old['item'],entry['ticks'])
        else:assert not (root/'actual-landing-probe.json').exists()
        result['hashes']={p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    except Exception:result['error']=traceback.format_exc()
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior',type=Path,required=True)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(),'freeze probe and plan first'
    prior=json.loads(args.prior.read_text());assert prior['complete']
    selected=dict(runs={e['name']:prior['runs'][e['name']] for e in plan()})
    I.H.verify_inputs(selected)
    binary=args.binary.resolve(strict=True);args.out.mkdir(parents=True,exist_ok=False)
    result=dict(schema=1,complete=False,model=MODEL,
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.E.digest(binary),prior_summary=dict(path=str(args.prior),sha256=F.E.digest(args.prior)),
        runner_sha256=F.E.digest(Path(__file__)),plan=plan(),retention={},runs={})
    save=lambda:F.D.write(args.out/'summary.json',result)
    save()
    try:
        for enabled in [False,True]:
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures=[pool.submit(run,e,selected['runs'][e['name']],binary,args.out,enabled) for e in plan()]
                for entry,future in zip(plan(),futures):
                    new=future.result();(result['runs'] if enabled else result['retention'])[entry['name']]=new
                    save();assert 'error' not in new,(entry['name'],new.get('error'))
                    print(entry['name']+(': probe audited' if enabled else ': retained exactly'),flush=True)
        I.H.verify_inputs(selected)
        assert F.E.digest(args.prior)==result['prior_summary']['sha256'] and F.E.digest(binary)==result['binary_sha256']
        result['complete']=True
    except BaseException as error:
        result['error']=repr(error);raise
    finally:save()


if __name__=='__main__':main()
