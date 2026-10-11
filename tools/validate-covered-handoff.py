#!/usr/bin/env python3
"""Frozen covered-request scheduling comparison against initial-cover matches."""
import argparse
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
import csv
import importlib.util
import json
from pathlib import Path
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('initial',Path(__file__).with_name('validate-initial-cover.py'))
I=importlib.util.module_from_spec(spec);spec.loader.exec_module(I)
C,F=I.C,I.F
PROFILE='covered_request_handoff_v1'


def command(old,binary,root,enabled):
    cmd=list(old['command']);assert '--covered-request-handoff-seats' not in cmd
    cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(root)
    if enabled:cmd+=['--covered-request-handoff-seats',str(old['item']['seat'])]
    return cmd


def audit_receipt(row, source, spent, last_charged):
    p=row['pilot'];tick=p['tick'];e=row['objective_evidence'];h=e['covered_handoff']
    local,current=I.witness_observation(dict(row,initial_cover=row['covered_handoff']))
    assert h['tick']==tick==e['tick']==e['measurement_tick']==e['request_tick']
    assert h['previous_measurement_tick']<=h['previous_request_tick']<tick<=h['deadline_tick']
    assert h['deadline_tick']==h['previous_measurement_tick']+120
    assert e['generation']!=h['previous_generation']
    assert e['invalidated_by'] is None and e['submission_deferred_by'] is None
    assert row['landing_objective'] is None and row['objective_work']=='pending'
    assert p['controls_armed'] and p['queries_ready'] and p['ship_available'] and p['ship_form']=='ship'
    assert isinstance(p['location'],dict) and 'aboard' in p['location']
    assert p['landing']['phase']!='landed' and p['landing']['supported_feet']==0
    assert local['combat']['recovery']['flight']['flight']['enabled']
    assert p['site_query']==dict(selected=h['site']) and h['site']['planet']==p['planet']['index']
    assert len(current['sites'])==1
    site=current['sites'][0];assert site['id']==h['site'] and site['revision']==p['planet']['revision']
    cover=next(c for c in row['cover'] if c['site']==h['site'])
    assert cover['grounded'] and cover['approach']
    assert source['pilot']['tick']==h['previous_request_tick'] and source['pilot']['site_query']=='survey'
    assert source['pilot']['planet']['index']==p['planet']['index']
    previous=source['objective_evidence']
    assert previous and previous['objective']['planet']==e['objective']['planet']
    assert previous['objective']['revision']==e['objective']['revision']
    assert previous['objective']['owner']==e['objective']['owner']
    assert abs(previous['objective']['range']-e['objective']['range'])<=.00011
    assert I.math.dist(I.A.J.vector(previous['objective']['position']),I.A.J.vector(e['objective']['position']))<=.00201
    assert spent==dict(graph=h['previous_graph'],physics_queries=h['previous_physics_queries'])
    assert last_charged is None or last_charged<tick
    return h


def audit_handoffs(root,item,enabled):
    report=json.loads((root/'report.json').read_text());live=report['live_objective_planning']
    assert live.get('covered_request_handoff')==(dict(profile=PROFILE,enabled_seats=[item['seat']]) if enabled else None)
    work=defaultdict(lambda:dict(graph=0,physics_queries=0));last_charged={};first_charge={}
    with (root/'live-planning.csv').open() as f:
        for row in csv.DictReader(f):
            if row['task']!='landing_objective':continue
            key=int(row['actor']),int(row['generation']);tick=int(row['tick'])
            work[key]['graph']+=int(row['graph']);work[key]['physics_queries']+=int(row['queries'])
            if int(row['graph']) or int(row['queries']):
                last_charged[key]=tick;first_charge.setdefault(key,tick)
    recent={};witnesses=[];records={};published=set();parents=set();counts=Counter()
    for row in F.rows(root/'capture-evidence.jsonl'):
        p=row['pilot'];tick=p['tick'];seat=row['seat'];e=row.get('objective_evidence') or {}
        recent.setdefault(seat,{})[tick]=row
        recent[seat].pop(tick-122,None)
        h=e.get('covered_handoff');key=seat,e.get('generation')
        if h:
            assert enabled and seat==item['seat']
            assert e['request_tick']==e['measurement_tick']==h['tick']
            assert h['deadline_tick']==h['previous_measurement_tick']+120
            event=h['tick']==tick
            assert ('covered_handoff' in row)==event
            if event:
                parent=seat,h['previous_generation']
                assert parent not in parents and parent not in published and key not in records
                source=recent[seat][h['previous_request_tick']]
                audit_receipt(row,source,work[parent],last_charged.get(parent))
                assert first_charge.get(key,tick)>=tick
                records[key]=dict(seat=seat,generation=key[1],receipt=h,
                    first_publication_tick=None,first_selected_tick=None,last_tick=tick)
                parents.add(parent);counts[seat]+=1
                witnesses.append(dict(kind='handoff',source=source,row=row))
            assert key in records and records[key]['receipt']==h
            record=records[key];record['last_tick']=tick
            if row.get('landing_objective'):
                assert tick<=h['deadline_tick']
                survey=row['landing_objective'];assert survey['tick']==h['tick']
                assert all(r['site']==h['site'] for r in survey['sites'])
                if record['first_publication_tick'] is None:
                    record['first_publication_tick']=tick
                    witnesses.append(dict(kind='first_publication',row=row))
                if (row.get('capture') or {}).get('site')==h['site'] and record['first_selected_tick'] is None:
                    record['first_selected_tick']=tick
                    witnesses.append(dict(kind='first_selection',row=row))
        else:
            assert 'covered_handoff' not in row
        if e.get('generation') is not None and row.get('landing_objective'):published.add(key)
    assert live['telemetry'].get('covered_request_handoffs',{})=={str(k):v for k,v in counts.items()}
    path=root/'covered-handoff-witnesses.json'
    F.D.write(path,dict(schema=1,profile=PROFILE,records=list(records.values()),rows=witnesses))
    return dict(count=sum(counts.values()),by_seat=dict(counts),
        first_tick=min((r['receipt']['tick'] for r in records.values()),default=None),
        publications=sum(r['first_publication_tick'] is not None for r in records.values()),
        selected=sum(r['first_selected_tick'] is not None for r in records.values()),
        retired_work={k:sum(r['receipt']['previous_'+k] for r in records.values()) for k in ['graph','physics_queries']},
        records=list(records.values()),witness_sha256=F.E.digest(path))


