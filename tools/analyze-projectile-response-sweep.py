#!/usr/bin/env python3
"""Archive matched response coverage and outcomes without pooling cohorts."""
import argparse
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

spec=importlib.util.spec_from_file_location('analysis',Path(__file__).with_name('analyze-projectile-response.py'))
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)
R=A.R


def metrics(record,seat):
    return dict(wins=int(record['round']['outcome']==dict(winner=f'player_{seat+1}')),
                claims=record['physical']['claimed'],departures=record['physical']['departed'],
                pilot_deaths=int(record['round']['pilots'][seat]['death_tick'] is not None))


def comparisons(cases,records):
    groups={};paired={}
    for case in cases:
        key=case['key'];seat=case['item']['seat'];group=case['group']
        before=records[key+'-observe'];baseline=metrics(before,seat)
        paired[key]=dict(group=group,seat=seat,baseline=baseline,modes={})
        for mode in ['observe','brake','left']:
            r=records[key+'-'+mode];m=metrics(r,seat)
            values=groups.setdefault(group,{}).setdefault(mode,dict(cases=0,wins=0,claims=0,departures=0,
                pilot_deaths=0,eligible_cases=0,triggered_cases=0,changed_cases=0))
            values['cases']+=1
            for field,value in m.items():values[field]+=value
            values['eligible_cases']+=int(r['coverage']['ready_ticks']>0)
            values['triggered_cases']+=int(r['attempt'] is not None)
            values['changed_cases']+=int(r['first_action_change'] is not None)
            if mode!='observe':
                paired[key]['modes'][mode]=dict(outcome=m,delta={f:m[f]-baseline[f] for f in m},
                    triggered=r['attempt'] is not None,changed=r['first_action_change'] is not None,
                    full_parity=r['parity'] is not None)
    return groups,paired


def response_source(summaries):
    matches=[s for s in summaries if s['plan'] and all('mode' in e for e in s['plan'])]
    assert len(matches)==1,'one prior response experiment is required'
    return matches[0]


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary',type=Path,required=True);parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();s=json.loads(args.summary.read_text());assert s['complete']
    assert R.digest(s['binary']['path'])==s['binary']['sha256']
    assert all(R.digest(p)==h for p,h in s['sources'].items())
    for path,field in [('tools/validate-projectile-response-sweep.py','runner_sha256'),
                       ('tools/validate-projectile-response.py','response_auditor_sha256')]:
        frozen=subprocess.check_output(['git','show',s['source_commit']+':'+path])
        assert hashlib.sha256(frozen).hexdigest()==s[field]
    archive=args.out.with_suffix(args.out.suffix+'.gz');assert not args.out.exists() and not archive.exists()
    records={};evidence={};raw={}
    for entry in s['plan']:
        run=s['runs'][entry['key']];assert 'error' not in run
        for name,h in run['hashes'].items():
            path=R.root_of(run)/name;assert R.digest(path)==h;raw[str(path)]=h
        r,e=A.analyze(entry,run)
        r.update(group=entry['group'],seat=run['item']['seat'],coverage=run.get('coverage'))
        records[entry['key']]=r;evidence[entry['key']]=e
    groups,paired=comparisons(s['cases'],records)
    earlier=response_source([json.loads(Path(p).read_text()) for p in s['sources']]);development_parity={}
    for case in s['cases']:
        for mode in ['observe','brake','left']:
            key=case['key']+'-'+mode;old=earlier['runs'].get('speed-'+key)
            if old is None:continue
            run=s['runs'][key]
            for name in ['capture-evidence.jsonl','projectiles.jsonl']:
                assert run['hashes'][name]==old['hashes'][name]
            assert run['round']==old['round'] and run['visits']==old['visits'] and run['allocation']==old['allocation']
            development_parity[key]=dict(source='speed-'+key,
                exact_streams=['capture-evidence.jsonl','projectiles.jsonl'],exact_round_visits_allocation=True)
    assert len(development_parity)==6
    payload=dict(schema=1,runner_summary=s,runs=evidence)
    with archive.open('xb') as stream:
        stream.write(gzip.compress((json.dumps(payload,sort_keys=True,allow_nan=False)+'\n').encode(),mtime=0))
    manifest=dict(schema=1,complete=True,source_commit=s['source_commit'],binary=s['binary'],
        sources={str(args.summary):R.digest(args.summary),**s['sources']},
        tools={str(p):R.digest(p) for p in [Path(__file__),Path(A.__file__),Path(R.__file__),
                                            Path(__file__).with_name('validate-projectile-response-sweep.py')]},
        archive=dict(path=str(archive),sha256=R.digest(archive)),raw_files=raw,groups=groups,paired=paired,
        development_parity=development_parity,runs=records)
    with args.out.open('x') as stream:stream.write(json.dumps(manifest,indent=2,allow_nan=False)+'\n')
    print(f'Archived {len(records)} trials and {len(raw)} raw file hashes: {args.out}')


if __name__=='__main__':main()
