#!/usr/bin/env python3
"""Inspect the complete measured standing patch without changing retained play."""
import argparse
from collections import Counter
import copy
import gzip
import importlib.util
import json
import math
from pathlib import Path
import shutil
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('selection',Path(__file__).with_name('validate-rebuild-forecast-selection.py'))
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)
ROOT,P,I,B,D,L,N=S.ROOT,S.P,S.I,S.B,S.D,S.L,S.N
PRIOR=ROOT/'target/rebuild-forecast-selection/v2'
REFERENCE=ROOT/'docs/data/rebuild-local-forecast-v1.json.gz'
PROFILE='retained_rebuild_standing_search_v1'
MODES={'handoff':('selection_handoff',(27114,)),'coarse':('selection_coarse',(25373,25440,25560,25710))}
OWN=('tools/probe-rebuild-standing-search.py','tools/tests/test_rebuild_standing_search.py','docs/rebuild-standing-search-plan.md')
NEW='scenarios/spacewars/src/surface_sortie/rebuild_placement/standing_forecast.rs'
CHANGED={*OWN,NEW,'scenarios/spacewars/src/surface_sortie/rebuild_placement.rs',
    'scenarios/spacewars/src/surface_sortie/rebuild_placement/staging.rs',
    'scenarios/spacewars/src/surface_sortie/rebuild_placement/local_forecast.rs',
    'crates/spacewars-ai/examples/support/rebuild_replay.rs'}
OFFSETS=[-8.,-14.,8.,14.]+[s*n*.5 for s in (-1,1) for n in range(17,28)]
TIMED={'rebuild-selection-live.jsonl','rebuild-selection-recorded.jsonl'}


def inputs():return dict(S.inputs(),**{p:P.digest(ROOT/p) for p in (*OWN,NEW)})


def command(prior,binary,out,mode):
    base,ticks=MODES[mode];cmd=list(prior['commands'][base]);cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(out)
    assert cmd[cmd.index('--rebuild-forecast-selection')+1]=='true'
    assert '--rebuild-standing-forecast-ticks' not in cmd
    return cmd+['--rebuild-standing-forecast-ticks',','.join(map(str,ticks))]


def check_inputs(expected,current,reaudit=False):
    assert expected.keys()==current.keys()
    changed={p for p in expected if expected[p]!=current[p]}
    assert not changed or reaudit and changed<=set(OWN[:2]),changed


def verify(plan,out,reaudit=False):
    check_inputs(plan['inputs'],inputs(),reaudit)
    for n in ('plan','summary'):assert P.digest(PRIOR/(n+'.json'))==plan['prior_'+n+'_sha256']
    assert P.digest(REFERENCE)==plan['reference_sha256']
    assert P.digest(plan['tape']['path'])==plan['tape']['sha256']
    assert P.digest(plan['binary']['path'])==plan['binary']['sha256']
    prior=json.loads((PRIOR/'plan.json').read_text())
    assert plan['ticks']=={n:list(v[1]) for n,v in MODES.items()}
    assert set(plan['commands'])==set(MODES)
    for n,cmd in plan['commands'].items():assert cmd==command(prior,Path(plan['binary']['path']),out/'raw'/n,n)


def freeze(out,binary):
    assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
    prior=json.loads((PRIOR/'plan.json').read_text());summary=json.loads((PRIOR/'summary.json').read_text())
    assert summary['complete'] and summary['screen']['decision']=='not_qualified'
    current=inputs();assert prior['inputs'].keys()<=current.keys()
    changed={p for p in current if prior['inputs'].get(p)!=current[p]};assert changed<=CHANGED,changed
    out.mkdir(parents=True,exist_ok=False)
    for n in ('logs','raw','archives','prior'):(out/n).mkdir()
    frozen=out/'surface_mission_soak';shutil.copy2(binary,frozen);frozen.chmod(0o555)
    plan=dict(schema=1,profile=PROFILE,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
        inputs=current,changed_inputs=sorted(changed),binary=dict(path=str(frozen),sha256=P.digest(frozen)),tape=prior['tape'],
        prior_plan_sha256=P.digest(PRIOR/'plan.json'),prior_summary_sha256=P.digest(PRIOR/'summary.json'),reference_sha256=P.digest(REFERENCE),
        ticks={n:list(v[1]) for n,v in MODES.items()},commands={n:command(prior,frozen,out/'raw'/n,n) for n in MODES},
        max_nodes=113,max_forecasts_per_snapshot=64,horizon_ticks=120,start_delay=0,max_offset_checks=3028,
        fresh_games=0,default_promotion=False,build_command=prior['build_command'])
    verify(plan,out);I.write(out/'plan.json',plan)
    print('Frozen two prefixes, four unchanged continuations and five read-only standing snapshots.',flush=True)


