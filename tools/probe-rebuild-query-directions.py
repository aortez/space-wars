#!/usr/bin/env python3
"""Pair radial and measured-normal placement queries on retained frozen scenes."""
import argparse
from collections import Counter
import copy
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('standing',Path(__file__).with_name('probe-rebuild-standing-search.py'))
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)
ROOT,P,I,B,D,L=A.ROOT,A.P,A.I,A.B,A.D,A.L
PRIOR=ROOT/'target/rebuild-standing-search/v1'
REFERENCE=ROOT/'docs/data/rebuild-standing-search-v1.json.gz'
PROFILE='retained_rebuild_query_directions_v1'
MODES={n:v[1] for n,v in A.MODES.items()}
NATIVE={'handoff':'contact_normal','coarse':'radial'}
OWN=('tools/probe-rebuild-query-directions.py','tools/tests/test_rebuild_query_directions.py','docs/rebuild-query-directions-plan.md')
CHANGED={*OWN,A.NEW,'crates/spacewars-ai/examples/support/rebuild_replay.rs'}
DIRECTIONS=('contact_normal','radial')


def inputs():return dict(A.inputs(),**{p:P.digest(ROOT/p) for p in OWN})


def command(prior,binary,out,mode):
    cmd=list(prior['commands'][mode]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    assert cmd[cmd.index('--rebuild-forecast-selection')+1]=='true'
    flag=cmd.index('--rebuild-standing-forecast-ticks')
    assert cmd[flag+1]==','.join(map(str,MODES[mode]))
    cmd[flag]='--rebuild-direction-forecast-ticks'
    return cmd


def check_inputs(expected,current,reaudit=False):
    assert expected.keys()==current.keys()
    changed={p for p in expected if expected[p]!=current[p]}
    assert not changed or reaudit and changed<=set(OWN[:2]),changed


def verify(plan,out,reaudit=False):
    check_inputs(plan['inputs'],inputs(),reaudit)
    for n in ('plan','summary'):assert P.digest(PRIOR/(n+'.json'))==plan['prior_'+n+'_sha256']
    assert P.digest(REFERENCE)==plan['reference_sha256']
    for n in ('tape','binary'):assert P.digest(plan[n]['path'])==plan[n]['sha256']
    assert plan['ticks']=={n:list(t) for n,t in MODES.items()} and plan['native_directions']==NATIVE
    assert plan['max_forecasts_per_direction']==64 and plan['max_steps_per_snapshot']==15360 and plan['max_offset_checks_per_snapshot']==6056
    assert plan['horizon_ticks']==120 and plan['start_delay']==0 and not plan['default_promotion'] and plan['fresh_games']==0
    prior=json.loads((PRIOR/'plan.json').read_text());assert plan['commands'].keys()==MODES.keys()
    for n,cmd in plan['commands'].items():assert cmd==command(prior,Path(plan['binary']['path']),out/'raw'/n,n)


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());summary=json.loads((PRIOR/'summary.json').read_text())
    assert summary['complete'] and summary['screen']['play_preserved'] and not summary['screen']['recovery_qualified']
    current=inputs();assert prior['inputs'].keys()<=current.keys()
    changed={p for p in current if prior['inputs'].get(p)!=current[p]};assert changed<=CHANGED,changed
    out.mkdir(parents=True,exist_ok=False)
    for n in ('logs','raw','archives','prior'):(out/n).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,changed_inputs=sorted(changed),binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        prior_plan_sha256=P.digest(PRIOR/'plan.json'),prior_summary_sha256=P.digest(PRIOR/'summary.json'),reference_sha256=P.digest(REFERENCE),
        ticks={n:list(t) for n,t in MODES.items()},native_directions=NATIVE,
        commands={n:command(prior,frozen,out/'raw'/n,n) for n in MODES},max_forecasts_per_direction=64,
        max_steps_per_snapshot=15360,max_offset_checks_per_snapshot=6056,horizon_ticks=120,start_delay=0,
        fresh_games=0,default_promotion=False,build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen two prefixes, four unchanged continuations, five scenes and two directions per scene.',flush=True)


