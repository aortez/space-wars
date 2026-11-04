#!/usr/bin/env python3
"""Check the recovery unit's real kiosk probe without running recovery policy.

Run from the workstation against a normally booted Pi with owner sudo access.
Transient services use the checkout's sandbox and the installed native health
worker. They create only the worker's temporary state-write probe. They never
open a watchdog, update a recovery journal, promote an image or request a reboot.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import uuid

REPO = Path(__file__).resolve().parents[2]
UNIT = REPO / 'yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-recovery.service'
SKIP = {'Type', 'ExecStart', 'RuntimeDirectory', 'RuntimeDirectoryMode',
        'RuntimeDirectoryPreserve', 'RemainAfterExit'}
PROPERTIES = {'User', 'Group', 'UMask', 'TimeoutStartSec', 'TimeoutStopSec',
              'NoNewPrivileges', 'ProtectSystem', 'ReadWritePaths', 'ProtectHome',
              'PrivateTmp', 'ProtectKernelTunables', 'ProtectKernelModules',
              'ProtectControlGroups', 'CapabilityBoundingSet', 'AmbientCapabilities',
              'RestrictNamespaces', 'SystemCallFilter', 'DevicePolicy', 'DeviceAllow',
              'RestrictAddressFamilies', 'RestrictRealtime', 'LockPersonality',
              'MemoryDenyWriteExecute', 'SystemCallArchitectures'}
MANAGER = '/usr/bin/engine-os-manager'
LOCK = '/run/spacewars-install/lock'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def properties(raw):
    service, result = False, {}
    for line in raw.splitlines():
        line = line.strip()
        if line.startswith('['):
            service = line == '[Service]'
            continue
        if not service or not line or line.startswith('#'):
            continue
        key, value = line.split('=', 1)
        require(key in PROPERTIES | SKIP, 'Review the new unit property before testing: ' + key)
        if key in SKIP:
            continue
        require(key not in result, 'Repeated unit property needs explicit handling: ' + key)
        result[key] = value
    require(result.get('User') == 'root' and result.get('NoNewPrivileges') == 'yes',
            'Expected root recovery worker with NoNewPrivileges')
    require(result.get('AmbientCapabilities') == 'CAP_SETUID', 'Expected the reviewed UID transition capability')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True)
    parser.add_argument('--user', default='swadmin')
    parser.add_argument('--output', type=Path, required=True, help='New private evidence directory')
    parser.add_argument('--compare-without-setuid', action='store_true',
                        help='Also require reproduction of the systemd 255 missing-effective-CAP_SETUID failure')
    args = parser.parse_args()
    require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', args.host), 'Use a DNS hostname or IPv4 address')
    require(re.fullmatch(r'[a-z_][a-z0-9_-]*', args.user), 'Invalid SSH account')
    os.umask(0o077)
    args.output.mkdir(mode=0o700)
    unit = UNIT.read_bytes()
    settings = properties(unit.decode())
    suffix = uuid.uuid4().hex[:10]
    ssh = ['ssh', '-T', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=5',
           '-o', 'ServerAliveInterval=5', '-o', 'ServerAliveCountMax=2',
           '-o', 'ControlMaster=auto', '-o', 'ControlPersist=60',
           '-o', 'ControlPath=' + str(args.output.resolve() / 'ssh.sock'),
           args.user + '@' + args.host]

    def remote(command, *, check=True):
        return subprocess.run(ssh + [shlex.join(command)], check=check,
                              capture_output=True, text=True, timeout=30)

    def query(command):
        return remote(command).stdout.strip()

    def journal_hashes():
        # Fixed journal paths; do not read key/configuration contents.
        script = '''set -eu
for p in /data/spacewars-bootstrap/active/boot-trial/record.json /data/spacewars-bootstrap/completed/boot-trial/record.json; do
    [ ! -L "$p" ]
    if [ -e "$p" ]; then sha256sum "$p"; else printf 'absent %s\n' "$p"; fi
done'''
        return query(['sudo', '-n', '/bin/sh', '-c', script])

    boot = query(['cat', '/proc/sys/kernel/random/boot_id'])
    command_line = query(['cat', '/proc/cmdline'])
    require('spacewars.trial=' not in command_line, 'Wait for a normal boot before running this diagnostic')
    require(query(['systemctl', 'show', 'spacewars-recovery.service', '-p', 'ActiveState', '--value']) != 'activating',
            'Recovery is observing a boot; do not run diagnostic probes concurrently')
    require(query(['sudo', '-n', 'stat', '-c', '%F:%u:%g:%a', LOCK]) == 'regular empty file:0:0:600',
            'Expected the existing private installation lock')
    image = json.loads(query(['cat', '/etc/spacewars/image.json']))
    expected = dict(build_id=image['build_id'], build_revision=image['source']['revision'],
                    executable_bytes=image['binaries']['client']['bytes'],
                    executable_sha256=list(bytes.fromhex(image['binaries']['client']['sha256'])))
    before = journal_hashes()
    manager_hash = query(['sha256sum', MANAGER]).split()[0]
    cases = {}

    def case(name, command, selected):
        require(query(['cat', '/proc/sys/kernel/random/boot_id']) == boot, 'Boot changed')
        invocation = ['sudo', '-n', 'systemd-run', '--quiet', '--wait', '--pipe', '--collect',
                      '--unit=spacewars-health-check-' + suffix + '-' + name,
                      '--property=Type=exec', '--property=RuntimeMaxSec=15s',
                      *['--property=' + key + '=' + value for key, value in selected.items()],
                      '/usr/bin/flock', '--close', '-n', LOCK, *command]
        result = remote(invocation, check=False)
        report = dict(returncode=result.returncode, stdout=result.stdout, stderr=result.stderr)
        with (args.output / (name + '.json')).open('x') as stream:
            json.dump(report, stream, indent=2); stream.write('\n')
        cases[name] = report
        return result

    state = ['/usr/bin/setpriv', '--reuid=1000', '--regid=1000', '--clear-groups', '--no-new-privs',
             '/bin/sh', '-c', 'cat /proc/self/status; exec /usr/bin/engine-os-manager kiosk-state-worker']
    if args.compare_without_setuid:
        result = case('previous', state, {k: v for k, v in settings.items() if k != 'AmbientCapabilities'})
        require(result.returncode != 0 and 'setresuid failed: Operation not permitted' in result.stderr,
                'The expected previous capability failure was not reproduced')
    result = case('credentials', state, settings)
    require(result.returncode == 0, 'Kiosk state probe failed: ' + result.stderr)
    fields = dict(line.split(':', 1) for line in result.stdout.splitlines() if ':' in line)
    require(fields.get('Uid', '').split() == ['1000'] * 4 and fields.get('Gid', '').split() == ['1000'] * 4,
            'State probe did not run solely as the kiosk account')
    require(fields.get('Groups', '').split() in ([], ['1000']), 'State probe retained supplemental groups')
    require(all(int(fields.get(name, '-1').strip(), 16) == 0 for name in ['CapPrm', 'CapEff', 'CapAmb'])
            and fields.get('NoNewPrivs', '').strip() == '1', 'State probe retained privilege')
    worker = [MANAGER, 'kiosk-health-worker', json.dumps(expected, separators=(',', ':'))]
    samples = []
    for name, selected in [('healthy-1', settings),
                           ('readonly', {**settings, 'BindReadOnlyPaths': '/data/spacewars/config:/data/spacewars/config'}),
                           ('healthy-2', settings)]:
        result = case(name, worker, selected)
        require(result.returncode == 0, 'Health worker execution failed: ' + result.stderr)
        response = json.loads(result.stdout)
        if name == 'readonly':
            require(response == {'Err': 'kiosk required state write failed'}, 'Read-only state was not rejected')
        else:
            require('Ok' in response and response['Ok']['boot_id'] == boot, 'Health observation failed: ' + result.stdout)
            samples.append(response['Ok'])
    require(all(samples[0][key] == samples[1][key] for key in ['process_id', 'start_ticks', 'invocation_id', 'build_id']),
            'Kiosk restarted during the diagnostic')
    require(all(samples[1][key] > samples[0][key] for key in ['uptime_ms', 'control_response', 'presented_frames']),
            'Kiosk UI or display did not advance')
    after = journal_hashes()
    require(before == after and query(['cat', '/proc/sys/kernel/random/boot_id']) == boot,
            'Boot or bootstrap journal changed during the diagnostic')
    require(query(['sha256sum', MANAGER]).split()[0] == manager_hash, 'Native worker changed during the diagnostic')
    report = dict(schema=1, host=args.host, boot_id=boot,
                  unit_sha256=hashlib.sha256(unit).hexdigest(), properties=settings,
                  manager_sha256=manager_hash, expected_kiosk=expected, samples=samples,
                  case_names=list(cases), journal_hashes_before=before, journal_hashes_after=after,
                  read_only_state_rejected=True, kiosk_uid=1000, kiosk_effective_capabilities=0,
                  recovery_policy_executed=False, watchdog_opened=False, reboot_requested=False,
                  scope='Native sandbox diagnostic only; no promotion or boot qualification')
    with (args.output / 'verification.json').open('x') as stream:
        json.dump(report, stream, indent=2); stream.write('\n')
    print('Recovery sandbox checks passed; boot and bootstrap journal are unchanged: ' + str(args.output))


if __name__ == '__main__':
    main()
