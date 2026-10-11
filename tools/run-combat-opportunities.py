#!/usr/bin/env python3
"""Retain reproducible #155 match traces using the production Value/Recovery host."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import itertools
import json
import os
from pathlib import Path
import shutil
import subprocess
import time


TEST = ('mission_pilot::terrain_recovery::qualification::combat_diagnosis::'
        'combat_opportunities_match')


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path, help='new evidence directory')
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--seeds', nargs='+', type=int, required=True)
    parser.add_argument('--shapes', nargs='+', choices=['round', 'angular'],
                        default=['round', 'angular'])
    parser.add_argument('--intervals', nargs='+', type=int, choices=[0, 3, 8], default=[0, 8])
    parser.add_argument('--seats', nargs='+', type=int, choices=[0, 1], default=[0, 1])
    parser.add_argument('--probe-laser', action='store_true')
    parser.add_argument('--direct-laser-window', action='store_true')
    parser.add_argument('--jobs', type=int, choices=[1, 2, 3, 4], default=2)
    args = parser.parse_args()
    root = args.root.resolve()
    root.mkdir(parents=True, exist_ok=False)
    binary = root / 'ai-tests'
    shutil.copy2(args.binary, binary)
    binary_hash = digest(binary)
    cases = [{'seed': seed, 'shape': shape, 'slumping': True,
              'candidate_seat': seat, 'asteroid_interval_seconds': interval,
              'probe_laser': args.probe_laser,
              'direct_laser_window': args.direct_laser_window}
             for seed, shape, interval, seat in itertools.product(
                 args.seeds, args.shapes, args.intervals, args.seats)]
    write(root / 'matrix.json', cases)
    patch = subprocess.check_output(['git', 'diff', 'HEAD', '--binary'])
    (root / 'tracked-source.patch').write_bytes(patch)
    # Retain new diagnostics too; the git diff alone omits untracked files.
    added = subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard'],
                                    text=True).splitlines()
    added = [name for name in added if not Path(name).resolve().is_relative_to(root)]
    for name in added:
        dest = root / 'untracked-source' / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(name, dest)
    write(root / 'runtime.json', {
        'source_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
        'rustc': subprocess.check_output(['rustc', '+1.89.0', '--version'], text=True).strip(),
        'binary_sha256': binary_hash, 'runner_sha256': digest(Path(__file__)),
        'matrix_sha256': digest(root / 'matrix.json'),
        'patch_sha256': digest(root / 'tracked-source.patch'),
        'untracked_source_sha256': {p: digest(Path(p)) for p in added},
        'policy': 'material_mission_v13',
        'recovery_profile': 'terrain_recovery_supported_hatch_ordinary_offsets_v1',
        'breaks': {'interval_seconds': 15, 'duration_seconds': 4},
        'time_limit_seconds': 600,
    })

    def run(item):
        index, case = item
        here = root / f'{index:02}-{case["shape"]}-{case["asteroid_interval_seconds"]}s-p{case["candidate_seat"]+1}'
        here.mkdir()
        spec = here / 'case.json'
        write(spec, case)
        command = [str(binary), TEST, '--ignored', '--exact', '--nocapture']
        env = {'RUST_MIN_STACK': '16777216', 'SPACEWARS_COMBAT_CASE': str(spec),
               'SPACEWARS_COMBAT_OUT': str(here / 'output')}
        result = {'index': index, 'case': case, 'command': command, 'env': env,
                  'started_unix': time.time()}
        write(here / 'command.json', result)
        with (here / 'run.log').open('w') as log:
            completed = subprocess.run(command, env={**os.environ, **env},
                                       stdout=log, stderr=subprocess.STDOUT)
        result.update(exit_code=completed.returncode, seconds=time.time()-result['started_unix'])
        summary = here / 'output/summary.json'
        if summary.exists():
            report = json.loads(summary.read_text())
            result.update(end_tick=report['end_tick'], match=report['match'])
        elif completed.returncode == 0:
            result.update(exit_code=1, error='test returned success without a match summary')
        write(here / 'result.json', result)
        print(json.dumps(result), flush=True)
        return result

    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        results = list(pool.map(run, enumerate(cases)))
    write(root / 'results.json', results)
    raise SystemExit(int(any(r['exit_code'] for r in results)))


if __name__ == '__main__':
    main()
