#!/usr/bin/env python3
"""Frozen focused-route delivery trials against the powered mission corpus."""
import argparse
import csv
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
from pathlib import Path
import subprocess

spec=importlib.util.spec_from_file_location('mission',Path(__file__).with_name('validate-powered-mission.py'))
M=importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)
F=M.F


def plan():
    return [item for item in M.plan() if item['delivery']=='shared']


def arguments(item,focused):
    args=M.arguments(item)
    if focused: args += ['--focused-objective-routes','true']
    return args


def publication(row):
    survey=row['landing_objective']
    if survey is None: return None
    planning='jetpack_round_trip' if row['mission']['powered_capture'] else 'joint_round_trip'
    M.audit_publication(row,planning)
    routes=survey['sites']+([survey['actual']]if survey['actual'] else [])
    if survey['validated_routes_only']:
        assert routes and all(r['endpoint'] is not None and r['outbound']['failure'] is None
            and r['returning'] is not None and r['returning']['failure'] is None for r in routes)
    return dict(seat=row['seat'],tick=row['pilot']['tick'],measurement_tick=survey['tick'],
        generation=row['objective_evidence']['generation'],age=row['pilot']['tick']-survey['tick'],
        partial=survey['validated_routes_only'],sites=[r['site'] for r in routes],
        matching_capture_site=any(r['site'] is not None and r['site']==row['capture']['site'] for r in routes) if row['capture'] else False)


def analyze(root,item):
    result=M.analyze(root,item)
    report=json.loads((root/'report.json').read_text())
    assert report['live_objective_planning']['focused_objective_routes'] is True
    assert report['live_objective_planning']['sensor_profile'] in ['live_joint_objective_v7','live_jetpack_objective_v7']
    first={};selected=[];seen=set()
    for row in F.rows(root/'capture-evidence.jsonl'):
        p=publication(row)
        if p is not None:
            first.setdefault((p['seat'],p['generation']),p)
            key=(p['seat'],p['generation'])
            if p['matching_capture_site'] and key not in seen:
                seen.add(key);selected.append(row)
    path=root/'focused-deliveries.json'
    F.D.write(path,dict(schema=1,first_deliveries=list(first.values()),selected_site_witnesses=selected))
    result['delivery']=dict(first_by_request=list(first.values()),selected_requests=len(selected),witness_sha256=F.E.digest(path))
    result['hashes'][path.name]=F.E.digest(path)
    return result


def run(binary,out,item,focused):
    root=out/(item['name']+('-focused' if focused else '-retained'))
    command=[str(binary),*arguments(item,focused),'--out',str(root)]
    result=dict(item=item,command=command)
    try:
        with (out/(root.name+'.log')).open('w') as log:
            subprocess.run(command,check=True,stdout=log,stderr=log,timeout=1800)
        result.update((analyze if focused else M.analyze)(root,item))
    except Exception as error:result['error']=repr(error)
    return result


def root_of(run):
    command=run['command']
    return Path(command[command.index('--out')+1])


