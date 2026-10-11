#!/usr/bin/env python3
"""Prepare or execute the frozen named-recovery matrix, retaining every exit."""
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import subprocess
import time


NAMESPACE = 'terrain-recovery-main-qualification-v1'
PROFILE = 'terrain_recovery_supported_hatch_v1'
TEST = 'mission_pilot::terrain_recovery::qualification::heldout::named_recovery_heldout_match'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def matrix():
    seeds = [int.from_bytes(hashlib.sha256(f'{NAMESPACE}:{i}'.encode()).digest()[:8],
                            'little') for i in range(2)]
    cases = []
    for seed, shape, collapse, interval in itertools.product(
            seeds, ('round', 'angular'), (False, True), (0, 8)):
        setting = f'{seed}-{shape}-collapse-{int(collapse)}-asteroids-{interval}'
        for seat in (None, 0, 1):
            arm = 'control' if seat is None else f'candidate-p{seat+1}'
            cases.append({'name': f'{setting}-{arm}', 'setting': setting, 'arm': arm,
                          'case': {'seed': seed, 'shape': shape, 'slumping': collapse,
                                   'candidate_seat': seat, 'asteroid_interval_seconds': interval}})
    assert len(cases) == 48
    return cases


def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('--prepare', action='store_true')
    args = parser.parse_args()
    root = args.root.resolve()
    cases = matrix()
    if args.prepare:
        with (root / 'matrix.json').open('x') as f:
            f.write(json.dumps(cases, indent=2) + '\n')
        print(json.dumps({'cases': len(cases), 'seeds': sorted({c['case']['seed'] for c in cases}),
                          'matrix_sha256': digest(root / 'matrix.json')}))
        return
    assert json.loads((root / 'matrix.json').read_text()) == cases
    runtime = json.loads((root / 'runtime.json').read_text())
    binary = root / 'recovery-heldout'
    assert digest(binary) == runtime['binary_sha256']
    assert digest(root / 'matrix.json') == runtime['matrix_sha256']
    assert digest(Path(__file__)) == runtime['runner_sha256']
    (root / 'cases').mkdir()
    results = []
    for index, item in enumerate(cases):
        here = root / 'cases' / item['name']
        here.mkdir()
        spec = here / 'case.json'
        write(spec, item['case'])
        command = [str(binary), TEST, '--ignored', '--exact', '--nocapture']
        env = {'RUST_MIN_STACK': '16777216', 'SPACEWARS_MISSION_CASE': str(spec),
               'SPACEWARS_RECOVERY_OUT': str(here / 'output')}
        result = {'name': item['name'], 'source_commit': runtime['source_commit'],
                  'binary_sha256': runtime['binary_sha256'], 'command': command, 'env': env,
                  'started_unix': time.time()}
        write(here / 'command.json', result)
        with (here / 'run.log').open('w') as log:
            try:
                completed = subprocess.run(command, env={**os.environ, **env}, stdout=log,
                                           stderr=subprocess.STDOUT, timeout=600)
                code = completed.returncode
            except subprocess.TimeoutExpired:
                code = 124
                log.write('\nExecution watchdog expired; retain partial evidence.\n')
        result.update(exit_code=code, seconds=time.time() - result['started_unix'])
        write(here / 'result.json', result)
        results.append(result)
        write(root / 'results.json', results)
        report = {'index': index + 1, 'of': len(cases), 'name': item['name'],
                  'exit_code': code, 'seconds': round(result['seconds'], 3)}
        summary = here / 'output/summary.json'
        if summary.exists():
            s = json.loads(summary.read_text())
            report.update(ticks=s['end_tick'], outcome=s['match']['outcome'],
                          recoveries=[b['completed_recoveries'] for b in s['brains']],
                          blocked_ticks=[m['recovery_blocked_ticks'] for m in s['metrics']])
        print(json.dumps(report), flush=True)
    raise SystemExit(int(any(r['exit_code'] for r in results)))


if __name__ == '__main__':
    main()
