#!/usr/bin/env python3
"""Compare rebuild ray predictions and round-foot contacts on unchanged paths."""
import argparse
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('arrival',Path(__file__).with_name('validate-rebuild-precise-arrival.py'))
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)
ROOT,P,I,B,D,L=A.ROOT,A.P,A.I,A.B,A.D,A.L
PRIOR=ROOT/'target/rebuild-precise-arrival/v1'
PROFILE='retained_rebuild_footprint_probe_v1'
MODES=('precise_handoff','precise_coarse')
OWN=('tools/probe-rebuild-footprint.py','tools/tests/test_rebuild_footprint.py','docs/rebuild-footprint-probe-plan.md')
NEW=('scenarios/spacewars/src/surface_sortie/rebuild_placement/footprint.rs',)
CHANGED={*OWN,*NEW,*A.R.NEW,'scenarios/spacewars/src/surface_sortie/rebuild_placement.rs'}


def inputs():
    return dict(A.inputs(),**{p:P.digest(ROOT/p) for p in (*OWN,*NEW)})


def command(prior,binary,out,mode):
    cmd=list(prior['commands'][mode]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    assert '--rebuild-footprint-probe' not in cmd
    return cmd+['--rebuild-footprint-probe','true']


def check_inputs(expected,current,reaudit=False):
    assert expected.keys()==current.keys()
    changed={p for p in expected if expected[p]!=current[p]}
    assert not changed or reaudit and changed<=set(OWN[:2]),changed


def verify(plan,out,reaudit=False):
    check_inputs(plan['inputs'],inputs(),reaudit)
    for name in ('plan','summary'):
        assert P.digest(PRIOR/(name+'.json'))==plan['prior_'+name+'_sha256']
    assert P.digest(plan['tape']['path'])==plan['tape']['sha256']
    assert P.digest(plan['binary']['path'])==plan['binary']['sha256']
    prior=json.loads((PRIOR/'plan.json').read_text())
    assert set(plan['commands'])==set(MODES)
    for name,cmd in plan['commands'].items():
        assert cmd==command(prior,Path(plan['binary']['path']),out/'raw'/name,name)


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());old=json.loads((PRIOR/'summary.json').read_text())
    assert old['complete'] and old['screen']['isolated_recovery_complete']==dict(precise_handoff=True,precise_coarse=False)
    current=inputs();assert prior['inputs'].keys()<=current.keys()
    changed={p for p in current if prior['inputs'].get(p)!=current[p]};assert changed<=CHANGED,changed
    out.mkdir(parents=True,exist_ok=False)
    for name in ('logs','raw','archives'):(out/name).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,changed_inputs=sorted(changed),binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        prior_plan_sha256=P.digest(PRIOR/'plan.json'),prior_summary_sha256=P.digest(PRIOR/'summary.json'),
        commands={n:command(prior,frozen,out/'raw'/n,n) for n in MODES},activation_tick=23767,seat=1,
        task_start=16820,end=29421,fresh_games=0,default_promotion=False,bounds=prior['bounds'],build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen two unchanged paths: two prefixes, four continuations, native-build footprint probes.',flush=True)


def distance(a,b):return math.hypot(a['x']-b['x'],a['y']-b['y'])


def analyze(probe,row):
    assert probe['read_only'] and probe['physics_unchanged'] and probe['tick']==row['tick'] and probe['seat']==1
    assert A.S.same_native(probe['report'],row['pilots'][1]['recovery']['placement'])
    assert set(p['name'] for p in probe['poses'])=={'spawn','predicted_rest','radial_descent'}
    assert len(probe['samples'])==len(probe['sweeps'])==2
    report=probe['report'];selected=next(a for a in report['attempts'] if a['offset']==report['selected_offset'])
    spawn=next(p for p in probe['poses'] if p['name']=='spawn')
    assert distance(spawn['center'],selected['center'])<.001
    rest=next(p for p in probe['poses'] if p['name']=='predicted_rest')
    assert abs(distance(rest['center'],probe['floor'])-5.45)<.001
    return dict(tick=row['tick'],sample_points=[h['point'] for h in probe['samples']],
        rotated_foot_bottoms=[f['bottom'] for f in rest['feet']],
        bottom_to_sample_distances=[distance(f['bottom'],h['point']) for f,h in zip(rest['feet'],probe['samples'])],
        poses=probe['poses'],sweeps=probe['sweeps'])


def audit(root):
    probes={int(p.stem.rsplit('-',1)[1]):json.loads(p.read_text()) for p in root.glob('rebuild-footprint-*.json')}
    assert probes
    result=[];seen=set();witnesses=[];previous=None
    for row in D.rows(root/'rebuild-live.jsonl'):
        p=row['pilots'][1];tick=row['tick']
        if tick in probes:
            result.append(analyze(probes[tick],row));seen.add(tick)
        key=(p['landing']['phase'],p['landing']['supported_feet'],p['planet']['revision'],
            p['recovery']['ships_lost'],row['task']['goal'],row['task']['reason'],row['stop'])
        if tick>=min(probes) and (key!=previous or tick in probes):witnesses.append(row)
        previous=key
    assert seen==probes.keys()
    path=root/'footprint-audit.json';I.write(path,dict(probes=probes,measurements=result,witnesses=witnesses))
    return dict(build_ticks=sorted(probes),measurements=result,sha256=P.digest(path))


def execute(path,reaudit=False):
    out=path.parent;plan=json.loads(path.read_text());verify(plan,out,reaudit)
    if (out/'summary.json').exists():
        assert reaudit
        backup=out/('summary-before-reaudit-'+P.digest(out/'summary.json')[:12]+'.json')
        assert not backup.exists();shutil.copy2(out/'summary.json',backup)
    prior=json.loads((PRIOR/'summary.json').read_text())
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
            previous=prior['runs'][name]
            # Exact runtime retention licenses reuse of the preceding audit
            # files; this is explicitly not a claim that those auditors reran.
            original={p:h for p,h in run['hashes'].items() if not p.startswith('rebuild-footprint-')}
            assert original==previous['hashes']
            missing=[p for p in previous['archive']['files'] if not (root/p).exists()]
            if missing:L.extract(previous['archive'],root,missing)
            assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
            run['retained_files']=len(previous['archive']['files']);assert run['retained_files']==36
            run['audits_reused_after_raw_retention']=True
            run['footprint']=audit(root)
            assert run['footprint']['build_ticks']==[b['tick'] for b in previous['forks']['live']['builds']]
            suffix='-reaudit-'+P.digest(ROOT/OWN[0])[:12] if reaudit else ''
            run['archive']=B.pack(root,out/'archives'/(name+suffix+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,'retained',run['retained_files'],'files; probed',run['footprint']['build_ticks'],flush=True)
        verify(plan,out,reaudit)
        summary.update(complete=True,screen=dict(all_paths_retained=True,read_only=True,default_promotion=False,scored_match=False))
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
