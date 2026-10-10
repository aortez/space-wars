#!/usr/bin/env python3
"""Compare bounded native settling forecasts with three retained build outcomes."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('round_foot',Path(__file__).with_name('probe-rebuild-round-foot.py'))
R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)
ROOT,P,I,B,D,L=R.ROOT,R.P,R.I,R.B,R.D,R.L
same_native=R.S.A.S.same_native
PRIOR=ROOT/'target/rebuild-round-foot-probe/v1'
PROFILE='retained_rebuild_native_forecast_v1'
MODES=R.MODES
HORIZON=120
OWN=('tools/probe-rebuild-native-forecast.py','tools/tests/test_rebuild_native_forecast.py','docs/rebuild-native-forecast-plan.md')
NEW=('scenarios/spacewars/src/surface_sortie/rebuild_placement/native_forecast.rs',)
CHANGED={*OWN,*NEW,'scenarios/spacewars/src/surface_sortie/rebuild_placement.rs',
    'crates/spacewars-ai/examples/support/rebuild_replay.rs'}


def inputs():return dict(R.inputs(),**{p:P.digest(ROOT/p) for p in (*OWN,*NEW)})


def command(prior,binary,out,mode):
    cmd=list(prior['commands'][mode]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    assert cmd[cmd.index('--rebuild-support-alignment')+1]=='false'
    assert cmd[cmd.index('--rebuild-round-foot-probe')+1]=='true'
    assert '--rebuild-native-forecast' not in cmd
    return cmd+['--rebuild-native-forecast','true']


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
    print('Frozen two retained controls and three 120-step native forecasts; no future replay actions.',flush=True)


def settled(row):
    p=row['pilot'];l=p['landing']
    return p['ship_form']=='ship' and p['ship_available'] and p['ship_health']>0 and l['phase']=='landed' and l['supported_feet']==2 and l['settled_seconds']>=.25


def prediction(first,steps,stop):
    if first is not None:return True
    return False if steps==HORIZON and stop=='horizon' else None


def events(rows):
    return dict(first_contact=next((r['tick'] for r in rows if any(f['contacts'] for f in r['landing_diagnostics']['feet'])),None),
        first_two_supported=next((r['tick'] for r in rows if r['pilot']['landing']['supported_feet']==2),None),
        first_settled=next((r['tick'] for r in rows if settled(r)),None))


def projection(row):
    p=row['pilot'];d=row['landing_diagnostics']
    return dict(ship=p['ship'],form=p['ship_form'],health=p['ship_health'],landing=p['landing'],
        planet=p['planet']['motion'],revision=p['planet']['revision'],feet=d['feet'],hull=d['hull'])


def analyze(probe,actual):
    assert probe['read_only'] and probe['physics_unchanged'] and probe['pilots_unchanged']
    assert probe['action_policy']=='all_seats_neutral' and probe['privileged_full_world'] and not probe['future_actions_read']
    assert probe['horizon_ticks']==HORIZON and probe['step_nanoseconds']==16_666_667 and probe['seat']==1
    samples=probe['samples'];steps=probe['steps'];assert 0<steps<=HORIZON and len(samples)==steps+1
    assert probe['stop'] in ('horizon','vehicle_unavailable','round_finished')
    assert [r['tick'] for r in samples]==list(range(probe['tick'],probe['tick']+steps+1))
    assert probe['stop']!='horizon' or steps==HORIZON
    for r in samples:assert r['native_settled']==settled(r)
    forecast_events=events(samples);assert forecast_events['first_settled']==probe['first_settled_tick']
    assert probe['settles_within_horizon']==prediction(probe['first_settled_tick'],steps,probe['stop'])
    assert all(math.isfinite(v) and v>=0 for v in probe['timing'].values())
    assert probe['bodies']>0 and probe['colliders']>0 and probe['physics_snapshot_bytes']>0
    assert same_native(probe['report'],actual[0]['pilot']['recovery']['placement'])
    assert same_native(projection(samples[0]),projection(actual[0]))
    observed=events(actual);first_differences={};differences=[];max_error=0.0;max_error_until_settle=0.0
    cutoff=observed['first_settled'] or actual[-1]['tick']
    for a,b in zip(samples,actual):
        assert a['tick']==b['tick']
        pa,pb=projection(a),projection(b)
        for k in pa:
            if not same_native(pa[k],pb[k]) and k not in first_differences:first_differences[k]=a['tick']
        error=math.dist([pa['ship']['position'][k] for k in ('x','y')],[pb['ship']['position'][k] for k in ('x','y')])
        max_error=max(max_error,error)
        if a['tick']<=cutoff:max_error_until_settle=max(max_error_until_settle,error)
        differences.append(dict(tick=a['tick'],position_error=error,forecast_support=pa['landing']['supported_feet'],
            actual_support=pb['landing']['supported_feet'],forecast_settled=a['native_settled'],actual_settled=settled(b)))
    observed_settled=observed['first_settled'] is not None
    # An incomplete observed window cannot prove a negative prediction.
    comparable=observed_settled or len(actual)==HORIZON+1
    value=probe['settles_within_horizon']
    return dict(tick=probe['tick'],forecast_events=forecast_events,actual_events=observed,
        prediction=value,matches_observed_settling=value==observed_settled if value is not None and comparable else None,
        common_rows=len(differences),first_differences=first_differences,max_position_error=max_error,
        max_position_error_until_actual_settle=max_error_until_settle,timing=probe['timing'],
        prediction_compute_ms=probe['timing']['clone_ms']+probe['timing']['native_steps_ms'],
        differences=differences)


def audit(root,previous):
    probes={p.name:json.loads(p.read_text()) for p in root.glob('rebuild-native-forecast-*.json')}
    measurements={};witnesses={};expected_names=[]
    for fork in ('live','recorded'):
        measurements[fork]=[];witnesses[fork]={}
        for case in previous['round_foot']['measurements'][fork]:
            tick=case['tick'];name=f'rebuild-native-forecast-{fork}-{tick}.json';expected_names.append(name)
            actual=[dict(tick=r['tick'],pilot=r['pilots'][1],landing_diagnostics=r['landing_diagnostics'])
                for r in D.rows(root/('rebuild-'+fork+'.jsonl')) if tick<=r['tick']<=tick+HORIZON]
            assert actual and actual[0]['tick']==tick
            measurements[fork].append(analyze(probes[name],actual));witnesses[fork][tick]=actual
    assert set(expected_names)==probes.keys()
    path=root/'native-forecast-audit.json';I.write(path,dict(probes=probes,measurements=measurements,actual_samples=witnesses))
    return dict(measurements={f:[{k:v for k,v in m.items() if k!='differences'} for m in ms] for f,ms in measurements.items()},sha256=P.digest(path))


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
            original={p:h for p,h in run['hashes'].items() if not p.startswith('rebuild-native-forecast-')}
            assert original==previous['hashes']
            missing=[p for p in previous['archive']['files'] if not (root/p).exists()]
            if missing:L.extract(previous['archive'],root,missing)
            assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
            run['retained_files']=len(previous['archive']['files']);assert run['retained_files']==(41 if name=='control_handoff' else 40)
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
