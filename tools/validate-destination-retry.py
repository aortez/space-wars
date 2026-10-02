#!/usr/bin/env python3
"""Frozen opt-in destination failure trial: recorded retries and fresh matches."""
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


Q = module('cover_response', 'validate-cover-response.py')
R = module('revisits', 'analyze-destination-revisits.py')
F, N, V = Q.F, Q.N, Q.V
MODEL = 'destination_failure_context_v1'
COUNTERS = ['recorded_failures', 'context_invalidations', 'initial_changes',
            'switch_rejections', 'probe_rejections', 'retry_selections']
EXTRA_CASES = ['world2-asteroids0-p1', 'world1-asteroids0-p1', 'destination-p1-bearing-0.8']


def plan():
    result = [p for p in F.plan() if p['kind'] == 'held_out']
    for item in result:
        world = int(item['seed_namespace'].rsplit(':', 1)[1])
        item['seed_namespace'] = f'destination-failure-context-v1:{world}'
        item['seed'] = int.from_bytes(hashlib.sha256(item['seed_namespace'].encode()).digest()[:8], 'little')
    return result


def arguments(item):
    command, policies = N.arguments(dict(item, candidate=True))
    return command + ['--trace', 'true', '--destination-retry-seats',
                      str(item['seat']) if item['candidate'] else 'none'], policies


def recorded_plan(study):
    result = []
    seats = {case[0]:case[3] for case in Q.A.CASES}
    for name, arms in study['regressions'].items():
        for arm in ['predecessor', 'candidate']:
            result.append(dict(name='recorded-'+name+'-cover-'+arm, seat=seats[name], source=arms[arm]))
    for name in EXTRA_CASES:
        for arm in ['predecessor', 'candidate']:
            result.append(dict(name='recorded-'+name+'-cover-'+arm, seat=0, source=study['runs'][name+'-'+arm]))
    assert len(result) == 16
    return result


def without_option(value):
    if isinstance(value, dict):
        return {k: without_option(v) for k, v in value.items() if k != 'destination_retry'}
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
        assert a == b, 'behavior changed before the recorded destination intervention'
        count += 1
        digest.update((json.dumps(a, sort_keys=True, separators=(',', ':'))+'\n').encode())
    return dict(rows=count, before_tick=first_effect, normalized_sha256=digest.hexdigest())


def check_failure(f):
    assert f['selected_tick'] <= f['started_tick'] <= f['failed_tick']
    assert f['kind'] in ['incomplete_search', 'execution_limit', 'unclassified_failure',
                         'observed_cover_constraints', 'observed_ground_route_failure']
    assert f['reason'] and f['context']['revision'] >= 0
    assert f['site'] is None or f['site']['planet'] == f['context']['planet']
    search = f['cover_search']
    if search is not None:
        assert search['probes'] <= 8 and search['omitted'] >= 0


def retry_stats(report, enabled):
    observations = [(s, m) for s, m in enumerate(report['missions'])]
    observations += [(e['seat'], e['telemetry']) for e in report.get('events', [])]
    observations += [(s, m) for row in report.get('samples', []) for s, m in enumerate(row['missions'])]
    for seat, mission in observations:
        memory = mission.get('destination_retry')
        assert (memory is not None) == (seat in enabled)
        if memory is None:
            continue
        assert all(isinstance(memory[k], int) and memory[k] >= 0 for k in COUNTERS)
        assert len(memory['failures']) <= memory['recorded_failures']
        planets = [f['context']['planet'] for f in memory['failures']]
        assert len(planets) == len(set(planets))
        effects = sum(memory[k] for k in ['initial_changes', 'switch_rejections', 'probe_rejections'])
        assert (effects == 0) == (memory['first_effect_tick'] is None)
        assert (effects == 0) == (memory['last_decision'] is None)
        assert (memory['retry_selections'] == 0) == (memory['last_retry'] is None)
        for f in memory['failures']:
            check_failure(f)
        if memory['last_decision'] is not None:
            d = memory['last_decision'];check_failure(d['failure'])
            assert memory['recorded_failures'] > 0
            assert d['rejected'] == d['failure']['context']['planet']
            assert d['retained'] != d['rejected']
            assert d['failure']['failed_tick'] < d['tick']
            assert memory['first_effect_tick'] <= d['tick']
        if memory['last_retry'] is not None:
            d = memory['last_retry'];check_failure(d['failure'])
            assert memory['recorded_failures'] > 0
            assert d['failure']['failed_tick'] < d['tick']
        final = report['missions'][seat]['destination_retry']
        assert all(memory[k] <= final[k] for k in COUNTERS), 'mission memory counters reset mid-run'
        if memory['first_effect_tick'] is not None:
            assert memory['first_effect_tick'] == final['first_effect_tick']
    return {str(seat): report['missions'][seat]['destination_retry'] for seat in enabled}


def analyze(root, seat, enabled):
    report = json.loads((root/'report.json').read_text())
    policies = [p['policy'] for p in report['policy_configuration']]
    result = N.analyze(root, dict(candidate=True, seat=seat, seed=report['seed']), policies)
    assert [p.get('destination_retry_model') for p in report['policy_configuration']] == [MODEL if s in enabled else None for s in range(2)]
    assert not any(p.get('cover_retry_model') for p in report['policy_configuration'])
    result['destination_retry'] = retry_stats(report, enabled)
    result['revisits'] = R.analyze(report, seat, result['decisions'])
    return result


