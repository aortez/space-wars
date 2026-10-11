#!/usr/bin/env python3
"""Frozen forecast diagnostics and four complete high-terrain candidate games."""
import argparse
import copy
import importlib.util
from itertools import zip_longest
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec = importlib.util.spec_from_file_location('probe', Path(__file__).with_name('probe-high-ledge.py'))
H = importlib.util.module_from_spec(spec); spec.loader.exec_module(H)
A, L, ROOT, P, I, B, D = H.A, H.L, H.ROOT, H.P, H.I, H.B, H.D
PROFILE = 'bounded_high_terrain_flight_v1'
SOURCE = ROOT/'target/crossing-arrival/v1/summary.json'
ORIGINAL = ROOT/'target/live-claim-sequences/v1/summary.json'
PROBE = ROOT/'target/high-ledge-probe/v1/summary.json'
OWN = ('tools/validate-terrain-flight.py', 'tools/tests/test_terrain_flight.py',
       'docs/terrain-flight-forecast-plan.md')
NEW = ('scenarios/spacewars/src/surface_sortie/jetpack/forecast/terrain.rs',)
RECEIPTS = tuple('target/high-ledge-probe/v1/review-extracts/'+p for p in (
    'high-ledge-probe.json','high-ledge-flight-22575.jsonl','high-ledge-flight-22965.jsonl'))


def inputs():
    return dict(H.inputs(), **{p:P.digest(ROOT/p) for p in (*OWN,*NEW,*RECEIPTS)})


def jobs(source, binary, out):
    result = []
    for name in (A.TARGET, *A.ORDER):
        diagnostic = not result
        prior = source['runs'][name]; cmd = list(prior['command'])
        key = 'forecast-probe' if diagnostic else name
        cmd[0] = str(binary); cmd[cmd.index('--out')+1] = str(out/'raw'/key)
        cmd += ['--probe-high-ledge',H.SPEC,'--probe-terrain-forecast','true'] if diagnostic else ['--terrain-flight-forecast','true']
        result.append(dict(key=key, prior=name, diagnostic=diagnostic,
            item=dict(prior['item'],stage='bounded_terrain_flight'), command=cmd))
    return result


def verify(plan, out, reaudit=False):
    assert plan['profile'] == PROFILE
    current=inputs(); assert current.keys()==plan['inputs'].keys()
    changed={p for p in current if current[p]!=plan['inputs'][p]}
    assert not changed or reaudit and changed <= set(OWN[:2])
    sources={}
    for key,record in plan['sources'].items():
        assert P.digest(record['path'])==record['sha256']
        sources[key]=json.loads(Path(record['path']).read_text()); assert sources[key]['complete']
        binary=sources[key]['binary']; assert P.digest(binary['path'])==binary['sha256']
    assert plan['jobs']==jobs(sources['arrival'],Path(plan['binary']['path']),out)
    assert P.digest(plan['binary']['path'])==plan['binary']['sha256']
    return sources


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    out.mkdir(parents=True,exist_ok=False)
    for name in ('raw','logs','archives','inputs'): (out/name).mkdir()
    target=out/'surface_mission_soak'; shutil.copy2(binary,target); target.chmod(0o555)
    source=json.loads(SOURCE.read_text())
    plan=dict(schema=1,profile=PROFILE,inputs=inputs(),fresh_games=0,default_promotion=False,
        runtime_source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        sources={k:dict(path=str(p),sha256=P.digest(p)) for k,p in [('arrival',SOURCE),('original',ORIGINAL),('probe',PROBE)]},
        binary=dict(path=str(target),sha256=P.digest(target)),jobs=jobs(source,target,out))
    verify(plan,out); I.write(out/'plan.json',plan)
    print('Frozen one observational replay and four complete candidate games.',flush=True)


def project(row):
    row=copy.deepcopy(row)
    jetpack=row['observation']['local']['combat']['recovery']['jetpack']
    if jetpack: jetpack.pop('terrain_flight',None)
    return row


def comparison(before,after):
    result=dict(common_rows=0,first_observation=None,first_action=None,first_native=None,first_behavior=None)
    for a,b in zip_longest(before,after):
        if a is None or b is None: break
        assert (a['tick'],a['seat'])==(b['tick'],b['seat']); result['common_rows']+=1
        values={'first_observation':a['observation']!=b['observation'],
            'first_action':a['actions']!=b['actions'],
            'first_native':L.X.R.pilot(a)!=L.X.R.pilot(b),
            'first_behavior':project(a)!=project(b)}
        for key,changed in values.items():
            if changed and result[key] is None: result[key]=dict(tick=b['tick'],seat=b['seat'],before=a,after=b)
    return result


