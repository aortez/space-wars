#!/usr/bin/env python3
"""Archive the frozen selector comparison and its full physical follow-through."""
import argparse
from collections import Counter
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

spec=importlib.util.spec_from_file_location('sweep_analysis',Path(__file__).with_name('analyze-projectile-response-sweep.py'))
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)
A,R=S.A,S.R


def metrics(record,seat):
    return dict(S.metrics(record,seat),ships_lost=record['final_recovery']['ships_lost'],
                rebuilds=record['final_recovery']['rebuilds'])


def compare(cases,records,references):
    groups={};paired={}
    for c in cases:
        key=c['key'];seat=c['item']['seat'];r=records[key+'-guarded_brake'];before=references[key]
        group=groups.setdefault(c['group'],dict(cases=0,control=Counter(),selector=Counter(),
            warning_cases=0,brake_cases=0,abstained_cases=0,no_warning_cases=0,full_native_parity=0,
            per_seat={},reasons=Counter()))
        group['cases']+=1
        base=metrics(before,seat);after=metrics(r,seat)
        group['control'].update(base);group['selector'].update(after)
        choice=r['selection'];warned=r['attempt'] is not None;braking=(choice or {}).get('action')=='brake'
        group['warning_cases']+=int(warned);group['brake_cases']+=int(braking)
        group['abstained_cases']+=int(warned and not braking);group['no_warning_cases']+=int(not warned)
        group['full_native_parity']+=int(r['parity'] is not None)
        by_seat=group['per_seat'].setdefault(str(seat),dict(cases=0,warning_cases=0,brake_cases=0))
        by_seat['cases']+=1;by_seat['warning_cases']+=int(warned);by_seat['brake_cases']+=int(braking)
        group['reasons'][(choice or {}).get('reason','no warning')]+=1
        paired[key]=dict(group=c['group'],seat=seat,seed=c['item']['seed'],scope=c['scope'],
                         control=base,selector=after,delta={k:after[k]-base[k] for k in base},
                         selection=choice,first_warning_tick=r['attempt']['started_tick'] if warned else None,
                         full_native_parity=r['parity'] is not None)
    return groups,paired


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary',type=Path,required=True);parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();s=json.loads(args.summary.read_text());assert s['complete'] and len(s['runs'])==52
    assert R.digest(s['binary']['path'])==s['binary']['sha256']
    assert all(R.digest(p)==h for p,h in s['sources'].items())
    tool_commit=s.get('audit_correction',{}).get('source_commit',s['source_commit'])
    for p,h in s['tools'].items():
        relative=Path(p).resolve().relative_to(Path.cwd())
        frozen=subprocess.check_output(['git','show',tool_commit+':'+str(relative)])
        assert hashlib.sha256(frozen).hexdigest()==h and R.digest(p)==h
    if s.get('audit_correction'):
        original=s['audit_correction']['original_summary'];assert R.digest(original['path'])==original['sha256']
    archive=args.out.with_suffix(args.out.suffix+'.gz');assert not args.out.exists() and not archive.exists()
    records={};evidence={};raw={}
    for e in s['plan']:
        run=s['runs'][e['key']];assert 'error' not in run
        for name,h in run['hashes'].items():
            path=R.root_of(run)/name;assert R.digest(path)==h;raw[str(path)]=h
        r,w=A.analyze(e,run)
        r.update(group=e['group'],seat=run['item']['seat'],coverage=run.get('coverage'),
                 selection=run.get('response',{}).get('selection'),
                 legacy_parity=run.get('legacy_parity'),retained_brake_parity=run.get('retained_brake_parity'))
        records[e['key']]=r;evidence[e['key']]=w
    historical={};sources={}
    for key,name in [('ordinary','ordinary-transfer-response'),('original','projectile-response')]:
        path=Path('docs/data')/(name+'-v1.json');m=json.loads(path.read_text())
        summary_path=f'target/{name}/v1/summary.json'
        assert m['complete'] and m['sources'][summary_path]==s['sources'][summary_path]
        historical[key]=m;sources[str(path)]=R.digest(path)
    references={}
    for c in s['cases']:
        ref=records[c['key']+'-none'] if c.get('new') else historical[c['reference_file']]['runs'][c['reference_key']]
        references[c['key']]={k:ref[k] for k in ['physical','round','elapsed_ticks','final_recovery']}
    groups,paired=compare(s['cases'],records,references)
    payload=dict(schema=1,runner_summary=s,runs=evidence)
    with archive.open('xb') as f:f.write(gzip.compress((json.dumps(payload,sort_keys=True,allow_nan=False)+'\n').encode(),mtime=0))
    paths=[Path(__file__),Path(S.__file__),Path(A.__file__),Path(R.__file__),Path(R.B.__file__)]
    manifest=dict(schema=1,complete=True,source_commit=s['source_commit'],binary=s['binary'],
        sources={str(args.summary):R.digest(args.summary),**s['sources'],**sources},
        tools={str(p):R.digest(p) for p in paths},archive=dict(path=str(archive),sha256=R.digest(archive)),
        raw_files=raw,groups=groups,paired=paired,references=references,runs=records,
        audit_correction=s.get('audit_correction'))
    with args.out.open('x') as f:f.write(json.dumps(manifest,indent=2,allow_nan=False)+'\n')
    print(f'Archived {len(records)} matches and {len(raw)} raw-file hashes: {args.out}')


if __name__=='__main__':main()
