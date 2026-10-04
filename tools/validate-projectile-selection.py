#!/usr/bin/env python3
"""Frozen retained and independent full-match test of one conservative selector."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess
import traceback

spec=importlib.util.spec_from_file_location('ordinary',Path(__file__).with_name('validate-ordinary-transfer-response.py'))
O=importlib.util.module_from_spec(spec);spec.loader.exec_module(O)
R=O.R


def archived_equal(current,saved):
    return json.loads(json.dumps(current,allow_nan=False))==saved


def cases(ordinary,original):
    result=[dict(key=c['key'],group='historical_'+c['group'],item=copy.deepcopy(c['item']),scope='transfer',
                 reference_file='ordinary',reference_key=c['key']+'-observe') for c in ordinary['cases']]
    for health in ['', 'health-']:
        key='approach-'+health+'shared-armed-world1-p1-powered';r=original['runs'][key+'-observe']
        result.append(dict(key=key,group='historical_clear_entry',item=copy.deepcopy(r['item']),scope='escape',
                           reference_file='original',reference_key=key+'-observe'))
    for world in range(4):
        namespace=f'projectile-selection-v1:{world}'
        seed=int.from_bytes(hashlib.sha256(namespace.encode()).digest()[:8],'little')
        for seat in (0,1):
            for asteroids in (0,3):
                template=f'shared-armed-world0-p{seat+1}-powered-observe'
                item=copy.deepcopy(ordinary['runs'][template]['item'])
                group='heldout_asteroids' if asteroids else 'heldout_quiet'
                key=f'new-world{world}-p{seat+1}-'+('asteroids' if asteroids else 'quiet')
                item.update(group=key,name=key,seed=seed,seed_namespace=namespace,asteroid_interval=asteroids)
                result.append(dict(key=key,group=group,item=item,scope='transfer',reference_file='ordinary',
                                   reference_key=template,new=True))
    assert len(result)==33
    return result


def command(case,old,binary,root,mode):
    cmd=O.command(old,binary,root,mode,case['scope'],case['item']['seat'])
    cmd[cmd.index('--seed')+1]=str(case['item']['seed'])
    if case.get('new'):cmd[cmd.index('--asteroid-interval')+1]=str(case['item']['asteroid_interval'])
    return cmd


def run(entry,case,template,old,binary,out):
    root=out/entry['key'];mode=entry['mode'];seat=case['item']['seat']
    cmd=command(case,template,binary,root,mode)
    r=dict(item=case['item'],command=cmd)
    try:
        with (out/(entry['key']+'.log')).open('x') as log:
            subprocess.run(cmd,check=True,stdout=log,stderr=log,timeout=1800)
        r.update(R.S.C.analyze(root,case['item']))
        report=json.loads((root/'report.json').read_text())
        r['projectiles']=R.P.audit_stream(root,report)
        if mode=='none':assert not (root/'projectile-response.jsonl').exists()
        else:
            r['response']=R.audit_response(root,R.root_of(old),mode,seat,report)
            r['coverage']=O.S.coverage(root,seat)
            if mode=='observe' or r['response']['first_action_change'] is None:
                r['parity']=R.S.C.P.replay_parity(old,r)
                assert R.digest(root/'projectiles.jsonl')==old['hashes']['projectiles.jsonl']
            if entry.get('legacy'):
                assert R.digest(root/'projectile-response.jsonl')==template['hashes']['projectile-response.jsonl']
                for f in ['capture-evidence.jsonl','projectiles.jsonl']:assert R.digest(root/f)==template['hashes'][f]
                assert all(archived_equal(r[f],template[f]) for f in ['round','visits','allocation'])
                r['legacy_parity']=True
        r['hashes']={p.name:R.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    except Exception:r['error']=traceback.format_exc()
    return r


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ['ordinary','original','binary','out']:parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--resume-audit-correction',action='store_true')
    args=parser.parse_args();assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(),'freeze first'
    inputs={k:json.loads(getattr(args,k).read_text()) for k in ['ordinary','original']}
    assert all(s['complete'] for s in inputs.values());selected=cases(**inputs)
    legacy=[dict(key='legacy-clear-brake',group='legacy',scope='escape',reference_file='original',
                 reference_key='approach-shared-armed-world1-p1-powered-brake',mode='brake'),
            dict(key='legacy-speed-left',group='legacy',scope='escape',reference_file='original',
                 reference_key='speed-shared-armed-world1-p1-powered-left',mode='left'),
            dict(key='legacy-p2-observe',group='legacy',scope='transfer',reference_file='ordinary',
                 reference_key='shared-armed-world0-p2-powered-observe',mode='observe')]
    for c in legacy:c['item']=inputs[c['reference_file']]['runs'][c['reference_key']]['item']
    checked=set()
    for c in selected+legacy:
        identity=(c['reference_file'],c['reference_key'])
        if identity in checked:continue
        checked.add(identity)
        old=inputs[c['reference_file']]['runs'][c['reference_key']]
        for n,h in old['hashes'].items():assert R.digest(R.root_of(old)/n)==h
    plan=[dict(key=c['key'],source=c['key'],mode=c['mode'],group='legacy',legacy=True) for c in legacy]
    plan += [dict(key=c['key']+'-none',source=c['key'],mode='none',group=c['group']) for c in selected if c.get('new')]
    plan += [dict(key=c['key']+'-guarded_brake',source=c['key'],mode='guarded_brake',group=c['group']) for c in selected]
    assert len(plan)==52
    binary=args.binary.resolve(strict=True)
    source_paths=[args.ordinary,args.original]
    tool_paths=[Path(__file__),Path(O.__file__),Path(R.__file__),Path(R.B.__file__)]
    s=dict(schema=1,complete=False,source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
           binary=dict(path=str(binary),sha256=R.digest(binary)),sources={str(p):R.digest(p) for p in source_paths},
           tools={str(p):R.digest(p) for p in tool_paths},cases=selected,legacy=legacy,plan=plan,runs={})
    if args.resume_audit_correction:
        current=s;s=json.loads((args.out/'summary.json').read_text())
        assert not s['complete'] and not s.get('audit_correction')
        assert all(s[f]==current[f] for f in ['binary','sources','cases','legacy','plan'])
        assert all(current['tools'][p]==h for p,h in s['tools'].items() if Path(p).resolve()!=Path(__file__).resolve())
        assert set(s['runs'])=={e['key'] for e in plan if e.get('legacy')},'only the legacy phase may have run'
        backup=args.out/'pre-audit-correction-summary.json';assert not backup.exists()
        backup.write_bytes((args.out/'summary.json').read_bytes())
        failures={}
        for c in legacy:
            r=s['runs'][c['key']];ref=inputs[c['reference_file']]['runs'][c['reference_key']]
            if 'error' not in r:continue
            assert "r['round']==template['round']" in r['error'] and r['error'].rstrip().endswith('AssertionError')
            assert all(archived_equal(r[f],ref[f]) for f in ['round','visits','allocation'])
            root=R.root_of(r)
            for f in ['projectile-response.jsonl','capture-evidence.jsonl','projectiles.jsonl']:
                assert R.digest(root/f)==ref['hashes'][f]
            failures[c['key']]=r.pop('error');r['legacy_parity']=True
            r['hashes']={p.name:R.digest(p) for p in sorted(root.iterdir()) if p.is_file()}
        s['audit_correction']=dict(source_commit=current['source_commit'],reason='normalize in-memory integer seat keys to archived JSON keys',
            original_summary=dict(path=str(backup),sha256=R.digest(backup)),failures=failures,original_tools=s['tools'])
        s['tools']=current['tools'];s.pop('error',None)
    else:args.out.mkdir(parents=True,exist_ok=False)
    save=lambda:R.S.F.D.write(args.out/'summary.json',s);save();lookup={c['key']:c for c in selected+legacy}
    try:
        for phase in ['legacy','none','guarded_brake']:
            entries=[e for e in plan if ('legacy' if e.get('legacy') else e['mode'])==phase and e['key'] not in s['runs']]
            with ThreadPoolExecutor(max_workers=2) as pool:
                futures=[]
                for e in entries:
                    c=lookup[e['source']];template=inputs[c['reference_file']]['runs'][c['reference_key']]
                    old=None if e['mode']=='none' else s['runs'][c['key']+'-none'] if c.get('new') else template
                    if e.get('legacy'):
                        old=inputs[c['reference_file']]['runs'][c['reference_key'].rsplit('-',1)[0]+'-observe']
                    futures.append(pool.submit(run,e,c,template,old,binary,args.out))
                for e,f in zip(entries,futures):
                    s['runs'][e['key']]=f.result();save()
                    print(e['key']+(': FAILED' if 'error' in s['runs'][e['key']] else ': audited'),flush=True)
            assert all('error' not in r for r in s['runs'].values()),'failures preserved'
        for c in selected:
            r=s['runs'][c['key']+'-guarded_brake']
            if not c.get('new') and (r['response']['selection'] or {}).get('action')=='brake':
                ref=inputs[c['reference_file']]['runs'][c['reference_key'].rsplit('-',1)[0]+'-brake']
                for f in ['capture-evidence.jsonl','projectiles.jsonl']:assert r['hashes'][f]==ref['hashes'][f]
                assert all(archived_equal(r[f],ref[f]) for f in ['round','visits','allocation'])
                r['retained_brake_parity']=True
        assert R.digest(binary)==s['binary']['sha256']
        assert all(R.digest(p)==h for p,h in {**s['sources'],**s['tools']}.items())
        s['complete']=True
    except BaseException as error:s['error']=repr(error);raise
    finally:save()


if __name__=='__main__':main()
