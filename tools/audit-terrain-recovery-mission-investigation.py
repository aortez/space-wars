#!/usr/bin/env python3
"""Verify retained recovery-only continuations and paired cost evidence."""
import argparse
from collections import Counter
import itertools
import json
from pathlib import Path
import runpy
import subprocess
import tarfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--output', type=Path, help='Write a separate receipt without replacing audit.json')
    parser.add_argument('--reference-root', type=Path, help='Directory containing CASE/actions.jsonl')
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    helpers = runpy.run_path(str(repo/'tools/audit-terrain-recovery-missions.py'))
    read, rows, digest, timing = (helpers[k] for k in ('read', 'rows', 'digest', 'timing'))
    root = args.root.resolve()
    runtime, inputs = read(root/'runtime.json'), read(root/'source-inputs.json')
    assert digest(root/'investigation-comparison') == runtime['binary_sha256']
    assert digest(root/'source-inputs.json') == runtime['source_inputs_sha256']
    checked = set()
    process = subprocess.Popen(['git', 'archive', runtime['source_commit']], cwd=repo, stdout=subprocess.PIPE)
    with tarfile.open(fileobj=process.stdout, mode='r|') as archive:
        for item in archive:
            if item.name in inputs:
                payload = item.linkname.encode() if item.issym() else archive.extractfile(item).read()
                assert helpers['hashlib'].sha256(payload).hexdigest() == inputs[item.name]
                checked.add(item.name)
    assert process.wait() == 0 and checked == set(inputs)
    results = read(root/'results.json')
    assert [r['name'] for r in results] == ['round-hatch', 'angular-footing', 'angular-replacement']
    cases = []
    for result in results:
        assert result['exit_code'] == 0, result
        assert result['binary_sha256'] == runtime['binary_sha256']
        reference_root = args.reference_root or root/'references'
        reference = reference_root/result['name']/'actions.jsonl'
        if not reference.exists() and args.reference_root is None:
            reference = Path(result['reference_actions'])
        assert digest(reference) == result['reference_sha256']
        path = root/'cases'/result['name']/'output'
        spec, fork = read(path/'spec.json'), read(path/'fork-verification.json')
        assert fork['prefix_ticks_verified'] == spec['fork_tick']
        initial = [read(path/arm/'initial.json') for arm in ('control', 'fresh')]
        assert initial[0]['scenario'] == initial[1]['scenario']
        assert initial[0]['match'] == initial[1]['match'] and initial[0]['audit'] == initial[1]['audit']
        arms = {}
        for arm in ('control', 'fresh'):
            p = path/arm
            s = read(p/'summary.json')
            assert read(p/'final.json')['brains'] == s['brains']
            assert s['start_tick'] == spec['fork_tick']
            assert s['end_tick'] == s['match']['finished_tick'] <= 36000
            assert s['reset_only_subject_recovery'] == (arm == 'fresh')
            native_ticks = [0, 0]
            for offset, row in enumerate(rows(p/'actions.jsonl')):
                assert row['tick'] == s['start_tick']+offset
                assert len(row['actions']) == 6
                assert row['native_refinement'] == [False, row['recovery_active'][1]]
                for seat in (0, 1):native_ticks[seat] += row['native_refinement'][seat]
            assert offset+1 == s['end_tick']-s['start_tick']
            assert native_ticks == [m['native_refinement_decisions'] for m in s['metrics']]
            samples = list(rows(p/'samples.jsonl'))
            expected = list(range((s['start_tick']//60+1)*60, s['end_tick']+1, 60))
            if s['end_tick']%60:expected.append(s['end_tick'])
            assert [r['tick'] for r in samples] == expected
            for sample in samples:
                a = sample['state']['audit']
                assert not a['issues'] and a['max_speed'] < 500
                assert a['occupied_cells']+a['loose_cells']+a['removed_cells'] == s['initial_cells']
            completions = []
            for seat in (0, 1):
                m = s['metrics'][seat]
                assert sum(m['mission_goals'].values()) == s['end_tick']-s['start_tick']
                assert len(m['completed']) == s['brains'][seat]['completed_recoveries']-s['initial_completed'][seat]
                for tick in m['completed']:
                    d = read(p/f'completion-{tick}-p{seat+1}-decision.json')
                    pilot = d['observations'][seat]['local']['combat']['recovery']['flight']['pilot']
                    assert pilot['ship_available'] and pilot['ship_form'] == 'ship'
                    assert pilot['location'] == {'aboard': seat} and d['brains'][seat]['recovery'] is None
                    completions.append({'tick': tick, 'seat': seat, 'pilot': pilot})
            feature_counts = Counter()
            for f in rows(p/'features.jsonl'):
                feature_counts[f['kind']] += 1
                if f['kind'] == 'survey':
                    for key, cap in (('coarse_candidates',8),('refined_candidates',8),('offset_checks',240),('staging_route_checks',16)):
                        assert 0 <= (f['survey'].get('refinement') or {}).get(key,0) <= cap
                if f['kind'] == 'pod_review' and f['review']:
                    r = f['review'];assert r['surveys'] <= 15
                    if r['started_tick'] is not None:
                        assert r['blocked_tick'] < r['started_tick'] <= r['blocked_tick']+900
                        assert r['deadline_tick'] == r['started_tick']+5400
            max_work = Counter()
            for offset, cost in enumerate(rows(p/'costs.jsonl')):
                assert cost['tick'] == s['start_tick']+offset
                for kind, cap in (('graph',4), ('physics_queries',384)):
                    charged = sum(v[kind] for v in cost['work'].values())
                    assert 0 <= charged <= cap
                    max_work[kind] = max(max_work[kind], charged)
            assert offset+1 == s['end_tick']-s['start_tick']
            arms[arm] = {'summary':s,'completions':completions,'features':dict(feature_counts),
                         'audit_samples':len(samples),'max_host_work':dict(max_work)}
        keys = ('actions','sites','decision_fnv64','scenario_before_step_fnv64')
        diffs, first = Counter(), dict.fromkeys(keys)
        lengths = [0,0]
        for a,b in itertools.zip_longest(rows(path/'control/actions.jsonl'),rows(path/'fresh/actions.jsonl')):
            lengths[0] += a is not None;lengths[1] += b is not None
            if a is None or b is None:continue
            assert a['tick'] == b['tick']
            for key in keys:
                if a[key] != b[key]:
                    diffs[key] += 1
                    if first[key] is None:first[key] = a['tick']
        reference_rows = itertools.islice(rows(reference),spec['fork_tick'],None)
        matched = 0
        for a,b in itertools.zip_longest(reference_rows,rows(path/'control/actions.jsonl')):
            assert a is not None and b is not None
            assert a['tick'] == b['tick'] and all(a[k] == b[k] for k in keys)
            matched += 1
        assert matched == arms['control']['summary']['reference_ticks_verified']
        inspections = {p.name:read(p) for p in sorted(path.glob('inspection-*.json'))}
        final_inspections = {arm:read(path/arm/f"inspection-{arms[arm]['summary']['end_tick']}.json")
                             for arm in arms}
        for data in itertools.chain(inspections.values(), final_inspections.values()):
            route = data['routes'];assert route.get('controller_input') is False
            assert route['posture']['physics_unchanged']
            coverage = data['coverage']
            assert coverage.get('unavailable') or coverage['physics_unchanged']
        receipt = {'name':result['name'],'spec':spec,'fork':fork,'arms':arms,'inspections':inspections,
                   'final_inspections':final_inspections,
                   'comparison':{'lengths':lengths,'different_ticks':dict(diffs),'first_difference':first}}
        cost_path = path/'cost-pairs.jsonl'
        if cost_path.exists():
            groups = {}
            physics_by_tick = {}
            for row in rows(cost_path):
                rep,tick = row['repeat'],row['tick']
                assert row['order'] == ([0,1] if rep%2 == 0 else [1,0])
                assert row['matching_physics_after_step']
                fingerprint = row['matching_physics_after_step']
                assert physics_by_tick.setdefault(tick, fingerprint) == fingerprint
                for index, sample in enumerate(row['samples']):
                    assert sample['refinement'] == bool(index)
                    v = groups.setdefault((rep,index),{'ticks':[],'ms':[],'inclusive':Counter(),'exclusive':Counter(),'calls':Counter(),'counters':Counter()})
                    v['ticks'].append(tick);v['ms'].append(sample['step_ms'])
                    for name, stage in sample['profile']['stages'].items():
                        v['inclusive'][name] += stage['inclusive_ms'];v['exclusive'][name] += stage['exclusive_ms'];v['calls'][name] += stage['calls']
                    v['counters'].update(sample['profile']['counters'])
            assert set(groups) == set(itertools.product(range(4),range(2)))
            reports=[]
            for (rep,index),v in sorted(groups.items()):
                assert v.pop('ticks') == list(range(spec['fork_tick'],spec['fork_tick']+600))
                assert v['calls']['native_recovery_update'] == 600
                assert v['calls']['native_rebuild_attempt'] == 20
                assert v['counters']['rebuild_placement_offset_checks'] == 20*(26 if index else 4)
                v['timing'] = timing(v.pop('ms'))
                reports.append({'repeat':rep,'refinement':bool(index),**v})
            receipt['cost_pairs']=reports
        cases.append(receipt)
        print(json.dumps({'case':result['name'],'first_difference':first,'completions':{arm:len(v['completions']) for arm,v in arms.items()}}),flush=True)
    files = {str(p.relative_to(root)):{'sha256':digest(p),'bytes':p.stat().st_size} for p in sorted((root/'cases').rglob('*')) if p.is_file()}
    report={'runtime':runtime,'source_files_verified':len(checked),'cases':cases,'artifact_receipts':files,
            'auditor_sha256':digest(Path(__file__)),'helper_sha256':digest(repo/'tools/audit-terrain-recovery-missions.py')}
    output = args.output or root/'audit.json'
    output.write_text(json.dumps(report,indent=2)+'\n')
    print(f'Verified {len(cases)} retained cases, {len(files)} artifacts, and {len(checked)} source inputs.')


if __name__ == '__main__':main()
