#!/usr/bin/env python3
"""Probe retained placement disagreements without changing their replay paths."""
import argparse
from collections import Counter
import gzip
import importlib.util
import json
import math
from pathlib import Path
import shutil

import subprocess
import traceback

spec=importlib.util.spec_from_file_location('radial',Path(__file__).with_name('validate-rebuild-radial-placement.py'))
Q=importlib.util.module_from_spec(spec);spec.loader.exec_module(Q)
F,C,H,W=Q.F,Q.C,Q.H,Q.W
ROOT,P,I,B,D,L,R=Q.ROOT,Q.P,Q.I,Q.B,Q.D,Q.L,Q.R
PRIOR=ROOT/'target/rebuild-radial-placement/v1'
BUNDLE=ROOT/'docs/data/rebuild-radial-placement-v1.json.gz'
PROFILE='frozen_rebuild_placement_probe_v1'
MODES={'control_handoff':27113,'control_recheck_walk':25829,
       'control_recheck_handoff':26370,'radial_handoff':25373}
OWN=('tools/probe-rebuild-placement.py','tools/tests/test_rebuild_placement_probe.py','docs/rebuild-placement-probe-plan.md')
NEW=('scenarios/spacewars/src/surface_sortie/rebuild_placement/probe.rs',)
CHANGED={*OWN,*NEW,*R.NEW,'scenarios/spacewars/src/surface_sortie/rebuild_placement.rs',
    'scenarios/spacewars/src/surface_sortie/recovery.rs'}


def inputs():
    return dict(Q.inputs(),**{p:P.digest(ROOT/p) for p in (*NEW,*OWN)})


def command(prior,binary,out,mode):
    cmd=list(prior['commands'][mode]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    assert '--rebuild-placement-probe' not in cmd
    return cmd+['--rebuild-placement-probe',str(out.parent.parent/'requests'/(mode+'.json'))]


def requests():
    bundle=json.loads(gzip.decompress(BUNDLE.read_bytes()));result={}
    for name,tick in MODES.items():
        source_tick=27114 if name=='control_handoff' else tick
        witness=next(w for w in bundle['site_witnesses'][name]['live'] if w['tick']==source_tick)
        preview=witness['preview']
        result[name]=dict(tick=tick,preview_tick=preview['tick'],planet=preview['planet'],revision=preview['revision'],
            bearing=witness['bearing'],expected_point=preview['standing'],expected_offset=preview['selected_offset'])
    return result


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
    assert P.digest(BUNDLE)==plan['prior_bundle_sha256']
    for name,value in plan['requests'].items():
        assert json.loads((out/'requests'/(name+'.json')).read_text())==value
    prior=json.loads((PRIOR/'plan.json').read_text())
    assert set(plan['commands'])==set(MODES)
    for name,cmd in plan['commands'].items():
        assert cmd==command(prior,Path(plan['binary']['path']),out/'raw'/name,name)


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());old=json.loads((PRIOR/'summary.json').read_text())
    assert old['complete'] and old['screen']['decision']=='not_qualified'
    assert old['runs']['control_handoff']['forks']['live']['reason']=='recovery_complete'
    current=inputs();assert prior['inputs'].keys()<=current.keys()
    changed={p for p in current if prior['inputs'].get(p)!=current[p]};assert changed<=CHANGED,changed
    out.mkdir(parents=True,exist_ok=False)
    for name in ('logs','raw','archives','requests'):(out/name).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    probe_requests=requests()
    for name,value in probe_requests.items():I.write(out/'requests'/(name+'.json'),value)
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,changed_inputs=sorted(changed),binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        prior_bundle_sha256=P.digest(BUNDLE),requests=probe_requests,prior_plan_sha256=P.digest(PRIOR/'plan.json'),prior_summary_sha256=P.digest(PRIOR/'summary.json'),
        commands={n:command(prior,frozen,out/'raw'/n,n) for n in MODES},
        activation_tick=23767,seat=1,task_start=16820,end=29421,fresh_games=0,default_promotion=False,
        bounds=prior['bounds'],
        build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen four unchanged replay paths and twelve queries per fixed-state probe.',flush=True)


def same_native(a,b):
    if isinstance(a,dict) and isinstance(b,dict):
        return a.keys()==b.keys() and all(same_native(a[k],b[k]) for k in a)
    if isinstance(a,list) and isinstance(b,list):
        return len(a)==len(b) and all(same_native(x,y) for x,y in zip(a,b))
    return F.same_native(a,b)


