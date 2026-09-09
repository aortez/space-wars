#!/usr/bin/env python3
"""Seeded three-minute combat/capture trials; report gameplay separately from audits."""
import argparse
import itertools
import json
import pathlib
import shlex
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', required=True)
parser.add_argument('--out', type=pathlib.Path, required=True)
parser.add_argument('--ssh')
parser.add_argument('--ssh-option', action='append', default=[])
parser.add_argument('--remote-out', default='/tmp/asteroid-pressure-trials')
parser.add_argument('--quick', action='store_true', help='One layout at each of the six pressure settings')
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
ssh = ['ssh', '-F', '/dev/null', '-o', 'BatchMode=yes']
for option in args.ssh_option:
    ssh.extend(['-o', option])
ssh.append(args.ssh or '')
layouts = [(42, 0, False)] if args.quick else itertools.product([7, 42], range(2), [False, True])
# Frequency changes retain Mixed strength; strength comparisons retain 3s mean.
cases = [(seed, seat, mirror, interval, severity)
         for seed, seat, mirror in layouts
         for interval, severity in [(0, 'mixed'), (8, 'mixed'), (3, 'mixed'), (1, 'mixed'), (3, 'light'), (3, 'heavy')]]
rows = []
for seed, seat, mirror, interval, severity in cases:
    name = f'seed{seed}-seat{seat}-mirror{int(mirror)}-interval{interval}-{severity}'
    out = args.out / name
    out.mkdir(exist_ok=True)
    destination = f'{args.remote_out}/{name}' if args.ssh else str(out.resolve())
    command = [args.binary, '--seed', str(seed), '--subject-seat', str(seat),
               '--mirror', str(mirror).lower(), '--seconds', '180', '--landing-policy', 'tactical',
               '--land-after', '0', '--subject-health', '100', '--opponent-fire', 'true',
               '--break-interval', '8', '--break-seconds', '4', '--continue-after-failure', 'true',
               '--asteroid-interval', str(interval), '--asteroid-severity', severity, '--out', destination]
    try:
        with (out / 'runner.log').open('w') as log:
            run = subprocess.run(ssh + [shlex.join(command)] if args.ssh else command,
                                 stdout=log, stderr=subprocess.STDOUT, timeout=180)
        if args.ssh:
            (out / 'report.json').write_bytes(subprocess.check_output(
                ssh + [shlex.join(['cat', destination + '/report.json'])], timeout=15))
        r = json.loads((out / 'report.json').read_text())
        assert r['version'] == 5 and r['completed_seconds'] == 180 and len(r['samples']) == 180
        assert r['asteroids']['settings'] == dict(interval_seconds=interval, severity=severity)
        mission = r['landing_under_fire']['telemetry']
        arrivals = [a for e in r['asteroid_events'] for a in e['arrivals']]
        impacts = [i for e in r['asteroid_events'] for i in e['impacts']]
        assert len(arrivals) == r['asteroids']['spawned']
        assert len(impacts) == r['asteroids']['contacts']
        spawned = {a['id']: a['tick'] for a in arrivals}
        assert all(spawned[i['id']] == i['spawn_tick'] and i['tick'] > i['spawn_tick'] for i in impacts)
        if interval:
            assert arrivals and impacts, 'Pressure run had no arrivals or actual contacts'
        else:
            assert not arrivals and not impacts
        terminal_goals = sorted({e['goal'] for e in r['events'] if any(word in e['goal'] for word in
            ['stopped making progress', 'exhausted', 'blocked', 'no measured', 'no reachable'])})
        final_pilots = [o['recovery']['flight']['pilot'] for o in r['samples'][-1]['pilots']]
        row = dict(name=name, physics_ok=run.returncode == 0 and r['failure'] is None,
                   mission_completed=mission['completed_tick'] is not None, mission_failure=mission['failure'],
                   terminal_goals=terminal_goals, exited_tick=r['landing_under_fire']['exited_tick'],
                   asteroid_summary={k: v for k, v in r['asteroids'].items() if k not in ['arrivals', 'impacts']},
                   recovery=[p['recovery'] for p in final_pilots],
                   ownership=final_pilots[0]['planet']['claim'],
                   removed_cells=r['samples'][-1]['audit']['removed_cells'],
                   max_fragments=max(s['audit']['fragments'] for s in r['samples']),
                   max_speed=max(s['audit']['max_speed'] for s in r['samples']),
                   step_p95_ms=r['step_p95_ms'], step_max_ms=r['step_max_ms'], ai_p95_ms=r['ai_p95_ms'])
    except (OSError, ValueError, KeyError, AssertionError, subprocess.SubprocessError) as error:
        row = dict(name=name, physics_ok=False, error=str(error))
    rows.append(row)
    (args.out / 'summary.json').write_text(json.dumps(rows, indent=2) + '\n')
    print(json.dumps(row), flush=True)
# This is an environmental diagnostic, not a promise that every mission wins.
# Individual mission failures remain explicit even when all physics audits pass.
raise SystemExit(0 if all(r['physics_ok'] for r in rows) else 1)