def untimed_report(report):
    r=copy.deepcopy(report)
    for case in r['attempts']:
        for p in case['forecasts']:
            if p['forecast'] is not None:p['forecast']=A.untimed_event(dict(kind='forecast',forecast=p['forecast']))['forecast']
    return r


def fixed_inputs(report):
    r={k:v for k,v in report.items() if k not in ('attempts','forecasts','offset_checks')}
    r['attempts']=[{k:v for k,v in c.items() if k not in ('placement','forecasts')} for c in report['attempts']]
    return r


def analyze(pair,tick,native,reference):
    assert pair['schema']==1 and pair['tick']==tick and pair['seat']==1 and pair['native_direction']==native
    assert all(pair[k] for k in ('physics_unchanged','native_flag_unchanged','same_search_inputs','preview_only'))
    assert not pair['controller_input'] and set(pair['directions'])==set(DIRECTIONS)
    reports=pair['directions'];left,right=[reports[d] for d in DIRECTIONS]
    assert fixed_inputs(left)==fixed_inputs(right)
    assert untimed_report(reports[native])==untimed_report(reference),'native baseline changed beyond explicit timing fields'
    measures={d:A.analyze(reports[d],tick) for d in DIRECTIONS}
    accepted={d:{(a['bearing'],a['offset']):a for a in m['accepted']} for d,m in measures.items()}
    transitions=Counter();pairs=[];angles=[]
    for contact,radial in zip(left['attempts'],right['attempts']):
        if not contact['eligible']:continue
        cr,rr=contact['placement'],radial['placement']
        assert cr['standing']==rr['standing'] and 'radial_up' not in cr
        pos=contact['position'];length=math.hypot(pos['x'],pos['y']);assert length>0
        up=rr['radial_up'];expected={k:pos[k]/length for k in ('x','y')}
        assert math.dist([up[k] for k in ('x','y')],[expected[k] for k in ('x','y')])<1e-5
        normal=contact['normal'];normal_length=math.hypot(normal['x'],normal['y']);assert normal_length>0
        angle=math.degrees(math.acos(max(-1,min(1,sum(expected[k]*normal[k]/normal_length for k in ('x','y'))))))
        angles.append(dict(bearing=contact['bearing'],degrees=angle))
        for ca,ra in zip(cr['attempts'],rr['attempts']):
            assert ca['offset']==ra['offset'];key=(contact['bearing'],ca['offset'])
            outcome=lambda d,a:('rejected:'+a['rejection']) if a['rejection'] else accepted[d][key]['classification']
            lc,rc=outcome('contact_normal',ca),outcome('radial',ra);transitions[lc+' -> '+rc]+=1
            separation=None
            if ca.get('center') is not None and ra.get('center') is not None:
                separation=math.dist([ca['center'][k] for k in ('x','y')],[ra['center'][k] for k in ('x','y')])
            pairs.append(dict(bearing=key[0],offset=key[1],contact_normal=lc,radial=rc,center_distance=separation))
    assert len(pairs)==measures['contact_normal']['placement_sites']*26
    forecasts=sum(m['forecasts'] for m in measures.values());steps=sum(m['physics_steps'] for m in measures.values())
    checks=sum(m['offset_checks'] for m in measures.values())
    assert forecasts<=128 and steps<=15360 and checks<=6056
    return dict(tick=tick,revision=left['revision'],native_direction=native,baseline_matches_except_timing=True,same_search_inputs=True,
        directions=measures,paired_offsets=len(pairs),transitions=dict(transitions),pairs=pairs,query_angles=angles,
        forecasts=forecasts,physics_steps=steps,offset_checks=checks)


