#!/usr/bin/env python3
"""Freeze task-owned rebuild search and staging against retained refinement."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('refinement',Path(__file__).with_name('validate-rebuild-refinement.py'))
F=importlib.util.module_from_spec(spec);spec.loader.exec_module(F)
ROOT,P,I,B,D,L,R=F.ROOT,F.P,F.I,F.B,F.D,F.L,F.R
PRIOR=ROOT/'target/rebuild-refinement/v1'
PROFILE='retained_rebuild_staging_v1'
OWN=('tools/validate-rebuild-staging.py','tools/tests/test_rebuild_staging.py','docs/rebuild-staging-plan.md')
NEW=('scenarios/spacewars/src/surface_sortie/rebuild_placement/staging.rs',
     'crates/spacewars-ai/src/recovery_task/search.rs')
CHANGED={*OWN,*NEW,F.C.NATIVE,F.REFINEMENT,'crates/spacewars-ai/src/recovery_task.rs',
    'crates/spacewars-ai/tests/surface_recovery.rs','scenarios/spacewars/src/surface_sortie/recovery_sensors.rs',*R.NEW}


def inputs():
    return dict(F.inputs(),**{p:P.digest(ROOT/p) for p in (*NEW,*OWN)})


def command(prior,binary,out,enabled):
    cmd=list(prior['commands']['refined']);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    assert cmd[cmd.index('--rebuild-refinement')+1]=='true'
    return cmd+['--rebuild-staging','true' if enabled else 'false']


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
    for name,cmd in plan['commands'].items():
        assert cmd==command(prior,Path(plan['binary']['path']),out/'raw'/name,name=='staged')


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());old=json.loads((PRIOR/'summary.json').read_text())
    assert old['complete'] and old['screen']['decision']=='not_qualified'
    current=inputs();assert prior['inputs'].keys()<=current.keys()
    changed={p for p in current if prior['inputs'].get(p)!=current[p]};assert changed<=CHANGED,changed
    out.mkdir(parents=True,exist_ok=False)
    for name in ('logs','raw','archives'):(out/name).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,changed_inputs=sorted(changed),binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        prior_plan_sha256=P.digest(PRIOR/'plan.json'),prior_summary_sha256=P.digest(PRIOR/'summary.json'),
        commands={n:command(prior,frozen,out/'raw'/n,n=='staged') for n in ('control','staged')},
        activation_tick=23767,seat=1,task_start=16820,end=29421,fresh_games=0,default_promotion=False,
        bounds=dict(prior['bounds'],staging_moves=1,staging_walk_min=2,staging_walk_max=4,
            combined_route_max=28,remaining_route_max=24,staging_route_checks=16,
            missing_site_ticks=300,search_history_ticks=300,search_origin_range=.5),
        build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen retained-refinement control and staged search: two prefixes, four continuations.',flush=True)


def foot_distance(p,position):
    actor=p['actor']['position'];up=p['actor_up'];frame=p['planet']['motion']
    x=actor['x']-up['x']*.45-frame['position']['x'];y=actor['y']-up['y']*.45-frame['position']['y']
    c,s=math.cos(frame['angle']),math.sin(frame['angle'])
    return math.hypot(x*c+y*s-position['x'],-x*s+y*c-position['y'])


def audit_search(root,name,enabled):
    surveys=[];relocations=[];stages=[];arrivals=[];placements={};seen=0;arrived=None;began=None
    for row in D.rows(root/('rebuild-'+name+'.jsonl')):
        task=row['task'];p=row['pilots'][1];survey=row['observation']['rebuild'];stage=task.get('rebuild_staging')
        if not enabled:
            assert 'rebuild_search' not in task and stage is None
        if survey is not None:
            work=survey['refinement'];assert work['coarse_candidates']<=8 and work['refined_candidates']<=8
            assert work['offset_checks']<=240 and work.get('staging_route_checks',0)<=16
            assert len(survey['attempts'])==work['coarse_candidates']+work['refined_candidates']
            assert work['offset_checks']==sum(len(a['placement']['attempts']) for a in survey['attempts'] if a['placement'])
            if not enabled:assert 'search' not in survey and 'staging' not in survey
            if 'search' in survey:
                progress=survey['search'];assert len(progress['visited'])==len(set(progress['visited']))<=512
                assert progress['planet']==p['planet']['index'] and progress['revision']==p['planet']['revision']
            surveys.append(dict(tick=row['tick'],survey=survey))
        if task['relocations']>seen:
            assert task['relocations']==seen+1<=4
            is_stage=stage is not None and stage['started_tick']==row['tick']
            assert (task['relocation_site'] is not None)!=is_stage
            relocations.append(row);seen=task['relocations']
        if stage:
            assert enabled and stage['started_tick']-stage['search_since']<=300
            proposal=stage['proposal'];assert 2<=proposal['walk_length']<=4 and proposal['remaining_length']<=24
            if began is None:began=stage['started_tick'];stages.append(row)
            assert began==stage['started_tick'],'only one staged move'
            if stage['arrived_tick'] is not None and arrived is None:
                arrived=stage['arrived_tick'];assert arrived==row['tick']
                assert task['ground']['goal']=='arrived' and task['ground']['precise_rebuild']
                assert p['supported_planet']==proposal['planet']
                distance=foot_distance(p,proposal['position'])
                # JSON/world coordinates are recomputed in f64 here; native
                # arrival is the stricter f32 <0.12 test in GroundNavigationTask.
                assert distance<.121
                assert arrived-stage['search_since']<=300
                arrivals.append(dict(row=row,reconstructed_foot_distance=distance))
            if stage['arrived_tick'] is None and stage['invalidated_tick'] is None and task['status']=='running':
                assert row['tick']-stage['search_since']<=300
        report=p['recovery']['placement']
        if report:
            assert len(report['attempts'])<=26
            placements[report['tick']]=report
    result=dict(surveys=surveys,relocations=relocations,stages=stages,arrivals=arrivals,
        native_placements=list(placements.values()),
        max_offset_checks=max((s['survey']['refinement']['offset_checks'] for s in surveys),default=0),
        max_staging_route_checks=max((s['survey']['refinement'].get('staging_route_checks',0) for s in surveys),default=0))
    path=root/(name+'-staging-audit.json');I.write(path,result)
    return dict(surveys=len(surveys),relocation_ticks=[r['tick'] for r in relocations],
        staging_ticks=[r['tick'] for r in stages],arrival_ticks=[r['row']['tick'] for r in arrivals],
        max_offset_checks=result['max_offset_checks'],max_staging_route_checks=result['max_staging_route_checks'],sha256=P.digest(path))


def execute(path,reaudit=False):
    out=path.parent;plan=json.loads(path.read_text());verify(plan,out,reaudit)
    if (out/'summary.json').exists():
        assert reaudit;shutil.copy2(out/'summary.json',out/('summary-before-reaudit-'+P.digest(out/'summary.json')[:12]+'.json'))
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
                run=dict(command=cmd,hashes=I.raw_hashes(root),log_sha256=P.digest(log),
                    report=json.loads((root/'rebuild-replay.json').read_text()))
                I.write(marker,run)
            assert all(P.digest(root/p)==h for p,h in run['hashes'].items())
            report=run['report'];run['prefix']=R.audit_prefix(root,plan['tape']['path'],report)
            assert run['prefix']['verified']
            run['forks']={f:R.audit_fork(root,f,report) for f in ('recorded','live')}
            if name=='control':
                previous=json.loads((PRIOR/'summary.json').read_text())['runs']['refined']
                run['refinement']={f:F.audit_refinement(root,f,True) for f in ('recorded','live')}
                assert run['hashes']==previous['hashes']
                assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
                run['retained_files']=len(previous['archive']['files'])
            run['search']={f:audit_search(root,f,name=='staged') for f in ('recorded','live')}
            run['archive']=B.pack(root,out/'archives'/(name+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,run['forks']['live']['reason'],'at',run['forks']['live']['last_tick'],flush=True)
        live=summary['runs']['staged']['report']['forks']['live']
        success=live['reason']=='recovery_complete' and live['pilots'][1]['recovery']['ships_lost']==1 and live['round']['pilots'][1]['health']>0
        verify(plan,out,reaudit)
        summary.update(complete=True,screen=dict(control_retained=True,isolated_recovery_complete=success,
            decision='isolated_chain_validated' if success else 'not_qualified',scored_match=False,default_promotion=False))
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