def untimed_event(event):
    e=copy.deepcopy(event)
    if e['kind']=='update':
        value=e.pop('elapsed_ms');assert math.isfinite(value) and value>=0
    if e.get('forecast') is not None:
        f=e['forecast'];timing=f.pop('timing');assert all(math.isfinite(v) and v>=0 for v in timing.values())
        for c in f['chunks']:
            value=c.pop('elapsed_ms');assert math.isfinite(value) and value>=0
    return e


def route_kind(case):
    reachable=lambda r:r['diagnostics']['failure'] is None and r['diagnostics']['length']<=24
    if case['already_here']:return 'already_here'
    if reachable(case['precise_route']):return 'precise'
    if case['staging'] is not None:
        s=case['staging'];assert 2<=s['walk_length']<=4 and s['remaining_length']<=24
        assert case['precise_route']['diagnostics']['failure'] is None and 24<=case['precise_route']['diagnostics']['length']<=28
        first,second=case['staging_routes']
        assert reachable(first) and reachable(second)
        assert not first['diagnostics']['jumps'] and not first['diagnostics']['flights']
        assert first['diagnostics']['length']==s['walk_length'] and second['diagnostics']['length']==s['remaining_length']
        return 'staged'
    if reachable(case['coarse_route']):return 'coarse_only'
    return 'unreachable'


def check_forecast(f,tick,offset):
    assert f['tick']==f['launch_tick']==tick and f['start_delay']==f['warmup_steps']==0
    assert f['horizon_ticks']==120 and f['max_steps_per_call']==4 and f['step_nanoseconds']==16666667
    assert f['input']['captured_before_construction'] and not f['input']['live_vehicle_available']
    assert f['read_only'] and not f['production_qualified'] and not f['future_actions_read']
    assert not any(f[k] for k in ('terrain_updates','other_actors','damage_and_hazards'))
    assert f['max_planets']==32 and f['max_planet_colliders']==128 and f['max_shape_parts']==16384 and f['max_local_travel']==16
    assert f['scope']=='selected_planet_geometry_and_gravity_ephemeris'
    assert f['report']['tick']==tick and f['report']['selected_offset']==offset
    assert 0<=f['steps']<=120
    done=0
    for c in f['chunks']:
        assert c['first_step']==done+1 and 1<=c['steps']<=4
        assert math.isfinite(c['elapsed_ms']) and c['elapsed_ms']>=0;done+=c['steps']
    assert done==f['steps']
    samples=f['samples']
    if samples:
        assert f['input']['bodies']==2 and f['input']['colliders']<=131 and f['input']['planet_count']<=32
        assert len(samples)==f['steps']+1 and [s['tick'] for s in samples]==list(range(tick,tick+len(samples)))
        assert all(s['settled']==N.settled(s) for s in samples)
        if f['stop']=='horizon':assert N.events(samples)['first_settled']==f['first_settled_tick']
    else:assert f['steps']==0
    assert f['stop']!='horizon' or f['steps']==120
    value=N.prediction(f['first_settled_tick'],f['steps'],f['stop'])
    assert f['settles_within_horizon']==value
    assert all(math.isfinite(v) and v>=0 for v in f['timing'].values())
    return dict(prediction=value,events=N.events(samples),steps=f['steps'],stop=f['stop'],timing=f['timing'],
        maximum_chunk_ms=max((c['elapsed_ms'] for c in f['chunks']),default=0))


