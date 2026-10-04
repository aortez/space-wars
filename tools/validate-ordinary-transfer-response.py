#!/usr/bin/env python3
"""Frozen extension of the unchanged response pulses to native Transfer states."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
from pathlib import Path
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('sweep',Path(__file__).with_name('validate-projectile-response-sweep.py'))
S=importlib.util.module_from_spec(spec);spec.loader.exec_module(S)
R=S.R


def command(old,binary,root,mode,scope,seat):
    cmd=list(old['command'])
    for option in ['--probe-projectile-response','--probe-projectile-response-seat','--probe-projectile-response-scope']:
        if option in cmd:
            i=cmd.index(option);del cmd[i:i+2]
    cmd[0]=str(binary);cmd[cmd.index('--out')+1]=str(root)
    cmd+=['--probe-projectile-response',mode,'--probe-projectile-response-seat',str(seat)]
    if scope!='escape':cmd+=['--probe-projectile-response-scope',scope]
    return cmd


def run(entry,case,old,binary,out):
    root=out/entry['key'];seat=case['item']['seat']
    cmd=command(old,binary,root,entry['mode'],entry['scope'],seat)
    result=dict(item=case['item'],command=cmd)
    try:
        with (out/(entry['key']+'.log')).open('x') as log:
            subprocess.run(cmd,check=True,stdout=log,stderr=log,timeout=1800)
        result.update(R.S.C.analyze(root,case['item']))
        report=json.loads((root/'report.json').read_text())
        result['projectiles']=R.P.audit_stream(root,report)
        result['response']=R.audit_response(root,R.root_of(old),entry['mode'],seat,report)
        result['coverage']=S.coverage(root,seat)
        if entry['mode']=='observe' or result['response']['first_action_change'] is None:
            result['parity']=R.S.C.P.replay_parity(old,result)
            assert R.digest(root/'projectiles.jsonl')==old['hashes']['projectiles.jsonl']
        if entry['scope']=='escape':
            assert R.digest(root/'projectile-response.jsonl')==old['hashes']['projectile-response.jsonl']
        result['hashes']={p.name:R.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    except Exception:result['error']=traceback.format_exc()
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['prior','survey','binary','out']:parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args();assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(),'freeze first'
    prior=json.loads(args.prior.read_text());survey=json.loads(args.survey.read_text())
    assert prior['complete'] and survey['complete'] and R.digest(args.prior)==survey['source']['sha256']
    cases=prior['cases'];assert len(cases)==15
    for c in cases:
        old=prior['runs'][c['key']+'-observe']
        for name,h in old['hashes'].items():assert R.digest(R.root_of(old)/name)==h
    plan=[dict(key=c['key']+'-'+mode,source=c['key'],mode=mode,scope='transfer',group=c['group'])
          for mode in ['observe','brake','left'] for c in cases]
    legacy=['shared-armed-world1-p1-powered','shared-armed-world0-p2-powered']
    plan=[dict(key='legacy-'+key,source=key,mode='observe',scope='escape',group='legacy') for key in legacy]+plan
    assert len(plan)==47
    binary=args.binary.resolve(strict=True);args.out.mkdir(parents=True,exist_ok=False)
    result=dict(schema=1,complete=False,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary=dict(path=str(binary),sha256=R.digest(binary)),sources={str(p):R.digest(p) for p in [args.prior,args.survey]},
        runner_sha256=R.digest(__file__),response_auditor_sha256=R.digest(R.__file__),cases=cases,plan=plan,runs={})
    save=lambda:R.S.F.D.write(args.out/'summary.json',result);save();lookup={c['key']:c for c in cases}
    try:
        for phase in ['legacy','observe','brake','left']:
            entries=[e for e in plan if (e['scope']=='escape' if phase=='legacy' else e['scope']=='transfer' and e['mode']==phase)]
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures=[]
                for e in entries:
                    old=prior['runs'][e['source']+'-observe'] if e['mode']=='observe' else result['runs'][e['source']+'-observe']
                    futures.append(pool.submit(run,e,lookup[e['source']],old,binary,args.out))
                for e,f in zip(entries,futures):
                    result['runs'][e['key']]=f.result();save()
                    print(e['key']+(': FAILED' if 'error' in result['runs'][e['key']] else ': audited'),flush=True)
            assert all('error' not in r for r in result['runs'].values()),'failures preserved'
        assert R.digest(binary)==result['binary']['sha256'] and all(R.digest(p)==h for p,h in result['sources'].items())
        result['complete']=True
    except BaseException as error:result['error']=repr(error);raise
    finally:save()


if __name__=='__main__':main()
