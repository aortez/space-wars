#!/usr/bin/env python3
"""Freeze paired old/fixed native recovery replays with an unchanged task history."""
import argparse
import copy
import importlib.util
from itertools import zip_longest
import json
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('settling',Path(__file__).with_name('validate-rebuild-settling.py'))
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)
ROOT,P,I,B,D,L=S.ROOT,S.P,S.I,S.B,S.D,S.L
PROFILE='retained_ledge_recovery_replay_v1'
START,HANDOFF,END,SEAT=16820,23767,29421,1
NATIVE=tuple(sorted(S.CHANGED))
HARNESS=('crates/spacewars-ai/examples/surface_mission_soak.rs',
         'scenarios/spacewars/src/surface_sortie/recovery_sensors.rs')
NEW=('crates/spacewars-ai/examples/support/rebuild_replay.rs',)
OWN=('tools/validate-rebuild-replay.py','tools/tests/test_rebuild_replay.py','docs/rebuild-replay-plan.md')
SOURCE=ROOT/'target/terrain-flight-forecast/v1/summary.json'
PLACEMENT=ROOT/'target/rebuild-settling/v1/summary.json'


def inputs():
    return dict(S.inputs(),**{p:P.digest(ROOT/p) for p in (*NEW,*OWN)})


def project_pilot(p):
    p=copy.deepcopy(p)
    if p.get('recovery'):p['recovery'].pop('placement',None)
    return p


def command(source,binary,out,tape):
    cmd=list(source['runs'][S.A.TARGET]['command'])
    cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    return cmd+['--rebuild-replay-tape',str(tape),'--rebuild-replay-seat',str(SEAT),
        '--rebuild-replay-start',str(START),'--rebuild-replay-handoff',str(HANDOFF),'--rebuild-replay-end',str(END)]


def prepare(out):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    out.mkdir(parents=True,exist_ok=False)
    for name in ('inputs','raw','logs','archives'):(out/name).mkdir()
    source=json.loads(SOURCE.read_text());prior=source['runs'][S.A.TARGET]
    L.extract(prior['archive'],out/'inputs/source',('trace.jsonl','report.json'))
    report=json.loads((out/'inputs/source/report.json').read_text())
    assert P.digest(out/'inputs/source/trace.jsonl')==prior['hashes']['trace.jsonl']
    audits={s['pilots'][0]['tick']:s['audit'] for s in report['samples']}
    rows=iter(D.rows(out/'inputs/source/trace.jsonl'))
    with (out/'inputs/tape.jsonl').open('x') as output:
        for tick in range(END+1):
            a,b=next(rows),next(rows);assert [(r['tick'],r['seat']) for r in (a,b)]==[(tick,0),(tick,1)]
            if START<=tick<=HANDOFF:assert b['mission']['goal']=='recover'
            value=dict(tick=tick,actions=[a['actions'],b['actions']],
                pilots=[L.X.R.pilot(a),L.X.R.pilot(b)],recovery=b['mission']['recovery'],audit=audits.get(tick))
            output.write(json.dumps(value,separators=(',',':'),allow_nan=False)+'\n')
    tree=out/'reference-source'
    subprocess.run(['git','worktree','add','--detach',str(tree),'HEAD'],cwd=ROOT,check=True)
    old=json.loads(SOURCE.read_text())
    # Only the two native placement files differ. All replay instrumentation
    # and controller code is identical in both executable builds.
    for p in NATIVE:
        data=subprocess.check_output(['git','show',old['runtime_source_commit']+':'+p],cwd=ROOT)
        (tree/p).write_bytes(data);assert P.digest(tree/p)==old['inputs'][p]
    I.write(out/'prepared.json',dict(source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=inputs(),source_sha256=P.digest(SOURCE),placement_sha256=P.digest(PLACEMENT),
        tape=dict(path=str(out/'inputs/tape.jsonl'),sha256=P.digest(out/'inputs/tape.jsonl'),rows=END+1),
        reference_source=str(tree),reference_native={p:P.digest(tree/p) for p in NATIVE}))
    print('Prepared fixed tape and reference checkout. Build reference with its own target directory.',flush=True)


def check_inputs(expected,current,reaudit):
    assert expected.keys()==current.keys()
    changed={p for p in expected if expected[p]!=current[p]}
    assert not changed or reaudit and changed<=set(OWN[:2]),changed


def verify(plan,out,reaudit=False):
    assert plan['profile']==PROFILE
    check_inputs(plan['inputs'],inputs(),reaudit)
    assert P.digest(SOURCE)==plan['source_sha256'] and P.digest(PLACEMENT)==plan['placement_sha256']
    assert P.digest(plan['tape']['path'])==plan['tape']['sha256']
    old=json.loads(SOURCE.read_text());tree=Path(plan['reference_source'])
    for p,h in plan['inputs'].items():
        if p.startswith('target/'):continue
        assert P.digest(tree/p)==(old['inputs'][p] if p in NATIVE else h),p
    for name,binary in plan['binaries'].items():
        assert P.digest(binary['path'])==binary['sha256']
        assert plan['commands'][name]==command(old,Path(binary['path']),out/'raw'/name,Path(plan['tape']['path']))
    return old


