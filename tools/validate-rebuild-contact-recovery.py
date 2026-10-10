#!/usr/bin/env python3
"""Execute contact-normal recovery only after the retained radial forecast veto."""
import argparse
import hashlib
import importlib.util
from itertools import zip_longest
import json
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('paired',Path(__file__).with_name('probe-rebuild-query-directions.py'))
Q=importlib.util.module_from_spec(spec);spec.loader.exec_module(Q)
ROOT,P,I,B,D,L,A=Q.ROOT,Q.P,Q.I,Q.B,Q.D,Q.L,Q.A
S=A.S
PRIOR=ROOT/'target/rebuild-query-directions/v1'
REFERENCE=ROOT/'docs/data/rebuild-query-directions-v1.json.gz'
BOUNDS=ROOT/'target/rebuild-forecast-selection/v2/plan.json'
PROFILE='retained_rebuild_contact_recovery_v1'
MODES={'control_handoff':('handoff',False),'control_coarse':('coarse',False),'contact_coarse':('coarse',True)}
TRIGGER=dict(search_tick=25373,negative_tick=25413,exhaustion_tick=25438,activation_tick=25439)
OWN=('tools/validate-rebuild-contact-recovery.py','tools/tests/test_rebuild_contact_recovery.py','docs/rebuild-contact-recovery-plan.md')
CHANGED={*OWN,'crates/spacewars-ai/examples/support/rebuild_replay.rs'}
SWITCH={f'rebuild-contact-switch-{f}.json' for f in ('live','recorded')}


def inputs():return dict(Q.inputs(),**{p:P.digest(ROOT/p) for p in OWN})


def command(prior,binary,out,mode):
    base,enabled=MODES[mode];cmd=list(prior['commands'][base]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    at=cmd.index('--rebuild-direction-forecast-ticks');del cmd[at:at+2]
    assert cmd[cmd.index('--rebuild-forecast-selection')+1]=='true'
    assert cmd[cmd.index('--rebuild-radial-placement')+1]==str(base=='coarse').lower()
    assert '--rebuild-contact-after-veto' not in cmd
    return cmd+['--rebuild-contact-after-veto',str(enabled).lower()]


def check_inputs(expected,current,reaudit=False):
    assert expected.keys()==current.keys()
    changed={p for p in expected if expected[p]!=current[p]}
    assert not changed or reaudit and changed<=set(OWN[:2]),changed


def verify(plan,out,reaudit=False):
    check_inputs(plan['inputs'],inputs(),reaudit)
    for n in ('plan','summary'):assert P.digest(PRIOR/(n+'.json'))==plan['prior_'+n+'_sha256']
    assert P.digest(REFERENCE)==plan['reference_sha256'] and P.digest(BOUNDS)==plan['bounds_source_sha256']
    for n in ('tape','binary'):assert P.digest(plan[n]['path'])==plan[n]['sha256']
    assert plan['expected_trigger']==TRIGGER and plan['seat']==1 and plan['task_start']==16820 and plan['end']==29421
    assert plan['bounds']==json.loads(BOUNDS.read_text())['bounds']
    assert not plan['default_promotion'] and plan['fresh_games']==0
    prior=json.loads((PRIOR/'plan.json').read_text());assert plan['commands'].keys()==MODES.keys()
    for n,cmd in plan['commands'].items():assert cmd==command(prior,Path(plan['binary']['path']),out/'raw'/n,n)


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());summary=json.loads((PRIOR/'summary.json').read_text())
    assert summary['complete'] and summary['screen']['play_preserved'] and summary['screen']['coarse_contact_positive']
    current=inputs();assert prior['inputs'].keys()<=current.keys()
    changed={p for p in current if prior['inputs'].get(p)!=current[p]};assert changed<=CHANGED,changed
    out.mkdir(parents=True,exist_ok=False)
    for n in ('logs','raw','archives','prior'):(out/n).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,changed_inputs=sorted(changed),binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        prior_plan_sha256=P.digest(PRIOR/'plan.json'),prior_summary_sha256=P.digest(PRIOR/'summary.json'),reference_sha256=P.digest(REFERENCE),
        bounds_source_sha256=P.digest(BOUNDS),bounds=json.loads(BOUNDS.read_text())['bounds'],
        commands={n:command(prior,frozen,out/'raw'/n,n) for n in MODES},expected_trigger=TRIGGER,seat=1,task_start=16820,end=29421,
        fresh_games=0,default_promotion=False,build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen two retained controls and one post-veto contact candidate: three prefixes, six continuations.',flush=True)


