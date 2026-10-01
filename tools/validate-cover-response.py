#!/usr/bin/env python3
"""Frozen qualified-cover trial: recorded failures, directed controls, fresh worlds."""
import argparse
import hashlib
import importlib.util
import itertools
import json
from pathlib import Path
import subprocess


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


R = module('retries', 'validate-cover-retries.py')
A = module('alternatives', 'probe-cover-alternatives.py')
N, V, F = R.N, R.V, R.F
MODEL = 'qualified_cover_response_v1'
COUNTERS = ['failures', 'searches', 'filtered_directions', 'requested_sites', 'measured_sites',
            'selected_routes', 'exhausted_searches', 'budget_exhaustions', 'deadlines', 'exposure_releases']


def plan():
    result = F.plan()
    for item in result:
        if item['kind'] == 'held_out':
            world = int(item['seed_namespace'].rsplit(':', 1)[1])
            item['seed_namespace'] = f'qualified-cover-response-v1:{world}'
            item['seed'] = int.from_bytes(hashlib.sha256(item['seed_namespace'].encode()).digest()[:8], 'little')
    return result


def arguments(item):
    command, policies = N.arguments(dict(item, candidate=True))
    command += ['--cover-response-seats', str(item['seat']) if item['candidate'] else 'none']
    return command, policies


def check_state(capture, state):
    assert all(isinstance(state[k], int) and state[k] >= 0 for k in COUNTERS)
    assert state['measured_sites'] <= state['requested_sites'] <= 8 * state['searches']
    if state['required_since'] is not None:
        assert capture['started_tick'] <= state['required_since'] and state['failures'] > 0
    if state['first_effect_tick'] is not None:
        assert capture['started_tick'] <= state['first_effect_tick'] and state['failures'] > 0
    s = state['search']
    if s is None:
        return
    assert s['deadline_tick'] == s['started_tick'] + 600
    assert 0 <= s['probes'] <= 8 and len(s['pending']) <= 8
    ids = [(p['planet'], p['bearing']) for p in s['pending']]
    assert len(ids) == len(set(ids)) and all(p == s['planet'] and 0 <= b < 64 for p, b in ids)
    assert (s['finished_tick'] is None) == (s['outcome'] is None)
    if s['finished_tick'] is not None:
        assert s['started_tick'] <= s['finished_tick'] <= s['deadline_tick']
    assert 60 <= s['hold_altitude'] <= 85
    assert s['omitted'] >= 0
    if s['outcome'] == 'observed_candidates_exhausted':
        assert s['seeded'] and not s['pending'] and s['omitted'] == 0
    if s['outcome'] == 'evidence_budget_exhausted':
        assert s['seeded'] and not s['pending'] and s['omitted'] > 0 and s['probes'] == 8


def response_stats(report, enabled):
    history = [(s, m) for s, m in enumerate(report['missions'])]
    history += [(e['seat'], e['telemetry']) for e in report['events']]
    history += [(s, m) for sample in report['samples'] for s, m in enumerate(sample['missions'])]
    captures = {}
    for seat, mission in history:
        c = mission['capture']
        if c is None:
            continue
        state = c.get('cover_response')
        assert (state is not None) == (seat in enabled)
        if state is None or c['started_tick'] is None:
            continue
        check_state(c, state)
        key = seat, c['started_tick']
        result = captures.setdefault(key, dict(seat=seat, started_tick=c['started_tick'],
            first_effect_tick=None, **{k: 0 for k in COUNTERS}))
        for k in COUNTERS:
            result[k] = max(result[k], state[k])
        if state['first_effect_tick'] is not None:
            assert result['first_effect_tick'] in [None, state['first_effect_tick']]
            result['first_effect_tick'] = state['first_effect_tick']
    return dict(captures=list(captures.values()), totals={k: sum(c[k] for c in captures.values()) for k in COUNTERS},
        affected_captures=sum(c['first_effect_tick'] is not None for c in captures.values()),
        first_effect_tick=min((c['first_effect_tick'] for c in captures.values() if c['first_effect_tick'] is not None), default=None),
        scope='Maxima per capture from retained snapshots; counts are events and searches, not independent success probabilities.')


def without_option(value):
    if isinstance(value, dict):
        return {k: without_option(v) for k, v in value.items() if k not in ['cover_response', 'cover_required']}
    if isinstance(value, list):
        return [without_option(v) for v in value]
    return value


def prefix_parity(before, after, first_effect):
    def rows(path):
        for row in F.rows(path):
            tick = row['observation']['local']['combat']['recovery']['flight']['pilot']['tick']
            if first_effect is not None and tick >= first_effect:
                break
            yield without_option(row)
    count, digest = 0, hashlib.sha256()
    for a, b in itertools.zip_longest(rows(before), rows(after)):
        assert a == b, 'behavior changed before the recorded cover response'
        count += 1
        digest.update((json.dumps(a, sort_keys=True, separators=(',', ':'))+'\n').encode())
    return dict(rows=count, before_tick=first_effect, normalized_sha256=digest.hexdigest())


