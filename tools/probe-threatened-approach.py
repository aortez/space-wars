#!/usr/bin/env python3
"""Read-only diagnosis of the retained damaged-ship transfer and capture."""
import argparse
import csv
import importlib.util
import json
import math
from pathlib import Path
import subprocess


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


H = module('health', 'validate-pursuit-health.py')
J = module('jetpack', 'probe-capture-jetpack.py')
C, F = J.C, H.C.F
NAME = 'shared-armed-world1-p1-powered'
TICKS = [5490, 5539, 5540, 13770, 13800, 13819, 13820]
START, END = 5488, 14139


def identity(site):
    return site['planet'], site['bearing']


def cover_state(cover):
    if cover is None:
        return 'unknown'
    return 'covered' if cover['grounded'] and cover['approach'] else 'exposed'


def geometry(local, site):
    """Current descent conditions, not an arrival or survival prediction."""
    p = local['combat']['recovery']['flight']['pilot']
    vector = J.vector
    sub = lambda a,b:tuple(x-y for x,y in zip(a,b))
    dot = lambda a,b:sum(x*y for x,y in zip(a,b))
    offset = sub(vector(p['ship']['position']), vector(p['planet']['motion']['position']))
    radius = math.hypot(*offset)
    assert radius > 0
    up = tuple(v/radius for v in offset)
    tangent = -up[1],up[0]
    spin = p['planet']['motion']['spin']
    ground_velocity = tuple(v+w for v,w in zip(vector(p['planet']['motion']['velocity']),
                                              (-spin*offset[1],spin*offset[0])))
    relative = sub(vector(p['ship']['velocity']),ground_velocity)
    target = local['combat']['target']
    distance = math.dist(vector(target['motion']['position']),vector(p['ship']['position'])) if target else None
    exposed = bool(target and not target['ground_occluded'] and distance < 300)
    cover = next((c for c in local['cover'] if c['site']==site['id']),None)
    angle = C.short_angle(p,site)
    lateral = dot(relative,tangent)
    height = dot(sub(vector(p['ship']['position']),vector(site['vehicle_position'])),vector(site['normal']))
    aligned = abs(angle) < C.f32(.2)
    ground_covered = bool(cover and cover['grounded'])
    covered = cover_state(cover)=='covered'
    return dict(short_angle=angle,height=height,altitude=radius-p['planet']['radius'],
        lateral_speed=lateral,enemy_range=distance,exposed=exposed,cover=cover,
        aligned=aligned,descent_gate=aligned and abs(lateral)<18 and
            (covered or ground_covered and height<40 or not exposed))


