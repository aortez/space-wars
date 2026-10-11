#!/usr/bin/env python3
"""Compare radial placement queries against retained successful and failed recoveries."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil

import subprocess
import traceback

spec=importlib.util.spec_from_file_location('recheck',Path(__file__).with_name('validate-rebuild-footing-recheck.py'))
F=importlib.util.module_from_spec(spec);spec.loader.exec_module(F)
C,H,W=F.C,F.H,F.W
ROOT,P,I,B,D,L,R=F.ROOT,F.P,F.I,F.B,F.D,F.L,F.R
PRIOR=ROOT/'target/rebuild-footing-recheck/v1'
PROFILE='shared_radial_rebuild_placement_v1'
MODES={'control_handoff':('control_handoff',False),
       'control_recheck_walk':('recheck_walk',False),'control_recheck_handoff':('recheck_handoff',False),
       'radial_handoff':('control_handoff',True),
       'radial_recheck_walk':('recheck_walk',True),'radial_recheck_handoff':('recheck_handoff',True)}
OWN=('tools/validate-rebuild-radial-placement.py','tools/tests/test_rebuild_radial_placement.py','docs/rebuild-radial-placement-plan.md')
NEW=('scenarios/spacewars/src/surface_sortie/rebuild_placement/radial.rs',)
CHANGED={*OWN,*NEW,*F.OWN[:2],*R.NEW,'scenarios/spacewars/src/surface_sortie.rs',
    'scenarios/spacewars/src/surface_sortie/rebuild_placement.rs',
    'scenarios/spacewars/src/surface_sortie/recovery/contact_frame.rs',
    'crates/spacewars-ai/tests/surface_recovery.rs'}

def inputs():
    return dict(F.inputs(),**{p:P.digest(ROOT/p) for p in (*NEW,*OWN)})


def command(prior,binary,out,mode):
    base,enabled=MODES[mode]
    cmd=list(prior['commands'][base]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    assert '--rebuild-radial-placement' not in cmd
    assert cmd[cmd.index('--rebuild-contact-frame')+1]==cmd[cmd.index('--rebuild-contact-probe')+1]=='true'
    return cmd+['--rebuild-radial-placement',str(enabled).lower()]


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
    assert old['complete'] and old['screen']['decision']=='not_qualified'
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
    print('Frozen three retained controls and three radial candidates: six prefixes, twelve continuations.',flush=True)


def check_direction(report,enabled,activation):
    # A native result latched before activation belongs to the unchanged prefix.
    if report['tick']<=activation:
        assert 'radial_up' not in report
        return None
    if not enabled:
        assert 'radial_up' not in report
        return None
    up=report['radial_up'];point=report['standing'];length=math.hypot(point['x'],point['y'])
    assert length>0 and math.isfinite(length)
    error=math.hypot(up['x']-point['x']/length,up['y']-point['y']/length)
    assert math.isfinite(error) and error<.00001,error
    for attempt in report['attempts']:
        if attempt['rejection'] is None:
            assert 0<=attempt['settling_angle_degrees']<20
            assert attempt['route']['failure'] is None and attempt['route']['length']<=24
    if report['selected_offset'] is not None:
        selected=[a for a in report['attempts'] if a['offset']==report['selected_offset']]
        assert len(selected)==1 and selected[0]['rejection'] is None
    return error


def audit_orientation(root,fork,enabled,activation):
    native={};previews=[];errors=[]
    for row in D.rows(root/('rebuild-'+fork+'.jsonl')):
        report=row['pilots'][1]['recovery']['placement']
        if report is not None and report['tick'] not in native:
            error=check_direction(report,enabled,activation)
            native[report['tick']]=report
            if error is not None:errors.append(error)
        survey=row['observation']['rebuild']
        if survey is not None:
            for attempt in survey['attempts']:
                report=attempt['placement']
                if report is not None:
                    error=check_direction(report,enabled,activation)
                    if error is not None:errors.append(error)
                    previews.append(dict(tick=row['tick'],bearing=attempt['bearing'],report=report))
    assert native
    result=dict(enabled=enabled,maximum_direction_error=max(errors,default=None),
        native_reports=list(native.values()),preview_reports=previews)
    if enabled:assert errors
    path=root/(fork+'-orientation-audit.json');I.write(path,result)
    return dict(enabled=enabled,maximum_direction_error=result['maximum_direction_error'],
        native_reports=len(native),preview_reports=len(previews),sha256=P.digest(path))


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
            base,hold,recheck=F.MODES[previous_name]
            run['execution']={f:W.audit_execution(root,f,base) for f in ('recorded','live')}
            run['footing']={f:H.audit_footing(root,f,hold) for f in ('recorded','live')}
            run['contacts']={f:C.audit_contacts(root,f,True) for f in ('recorded','live')}
            run['recheck']={f:F.audit_recheck(root,f,hold,recheck) for f in ('recorded','live')}
            if not enabled:
                previous=json.loads((PRIOR/'summary.json').read_text())['runs'][previous_name]
                assert run['hashes']==previous['hashes']
                assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
                run['retained_files']=len(previous['archive']['files'])
            run['orientation']={f:audit_orientation(root,f,enabled,plan['activation_tick']) for f in ('recorded','live')}
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