def analyze(root, item, policies):
    result = N.analyze(root, dict(item, candidate=True), policies)
    report = json.loads((root/'report.json').read_text())
    enabled = [item['seat']] if item['candidate'] else []
    assert [c.get('cover_response_model') for c in report['policy_configuration']] == [MODEL if s in enabled else None for s in range(2)]
    assert not any(c.get('cover_retry_model') for c in report['policy_configuration'])
    result['cover_response'] = response_stats(report, enabled)
    return result


def regressions(binary, out, study, result, save):
    previous = json.loads((study/'summary.json').read_text())
    assert previous['complete']
    for label, _, _, seat, _ in A.CASES:
        old = previous['runs'][label+'-predecessor']
        command = list(old['command'])
        old_root = Path(command[command.index('--out')+1])
        assert F.E.digest(old_root/'report.json') == old['report_sha256']
        original = json.loads((old_root/'report.json').read_text())
        for flag in ['--probe-cover-seat', '--probe-cover-ticks']:
            i = command.index(flag)
            del command[i:i+2]
        runs = result.setdefault(label, {})
        for candidate in [False, True]:
            arm = 'candidate' if candidate else 'predecessor'
            current = list(command)
            root = out/(label+'-'+arm)
            current[0] = str(binary)
            current[current.index('--out')+1] = str(root)
            current += ['--cover-response-seats', str(seat) if candidate else 'none']
            runs[arm] = dict(command=current)
            save()
            print('regression', label, arm, flush=True)
            with (out/(label+'-'+arm+'.log')).open('w') as log:
                subprocess.run(current, check=True, stdout=log, stderr=log, timeout=900)
            report = json.loads((root/'report.json').read_text())
            assert report['physics_ok']
            item = dict(candidate=candidate, seat=seat, seed=report['seed'])
            run = analyze(root, item, [p['policy'] for p in report['policy_configuration']])
            run.update(command=current, report_sha256=F.E.digest(root/'report.json'))
            if not candidate:
                for field in V.EXACT_REPORT_FIELDS + ['metrics', 'policy_configuration']:
                    assert report[field] == original[field], (label, field)
                for stream in V.EXACT_STREAMS:
                    assert F.E.digest(root/stream) == F.E.digest(old_root/stream), (label, stream)
                run['disabled_exact_parity'] = True
                run['sensor_parity'] = A.C.audit_sensors(old_root, root)
            else:
                run['prefix_parity'] = prefix_parity(old_root/'trace.jsonl', root/'trace.jsonl',
                    run['cover_response']['first_effect_tick'])
            runs[arm] = run
            save()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--study', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip(), 'freeze code and plan first'
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, plan=plan(), runs={}, comparisons=[], regressions={},
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        binary_sha256=F.E.digest(binary), runner_sha256=F.E.digest(Path(__file__)),
        baseline_summary_sha256=F.E.digest(args.study/'summary.json'),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [R, N, V, F, F.P, F.D, F.E, F.M, N.A, A, A.C]},
        scope='Ten recorded replays, 32 directed trials and 32 new finished matches across four correlated worlds. Only the tested-seat qualified cover response differs. Eight probe requests and 600 ticks per search; original capture/safety gates retained. Synchronous sensing is outside the shared 4/384 dispatch quota. No default promotion or Pi performance claim.')
    save = lambda: F.D.write(args.out/'summary.json', result)
    save()
    try:
        regressions(binary, args.out, args.study, result['regressions'], save)
        for item in result['plan']:
            command, policies = arguments(item)
            name = item['name']
            run = F.D.run(binary, args.out, name, command, item['seat'],
                seconds=180 if item['kind'] == 'directed' else 600, require_finish=item['kind'] == 'held_out')
            run.update(analyze(args.out/name, item, policies))
            result['runs'][name] = run
            save()
        for item in result['plan']:
            if not item['candidate']:
                continue
            names = [item['group']+'-'+arm for arm in ['predecessor', 'candidate']]
            a, b = [json.loads((args.out/name/'report.json').read_text()) for name in names]
            assert a['initial_world'] == b['initial_world']
            same = F.D.same_physical_outcomes(a, b)
            assert same or result['runs'][names[1]]['cover_response']['affected_captures'], 'behavior changed without cover response'
            result['comparisons'].append(dict(group=item['group'], kind=item['kind'], seat=item['seat'],
                matching_recorded_physical_outcomes=same, predecessor_outcome=F.D.outcome(a, item['seat']),
                candidate_outcome=F.D.outcome(b, item['seat'])))
        assert F.E.digest(binary) == result['binary_sha256']
        result['complete'] = True
        save()
    except Exception as error:
        result['error'] = repr(error)
        save()
        raise


if __name__ == '__main__':
    main()