def execute(path,reaudit=False):
    out=path.parent;plan=json.loads(path.read_text());verify(plan,out,reaudit)
    if (out/'summary.json').exists():
        assert reaudit
        backup=out/('summary-before-reaudit-'+P.digest(out/'summary.json')[:12]+'.json');assert not backup.exists();shutil.copy2(out/'summary.json',backup)
    prior=json.loads((PRIOR/'summary.json').read_text())
    summary=dict(schema=1,profile=PROFILE,complete=False,reaudit=reaudit,plan_sha256=P.digest(path),runs={},auditor_inputs={p:P.digest(ROOT/p) for p in OWN[:2]})
    try:
        for mode,cmd in plan['commands'].items():
            root=out/'raw'/mode;log=out/'logs'/(mode+'.log');marker=out/(mode+'-raw.json');ticks=MODES[mode];old=prior['runs'][mode]
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
            probes={f'rebuild-direction-forecast-{t}.json' for t in ticks};historical={f'rebuild-standing-forecast-{t}.json' for t in ticks}
            assert run['hashes'].keys()==(old['hashes'].keys()-historical)|probes
            assert run['report']==old['report']
            assert all(run['hashes'][p]==h for p,h in old['hashes'].items() if p not in A.TIMED|historical)
            previous=out/'prior'/mode;L.extract(old['archive'],previous,list(A.TIMED))
            timed={}
            for name in sorted(A.TIMED):
                left=list(D.rows(root/name));right=list(D.rows(previous/name))
                assert [A.untimed_event(e) for e in left]==[A.untimed_event(e) for e in right]
                timed[name]=dict(rows=len(left),new_sha256=P.digest(root/name),prior_sha256=P.digest(previous/name),all_non_timing_fields_equal=True)
            missing=[p for p in old['archive']['files'] if not (root/p).exists()]
            if missing:L.extract(old['archive'],root,missing)
            assert all(P.digest(root/p)==v['sha256'] for p,v in old['archive']['files'].items() if p not in A.TIMED)
            run.update(retained_files=len(old['archive']['files'])-2,timed_logs=timed,prior_audits_reused=True,prior_audits_contain_prior_timings=True,
                prior_standing_reports_preserved=True)
            reports={t:json.loads((root/f'rebuild-direction-forecast-{t}.json').read_text()) for t in ticks}
            references={t:json.loads((root/f'rebuild-standing-forecast-{t}.json').read_text()) for t in ticks}
            rows={r['tick']:r for r in D.rows(root/'rebuild-live.jsonl') if r['tick'] in ticks};assert rows.keys()==reports.keys()
            historical_rows=json.loads((root/'standing-search-audit.json').read_text())['observed_rows']
            assert {str(t):r for t,r in rows.items()}==historical_rows
            measured={t:analyze(r,t,NATIVE[mode],references[t]) for t,r in reports.items()}
            audit=root/'query-directions-audit.json';I.write(audit,dict(measurements=measured,observed_rows=rows))
            run.update(measurements=measured,audit_sha256=P.digest(audit))
            suffix='-reaudit-'+P.digest(ROOT/OWN[0])[:12] if reaudit else ''
            run['archive']=B.pack(root,out/'archives'/(mode+suffix+'.tar.gz'))
            I.write(out/(mode+'-result.json'),run);summary['runs'][mode]=run;I.write(out/'summary.json',summary)
            print(mode,{t:{d:m['directions'][d]['classifications'] for d in DIRECTIONS} for t,m in measured.items()},flush=True)
        verify(plan,out,reaudit)
        summary.update(complete=True,screen=dict(play_preserved=True,native_baselines_preserved=True,same_search_inputs=True,
            coarse_contact_positive=any(m['directions']['contact_normal']['precisely_reachable_positive'] for m in summary['runs']['coarse']['measurements'].values()),
            default_promotion=False,recovery_qualified=False,preview_only=True))
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
