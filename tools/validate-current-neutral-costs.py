#!/usr/bin/env python3
"""Frozen trial of surveying the current neutral beside a neutral alternative."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import subprocess


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


F = module('flags', 'validate-flag-costs.py')
A = module('arrival', 'analyze-evidence-arrival.py')
MODEL = 'capture_value_current_neutral_v1'


def plan():
    result = F.plan()
    for item in result:
        if item['kind'] == 'held_out':
            world = int(item['seed_namespace'].rsplit(':', 1)[1])
            item['seed_namespace'] = f'current-neutral-costs-v1:{world}'
            item['seed'] = int.from_bytes(hashlib.sha256(item['seed_namespace'].encode()).digest()[:8], 'little')
    return result


def arguments(item):
    # Both arms include the merged published-flag experiment. The only new
    # demand in the candidate is its current neutral destination.
    command, policies = F.arguments(dict(item, candidate=True))
    command += ['--survey-current-neutral', str(item['seat']) if item['candidate'] else 'none']
    return command, policies


def analyze(root, item, policies):
    result = F.analyze(root, dict(item, candidate=True), policies)
    report = json.loads((root/'report.json').read_text())
    expected = [None, None]
    if item['candidate']:
        expected[item['seat']] = MODEL
    assert [c.get('current_neutral_model') for c in report['policy_configuration']] == expected
    result['evidence_arrival'] = A.analyze(report, F.rows(root/'mission-evaluations.jsonl'), item['seat'])
    expected_model = MODEL if item['candidate'] else 'capture_value_published_flags_v1'
    for row in F.rows(root/'mission-evaluations.jsonl'):
        if row['actor'] == f'player_{item["seat"]+1}':
            assert row['model'] == expected_model
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    opts = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip(), 'freeze code and plan first'
    binary = opts.binary.resolve(strict=True)
    opts.out.mkdir(parents=True, exist_ok=False)
    result = dict(schema=1, plan=plan(), runs={}, comparisons=[], complete=False,
        source_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        binary_sha256=F.E.digest(binary), runner_sha256=F.E.digest(Path(__file__)),
        tools={Path(m.__file__).name:F.E.digest(Path(m.__file__)) for m in [F, F.P, F.D, F.E, A]},
        scope='32 recorded directed trials and 32 new finished matches in four correlated generated worlds. Both arms admit published flag costs; only the candidate surveys its current neutral alongside one neutral alternative. Same native controls, thresholds and shared 4/384 quota; defaults remain unchanged.')
    save = lambda: F.D.write(opts.out/'summary.json', result)
    save()
    for item in result['plan']:
        command, policies = arguments(item)
        name = item['name']
        try:
            run = F.D.run(binary, opts.out, name, command, item['seat'],
                seconds=180 if item['kind'] == 'directed' else 600,
                require_finish=item['kind'] == 'held_out')
            run.update(analyze(opts.out/name, item, policies))
            run['log_sha256'] = F.E.digest(opts.out/(name+'.log'))
            result['runs'][name] = run
            save()
        except Exception as error:
            result['error'] = dict(case=item, command=command, error=repr(error))
            save()
            raise
    for item in result['plan']:
        if not item['candidate']:
            continue
        names = [item['group']+suffix for suffix in ['-predecessor', '-candidate']]
        a, b = [json.loads((opts.out/name/'report.json').read_text()) for name in names]
        assert a['initial_world'] == b['initial_world']
        same = F.D.same_physical_outcomes(a, b)
        switches = [r['missions'][item['seat']]['destination_planning']['switches'] for r in [a, b]]
        assert any(switches) or same, 'behavior changed without a destination switch'
        result['comparisons'].append(dict(group=item['group'], kind=item['kind'], seat=item['seat'],
            matching_recorded_physical_outcomes=same, predecessor_outcome=F.D.outcome(a, item['seat']),
            candidate_outcome=F.D.outcome(b, item['seat'])))
    assert F.E.digest(binary) == result['binary_sha256']
    result['complete'] = True
    save()


if __name__ == '__main__':
    main()
