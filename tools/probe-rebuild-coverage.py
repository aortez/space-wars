#!/usr/bin/env python3
"""Freeze one read-only placement coverage probe on the verified ledge replay."""
import argparse
from collections import Counter
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('replay',Path(__file__).with_name('validate-rebuild-replay.py'))
R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)
ROOT,P,I,B,D,L=R.ROOT,R.P,R.I,R.B,R.D,R.L
PRIOR=ROOT/'target/rebuild-replay/v2'
TICK=24330
PROFILE='retained_rebuild_coverage_v1'
NATIVE='scenarios/spacewars/src/surface_sortie/rebuild_placement.rs'
COVERAGE='scenarios/spacewars/src/surface_sortie/rebuild_placement/coverage.rs'
OWN=('tools/probe-rebuild-coverage.py','tools/tests/test_rebuild_coverage.py','docs/rebuild-coverage-plan.md')


def inputs():
    return dict(R.inputs(),**{p:P.digest(ROOT/p) for p in (COVERAGE,*OWN)})


def command(prior,binary,output):
    result=list(prior['commands']['candidate'])
    result[0]=str(binary);result[result.index('--out')+1]=str(output)
    return result+['--rebuild-coverage-tick',str(TICK)]


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
    assert plan['command']==command(json.loads((PRIOR/'plan.json').read_text()),Path(plan['binary']['path']),out/'raw')


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());summary=json.loads((PRIOR/'summary.json').read_text())
    assert summary['complete'] and summary['screen']['reference_reproduced']
    expected=dict(prior['inputs'],**summary['auditor_inputs'])
    current=inputs();changed={p for p in current if expected.get(p)!=current[p]}
    assert expected.keys()<=current.keys()
    assert changed<={NATIVE,COVERAGE,*R.NEW,*OWN},changed
    B.verify_archive(summary['runs']['candidate']['archive'])
    out.mkdir(parents=True,exist_ok=False)
    for name in ('logs','archives'):(out/name).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        prior_plan_sha256=P.digest(PRIOR/'plan.json'),prior_summary_sha256=P.digest(PRIOR/'summary.json'),
        command=command(prior,frozen,out/'raw'),probe_tick=TICK,fresh_games=0,default_promotion=False,
        changed_inputs=sorted(changed),max_nodes=113,offset_count=26,
        build_command=['cargo','+1.89.0','build','--release','--locked','-p','spacewars-ai','--example','surface_mission_soak','--features','sensor-profile'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen one unchanged replay with one read-only probe at',TICK,flush=True)


def analyze(report):
    assert report['tick']==TICK and report['seat']==1 and report['physics_unchanged']
    offsets=[-8.,-14.,8.,14.]+[s*n*.5 for s in (-1,1) for n in range(17,28)]
    assert report['offsets']==offsets
    nodes=report['attempts'];assert len(nodes)<=113
    assert len({a['node']['id'] for a in nodes})==len(nodes)
    routes=Counter();rejections=Counter();legacy=[];dense=[];checked=0
    for a in nodes:
        assert a['distance']<=24
        route=a['route']['diagnostics'];eligible=route['failure'] is None and route['length']<=24
        assert a['eligible']==eligible
        routes[route['failure'] or ('too_long' if route['length']>24 else 'within_limit')]+=1
        if not eligible:
            assert a['placement'] is None;continue
        checked+=1
        old,new=a['placement']['legacy'],a['placement']['dense']
        assert [p['offset'] for p in new['attempts']]==offsets
        assert old['attempts']==new['attempts'][:4]
        for name,placement,dest in [('legacy',old,legacy),('dense',new,dense)]:
            accepted=[p for p in placement['attempts'] if p['rejection'] is None]
            assert (placement['selected_offset'] is not None)==bool(accepted)
            if accepted:
                assert placement['selected_offset'] in [p['offset'] for p in accepted]
                dest.append(dict(bearing=a['node']['id'],position=a['node']['position'],distance=a['distance'],
                    sparse_candidate=a['node']['id'] in report['sparse_bearings'],
                    outbound=route,selected_offset=placement['selected_offset'],accepted=accepted))
            for pose in accepted:
                assert pose['settling_angle_degrees']<20 and pose['route']['failure'] is None
                assert pose['route']['length']<=24
            if name=='dense':rejections.update(p['rejection'] or 'accepted' for p in placement['attempts'])
    return dict(tick=TICK,nodes=len(nodes),placement_nodes=checked,offset_checks=checked*26,
        route_results=dict(routes),offset_results=dict(rejections),legacy_valid=legacy,dense_valid=dense,
        preview_only=True,default_promotion=False)


def execute(path,reaudit=False):
    out=path.parent;plan=json.loads(path.read_text());verify(plan,out,reaudit)
    root=out/'raw';marker=out/'raw-result.json';log=out/'logs/replay.log'
    if marker.exists():
        assert reaudit
        raw=json.loads(marker.read_text());assert raw['command']==plan['command']
        assert raw['log_sha256']==P.digest(log)
    else:
        assert not root.exists() and not log.exists()
        with log.open('x') as stream:subprocess.run(plan['command'],stdout=stream,stderr=stream,check=True,timeout=1800)
        raw=dict(command=plan['command'],hashes=I.raw_hashes(root),log_sha256=P.digest(log))
        I.write(marker,raw)
    summary=dict(schema=1,profile=PROFILE,complete=False,plan_sha256=P.digest(path),reaudit=reaudit,
        auditor_inputs={p:P.digest(ROOT/p) for p in OWN[:2]},raw=raw)
    if (out/'summary.json').exists():
        assert reaudit;shutil.copy2(out/'summary.json',out/('summary-before-reaudit-'+P.digest(out/'summary.json')[:12]+'.json'))
        if not root.exists():B.unpack(json.loads((out/'summary.json').read_text())['archive'],root)
    try:
        assert all(P.digest(root/p)==h for p,h in raw['hashes'].items())
        prior=json.loads((PRIOR/'summary.json').read_text())['runs']['candidate']
        assert raw['hashes'].keys()==prior['hashes'].keys()|{'rebuild-coverage.json'}
        assert all(raw['hashes'][p]==h for p,h in prior['hashes'].items()),'probe or refactor changed retained replay output'
        report=json.loads((root/'rebuild-replay.json').read_text())
        summary['prefix']=R.audit_prefix(root,plan['tape']['path'],report)
        summary['forks']={name:R.audit_fork(root,name,report) for name in ('recorded','live')}
        assert all(P.digest(root/p)==f['sha256'] for p,f in prior['archive']['files'].items())
        coverage=json.loads((root/'rebuild-coverage.json').read_text())
        row=next(r for r in D.rows(root/'rebuild-live.jsonl') if r['tick']==TICK)
        assert coverage['native_survey']==row['observation']['rebuild']
        summary['coverage']=analyze(coverage)
        summary['retained_files']=len(prior['archive']['files'])
        I.write(root/'coverage-audit.json',summary['coverage'])
        summary['archive']=B.pack(root,out/'archives/replay.tar.gz')
        verify(plan,out,reaudit)
        summary.update(complete=True,decision='preview_found' if summary['coverage']['dense_valid'] else 'no_preview_found',default_promotion=False)
    except BaseException:
        summary['error']=traceback.format_exc();raise
    finally:I.write(out/'summary.json',summary)
    print(summary['decision'],'nodes',summary['coverage']['nodes'],'legacy',len(summary['coverage']['legacy_valid']),
        'dense',len(summary['coverage']['dense_valid']),flush=True)


if __name__=='__main__':
    assert __debug__
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='action',required=True)
    p=sub.add_parser('plan');p.add_argument('--out',type=Path,required=True);p.add_argument('--binary',type=Path,required=True)
    p=sub.add_parser('run');p.add_argument('--plan',type=Path,required=True);p.add_argument('--reaudit',action='store_true')
    a=parser.parse_args()
    if a.action=='plan':freeze(a.out.resolve(),a.binary.resolve())
    else:execute(a.plan.resolve(),a.reaudit)