def analyze(report,tick):
    assert report['tick']==tick and report['seat']==1 and report['offsets']==OFFSETS
    assert all(report[k] for k in ('physics_unchanged','actor_unchanged','recovery_unchanged','preview_only'))
    assert not report['controller_input'] and not report['travel_or_build_time_predicted'] and report['max_forecasts']==64
    cases=report['attempts'];nodes=[c for c in cases if not c['already_here']]
    assert len(nodes)<=113 and len(cases)<=114 and sum(c['already_here'] for c in cases)<=1
    assert not any(c['already_here'] for c in cases[1:])
    assert len({c['bearing'] for c in nodes})==len(nodes)
    assert [(c['distance'],c['bearing']) for c in nodes]==sorted((c['distance'],c['bearing']) for c in nodes)
    positions={n['id']:n['position'] for n in report['map']['nodes']};foot=report['foot']
    distance=lambda p:math.dist([p[k] for k in ('x','y')],[foot[k] for k in ('x','y')])
    ids={c['bearing'] for c in nodes}
    assert {n for n,p in positions.items() if distance(p)<23.9999}<=ids
    assert all(c['position']==positions[c['bearing']] and c['distance']<=24 for c in nodes)
    sparse=[]
    for c in nodes:
        if c['distance']>=2 and all(math.dist([c['position'][k] for k in ('x','y')],[p['position'][k] for k in ('x','y')])>=2 for p in sparse):
            sparse.append(c)
            if len(sparse)==32:break
    assert all(c['sparse_candidate']==(c['bearing'] in {p['bearing'] for p in sparse}) for c in nodes)
    routes=Counter();rejections=Counter();predictions=Counter();accepted=[];count=0;steps=0;checked=0;untested=0
    for c in cases:
        assert c['already_here']==(c['bearing'] is None)
        kind=route_kind(c);routes[kind]+=1
        assert c['eligible']==(kind!='unreachable')
        if not c['eligible']:
            assert c['placement'] is None and not c['forecasts'];continue
        checked+=1;r=c['placement'];assert [a['offset'] for a in r['attempts']]==OFFSETS and r['tick']==tick
        assert r['planet']==report['planet'] and r['revision']==report['revision']
        rejections.update(a['rejection'] or 'accepted' for a in r['attempts'])
        good=[a for a in r['attempts'] if a['rejection'] is None]
        assert [a['offset'] for a in good]==[f['offset'] for f in c['forecasts']]
        for a,p in zip(good,c['forecasts']):
            assert a['settling_angle_degrees']<20 and a['route']['failure'] is None and a['route']['length']<=24
            record=dict(bearing=c['bearing'],position=c['position'],distance=c['distance'],route_kind=kind,
                sparse_candidate=c['sparse_candidate'],offset=a['offset'],hatch=a['hatch'],hatch_route=a['route'],
                coarse_route=c['coarse_route']['diagnostics'],precise_route=c['precise_route']['diagnostics'],staging=c['staging'])
            if p['forecast'] is None:
                assert count==64 and p['unavailable']=='diagnostic forecast budget'
                untested+=1;record.update(classification='untested_budget')
            else:
                assert count<64;count+=1;f=p['forecast'];assert S.same_native(f['report']['attempts'],[a])
                m=check_forecast(f,tick,a['offset']);steps+=m['steps']
                classification='positive' if m['prediction'] is True else 'negative' if m['prediction'] is False else 'inconclusive'
                predictions[classification]+=1;record.update(classification=classification,forecast=m)
            accepted.append(record)
    assert report['forecasts']==count<=64 and steps<=7680
    assert report['offset_checks']==checked*26+count<=3028
    return dict(tick=tick,revision=report['revision'],nodes=len(nodes),actual_support=len(cases)-len(nodes),placement_sites=checked,
        offset_checks=report['offset_checks'],routes=dict(routes),rejections=dict(rejections),classifications=dict(predictions),
        forecasts=count,physics_steps=steps,untested_budget=untested,accepted=accepted,
        precisely_reachable_positive=sum(a['classification']=='positive' and a['route_kind'] in ('precise','staged') for a in accepted))


def reference_check(reports,mode):
    bundle=json.loads(gzip.decompress(REFERENCE.read_bytes()));tick=MODES[mode][1][0]
    name='control_'+mode;old=bundle['evidence'][name]['local-forecast-audit.json']['probes'][f'rebuild-local-forecast-live-{tick}.json']
    current=next(c for c in reports[tick]['attempts'] if c['already_here'])
    matching=next(p['forecast'] for p in current['forecasts'] if p['offset']==old['report']['selected_offset'])
    assert matching is not None
    comparison=N.compare(matching['samples'],old['samples'])
    return dict(tick=tick,offset=old['report']['selected_offset'],forecast=matching['settles_within_horizon'],
        reference=old['settles_within_horizon'],classification_agrees=matching['settles_within_horizon']==old['settles_within_horizon'],comparison=comparison)


