#!/usr/bin/env python3
"""Frozen opt-in cover-retry trial, including disabled parity and failed attempts."""
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


N = module('neutral', 'validate-current-neutral-costs.py')
V = module('visit_replays', 'validate-visit-metrics.py')
F = N.F
MODEL = 'cover_retry_cooldown_v1'


def plan():
    result = F.plan()
    for item in result:
        if item['kind'] == 'held_out':
            world = int(item['seed_namespace'].rsplit(':', 1)[1])
            item['seed_namespace'] = f'cover-retry-cooldown-v1:{world}'
            item['seed'] = int.from_bytes(hashlib.sha256(item['seed_namespace'].encode()).digest()[:8], 'little')
    return result


def arguments(item):
    # Both arms keep the previous current-neutral and published-flag experiments.
    command, policies = N.arguments(dict(item, candidate=True))
    command += ['--cover-retry-seats', str(item['seat']) if item['candidate'] else 'none']
    return command, policies


def retry_stats(report, enabled):
    history = [(s, m) for s, m in enumerate(report['missions'])]
    history += [(e['seat'], e['telemetry']) for e in report['events']]
    history += [(s, m) for sample in report['samples'] for s, m in enumerate(sample['missions'])]
    captures = {}
    for seat, mission in history:
        c = mission['capture']
        if c is None:
            continue
        memory = c.get('cover_retry_cooldown')
        assert (memory is not None) == (seat in enabled)
        if memory is None or c['started_tick'] is None:
            continue
        assert len(memory['rejected']) <= 8
        identities = {(r['site']['planet'], r['site']['bearing']) for r in memory['rejected']}
        assert len(identities) == len(memory['rejected'])
        assert all(r['until_tick'] == r['rejected_tick'] + 1800 for r in memory['rejected'])
        first, last = memory['first_blocked_tick'], memory['last_blocked_tick']
        assert (memory['blocked_selections'] == 0) == (first is None and last is None)
        if first is not None:
            assert c['started_tick'] <= first <= last
        key = seat, c['started_tick']
        previous = captures.get(key)
        if previous is None or memory['blocked_selections'] > previous['blocked_selections']:
            captures[key] = dict(seat=seat, started_tick=c['started_tick'],
                blocked_selections=memory['blocked_selections'], first_blocked_tick=first, last_blocked_tick=last)
    return dict(captures=list(captures.values()),
        affected_captures=sum(c['blocked_selections'] > 0 for c in captures.values()),
        blocked_selections=sum(c['blocked_selections'] for c in captures.values()),
        first_blocked_tick=min((c['first_blocked_tick'] for c in captures.values() if c['first_blocked_tick'] is not None), default=None),
        scope='Maxima per capture from retained mission snapshots; repeated exclusion calls are not independent decisions or completion probabilities.')


def analyze(root, item, policies):
    result = N.analyze(root, dict(item, candidate=True), policies)
    report = json.loads((root/'report.json').read_text())
    expected = [None, None]
    enabled = [item['seat']] if item['candidate'] else []
    for seat in enabled:
        expected[seat] = MODEL
    assert [c.get('cover_retry_model') for c in report['policy_configuration']] == expected
    result['cover_retries'] = retry_stats(report, enabled)
    return result


def without_cooldown(value):
    if isinstance(value, dict):
        return {k: without_cooldown(v) for k, v in value.items() if k not in ['cover_retry_cooldown', 'cover_cooldown']}
    if isinstance(value, list):
        return [without_cooldown(v) for v in value]
    return value


def prefix_parity(before, after, first_blocked):
    def rows(path):
        for row in F.rows(path):
            if first_blocked is not None and row['tick'] >= first_blocked:
                break
            yield without_cooldown(row)
    count, digest = 0, hashlib.sha256()
    for a, b in itertools.zip_longest(rows(before), rows(after)):
        assert a == b, 'recorded behavior changed before a cooldown exclusion'
        count += 1
        digest.update((json.dumps(a, sort_keys=True, separators=(',', ':'))+'\n').encode())
    return dict(rows=count, before_tick=first_blocked, normalized_sha256=digest.hexdigest())


def regressions(binary, out, baseline_manifest, result, save):
    prior = json.loads(baseline_manifest.read_text())
    assert prior['complete']
    for name, seat, _, _, _ in V.CASES:
        reference = prior['runs'][name]
        old_root = Path(reference['commands']['after'][reference['commands']['after'].index('--out')+1])
        assert F.E.digest(old_root/'report.json') == reference['report_sha256']['after']
        original = json.loads((old_root/'report.json').read_text())
        runs = {}
        result[name] = runs
        for candidate in [False, True]:
            arm = 'candidate' if candidate else 'predecessor'
            command = list(reference['commands']['after'])
            command[0] = str(binary)
            root = out/(name+'-'+arm)
            command[command.index('--out')+1] = str(root)
            command += ['--cover-retry-seats', str(seat) if candidate else 'none']
            runs[arm] = dict(command=command)
            save()
            print('regression', name, arm, flush=True)
            with (out/(name+'-'+arm+'.log')).open('w') as log:
                subprocess.run(command, check=True, stdout=log, stderr=log, timeout=900)
            report = json.loads((root/'report.json').read_text())
            assert report['physics_ok']
            item = dict(candidate=candidate, seat=seat, seed=report['seed'])
            run = analyze(root, item, [p['policy'] for p in report['policy_configuration']])
            run.update(command=command, report_sha256=F.E.digest(root/'report.json'))
            if not candidate:
                for field in V.EXACT_REPORT_FIELDS + ['metrics']:
                    assert report[field] == original[field], (name, field)
                for stream in V.EXACT_STREAMS:
                    assert F.E.digest(root/stream) == reference['unchanged_stream_sha256'][stream]
                run['disabled_exact_parity'] = True
            else:
                run['prefix_parity'] = prefix_parity(old_root/'trace.jsonl', root/'trace.jsonl',
                    run['cover_retries']['first_blocked_tick'])
            runs[arm] = run
            save()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--baseline-manifest', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip(), 'freeze code and plan first'
    binary = args.binary.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, complete=False, plan=plan(), runs={}, comparisons=[],
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        binary_sha256=F.E.digest(binary), runner_sha256=F.E.digest(Path(__file__)),
        baseline_manifest_sha256=F.E.digest(args.baseline_manifest),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [N, V, F, F.P, F.D, F.E, F.M, N.A]},
        scope='Six recorded regression runs, 32 directed runs and 32 new finished matches across four correlated worlds. Both arms use current-neutral and flag evidence; only selected-seat cover retry memory differs. Unchanged safety, retry/time limits, priority, hysteresis and shared 4/384 budget. No default promotion or Pi performance claim.')
    save = lambda: F.D.write(args.out/'summary.json', result)
    save()
    try:
        result['regressions'] = {}
        regressions(binary, args.out, args.baseline_manifest, result['regressions'], save)
        save()
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
            assert same or result['runs'][names[1]]['cover_retries']['affected_captures'], 'behavior changed without cooldown activation'
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