def check_switch(record,requested,eligible):
    assert record['schema']==1 and record['seat']==1 and record['requested']==requested and record['eligible']==eligible
    assert record['applied']==eligible
    if not eligible:
        assert record['trigger'] is None and record['transition'] is None;return
    assert record['trigger']==TRIGGER
    t=record['transition'];assert t['tick']==TRIGGER['activation_tick'] and t['seat']==1 and t['from']=='radial' and t['to']=='contact_normal'
    assert all(t[k] for k in ('pilot_unchanged','contact_unchanged','task_unchanged','search_unchanged'))
    assert t['task']['started_tick']==16820 and t['task']['ground_budget_ticks']==5400 and t['task']['relocations']==1
    assert t['pilot']['recovery']['rebuilds']==0 and t['pilot']['recovery']['ships_lost']==1


def compare_rows(candidate,control,before):
    digest=hashlib.sha256();count=0;first={};witnesses={};last=[None,None]
    with candidate.open('rb') as a,control.open('rb') as b:
        for left,right in zip_longest(a,b):
            x=json.loads(left) if left else None;y=json.loads(right) if right else None
            if x is not None:last[0]=x['tick']
            if y is not None:last[1]=y['tick']
            tick=(x or y)['tick']
            if tick<before:
                assert left==right,'pre-intervention replay changed'
                assert tick==23767+count;digest.update(left);count+=1
            if tick in (before-1,before):witnesses[str(tick)]=dict(candidate=x,control=y)
            if x is None or y is None:
                first.setdefault('length',dict(tick=tick,candidate=x is not None,control=y is not None));continue
            assert x['tick']==y['tick']
            projections={'row':(x,y),'actions':(x['actions'],y['actions']),
                'physical_pilots':([S.R.project_pilot(p) for p in x['pilots']],[S.R.project_pilot(p) for p in y['pilots']]),
                'task':(x['task'],y['task'])}
            for key,(v,w) in projections.items():
                if v!=w and key not in first:
                    first[key]=dict(tick=tick,candidate=v,control=w)
    assert count==before-23767
    return dict(equal_rows=count,through_tick=before-1,sha256=digest.hexdigest(),first_differences=first,witnesses=witnesses,last_ticks=last)


def check_event_prefix(current,previous):
    current=[A.untimed_event(e) for e in current if e['tick']<TRIGGER['activation_tick']]
    previous=[A.untimed_event(e) for e in previous if e['tick']<TRIGGER['activation_tick']]
    assert current==previous
    negative=[e for e in current if e['kind']=='evaluated' and e['tick']==TRIGGER['negative_tick']]
    assert len(negative)==1 and negative[0]['prediction'] is False and not negative[0]['accepted'] and negative[0]['search_tick']==TRIGGER['search_tick']
    exhausted=[e for e in current if e['kind']=='exhausted']
    assert len(exhausted)==1 and exhausted[0]['tick']==TRIGGER['exhaustion_tick'] and exhausted[0]['search_tick']==TRIGGER['search_tick']
    return dict(events=len(current),negative_tick=TRIGGER['negative_tick'],exhaustion_tick=TRIGGER['exhaustion_tick'],all_non_timing_fields_equal=True)