def freeze(out,binary,reference):
    prepared=json.loads((out/'prepared.json').read_text());assert prepared['inputs']==inputs()
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    assert prepared['source_commit']==subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
    binaries={}
    for name,source in [('reference',reference),('candidate',binary)]:
        target=out/(name+'-surface_mission_soak');assert not target.exists()
        shutil.copy2(source,target);target.chmod(0o555)
        binaries[name]=dict(path=str(target),sha256=P.digest(target))
    old=json.loads(SOURCE.read_text())
    plan=dict(prepared,schema=1,profile=PROFILE,binaries=binaries,default_promotion=False,fresh_games=0,
        commands={n:command(old,Path(b['path']),out/'raw'/n,Path(prepared['tape']['path'])) for n,b in binaries.items()},
        task_start=START,handoff=HANDOFF,end=END,seat=SEAT,
        build_command=['cargo','+1.89.0','build','--release','--locked','-p','spacewars-ai',
            '--example','surface_mission_soak','--features','sensor-profile'],
        reference_target=str(ROOT/'target/rebuild-replay-build/reference'))
    verify(plan,out);assert not (out/'plan.json').exists();I.write(out/'plan.json',plan)
    print('Frozen two prefixes and four continuations; no task reset or new seeds.',flush=True)


def audit_prefix(root,tape,report):
    if report['prefix']['failure'] is not None:return dict(verified=False,failure=report['prefix']['failure'])
    count=0;steps=0;audits=0;witnesses=[]
    expected=iter(D.rows(Path(tape)))
    for row in D.rows(root/'rebuild-prefix.jsonl'):
        original=next(expected);tick=row['tick'];assert tick==original['tick']==count
        assert [project_pilot(p) for p in row['pilots']]==[project_pilot(p) for p in original['pilots']]
        assert row['actions']==original['actions']
        assert row['audit']==original['audit'];audits+=row['audit'] is not None
        if tick>=START:
            assert row['task']==original['recovery'];steps+=1
        else:assert row['task'] is None
        if tick in (START,18278,22932,23226,HANDOFF):witnesses.append(row)
        count+=1
    assert count==HANDOFF+1 and steps==HANDOFF-START+1
    assert report['prefix']['native_rows']==count*2 and report['prefix']['task_steps']==steps
    assert report['prefix']['audit_samples']==audits
    task=report['prefix']['task'];assert task['started_tick']==START and task['ground_budget_ticks']==5400
    assert task['ground']['started_tick']==18278 and task['ground']['crossing']['completed_tick']==23226
    assert report['prefix']['pilots'][1]['planet']['claim']['owner']=='player_2'
    I.write(root/'prefix-audit.json',dict(rows=count,task_steps=steps,audit_samples=audits,witnesses=witnesses))
    return dict(verified=True,rows=count,task_steps=steps,audit_samples=audits,sha256=P.digest(root/'prefix-audit.json'))


def audit_fork(root,name,report):
    rows=0;builds=[];boardings=[];last_rebuilds=0;milestones=[];previous=None;reference_equal=0
    trace=root/('rebuild-'+name+'.jsonl')
    for row in D.rows(trace):
        tick=row['tick'];assert tick==HANDOFF+rows;rows+=1
        assert row['live']==(name=='live')
        assert row['actions'][:3]==row['recorded_actions'][0], 'opponent controls changed'
        assert row['actions'][3:]==(row['generated_actions'] if name=='live' else row['recorded_actions'][1])
        assert row['task']['started_tick']==START and row['task']['ground_budget_ticks']==5400
        assert row['task']['relocations']<=4 and tick<=END
        pilot=row['pilots'][1];n=pilot['recovery']['rebuilds'];angle=pilot['landing']['angle_degrees']
        if n>last_rebuilds:
            builds.append(dict(tick=tick,pilot=pilot,task=row['task']));last_rebuilds=n
        state=[pilot['location'],pilot['ship_form'],n,pilot['landing']['phase'],row['task']['goal'],row['task']['status'],row['task']['relocations']]
        if state!=previous:milestones.append(row);previous=state
        if n and isinstance(pilot['location'],dict) and 'aboard' in pilot['location'] and not boardings:
            assert pilot['ship_form']=='ship' and angle<20 and pilot['landing']['supported_feet']==2
            assert pilot['landing']['settled_seconds']>=.25
            boardings.append(dict(tick=tick,pilot=pilot))
        if row['audit']:
            assert not row['audit']['issues'] and row['audit']['max_speed']<500
        last=row
    result=report['forks'][name];assert rows and last['tick']==result['last_tick']
    assert last['stop']==result['reason'] and not last['applied'] and not result['audit_failures']
    if result['reason']=='recovery_complete':
        assert boardings and result['task']['status']=='succeeded' and result['task']['completed_tick']==last['tick']
    I.write(root/(name+'-audit.json'),dict(rows=rows,builds=builds,boardings=boardings,milestones=milestones))
    return dict(rows=rows,builds=builds,boardings=boardings,reason=result['reason'],last_tick=last['tick'],
        task=result['task'],first_native_difference=result['first_native_difference'] and result['first_native_difference']['tick'],
        first_task_difference=result['first_task_difference'] and result['first_task_difference']['tick'],
        first_action_difference=result['first_action_difference'] and result['first_action_difference']['tick'],
        sha256=P.digest(root/(name+'-audit.json')))