def analyze(probe,request,anchor):
    assert probe['tick']==request['tick'] and probe['seat']==1 and probe['physics_unchanged'] and probe['preview_only']
    assert same_native(probe['anchor'],anchor) and same_native(anchor['request'],request)
    assert anchor['report']['selected_offset']==request['expected_offset']
    assert anchor['report']['tick']==request['preview_tick'] and anchor['report']['revision']==request['revision']
    cases=probe['cases'];assert len(cases)==12
    assert {(c['map'],c['position'],c['direction']) for c in cases}=={
        (m,p,d) for m in ('actual','preview_extent') for p in ('actual','preview') for d in ('contact','preview','radial')}
    result=[]
    for c in cases:
        report=c['report'];assert report['tick']==request['tick'] and report['planet']==request['planet'] and report['revision']==request['revision']
        assert C.distance(report['standing'],c['query_point'])<.00001
        assert abs(math.hypot(c['query_up']['x'],c['query_up']['y'])-1)<.00001
        expected_point=probe['actual_point'] if c['position']=='actual' else anchor['point']
        assert C.distance(c['query_point'],expected_point)<.001
        accepted=[a for a in report['attempts'] if a['rejection'] is None]
        assert (report['selected_offset'] is not None)==(c['pose'] is not None)==bool(accepted)
        if accepted:
            assert report['selected_offset'] in [a['offset'] for a in accepted]
            for a in accepted:assert a['settling_angle_degrees']<20 and a['route']['failure'] is None and a['route']['length']<=24
        result.append(dict(map=c['map'],position=c['position'],direction=c['direction'],selected_offset=report['selected_offset'],
            accepted_offsets=[a['offset'] for a in accepted],rejections=dict(Counter(a['rejection'] or 'accepted' for a in report['attempts']))))
    direction='radial' if probe['radial_enabled'] else 'contact'
    native=next(c['report'] for c in cases if (c['map'],c['position'],c['direction'])==('actual','actual',direction))
    assert same_native(native['attempts'],probe['baseline']['attempts'])
    assert native['selected_offset']==probe['baseline']['selected_offset']
    old=probe['native_recovery']['placement']
    return dict(tick=request['tick'],preview_tick=request['preview_tick'],bearing=request['bearing'],
        point_distance=C.distance(probe['actual_point'],anchor['point']),radial_enabled=probe['radial_enabled'],cases=result,
        baseline_selected=probe['baseline']['selected_offset'],latched_tick=old['tick'] if old else None,
        fresh_native_match=(same_native(old,probe['baseline']) if old and old['tick']==request['tick'] else None))


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
            previous_name,enabled=Q.MODES[name]
            base,hold,recheck=F.MODES[previous_name]
            run['execution']={f:W.audit_execution(root,f,base) for f in ('recorded','live')}
            run['footing']={f:H.audit_footing(root,f,hold) for f in ('recorded','live')}
            run['contacts']={f:C.audit_contacts(root,f,True) for f in ('recorded','live')}
            run['recheck']={f:F.audit_recheck(root,f,hold,recheck) for f in ('recorded','live')}
            run['orientation']={f:Q.audit_orientation(root,f,enabled,plan['activation_tick']) for f in ('recorded','live')}
            previous=json.loads((PRIOR/'summary.json').read_text())['runs'][name]
            assert set(run['hashes'])==set(previous['hashes'])|{'rebuild-placement-probe.json','rebuild-placement-anchor.json'}
            assert all(run['hashes'][p]==h for p,h in previous['hashes'].items())
            assert all(P.digest(root/p)==v['sha256'] for p,v in previous['archive']['files'].items())
            run['retained_files']=len(previous['archive']['files'])
            probe=json.loads((root/'rebuild-placement-probe.json').read_text())
            anchor=json.loads((root/'rebuild-placement-anchor.json').read_text())
            preview_row=next(r for r in D.rows(root/'rebuild-live.jsonl') if r['tick']==plan['requests'][name]['preview_tick'])
            matched=next(a['placement'] for a in preview_row['observation']['rebuild']['attempts'] if a['bearing']==plan['requests'][name]['bearing'] and a['placement'] and a['placement']['selected_offset'] is not None)
            assert same_native(matched,anchor['report'])
            run['probe']=analyze(probe,plan['requests'][name],anchor)
            I.write(root/'placement-probe-audit.json',run['probe'])
            # Retain the original frozen-auditor archives when repairing only
            # verification. Runtime bytes are hash checked above, never rerun.
            suffix='-reaudit-'+P.digest(ROOT/OWN[0])[:12] if reaudit else ''
            run['archive']=B.pack(root,out/'archives'/(name+suffix+'.tar.gz'))
            I.write(out/(name+'-result.json'),run);summary['runs'][name]=run;I.write(out/'summary.json',summary)
            print(name,run['forks']['live']['reason'],'at',run['forks']['live']['last_tick'],flush=True)
        verify(plan,out,reaudit)
        summary.update(complete=True,screen=dict(all_paths_retained=True,read_only=True,scored_match=False,default_promotion=False))
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
