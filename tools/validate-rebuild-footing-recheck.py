#!/usr/bin/env python3
"""Compare fresh footing failures and explicit rechecks after terrain changes."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil

import subprocess
import traceback

spec=importlib.util.spec_from_file_location('contact',Path(__file__).with_name('validate-rebuild-contact-frame.py'))
C=importlib.util.module_from_spec(spec);spec.loader.exec_module(C)
H,W=C.H,C.W
ROOT,P,I,B,D,L,R=W.ROOT,W.P,W.I,W.B,W.D,W.L,W.R
PRIOR=ROOT/'target/rebuild-contact-frame/v1'
PROFILE='explicit_rebuild_footing_recheck_v1'
MODES={'control_walk':('walk',False,False),'control_handoff':('handoff',False,False),
       'hold_walk':('walk',True,False),'hold_handoff':('handoff',True,False),
       'recheck_walk':('walk',True,True),'recheck_handoff':('handoff',True,True)}
OWN=('tools/validate-rebuild-footing-recheck.py','tools/tests/test_rebuild_footing_recheck.py','docs/rebuild-footing-recheck-plan.md')
CHANGED={*OWN,*R.NEW,'crates/spacewars-ai/src/recovery_task.rs',
    'crates/spacewars-ai/src/recovery_task/footing.rs','crates/spacewars-ai/src/recovery_task/search.rs',
    'crates/spacewars-ai/tests/surface_recovery.rs',
    'scenarios/spacewars/src/surface_sortie/rebuild_placement/staging.rs'}


def inputs():
    return dict(C.inputs(),**{p:P.digest(ROOT/p) for p in OWN})


def command(prior,binary,out,mode):
    base,hold,recheck=MODES[mode]
    cmd=list(prior['commands']['contact_'+base]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    for option in ('--rebuild-refinement','--rebuild-staging','--rebuild-staging-walk'):
        assert cmd[cmd.index(option)+1]=='true'
    assert cmd[cmd.index('--rebuild-staging-handoff')+1]==str(base=='handoff').lower()
    assert cmd[cmd.index('--rebuild-footing-hold')+1]=='false'
    assert cmd[cmd.index('--rebuild-contact-probe')+1]==cmd[cmd.index('--rebuild-contact-frame')+1]=='true'
    cmd[cmd.index('--rebuild-footing-hold')+1]=str(hold).lower()
    return cmd+['--rebuild-footing-recheck',str(recheck).lower()]


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
    assert old['complete'] and old['screen']['isolated_recovery_complete']=={'contact_walk':False,'contact_handoff':True}
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
    print('Frozen two retained controls, two holding cases, two explicit rechecks: six prefixes, twelve continuations.',flush=True)


def audit_recheck(root,fork,hold,recheck):
    pending=None;seen=set();rechecks=[];failures=[];ignored=[]
    for row in D.rows(root/('rebuild-'+fork+'.jsonl')):
        t=row['task'];p=row['pilots'][1];h=t.get('rebuild_footing');tick=row['tick']
        if not hold:assert h is None
        if h and h['ended_tick']==tick and h['started_tick'] not in seen:
            seen.add(h['started_tick'])
            if h['reason'] in ('terrain changed','footing displaced'):
                pending=dict(tick=tick,held=h,relocations=t['relocations'],surveys=[])
                rechecks.append(pending)
            elif h['reason']=='native placement rejected':
                report=p['recovery']['placement']
                assert report is not None and h['started_tick']<=report['tick']<=tick
                assert report['planet']==h['site']['planet']==p['planet']['index']
                assert report['revision']==h['site']['revision']==p['planet']['revision']
                assert report['selected_offset'] is None
                failures.append(row)
        if h and h['ended_tick'] is None and p['recovery']['status'] in ('hatch_blocked','clearance_blocked'):
            report=p['recovery']['placement']
            if tick>h['started_tick'] and p['controls_armed'] and p['queries_ready']:
                assert report is None or report['tick']<h['started_tick'] or report['tick']>tick or report['planet']!=h['site']['planet'] or report['revision']!=h['site']['revision'] or report['selected_offset'] is not None
                ignored.append(row)
        if pending:
            survey=row['observation']['rebuild']
            if survey is not None:
                assert not (survey.get('search') or {}).get('recheck_preferred',False)
                coarse=survey['refinement']['coarse_candidates'];refined=survey['attempts'][coarse:]
                if recheck and refined:
                    # The bearing must still exist in the fresh local map and be
                    # within its query range to be considered. If tested, it is first.
                    selected=[a for a in refined if a['bearing']==pending['held']['bearing']]
                    if selected:assert refined[0]['bearing']==pending['held']['bearing']
                pending['surveys'].append(row)
            if t['relocation_site'] is not None:
                assert t['relocations']==pending['relocations']+1
                assert survey is not None and survey['site']==t['relocation_site']
                assert survey['tick']==tick and survey['site']['revision']==p['planet']['revision']
                pending['selected_tick']=tick;pending['site']=survey['site'];pending=None
            elif p['ship_available'] and p['ship_form']=='ship' or t['status']!='running':pending=None
    result=dict(holding=hold,explicit_recheck=recheck,rechecks=rechecks,fresh_failures=failures,ignored_failures=ignored)
    path=root/(fork+'-recheck-audit.json');I.write(path,result)
    return dict(holding=hold,explicit_recheck=recheck,
        rechecks=[dict(tick=r['tick'],bearing=r['held']['bearing'],survey_ticks=[s['tick'] for s in r['surveys']],selected_tick=r.get('selected_tick')) for r in rechecks],
        fresh_failure_ticks=[r['tick'] for r in failures],ignored_failure_ticks=[r['tick'] for r in ignored],sha256=P.digest(path))


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
            base,hold,recheck=MODES[name]
            run['execution']={f:W.audit_execution(root,f,base) for f in ('recorded','live')}
            run['footing']={f:H.audit_footing(root,f,hold) for f in ('recorded','live')}
            run['contacts']={f:C.audit_contacts(root,f,True) for f in ('recorded','live')}
            if not hold:
                previous=json.loads((PRIOR/'summary.json').read_text())['runs']['contact_'+base]
                assert run['hashes']==previous['hashes']
                assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
                run['retained_files']=len(previous['archive']['files'])
            run['recheck']={f:audit_recheck(root,f,hold,recheck) for f in ('recorded','live')}
            # Retain the original frozen-auditor archives when repairing only
            # verification. Runtime bytes are hash checked above, never rerun.
            suffix='-reaudit-'+P.digest(ROOT/OWN[0])[:12] if reaudit else ''
            run['archive']=B.pack(root,out/'archives'/(name+suffix+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,run['forks']['live']['reason'],'at',run['forks']['live']['last_tick'],flush=True)
        outcomes={}
        for name in ('hold_walk','hold_handoff','recheck_walk','recheck_handoff'):
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
