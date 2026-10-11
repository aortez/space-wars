#!/usr/bin/env python3
"""Frozen broader matched comparison of the existing brake and left pulses."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('response',Path(__file__).with_name('validate-projectile-response.py'))
R=importlib.util.module_from_spec(spec);spec.loader.exec_module(R)


def cases(prior):
    result=[]
    for e in prior['plan']:
        if e['item']['kind']=='armed':
            assert e['enabled']
            result.append(dict(key=e['name'],group='health' if e['source']=='health' else 'retained',
                               template=e['name'],item=copy.deepcopy(e['item'])))
    assert len(result)==11
    for world in (2,3):
        namespace=f'powered-mission-integration-v1:{world}'
        seed=int.from_bytes(hashlib.sha256(namespace.encode()).digest()[:8],'little')
        for seat in (0,1):
            template=f'shared-armed-world0-p{seat+1}-powered'
            item=copy.deepcopy(prior['runs'][template]['item'])
            group=f'shared-armed-world{world}-p{seat+1}'
            item.update(group=group,name=group+'-powered',seed=seed,seed_namespace=namespace)
            result.append(dict(key=item['name'],group='fresh',template=template,item=item))
    return result


def command(case, template, binary, root, mode):
    cmd=R.P.command(template,binary,root,True)
    cmd[cmd.index('--seed')+1]=str(case['item']['seed'])
    assert '--probe-projectile-response-seat' not in cmd and '--probe-projectile-response' not in cmd
    if mode!='none':
        cmd+=['--probe-projectile-response',mode,'--probe-projectile-response-seat',str(case['item']['seat'])]
    return cmd


def coverage(root,seat):
    ready=sampled=projectiles=0;first=None
    for r in R.rows(root/'projectile-response.jsonl'):
        if 'final_tick' in r:continue
        assert r['seat']==seat
        if r['ready'] is not None:
            ready+=1
            if first is None:first=r['tick']
        if r['diagnostic'] is not None:
            sampled+=1;projectiles+=len(r['diagnostic']['projectiles'])
    return dict(ready_ticks=ready,first_ready_tick=first,sampled_ticks=sampled,projectile_samples=projectiles)


def run(case,mode,template,old,binary,out):
    key=case['key']+'-'+mode;root=out/key
    cmd=command(case,template,binary,root,mode)
    result=dict(item=case['item'],command=cmd,group=case['group'],case=case['key'],mode=mode)
    try:
        with (out/(key+'.log')).open('x') as log:
            subprocess.run(cmd,check=True,stdout=log,stderr=log,timeout=1800)
        result.update(R.S.C.analyze(root,case['item']))
        report=json.loads((root/'report.json').read_text())
        result['projectiles']=R.P.audit_stream(root,report)
        if mode=='none':
            assert old is None and not (root/'projectile-response.jsonl').exists()
        else:
            assert old is not None
            result['response']=R.audit_response(root,R.root_of(old),mode,case['item']['seat'],report)
            result['coverage']=coverage(root,case['item']['seat'])
            if mode=='observe' or result['response']['first_action_change'] is None:
                result['parity']=R.S.C.P.replay_parity(old,result)
                reports=[json.loads((p/'report.json').read_text()) for p in [R.root_of(old),root]]
                R.S.C.P.M.T.report_parity(*reports,['pursuit_health','initial_cover','actual_route_recovery',
                    'capture_escape','escape_travel','transfer_approach','transfer_speed'])
                if 'projectiles.jsonl' in old['hashes']:
                    assert R.digest(root/'projectiles.jsonl')==old['hashes']['projectiles.jsonl']
        result['hashes']={p.name:R.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    except Exception:result['error']=traceback.format_exc()
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prior',type=Path,required=True)
    parser.add_argument('--responses',type=Path,required=True)
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True);args=parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(),'freeze first'
    prior=json.loads(args.prior.read_text());responses=json.loads(args.responses.read_text())
    assert prior['complete'] and responses['complete']
    selected=cases(prior)
    for case in selected:
        old=prior['runs'][case['template']]
        for name,h in old['hashes'].items():assert R.digest(R.root_of(old)/name)==h
    # Retain the earlier short-warning regression and development outcomes as
    # immutable comparison evidence, not new independent observations.
    for old in responses['runs'].values():
        for name,h in old['hashes'].items():assert R.digest(R.root_of(old)/name)==h
    plan=[dict(key=c['key']+'-'+mode,source=c['key'],mode=mode,group=c['group'])
          for mode in ['none','observe','brake','left'] for c in selected if mode!='none' or c['group']=='fresh']
    assert len(plan)==49
    binary=args.binary.resolve(strict=True);args.out.mkdir(parents=True,exist_ok=False)
    result=dict(schema=1,complete=False,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary=dict(path=str(binary),sha256=R.digest(binary)),
        sources={str(p):R.digest(p) for p in [args.prior,args.responses]},
        runner_sha256=R.digest(__file__),response_auditor_sha256=R.digest(R.__file__),
        cases=selected,plan=plan,runs={})
    save=lambda:R.S.F.D.write(args.out/'summary.json',result);save()
    try:
        for mode in ['none','observe','brake','left']:
            jobs=[c for c in selected if mode!='none' or c['group']=='fresh']
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures=[]
                for c in jobs:
                    template=prior['runs'][c['template']]
                    old=(None if mode=='none' else
                         (result['runs'][c['key']+'-none'] if c['group']=='fresh' else template)
                         if mode=='observe' else result['runs'][c['key']+'-observe'])
                    futures.append(pool.submit(run,c,mode,template,old,binary,args.out))
                for c,f in zip(jobs,futures):
                    key=c['key']+'-'+mode;result['runs'][key]=f.result();save()
                    print(key+(': FAILED' if 'error' in result['runs'][key] else ': audited'),flush=True)
            assert all('error' not in r for r in result['runs'].values()),'audit failures preserved in summary'
        assert all(R.digest(p)==h for p,h in result['sources'].items())
        assert R.digest(binary)==result['binary']['sha256']
        result['complete']=True
    except BaseException as error:result['error']=repr(error);raise
    finally:save()


if __name__=='__main__':main()
