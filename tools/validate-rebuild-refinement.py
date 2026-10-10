#!/usr/bin/env python3
"""Freeze default and opt-in bounded refinement on the original recovery task."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('coverage',Path(__file__).with_name('probe-rebuild-coverage.py'))
C=importlib.util.module_from_spec(spec);spec.loader.exec_module(C)
R,ROOT,P,I,B,D,L=C.R,C.ROOT,C.P,C.I,C.B,C.D,C.L
PRIOR=ROOT/'target/rebuild-coverage/v1'
PROFILE='retained_rebuild_refinement_v1'
OWN=('tools/validate-rebuild-refinement.py','tools/tests/test_rebuild_refinement.py','docs/rebuild-refinement-plan.md')
REFINEMENT='scenarios/spacewars/src/surface_sortie/rebuild_placement/refinement.rs'
CHANGED={C.NATIVE,C.COVERAGE,REFINEMENT,'scenarios/spacewars/src/surface_sortie.rs',
    'crates/spacewars-ai/src/ground_task.rs','crates/spacewars-ai/src/ground_task/jetpack.rs',
    'crates/spacewars-ai/src/recovery_task.rs','crates/spacewars-ai/tests/ground_navigation.rs',
    'crates/spacewars-ai/tests/surface_recovery.rs',*R.NEW,*OWN}


def inputs():
    return dict(C.inputs(),**{p:P.digest(ROOT/p) for p in (*OWN,REFINEMENT)})


def command(prior,binary,output,enabled):
    cmd=list(prior['commands']['candidate']);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(output)
    return cmd+['--rebuild-refinement','true' if enabled else 'false']


def check_inputs(expected,current,reaudit=False):
    assert expected.keys()==current.keys()
    changed={p for p in expected if expected[p]!=current[p]}
    assert not changed or reaudit and changed<=set(OWN[:2]),changed


def verify(plan,out,reaudit=False):
    check_inputs(plan['inputs'],inputs(),reaudit)
    assert P.digest(PRIOR/'plan.json')==plan['coverage_plan_sha256']
    assert P.digest(PRIOR/'summary.json')==plan['coverage_summary_sha256']
    assert P.digest(C.PRIOR/'summary.json')==plan['replay_summary_sha256']
    assert P.digest(plan['tape']['path'])==plan['tape']['sha256']
    assert P.digest(plan['binary']['path'])==plan['binary']['sha256']
    prior=json.loads((C.PRIOR/'plan.json').read_text())
    for name,cmd in plan['commands'].items():
        assert cmd==command(prior,Path(plan['binary']['path']),out/'raw'/name,name=='refined')


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());coverage=json.loads((PRIOR/'summary.json').read_text())
    assert coverage['complete'] and coverage['decision']=='preview_found'
    current=inputs();assert prior['inputs'].keys()<=current.keys()
    changed={p for p in current if prior['inputs'].get(p)!=current[p]};assert changed<=CHANGED,changed
    out.mkdir(parents=True,exist_ok=False)
    for name in ('logs','raw','archives'):(out/name).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    replay=json.loads((C.PRIOR/'plan.json').read_text())
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,changed_inputs=sorted(changed),binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        coverage_plan_sha256=P.digest(PRIOR/'plan.json'),coverage_summary_sha256=P.digest(PRIOR/'summary.json'),
        replay_summary_sha256=P.digest(C.PRIOR/'summary.json'),
        commands={name:command(replay,frozen,out/'raw'/name,name=='refined') for name in ('control','refined')},
        activation_tick=23767,seat=1,task_start=16820,end=29421,fresh_games=0,default_promotion=False,
        bounds=dict(coarse_candidates=8,refined_candidates=8,offset_checks=240,extra_offsets=22,
            neighborhood_radius=2,max_route_length=24,precise_route_range=.01,precise_arrival_range=.12),
        build_command=['cargo','+1.89.0','build','--release','--locked','-p','spacewars-ai','--example','surface_mission_soak','--features','sensor-profile'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen control and opt-in refinement: same task, two prefixes, four continuations.',flush=True)


def audit_refinement(root,name,enabled):
    surveys=[];relocations=[];arrivals=[];native_reports={};previous_relocations=0;previous_ground=None
    for row in D.rows(root/('rebuild-'+name+'.jsonl')):
        task=row['task'];p=row['pilots'][1];survey=row['observation']['rebuild']
        if survey is not None:
            work=survey.get('refinement')
            assert (work is not None)==enabled
            if work:
                assert work['coarse_candidates']<=8 and work['refined_candidates']<=8
                assert len(survey['attempts'])==work['coarse_candidates']+work['refined_candidates']
                assert work['offset_checks']==sum(len(a['placement']['attempts']) for a in survey['attempts'] if a['placement'])
                assert work['offset_checks']<=240
            surveys.append(dict(tick=row['tick'],survey=survey))
        if task['relocations']>previous_relocations:
            assert task['relocations']==previous_relocations+1
            assert task['relocation_site'] is not None
            relocations.append(row);previous_relocations=task['relocations']
        ground=task['ground']
        ground_state=(ground and ground['goal'],ground and ground.get('precise_rebuild',False))
        if ground_state!=previous_ground and ground_state==('arrived',True):arrivals.append(row)
        previous_ground=ground_state
        if not enabled:
            assert not ground or not ground.get('precise_rebuild',False)
        placement=p['recovery']['placement']
        if placement:
            assert len(placement['attempts'])<=(26 if enabled else 4)
            native_reports[placement['tick']]=placement
    result=dict(surveys=surveys,relocations=relocations,precise_arrivals=arrivals,
        native_placements=list(native_reports.values()),max_offset_checks=max((s['survey'].get('refinement',{}).get('offset_checks',0) for s in surveys),default=0))
    I.write(root/(name+'-refinement-audit.json'),result)
    return dict(surveys=len(surveys),relocation_ticks=[r['tick'] for r in relocations],
        precise_arrival_ticks=[r['tick'] for r in arrivals],max_offset_checks=result['max_offset_checks'],
        native_reports=len(native_reports),sha256=P.digest(root/(name+'-refinement-audit.json')))


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
            previous=json.loads((C.PRIOR/'summary.json').read_text())['runs']['candidate']
            if name=='control':
                assert run['hashes']==previous['hashes']
                assert all(P.digest(root/p)==r['sha256'] for p,r in previous['archive']['files'].items())
                run['retained_files']=len(previous['archive']['files'])
            run['refinement']={f:audit_refinement(root,f,name=='refined') for f in ('recorded','live')}
            run['archive']=B.pack(root,out/'archives'/(name+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,run['forks']['live']['reason'],'at',run['forks']['live']['last_tick'],flush=True)
        live=summary['runs']['refined']['report']['forks']['live']
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