def replay_parity(old,new):
    roots=[root_of(r)for r in [old,new]]
    reports=[json.loads((p/'report.json').read_text())for p in roots]
    fields=M.V.EXACT_REPORT_FIELDS+['metrics','policy_configuration','cover_response','destination_retry']
    M.T.report_parity(*reports,fields)
    streams=M.V.EXACT_STREAMS+['capture-evidence.jsonl']
    assert all(F.E.digest(roots[0]/f)==F.E.digest(roots[1]/f)for f in streams)
    assert old['allocation']==new['allocation']
    ledgers=[]
    for root in roots:
        with (root/'live-planning.csv').open()as f:
            ledgers.append([{k:v for k,v in row.items()if k!='dispatch_ms'}for row in csv.DictReader(f)])
    assert ledgers[0]==ledgers[1]
    ignored={'reused_ground','snapshot_total_ms','snapshot_max_ms','validation_total_ms','validation_max_ms'}
    telemetry=[{k:v for k,v in r['live_objective_planning']['telemetry'].items()if k not in ignored}for r in reports]
    assert telemetry[0]==telemetry[1]
    sensor_parity=M.C.audit_sensors(*roots)
    return dict(exact_streams=streams,sensor_parity=sensor_parity,exact_allocation_ledger=True,
        reused_ground_before=reports[0]['live_objective_planning']['telemetry']['reused_ground'],
        reused_ground_after=reports[1]['live_objective_planning']['telemetry']['reused_ground'])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior',type=Path,required=True)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--replay',type=Path,help='require gameplay parity with an earlier focused study')
    args=parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code, tests and plan first'
    prior=json.loads(args.prior.read_text());assert prior['complete']
    replay=json.loads(args.replay.read_text())if args.replay else None
    if replay:
        assert replay['complete'] and replay['plan']==plan()
        for old in replay['runs'].values():
            for filename,digest in old['hashes'].items():assert F.E.digest(root_of(old)/filename)==digest
    binary=args.binary.resolve(strict=True);args.out.mkdir(parents=True,exist_ok=False)
    items=plan()
    for item in items:
        old=prior['runs'][item['name']]
        assert old['item']==item
        for filename,digest in old['hashes'].items():assert F.E.digest(root_of(old)/filename)==digest
    result=dict(schema=1,complete=False,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.E.digest(binary),prior_summary=dict(path=str(args.prior),sha256=F.E.digest(args.prior)),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [M,M.E,M.J,M.T,M.V,M.C,F,F.M,F.E,F.D]},
        runner_sha256=F.E.digest(Path(__file__)),plan=items,retention={},runs={},comparisons={})
    if replay:result['replay_summary']=dict(path=str(args.replay),sha256=F.E.digest(args.replay))
    save=lambda:F.D.write(args.out/'summary.json',result)
    save()
    try:
        for focused,phase in [(False,'directed'),(True,'directed'),(True,'armed')]:
            selected=[p for p in items if p['kind']==phase]
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures=[pool.submit(run,binary,args.out,item,focused)for item in selected]
                for item,future in zip(selected,futures):
                    name=item['name'];new=future.result()
                    (result['runs']if focused else result['retention'])[name]=new;save()
                    assert 'error' not in new,(name,new.get('error'))
                    old=prior['runs'][name]
                    if not focused:
                        before,after=[json.loads((root_of(r)/'report.json').read_text())for r in [old,new]]
                        fields=M.V.EXACT_REPORT_FIELDS+['metrics','policy_configuration','cover_response','destination_retry']
                        M.T.report_parity(before,after,fields)
                        streams=M.V.EXACT_STREAMS+['capture-evidence.jsonl']
                        new['exact_streams']={f:F.E.digest(root_of(new)/f) for f in streams}
                        assert all(sha==old['hashes'][f]for f,sha in new['exact_streams'].items())
                        new['sensor_parity']=M.C.audit_sensors(root_of(old),root_of(new))
                        assert new['allocation']==old['allocation']
                    else:
                        result['comparisons'][name]=M.compare(root_of(old),root_of(new))
                        if replay:new['replay_parity']=replay_parity(replay['runs'][name],new)
                    print(name+(': focused audited'if focused else ': retained exactly'),flush=True);save()
        for item in items:
            old=prior['runs'][item['name']]
            for filename,digest in old['hashes'].items():assert F.E.digest(root_of(old)/filename)==digest
        assert F.E.digest(binary)==result['binary_sha256']
        if replay:
            assert result['comparisons']==replay['comparisons']
            assert F.E.digest(args.replay)==result['replay_summary']['sha256']
            for old in replay['runs'].values():
                for filename,digest in old['hashes'].items():assert F.E.digest(root_of(old)/filename)==digest
        result['complete']=True
    except BaseException as error:
        result['error']=repr(error);raise
    finally:save()


if __name__=='__main__':main()