def execute(path,reaudit=False):
    out=path.parent;plan=json.loads(path.read_text());verify(plan,out,reaudit)
    if (out/'summary.json').exists():
        assert reaudit
        backup=out/('summary-before-reaudit-'+P.digest(out/'summary.json')[:12]+'.json')
        assert not backup.exists();shutil.copy2(out/'summary.json',backup)
    summary=dict(schema=1,profile=PROFILE,complete=False,plan_sha256=P.digest(path),runs={},default_promotion=False,
        reaudit=reaudit,auditor_inputs={p:P.digest(ROOT/p) for p in OWN[:2]})
    try:
        for name,command in plan['commands'].items():
            root=out/'raw'/name;log=out/'logs'/(name+'.log');marker=out/(name+'-raw.json')
            if marker.exists():
                assert reaudit
                result=json.loads(marker.read_text());assert result['command']==command and result['log_sha256']==P.digest(log)
                if not root.exists():B.unpack(json.loads((out/(name+'-result.json')).read_text())['archive'],root)
                hashes=result['hashes'];assert all(P.digest(root/p)==h for p,h in hashes.items())
                result['reused_raw']=True
            else:
                assert not root.exists() and not log.exists(), 'partial replays are never retried'
                with log.open('x') as stream:subprocess.run(command,stdout=stream,stderr=stream,check=True,timeout=1800)
                hashes=I.raw_hashes(root);report=json.loads((root/'rebuild-replay.json').read_text())
                result=dict(command=command,hashes=hashes,report=report,log_sha256=P.digest(log))
                I.write(marker,result)
            report=result['report']
            result['prefix']=audit_prefix(root,plan['tape']['path'],report)
            if result['prefix']['verified']:
                result['forks']={f:audit_fork(root,f,report) for f in ('recorded','live')}
            assert all(P.digest(root/p)==h for p,h in hashes.items())
            result['archive']=B.pack(root,out/'archives'/(name+'.tar.gz'))
            I.write(out/(name+'-result.json'),result)
            summary['runs'][name]=result;I.write(out/'summary.json',summary)
            print(name,'prefix',result['prefix']['verified'],'live',result.get('forks',{}).get('live',{}).get('reason'),flush=True)
        old,new=summary['runs']['reference'],summary['runs']['candidate']
        control=old['prefix']['verified'] and old['report']['forks']['recorded']['first_native_difference'] is None
        if control:
            control=old['report']['forks']['live']['first_native_difference'] is None and old['report']['forks']['live']['first_task_difference'] is None and old['report']['forks']['live']['first_action_difference'] is None
        success=control and new['prefix']['verified'] and new['report']['forks']['live']['reason']=='recovery_complete'
        if success:
            p=new['report']['forks']['live']['pilots'][1]
            success=p['recovery']['ships_lost']<=1 and new['report']['forks']['live']['round']['pilots'][1]['health']>0
        verify(plan,out,reaudit)
        summary.update(complete=True,screen=dict(reference_reproduced=control,isolated_recovery_complete=bool(success),
            decision='isolated_chain_validated' if success else 'not_qualified',scored_match=False,default_promotion=False))
    except BaseException:
        summary['error']=traceback.format_exc();raise
    finally:I.write(out/'summary.json',summary)
    print(summary['screen'],flush=True)


if __name__=='__main__':
    assert __debug__
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='action',required=True)
    p=sub.add_parser('prepare');p.add_argument('--out',type=Path,required=True)
    p=sub.add_parser('plan');p.add_argument('--out',type=Path,required=True);p.add_argument('--binary',type=Path,required=True);p.add_argument('--reference',type=Path,required=True)
    p=sub.add_parser('run');p.add_argument('--plan',type=Path,required=True);p.add_argument('--reaudit',action='store_true')
    a=parser.parse_args()
    if a.action=='prepare':prepare(a.out.resolve())
    elif a.action=='plan':freeze(a.out.resolve(),a.binary.resolve(),a.reference.resolve())
    else:execute(a.plan.resolve(),a.reaudit)
