#!/usr/bin/env python3
"""Replay two failed approaches and a successful control after the visit-metrics fix."""
import argparse
import copy
import importlib.util
import json
import math
from pathlib import Path
import subprocess


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


A = module('visits', 'audit-mission-visits.py')
F = module('flags', 'validate-flag-costs.py')
CASES = [
    ('value-destination-p1-bearing-0.8-candidate', 0, 4800, 4956, [2863, 7264]),
    ('world3-asteroids3-p2-candidate', 1, 10800, 10915, [9220]),
    ('value-destination-p1-bearing0.8-candidate', 0, 4700, 4775, [2799]),
]
EXACT_STREAMS = ['trace.jsonl', 'mission-evaluations.jsonl', 'mission-evaluation-work.jsonl',
                 'flag-survey.jsonl', 'flag-survey-work.jsonl', 'destination-cover.jsonl']
EXACT_REPORT_FIELDS = [k for k in F.D.PHYSICAL_FIELDS if k != 'metrics']+[
    'initial_world', 'missions', 'events', 'samples', 'pilot_damage_events', 'mission_progress']


def compare_metrics(before, after):
    audit = A.audit(before)
    assert audit['counts'].get('unverified', 0) == 0
    assert audit['counts']['milestones_outside_visit'] == 0
    expected = copy.deepcopy(before['metrics'])
    for v in audit['visits']:
        visit = next(old for old in expected[v['seat']]['visits']
                     if (old['planet'], old['selected_tick']) == (v['planet'], v['selected_tick']))
        visit.update(v['expected'])
    assert after['metrics'] == expected, 'metrics changed beyond terminal correction'
    fixed = A.audit(after)
    assert fixed['counts']['corrected_visits'] == 0 and fixed['counts']['milestones_outside_visit'] == 0
    return audit


def trace_states(path, seat, visits):
    """Sparse observations of native retries; these are not a per-tick risk model."""
    output = [dict(visit=v, states=[]) for v in visits]
    previous = {}
    for row in F.rows(path):
        if row['seat'] != seat:
            continue
        for i, record in enumerate(output):
            visit = record['visit']
            end = visit['terminal']['tick']
            if not visit['selected_tick'] <= row['tick'] <= end:
                continue
            mission = row['mission']
            capture = mission['capture']
            combat = row['observation']['local']['combat']
            p = combat['recovery']['flight']['pilot']
            state = {k: capture.get(k) for k in ['goal', 'site', 'replans', 'cover_replans',
                      'solar_replans', 'objective_replans', 'failure']} if capture else {}
            state.update(frame=p['planet']['index'], target=mission['target'])
            if previous.get(i) == state:
                continue
            previous[i] = state
            target = combat['target']
            vec = lambda v: (v['x'], v['y'])
            distance = None if target is None else math.dist(vec(target['motion']['position']), vec(p['ship']['position']))
            record['states'].append(dict(tick=row['tick'], **state,
                exposed=target is not None and not target['ground_occluded'] and distance < 300,
                opponent_distance=distance,
                cover=next((c for c in row['observation']['local']['cover']
                            if c['site'] == state.get('site')), None)))
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--study', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip(), 'commit the fix and replay plan first'
    summary_path = args.study/'summary.json'
    summary = json.loads(summary_path.read_text())
    assert summary['complete']
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False,
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        source_summary_sha256=A.digest(summary_path), binary_sha256=A.digest(binary),
        before_binary_sha256=summary['binary_sha256'],
        tools={Path(m.__file__).name:A.digest(Path(m.__file__)) for m in [A, F, F.D]},
        runner_sha256=A.digest(Path(__file__)), runs={},
        scope='Known regression replays, not a new policy or strength trial. Only visit bookkeeping changes. Physical observations, native controls and all recorded planner streams must remain exact. Trace observations are pre-intent; mission telemetry is post-intent. Sparse state counts are not durations.')
    save = lambda: F.D.write(args.out/'summary.json', result)
    save()
    try:
        for name, seat, start, end, selections in CASES:
            source = summary['runs'][name]
            original_path = args.study/name/'report.json'
            assert A.digest(original_path) == source['report_sha256']
            original = json.loads(original_path.read_text())
            commands, reports, roots = {}, {}, {}
            for arm in ['before', 'after']:
                command = list(source['command'])
                if arm == 'after':
                    command[0] = str(binary)
                else:
                    assert A.digest(Path(command[0])) == summary['binary_sha256']
                root = args.out/(name+'-'+arm)
                command[command.index('--out')+1] = str(root)
                command += ['--trace', 'true', '--trace-start-tick', str(start), '--trace-end-tick', str(end)]
                commands[arm], roots[arm] = command, root
                print(name, arm, flush=True)
                with (args.out/(name+'-'+arm+'.log')).open('w') as log:
                    subprocess.run(command, check=True, stdout=log, stderr=log, timeout=900)
                reports[arm] = json.loads((root/'report.json').read_text())
                assert reports[arm]['physics_ok']
                for field in EXACT_REPORT_FIELDS:
                    assert reports[arm][field] == original[field], (name, arm, field)
            assert reports['before']['metrics'] == original['metrics']
            audit = compare_metrics(reports['before'], reports['after'])
            streams = {}
            for stream in EXACT_STREAMS:
                before, after = [A.digest(roots[arm]/stream) for arm in ['before', 'after']]
                assert before == after, (name, stream)
                if stream != 'trace.jsonl':
                    assert before == A.digest(args.study/name/stream), (name, stream, 'original')
                streams[stream] = before
            visits = [v for v in audit['visits'] if v['seat'] == seat and v['selected_tick'] in selections]
            assert len(visits) == len(selections)
            allocation = F.allocation_audit(roots['after'])
            assert allocation == F.allocation_audit(roots['before'])
            result['runs'][name] = dict(commands=commands, original_report_sha256=source['report_sha256'],
                report_sha256={arm:A.digest(root/'report.json') for arm, root in roots.items()},
                unchanged_stream_sha256=streams, unchanged_report_fields=EXACT_REPORT_FIELDS,
                corrections=audit['counts'], allocation=allocation,
                attempts=trace_states(roots['after']/'trace.jsonl', seat, visits))
            save()
        assert A.digest(binary) == result['binary_sha256']
        result['complete'] = True
        save()
    except Exception as error:
        result['error'] = repr(error)
        save()
        raise


if __name__ == '__main__':
    main()