def audit_probe(row):
    local = row['observation']['local'];p = local['combat']['recovery']['flight']['pilot']
    assert row['world_tick']==p['tick'] and p['owner']==f"player_{row['seat']+1}"
    sites={identity(s['id']):s for s in p['sites']}
    covers={identity(c['site']):c for c in local['cover']}
    assert len(sites)==len(p['sites'])<=64 and len(covers)==len(local['cover'])<=64
    # The old walking-only route diagnostic deliberately rejects this runtime
    # profile. The paired probe separately measures walking and powered routes.
    assert row['route_batches'] is None and row['routes_unknown']=='diagnostic requires joint round-trip planning'
    diagnostic=row['jetpack'];pairs={}
    if diagnostic is None:
        assert row['jetpack_unknown']
    else:
        assert row['jetpack_unknown'] is None and p['site_query']=='survey'
        native={}
        for batch in diagnostic['batches']:
            assert batch['tick']==p['tick'] and batch['actor']==p['owner']
            assert batch['planning']=='jetpack_round_trip' and batch['actual'] is None
            assert 1<=len(batch['sites'])<=8
            for route in batch['sites']:
                key=identity(route['site']);assert key not in native
                native[key]=route
        for pair in diagnostic['sites']:
            key=identity(pair['site']);assert key not in pairs
            pairs[key]=pair
            for mode in ['walking','jetpack']:J.audit_measurement(pair[mode])
            assert pair['jetpack']['route']==native[key]
            assert pair['walking']['forecasts']['started']==0
            if pair['walking']['cost'] is not None:
                assert pair['walking']['route']==pair['jetpack']['route']
            if not diagnostic['equipped']:
                assert pair['walking']['route']==pair['jetpack']['route']
        assert pairs.keys()==native.keys()==sites.keys()
    records=[]
    for key,site in sites.items():
        pair=pairs.get(key)
        solar=[C.solar_plan(local,site,side) for side in [-1,1]]
        # Reconstructed solar geometry has a separate numerical uncertainty
        # band. It never grants live permission or predicts opponent motion.
        solar_clear=any(s is None or min(s[k] for k in
            ['approach_clearance','parked_clearance','departure_clearance'])>C.CLEARANCE_TOLERANCE for s in solar)
        records.append(dict(site=site['id'],cover=covers.get(key),cover_state=cover_state(covers.get(key)),
            walking_cost=pair['walking']['cost'] if pair else None,
            powered_cost=pair['jetpack']['cost'] if pair else None,
            routes_measured=pair is not None,solar_reconstruction=solar,
            solar_clear_beyond_tolerance=solar_clear))
    choice=row['choice']
    if choice is not None:
        assert row['choice_unknown'] is None
        cap=row['mission']['capture'];a=cap['acquisition']
        assert a['reason']=='selected_site' and a['tick']==p['tick']==choice['tick']
        assert choice['checks']==a['checks'] and choice['selected']['site']==cap['site']
        eligible=[r for r in choice['assessments'] if r['rejection'] is None]
        assert choice['selected']==min(eligible,key=lambda r:r['total_score'])
        for assessment in eligible:
            site=sites[identity(assessment['site'])]
            reconstructed=C.solar_plan(local,site,assessment['side'])
            if reconstructed is not None:
                for key in ['arrival_seconds','approach_clearance','parked_clearance','departure_clearance']:
                    C.close(assessment['solar'][key],reconstructed[key],C.CLEARANCE_TOLERANCE)
            cover=covers.get(identity(site['id']))
            penalty=0 if not choice['exposed'] or cover and all(cover[k] for k in ['grounded','approach','departure']) else 4000
            assert assessment['cover_penalty']==penalty
    else:
        assert row['choice_unknown']
    covered=[r for r in records if r['cover_state']=='covered']
    return dict(tick=p['tick'],planet=p['planet']['index'],site_query=p['site_query'],
        choice=choice,choice_unknown=row['choice_unknown'],routes_unknown=row['jetpack_unknown'],
        sites=records,counts=dict(observed=len(records),cover_unknown=sum(r['cover_state']=='unknown' for r in records),
            approach_covered=len(covered),measured_routes=len(pairs),
            covered_powered_round_trips=sum(r['routes_measured'] and r['powered_cost'] is not None for r in covered) if diagnostic else None,
            covered_round_trips_with_solar_clearance=sum(r['powered_cost'] is not None and r['solar_clear_beyond_tolerance'] for r in covered) if diagnostic else None))


def sparse_parity(before, after):
    old=iter(F.rows(before));wanted=next(old,None);matched=0
    for current in F.rows(after):
        if wanted is None:break
        a=(wanted['tick'],wanted['seat']);b=(current['tick'],current['seat'])
        assert b<=a,('missing retained trace',a,b)
        if a==b:
            assert current==wanted,('changed retained trace',a)
            matched+=1;wanted=next(old,None)
    assert wanted is None,'truncated retained trace'
    return matched


def parity(old, new):
    roots=[H.C.P.root_of(r) for r in [old,new]]
    reports=[json.loads((p/'report.json').read_text()) for p in roots]
    H.C.M.T.report_parity(*reports,H.C.M.V.EXACT_REPORT_FIELDS+['metrics','policy_configuration','cover_response','destination_retry','pursuit_health'])
    names=[n for n in H.C.M.V.EXACT_STREAMS+['capture-evidence.jsonl'] if n!='trace.jsonl']
    assert all(F.E.digest(roots[0]/n)==F.E.digest(roots[1]/n) for n in names)
    ledgers=[]
    for root in roots:
        with (root/'live-planning.csv').open() as f:
            ledgers.append([{k:v for k,v in r.items() if k!='dispatch_ms'} for r in csv.DictReader(f)])
    assert ledgers[0]==ledgers[1] and old['allocation']==new['allocation']
    ignored={'reused_ground','snapshot_total_ms','snapshot_max_ms','validation_total_ms','validation_max_ms'}
    telemetry=[{k:v for k,v in r['live_objective_planning']['telemetry'].items() if k not in ignored} for r in reports]
    assert telemetry[0]==telemetry[1]
    return dict(exact_streams=names,exact_allocation_ledger=True,
        retained_trace_rows=sparse_parity(roots[0]/'trace.jsonl',roots[1]/'trace.jsonl'),
        sensor_parity=H.C.M.C.audit_sensors(*roots))


