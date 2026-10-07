#!/usr/bin/env python3
"""Compare completed-frame native placement with two frozen recovery cases."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import math
from itertools import zip_longest
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('hold',Path(__file__).with_name('validate-rebuild-footing-hold.py'))
H=importlib.util.module_from_spec(spec);spec.loader.exec_module(H)
W=H.W
ROOT,P,I,B,D,L,R=W.ROOT,W.P,W.I,W.B,W.D,W.L,W.R
PRIOR=ROOT/'target/rebuild-footing-hold/v1'
PROFILE='native_rebuild_contact_frame_v1'
MODES={'control_walk':('walk',False),'control_handoff':('handoff',False),
       'contact_walk':('walk',True),'contact_handoff':('handoff',True)}
OWN=('tools/validate-rebuild-contact-frame.py','tools/tests/test_rebuild_contact_frame.py','docs/rebuild-contact-frame-plan.md')
NEW=('scenarios/spacewars/src/surface_sortie/recovery/contact_frame.rs',)
CHANGED={*OWN,*NEW,'scenarios/spacewars/src/surface_sortie.rs',
    'scenarios/spacewars/src/surface_sortie/recovery.rs',*R.NEW}


def inputs():
    return dict(H.inputs(),**{p:P.digest(ROOT/p) for p in (*NEW,*OWN)})


def command(prior,binary,out,mode):
    base,enabled=MODES[mode]
    cmd=list(prior['commands']['control_'+base]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    for option in ('--rebuild-refinement','--rebuild-staging','--rebuild-staging-walk'):
        assert cmd[cmd.index(option)+1]=='true'
    assert cmd[cmd.index('--rebuild-staging-handoff')+1]==str(base=='handoff').lower()
    assert cmd[cmd.index('--rebuild-footing-hold')+1]=='false'
    return cmd+['--rebuild-contact-probe','true','--rebuild-contact-frame',str(enabled).lower()]


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
    assert old['complete'] and old['screen']['isolated_recovery_complete']=={'hold_walk':False,'hold_handoff':False}
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
    print('Frozen two retained controls and two native contact-frame candidates: four prefixes, eight continuations.',flush=True)


def distance(a,b):
    return math.hypot(a['x']-b['x'],a['y']-b['y'])


def audit_contacts(root,fork,enabled):
    rows=0;contacts=0;candidates=0;attempts=[];max_offset=None
    trace=D.rows(root/('rebuild-'+fork+'.jsonl'))
    probes=D.rows(root/('rebuild-'+fork+'-contacts.jsonl'))
    for row,probe in zip_longest(trace,probes):
        assert row is not None and probe is not None
        assert probe['tick']==row['tick'] and probe['seat']==1 and probe['enabled']==enabled
        rows+=1;c=probe['contact'];candidate=probe['candidate']
        if c is not None:
            contacts+=1
            assert abs(distance(c['solver_position'],c['current_position'])-c['offset'])<.0001
            if max_offset is None or c['offset']>max_offset['offset']:
                max_offset=dict(tick=row['tick'],offset=c['offset'])
        if 'Ok' in candidate:
            candidates+=1;planet,point,normal=candidate['Ok']
            assert c is not None and c['planet']==planet
            assert distance(point,c['current_position' if enabled else 'solver_position'])<.0001
            assert distance(normal,c['current_normal' if enabled else 'solver_normal'])<.0001
        report=row['pilots'][1]['recovery']['placement']
        if report is not None and report['tick']==row['tick']:
            # Successful construction can invalidate contacts by inserting a ship.
            # Failed attempts keep the current selected support for direct comparison.
            error=None
            if c is not None and 'Ok' in candidate:
                point=candidate['Ok'][1];f=c['frame_position'];angle=c['frame_angle']
                x=point['x']-f['x'];y=point['y']-f['y']
                local=dict(x=x*math.cos(angle)+y*math.sin(angle),y=-x*math.sin(angle)+y*math.cos(angle))
                error=distance(local,report['standing']);assert error<.001,error
                if enabled:assert distance(report['standing'],c['local_position'])<.001
            attempts.append(dict(tick=row['tick'],report=report,probe=probe,standing_error=error))
    assert rows>0 and contacts>0 and candidates>0
    result=dict(enabled=enabled,rows=rows,contact_rows=contacts,candidate_rows=candidates,
        maximum_solver_offset=max_offset,native_attempts=attempts)
    path=root/(fork+'-contact-audit.json');I.write(path,result)
    return dict(enabled=enabled,rows=rows,contact_rows=contacts,candidate_rows=candidates,
        maximum_solver_offset=max_offset,attempt_ticks=[r['tick'] for r in attempts],sha256=P.digest(path))


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
            run['footing']={f:H.audit_footing(root,f,False) for f in ('recorded','live')}
            if not enabled:
                previous=json.loads((PRIOR/'summary.json').read_text())['runs'][name]
                assert {p:run['hashes'][p] for p in previous['hashes']}==previous['hashes']
                assert set(run['hashes'])-set(previous['hashes'])=={
                    'rebuild-live-contacts.jsonl','rebuild-recorded-contacts.jsonl'}
                assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
                run['retained_files']=len(previous['archive']['files'])
            run['contacts']={f:audit_contacts(root,f,enabled) for f in ('recorded','live')}
            run['archive']=B.pack(root,out/'archives'/(name+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,run['forks']['live']['reason'],'at',run['forks']['live']['last_tick'],flush=True)
        outcomes={}
        for name in ('contact_walk','contact_handoff'):
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