def run(entry,old,binary,out,enabled):
    root=out/(entry['name']+('-handoff' if enabled else '-retained'))
    active=enabled and entry['enabled'];cmd=command(old,binary,root,active)
    result=dict(item=old['item'],command=cmd)
    try:
        with (out/(root.name+'.log')).open('x') as log:
            subprocess.run(cmd,check=True,stdout=log,stderr=log,timeout=1800)
        result.update(C.analyze(root,entry['item']))
        result['initial_cover']=I.audit_initial(root,entry['item'],entry['enabled'])
        if entry['source']=='health':result['pursuit_health']=I.H.audit_health(root,entry['item'])
        if enabled:
            result['handoff']=audit_handoffs(root,entry['item'],active)
            result['comparison']=C.M.compare(C.P.root_of(old),root)
            difference=result['comparison']['first_control_difference']
            if result['handoff']['count']==0:
                result['unchanged_state']=C.P.replay_parity(old,result)
            if difference:
                assert result['handoff']['first_tick'] is not None and difference['tick']>=result['handoff']['first_tick']
        else:
            result['replay_parity']=C.P.replay_parity(old,result)
            assert result['initial_cover']==old['initial_cover']
            if entry['source']=='health':assert result['pursuit_health']==old['pursuit_health']
        result['hashes']={p.name:F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    except Exception:
        result['error']=traceback.format_exc()
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior',type=Path,required=True)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(),'freeze code, tests and plan first'
    prior=json.loads(args.prior.read_text());assert prior['complete'] and prior['plan']==I.plan()
    I.H.verify_inputs(prior)
    binary=args.binary.resolve(strict=True);args.out.mkdir(parents=True,exist_ok=False)
    result=dict(schema=1,complete=False,profile=PROFILE,
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.E.digest(binary),prior_summary=dict(path=str(args.prior),sha256=F.E.digest(args.prior)),
        runner_sha256=F.E.digest(Path(__file__)),
        tools={name:F.E.digest(Path(__file__).with_name(name)) for name in set(prior['tools'])|{'validate-initial-cover.py'}},
        plan=I.plan(),retention={},runs={})
    save=lambda:F.D.write(args.out/'summary.json',result)
    save()
    try:
        for enabled in [False,True]:
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures=[pool.submit(run,e,prior['runs'][e['name']],binary,args.out,enabled) for e in result['plan']]
                for entry,future in zip(result['plan'],futures):
                    new=future.result();(result['runs'] if enabled else result['retention'])[entry['name']]=new
                    save();assert 'error' not in new,(entry['name'],new.get('error'))
                    print(entry['name']+(': handoff audited' if enabled else ': retained exactly'),flush=True)
        I.H.verify_inputs(prior)
        assert F.E.digest(args.prior)==result['prior_summary']['sha256'] and F.E.digest(binary)==result['binary_sha256']
        result['complete']=True
    except BaseException as error:
        result['error']=repr(error);raise
    finally:save()


if __name__=='__main__':main()
