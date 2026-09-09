#!/usr/bin/env python3
"""Twenty controlled three-minute flag/return trials, locally or over SSH."""
import argparse
import json
import pathlib
import shlex
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', required=True, help='Built surface_flag_soak on the execution host')
parser.add_argument('--out', type=pathlib.Path, required=True)
parser.add_argument('--ssh', help='Optional SSH destination; install the binary there first')
parser.add_argument('--ssh-option', action='append', default=[])
parser.add_argument('--ssh-config', help='Optional SSH configuration file (for example /dev/null)')
parser.add_argument('--remote-out', default='/tmp/ground-navigation-trials')
parser.add_argument('--jetpacks', action='store_true', help='Equip both pilots with the shared jetpack')
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
ssh = ['ssh', '-o', 'BatchMode=yes']
if args.ssh_config:
    ssh.extend(['-F', args.ssh_config])
for option in args.ssh_option:
    ssh.extend(['-o', option])
ssh.append(args.ssh or '')

cases = [(42, seat, mode, edit) for mode, edit in [
    ('capture', 'none'), ('recovery', 'none'), ('navigation', 'crater'),
    ('navigation', 'flag'), ('navigation', 'blocked'), ('pod', 'none'),
] for seat in [0, 1]]
cases += [(42, seat, mode, 'rebuild') for mode in ['recovery', 'pod'] for seat in [0, 1]]
cases += [(7, seat, mode, 'none') for mode in ['capture', 'recovery'] for seat in [0, 1]]
rows = []
for seed, seat, mode, edit in cases:
    name = f'seed{seed}-seat{seat}-{mode}-{edit}'
    out = args.out / name
    out.mkdir(parents=True, exist_ok=True)
    destination = f'{args.remote_out}/{name}' if args.ssh else str(out.resolve())
    expected = 'blocked' if edit == 'blocked' else 'complete'
    command = [args.binary, '--seed', str(seed), '--seat', str(seat), '--mode', mode,
               '--edit', edit, '--expect', expected, '--out', destination]
    if args.jetpacks:
        command.extend(['--jetpacks', 'true'])
    with (out / 'runner.log').open('w') as log:
        result = subprocess.run(ssh + [shlex.join(command)] if args.ssh else command,
                                stdout=log, stderr=subprocess.STDOUT)
    try:
        if args.ssh:
            report = subprocess.check_output(ssh + [shlex.join(['cat', destination + '/report.json'])])
            (out / 'report.json').write_bytes(report)
        report = json.loads((out / 'report.json').read_text())
        identity = (report['version'], report['seed'], report['seat'], report['mode'],
                    report['edit'], report['expected'], report['seconds'])
        assert identity == (1, seed, seat, mode, edit, expected, 180)
        assert report.get('jetpacks', False) == args.jetpacks
        row = dict(name=name, expected=expected, accepted=result.returncode == 0,
                   complete=report['complete'], captured=report['captured'],
                   blocked=report['blocked'], audit_passed=report['audit_passed'],
                   ground_failure=(report.get('ground') or {}).get('reason'),
                   relocations=((report.get('recovery') or {}).get('recovery') or {}).get('relocations', 0),
                   claimed_tick=report['claimed_tick'], departed_tick=report['departed_tick'],
                   sensor_p95_ms=report['sensor_p95_ms'],
                   ground_refresh_p95_ms=report['ground_refresh_p95_ms'],
                   ground_refresh_max_ms=report['ground_refresh_max_ms'],
                   rebuild_refresh_p95_ms=report['rebuild_refresh_p95_ms'],
                   rebuild_refresh_max_ms=report['rebuild_refresh_max_ms'],
                   step_p95_ms=report['step_p95_ms'], step_max_ms=report['step_max_ms'])
        row['jetpack_crossings'] = len({c['completed_tick'] for e in report['events']
                                      if (c := (e.get('ground') or {}).get('crossing')) and c.get('completed_tick') is not None})
        row['flight_interruptions'] = max((e.get('ground') or {}).get('flight_interruptions', 0) for e in report['events'])
    except (OSError, ValueError, KeyError, AssertionError, subprocess.CalledProcessError) as error:
        row = dict(name=name, expected=expected, accepted=False, error=str(error), exit_code=result.returncode)
    rows.append(row)
    (args.out / 'summary.json').write_text(json.dumps(rows, indent=2) + '\n')
    print(json.dumps(row), flush=True)

raise SystemExit(0 if all(row['accepted'] for row in rows) else 1)
