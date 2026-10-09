#!/usr/bin/env python3
"""Validate bounded forecast selection before native replacement construction."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('local_forecast',Path(__file__).with_name('probe-rebuild-local-forecast.py'))
N=importlib.util.module_from_spec(spec);spec.loader.exec_module(N)
ROOT,P,I,B,D,L=N.ROOT,N.P,N.I,N.B,N.D,N.L
same_native=N.same_native
PRIOR=ROOT/'target/rebuild-local-forecast/v1'
PROFILE='native_rebuild_forecast_selection_v1'
S=N.N.R.S
R,Q,F,C,H,W,A=S.R,S.Q,S.F,S.C,S.H,S.W,S.A
MODES={'control_handoff':('control_handoff',False),'control_coarse':('control_coarse',False),
    'selection_handoff':('control_handoff',True),'selection_coarse':('control_coarse',True)}
OWN=('tools/validate-rebuild-forecast-selection.py','tools/tests/test_rebuild_forecast_selection.py','docs/rebuild-forecast-selection-plan.md')
NEW=('scenarios/spacewars/src/surface_sortie/rebuild_placement/selection.rs',)
CHANGED={*OWN,*NEW,'scenarios/spacewars/src/surface_sortie/rebuild_placement.rs',
    'scenarios/spacewars/src/surface_sortie/rebuild_placement/native_forecast.rs',
    'scenarios/spacewars/src/surface_sortie/rebuild_placement/local_forecast.rs',
    'scenarios/spacewars/src/surface_sortie/recovery.rs','scenarios/spacewars/src/surface_sortie.rs',
    'scenarios/spacewars/src/physics.rs','crates/spacewars-ai/tests/surface_recovery.rs',
    'crates/spacewars-ai/examples/support/rebuild_replay.rs'}


def inputs():return dict(N.inputs(),**{p:P.digest(ROOT/p) for p in (*OWN,*NEW)})


def command(prior,binary,out,mode):
    base,enabled=MODES[mode]
    cmd=list(prior['commands'][base]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    assert cmd[cmd.index('--rebuild-support-alignment')+1]=='false'
    assert cmd[cmd.index('--rebuild-round-foot-probe')+1]=='true'
    at=cmd.index('--rebuild-local-forecast');assert cmd[at+1]=='true';del cmd[at:at+2]
    assert '--rebuild-forecast-selection' not in cmd
    return cmd+['--rebuild-forecast-selection',str(enabled).lower()]


def check_inputs(expected,current,reaudit=False):
    assert expected.keys()==current.keys()
    changed={p for p in expected if expected[p]!=current[p]}
    assert not changed or reaudit and changed<=set(OWN[:2]),changed


def verify(plan,out,reaudit=False):
    check_inputs(plan['inputs'],inputs(),reaudit)
    for name in ('plan','summary'):
        assert P.digest(PRIOR/(name+'.json'))==plan['prior_'+name+'_sha256']
    assert P.digest(plan['tape']['path'])==plan['tape']['sha256']
    assert P.digest(plan['binary']['path'])==plan['binary']['sha256']
    prior=json.loads((PRIOR/'plan.json').read_text())
    assert set(plan['commands'])==set(MODES) and plan['horizon_ticks']==120 and plan['start_delay_ticks']==40
    for name,cmd in plan['commands'].items():
        assert cmd==command(prior,Path(plan['binary']['path']),out/'raw'/name,name)


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());old=json.loads((PRIOR/'summary.json').read_text())
    assert old['complete'] and old['screen']['all_paths_retained']
    current=inputs();assert prior['inputs'].keys()<=current.keys()
    changed={p for p in current if prior['inputs'].get(p)!=current[p]};assert changed<=CHANGED,changed
    out.mkdir(parents=True,exist_ok=False)
    for name in ('logs','raw','archives'):(out/name).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,changed_inputs=sorted(changed),binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        prior_plan_sha256=P.digest(PRIOR/'plan.json'),prior_summary_sha256=P.digest(PRIOR/'summary.json'),
        commands={n:command(prior,frozen,out/'raw'/n,n) for n in MODES},horizon_ticks=120,start_delay_ticks=40,activation_tick=23767,seat=1,
        task_start=16820,end=29421,fresh_games=0,default_promotion=False,bounds=prior['bounds'],build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen four prefixes and eight continuations; selector alone differs within each pair.',flush=True)


def audit_orientation(root,fork,radial):
    reports={};errors=[]
    for row in D.rows(root/('rebuild-'+fork+'.jsonl')):
        report=row['pilots'][1]['recovery']['placement']
        if report is not None and report['tick'] not in reports:
            projected=dict(report)
            if report.get('anchor') is not None:projected['standing']=report['anchor']
            error=Q.check_direction(projected,radial,23767)
            reports[report['tick']]=report
            if error is not None:errors.append(error)
        if row['observation']['rebuild'] is not None:
            for a in row['observation']['rebuild']['attempts']:
                if a['placement'] is not None:Q.check_direction(a['placement'],radial,23767)
    path=root/(fork+'-selection-orientation-audit.json')
    I.write(path,dict(reports=list(reports.values()),maximum_direction_error=max(errors,default=None)))
    return dict(reports=len(reports),maximum_direction_error=max(errors,default=None),sha256=P.digest(path))


def check_forecast(event,work):
    f=event['forecast'];assert f['start_delay']==40 and f['launch_tick']==f['tick']+40
    assert f['max_steps_per_call']==4 and f['horizon_ticks']==120 and f['step_nanoseconds']==16666667
    assert f['input']['captured_before_construction'] and not f['input']['live_vehicle_available']
    assert not f['future_actions_read'] and not f['production_qualified']
    assert f['scope']=='selected_planet_geometry_and_gravity_ephemeris'
    assert f['max_planets']==32 and f['max_planet_colliders']==128 and f['max_shape_parts']==16384
    assert not any(f[k] for k in ('terrain_updates','other_actors','damage_and_hazards'))
    assert 0<=f['warmup_steps']<=40 and 0<=f['steps']<=120
    assert sum(w['steps'] for w in work)==sum(c['steps'] for c in f['chunks'])==f['warmup_steps']+f['steps']
    assert all(0<=w['steps']<=4 for w in work)
    assert [w['tick'] for w in work]==list(range(f['tick']+1,f['tick']+1+len(work)))
    assert all(math.isfinite(c['elapsed_ms']) and c['elapsed_ms']>=0 for c in f['chunks'])
    samples=f['samples']
    if samples:
        assert f['warmup_steps']==40 and len(samples)==f['steps']+1
        assert [s['tick'] for s in samples]==list(range(f['launch_tick'],f['launch_tick']+len(samples)))
        assert all(s['settled']==N.settled(s) for s in samples)
        if f['stop']=='horizon':assert N.events(samples)['first_settled']==f['first_settled_tick']
    value=N.prediction(f['first_settled_tick'],f['steps'],f['stop'])
    assert value==f['settles_within_horizon']==event['prediction']
    if event['accepted']:
        assert value is True and f['warmup_steps']==40 and f['steps']==120
        assert event['tick']==f['launch_tick'] and len(work)==40
        r=event['revalidation'];assert r['tick']==event['tick'] and r['launch_matches']
        assert r['report']['tick']==event['tick'] and r['report']['revision']==f['report']['revision']
        assert r['report']['selected_offset']==event['offset'] and r['report']['anchor'] is not None
        a=next(a for a in r['report']['attempts'] if a['offset']==event['offset'])
        assert a['rejection'] is None and a['route']['failure'] is None and a['route']['length']<=24
    return dict(capture_tick=f['tick'],launch_tick=f['launch_tick'],decision_tick=event['tick'],offset=event['offset'],
        accepted=event['accepted'],prediction=value,warmup_steps=f['warmup_steps'],steps=f['steps'],stop=f['stop'],
        forecast_events=N.events(samples),timing=f['timing'],maximum_chunk_ms=max((c['elapsed_ms'] for c in f['chunks']),default=0))


def audit_selection(root,fork):
    events=list(D.rows(root/('rebuild-selection-'+fork+'.jsonl')))
    work={};measured=[];accepted={};searches={};previous_tick=0
    for e in events:
        assert e['seat']==1 and 23767<=e['tick']<=29421 and e['tick']>=previous_tick;previous_tick=e['tick']
        kind=e['kind']
        if kind=='update':
            assert math.isfinite(e['elapsed_ms']) and e['elapsed_ms']>=0
            continue
        if kind=='started':
            offsets=e['offsets'];assert 1<=len(offsets)<=26 and len(offsets)==len(set(offsets)) and offsets[0]==e['preferred']
            assert e['target_tick']==e['tick']+40
            searches[e['tick']]=dict(offsets=offsets,next=1,terminal=False)
        else:
            search=searches[e['search_tick']];assert not search['terminal']
            if kind=='candidate':
                assert e['offset']==search['offsets'][search['next']];search['next']+=1
            elif kind=='work':
                work.setdefault(e['forecast_tick'],[]).append(e)
            elif kind=='evaluated':
                m=check_forecast(e,work.get(e['forecast']['tick'],[]));measured.append(m)
                if e['accepted']:
                    accepted[e['tick']]=e;search['terminal']=True
            elif kind in ('cancelled','invalidated','exhausted'):
                search['terminal']=True
            else:raise AssertionError(kind)
    assert all(sum(w['steps'] for ws in work.values() for w in ws if w['tick']==tick)<=4 for tick in {w['tick'] for ws in work.values() for w in ws})
    actual={tick:[] for tick in accepted};builds=[];previous_builds=None;revision_changes={tick:[] for tick in accepted}
    for row in D.rows(root/('rebuild-'+fork+'.jsonl')):
        p=row['pilots'][1];tick=row['tick'];n=p['recovery']['rebuilds']
        if previous_builds is not None and n>previous_builds:
            assert tick in accepted and n==previous_builds+1
            assert same_native(p['recovery']['placement'],accepted[tick]['revalidation']['report'])
            builds.append(tick)
        previous_builds=n
        for start,e in accepted.items():
            if start<=tick<=start+120:
                actual[start].append(dict(tick=tick,pilot=p,landing_diagnostics=row['landing_diagnostics']))
                if p['planet']['revision']!=e['revalidation']['report']['revision']:revision_changes[start].append(tick)
    assert set(builds)==accepted.keys()
    for m in measured:
        if not m['accepted']:continue
        start=m['decision_tick'];samples=accepted[start]['forecast']['samples'];rows=actual[start]
        assert rows and rows[0]['tick']==start
        comparison=N.compare(samples,[N.reference(r) for r in rows])
        m.update(actual=comparison,terrain_revision_changes=revision_changes[start],
            matches_observed_settling=comparison['events']['first_settled'] is not None if len(rows)==121 or any(N.N.settled(r) for r in rows) else None)
    path=root/(fork+'-selection-audit.json');I.write(path,dict(events=events,measurements=measured,actual_samples=actual))
    compact=[]
    for m in measured:
        c=dict(m)
        if 'actual' in c:c['actual']={k:v for k,v in c['actual'].items() if k!='differences'}
        compact.append(c)
    return dict(searches=len(searches),candidate_queries=sum(e['kind']=='candidate' for e in events),
        maximum_update_ms=max((e['elapsed_ms'] for e in events if e['kind']=='update'),default=0),
        cancellations=sum(e['kind']=='cancelled' for e in events),invalidations=sum(e['kind']=='invalidated' for e in events),
        exhausted=sum(e['kind']=='exhausted' for e in events),pending=sum(not s['terminal'] for s in searches.values()),
        measurements=compact,physics_work=sum(w['steps'] for ws in work.values() for w in ws),sha256=P.digest(path))


def execute(path,reaudit=False):
    out=path.parent;plan=json.loads(path.read_text());verify(plan,out,reaudit)
    if (out/'summary.json').exists():
        assert reaudit
        backup=out/('summary-before-reaudit-'+P.digest(out/'summary.json')[:12]+'.json')
        assert not backup.exists();shutil.copy2(out/'summary.json',backup)
    prior=json.loads((PRIOR/'summary.json').read_text())
    summary=dict(schema=1,profile=PROFILE,complete=False,plan_sha256=P.digest(path),reaudit=reaudit,runs={},
        auditor_inputs={p:P.digest(ROOT/p) for p in OWN[:2]})
    try:
        for name,cmd in plan['commands'].items():
            root=out/'raw'/name;log=out/'logs'/(name+'.log');marker=out/(name+'-raw.json')
            if marker.exists():
                assert reaudit
                run=json.loads(marker.read_text());assert run['command']==cmd and run['log_sha256']==P.digest(log)
                if not root.exists():B.unpack(json.loads((out/(name+'-result.json')).read_text())['archive'],root)
                run['reused_raw']=True
            else:
                assert not root.exists() and not log.exists(),'partial simulations are never retried'
                with log.open('x') as stream:subprocess.run(cmd,stdout=stream,stderr=stream,check=True,timeout=1800)
                run=dict(command=cmd,hashes=I.raw_hashes(root),log_sha256=P.digest(log),report=json.loads((root/'rebuild-replay.json').read_text()))
                I.write(marker,run)
            assert all(P.digest(root/p)==h for p,h in run['hashes'].items())
            base,enabled=MODES[name];previous=prior['runs'][base]
            if not enabled:
                assert run['hashes']=={p:h for p,h in previous['hashes'].items() if not p.startswith('rebuild-local-forecast-')}
                missing=[p for p in previous['archive']['files'] if not (root/p).exists()]
                if missing:L.extract(previous['archive'],root,missing)
                assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
                run['retained_files']=len(previous['archive']['files']);assert run['retained_files']==(47 if base=='control_handoff' else 44)
                run['audits_reused_after_raw_retention']=True;run['timed_forecasts_restored_from_archive']=True
            else:
                run['prefix']=R.audit_prefix(root,plan['tape']['path'],run['report']);assert run['prefix']['verified']
                assert run['hashes']['rebuild-prefix.jsonl']==previous['hashes']['rebuild-prefix.jsonl']
                run['forks']={f:R.audit_fork(root,f,run['report']) for f in ('recorded','live')}
                previous_name=S.MODES[base][0];radial_case,precise=A.MODES[previous_name]
                source,radial=Q.MODES[radial_case];mode,hold,recheck=F.MODES[source]
                run['search']={f:W.S.audit_search(root,f,True) for f in ('recorded','live')}
                run['execution']={f:W.audit_execution(root,f,mode) for f in ('recorded','live')}
                run['footing']={f:H.audit_footing(root,f,hold) for f in ('recorded','live')}
                run['contacts']={f:C.audit_contacts(root,f,True) for f in ('recorded','live')}
                run['recheck']={f:F.audit_recheck(root,f,hold,recheck) for f in ('recorded','live')}
                run['orientation']={f:audit_orientation(root,f,radial) for f in ('recorded','live')}
                run['arrival']={f:A.audit_arrival(root,f,precise) for f in ('recorded','live')}
                run['selection']={f:audit_selection(root,f) for f in ('recorded','live')}
            suffix='-reaudit-'+P.digest(ROOT/OWN[0])[:12] if reaudit else ''
            run['archive']=B.pack(root,out/'archives'/(name+suffix+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,run['report']['forks']['live']['reason'],run['report']['forks']['live']['last_tick'],flush=True)
        verify(plan,out,reaudit)
        results={name:r['report']['forks']['live']['reason']=='recovery_complete'
            and r['report']['forks']['live']['pilots'][1]['recovery']['ships_lost']==1
            and r['report']['forks']['live']['round']['pilots'][1]['health']>0
            for name,r in summary['runs'].items() if MODES[name][1]}
        recorded=any(m.get('actual',{}).get('events',{}).get('first_settled') is not None
            for m in summary['runs']['selection_handoff']['selection']['recorded']['measurements'])
        summary.update(complete=True,screen=dict(controls_retained=True,live_chains=results,recorded_positive_retained=recorded,
            decision='isolated_chains_validated' if all(results.values()) and recorded else 'not_qualified',
            default_promotion=False,scored_match=False))
    except BaseException:
        summary['error']=traceback.format_exc();raise
    finally:I.write(out/'summary.json',summary)
    print(summary['screen'],flush=True)


if __name__=='__main__':
    assert __debug__
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='action',required=True)
    p=sub.add_parser('plan');p.add_argument('--out',type=Path,required=True);p.add_argument('--binary',type=Path,required=True)
    p=sub.add_parser('run');p.add_argument('--plan',type=Path,required=True);p.add_argument('--reaudit',action='store_true')
    a=parser.parse_args()
    if a.action=='plan':freeze(a.out.resolve(),a.binary.resolve())
    else:execute(a.plan.resolve(),a.reaudit)
