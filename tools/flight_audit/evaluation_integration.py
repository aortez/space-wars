"""Verify the routine evaluation CLI against frozen diagnostic trajectories."""
import argparse
import gzip
import json
from pathlib import Path
import subprocess
import sys
import time


def read(path):
    opener = gzip.open if path.suffix == '.gz' else open
    with opener(path, 'rt') as stream:
        return json.load(stream)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--seed', type=int, choices=[17, 29, 43], required=True)
    args = parser.parse_args()
    archive, seed = args.archive.resolve(), args.seed
    inputs = archive/f'inputs/seed-{seed}'
    out = archive/f'seed-{seed}'
    out.mkdir()
    summaries = {}
    start = time.monotonic()
    for mode, extra in [
        ('legacy', []),
        ('mixed', ['--mode', 'mean-steering-sampled-buttons-v1', '--repeats', '4',
                   '--seed-base', '5310000', '--learner-seed', str(seed)]),
    ]:
        command = [sys.executable, '-m', 'tools.flight_ppo.evaluate',
                   '--bridge', str(archive/'runtime/flight_ppo_bridge'),
                   '--policy', str(inputs/'policy-0128.json'),
                   '--admission', str(inputs/'development-admission.json'),
                   '--out', str(out/mode), '--wall-seconds', '1200'] + extra
        (out/f'{mode}-command.json').write_text(json.dumps(command, indent=2)+'\n')
        result = subprocess.run(command, capture_output=True, text=True, timeout=1260)
        (out/f'{mode}.log').write_text(result.stdout+result.stderr)
        if result.returncode:
            raise RuntimeError(f'{mode} evaluation failed: {result.stderr}')
        rows = read(out/mode/'results.json')
        assert len(rows) == (64 if mode == 'mixed' else 16)
        legacy_rows = read(inputs/'legacy/results.json') if mode == 'legacy' else None
        for row in rows:
            name = f"clear-{row['index']}"
            if mode == 'legacy':
                assert row == next(r for r in legacy_rows if r['index'] == row['index'])
                assert read(out/mode/f'{name}.json') == read(inputs/'legacy'/f'{name}.json')
            else:
                directory = out/mode/f"repeat-{row['repeat']}"
                stem = f"mean_steering-r{row['repeat']}-{name}.json.gz"
                assert read(directory/f'{name}.json') == read(inputs/'mixed/traces'/stem)
                assert read(directory/'policy'/f'{name}.json.gz') == read(inputs/'mixed/policy'/stem)
        summaries[mode] = {**read(out/mode/'metadata/summary.json'),
                           'physics_ticks': sum(row['ticks'] for row in rows),
                           'all_physics_traces_exact': True,
                           'all_policy_rows_exact': mode == 'mixed'}
    summary = {'seed': seed, 'seconds': time.monotonic()-start, 'modes': summaries}
    (out/'verification.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(json.dumps(summary), flush=True)


if __name__ == '__main__':
    main()
