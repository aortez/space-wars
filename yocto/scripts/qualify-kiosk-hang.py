#!/usr/bin/env python3
"""Inject one reviewed kiosk hang over an already established owner SSH master.

This does not connect, install, reboot, retry, stop recovery or open a watchdog.
The separately built native helper enforces the live trial and process guards.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import time


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def expectation_for_trial(selection, before, trial, candidate_image):
    """Join independently retained selection/image to this new live transaction.

    Inputs are the native selection object and before/trial snapshots containing
    boot and recovery. Caller retains their original hashes before activation.
    """
    old = before['boot']['observation']
    boot = trial['boot']['observation']
    record = trial['recovery']['record']
    tx = record['transaction']
    image = selection['candidate']
    good = before['recovery']['record']['good']
    candidate = dict(slot='a' if good['slot'] == 'b' else 'b',
        build_id=image['build_id'], generation=image['generation'],
        payload=image['compressed']['sha256'], expanded=image['expanded']['sha256'],
        data=image['data'])
    require(selection['boot_id'] == old['boot_id']
            and selection['binding'] == old['binding']
            and selection['recovery'] == before['recovery']['checksum']
            and not old['tryboot'] and old['actual_slot'] == old['default_slot'] == good['slot'],
            'Selection does not describe the retained normal baseline')
    require(boot['boot_id'] != old['boot_id'] and boot['tryboot']
            and boot['binding'] == old['binding'] and boot['uptime_ms'] < 45_000
            and trial['boot']['image'] == candidate_image
            and trial['boot']['kernel_build_id'] == before['boot']['kernel_build_id']
            and candidate_image['source']['commit'] == image['source_commit']
            and candidate_image['build_id'] == image['build_id'],
            'Not the independently selected candidate trial')
    require(tx is not None and tx['phase'] == 'observing'
            and tx['id'] == boot['trial_id'] and tx['origin_boot_id'] == old['boot_id']
            and tx['trial_boot_id'] == boot['boot_id']
            and tx['previous_good'] == record['good'] == good
            and tx['candidate'] == candidate
            and boot['actual_slot'] == candidate['slot']
            and boot['default_slot'] == good['slot'],
            'Retained or mismatched recovery transaction; do not inject')
    return dict(schema=1, binding=old['binding'], kernel_build_id=trial['boot']['kernel_build_id'],
        image=candidate_image, transaction=tx['id'], origin_boot_id=old['boot_id'],
        trial_boot_id=boot['boot_id'], previous_good=good, candidate=candidate)


def save(directory, name, value):
    with (directory / name).open('x') as stream:
        json.dump(value, stream, indent=2)
        stream.write('\n')
        stream.flush()
        os.fsync(stream.fileno())
    fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


class SharedConnection:
    def __init__(self, host, control, helper_sha):
        require(re.fullmatch(r'[A-Za-z0-9._-]+@[A-Za-z0-9][A-Za-z0-9._-]*', host),
                'Use an explicit owner@host')
        require(Path(control).is_absolute(), 'Select an absolute existing control socket')
        require(re.fullmatch(r'[a-f0-9]{64}', helper_sha), 'Select the exact helper SHA-256')
        self.host, self.control, self.sha = host, str(control), helper_sha
        self.helper = '/data/spacewars-recovery-tools/' + helper_sha + '/kiosk_hang_trial'

    def remote(self, args, data=None):
        checked = subprocess.run(['ssh', '-o', 'ControlPath=' + self.control, '-O', 'check',
                                  self.host], capture_output=True, timeout=3)
        require(checked.returncode == 0, 'Shared connection lost; no fresh login or retry')
        result = subprocess.run(['ssh', '-T', '-o', 'BatchMode=yes', '-o', 'ControlMaster=no',
            '-o', 'ControlPath=' + self.control, '-o', 'ProxyCommand=false',
            self.host, shlex.join(['sudo', '-n', *args])],
            input=data, capture_output=True, timeout=12)
        require(len(result.stdout) <= 128 * 1024 and len(result.stderr) <= 128 * 1024,
                'Oversized native response; inspect without replay')
        return result

    def verify(self):
        result = self.remote(['stat', '-c', '%F:%u:%g:%a:%h', self.helper])
        require(result.returncode == 0 and result.stdout.strip() == b'regular file:0:0:500:1',
                'Unsafe helper owner/type/mode/links')
        result = self.remote(['sha256sum', self.helper])
        require(result.returncode == 0 and result.stdout.decode().split()[0] == self.sha,
                'Native helper hash changed')


def invoke(connection, output, operation, arguments, value):
    # Persist intent before any remote call. A timeout or lost reply remains
    # uncertain; both the directory and the native consumed record prevent replay.
    save(output, operation + '-intent.json', dict(
        arguments=arguments, input_sha256=hashlib.sha256(canonical(value)).hexdigest(),
        host_boot_id=Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
        monotonic=time.monotonic(), utc_seconds=time.time()))
    try:
        result = connection.remote([connection.helper, *arguments], canonical(value))
    except (subprocess.TimeoutExpired, OSError, ValueError) as error:
        save(output, operation + '-uncertain.json', dict(error=str(error),
            action='Observe only; never replay this invocation'))
        raise
    save(output, operation + '-response.json', dict(returncode=result.returncode,
         stdout=result.stdout.decode(errors='replace'), stderr=result.stderr.decode(errors='replace')))
    require(result.returncode == 0, 'Native ' + operation + ' refused or failed; inspect retained evidence')
    return json.loads(result.stdout)


def inject(connection, expectation, output):
    connection.verify()
    inspected = invoke(connection, output, 'inspect', ['inspect'], expectation)
    require(set(inspected) == {'plan', 'plan_sha256'}, 'Unexpected native plan envelope')
    plan, selected = inspected['plan'], inspected['plan_sha256']
    require(plan['expectation'] == expectation, 'Native plan changed the selected expectation')
    require(digest(plan) == selected, 'Native plan digest mismatch')
    save(output, 'plan.json', plan)
    connection.verify()
    result = invoke(connection, output, 'stop', ['stop', selected], plan)
    require(result.get('plan_sha256') == selected
            and result.get('transaction') == expectation['transaction']
            and result.get('boot_id') == expectation['trial_boot_id']
            and result.get('signal') == 'SIGSTOP'
            and result.get('stopped_process_observed') is True
            and result.get('automatic_recovery_qualified') is False,
            'Stop receipt differs; observe without replay')
    save(output, 'result.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--host', required=True)
    parser.add_argument('--control-path', required=True)
    parser.add_argument('--helper-sha256', required=True)
    parser.add_argument('--expectation', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    os.umask(0o077)
    require(args.expectation.stat().st_size <= 32 * 1024, 'Oversized expectation')
    expectation = json.loads(args.expectation.read_bytes())
    args.output.mkdir(mode=0o700)
    connection = SharedConnection(args.host, args.control_path, args.helper_sha256)
    result = inject(connection, expectation, args.output)
    print('Selected kiosk stopped at uptime ' + str(result['uptime_ms']) +
          ' ms. Observe automatic recovery; do not run this command again.')


if __name__ == '__main__':
    main()
