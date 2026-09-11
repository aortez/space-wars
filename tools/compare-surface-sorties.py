#!/usr/bin/env python3
"""Paired three-minute landing trials using the built surface_combat_soak example."""
import argparse
import itertools
import json
import pathlib
import shlex
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', required=True, help='Path to surface_combat_soak on the execution host')
parser.add_argument('--out', type=pathlib.Path, required=True)
parser.add_argument('--policy', choices=['basic', 'tactical', 'both'], default='both')
parser.add_argument('--ssh', help='Optional SSH destination; the runner must already be installed there')
parser.add_argument('--ssh-option', action='append', default=[], help='Additional SSH -o option; repeat as needed')
parser.add_argument('--continue-after-failure', action='store_true', help='Continue each diagnostic run after an alarm, retaining failure status')
parser.add_argument('--remote-out', default='/tmp/spacewars-sortie-matrix')
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
ssh = ['ssh', '-o', 'BatchMode=yes']
for option in args.ssh_option:
    ssh.extend(['-o', option])
ssh.append(args.ssh or '')
policies = ['basic', 'tactical'] if args.policy == 'both' else [args.policy]
rows = []
initial_states = {}
for policy in policies:
    for seed, mirror, seat, health, fire in itertools.product([7, 42], [False, True], [0, 1], [100, 50], [False, True]):
        case = f'seed{seed}-mirror{int(mirror)}-seat{seat}-health{health}-fire{int(fire)}'
        out = args.out / policy / case
        out.mkdir(parents=True, exist_ok=True)
        destination = f'{args.remote_out}/{policy}/{case}' if args.ssh else str(out.resolve())
        command = [args.binary, '--seed', str(seed), '--mirror', str(mirror).lower(),
                   '--subject-seat', str(seat), '--subject-health', str(health),
                   '--opponent-fire', str(fire).lower(), '--land-after', '0',
                   '--break-interval', '8', '--break-seconds', '4', '--seconds', '180',
                   '--landing-policy', policy, '--out', destination]
        if args.continue_after_failure:
            command.extend(['--continue-after-failure', 'true'])
        with (out / 'runner.log').open('w') as log:
            result = subprocess.run(ssh + [shlex.join(command)] if args.ssh else command,
                                    stdout=log, stderr=subprocess.STDOUT)
        if args.ssh:
            report = subprocess.check_output(ssh + [shlex.join(['cat', destination + '/report.json'])])
            (out / 'report.json').write_bytes(report)
        report = json.loads((out / 'report.json').read_text())
        assert report['version'] == 4 and report['landing_policy'] == policy
        ok = result.returncode == 0 and report['failure'] is None and report['completed_seconds'] == 180
        assert report['opponent_fire'] == fire and report['subject_seat'] == seat
        assert report['subject_health'] == health
        key = seed, mirror, seat, health
        initial = report['initial_state']
        assert initial_states.setdefault(key, initial) == initial, 'Paired initial conditions differ'
        if not fire:
            weapons = report['weapons'][1 - seat]
            assert weapons['shells_fired'] == 0 and weapons['laser_hit_ticks'] == 0
        sortie = report['landing_under_fire']
        telemetry = sortie['telemetry']
        landing = telemetry.get('landing', telemetry)
        completed = telemetry['completed_tick']
        lost = sortie['lost_tick']
        assert completed is None or lost is None or completed < lost
        row = dict(policy=policy, name=case, seed=seed, mirror=mirror, seat=seat,
                   audit_passed=ok, completed_seconds=report['completed_seconds'], failure=report['failure'],
                   health=health, fire=fire, landed=landing['landed_tick'],
                   exited=sortie['exited_tick'], claimed=landing['claimed_tick'],
                   boarded=landing['boarded_tick'], completed=completed, lost=lost,
                   step_p95_ms=report['step_p95_ms'], ai_p95_ms=report['ai_p95_ms'])
        rows.append(row)
        (args.out / 'summary.json').write_text(json.dumps(rows, indent=2) + '\n')
        print(json.dumps(row), flush=True)

raise SystemExit(0 if all(row['audit_passed'] for row in rows) else 1)
