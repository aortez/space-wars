#!/usr/bin/env python3
"""Fixed compatibility replays for the mission-execution checkpoint; no tuning."""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import traceback


def digest(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def plan():
    common = ['--seconds', '600', '--world', 'generated', '--seed', '186767996776005237',
              '--match', 'true', '--mode', 'duel', '--seat', '0', '--asteroid-interval', '0',
              '--require-finish', 'true', '--trace-destination-behavior', 'true']
    cases = [dict(name='legacy-v9-v10', arguments=common + [
        '--p1-policy', 'material_mission_v9', '--p2-policy', 'material_mission_v10'])]
    cases.append(dict(name='survey-v14-jetpack-v11', arguments=common + [
        '--p1-policy', 'material_mission_v14', '--p2-policy', 'material_mission_v11',
        '--live-objective-planning', 'true', '--live-objective-seats', 'both',
        '--objective-graph-budget', '4', '--objective-query-budget', '384',
        '--evaluate-missions', 'true', '--survey-capture-alternative', 'true',
        '--survey-capture-flags', 'true']))
    source = json.loads(Path('docs/data/approach-aware-surveys-v1.json').read_text())
    for version in [15, 16]:
        name = f'destination-p1-bearing0.8-v{version}'
        command = source['runs'][name]['command'][1:]
        out = command.index('--out')
        del command[out:out + 2]
        cases.append(dict(name=name, arguments=command))
    return cases


def run(name, command, out):
    print('Starting', name, flush=True)
    record = dict(command=command)
    try:
        with (out / f'{name}.log').open('x') as log:
            subprocess.run(command, check=True, stdout=log, stderr=log, timeout=1800)
        root = Path(command[command.index('--out') + 1])
        report = json.loads((root / 'report.json').read_text())
        assert report['physics_ok'] and not report['audit_failures']
        record['physical'] = {key: report[key] for key in [
            'elapsed_ticks', 'round', 'termination', 'final_audit', 'final_combat',
            'final_pilots', 'final_planets', 'asteroid_events', 'pilot_damage_events']}
        record['hashes'] = {p.name: digest(p) for p in sorted(root.iterdir()) if p.is_file()}
    except Exception:
        record['error'] = traceback.format_exc()
    print('Finished', name, 'FAIL' if 'error' in record else 'OK', flush=True)
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ['baseline', 'candidate', 'out']:
        parser.add_argument('--' + flag, type=Path, required=True)
    parser.add_argument('--baseline-commit', required=True)
    args = parser.parse_args()
    assert not subprocess.check_output(['git', 'status', '--porcelain'], text=True).strip()
    args.out.mkdir(parents=True, exist_ok=False)
    binaries = {key: dict(path=str(getattr(args, key).resolve()), sha256=digest(getattr(args, key)))
                for key in ['baseline', 'candidate']}
    manifest_path = Path('docs/data/projectile-selection-v1.json')
    manifest = json.loads(manifest_path.read_text())
    archive = Path(manifest['archive']['path'])
    assert digest(archive) == manifest['archive']['sha256']
    with gzip.open(archive, 'rt') as stream:
        frozen = json.load(stream)['runner_summary']
    key = 'shared-armed-world1-p1-powered-guarded_brake'
    retained = frozen['runs'][key]
    assert retained['response']['selection']['action'] == 'brake'
    cases = plan()
    jobs = []
    for case in cases:
        for variant in ['baseline', 'candidate']:
            name = case['name'] + '-' + variant
            command = [binaries[variant]['path'], *case['arguments'], '--out', str(args.out / name)]
            jobs.append((name, command))
    replay = list(retained['command'])
    replay[0] = binaries['candidate']['path']
    replay[replay.index('--out') + 1] = str(args.out / 'retained-selector')
    jobs.append(('retained-selector', replay))
    summary = dict(schema=1, complete=False,
        candidate_commit=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        baseline_commit=args.baseline_commit, binaries=binaries,
        inputs={str(p): digest(p) for p in [manifest_path, archive,
            Path('docs/data/approach-aware-surveys-v1.json'), Path(__file__)]},
        plan=[dict(name=name, command=command) for name, command in jobs], runs={}, comparisons={})
    destination = args.out / 'summary.json'

    def save():
        destination.write_text(json.dumps(summary, indent=2, allow_nan=False) + '\n')

    save()
    with ThreadPoolExecutor(max_workers=2) as executor:
        futures = {executor.submit(run, name, command, args.out): name for name, command in jobs}
        for future in as_completed(futures):
            summary['runs'][futures[future]] = future.result()
            save()
    try:
        assert all('error' not in r for r in summary['runs'].values())
        for case in cases:
            a = summary['runs'][case['name'] + '-baseline']
            b = summary['runs'][case['name'] + '-candidate']
            files = ['destination-behavior.jsonl', 'landing-handoffs.jsonl']
            if '--evaluate-missions' in case['arguments']:
                files += ['mission-evaluations.jsonl', 'destination-cover.jsonl', 'flag-survey.jsonl']
            if '--trace-neutral-approaches' in case['arguments']:
                files += ['neutral-approach-inputs.jsonl']
            assert a['physical'] == b['physical'], case['name'] + ': physical result differs'
            for filename in files:
                assert a['hashes'][filename] == b['hashes'][filename], (case['name'], filename)
            summary['comparisons'][case['name']] = dict(physical_equal=True, identical_files=files)
        r = summary['runs']['retained-selector']
        files = ['capture-evidence.jsonl', 'projectiles.jsonl', 'projectile-response.jsonl',
                 'mission-evaluations.jsonl', 'destination-cover.jsonl', 'flag-survey.jsonl']
        for filename in files:
            assert r['hashes'][filename] == retained['hashes'][filename], filename
        assert r['physical']['round'] == retained['round']
        summary['comparisons']['retained-selector'] = dict(
            source_commit=frozen['source_commit'], source_case=key, identical_files=files,
            round_equal=True, selection=retained['response']['selection'])
        summary['complete'] = True
    except Exception:
        summary['comparison_error'] = traceback.format_exc()
    save()
    print('Compatibility replays:', 'PASS' if summary['complete'] else 'FAIL', flush=True)
    raise SystemExit(0 if summary['complete'] else 1)


if __name__ == '__main__':
    main()