def compare(a, b, seat, root_a, root_b):
    assert a['initial_world'] == b['initial_world']
    first = b['missions'][seat]['destination_retry']['first_effect_tick']
    same = F.D.same_physical_outcomes(a, b)
    parity = prefix_parity(root_a/'trace.jsonl', root_b/'trace.jsonl', first)
    if first is None:
        assert same, 'physical outcomes changed without an intervention'
        for field in ['missions', 'events', 'samples', 'mission_progress']:
            assert without_option(a[field]) == without_option(b[field]), (field, 'changed without intervention')
    return dict(matching_recorded_physical_outcomes=same, first_effect_tick=first,
        predecessor_outcome=F.D.outcome(a, seat), candidate_outcome=F.D.outcome(b, seat), prefix_parity=parity)


def recorded(binary, out, study, result, save):
    for item in recorded_plan(study):
        source = item['source'];old_command = source['command']
        old = Path(old_command[old_command.index('--out')+1]);seat = item['seat']
        for filename, sha in source['hashes'].items():
            assert F.E.digest(old/filename) == sha, (item['name'], filename)
        original = json.loads((old/'report.json').read_text())
        runs = {};result['regressions'][item['name']] = runs
        for candidate in [False, True]:
            arm = 'candidate' if candidate else 'predecessor';name = item['name']+'-'+arm;root = out/name
            command = list(old_command);command[0] = str(binary);command[command.index('--out')+1] = str(root)
            if '--trace' not in command:
                command += ['--trace', 'true']
            command += ['--destination-retry-seats', str(seat) if candidate else 'none']
            runs[arm] = dict(command=command, source_report=dict(path=str(old/'report.json'), sha256=F.E.digest(old/'report.json')))
            save();print(name, flush=True)
            with (out/(name+'.log')).open('w') as log:
                subprocess.run(command, check=True, stdout=log, stderr=log, timeout=900)
            report = json.loads((root/'report.json').read_text());assert report['physics_ok']
            expected_config = json.loads(json.dumps(original['policy_configuration']))
            if candidate: expected_config[seat]['destination_retry_model'] = MODEL
            assert report['policy_configuration'] == expected_config
            runs[arm].update(analyze(root, seat, [seat] if candidate else []))
            if not candidate:
                for field in V.EXACT_REPORT_FIELDS + ['metrics', 'policy_configuration']:
                    assert report[field] == original[field], (name, field)
                for stream in V.EXACT_STREAMS:
                    if stream == 'trace.jsonl' and not (old/stream).exists(): continue
                    assert F.E.digest(root/stream) == F.E.digest(old/stream), (name, stream)
                runs[arm]['disabled_exact_parity'] = True
                runs[arm]['sensor_parity'] = Q.A.C.audit_sensors(old, root)
            save()
        roots = [out/(item['name']+'-'+a) for a in ['predecessor','candidate']]
        reports = [json.loads((r/'report.json').read_text()) for r in roots]
        result['comparisons'].append(dict(group=item['name'], kind='recorded', seat=seat,
            **compare(*reports, seat, *roots)))
        save()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--study', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip(), 'freeze code and plan first'
    study = json.loads((args.study/'summary.json').read_text());assert study['complete']
    binary = args.binary.resolve(strict=True);args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, plan=plan(), recorded_plan=[{k:v for k,v in i.items() if k!='source'} for i in recorded_plan(study)],
        source_commit=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        binary_sha256=F.E.digest(binary), runner_sha256=F.E.digest(Path(__file__)),
        source_summary_sha256=F.E.digest(args.study/'summary.json'),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [Q,R,N,V,F,F.P,F.D,F.E,F.M,Q.A,Q.A.C,N.A]},
        runs={}, regressions={}, comparisons=[],
        scope='32 recorded replays (both original cover settings), then 32 new finished matches in four correlated worlds with cover response disabled. Only the tested v13 seat destination-failure preference differs within each pair. No new sensors, timing coefficients, retry limits or defaults.')
    save = lambda: F.D.write(args.out/'summary.json', result)
    save()
    try:
        recorded(binary, args.out, study, result, save)
        for item in result['plan']:
            command, _ = arguments(item);name = item['name'];seat = item['seat']
            result['runs'][name] = dict(arguments=command);save()
            run = F.D.run(binary, args.out, name, command, seat, seconds=600, require_finish=True)
            run.update(analyze(args.out/name, seat, [seat] if item['candidate'] else []))
            report = json.loads((args.out/name/'report.json').read_text())
            assert not any(p.get('cover_response_model') for p in report['policy_configuration'])
            result['runs'][name] = run;save()
        for item in result['plan']:
            if not item['candidate']: continue
            roots = [args.out/(item['group']+'-'+arm) for arm in ['predecessor','candidate']]
            reports = [json.loads((r/'report.json').read_text()) for r in roots]
            result['comparisons'].append(dict(group=item['group'], kind='held_out', seat=item['seat'],
                **compare(*reports, item['seat'], *roots)))
        assert F.E.digest(binary) == result['binary_sha256']
        result['complete'] = True;save()
    except Exception as error:
        result['error'] = repr(error);save();raise


if __name__ == '__main__': main()