def audit_orientation(root,fork,radial,activation):
    native={};previews=[];errors=[]
    def check(report):
        enabled=radial and (activation is None or report['tick']<activation)
        projected=dict(report)
        if report.get('anchor') is not None:projected['standing']=report['anchor']
        error=S.Q.check_direction(projected,enabled,23767)
        if error is not None:errors.append(error)
    for row in D.rows(root/('rebuild-'+fork+'.jsonl')):
        report=row['pilots'][1]['recovery']['placement']
        if report is not None and report['tick'] not in native:check(report);native[report['tick']]=report
        survey=row['observation']['rebuild']
        if survey is not None:
            for attempt in survey['attempts']:
                if attempt['placement'] is not None:
                    check(attempt['placement']);previews.append(dict(tick=row['tick'],bearing=attempt['bearing'],report=attempt['placement']))
    path=root/(fork+'-contact-orientation-audit.json')
    I.write(path,dict(initial_radial=radial,activation_tick=activation,native_reports=list(native.values()),preview_reports=previews,maximum_direction_error=max(errors,default=None)))
    return dict(native_reports=len(native),preview_reports=len(previews),maximum_direction_error=max(errors,default=None),sha256=P.digest(path))


def audit_candidate(root,report):
    result={}
    for fork in ('recorded','live'):
        result[fork]=dict(fork=S.R.audit_fork(root,fork,report),search=S.W.S.audit_search(root,fork,True),
            execution=S.W.audit_execution(root,fork,'handoff'),footing=S.H.audit_footing(root,fork,False),
            contacts=S.C.audit_contacts(root,fork,True),recheck=S.F.audit_recheck(root,fork,False,False),
            orientation=audit_orientation(root,fork,True,TRIGGER['activation_tick'] if fork=='live' else None),
            arrival=S.A.audit_arrival(root,fork,True),selection=S.audit_selection(root,fork))
    return result


