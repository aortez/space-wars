#!/usr/bin/env python3
"""Compare native foot-support alignment in rebuild previews and construction."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil
import struct

import subprocess
import traceback

spec=importlib.util.spec_from_file_location('footprint',Path(__file__).with_name('probe-rebuild-footprint.py'))
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)
A=S.A
Q,F,C,H,W=A.Q,A.F,A.C,A.H,A.W
ROOT,P,I,B,D,L,R=A.ROOT,A.P,A.I,A.B,A.D,A.L,A.R
PRIOR=ROOT/'target/rebuild-footprint-probe/v1'
PROFILE='native_rebuild_support_alignment_v1'
MIN_ALIGNMENT=struct.unpack('f',struct.pack('f',.7))[0]
MODES={'control_handoff':('precise_handoff',False),'control_coarse':('precise_coarse',False),
       'support_handoff':('precise_handoff',True),'support_coarse':('precise_coarse',True)}
OWN=('tools/validate-rebuild-support-alignment.py','tools/tests/test_rebuild_support_alignment.py','docs/rebuild-support-alignment-plan.md')
NEW=('scenarios/spacewars/src/surface_sortie/rebuild_placement/support.rs',)
CHANGED={*OWN,*NEW,*R.NEW,'scenarios/spacewars/src/physics.rs','scenarios/spacewars/src/surface_sortie.rs',
         'scenarios/spacewars/src/surface_sortie/rebuild_placement.rs'}


def omitted(path):return path.startswith('rebuild-footprint-') or path=='footprint-audit.json'


def inputs():
    return dict(S.inputs(),**{p:P.digest(ROOT/p) for p in (*OWN,*NEW)})


def command(prior,binary,out,mode):
    base,enabled=MODES[mode]
    cmd=list(prior['commands'][base]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    at=cmd.index('--rebuild-footprint-probe');del cmd[at:at+2]
    assert '--rebuild-support-alignment' not in cmd
    return cmd+['--rebuild-support-alignment',str(enabled).lower()]


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
    assert old['runs']['precise_handoff']['report']['forks']['live']['reason']=='recovery_complete'
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
    print('Frozen two retained controls and two native-support candidates: four prefixes, eight continuations.',flush=True)


def check_report(report,enabled,activation):
    checked=0;rejected=0
    for attempt in report['attempts']:
        values=attempt.get('support_alignments')
        if not enabled or report['tick']<=activation:
            assert values is None
            assert attempt['rejection']!='landing_unsupported'
            continue
        if values is None:
            assert attempt['rejection'] in ('queries_pending','no_ground','landing_misaligned')
            continue
        assert len(values)==2 and all(math.isfinite(a) for a in values)
        checked+=1;valid=all(a>=MIN_ALIGNMENT for a in values)
        if attempt['rejection']=='landing_unsupported':
            assert not valid;rejected+=1
        else:assert valid
    return checked,rejected


def audit_support(root,fork,enabled,activation):
    native={};previews=[];checked=0;rejected=0
    for row in D.rows(root/('rebuild-'+fork+'.jsonl')):
        report=row['pilots'][1]['recovery']['placement'];reports=[]
        if report is not None and report['tick'] not in native:
            native[report['tick']]=report;reports.append(report)
        survey=row['observation']['rebuild']
        if survey is not None:
            for attempt in survey['attempts']:
                report=attempt['placement']
                if report is not None:
                    reports.append(report)
                    previews.append(dict(tick=row['tick'],bearing=attempt['bearing'],report=report))
        for report in reports:
            a,b=check_report(report,enabled,activation);checked+=a;rejected+=b
    path=root/(fork+'-support-audit.json')
    I.write(path,dict(enabled=enabled,native_reports=list(native.values()),preview_reports=previews,
        measured_candidates=checked,unsupported_candidates=rejected))
    return dict(enabled=enabled,native_reports=len(native),preview_reports=len(previews),measured_candidates=checked,
        unsupported_candidates=rejected,sha256=P.digest(path))


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
            radial_case,precise=A.MODES[previous_name]
            source,radial=Q.MODES[radial_case]
            base,hold,recheck=F.MODES[source]
            run['execution']={f:W.audit_execution(root,f,base) for f in ('recorded','live')}
            run['footing']={f:H.audit_footing(root,f,hold) for f in ('recorded','live')}
            run['contacts']={f:C.audit_contacts(root,f,True) for f in ('recorded','live')}
            run['recheck']={f:F.audit_recheck(root,f,hold,recheck) for f in ('recorded','live')}
            run['orientation']={f:Q.audit_orientation(root,f,radial,plan['activation_tick']) for f in ('recorded','live')}
            run['arrival']={f:A.audit_arrival(root,f,precise) for f in ('recorded','live')}
            previous=json.loads((PRIOR/'summary.json').read_text())['runs'][previous_name]
            if not enabled:
                assert run['hashes']=={p:h for p,h in previous['hashes'].items() if not omitted(p)}
                retained={p:v for p,v in previous['archive']['files'].items() if not omitted(p)}
                assert all(P.digest(root/p)==v['sha256'] for p,v in retained.items())
                run['retained_files']=len(retained)
            run['support']={f:audit_support(root,f,enabled,plan['activation_tick']) for f in ('recorded','live')}
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
            decision='isolated_chains_validated' if all(outcomes.values()) else 'not_qualified',scored_match=False,default_promotion=False))
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
