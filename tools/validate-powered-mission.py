#!/usr/bin/env python3
"""Frozen v13 powered-capture integration, delivery and armed-match comparison."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import itertools
import json
import math
from pathlib import Path
import subprocess

spec = importlib.util.spec_from_file_location('execution', Path(__file__).with_name('probe-capture-execution.py'))
E = importlib.util.module_from_spec(spec)
spec.loader.exec_module(E)
J, T, V, C, F = E.J, E.T, E.V, E.C, E.F
MODEL = 'mission_powered_capture_v1'


def plan():
    items = []
    for delivery in ['native','shared']:
        for name, bearing, cover in [('failure-cover-on',-0.8,True),('failure-cover-off',-0.8,False),('successful-control',0.8,True)]:
            group = f'{delivery}-{name}'
            for powered in [False,True]:
                items.append(dict(group=group,name=group+('-powered' if powered else '-walking'),
                    kind='directed',delivery=delivery,seat=0,seed=42,world='value-destination',
                    bearing=bearing,cover=cover,powered=powered,seconds=180))
    for delivery in ['native','shared']:
        for world in range(2):
            namespace = f'powered-mission-integration-v1:{world}'
            seed = int.from_bytes(hashlib.sha256(namespace.encode()).digest()[:8],'little')
            for seat in [0,1]:
                group = f'{delivery}-armed-world{world}-p{seat+1}'
                for powered in ([False,True] if (world+seat)%2 == 0 else [True,False]):
                    items.append(dict(group=group,name=group+('-powered' if powered else '-walking'),
                        kind='armed',delivery=delivery,seat=seat,seed=seed,seed_namespace=namespace,
                        world='generated',cover=True,powered=powered,seconds=600))
    return items


def arguments(item):
    seat = item['seat']
    policies = ['material_mission_v10']*2
    policies[seat] = 'material_mission_v13'
    shared = item['delivery'] == 'shared'
    args = ['--seconds',str(item['seconds']),'--world',item['world'],'--seed',str(item['seed']),
        '--match','true','--mode','quiet' if item['kind']=='directed' else 'duel','--seat','0',
        '--asteroid-interval','0','--p1-policy',policies[0],'--p2-policy',policies[1],
        '--live-objective-planning','true','--live-objective-seats',('both' if item['kind']=='armed' else str(seat)) if shared else 'none',
        '--objective-graph-budget','4','--objective-query-budget','384',
        '--reuse-objective-ground',str(shared).lower(),'--objective-dependencies','routes' if shared else 'region',
        '--early-objective-routes',str(shared).lower(),
        '--evaluate-missions','true','--survey-capture-alternative','true','--survey-capture-flags','true',
        '--measure-mission-progress','true','--admit-flag-costs',str(seat),'--survey-current-neutral',str(seat),
        '--cover-response-seats',str(seat) if item['cover'] else 'none','--destination-retry-seats',str(seat),
        '--powered-capture-seats',str(seat) if item['powered'] else 'none',
        '--trace','true','--trace-capture-evidence','true']
    if item['kind'] == 'directed': args += ['--mirror','false','--flag-bearing',str(item['bearing'])]
    else: args += ['--require-finish','true']
    return args


def audit_publication(row, planning):
    p, survey = row['pilot'], row['landing_objective']
    if survey is None: return None
    assert survey['planning'] == planning and survey['actor'] == p['owner']
    assert survey['objective']['planet'] == p['planet']['index']
    assert 0 <= p['tick']-survey['tick'] <= 120
    assert survey.get('validated_tick') in [None,p['tick']]
    if row['objective_work'] is not None:
        assert row['objective_work'] == 'ready'
        assert survey['validated_tick'] == p['tick']
        evidence = row['objective_evidence']
        assert evidence and evidence['publication'] and evidence['measurement_tick'] == survey['tick']
    else: assert survey['tick'] == p['tick']
    powered = 0
    for route in survey['sites']+([survey['actual']] if survey.get('actual') else []):
        crossing = route.get('crossing')
        if crossing:
            assert planning == 'jetpack_round_trip'
            assert crossing['measured_tick'] <= p['tick'] <= crossing['launch_until_tick']
            assert crossing['plan']['planet'] == survey['objective']['planet']
            assert crossing['plan']['revision'] == survey['objective']['revision']
            powered += 1
    return dict(age=p['tick']-survey['tick'],powered=powered)


def same_corridor(a,b):
    """The existing CrossingPlan comparison, including its reversible endpoints."""
    if a['planet']!=b['planet'] or a['revision']!=b['revision']: return False
    if abs(a['cruise_radius']-b['cruise_radius'])>0.25: return False
    aa,ba=a['anchor'],b['anchor']
    if aa.keys()!=ba.keys(): return False
    if 'GroundGap' in aa:
        tolerance=1.0
        for key in ['from','to']:
            x,y=aa['GroundGap'][key],ba['GroundGap'][key]
            if not (0<=x<512 and 0<=y<512 and (abs(x-y)<=1 or abs(x-y)==511)): return False
    elif 'Vehicle' in aa:
        tolerance=0.5;x,y=aa['Vehicle'],ba['Vehicle']
        if x['index']!=y['index'] or x['form']!=y['form']: return False
        if math.dist(J.vector(x['position']),J.vector(y['position']))>0.5: return False
        if abs((x['angle']-y['angle']+math.pi)%math.tau-math.pi)>0.1: return False
    else: return False
    reverse=a['direction']!=b['direction']
    return all(math.dist(J.vector(a[key]),J.vector(b[other if reverse else key]))<=tolerance
        for key,other in [('start','destination'),('destination','start')])


def audit_corridor(launch,latest_survey):
    if launch['ground']['policy']=='ground_navigation_v12':
        forecast=launch['latest_forecast']
        assert forecast and forecast['version']==2
        assert forecast['measured_tick']<=launch['tick']<=forecast['launch_until_tick']
        assert 0<=forecast['launch_until_tick']-forecast['measured_tick']<=120
        assert same_corridor(launch['ground']['crossing']['plan'],forecast['plan'])
        return 'forecast_vehicle'
    # Existing v10/v11 return traversal can hop measured terrain gaps without
    # the separate, newer bidirectional vehicle-flight forecast.
    assert launch['ground']['policy'] in ['ground_navigation_v10','ground_navigation_v11']
    assert latest_survey and 0<=launch['tick']-latest_survey['tick']<30
    equipment=latest_survey['equipment']
    candidates=equipment['terrain_crossings']+([equipment['crossing']] if equipment['crossing'] else [])
    corridor=launch['ground']['crossing']['plan']
    assert any(same_corridor(corridor,c) for c in candidates)
    return 'existing_ground_gap' if 'GroundGap' in corridor['anchor'] else 'existing_vehicle_corridor'


def audit_launch(launch,latest_survey):
    assert launch['equipment']['charge']>=0.98
    if launch['ground']['policy']=='ground_navigation_v12': E.audit_launch(launch)
    return audit_corridor(launch,latest_survey)


def first_launch(seat,ground,seen):
    # A terrain revision can pause a flight for its next corridor survey.
    # Resuming the same crossing's Lift phase is not another charged takeoff.
    crossing=ground['crossing']
    assert crossing['goal']=='Lift' and crossing['started_tick'] is not None
    key=(seat,ground['started_tick'],crossing['started_tick'])
    if key in seen: return False
    seen.add(key)
    return True


def observe_visit(visit, row):
    p, tick = row['pilot'], row['pilot']['tick']
    planet = row['planets'][visit['planet']]
    claim = planet['claim']
    if 'source' not in visit:
        visit['source'] = dict(vehicle=p['vehicle'],transfers=p['transfers'],captures=claim['captures'],owner=claim['owner'])
    source = visit['source']
    milestones = visit['physical']
    if row.get('capture') and row['capture']['completed_tick'] is not None:
        visit['controller_completed_tick']=row['capture']['completed_tick']
    conditions = dict(landed=p['landing']['planet']==visit['planet'] and p['landing']['phase']=='landed',
        exited=p['planet']['index']==visit['planet'] and p['location']=='on_foot' and p['transfers']>source['transfers'],
        claimed=claim['owner']==p['owner'] and claim['captures']>source['captures'])
    for key, value in conditions.items():
        if value and milestones[key] is None: milestones[key] = tick
    if milestones['claimed'] is not None and p['location']=={'aboard':source['vehicle']} and p['transfers']>=source['transfers']+2:
        if milestones['boarded'] is None: milestones['boarded'] = tick
    if tick == visit['recorded']['departed_tick']:
        assert all(milestones[k] is not None for k in ['landed','exited','claimed','boarded']), 'departure without physical capture/return'
        assert milestones['landed'] <= milestones['exited'] < milestones['claimed'] <= milestones['boarded'] < tick
        assert p['location']=={'aboard':source['vehicle']}
        f=planet['motion']
        dx,dy=(p['ship']['position'][k]-f['position'][k] for k in ['x','y'])
        clearance=math.hypot(dx,dy)-planet['radius']
        # Mission handoff permits departure at 70 units, independently of local
        # controller completion or which planet is now the nearest frame.
        assert clearance>70 or (visit.get('controller_completed_tick') is not None and clearance>60)
        milestones['departed']=tick


def analyze(root, item, audit_out=None):
    report=json.loads((root/'report.json').read_text())
    assert report['physics_ok'] and not report['audit_failures']
    shared=item['delivery']=='shared'
    enabled=[False,False];enabled[item['seat']]=item['powered']
    assert report.get('powered_capture',{}).get('enabled_seats',[False,False])==enabled
    if item['powered']: assert report['powered_capture']['profile']==MODEL
    live=report['live_objective_planning']
    assert live['allowance']==dict(graph=4,physics_queries=384)
    expected_live=([0,1] if item['kind']=='armed' else [item['seat']]) if shared else []
    assert live['enabled_seats']==expected_live
    audit=F.M.audit(report)
    assert not any(audit['counts'].get(k,0) for k in ['unverified','corrected_visits','milestones_outside_visit'])
    visits={s:[dict(planet=v['planet'],selected_tick=v['selected_tick'],recorded=v,
        physical=dict.fromkeys(['landed','exited','claimed','boarded','departed'])) for v in report['metrics'][s]['visits']] for s in [0,1]}
    last={};counts=Counter();first={};publications=Counter();invalidations=Counter();launches=[];completions=set();lowest=1.0
    forecasts={};ground_surveys={};previous_goal={};max_age=0;visits_index={0:0,1:0};witnesses=[]
    launched=set();resumed=[]
    for row in F.rows(root/'capture-evidence.jsonl'):
        seat=row['seat'];p=row['pilot'];tick=p['tick'];counts[seat]+=1
        assert p['owner']==f'player_{seat+1}' and row['schema']==1
        if seat in last: assert tick==last[seat]+1
        first.setdefault(seat,tick);last[seat]=tick
        assert row['mission']['powered_capture']==enabled[seat]
        planning='jetpack_round_trip' if enabled[seat] else 'joint_round_trip'
        published=audit_publication(row,planning)
        if published:
            publications['deliveries']+=1;publications['powered_entries']+=published['powered'];max_age=max(max_age,published['age'])
        evidence=row['objective_evidence']
        if evidence and evidence['invalidated_by']: invalidations[evidence['invalidated_by']]+=1
        capture=row['capture']
        if capture: assert capture['policy']==('tactical_sortie_v12' if enabled[seat] else 'tactical_sortie_v11')
        ground=capture.get('ground') if capture else None
        j=row['jetpack']
        if j:
            assert math.isfinite(j['charge']) and 0<=j['charge']<=1
            lowest=min(lowest,j['charge'])
            if j.get('vehicle_forecast'): forecasts[seat]=j['vehicle_forecast']
            if j['surveyed']: ground_surveys[seat]=dict(tick=tick,equipment=j)
        goal=ground['goal'] if ground else None
        if goal=='jetpack_lift' and previous_goal.get(seat)!='jetpack_lift':
            launch=dict(seat=seat,tick=tick,equipment=j,latest_forecast=forecasts.get(seat),ground=ground)
            takeoff=first_launch(seat,ground,launched)
            launch['kind']=(audit_launch if takeoff else audit_corridor)(launch,ground_surveys.get(seat))
            if launch['kind']!='forecast_vehicle': launch['source_survey']=ground_surveys[seat]
            (launches if takeoff else resumed).append(launch);witnesses.append(row)
        previous_goal[seat]=goal
        if ground and ground.get('crossing') and ground['crossing']['completed_tick'] is not None:
            key=(seat,ground['crossing']['completed_tick'])
            if key not in completions: completions.add(key);witnesses.append(row)
        index=visits_index[seat]
        while index+1<len(visits[seat]) and visits[seat][index+1]['selected_tick']<=tick: index+=1
        visits_index[seat]=index
        if visits[seat]:
            visit=visits[seat][index];v=visit['recorded'];end=v['departed_tick'] or v['abandoned_tick']
            if v['selected_tick']<=tick and (end is None or tick<=end):
                before=dict(visit['physical']);observe_visit(visit,row)
                if visit['physical']!=before: witnesses.append(row)
        if item['kind']=='directed': assert row['actions'][2]['Scenario']['payload'][1:]==[0,0]
    assert set(counts)==({0,1} if item['kind']=='armed' else {item['seat']})
    assert all(n==report['elapsed_ticks'] for n in counts.values())
    for seat in counts:
        for visit in visits[seat]:
            if visit['recorded']['departed_tick'] is not None:
                assert visit['physical']['departed']==visit['recorded']['departed_tick']
    witness_path=(root if audit_out is None else audit_out)/'powered-mission-witnesses.json'
    F.D.write(witness_path,dict(schema=1,rows=witnesses))
    return dict(allocation=F.allocation_audit(root),visits=visits,visit_audit=audit['counts'],
        evidence=dict(rows=dict(counts),first_ticks=first,last_ticks=last,publications=dict(publications),
            invalidations=dict(invalidations),max_published_age=max_age,launches=launches,resumed_flights=resumed,
            completed_crossings=sorted(completions),lowest_charge=lowest),
        live_telemetry=live['telemetry'],round=report['round'],combat=report['final_combat'],
        completed_sorties=[m['completed_sorties'] for m in report['missions']],
        witness=dict(path=str(witness_path),sha256=F.E.digest(witness_path)),
        hashes={p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file() and p.name!='powered-mission-witnesses.json'})


def run(binary,out,item):
    root=out/item['name'];command=[str(binary),*arguments(item),'--out',str(root)]
    result=dict(item=item,command=command)
    try:
        with (out/(item['name']+'.log')).open('w') as log:
            subprocess.run(command,check=True,stdout=log,stderr=log,timeout=1800)
        result.update(analyze(root,item))
    except Exception as error: result['error']=repr(error)
    return result


def compare(root_a,root_b):
    a,b=[json.loads((r/'report.json').read_text()) for r in [root_a,root_b]]
    assert a['initial_world']==b['initial_world']
    first_difference=None;matched=0
    for x,y in itertools.zip_longest(F.rows(root_a/'capture-evidence.jsonl'),F.rows(root_b/'capture-evidence.jsonl')):
        if x is None or y is None: break
        assert (x['seat'],x['pilot']['tick'])==(y['seat'],y['pilot']['tick'])
        for key in ['owner','location','vehicle','transfers','ship_form','ship_health','ship','actor','planet','landing']:
            assert x['pilot'][key]==y['pilot'][key], ('physical difference before controls',key)
        if x['actions']!=y['actions']:
            first_difference=dict(seat=x['seat'],tick=x['pilot']['tick']);break
        matched+=1
    return dict(first_control_difference=first_difference,matching_control_prefix_rows=matched,
        same_final_planets=a['final_planets']==b['final_planets'],
        completed_sorties=[[m['completed_sorties'] for m in r['missions']] for r in [a,b]])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--source',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code and plan first'
    binary=args.binary.resolve(strict=True);args.out.mkdir(parents=True,exist_ok=False)
    prior=json.loads((args.source/'summary.json').read_text());assert prior['complete']
    assert prior['plan']==[list(case) for case in T.CASES]
    result=dict(schema=1,complete=False,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.E.digest(binary),runner_sha256=F.E.digest(Path(__file__)),source_summary_sha256=F.E.digest(args.source/'summary.json'),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [E,J,T,V,C,F,F.M,F.E,F.D]},
        plan=plan(),regressions={},runs={},comparisons={})
    save=lambda:F.D.write(args.out/'summary.json',result)
    save()
    try:
        for label,old in prior['runs'].items():
            command=list(old['command']);old_root=Path(command[command.index('--out')+1]);root=args.out/('retained-'+label)
            for filename,digest in old['hashes'].items(): assert F.E.digest(old_root/filename)==digest
            command[0]=str(binary);command[command.index('--out')+1]=str(root)
            print('retained-'+label,flush=True)
            with (args.out/('retained-'+label+'.log')).open('w') as log: subprocess.run(command,check=True,stdout=log,stderr=log,timeout=900)
            before,after=[json.loads((p/'report.json').read_text()) for p in [old_root,root]]
            T.report_parity(before,after,old['unchanged_report_fields'])
            streams={f:F.E.digest(root/f) for f in V.EXACT_STREAMS}
            assert all(sha==F.E.digest(old_root/f) for f,sha in streams.items())
            result['regressions'][label]=dict(command=command,unchanged_stream_sha256=streams,
                sensor_parity=C.audit_sensors(old_root,root),allocation=F.allocation_audit(root))
            save()
        # Each phase is declared before outcomes; two independent worlds may run concurrently.
        for phase in ['directed','armed']:
            items=[p for p in result['plan'] if p['kind']==phase]
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures=[pool.submit(run,binary,args.out,item) for item in items]
                for item,future in zip(items,futures):
                    print(item['name'],flush=True);result['runs'][item['name']]=future.result();save()
        for item in result['plan']:
            if item['powered']:
                group=item['group'];result['comparisons'][group]=compare(args.out/(group+'-walking'),args.out/(group+'-powered'))
        assert not any('error' in r for r in result['runs'].values()), 'one or more run audits failed'
        assert F.E.digest(binary)==result['binary_sha256']
        result['complete']=True
    except BaseException as error:
        result['error']=repr(error);raise
    finally: save()


if __name__=='__main__': main()