def terrain_audit(rows):
    surveys=[]; milestones=[]; previous={}; approvals={}; launches={}
    for row in rows:
        tick,seat=row['tick'],row['seat']; p=L.X.R.pilot(row)
        o=row['observation']['local']['combat']['recovery']; j=o.get('jetpack') or {}
        survey=j.get('terrain_flight')
        if survey:
            assert 0<=survey['graph_work']<=8192 and 0<=survey['query_work']<=8192
            assert len(survey['attempts'])<=24 and j['surveyed']
            f=survey['forecast']; surveys.append(dict(tick=tick,seat=seat,**survey))
            if f:
                assert f['measured_tick']==tick and f['launch_until_tick']==tick+30
                assert f['plan']['planet']==p['planet']['index'] and f['plan']['revision']==p['planet']['revision']
                nodes={n['id']:n['position'] for n in o['ground']['nodes']}
                assert [nodes[n] for n in f['nodes']]==[f['plan']['start'],f['plan']['destination']]
                for leg in f['flights']:
                    assert 0<leg['seconds']<=12 and 0<=leg['burn_seconds']<=2.790001 and 0<=leg['arrival_speed']<=7
                approvals[seat]=f
        recovery=p.get('recovery') or {}; mission=row['mission']
        claim=p['planet'].get('claim') or {}
        key=dict(form=p['ship_form'],location=p['location'],planet=p['planet']['index'],claim_owner=claim.get('owner'),
            rebuilds=recovery.get('rebuilds',0),completed_recoveries=mission['completed_recoveries'])
        if previous.get(seat)!=key:
            milestones.append(dict(tick=tick,seat=seat,state=key,pilot=p)); previous[seat]=key
        ground=(mission.get('recovery') or {}).get('ground') or {}
        crossing=ground.get('crossing') or {}; plan=crossing.get('plan')
        if not plan or 'GroundGap' not in plan['anchor']: continue
        radius=lambda v: math.hypot(v['x'],v['y'])
        if plan['cruise_radius']<=min(radius(plan['start']),radius(plan['destination']))+10: continue
        identity=f"{seat}:{ground['started_tick']}:{crossing['started_tick']}"
        if identity not in launches and crossing['goal']=='Lift' and any(
            H.movement(a,seat) and a['Scenario']['payload'][4] for a in row['actions']):
            f=approvals[seat]
            assert f['measured_tick']<=tick<=f['launch_until_tick'] and j['charge']>=.98
            assert f['plan']['anchor']==plan['anchor']
            assert [plan['start'],plan['destination']] in ([f['plan']['start'],f['plan']['destination']],[f['plan']['destination'],f['plan']['start']])
            launches[identity]=dict(tick=tick,seat=seat,ground_deadline=ground['started_tick']+5400,
                forecast=f,plan=plan,minimum_charge=j['charge'],completed_tick=None)
        if identity in launches:
            flight=launches[identity]; flight['minimum_charge']=min(flight['minimum_charge'],j['charge'])
            # The controller may expose a final interruption at the boundary;
            # only applied powered flight must stay inside all limits.
            if ground['goal'] in ('jetpack_lift','jetpack_cross','jetpack_land'):
                assert j['charge']>=.05 and tick-flight['tick']<720 and tick<=flight['ground_deadline']
            if crossing['completed_tick']==tick:
                flight['completed_tick']=tick; assert A.footing(p,plan)['distance']<1
    return dict(surveys=surveys,launches=launches,milestones=milestones)


