#!/usr/bin/env python3
"""Three-minute pod recovery and flag approach regressions, locally or over SSH."""
import argparse
import json
import pathlib
import shlex
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', required=True)
parser.add_argument('--out', type=pathlib.Path, required=True)
parser.add_argument('--ssh')
parser.add_argument('--ssh-config')
parser.add_argument('--ssh-option', action='append', default=[])
parser.add_argument('--remote-out', default='/tmp/jetpack-recovery-trials')
parser.add_argument('--include-preflight', action='store_true',
                    help='Also retain three known seed-7 pod stabilization failures before exit')
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
ssh = ['ssh', '-o', 'BatchMode=yes']
if args.ssh_config:
    ssh.extend(['-F', args.ssh_config])
for option in args.ssh_option:
    ssh.extend(['-o', option])
ssh.append(args.ssh or '')

# Both pod starts previously exceeded the ground deadline by walking the long
# way around the planet. Every pod case must now complete a measured flight.
cases = [(seed, seat, 'pod', offset, edit, 1)
         for seed in [7, 42] for seat, offset in [(0, '-0.5'), (1, '-0.2')]
         if args.include_preflight or (seed, seat) != (7, 0)
         for edit in ['none', 'crater', 'flight-crater']]
# These layouts have reachable standing centers inside the actual flag range,
# but no reachable node inside the old, unnecessarily tight foot-radius test.
cases += [(42, 0, 'capture', offset, 'none', 0) for offset in ['-0.4', '-0.5', '-0.6']]
# The historical two-cut fixture is now traversable from this seat by flight.
cases += [(42, 1, 'navigation', '0.6', 'blocked', 1)]
rows = []
for seed, seat, mode, offset, edit, minimum_flights in cases:
    name = f'seed{seed}-seat{seat}-{mode}-offset{offset}-{edit}'
    out = args.out / name
    out.mkdir(exist_ok=True)
    destination = f'{args.remote_out}/{name}' if args.ssh else str(out.resolve())
    command = [args.binary, '--seed', str(seed), '--seat', str(seat), '--mode', mode,
               '--offset', offset, '--edit', edit, '--expect', 'complete',
               '--jetpacks', 'true', '--out', destination]
    with (out / 'runner.log').open('w') as log:
        result = subprocess.run(ssh + [shlex.join(command)] if args.ssh else command,
                                stdout=log, stderr=subprocess.STDOUT)
    try:
        if args.ssh:
            (out / 'report.json').write_bytes(subprocess.check_output(
                ssh + [shlex.join(['cat', destination + '/report.json'])]))
        report = json.loads((out / 'report.json').read_text())
        assert (report['version'], report['seed'], report['seat'], report['mode'],
                report['edit'], report['expected'], report['seconds'], report['jetpacks']) == (
                    1, seed, seat, mode, edit, 'complete', 180, True)
        flights = [f for e in report['events']
                   if (f := (e.get('ground') or {}).get('crossing'))]
        landed = {f['completed_tick']: f for f in flights if f['completed_tick'] is not None}
        row = dict(name=name, exit_code=result.returncode, complete=report['complete'],
                   audit_passed=report['audit_passed'], crossings=len(landed),
                   flight_anchors=[f['plan']['anchor'] for f in landed.values()],
                   minimum_landing_charge=min((f['lowest_charge'] for f in landed.values()), default=None),
                   flight_interruptions=max((e.get('ground') or {}).get('flight_interruptions', 0)
                                            for e in report['events']),
                   ground_failure=(report.get('ground') or {}).get('reason'),
                   strike_tick=report['strike_tick'], exited_tick=report['exited_tick'],
                   claimed_tick=report['claimed_tick'], departed_tick=report['departed_tick'],
                   ground_refresh_max_ms=report['ground_refresh_max_ms'], step_max_ms=report['step_max_ms'])
        row['accepted'] = (result.returncode == 0 and row['complete']
                           and row['audit_passed'] and len(landed) >= minimum_flights)
    except (OSError, ValueError, KeyError, AssertionError, subprocess.CalledProcessError) as error:
        row = dict(name=name, accepted=False, exit_code=result.returncode, error=str(error))
    rows.append(row)
    (args.out / 'summary.json').write_text(json.dumps(rows, indent=2) + '\n')
    print(json.dumps(row), flush=True)
raise SystemExit(0 if all(row['accepted'] for row in rows) else 1)
