#!/usr/bin/env python3
"""Compare precise foot arrival for coarse and refined rebuild sites."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil

import subprocess
import traceback

spec=importlib.util.spec_from_file_location('probe',Path(__file__).with_name('probe-rebuild-placement.py'))
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)
Q,F,C,H,W=S.Q,S.F,S.C,S.H,S.W
ROOT,P,I,B,D,L,R=S.ROOT,S.P,S.I,S.B,S.D,S.L,S.R
PRIOR=ROOT/'target/rebuild-placement-probe/v1'
PROFILE='precise_coarse_rebuild_arrival_v1'
MODES={'control_handoff':('control_handoff',False),'control_coarse':('radial_handoff',False),
       'precise_handoff':('control_handoff',True),'precise_coarse':('radial_handoff',True)}
OWN=('tools/validate-rebuild-precise-arrival.py','tools/tests/test_rebuild_precise_arrival.py','docs/rebuild-precise-arrival-plan.md')
CHANGED={*OWN,*R.NEW,'crates/spacewars-ai/src/recovery_task.rs','crates/spacewars-ai/tests/surface_recovery.rs'}
OMITTED={'rebuild-placement-probe.json','rebuild-placement-anchor.json','placement-probe-audit.json'}


def inputs():
    return dict(S.inputs(),**{p:P.digest(ROOT/p) for p in OWN})


def command(prior,binary,out,mode):
    base,enabled=MODES[mode]
    cmd=list(prior['commands'][base]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    at=cmd.index('--rebuild-placement-probe');del cmd[at:at+2]
    assert '--rebuild-precise-arrival' not in cmd
    return cmd+['--rebuild-precise-arrival',str(enabled).lower()]


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
    assert old['complete'] and old['screen']['all_paths_retained']
    assert old['runs']['control_handoff']['forks']['live']['reason']=='recovery_complete'
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
        bounds=prior['bounds'],
        build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen two retained controls and two precise-arrival candidates: four prefixes, eight continuations.',flush=True)


def audit_arrival(root,fork,enabled):
    selected=None;relocations=0;arrivals=[];attempts=[]
    for row in D.rows(root/('rebuild-'+fork+'.jsonl')):
        t=row['task'];p=row['pilots'][1];g=t['ground'] or {};tick=row['tick']
        if t['relocations']!=relocations:
            assert t['relocations']==relocations+1<=4
            relocations=t['relocations'];selected=None
            if t['relocation_site'] is not None:
                selected=dict(tick=tick,site=t['relocation_site'],arrived=None)
        if selected is None:continue
        site=selected['site'];destination=g.get('destination');rebuild=destination.get('rebuild') if isinstance(destination,dict) else None
        if rebuild and g['started_tick']>=selected['tick'] and tick>selected['tick'] and rebuild['planet']==site['planet'] and C.distance(rebuild['position'],site['position'])<.001:
            precise=enabled or site.get('precise',False)
            assert g.get('precise_rebuild',False)==precise
            if selected['arrived'] is None and g['goal']=='arrived':
                distance=W.S.foot_distance(p,site['position'])
                if precise:assert distance<.121
                selected['arrived']=tick
                arrivals.append(dict(tick=tick,selection_tick=selected['tick'],site=site,foot_distance=distance,row=row))
        report=p['recovery']['placement']
        if report is not None and report['tick']==tick and selected['arrived'] is not None:
            attempts.append(dict(tick=tick,arrival_tick=selected['arrived'],selection_tick=selected['tick'],site=site,
                same_terrain=report['planet']==site['planet'] and report['revision']==site['revision'],
                point_distance=C.distance(report['standing'],site['position']),report=report,row=row))
    result=dict(enabled=enabled,arrivals=arrivals,native_attempts=attempts)
    path=root/(fork+'-precise-arrival-audit.json');I.write(path,result)
    return dict(enabled=enabled,arrivals=[{k:v for k,v in a.items() if k!='row'} for a in arrivals],
        native_attempts=[{k:v for k,v in a.items() if k not in ('row','report')} for a in attempts],sha256=P.digest(path))


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
            previous_name,enabled=MODES[name]
            source,radial=Q.MODES[previous_name]
            base,hold,recheck=F.MODES[source]
            run['execution']={f:W.audit_execution(root,f,base) for f in ('recorded','live')}
            run['footing']={f:H.audit_footing(root,f,hold) for f in ('recorded','live')}
            run['contacts']={f:C.audit_contacts(root,f,True) for f in ('recorded','live')}
            run['recheck']={f:F.audit_recheck(root,f,hold,recheck) for f in ('recorded','live')}
            run['orientation']={f:Q.audit_orientation(root,f,radial,plan['activation_tick']) for f in ('recorded','live')}
            previous=json.loads((PRIOR/'summary.json').read_text())['runs'][previous_name]
            if not enabled:
                assert run['hashes']=={p:h for p,h in previous['hashes'].items() if p not in OMITTED}
                retained={p:v for p,v in previous['archive']['files'].items() if p not in OMITTED}
                assert all(P.digest(root/p)==v['sha256'] for p,v in retained.items())
                run['retained_files']=len(retained)
            assert run['hashes']['rebuild-recorded-contacts.jsonl']==previous['hashes']['rebuild-recorded-contacts.jsonl']
            run['arrival']={f:audit_arrival(root,f,enabled) for f in ('recorded','live')}
            # Retain the original frozen-auditor archives when repairing only
            # verification. Runtime bytes are hash checked above, never rerun.
            suffix='-reaudit-'+P.digest(ROOT/OWN[0])[:12] if reaudit else ''
            run['archive']=B.pack(root,out/'archives'/(name+suffix+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,run['forks']['live']['reason'],'at',run['forks']['live']['last_tick'],flush=True)
        outcomes={}
        for name,(_,enabled) in MODES.items():
            if not enabled:continue
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
