#!/usr/bin/env python3
"""Complete the frozen cover-response audit using a separately bound activation replay."""
import argparse
import copy
import importlib.util
import json
from pathlib import Path
import subprocess

spec=importlib.util.spec_from_file_location('response',Path(__file__).with_name('validate-cover-response.py'))
Q=importlib.util.module_from_spec(spec);spec.loader.exec_module(Q)


def trace_history(path):
    for row in Q.F.rows(path):
        yield row['seat'], row['mission']


def replay_arguments(command):
    result=list(command)
    for flag in ['--out','--trace','--trace-start-tick','--trace-end-tick']:
        while flag in result:
            i=result.index(flag)
            del result[i:i+2]
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--study',type=Path,required=True)
    parser.add_argument('--replay',type=Path,required=True)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze the audit correction first'
    source=args.study/'summary.json';original=json.loads(source.read_text())
    assert not original['complete'] and original['error']=="AssertionError('behavior changed without cover response')"
    assert len(original['runs'])==64 and sum(len(a) for a in original['regressions'].values())==10
    result=copy.deepcopy(original)
    result.update(complete=False,comparisons=[],audit_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        audit_runner_sha256=Q.F.E.digest(Path(__file__)),response_analysis_sha256=Q.F.E.digest(Path(Q.__file__)),
        rejected_validation_attempt=dict(path=str(source),sha256=Q.F.E.digest(source),error=result.pop('error')),
        activation_replay={},report_scope='Original frozen 74 gameplay runs. Two supplementary unchanged-binary replays resolve an activation omitted from sparse report snapshots; no settings or outcomes were fitted. Per-capture counters remain lower bounds from the retained evidence.')
    args.out.mkdir(parents=True,exist_ok=False)
    save=lambda:Q.F.D.write(args.out/'summary.json',result)
    save()
    try:
        runs=list(result['runs'].values())+[r for a in result['regressions'].values() for r in a.values()]
        for r in runs:
            root=Path(r['command'][r['command'].index('--out')+1])
            for filename,digest in r['hashes'].items():
                assert Q.F.E.digest(root/filename)==digest,(root,filename)
            report=json.loads((root/'report.json').read_text())
            enabled=[i for i,c in enumerate(report['policy_configuration']) if c.get('cover_response_model')]
            prior=Q.response_stats(report,enabled)
            assert {k:v for k,v in prior.items() if k!='scope'}=={k:v for k,v in r['cover_response'].items() if k!='scope'}
            r['cover_response']=prior
        plan_path=args.replay/'plan.json';plan=json.loads(plan_path.read_text())
        assert plan['binary_sha256']==result['binary_sha256'] and plan['source_commit']==result['source_commit']
        bindings=[]
        for arm,source_run in plan['runs'].items():
            command=source_run['command'];root=Path(command[command.index('--out')+1]);old=Path(source_run['source_root'])
            matches=[(name,r) for name,r in result['runs'].items() if Path(r['command'][r['command'].index('--out')+1])==old]
            assert len(matches)==1
            name,r=matches[0]
            assert replay_arguments(command)==replay_arguments(r['command'])
            assert Q.F.E.digest(old/'report.json')==source_run['source_report_sha256']==r['hashes']['report.json']
            assert Q.F.E.digest(Path(command[0]))==result['binary_sha256']
            before,after=[json.loads((p/'report.json').read_text()) for p in [old,root]]
            for field in Q.V.EXACT_REPORT_FIELDS+['metrics','policy_configuration']:
                assert before[field]==after[field],(name,field)
            for stream in Q.V.EXACT_STREAMS:
                if stream!='trace.jsonl': assert Q.F.E.digest(root/stream)==Q.F.E.digest(old/stream),(name,stream)
            sensor=Q.A.C.audit_sensors(old,root)
            enabled=[i for i,c in enumerate(after['policy_configuration']) if c.get('cover_response_model')]
            stats=Q.response_stats(after,enabled,trace_history(root/'trace.jsonl'))
            result['activation_replay'][arm]=dict(name=name,command=command,source_root=str(old),
                report_parity=True,sensor_parity=sensor,allocation=Q.F.allocation_audit(root),
                hashes={p.name:Q.F.E.digest(p) for p in sorted(root.iterdir()) if p.is_file()},
                report_only_response=r['cover_response'],trace_augmented_response=stats)
            r['cover_response']=stats
            bindings.append(name)
        assert len(bindings)==2 and {n.rsplit('-',1)[0] for n in bindings}=={'world2-asteroids3-p1'}
        a,b=[result['activation_replay'][arm] for arm in ['predecessor','candidate']]
        roots=[Path(r['command'][r['command'].index('--out')+1]) for r in [a,b]]
        effect=b['trace_augmented_response']['first_effect_tick']
        assert effect is not None
        result['activation_replay']['prefix_parity']=Q.prefix_parity(roots[0]/'trace.jsonl',roots[1]/'trace.jsonl',effect)
        result['activation_replay']['plan_sha256']=Q.F.E.digest(plan_path)
        for item in result['plan']:
            if not item['candidate']: continue
            names=[item['group']+'-'+arm for arm in ['predecessor','candidate']]
            reports=[]
            for name in names:
                r=result['runs'][name];root=Path(r['command'][r['command'].index('--out')+1])
                reports.append(json.loads((root/'report.json').read_text()))
            a,b=reports
            assert a['initial_world']==b['initial_world']
            same=Q.F.D.same_physical_outcomes(a,b)
            assert same or result['runs'][names[1]]['cover_response']['affected_captures'],'unexplained physical difference'
            result['comparisons'].append(dict(group=item['group'],kind=item['kind'],seat=item['seat'],
                matching_recorded_physical_outcomes=same,predecessor_outcome=Q.F.D.outcome(a,item['seat']),
                candidate_outcome=Q.F.D.outcome(b,item['seat'])))
        result['complete']=True
        save()
        print(json.dumps(dict(complete=True,comparisons=len(result['comparisons']),prefix=result['activation_replay']['prefix_parity'])))
    except Exception as error:
        result['error']=repr(error);save();raise


if __name__=='__main__': main()
