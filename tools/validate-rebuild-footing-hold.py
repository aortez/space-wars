#!/usr/bin/env python3
"""Retain arrived build footing and revalidate changes against two frozen cases."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import struct
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('walk',Path(__file__).with_name('validate-rebuild-staging-walk.py'))
W=importlib.util.module_from_spec(spec);spec.loader.exec_module(W)
ROOT,P,I,B,D,L,R=W.ROOT,W.P,W.I,W.B,W.D,W.L,W.R
PRIOR=ROOT/'target/rebuild-staging-walk/v1'
PROFILE='retained_rebuild_footing_hold_v1'
MODES={'control_walk':('walk',False),'control_handoff':('handoff',False),
       'hold_walk':('walk',True),'hold_handoff':('handoff',True)}
OWN=('tools/validate-rebuild-footing-hold.py','tools/tests/test_rebuild_footing_hold.py','docs/rebuild-footing-hold-plan.md')
NEW=('crates/spacewars-ai/src/recovery_task/footing.rs',)
CHANGED={*OWN,*NEW,*W.OWN[:2],*R.NEW,'crates/spacewars-ai/src/ground_task.rs',
    'crates/spacewars-ai/src/recovery_task.rs','crates/spacewars-ai/src/recovery_task/search.rs',
    'crates/spacewars-ai/tests/surface_recovery.rs'}


def inputs():
    return dict(W.inputs(),**{p:P.digest(ROOT/p) for p in (*NEW,*OWN)})


def command(prior,binary,out,mode):
    base,enabled=MODES[mode]
    cmd=list(prior['commands'][base]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    for option in ('--rebuild-refinement','--rebuild-staging','--rebuild-staging-walk'):
        assert cmd[cmd.index(option)+1]=='true'
    assert cmd[cmd.index('--rebuild-staging-handoff')+1]==str(base=='handoff').lower()
    return cmd+['--rebuild-footing-hold',str(enabled).lower()]


def check_inputs(expected,current,reaudit=False):
    assert expected.keys()==current.keys()
    changed={p for p in expected if expected[p]!=current[p]}
    assert not changed or reaudit and changed<=set(OWN[:2]),changed


def verify(plan,out,reaudit=False):
    check_inputs(plan['inputs'],inputs(),reaudit)
    assert P.digest(PRIOR/'plan.json')==plan['prior_plan_sha256']
    assert P.digest(PRIOR/'summary.json')==plan['prior_summary_sha256']
    assert P.digest(plan['tape']['path'])==plan['tape']['sha256']
    assert P.digest(plan['binary']['path'])==plan['binary']['sha256']
    prior=json.loads((PRIOR/'plan.json').read_text())
    assert set(plan['commands'])==set(MODES)
    for name,cmd in plan['commands'].items():
        assert cmd==command(prior,Path(plan['binary']['path']),out/'raw'/name,name)


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());old=json.loads((PRIOR/'summary.json').read_text())
    assert old['complete'] and old['screen']['isolated_recovery_complete']=={'walk':False,'handoff':True}
    current=inputs();assert prior['inputs'].keys()<=current.keys()
    changed={p for p in current if prior['inputs'].get(p)!=current[p]};assert changed<=CHANGED,changed
    out.mkdir(parents=True,exist_ok=False)
    for name in ('logs','raw','archives'):(out/name).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,changed_inputs=sorted(changed),binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        prior_plan_sha256=P.digest(PRIOR/'plan.json'),prior_summary_sha256=P.digest(PRIOR/'summary.json'),
        commands={n:command(prior,frozen,out/'raw'/n,n) for n in MODES},
        activation_tick=23767,seat=1,task_start=16820,end=29421,fresh_games=0,default_promotion=False,
        bounds=dict(prior['bounds'],footing_hold_ticks=1200,footing_hold_displacement=2.0),
        build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen two retained controls and two footing-hold candidates: four prefixes, eight continuations.',flush=True)


def audit_footing(root,fork,enabled):
    holds=[];releases=[];corrections=[];seen={};ended=set()
    for row in D.rows(root/('rebuild-'+fork+'.jsonl')):
        task=row['task'];held=task.get('rebuild_footing');p=row['pilots'][1];tick=row['tick']
        if not enabled:assert held is None
        if held is None:continue
        assert enabled
        start=held['started_tick'];site=held['site'];ground=task['ground']
        if start not in seen:
            assert tick==start and site['precise'] and held['ended_tick'] is None
            assert task['relocation_site'] is None and ground['goal']=='arrived' and ground['precise_rebuild']
            assert site['planet']==p['supported_planet']==p['planet']['index'] and site['revision']==p['planet']['revision']
            assert W.S.foot_distance(p,site['position'])<.121
            seen[start]=ground['started_tick'];holds.append(row)
        assert held['ended_tick'] is None or held['ended_tick']>=start
        if held['ended_tick'] is not None:
            if start not in ended:
                assert held['ended_tick']==tick
                ended.add(start);releases.append(row)
            continue
        if task['status']!='running':continue
        assert tick-start<=1200
        assert ground['started_tick']==seen[start] and ground['precise_rebuild']
        assert not ground.get('continuous_walk',False)
        payload=row['generated_actions'][0]['Scenario']['payload']
        horizontal=struct.unpack('<f',bytes(payload[:4]))[0]
        if not p['controls_armed'] or not p['queries_ready'] or p['supported_planet']!=site['planet']:
            assert horizontal==0
        elif tick>start:
            assert p['planet']['revision']==site['revision']
            assert W.S.foot_distance(p,site['position'])<=2.000001
            if horizontal:corrections.append(dict(tick=tick,horizontal=horizontal,distance=W.S.foot_distance(p,site['position'])))
    assert len(holds)<=4
    result=dict(enabled=enabled,holds=holds,releases=releases,corrections=corrections)
    path=root/(fork+'-footing-audit.json');I.write(path,result)
    return dict(hold_ticks=[r['tick'] for r in holds],
        releases=[dict(tick=r['tick'],reason=r['task']['rebuild_footing']['reason']) for r in releases],
        correction_rows=len(corrections),first_correction_tick=corrections[0]['tick'] if corrections else None,sha256=P.digest(path))


def execute(path,reaudit=False):
    out=path.parent;plan=json.loads(path.read_text());verify(plan,out,reaudit)
    if (out/'summary.json').exists():
        assert reaudit
        backup=out/('summary-before-reaudit-'+P.digest(out/'summary.json')[:12]+'.json')
        assert not backup.exists();shutil.copy2(out/'summary.json',backup)
    summary=dict(schema=1,profile=PROFILE,complete=False,plan_sha256=P.digest(path),reaudit=reaudit,runs={},
        auditor_inputs={p:P.digest(ROOT/p) for p in OWN[:2]})
    try:
        for name in MODES:
            cmd=plan['commands'][name]
            root=out/'raw'/name;log=out/'logs'/(name+'.log');marker=out/(name+'-raw.json')
            if marker.exists():
                assert reaudit
                run=json.loads(marker.read_text());assert run['command']==cmd and run['log_sha256']==P.digest(log)
                if not root.exists():B.unpack(json.loads((out/(name+'-result.json')).read_text())['archive'],root)
                run['reused_raw']=True
            else:
                assert not root.exists() and not log.exists(),'partial simulations are never retried'
                with log.open('x') as stream:subprocess.run(cmd,stdout=stream,stderr=stream,check=True,timeout=1800)
                run=dict(command=cmd,hashes=I.raw_hashes(root),log_sha256=P.digest(log),
                    report=json.loads((root/'rebuild-replay.json').read_text()))
                I.write(marker,run)
            assert all(P.digest(root/p)==h for p,h in run['hashes'].items())
            report=run['report'];run['prefix']=R.audit_prefix(root,plan['tape']['path'],report)
            assert run['prefix']['verified']
            run['forks']={f:R.audit_fork(root,f,report) for f in ('recorded','live')}
            run['search']={f:W.S.audit_search(root,f,True) for f in ('recorded','live')}
            base,enabled=MODES[name]
            run['execution']={f:W.audit_execution(root,f,base) for f in ('recorded','live')}
            if not enabled:
                previous=json.loads((PRIOR/'summary.json').read_text())['runs'][base]
                assert run['hashes']==previous['hashes']
                assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
                run['retained_files']=len(previous['archive']['files'])
            run['footing']={f:audit_footing(root,f,enabled) for f in ('recorded','live')}
            run['archive']=B.pack(root,out/'archives'/(name+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,run['forks']['live']['reason'],'at',run['forks']['live']['last_tick'],flush=True)
        outcomes={}
        for name in ('hold_walk','hold_handoff'):
            live=summary['runs'][name]['report']['forks']['live']
            outcomes[name]=live['reason']=='recovery_complete' and live['pilots'][1]['recovery']['ships_lost']==1 and live['round']['pilots'][1]['health']>0
        verify(plan,out,reaudit)
        summary.update(complete=True,screen=dict(control_retained=True,isolated_recovery_complete=outcomes,
            decision='isolated_chain_validated' if any(outcomes.values()) else 'not_qualified',scored_match=False,default_promotion=False))
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