def probe_audit(root):
    probe=json.loads((root/'high-ledge-probe.json').read_text())
    retained=json.loads((ROOT/'target/high-ledge-probe/v1/review-extracts/high-ledge-probe.json').read_text())
    results=[]
    for new,old in zip(probe['results'],retained['results'],strict=True):
        assert new['tick']==old['tick'] and new['observation']==old['observation']
        assert new['control_exact_ticks']==old['control_exact_ticks'] and new['execution']==old['execution']
        actual=old['execution']; current=None; launch_forecast=None; checked=0; measurements=[]
        name=f"high-ledge-flight-{new['tick']}.jsonl"
        for a,b in zip_longest(D.rows(ROOT/'target/high-ledge-probe/v1/review-extracts'/name),D.rows(root/name)):
            assert a is not None and b is not None
            forecast=b.pop('terrain_forecast'); assert a==b, 'forecast changed a retained native flight'
            checked+=1
            if forecast:
                assert forecast['graph_work']<=8192 and forecast['query_work']<=8192
                current=forecast['forecast']; measurements.append(dict(tick=b['observation']['tick'],survey=forecast))
            if b['observation']['tick']==actual['launched_tick']: launch_forecast=current
        f=launch_forecast
        same=f is not None and f['plan']==old['selected']['plan'] and f['measured_tick']<=actual['launched_tick']<=f['launch_until_tick']
        results.append(dict(tick=new['tick'],survey=new['terrain_forecast'],same_native_plan=same,
            launch_forecast=launch_forecast,flight_rows_exact=checked,measurements=measurements,
            actual_launch_tick=actual['launched_tick'],actual_completion_tick=actual['last_tick'],
            actual_minimum_charge=actual['lowest_charge'],control_exact_ticks=new['control_exact_ticks']))
    assert len(results)==2
    return dict(results=results,both_native_plans_forecast=all(r['same_native_plan'] for r in results))


def run_case(job,plan,sources,out,reaudit):
    key=job['key']; prior=sources['arrival']['runs'][job['prior']]
    root=out/'raw'/key; oldroot=out/'inputs'/key; log=out/'logs'/(key+'.log'); marker=out/(key+'-raw.json')
    result=copy.deepcopy(job)
    if marker.exists():
        assert reaudit
        raw=json.loads(marker.read_text()); assert raw['command']==job['command'] and P.digest(log)==raw['log_sha256']
        if not root.exists(): B.unpack(json.loads((out/(key+'-result.json')).read_text())['archive'],root)
        result.update(hashes=raw['hashes'],log_sha256=raw['log_sha256'],reused_raw=True)
    else:
        assert not root.exists() and not log.exists(), 'partial games are never retried'
        with log.open('x') as stream: subprocess.run(job['command'],stdout=stream,stderr=stream,check=True,timeout=1800)
        result.update(hashes=I.raw_hashes(root),log_sha256=P.digest(log)); I.write(marker,result)
    assert all(P.digest(root/p)==sha for p,sha in result['hashes'].items())
    report=json.loads((root/'report.json').read_text())
    result.update(L.Q.analyze(root,job,root))
    result['stopping_rule']=L.X.audit_switch(D.rows(root/'trace.jsonl'),report,job['item'])
    result['defense']=L.C.A.audit(root,job['item'],report)
    result['clearance']=L.C.audit(root,job['item'],report)
    result['native']=L.S.native_audit(root,report,job['item'])
    result['observer']=L.observer_audit(D.rows(root/'impact.jsonl'),D.rows(root/'trace.jsonl'),report,report['seat'])
    ground=A.ground_audit(D.rows(root/'trace.jsonl'),report['elapsed_ticks']); I.write(root/'ground-arrival-audit.json',ground)
    result['ground']=dict(sha256=P.digest(root/'ground-arrival-audit.json'),attempts=len(ground['attempts']),
        completed=sum(a['completed_tick'] is not None for a in ground['attempts'].values()))
    terrain=terrain_audit(D.rows(root/'trace.jsonl')); I.write(root/'terrain-flight-audit.json',terrain)
    result['terrain']=dict(sha256=P.digest(root/'terrain-flight-audit.json'),surveys=len(terrain['surveys']),
        approvals=sum(s['forecast'] is not None for s in terrain['surveys']),launches=terrain['launches'],
        milestones=terrain['milestones'])
    if job['diagnostic']: result['probe']=probe_audit(root)
    L.extract(prior['archive'],oldroot,('trace.jsonl','report.json','sensors.jsonl','live-planning.csv'))
    delta=comparison(D.rows(oldroot/'trace.jsonl'),D.rows(root/'trace.jsonl')); I.write(root/'baseline-comparison.json',delta)
    result['comparison']={k:(dict(tick=v['tick'],seat=v['seat']) if isinstance(v,dict) else v) for k,v in delta.items()}
    result['exact_streams']=[p for p,sha in prior['hashes'].items() if p not in ('report.json','sensors.jsonl','live-planning.csv') and result['hashes'][p]==sha]
    if job['diagnostic']:
        projected=dict(result,hashes={p:result['hashes'][p] for p in prior['hashes']})
        # parity locates the replay through its command's --out argument.
        result['parity']=L.C.parity(prior,projected,oldroot)
    oldreport=json.loads((oldroot/'report.json').read_text()); normalized=copy.deepcopy(report); normalized.pop('terrain_flight_forecast',None)
    result['retained_non_timing_report']=D.timing_free(oldreport)==D.timing_free(normalized)
    result['retained_physics']=delta['first_native'] is None and result['elapsed_ticks']==prior['elapsed_ticks'] and result['players']==prior['players']
    result['sequence']=L.sequence(root,report,job['item']['seat'])
    assert all(P.digest(root/p)==sha for p,sha in result['hashes'].items())
    result['audited']=True
    suffix='-'+P.digest(__file__)[:12] if reaudit else ''
    result['archive']=B.pack(root,out/'archives'/(key+suffix+'.tar.gz'))
    I.write(out/(key+'-result.json'),result)
    print(key,'audited:',result['elapsed_ticks'],'ticks; terrain launches',len(terrain['launches']),flush=True)
    return result


