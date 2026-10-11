#!/usr/bin/env python3
"""Check named recovery selection against the frozen hatch-arrival matches."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile


def read(path):
    return json.loads(path.read_text())


def rows(path):
    with path.open() as source:
        for line in source:
            yield json.loads(line)


def sha(path):
    value = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024*1024), b''):
            value.update(block)
    return value.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    repo = Path(__file__).resolve().parent.parent
    runtime = read(root/'runtime.json')
    assert sha(root/'recovery-profile') == runtime['binary_sha256']
    assert sha(root/'source-inputs.json') == runtime['source_inputs_sha256']
    inputs = read(root/'source-inputs.json')
    checked = set()
    process = subprocess.Popen(['git', 'archive', runtime['source_commit']], cwd=repo, stdout=subprocess.PIPE)
    with tarfile.open(fileobj=process.stdout, mode='r|') as archive:
        for item in archive:
            if item.name in inputs:
                data = item.linkname.encode() if item.issym() else archive.extractfile(item).read()
                assert hashlib.sha256(data).hexdigest() == inputs[item.name]
                checked.add(item.name)
    assert process.wait() == 0 and checked == set(inputs)
    correction = read(root/'harness-correction.json')
    earlier = root.parent/Path(correction['prior_directory']).name
    assert sha(earlier/'recovery-profile') == correction['failed_runtime']['binary_sha256']
    assert read(earlier/'results.json') == correction['failed_cases']
    assert len(correction['failed_cases']) == 2
    assert all(c['exit_code'] == 101 for c in correction['failed_cases'])
    assert all(not (earlier/'cases'/c['name']/'output/actions.jsonl').exists() for c in correction['failed_cases'])
    changed_runtime = subprocess.check_output(
        ['git', 'diff', '--name-only', correction['failed_runtime']['source_commit'],
         runtime['source_commit'], '--', 'crates', 'scenarios'], cwd=repo, text=True).splitlines()
    assert changed_runtime == ['crates/spacewars-ai/src/mission_pilot/terrain_recovery/qualification/profile.rs']
    provenance = read(root/'references/provenance.json')
    for name, digest in provenance['files'].items():
        assert sha(root/'references'/name) == digest
    source_status = read(root/'terrain-source-status.json')
    assert all(name.startswith('docs/') for name in source_status['changed'])
    assert all(name.startswith('docs/') for name in source_status['added'])
    assert source_status['main_changed_paths'] == ['README.md']
    for name, digests in source_status['files'].items():
        assert (digests['current'] != digests['frozen']) == (name in source_status['changed'])
    cases = read(root/'cases.json')
    assert [c['name'] for c in cases] == [
        '20202-round-collapse-0-candidate-p2',
        '10101-angular-collapse-0-candidate-p1',
    ]
    results = []
    artifacts = samples = decisions = 0
    for case in cases:
        here = root/'cases'/case['name']
        receipt = read(here/'result.json')
        assert receipt['exit_code'] == 0 and receipt['source_commit'] == runtime['source_commit']
        assert receipt['binary_sha256'] == runtime['binary_sha256']
        assert receipt['env']['SPACEWARS_RECOVERY_PROFILE'] == 'terrain_recovery_supported_hatch_v1'
        for name, digest in read(here/'manifest.json').items():
            assert sha(here/name) == digest
            artifacts += 1
        out = here/'output'
        reference = root/'references'/case['name']
        summary = read(out/'summary.json')
        prior = read(reference/'summary.json')
        assert summary['profile'] == 'terrain_recovery_supported_hatch_v1'
        assert summary['case'] == case['case'] == prior['case']
        assert not any(summary[k] for k in ('controller_reset', 'input_override', 'physics_edited'))
        assert summary['initial_controllers_match'] and summary['reference_final_matches']
        for name in ('initial.json', 'final.json'):
            assert (out/name).read_bytes() == (reference/name).read_bytes()
        assert (out/'initial-controllers.txt').read_bytes() == (reference/'initial-controllers.txt').read_bytes()
        actions = list(rows(out/'actions.jsonl'))
        assert actions == list(rows(reference/'actions.jsonl'))
        assert [r['tick'] for r in actions] == list(range(summary['end_tick']))
        assert summary['reference_decisions_verified'] == len(actions)
        assert summary['match']['outcome'] is not None
        assert summary['metrics'] == prior['metrics'] and summary['brains'] == prior['brains']
        decisions += len(actions)
        for sample in rows(out/'samples.jsonl'):
            audit = sample['state']['audit']
            assert not audit['issues'] and audit['max_speed'] < 500
            assert audit['occupied_cells'] + audit['loose_cells'] + audit['removed_cells'] == summary['initial_cells']
            samples += 1
        for seat, metrics in enumerate(summary['metrics']):
            for tick in metrics['completed']:
                state = read(out/f'completion-{tick}-p{seat+1}.json')
                pilot = state['pilots'][seat]['recovery']['flight']['pilot']
                assert pilot['ship_available'] and pilot['ship_form'] == 'ship'
                assert pilot['location'] == {'aboard': pilot['vehicle']}
                assert state['brains'][seat]['recovery'] is None
                assert not actions[tick]['recovery_active'][seat]
                assert not actions[tick]['native_refinement'][seat]
        results.append({'name': case['name'], 'case': case['case'], 'profile': summary['profile'],
                        'end_tick': summary['end_tick'], 'verified_decisions': len(actions),
                        'completed': [m['completed'] for m in summary['metrics']],
                        'blocked_ticks': [m['recovery_blocked_ticks'] for m in summary['metrics']],
                        'match': summary['match']})
    report = {'schema': 1, 'passed': True, 'source_inputs_checked': len(checked),
              'reference_files_checked': len(provenance['files']), 'artifacts_checked': artifacts,
              'decisions_verified': decisions, 'material_samples': samples,
              'initial_controllers_equal': True, 'all_action_rows_equal': True,
              'final_snapshots_equal': True, 'retained_pre_decision_failures': 2,
              'production_unchanged_by_harness_correction': True, 'cases': results}
    if args.output:
        args.output.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
