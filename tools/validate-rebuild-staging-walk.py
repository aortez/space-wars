#!/usr/bin/env python3
"""Compare retained staging, continuous staging, and measured-map handoff."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('staging',Path(__file__).with_name('validate-rebuild-staging.py'))
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)
ROOT,P,I,B,D,L,R=S.ROOT,S.P,S.I,S.B,S.D,S.L,S.R
PRIOR=ROOT/'target/rebuild-staging/v1'
PROFILE='retained_rebuild_staging_walk_v1'
MODES={'control':(False,False),'walk':(True,False),'handoff':(True,True)}
OWN=('tools/validate-rebuild-staging-walk.py','tools/tests/test_rebuild_staging_walk.py','docs/rebuild-staging-walk-plan.md')
NEW=('crates/spacewars-ai/src/ground_task/staging.rs',)
CHANGED={*OWN,*NEW,*S.NEW,S.F.C.NATIVE,'crates/spacewars-ai/src/ground_task.rs',
    'crates/spacewars-ai/src/recovery_task.rs','crates/spacewars-ai/tests/surface_recovery.rs',*R.NEW}


def inputs():
    return dict(S.inputs(),**{p:P.digest(ROOT/p) for p in (*NEW,*OWN)})


def command(prior,binary,out,mode):
    cmd=list(prior['commands']['staged']);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    for option in ('--rebuild-refinement','--rebuild-staging'):
        assert cmd[cmd.index(option)+1]=='true'
    walk,handoff=MODES[mode]
    return cmd+['--rebuild-staging-walk',str(walk).lower(),'--rebuild-staging-handoff',str(handoff).lower()]


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
        bounds=dict(prior['bounds'],staging_arrival_radius=.12,staging_handoff_route_queries=1),
        build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen three staging modes: three original prefixes, six continuations.',flush=True)


def endpoint_matches(a,b):
    # The native route accepts endpoints within 0.01. Structs and nested JSON
    # values can serialize the same f32 with different final decimal digits.
    return math.hypot(a['x']-b['x'],a['y']-b['y'])<=.010001


def audit_execution(root,fork,mode):
    walk,handoff=MODES[mode];seeds=[];continuous_ticks=[];stage_ticks=[];first_path=None
    for row in D.rows(root/('rebuild-'+fork+'.jsonl')):
        task=row['task'];stage=task.get('rebuild_staging');ground=task['ground'];p=row['pilots'][1]
        survey=row['observation']['rebuild']
        if survey:
            if survey.get('search'):
                assert survey['search'].get('include_staging_map',False)==handoff
            if survey.get('staging_map'):
                assert handoff and survey.get('staging')
                m=survey['staging_map']
                assert m['tick']==p['tick']==row['tick'] and m['actor']==p['owner']
                assert m['planet']==p['planet']['index'] and m['revision']==p['planet']['revision']
                assert len(m['nodes'])<=512 and len(m['edges'])<=512*6*2
                assert all(e['kind']!='jetpack' for e in m['edges'])
        if ground and ground.get('continuous_walk',False):
            assert walk and stage
            assert ground['destination']=={'rebuild':{'planet':stage['proposal']['planet'],'position':stage['proposal']['position']}}
            continuous_ticks.append(row['tick'])
        if ground and ground.get('staging_seed_tick') is not None:
            assert handoff and stage and ground['staging_seed_tick']==stage['started_tick']
            assert ground['precise_rebuild']
        if stage and stage['started_tick']==row['tick']:
            stage_ticks.append(row['tick'])
            if handoff:
                assert ground['staging_seed_tick']==row['tick']
                assert ground['started_tick'] is None and ground['target']==stage['proposal']['position']
                route=ground['route'];assert route['failure'] is None and not route['partial']
                assert route['jumps']==route['flights']==0 and 2<=route['length']<=4
                nodes={n['id']:n for n in survey['staging_map']['nodes']}
                assert endpoint_matches(nodes[ground['path'][-1]]['position'],stage['proposal']['position'])
                edges={(e['from'],e['to']):e for e in survey['staging_map']['edges']}
                assert all(edges[a,b]['kind']=='walk' for a,b in zip(ground['path'],ground['path'][1:]))
                seeds.append(row)
        if stage and ground and ground['path'] and first_path is None:
            if ground['destination']=={'rebuild':{'planet':stage['proposal']['planet'],'position':stage['proposal']['position']}}:
                first_path=row['tick']
    result=dict(mode=mode,staging_ticks=stage_ticks,seed_rows=seeds,continuous_ticks=continuous_ticks,
        first_staging_path_tick=first_path)
    path=root/(fork+'-staging-walk-audit.json');I.write(path,result)
    return dict(staging_ticks=stage_ticks,seed_ticks=[r['tick'] for r in seeds],
        continuous_rows=len(continuous_ticks),first_staging_path_tick=first_path,sha256=P.digest(path))


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
            run['search']={f:S.audit_search(root,f,True) for f in ('recorded','live')}
            if name=='control':
                previous=json.loads((PRIOR/'summary.json').read_text())['runs']['staged']
                assert run['hashes']==previous['hashes']
                assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
                run['retained_files']=len(previous['archive']['files'])
            run['execution']={f:audit_execution(root,f,name) for f in ('recorded','live')}
            run['archive']=B.pack(root,out/'archives'/(name+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,run['forks']['live']['reason'],'at',run['forks']['live']['last_tick'],flush=True)
        outcomes={}
        for name in ('walk','handoff'):
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
