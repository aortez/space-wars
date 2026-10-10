#!/usr/bin/env python3
"""Audit the two frozen standing-ground rejoin continuations without simulating."""
import argparse
from collections import Counter
import itertools
import json
import math
from pathlib import Path
import runpy
import struct
import subprocess
import tarfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    root, repo = args.root.resolve(), Path(__file__).resolve().parent.parent
    helper = runpy.run_path(str(repo/'tools/audit-terrain-recovery-missions.py'))
    read, rows, digest = (helper[k] for k in ('read','rows','digest'))
    runtime, inputs = read(root/'runtime.json'), read(root/'source-inputs.json')
    assert digest(root/'standing-rejoin-comparison') == runtime['binary_sha256']
    assert digest(root/'source-inputs.json') == runtime['source_inputs_sha256']
    process = subprocess.Popen(['git','archive',runtime['source_commit']],cwd=repo,stdout=subprocess.PIPE)
    checked = set()
    with tarfile.open(fileobj=process.stdout,mode='r|') as archive:
        for item in archive:
            if item.name in inputs:
                data = item.linkname.encode() if item.issym() else archive.extractfile(item).read()
                assert helper['hashlib'].sha256(data).hexdigest() == inputs[item.name]
                checked.add(item.name)
    assert process.wait() == 0 and checked == set(inputs)
    reference = root/'references/angular-footing/actions.jsonl'
    reference_sha = digest(reference)
    results = read(root/'results.json')
    assert [r['fork_tick'] for r in results] == [8025,8325]
    cases = []
    for result in results:
        fork = result['fork_tick']
        assert result['exit_code'] == 0 and result['reference_sha256'] == reference_sha
        assert result['binary_sha256'] == runtime['binary_sha256']
        path = root/'cases'/str(fork)/'output'
        verification = read(path/'fork-verification.json')
        assert verification['prefix_ticks_verified'] == fork and not verification['controller_reset']
        initial = [read(path/arm/'initial.json') for arm in ('control','candidate')]
        assert initial[0] == initial[1] == read(path/'fork.json')
        control_memory = (path/'control/initial-controllers.txt').read_text()
        candidate_memory = (path/'candidate/initial-controllers.txt').read_text()
        flag = 'rebuild_ground_rejoin_enabled: true'
        assert candidate_memory.count(flag) == 1
        assert candidate_memory.replace(flag,'rebuild_ground_rejoin_enabled: false') == control_memory
        arms = {}
        for arm in ('control','candidate'):
            p = path/arm
            s = read(p/'summary.json')
            assert s['start_tick'] == fork and s['end_tick'] == s['match']['finished_tick'] <= 36000
            assert not s['controller_reset'] and s['rejoin_selected_for_existing_task'] == (arm == 'candidate')
            assert read(p/'final.json')['brains'] == s['brains']
            native = [0,0]
            action_count = 0
            for row in rows(p/'actions.jsonl'):
                assert row['tick'] == fork+action_count and len(row['actions']) == 6
                assert row['native_refinement'] == [False,row['recovery_active'][1]]
                for seat in (0,1): native[seat] += row['native_refinement'][seat]
                action_count += 1
            assert action_count == s['end_tick']-fork
            assert native == [m['native_refinement_decisions'] for m in s['metrics']]
            samples = list(rows(p/'samples.jsonl'))
            expected = list(range((fork//60+1)*60,s['end_tick']+1,60))
            if s['end_tick']%60: expected.append(s['end_tick'])
            assert [r['tick'] for r in samples] == expected
            for sample in samples:
                a = sample['state']['audit']
                assert not a['issues'] and a['max_speed'] < 500
                assert a['occupied_cells']+a['loose_cells']+a['removed_cells'] == s['initial_cells']
            max_work = Counter()
            for offset, cost in enumerate(rows(p/'costs.jsonl')):
                assert cost['tick'] == fork+offset
                for kind,cap in (('graph',4),('physics_queries',384)):
                    used = sum(v[kind] for v in cost['work'].values())
                    assert 0 <= used <= cap
                    max_work[kind] = max(max_work[kind],used)
            assert offset+1 == action_count
            completions = []
            for seat in (0,1):
                m = s['metrics'][seat]
                assert sum(m['mission_goals'].values()) == action_count
                assert len(m['completed']) == s['brains'][seat]['completed_recoveries']-s['initial_completed'][seat]
                for tick in m['completed']:
                    d = read(p/f'completion-{tick}-p{seat+1}-decision.json')
                    pilot = d['observations'][seat]['local']['combat']['recovery']['flight']['pilot']
                    assert pilot['ship_available'] and pilot['ship_form'] == 'ship' and pilot['location'] == {'aboard':seat}
                    assert d['brains'][seat]['recovery'] is None
                    completions.append({'tick':tick,'seat':seat,'pilot':pilot})
            features = Counter()
            for f in rows(p/'features.jsonl'):
                features[f['kind']] += 1
                if f['kind'] == 'survey':
                    for key,cap in (('coarse_candidates',8),('refined_candidates',8),('offset_checks',240),('staging_route_checks',16)):
                        assert 0 <= (f['survey'].get('refinement') or {}).get(key,0) <= cap
            attempt_rows = list(rows(p/'rejoin-decisions.jsonl'))
            attempt = None
            movement = []
            displacements = []
            if arm == 'control':
                assert not attempt_rows
            else:
                assert attempt_rows, 'candidate never exercised its selected behavior'
                first = attempt_rows[0]['rejoin']
                assert first['started_tick'] == fork and first['deadline_tick'] == fork+480
                for i,row in enumerate(attempt_rows):
                    assert row['tick'] == fork+i <= first['deadline_tick']+1
                    attempt = row['rejoin']
                    assert all(attempt[k] == first[k] for k in ('started_tick','deadline_tick','planet','origin','target','search_since'))
                    pilot = row['pilot']; actor = pilot['actor']; frame = pilot['planet']['motion']
                    dx,dy = actor['position']['x']-frame['position']['x'],actor['position']['y']-frame['position']['y']
                    c,sin = math.cos(frame['angle']),math.sin(frame['angle'])
                    position = (dx*c+dy*sin,dy*c-dx*sin)
                    displacements.append(math.hypot(position[0]-first['origin']['x'],position[1]-first['origin']['y']))
                    if attempt['finished_tick'] is None:
                        assert displacements[-1] <= 6.001
                        action = row['actions'][0]['Scenario']
                        assert action['kind'] == 1398079490
                        payload = action['payload']
                        assert payload[4:] == [0,0,0,1], 'rejoin authorizes only horizontal controls'
                        horizontal = struct.unpack('<f',bytes(payload[:4]))[0]
                        assert horizontal in (-1,0,1)
                        if horizontal:
                            posture = row['posture']
                            assert pilot['queries_ready'] and pilot['balanced'] and pilot['supported_planet'] == pilot['planet']['index']
                            assert posture['tick'] == row['tick'] and posture['stable'] and posture['balance'] == 'balanced'
                            assert posture['revision'] == pilot['planet']['revision'] and posture['owner'] == pilot['owner']
                            movement.append({'tick':row['tick'],'horizontal':horizontal})
                assert attempt['finished_tick'] == attempt_rows[-1]['tick']
                if attempt['rejoined_tick'] is not None:
                    row = attempt_rows[-1]
                    assert row['posture']['standing_clear'] and row['map']['tick'] == row['tick']
            arms[arm] = {'summary':s,'audit_samples':len(samples),'max_host_work':dict(max_work),
                         'features':dict(features),'completions':completions,
                         'rejoin':attempt,'rejoin_trace_ticks':len(attempt_rows),'movement_ticks':len(movement),
                         'first_movement':movement[:1],'max_rejoin_displacement':max(displacements,default=0)}
        keys = ('actions','sites','decision_fnv64','scenario_before_step_fnv64')
        first, differences = dict.fromkeys(keys),Counter()
        overlap = 0
        for a,b in itertools.zip_longest(rows(path/'control/actions.jsonl'),rows(path/'candidate/actions.jsonl')):
            if a is None or b is None: continue
            assert a['tick'] == b['tick']
            overlap += 1
            for key in keys:
                if a[key] != b[key]:
                    differences[key] += 1
                    if first[key] is None: first[key] = a['tick']
        matched = 0
        for a,b in itertools.zip_longest(itertools.islice(rows(reference),fork,None),rows(path/'control/actions.jsonl')):
            assert a is not None and b is not None
            assert a['tick'] == b['tick'] and all(a[k] == b[k] for k in keys)
            matched += 1
        assert matched == arms['control']['summary']['reference_ticks_verified'] == 36000-fork
        cases.append({'fork_tick':fork,'verification':verification,'arms':arms,
                      'comparison':{'overlap_ticks':overlap,'first_difference':first,'different_ticks':dict(differences)}})
        print(json.dumps({'fork':fork,'rejoin':arms['candidate']['rejoin'],'movement_ticks':arms['candidate']['movement_ticks'],
                          'completed':{arm:len(v['completions']) for arm,v in arms.items()},'first_difference':first}),flush=True)
    artifacts = {str(p.relative_to(root)):{'sha256':digest(p),'bytes':p.stat().st_size}
                 for p in sorted((root/'cases').rglob('*')) if p.is_file()}
    report = {'runtime':runtime,'source_files_verified':len(checked),'reference_sha256':reference_sha,
              'cases':cases,'artifact_receipts':artifacts,'auditor_sha256':digest(Path(__file__)),
              'helper_sha256':digest(repo/'tools/audit-terrain-recovery-missions.py')}
    (args.output or root/'audit.json').write_text(json.dumps(report,indent=2)+'\n')
    print(f'Verified {len(cases)} comparisons, {len(artifacts)} artifacts and {len(checked)} source inputs.')


if __name__ == '__main__': main()
