#!/usr/bin/env python3
"""Measure round-foot first contacts on retained successful and failed builds."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('support',Path(__file__).with_name('validate-rebuild-support-alignment.py'))
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)
ROOT,P,I,B,D,L=S.ROOT,S.P,S.I,S.B,S.D,S.L
PRIOR=ROOT/'target/rebuild-support-alignment/v1'
PROFILE='retained_rebuild_round_foot_probe_v1'
MODES=('control_handoff','control_coarse')
OWN=('tools/probe-rebuild-round-foot.py','tools/tests/test_rebuild_round_foot.py','docs/rebuild-round-foot-probe-plan.md')
NEW=('crates/engine-rapier/src/world/ball_cast.rs','scenarios/spacewars/src/surface_sortie/rebuild_placement/round_foot.rs')
CHANGED={*OWN,*NEW,'crates/engine-rapier/src/world.rs','crates/spacewars-ai/examples/support/rebuild_replay.rs',
    'scenarios/spacewars/src/surface_sortie/rebuild_placement.rs',
    'scenarios/spacewars/src/surface_sortie/rebuild_placement/footprint.rs'}


def inputs():return dict(S.inputs(),**{p:P.digest(ROOT/p) for p in (*OWN,*NEW)})


def command(prior,binary,out,mode):
    cmd=list(prior['commands'][mode]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    assert cmd[cmd.index('--rebuild-support-alignment')+1]=='false'
    assert '--rebuild-round-foot-probe' not in cmd and '--rebuild-footprint-probe' not in cmd
    return cmd+['--rebuild-round-foot-probe','true']


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
    assert set(plan['commands'])==set(MODES)
    for name,cmd in plan['commands'].items():
        assert cmd==command(prior,Path(plan['binary']['path']),out/'raw'/name,name)


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());old=json.loads((PRIOR/'summary.json').read_text())
    assert old['complete'] and old['screen']['control_retained'] and old['screen']['decision']=='not_qualified'
    current=inputs();assert prior['inputs'].keys()<=current.keys()
    changed={p for p in current if prior['inputs'].get(p)!=current[p]};assert changed<=CHANGED,changed
    out.mkdir(parents=True,exist_ok=False)
    for name in ('logs','raw','archives'):(out/name).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,changed_inputs=sorted(changed),binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        prior_plan_sha256=P.digest(PRIOR/'plan.json'),prior_summary_sha256=P.digest(PRIOR/'summary.json'),
        commands={n:command(prior,frozen,out/'raw'/n,n) for n in MODES},activation_tick=23767,seat=1,
        task_start=16820,end=29421,fresh_games=0,default_promotion=False,bounds=prior['bounds'],build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen two retained controls: two prefixes, four continuations, live and recorded round-foot probes.',flush=True)


def first_contact_prediction(sweep,threshold):
    values=[]
    assert len(sweep['feet'])==2
    for foot in sweep['feet']:
        if foot is None:
            values.append(None);continue
        assert foot['status'] in ('Converged','Penetrating','OutOfIterations','Failed')
        assert math.isfinite(foot['up_alignment'])
        value=foot['up_alignment']>=threshold if foot['status']=='Converged' and foot['retained_surface'] else None
        assert foot['supports_first_contact']==value
        values.append(value)
    return all(values) if all(v is not None for v in values) else None


def analyze(probe,row):
    assert probe['read_only'] and probe['physics_unchanged'] and probe['tick']==row['tick'] and probe['seat']==1
    assert probe['prediction_kind']=='independent_first_contacts_without_rotation'
    assert probe['support_threshold']==S.MIN_ALIGNMENT
    S.S.analyze(probe['footprint'],row)
    assert [s['name'] for s in probe['sweeps']]==['normal','radial']
    result={}
    for sweep in probe['sweeps']:
        prediction=first_contact_prediction(sweep,probe['support_threshold'])
        assert sweep['both_first_contacts_supported']==prediction
        for foot in sweep['feet']:
            if foot is None:continue
            assert 0<=foot['distance']<=4
            normal=foot['normal'];up=foot['radial_up_at_impact']
            assert abs(foot['up_alignment']-sum(normal[a]*up[a] for a in ('x','y')))<.00001
            if foot['status']=='Converged':
                assert abs(math.hypot(normal['x'],normal['y'])-1)<.0001
                # Rounded corner contacts must lie one actual foot radius
                # from the swept center; allow floating-point query tolerance.
                assert abs(S.S.distance(foot['center_at_impact'],foot['point'])-probe['footprint']['foot_radius'])<.005
        result[sweep['name']]=dict(prediction=prediction,alignments=[h['up_alignment'] if h else None for h in sweep['feet']],
            distance_gap=sweep['distance_gap'])
    return dict(tick=row['tick'],predictions=result)


def audit(root,expected_builds):
    probes={p.name:json.loads(p.read_text()) for p in root.glob('rebuild-round-foot-*.json')}
    result={};witnesses={}
    for fork in ('live','recorded'):
        selected={int(p.rsplit('-',1)[1][:-5]):v for p,v in probes.items() if p.startswith('rebuild-round-foot-'+fork+'-')}
        assert sorted(selected)==expected_builds[fork]
        measured={};seen=set();events=[];previous=None
        for row in D.rows(root/('rebuild-'+fork+'.jsonl')):
            p=row['pilots'][1];tick=row['tick'];landing=p['landing']
            if tick in selected:
                measured[tick]=dict(analyze(selected[tick],row),first_settled_tick=None);seen.add(tick)
            if measured:
                current=measured[max(measured)]
                if current['first_settled_tick'] is None and p['ship_form']=='ship' and landing['phase']=='landed' and landing['supported_feet']==2 and landing['settled_seconds']>=.25:
                    current['first_settled_tick']=tick
                key=(landing['phase'],landing['supported_feet'],p['planet']['revision'],p['recovery']['ships_lost'],row['task']['goal'],row['stop'])
                if tick in selected or key!=previous or tick==current['first_settled_tick']:events.append(row)
                previous=key
        assert seen==selected.keys()
        for current in measured.values():
            for prediction in current['predictions'].values():
                value=prediction['prediction']
                prediction['matches_observed_settling']=None if value is None else value==(current['first_settled_tick'] is not None)
        result[fork]=list(measured.values());witnesses[fork]=events
    path=root/'round-foot-audit.json';I.write(path,dict(probes=probes,measurements=result,witnesses=witnesses))
    return dict(measurements=result,sha256=P.digest(path))


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
            original={p:h for p,h in run['hashes'].items() if not p.startswith('rebuild-round-foot-')}
            assert original==previous['hashes']
            missing=[p for p in previous['archive']['files'] if not (root/p).exists()]
            if missing:L.extract(previous['archive'],root,missing)
            assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
            run['retained_files']=len(previous['archive']['files']);assert run['retained_files']==38
            run['audits_reused_after_raw_retention']=True
            run['round_foot']=audit(root,{f:[b['tick'] for b in previous['forks'][f]['builds']] for f in ('live','recorded')})
            suffix='-reaudit-'+P.digest(ROOT/OWN[0])[:12] if reaudit else ''
            run['archive']=B.pack(root,out/'archives'/(name+suffix+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,'retained',run['retained_files'],'files;',run['round_foot']['measurements'],flush=True)
        verify(plan,out,reaudit)
        summary.update(complete=True,screen=dict(all_paths_retained=True,read_only=True,default_promotion=False,scored_match=False))
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
