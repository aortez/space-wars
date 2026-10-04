#!/usr/bin/env python3
"""Archive the ordinary-transfer extension against its survey and full controls."""
import argparse
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess

spec=importlib.util.spec_from_file_location('sweep_analysis',Path(__file__).with_name('analyze-projectile-response-sweep.py'))
A=importlib.util.module_from_spec(spec);spec.loader.exec_module(A)
R=A.R


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--summary',type=Path,required=True);parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();s=json.loads(args.summary.read_text());assert s['complete']
    assert R.digest(s['binary']['path'])==s['binary']['sha256']
    assert all(R.digest(p)==h for p,h in s['sources'].items())
    for path,field in [('tools/validate-ordinary-transfer-response.py','runner_sha256'),
                       ('tools/validate-projectile-response.py','response_auditor_sha256')]:
        frozen=subprocess.check_output(['git','show',s['source_commit']+':'+path])
        assert hashlib.sha256(frozen).hexdigest()==s[field]
    sources=[json.loads(Path(p).read_text()) for p in s['sources']]
    survey=next(v for v in sources if 'surveyor_sha256' in v)
    prior=next(v for v in sources if 'cases' in v)
    archive=args.out.with_suffix(args.out.suffix+'.gz');assert not args.out.exists() and not archive.exists()
    records={};evidence={};raw={}
    for e in s['plan']:
        run=s['runs'][e['key']];assert 'error' not in run
        for name,h in run['hashes'].items():
            path=R.root_of(run)/name;assert R.digest(path)==h;raw[str(path)]=h
        r,w=A.A.analyze(e,run)
        r.update(group=e['group'],scope=e['scope'],seat=run['item']['seat'],coverage=run['coverage'])
        records[e['key']]=r;evidence[e['key']]=w
    groups,paired=A.comparisons(s['cases'],records)
    survey_comparison={};legacy_parity={};retained_response_parity={}
    for c in s['cases']:
        key=c['key'];measured=survey['runs'][key];live=records[key+'-observe']
        old=measured['first_warning'];new=live['attempt']
        assert (old is None)==(new is None)
        if old:
            assert old['tick']==new['started_tick'] and old['threat']['id']==new['threat']['id']
            assert old['threat']['spawn_tick']==new['threat']['spawn_tick']
            assert abs(old['threat']['entry_seconds']-new['threat']['entry_seconds'])<1.e-7
        assert measured['eligible_ticks']==live['coverage']['ready_ticks']
        survey_comparison[key]=dict(exact_eligibility_count=True,exact_first_warning=True,
                                    eligible_ticks=measured['eligible_ticks'])
        # Keep the previously active post-escape trajectory recognizable.
        if old and old['post_escape']:
            for mode in ['observe','brake','left']:
                name=key+'-'+mode;run=s['runs'][name];ref=prior['runs'][name]
                for f in ['capture-evidence.jsonl','projectiles.jsonl']:assert run['hashes'][f]==ref['hashes'][f]
                assert run['round']==ref['round'] and run['visits']==ref['visits'] and run['allocation']==ref['allocation']
                retained_response_parity[name]=dict(exact_native_projectile_round_visits_allocation=True)
    for e in s['plan']:
        if e['scope']=='escape':legacy_parity[e['key']]=s['runs'][e['key']]['parity']
    payload=dict(schema=1,runner_summary=s,runs=evidence)
    with archive.open('xb') as stream:
        stream.write(gzip.compress((json.dumps(payload,sort_keys=True,allow_nan=False)+'\n').encode(),mtime=0))
    paths=[Path(__file__),Path(A.__file__),Path(A.A.__file__),Path(R.__file__),Path(__file__).with_name('validate-ordinary-transfer-response.py')]
    manifest=dict(schema=1,complete=True,source_commit=s['source_commit'],binary=s['binary'],
        sources={str(args.summary):R.digest(args.summary),**s['sources']},tools={str(p):R.digest(p) for p in paths},
        archive=dict(path=str(archive),sha256=R.digest(archive)),raw_files=raw,groups=groups,paired=paired,
        survey_comparison=survey_comparison,legacy_parity=legacy_parity,retained_response_parity=retained_response_parity,runs=records)
    with args.out.open('x') as stream:stream.write(json.dumps(manifest,indent=2,allow_nan=False)+'\n')
    print(f'Archived {len(records)} trials and {len(raw)} raw file hashes: {args.out}')


if __name__=='__main__':main()
