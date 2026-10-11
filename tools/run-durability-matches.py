#!/usr/bin/env python3
"""Run fixed Value v13 / Recovery v1 pairs on ordinary and heavily damaged terrain.

Use the same matrix with separately retained baseline and tuned test binaries.
No bot policy, input override, controller reset or physics edit is enabled.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import itertools
import json
import os
from pathlib import Path
import subprocess
import time


PROFILE = 'terrain_recovery_supported_hatch_ordinary_offsets_v1'
TEST = 'mission_pilot::terrain_recovery::qualification::heldout::ordinary_offsets_recovery_match'


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def matrix():
    # Freeze seeds before inspecting outcomes. Both builds use every case.
    seeds = [int.from_bytes(hashlib.sha256(f'durability-154-v1:{i}'.encode()).digest()[:8],
                            'little') for i in range(2)]
    return [{'seed': seed, 'shape': shape, 'slumping': True,
             'candidate_seat': seat, 'asteroid_interval_seconds': interval}
            for seed, shape, interval, seat in itertools.product(
                seeds, ('round', 'angular'), (8, 3), (0, 1))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path, help='new evidence directory')
    parser.add_argument('--binary', type=Path, required=True, help='retained AI test executable')
    parser.add_argument('--jobs', type=int, default=2)
    args = parser.parse_args()
    assert 1 <= args.jobs <= 4
    root, binary = args.root.resolve(), args.binary.resolve()
    binary_hash = digest(binary)
    root.mkdir(parents=True, exist_ok=False)
    cases = matrix()
    write(root / 'matrix.json', cases)
    write(root / 'runtime.json', {'binary': str(binary), 'binary_sha256': binary_hash,
                                'runner_sha256': digest(Path(__file__)),
                                'matrix_sha256': digest(root / 'matrix.json'),
                                'recovery_profile': PROFILE, 'policy': 'material_mission_v13',
                                'breaks': {'interval_seconds': 15, 'duration_seconds': 4},
                                'ship_health_percent': 100, 'pilot_health': 100})

    def run(item):
        index, case = item
        here = root / f'{index:02}-{case["shape"]}-{case["asteroid_interval_seconds"]}s-p{case["candidate_seat"]+1}'
        here.mkdir()
        spec = here / 'case.json'
        write(spec, case)
        command = [str(binary), TEST, '--ignored', '--exact', '--nocapture']
        env = {'RUST_MIN_STACK': '16777216', 'SPACEWARS_MISSION_CASE': str(spec),
               'SPACEWARS_RECOVERY_OUT': str(here / 'output'),
               'SPACEWARS_RECOVERY_PROFILE': PROFILE}
        result = {'index': index, 'case': case, 'command': command, 'env': env,
                  'binary_sha256': binary_hash, 'started_unix': time.time()}
        write(here / 'command.json', result)
        with (here / 'run.log').open('w') as log:
            completed = subprocess.run(command, env={**os.environ, **env},
                                       stdout=log, stderr=subprocess.STDOUT)
        result.update(exit_code=completed.returncode,
                      seconds=time.time() - result['started_unix'])
        summary = here / 'output/summary.json'
        if summary.exists():
            report = json.loads(summary.read_text())
            result.update(end_tick=report['end_tick'], match=report['match'],
                          recovery=report['native_recovery'],
                          completed_recoveries=[b['completed_recoveries'] for b in report['brains']])
        write(here / 'result.json', result)
        print(json.dumps({'index': index, 'case': case, 'exit_code': completed.returncode,
                          'seconds': round(result['seconds'], 2),
                          'end_tick': result.get('end_tick'),
                          'outcome': result.get('match', {}).get('outcome'),
                          'recoveries': result.get('completed_recoveries')}), flush=True)
        return result

    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        results = list(pool.map(run, enumerate(cases)))
    write(root / 'results.json', results)
    raise SystemExit(int(any(r['exit_code'] for r in results)))


if __name__ == '__main__':
    main()
