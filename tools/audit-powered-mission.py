#!/usr/bin/env python3
"""Re-audit immutable powered mission trials without rerunning the simulation."""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess

spec=importlib.util.spec_from_file_location('mission',Path(__file__).with_name('validate-powered-mission.py'))
M=importlib.util.module_from_spec(spec)
spec.loader.exec_module(M)
F=M.F


def verify_files(manifest):
    root=Path(manifest['root'])
    actual={str(p.relative_to(root)) for p in root.rglob('*') if p.is_file()}
    assert actual==manifest['files'].keys(), 'source file set changed'
    for name,entry in manifest['files'].items():
        path=root/name
        assert path.stat().st_size==entry['bytes'] and F.E.digest(path)==entry['sha256'], name
    return root


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest',type=Path,required=True)
    parser.add_argument('--previous-audit',type=Path)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze audit correction first'
    manifest=json.loads(args.manifest.read_text())
    root=verify_files(manifest)
    prior=json.loads((root/'summary.json').read_text())
    assert not prior['complete'] and prior['plan']==M.plan()
    assert {k:v['error'] for k,v in prior['runs'].items() if 'error' in v}=={
        'native-armed-world1-p2-powered':'AssertionError()'}
    assert len(prior['regressions'])==3 and len(prior['runs'])==28 and len(prior['comparisons'])==14
    binary=Path(next(iter(prior['runs'].values()))['command'][0])
    assert F.E.digest(binary)==prior['binary_sha256']
    args.out.mkdir(parents=True,exist_ok=False)
    result=dict(schema=1,complete=False,source_commit=prior['source_commit'],
        audit_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=prior['binary_sha256'],input_manifest=dict(path=str(args.manifest),sha256=F.E.digest(args.manifest)),
        rejected_summary=dict(path=str(root/'summary.json'),sha256=F.E.digest(root/'summary.json')),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [M,M.E,M.J,M.T,M.V,M.C,F,F.M,F.E,F.D]},
        runner_sha256=F.E.digest(Path(__file__)),plan=prior['plan'],regressions=prior['regressions'],runs={},comparisons={})
    if args.previous_audit:
        previous=json.loads(args.previous_audit.read_text())
        assert not previous['complete'] and previous['input_manifest']==result['input_manifest']
        result['previous_rejected_audit']=dict(path=str(args.previous_audit),sha256=F.E.digest(args.previous_audit))
    save=lambda:F.D.write(args.out/'summary.json',result)
    save()
    try:
        for item in result['plan']:
            name=item['name'];old=prior['runs'][name];command=old['command']
            game=Path(command[command.index('--out')+1]);audit_out=args.out/name
            assert game.resolve()==root/name
            for filename,digest in old.get('hashes',{}).items(): assert F.E.digest(game/filename)==digest
            audit_out.mkdir()
            run=dict(item=item,command=command)
            try: run.update(M.analyze(game,item,audit_out))
            except Exception as error: run['error']=repr(error)
            result['runs'][name]=run;save()
            print(name+(': '+run['error'] if 'error' in run else ': audited'),flush=True)
        for item in result['plan']:
            if item['powered']:
                group=item['group']
                result['comparisons'][group]=M.compare(root/(group+'-walking'),root/(group+'-powered'))
        assert result['comparisons']==prior['comparisons'], 'paired outcomes changed'
        verify_files(manifest)
        assert F.E.digest(binary)==prior['binary_sha256']
        assert not any('error' in r for r in result['runs'].values()), 'one or more run audits failed'
        result['complete']=True
    except BaseException as error:
        result['error']=repr(error);raise
    finally: save()


if __name__=='__main__': main()
