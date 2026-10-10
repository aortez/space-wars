#!/usr/bin/env python3
"""Execute a live-only measured preview-normal handoff on two retained paths."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('contact',Path(__file__).with_name('validate-rebuild-contact-recovery.py'))
V=importlib.util.module_from_spec(spec);spec.loader.exec_module(V)
ROOT,P,I,B,D,L,A,S=V.ROOT,V.P,V.I,V.B,V.D,V.L,V.A,V.S
PRIOR=ROOT/'target/rebuild-contact-recovery/v1'
REFERENCE=ROOT/'docs/data/rebuild-contact-recovery-v1.json.gz'
PROFILE='retained_rebuild_preview_normal_v1'
MODES={'control_handoff':('control_handoff',False),'control_coarse':('contact_coarse',False),
       'normal_handoff':('control_handoff',True),'normal_coarse':('contact_coarse',True)}
OWN=('tools/validate-rebuild-preview-normal.py','tools/tests/test_rebuild_preview_normal.py','docs/rebuild-preview-normal-plan.md')
NEW='scenarios/spacewars/src/surface_sortie/rebuild_placement/preview_normal.rs'
CHANGED={*OWN,NEW,'crates/spacewars-ai/examples/support/rebuild_replay.rs','crates/spacewars-ai/tests/surface_recovery.rs',
    'scenarios/spacewars/src/surface_sortie.rs','scenarios/spacewars/src/surface_sortie/rebuild_placement.rs',
    'scenarios/spacewars/src/surface_sortie/rebuild_placement/footprint.rs',
    'scenarios/spacewars/src/surface_sortie/rebuild_placement/selection.rs','scenarios/spacewars/src/surface_sortie/recovery.rs'}
ADDED={f'rebuild-preview-normal-{f}.json' for f in ('live','recorded')}
LIMITS=dict(site_distance=1.0,arrival_distance=.12,capture_offset_checks=1,capture_local_maps=2)


def inputs():return dict(V.inputs(),**{p:P.digest(ROOT/p) for p in (*OWN,NEW)})


def command(prior,binary,out,mode):
    base,enabled=MODES[mode];cmd=list(prior['commands'][base]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    for flag in ('--rebuild-forecast-selection','--rebuild-contact-frame','--rebuild-precise-arrival'):
        assert cmd[cmd.index(flag)+1]=='true'
    assert '--rebuild-preview-normal' not in cmd
    return cmd+['--rebuild-preview-normal',str(enabled).lower()]


def check_inputs(expected,current,reaudit=False):
    assert expected.keys()==current.keys()
    changed={p for p in expected if expected[p]!=current[p]}
    assert not changed or reaudit and changed<=set(OWN[:2]),changed


def verify(plan,out,reaudit=False):
    check_inputs(plan['inputs'],inputs(),reaudit)
    for n in ('plan','summary'):assert P.digest(PRIOR/(n+'.json'))==plan['prior_'+n+'_sha256']
    assert P.digest(REFERENCE)==plan['reference_sha256']
    for n in ('tape','binary'):assert P.digest(plan[n]['path'])==plan[n]['sha256']
    prior=json.loads((PRIOR/'plan.json').read_text())
    assert plan['bounds']==prior['bounds'] and plan['limits']==LIMITS and plan['expected_trigger']==V.TRIGGER
    assert plan['seat']==1 and plan['task_start']==16820 and plan['end']==29421
    assert not plan['default_promotion'] and plan['fresh_games']==0 and plan['commands'].keys()==MODES.keys()
    for n,cmd in plan['commands'].items():assert cmd==command(prior,Path(plan['binary']['path']),out/'raw'/n,n)


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());summary=json.loads((PRIOR/'summary.json').read_text())
    assert summary['complete'] and summary['screen']['controls_retained'] and not summary['screen']['isolated_recovery_complete']
    current=inputs();assert prior['inputs'].keys()<=current.keys()
    changed={p for p in current if prior['inputs'].get(p)!=current[p]};assert changed<=CHANGED,changed
    out.mkdir(parents=True,exist_ok=False)
    for n in ('logs','raw','archives','prior'):(out/n).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,changed_inputs=sorted(changed),binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        prior_plan_sha256=P.digest(PRIOR/'plan.json'),prior_summary_sha256=P.digest(PRIOR/'summary.json'),reference_sha256=P.digest(REFERENCE),
        bounds=prior['bounds'],limits=LIMITS,commands={n:command(prior,frozen,out/'raw'/n,n) for n in MODES},
        expected_trigger=V.TRIGGER,seat=1,task_start=16820,end=29421,fresh_games=0,default_promotion=False,build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen two retained controls and two live preview-normal candidates: four prefixes, eight continuations.',flush=True)


def check_record(record,requested,live):
    assert record['schema']==1 and record['seat']==1 and record['requested']==requested and record['eligible']==(requested and live)
    if not record['eligible']:assert record['events']==[]
    previous=0;captured={};arrived={};active=None
    for e in record['events']:
        assert e['tick']>=previous and 23767<=e['tick']<=29421 and e['seat']==1;previous=e['tick']
        c=e['context'];key=c['captured_tick'];assert c['seat']==1 and c['ships_lost']==1
        assert all(math.isfinite(c['normal'][k]) for k in ('x','y')) and abs(math.hypot(*c['normal'].values())-1)<.0001
        if e['kind']=='captured':
            assert active is None and key==e['tick'] and c['arrival_tick'] is None and key not in captured
            assert e['offset_checks']==1 and len(e['validation']['attempts'])==1
            p=e['validation'];assert p['tick']==key and p['planet']==c['planet'] and p['revision']==c['revision']
            assert S.C.distance(p['standing'],c['position'])<.001 and p['selected_offset']==c['offset'] and p['attempts'][0]['rejection'] is None
            assert p.get('radial_up') is None and p.get('preview_normal') is None
            captured[key]=e;active=c
        elif e['kind']=='arrived':
            assert active is not None and key==active['captured_tick'] and key not in arrived and c['arrival_tick']==e['tick']>key
            assert dict(c,arrival_tick=None)==captured[key]['context']
            assert 0<=e['distance']<LIMITS['arrival_distance'] and abs(S.C.distance(e['foot'],c['position'])-e['distance'])<.0001
            arrived[key]=e;active=c
        elif e['kind']=='cleared':
            assert active==c and e['reason'] in ('new_destination','context_changed','destination_cleared_without_arrival');active=None
        else:raise AssertionError(e['kind'])
    assert len(captured)<=4
    return captured,arrived


def check_report(report,captured,arrived):
    c=report.get('preview_normal')
    if c is None:return False
    assert c['captured_tick'] in captured and c['captured_tick'] in arrived
    assert c==arrived[c['captured_tick']]['context']
    assert c['arrival_tick']<report['tick'] and c['planet']==report['planet'] and c['revision']==report['revision']
    assert report.get('radial_up') is None
    assert S.C.distance(report['standing'],c['position'])<=1.001
    if report.get('anchor') is not None:
        assert S.C.distance(report['anchor_up'],c['normal'])<.0001
        assert S.C.distance(report['anchor'],c['position'])<=1.001
    return True


def audit_normal(root,fork,requested):
    record=json.loads((root/f'rebuild-preview-normal-{fork}.json').read_text())
    captured,arrived=check_record(record,requested,fork=='live')
    by_tick={}
    for e in record['events']:by_tick.setdefault(e['tick'],[]).append(e)
    witnesses=[];native={};previous=None;relocations=0;uses=[]
    for row in D.rows(root/f'rebuild-{fork}.jsonl'):
        tick=row['tick'];task=row['task'];p=row['pilots'][1]
        for e in by_tick.get(tick,[]):
            c=e['context']
            if e['kind']=='captured':
                survey=row['observation']['rebuild'];site=task['relocation_site']
                assert task['relocations']==relocations+1 and survey['tick']==tick and survey['site']==site
                assert c['position']==site['position'] and c['planet']==site['planet'] and c['revision']==site['revision']
                selected=next(a['placement'] for a in survey['attempts'] if a['bearing']==c['bearing'])
                attempt=next(a for a in selected['attempts'] if a['offset']==c['offset'])
                assert selected['selected_offset']==c['offset'] and e['validation']['attempts']==[attempt]
                assert selected.get('preview_normal') is None and selected.get('radial_up') is None
            elif e['kind']=='arrived':
                assert previous is not None and previous['task']['relocation_site'] is not None and task['relocation_site'] is None
                assert task['ground']['goal']=='arrived' and task['ground']['precise_rebuild']
                assert p['supported_planet']==c['planet'] and S.W.S.foot_distance(p,c['position'])<.121
            witnesses.append(dict(event=e,row=row))
        report=p['recovery']['placement']
        if report is not None and report['tick'] not in native:
            native[report['tick']]=report
            if check_report(report,captured,arrived):uses.append(dict(kind='native',tick=tick,report=report,row=row))
        survey=row['observation']['rebuild']
        if survey is not None:
            assert all(a['placement'] is None or a['placement'].get('preview_normal') is None for a in survey['attempts'])
        previous=row;relocations=task['relocations']
    events=list(D.rows(root/f'rebuild-selection-{fork}.jsonl'))
    for e in events:
        reports=[e.get('report'),(e.get('revalidation') or {}).get('report'),(e.get('forecast') or {}).get('report')]
        for report in reports:
            if report is not None and check_report(report,captured,arrived):uses.append(dict(kind='selector_'+e['kind'],tick=e['tick'],report=report))
    result=dict(record=record,witnesses=witnesses,uses=uses)
    path=root/f'{fork}-preview-normal-audit.json';I.write(path,result)
    return dict(captures=[e['context'] for e in captured.values()],arrivals=[dict(tick=e['tick'],distance=e['distance'],context=e['context']) for e in arrived.values()],
        clears=[e for e in record['events'] if e['kind']=='cleared'],native_use_ticks=[r['tick'] for r in native.values() if r.get('preview_normal') is not None],
        selector_uses=sum(u['kind'].startswith('selector_') for u in uses),sha256=P.digest(path))


def audit_candidate(root,report,coarse):
    result={}
    for fork in ('recorded','live'):
        result[fork]=dict(fork=S.R.audit_fork(root,fork,report),search=S.W.S.audit_search(root,fork,True),
            execution=S.W.audit_execution(root,fork,'handoff'),footing=S.H.audit_footing(root,fork,False),
            contacts=S.C.audit_contacts(root,fork,True),recheck=S.F.audit_recheck(root,fork,False,False),
            orientation=V.audit_orientation(root,fork,coarse,V.TRIGGER['activation_tick'] if coarse and fork=='live' else None),
            arrival=S.A.audit_arrival(root,fork,True),selection=S.audit_selection(root,fork),normal=audit_normal(root,fork,True))
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
            root=out/'raw'/mode;log=out/'logs'/(mode+'.log');marker=out/(mode+'-raw.json');base,enabled=MODES[mode];old=prior['runs'][base];coarse=base=='contact_coarse'
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
            assert run['report']['prefix']==old['report']['prefix'] and run['report']['forks']['recorded']==old['report']['forks']['recorded']
            records={f:json.loads((root/f'rebuild-preview-normal-{f}.json').read_text()) for f in ('live','recorded')}
            for f,r in records.items():check_record(r,enabled,f=='live')
            previous=out/'prior'/mode
            L.extract(old['archive'],previous,list(A.TIMED)+(['rebuild-live.jsonl','rebuild-live-contacts.jsonl','prefix-audit.json'] if enabled else []))
            boundary=None
            if enabled:
                activations=[e['tick'] for e in records['live']['events'] if e['kind']=='arrived']
                boundary=min(activations)+1 if activations else run['report']['forks']['live']['last_tick']+1
            timed={}
            for name in sorted(A.TIMED):
                left=list(D.rows(root/name));right=list(D.rows(previous/name))
                if enabled and name=='rebuild-selection-live.jsonl':
                    if coarse:V.check_event_prefix(left,right)
                    left=[e for e in left if e['tick']<boundary];right=[e for e in right if e['tick']<boundary]
                assert [A.untimed_event(e) for e in left]==[A.untimed_event(e) for e in right]
                timed[name]=dict(events=len(left),all_non_timing_fields_equal=True,before_tick=boundary if enabled and name=='rebuild-selection-live.jsonl' else None,
                    new_sha256=P.digest(root/name),prior_sha256=P.digest(previous/name))
            run['timed_logs']=timed
            for f in ('live','recorded'):
                assert P.digest(root/f'rebuild-contact-switch-{f}.json')==old['hashes'][f'rebuild-contact-switch-{f}.json']
                V.check_switch(json.loads((root/f'rebuild-contact-switch-{f}.json').read_text()),coarse,coarse and f=='live')
            if not enabled:
                assert run['hashes'].keys()==old['hashes'].keys()|ADDED
                assert run['report']==old['report']
                assert all(run['hashes'][p]==h for p,h in old['hashes'].items() if p not in A.TIMED)
                missing=[p for p in old['archive']['files'] if not (root/p).exists()]
                if missing:L.extract(old['archive'],root,missing)
                assert all(P.digest(root/p)==v['sha256'] for p,v in old['archive']['files'].items() if p not in A.TIMED)
                run.update(retained_files=len(old['archive']['files'])-2,prior_audits_reused=True,prior_audits_contain_prior_timings=True)
            else:
                dynamic=lambda p:p.startswith('rebuild-round-foot-live-')
                mutable={'rebuild-live.jsonl','rebuild-live-contacts.jsonl','rebuild-replay.json','rebuild-selection-live.jsonl'}
                assert {p for p in run['hashes'] if not dynamic(p)}=={p for p in old['hashes'] if not dynamic(p)}|ADDED
                assert all(run['hashes'][p]==h for p,h in old['hashes'].items() if p not in mutable|A.TIMED and not dynamic(p))
                shutil.copy2(previous/'prefix-audit.json',root/'prefix-audit.json');run['prefix_reused_after_byte_match']=True
                comparison=V.compare_rows(root/'rebuild-live.jsonl',previous/'rebuild-live.jsonl',boundary)
                contact_rows=0
                for x,y in zip(D.rows(root/'rebuild-live-contacts.jsonl'),D.rows(previous/'rebuild-live-contacts.jsonl')):
                    if x['tick']>=boundary:break
                    assert x==y;contact_rows+=1
                assert contact_rows==comparison['equal_rows'];comparison['contact_rows_equal']=contact_rows
                I.write(root/'preview-normal-continuation-comparison.json',comparison)
                run['comparison']={k:v for k,v in comparison.items() if k not in ('first_differences','witnesses')}
                run['first_differences']={k:v['tick'] for k,v in comparison['first_differences'].items()}
                run['audits']=audit_candidate(root,run['report'],coarse)
            suffix='-reaudit-'+P.digest(ROOT/OWN[0])[:12] if reaudit else ''
            run['archive']=B.pack(root,out/'archives'/(mode+suffix+'.tar.gz'))
            I.write(out/(mode+'-result.json'),run);summary['runs'][mode]=run;I.write(out/'summary.json',summary)
            print(mode,run['report']['forks']['live']['reason'],run['report']['forks']['live']['last_tick'],flush=True)
        verify(plan,out,reaudit)
        chains={m:r['report']['forks']['live']['reason']=='recovery_complete' and r['report']['forks']['live']['pilots'][1]['recovery']['ships_lost']==1
            and r['report']['forks']['live']['round']['pilots'][1]['health']>0 for m,r in summary['runs'].items() if MODES[m][1]}
        summary.update(complete=True,screen=dict(controls_retained=True,recorded_forks_preserved=True,pre_activation_paths_preserved=True,
            live_chains=chains,decision='isolated_chains_validated' if all(chains.values()) else 'not_qualified',scored_match=False,default_promotion=False))
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