def command(old, binary, root):
    result=list(old['command']);result[0]=str(binary)
    result[result.index('--out')+1]=str(root)
    return result+['--trace-start-tick',str(START),'--trace-end-tick',str(END),
        '--probe-cover-seat','0','--probe-cover-ticks',','.join(map(str,TICKS)),
        '--probe-cover-jetpack','true']


def analyze_trace(root, probe):
    report=json.loads((root/'report.json').read_text())
    wanted={r['world_tick']:r for r in probe['rows']};witnessed={};dense={0:[],1:[]};samples=[]
    for row in F.rows(root/'trace.jsonl'):
        tick=row['tick'];seat=row['seat']
        if START<=tick<END:dense[seat].append(tick)
        if seat!=0:continue
        p=row['observation']['local']['combat']['recovery']['flight']['pilot']
        if tick in wanted:
            assert p['tick']==tick
            assert row['observation']==wanted[tick]['observation'] and row['mission']==wanted[tick]['mission']
            witnessed[tick]=row
        if not 12520<=tick<END:continue
        local=row['observation']['local'];c=local['combat'];cap=row['mission']['capture']
        site=next((s for s in p['sites'] if cap and s['id']==cap['site']),None)
        metrics=geometry(local,site) if site else None
        samples.append(dict(tick=tick,ship_hull=p['ship_health'],ship_form=p['ship_form'],
            mission_goal=row['mission']['goal'],capture_goal=cap and cap['goal'],
            capture_site=cap and cap['site'],capture_since=cap and cap['goal_since'],
            cover_response=cap and cap.get('cover_response'),
            controls=row['controls'],actions=row['actions'],weapons=c['weapons'],
            enemy_range=math.dist(J.vector(c['target']['motion']['position']),J.vector(p['ship']['position'])) if c['target'] else None,
            target=c['target'],geometry=metrics,objective_work=local.get('objective_work'),
            objective_evidence=local.get('objective_evidence')))
    assert set(witnessed)==set(TICKS)
    assert all(ticks==list(range(START,min(END,report['elapsed_ticks']))) for ticks in dense.values())
    return dict(dense_rows=sum(map(len,dense.values())),probe_witnesses=witnessed,samples=samples)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior',type=Path,required=True)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(),'freeze runner, tests and plan first'
    prior=json.loads(args.prior.read_text());assert prior['complete'];old=prior['runs'][NAME]
    verify=lambda:all(F.E.digest(H.C.P.root_of(old)/name)==digest for name,digest in old['hashes'].items())
    assert verify()
    binary=args.binary.resolve(strict=True);assert F.E.digest(binary)==prior['binary_sha256']
    args.out.mkdir(parents=True,exist_ok=False);root=args.out/'diagnostic';cmd=command(old,binary,root)
    result=dict(schema=1,complete=False,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        runtime_commit=prior['source_commit'],binary_sha256=F.E.digest(binary),
        runner_sha256=F.E.digest(Path(__file__)),prior_summary=dict(path=str(args.prior),sha256=F.E.digest(args.prior)),
        plan=dict(case=NAME,ticks=TICKS,dense_start=START,dense_end=END),command=cmd)
    try:
        with (args.out/'diagnostic.log').open('x') as log:
            subprocess.run(cmd,check=True,stdout=log,stderr=log,timeout=1800)
        run=dict(item=old['item'],command=cmd,**H.C.analyze(root,old['item']))
        run['pursuit_health']=H.audit_health(root,old['item'])
        assert run['pursuit_health']==old['pursuit_health']
        run['parity']=parity(old,run)
        probe=json.loads((root/'cover-probe.json').read_text())
        assert probe['requested_world_ticks']==TICKS and not probe['unreached_world_ticks']
        assert [r['world_tick'] for r in probe['rows']]==TICKS
        run['probes']=[audit_probe(row) for row in probe['rows']]
        trace=analyze_trace(root,probe)
        F.D.write(root/'approach-analysis.json',dict(schema=1,probes=run['probes'],**trace))
        run['dense_rows']=trace['dense_rows']
        run['hashes']={p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
        result['run']=run
        assert verify() and F.E.digest(binary)==result['binary_sha256']
        assert F.E.digest(args.prior)==result['prior_summary']['sha256']
        result['complete']=True
        print(json.dumps([dict(tick=r['tick'],counts=r['counts'],routes_unknown=r['routes_unknown']) for r in run['probes']],indent=2))
    except BaseException as error:
        result['error']=repr(error)
        raise
    finally:
        F.D.write(args.out/'summary.json',result)


if __name__=='__main__':
    main()