def decision(runs,sources):
    target=runs[A.TARGET]; actor=target['players'][1]
    original=sources['original']['runs'][A.TARGET]['players'][1]
    arrival=sources['arrival']['runs'][A.TARGET]['players'][1]
    completed=[f for f in target['terrain']['launches'].values() if f['seat']==1 and f['completed_tick'] is not None]
    milestones=[m for m in target['terrain']['milestones'] if m['seat']==1]
    after=min((f['completed_tick'] for f in completed),default=10**20)
    claim=next((m for m in milestones if m['tick']>=after and m['state']['planet']==0 and m['state']['claim_owner']=='player_2'),None)
    rebuild=next((m for m in milestones if claim and m['tick']>=claim['tick'] and m['state']['rebuilds']>0),None)
    boarded=next((m for m in milestones if rebuild and m['tick']>=rebuild['tick'] and m['state']['form']=='ship' and isinstance(m['state']['location'],dict) and 'aboard' in m['state']['location']),None)
    controls=all(runs[n]['retained_physics'] for n in A.ORDER if n!=A.TARGET)
    recovery=bool(boarded) and actor['completed_recoveries']>arrival['completed_recoveries']
    survival=actor['outcome']==original['outcome'] and actor['pilot_deaths']<=original['pilot_deaths'] and actor['ships_lost']<=original['ships_lost']
    forecast=runs['forecast-probe']['probe']['both_native_plans_forecast']
    return dict(decision='advance_to_broader_validation' if controls and recovery and survival and forecast else 'not_qualified',
        forecast_matches_native=forecast,controls_retained=controls,complete_recovery=recovery,survival_retained=survival,
        native_receipt_ticks=dict(claim=claim and claim['tick'],rebuild=rebuild and rebuild['tick'],boarding=boarded and boarded['tick']),
        arrival_before=arrival,original_before=original,after=actor,default_promotion=False)


def execute(path,reaudit):
    out=path.parent; plan=json.loads(path.read_text()); sources=verify(plan,out,reaudit)
    target=out/'summary.json'
    if target.exists():
        assert reaudit
        backup=out/('summary-before-reaudit-'+P.digest(target)[:12]+'.json'); assert not backup.exists(); shutil.copy2(target,backup)
    summary=dict(schema=1,profile=PROFILE,complete=False,inputs=inputs(),binary=plan['binary'],
        runtime_source_commit=plan['runtime_source_commit'],plan_sha256=P.digest(path),reaudit=reaudit,runs={})
    I.write(target,summary)
    try:
        for job in plan['jobs']:
            summary['runs'][job['key']]=run_case(job,plan,sources,out,reaudit); I.write(target,summary)
        verify(plan,out,reaudit); summary.update(complete=True,screen=decision(summary['runs'],sources))
    except BaseException:
        summary['error']=traceback.format_exc(); raise
    finally: I.write(target,summary)
    print('Complete:',summary['screen']['decision'],flush=True)


if __name__=='__main__':
    assert __debug__
    parser=argparse.ArgumentParser(description=__doc__); sub=parser.add_subparsers(dest='action',required=True)
    p=sub.add_parser('plan'); p.add_argument('--out',type=Path,required=True); p.add_argument('--binary',type=Path,required=True)
    p=sub.add_parser('run'); p.add_argument('--plan',type=Path,required=True); p.add_argument('--reaudit',action='store_true')
    args=parser.parse_args()
    if args.action=='plan': freeze(args.out.resolve(),args.binary.resolve())
    else: execute(args.plan.resolve(),args.reaudit)
