#!/usr/bin/env python3
"""Compare a limited preconstruction forecast with retained full-world references."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('native_forecast',Path(__file__).with_name('probe-rebuild-native-forecast.py'))
N=importlib.util.module_from_spec(spec);spec.loader.exec_module(N)
ROOT,P,I,B,D,L=N.ROOT,N.P,N.I,N.B,N.D,N.L
same_native=N.same_native
PRIOR=ROOT/'target/rebuild-native-forecast/v1'
PROFILE='retained_rebuild_local_forecast_v1'
MODES=N.MODES
HORIZON=120
OWN=('tools/probe-rebuild-local-forecast.py','tools/tests/test_rebuild_local_forecast.py','docs/rebuild-local-forecast-plan.md')
NEW=('scenarios/spacewars/src/surface_sortie/rebuild_placement/local_forecast.rs',
    'crates/engine-rapier/src/world/forecast_body.rs')
CHANGED={*OWN,*NEW,'scenarios/spacewars/src/surface_sortie/rebuild_placement.rs',
    'scenarios/spacewars/src/surface_sortie/rebuild_placement/native_forecast.rs',
    'scenarios/spacewars/src/surface_sortie/recovery.rs','scenarios/spacewars/src/surface_sortie.rs',
    'scenarios/spacewars/src/physics.rs','crates/engine-rapier/src/world.rs',
    'crates/spacewars-ai/examples/support/rebuild_replay.rs'}


def inputs():return dict(N.inputs(),**{p:P.digest(ROOT/p) for p in (*OWN,*NEW)})


def command(prior,binary,out,mode):
    cmd=list(prior['commands'][mode]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    assert cmd[cmd.index('--rebuild-support-alignment')+1]=='false'
    assert cmd[cmd.index('--rebuild-round-foot-probe')+1]=='true'
    idx=cmd.index('--rebuild-native-forecast');assert cmd[idx+1]=='true'
    del cmd[idx:idx+2]
    assert '--rebuild-local-forecast' not in cmd
    return cmd+['--rebuild-local-forecast','true']


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
    assert set(plan['commands'])==set(MODES) and plan['horizon_ticks']==HORIZON
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
        commands={n:command(prior,frozen,out/'raw'/n,n) for n in MODES},horizon_ticks=HORIZON,activation_tick=23767,seat=1,
        task_start=16820,end=29421,fresh_games=0,default_promotion=False,bounds=prior['bounds'],build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen three preconstruction candidates, two-body forecasts, four steps per live frame.',flush=True)


def settled(row):
    landing=row['landing']
    return landing['phase']=='landed' and landing['supported_feet']==2 and landing['settled_seconds']>=.25


def prediction(first,steps,stop):
    return first is not None if steps==HORIZON and stop=='horizon' else None


def events(rows):
    return dict(first_contact=next((r['tick'] for r in rows if any(f['contacts'] for f in r['contacts'][1:])),None),
        first_two_supported=next((r['tick'] for r in rows if r['landing']['supported_feet']==2),None),
        first_settled=next((r['tick'] for r in rows if settled(r)),None))


def reference(row):
    p=row['pilot'];d=row['landing_diagnostics']
    return dict(tick=row['tick'],ship=p['ship'],planet=p['planet']['motion'],revision=p['planet']['revision'],
        landing=p['landing'],contacts=[d['hull'],*d['feet']],settled=N.settled(row))


def compare(samples,reference_rows):
    first_differences={};differences=[];max_error=0.0
    for a,b in zip(samples,reference_rows):
        assert a['tick']==b['tick']
        for k in b:
            if not same_native(a[k],b[k]) and k not in first_differences:first_differences[k]=a['tick']
        error=math.dist([a['ship']['position'][k] for k in ('x','y')],[b['ship']['position'][k] for k in ('x','y')])
        max_error=max(max_error,error)
        differences.append(dict(tick=a['tick'],position_error=error,forecast_support=a['landing']['supported_feet'],
            reference_support=b['landing']['supported_feet'],forecast_settled=a['settled'],reference_settled=b['settled']))
    return dict(common_rows=len(differences),first_differences=first_differences,max_position_error=max_error,
        events=events(reference_rows),differences=differences)


def analyze(probe,full,actual):
    assert probe['read_only'] and not probe['production_qualified'] and not probe['future_actions_read']
    assert probe['scope']=='selected_planet_geometry_and_gravity_ephemeris'
    assert probe['action_policy']=='unoccupied_replacement_neutral'
    assert not any(probe[k] for k in ('terrain_updates','other_actors','damage_and_hazards'))
    assert probe['horizon_ticks']==HORIZON and probe['step_nanoseconds']==16_666_667 and probe['seat']==1
    assert probe['max_steps_per_call']==4 and probe['max_planets']==32 and probe['max_planet_colliders']==128
    assert probe['max_shape_parts']==16384 and probe['max_local_travel']==16
    inp=probe['input'];assert inp['captured_before_construction'] and not inp['live_vehicle_available']
    assert same_native(probe['report'],full['report'])
    assert full['tick']==probe['tick'] and full['steps']==HORIZON
    samples=probe['samples'];steps=probe['steps'];chunks=probe['chunks']
    assert 0<=steps<=HORIZON
    stop=probe['stop'];assert stop in (None,'horizon','outside_model_scope','source_limit_or_existing_vehicle','planet_geometry_unavailable_or_over_limit')
    if samples:
        assert len(samples)==steps+1 and inp['bodies']==2 and inp['colliders']<=131 and inp['planet_count']<=32
        assert [r['tick'] for r in samples]==list(range(probe['tick'],probe['tick']+steps+1))
        assert same_native(samples[0]['ship'],full['samples'][0]['pilot']['ship'])
        assert same_native(samples[0]['planet'],full['samples'][0]['pilot']['planet']['motion'])
        for r in samples:assert r['settled']==settled(r)
    else:assert steps==0 and not chunks
    assert stop!='horizon' or steps==HORIZON
    total=0
    for c in chunks:
        assert c['first_step']==total+1 and 1<=c['steps']<=4
        assert math.isfinite(c['elapsed_ms']) and c['elapsed_ms']>=0
        total+=c['steps']
    assert total==steps
    if 'completed_at_tick' in probe:
        assert stop is not None
        assert probe['completed_at_tick']==probe['tick']+max(0,len(chunks)-1)
    assert all(math.isfinite(v) and v>=0 for v in probe['timing'].values())
    first=events(samples)['first_settled'];assert stop=='outside_model_scope' or first==probe['first_settled_tick']
    value=prediction(probe['first_settled_tick'],steps,stop);assert probe['settles_within_horizon']==value
    projected=[reference(r) for r in full['samples']];observed=[reference(r) for r in actual]
    a,b=compare(samples,projected),compare(samples,observed)
    observed_settled=b['events']['first_settled'] is not None
    comparable=observed_settled or len(actual)==HORIZON+1
    return dict(tick=probe['tick'],steps=steps,stop=stop,input=inp,forecast_events=events(samples),prediction=value,
        matches_full_world_settling=value==full['settles_within_horizon'] if value is not None else None,
        matches_observed_settling=value==observed_settled if value is not None and comparable else None,
        full_world=a,actual=b,timing=probe['timing'],chunks=len(chunks),
        maximum_chunk_ms=max((c['elapsed_ms'] for c in chunks),default=0),
        prediction_compute_ms=probe['timing']['setup_ms']+probe['timing']['physics_steps_ms'])


def compact(measured):
    return {k:({a:b for a,b in v.items() if a!='differences'} if k in ('actual','full_world') else v) for k,v in measured.items()}


def audit(root,previous):
    old=json.loads((root/'native-forecast-audit.json').read_text())
    probes={p.name:json.loads(p.read_text()) for p in root.glob('rebuild-local-forecast-*.json')}
    measurements={};expected_names=[]
    for fork in ('live','recorded'):
        measurements[fork]=[]
        for case in previous['forecast']['measurements'][fork]:
            tick=case['tick'];name=f'rebuild-local-forecast-{fork}-{tick}.json';expected_names.append(name)
            full=old['probes'][f'rebuild-native-forecast-{fork}-{tick}.json'];actual=old['actual_samples'][fork][str(tick)]
            measurements[fork].append(analyze(probes[name],full,actual))
    assert set(expected_names)==probes.keys()
    path=root/'local-forecast-audit.json';I.write(path,dict(probes=probes,measurements=measurements,
        archived_native_audit_sha256=P.digest(root/'native-forecast-audit.json')))
    return dict(measurements={f:[compact(m) for m in ms] for f,ms in measurements.items()},sha256=P.digest(path))


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
            previous=prior['runs'][name]
            original={p:h for p,h in run['hashes'].items() if not p.startswith('rebuild-local-forecast-')}
            assert original=={p:h for p,h in previous['hashes'].items() if not p.startswith('rebuild-native-forecast-')}
            run['timed_native_forecasts_restored_from_archive']=True
            missing=[p for p in previous['archive']['files'] if not (root/p).exists()]
            if missing:L.extract(previous['archive'],root,missing)
            assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
            run['retained_files']=len(previous['archive']['files']);assert run['retained_files']==(44 if name=='control_handoff' else 42)
            run['audits_reused_after_raw_retention']=True
            run['forecast']=audit(root,previous)
            suffix='-reaudit-'+P.digest(ROOT/OWN[0])[:12] if reaudit else ''
            run['archive']=B.pack(root,out/'archives'/(name+suffix+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,'retained',run['retained_files'],'files;',run['forecast']['measurements'],flush=True)
        verify(plan,out,reaudit)
        results=[m['matches_observed_settling'] for r in summary['runs'].values() for ms in r['forecast']['measurements'].values() for m in ms]
        assert len(results)==3
        summary.update(complete=True,screen=dict(all_paths_retained=True,read_only=True,default_promotion=False,scored_match=False,
            all_classifications_match=all(r is True for r in results),production_qualified=False))
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