def execute(path,reaudit=False):
    out=path.parent;plan=json.loads(path.read_text());verify(plan,out,reaudit)
    if (out/'summary.json').exists():
        assert reaudit
        backup=out/('summary-before-reaudit-'+P.digest(out/'summary.json')[:12]+'.json');assert not backup.exists();shutil.copy2(out/'summary.json',backup)
    prior=json.loads((PRIOR/'summary.json').read_text())
    summary=dict(schema=1,profile=PROFILE,complete=False,reaudit=reaudit,plan_sha256=P.digest(path),runs={},
        auditor_inputs={p:P.digest(ROOT/p) for p in OWN[:2]})
    try:
        for mode,cmd in plan['commands'].items():
            root=out/'raw'/mode;log=out/'logs'/(mode+'.log');marker=out/(mode+'-raw.json');base,ticks=MODES[mode];old=prior['runs'][base]
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
            probes={f'rebuild-standing-forecast-{tick}.json' for tick in ticks}
            assert run['hashes'].keys()==old['hashes'].keys()|probes
            assert all(run['hashes'][p]==h for p,h in old['hashes'].items() if p not in TIMED)
            previous=out/'prior'/mode;L.extract(old['archive'],previous,list(TIMED))
            timed={}
            for name in sorted(TIMED):
                left=list(D.rows(root/name));right=list(D.rows(previous/name))
                assert [untimed_event(e) for e in left]==[untimed_event(e) for e in right]
                timed[name]=dict(rows=len(left),new_sha256=P.digest(root/name),prior_sha256=P.digest(previous/name),all_non_timing_fields_equal=True)
            missing=[p for p in old['archive']['files'] if not (root/p).exists()]
            if missing:L.extract(old['archive'],root,missing)
            assert all(P.digest(root/p)==v['sha256'] for p,v in old['archive']['files'].items() if p not in TIMED)
            run.update(retained_files=len(old['archive']['files'])-2,timed_logs=timed,prior_audits_reused=True,prior_audits_contain_prior_timings=True)
            reports={t:json.loads((root/f'rebuild-standing-forecast-{t}.json').read_text()) for t in ticks}
            rows={r['tick']:r for r in D.rows(root/'rebuild-live.jsonl') if r['tick'] in ticks};assert rows.keys()==reports.keys()
            for t,r in reports.items():
                row=rows[t];assert r['revision']==row['pilots'][1]['planet']['revision']
                assert row['task']['started_tick']==16820 and row['task']['ground_budget_ticks']==5400
                assert row['pilots'][1]['recovery']['rebuilds']==0
            measured={t:analyze(r,t) for t,r in reports.items()};reference=reference_check(reports,mode)
            audit=root/'standing-search-audit.json';I.write(audit,dict(measurements=measured,observed_rows=rows,reference=reference))
            run.update(measurements=measured,reference={k:v for k,v in reference.items() if k!='comparison'},audit_sha256=P.digest(audit))
            suffix='-reaudit-'+P.digest(ROOT/OWN[0])[:12] if reaudit else ''
            run['archive']=B.pack(root,out/'archives'/(mode+suffix+'.tar.gz'))
            I.write(out/(mode+'-result.json'),run);summary['runs'][mode]=run;I.write(out/'summary.json',summary)
            print(mode,{t:dict(nodes=m['nodes'],forecasts=m['forecasts'],classifications=m['classifications'],precise_positive=m['precisely_reachable_positive']) for t,m in measured.items()},flush=True)
        verify(plan,out,reaudit)
        summary.update(complete=True,screen=dict(play_preserved=True,reference_classifications_agree=all(r['reference']['classification_agrees'] for r in summary['runs'].values()),
            precisely_reachable_positive_after_rejection=any(m['precisely_reachable_positive'] for t,m in summary['runs']['coarse']['measurements'].items() if int(t)>25373),
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