def execute(path,reaudit=False):
    out=path.parent;plan=json.loads(path.read_text());verify(plan,out,reaudit)
    if (out/'summary.json').exists():
        assert reaudit
        backup=out/('summary-before-reaudit-'+P.digest(out/'summary.json')[:12]+'.json');assert not backup.exists();shutil.copy2(out/'summary.json',backup)
    prior=json.loads((PRIOR/'summary.json').read_text())
    summary=dict(schema=1,profile=PROFILE,complete=False,reaudit=reaudit,plan_sha256=P.digest(path),runs={},auditor_inputs={p:P.digest(ROOT/p) for p in OWN[:2]})
    try:
        for mode,cmd in plan['commands'].items():
            root=out/'raw'/mode;log=out/'logs'/(mode+'.log');marker=out/(mode+'-raw.json');base,enabled=MODES[mode];old=prior['runs'][base]
            if marker.exists():
                assert reaudit;run=json.loads(marker.read_text());assert run['command']==cmd and run['log_sha256']==P.digest(log)
                if not root.exists():B.unpack(json.loads((out/(mode+'-result.json')).read_text())['archive'],root)
                run['reused_raw']=True
            else:
                assert not root.exists() and not log.exists(),'partial simulations are never retried'
                with log.open('x') as stream:subprocess.run(cmd,stdout=stream,stderr=stream,check=True,timeout=1800)
                run=dict(command=cmd,hashes=I.raw_hashes(root),log_sha256=P.digest(log),report=json.loads((root/'rebuild-replay.json').read_text()))
                I.write(marker,run)
            assert all(P.digest(root/p)==h for p,h in run['hashes'].items())
            omitted={p for p in old['hashes'] if p.startswith('rebuild-direction-forecast-')}
            dynamic={p for p in run['hashes'] if p.startswith('rebuild-round-foot-live-')}
            assert run['hashes'].keys()==old['hashes'].keys()-omitted|SWITCH|dynamic
            run['switches']={f:json.loads((root/f'rebuild-contact-switch-{f}.json').read_text()) for f in ('live','recorded')}
            for f,v in run['switches'].items():check_switch(v,enabled,enabled and f=='live')
            assert run['report']['prefix']==old['report']['prefix'] and run['report']['forks']['recorded']==old['report']['forks']['recorded']
            previous=out/'prior'/mode
            L.extract(old['archive'],previous,list(A.TIMED)+(['rebuild-live.jsonl','rebuild-live-contacts.jsonl','prefix-audit.json'] if enabled else []))
            timed={}
            for name in sorted(A.TIMED):
                left=list(D.rows(root/name));right=list(D.rows(previous/name))
                if enabled and name=='rebuild-selection-live.jsonl':timed[name]=check_event_prefix(left,right)
                else:
                    assert [A.untimed_event(e) for e in left]==[A.untimed_event(e) for e in right]
                    timed[name]=dict(events=len(left),all_non_timing_fields_equal=True)
                timed[name].update(new_sha256=P.digest(root/name),prior_sha256=P.digest(previous/name))
            run['timed_logs']=timed
            if not enabled:
                assert run['report']==old['report']
                assert all(run['hashes'][p]==h for p,h in old['hashes'].items() if p not in omitted|A.TIMED)
                missing=[p for p in old['archive']['files'] if not (root/p).exists()]
                if missing:L.extract(old['archive'],root,missing)
                assert all(P.digest(root/p)==v['sha256'] for p,v in old['archive']['files'].items() if p not in A.TIMED)
                run.update(retained_files=len(old['archive']['files'])-2,prior_audits_reused=True,prior_audits_contain_prior_timings=True)
            else:
                mutable={'rebuild-live.jsonl','rebuild-live-contacts.jsonl','rebuild-replay.json',*A.TIMED,*omitted}
                assert all(run['hashes'][p]==h for p,h in old['hashes'].items() if p not in mutable)
                shutil.copy2(previous/'prefix-audit.json',root/'prefix-audit.json')
                run['prefix_reused_after_byte_match']=True
                comparison=compare_rows(root/'rebuild-live.jsonl',previous/'rebuild-live.jsonl',TRIGGER['activation_tick'])
                # Contact evidence has a different schema; preserve its prefix directly.
                before=TRIGGER['activation_tick'];contact_rows=0
                for x,y in zip(D.rows(root/'rebuild-live-contacts.jsonl'),D.rows(previous/'rebuild-live-contacts.jsonl')):
                    if x['tick']>=before:break
                    assert x==y;contact_rows+=1
                assert contact_rows==comparison['equal_rows']
                comparison['contact_rows_equal']=contact_rows
                I.write(root/'contact-continuation-comparison.json',comparison)
                run['comparison']={k:v for k,v in comparison.items() if k not in ('first_differences','witnesses')}
                run['first_differences']={k:v['tick'] for k,v in comparison['first_differences'].items()}
                run['audits']=audit_candidate(root,run['report'])
            suffix='-reaudit-'+P.digest(ROOT/OWN[0])[:12] if reaudit else ''
            run['archive']=B.pack(root,out/'archives'/(mode+suffix+'.tar.gz'))
            I.write(out/(mode+'-result.json'),run);summary['runs'][mode]=run;I.write(out/'summary.json',summary)
            print(mode,run['report']['forks']['live']['reason'],run['report']['forks']['live']['last_tick'],flush=True)
        verify(plan,out,reaudit)
        candidate=summary['runs']['contact_coarse'];live=candidate['report']['forks']['live']
        qualified=live['reason']=='recovery_complete' and live['pilots'][1]['recovery']['ships_lost']==1 and live['round']['pilots'][1]['health']>0
        summary.update(complete=True,screen=dict(controls_retained=True,pre_veto_path_preserved=True,recorded_forks_preserved=True,
            isolated_recovery_complete=qualified,decision='isolated_chain_validated' if qualified else 'not_qualified',
            scored_match=False,default_promotion=False))
    except BaseException:
        summary['error']=traceback.format_exc();raise
    finally:I.write(out/'summary.json',summary)
    print(summary['screen'],flush=True)


if __name__=='__main__':
    assert __debug__
    p=argparse.ArgumentParser(description=__doc__);sub=p.add_subparsers(dest='action',required=True)
    a=sub.add_parser('plan');a.add_argument('--out',type=Path,required=True);a.add_argument('--binary',type=Path,required=True)
    a=sub.add_parser('run');a.add_argument('--plan',type=Path,required=True);a.add_argument('--reaudit',action='store_true')
    args=p.parse_args()
    if args.action=='plan':freeze(args.out.resolve(),args.binary.resolve())
    else:execute(args.plan.resolve(),args.reaudit)
