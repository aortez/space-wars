#!/usr/bin/env python3
"""Run one supervised missing-root diagnostic from the workstation.

Use inspect, prepare, activate, observe, then restore with one private evidence
directory. Only activate requests a reboot, exactly once. A lost reply is never
retried. This tests boot fallback, not a signed OTA candidate or its quarantine.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shlex
import subprocess
import time

REPO = Path(__file__).resolve().parents[2]
CLI = ['sudo', '-n', '/usr/bin/setpriv', '--reuid=1000', '--regid=1000',
       '--clear-groups', '--no-new-privs', '/usr/bin/spacewars-cli',
       '--socket', '/run/spacewars-kiosk/control.sock']


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def save(path, value):
    with path.open('x') as out:
        json.dump(value, out, indent=2)
        out.write('\n')
        out.flush()
        os.fsync(out.fileno())
    sync_directory(path.parent)


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def read(path):
    return json.loads(path.read_text())


class Runner:
    def __init__(self, directory, host, helper):
        self.directory, self.host, self.helper = directory, host, helper
        require(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*', host), 'Use a DNS hostname or IPv4 address')
        require(re.fullmatch(r'/data/spacewars-recovery-tools/[0-9a-f]{64}/recovery_qualify', helper),
                'Unexpected operator helper path')
        self.ssh = ['ssh', '-T', '-o', 'BatchMode=yes', '-o', 'StrictHostKeyChecking=yes',
                    '-o', 'ConnectTimeout=5', '-o', 'ServerAliveInterval=5',
                    '-o', 'ServerAliveCountMax=2', '-o', 'ControlMaster=no',
                    '-o', 'ControlPath=none', 'swadmin@' + host]

    def remote(self, command, *, data=None, timeout=90):
        return subprocess.run(self.ssh + [shlex.join(command)], input=data,
                              capture_output=True, timeout=timeout)

    def query(self, command):
        result = self.remote(command)
        require(result.returncode == 0, 'Remote check failed: ' + result.stderr.decode(errors='replace'))
        return result.stdout.decode().strip()

    def helper_identity(self, expected):
        require(self.query(['sudo', '-n', 'sha256sum', self.helper]).split()[0] == expected,
                'Operator helper changed')
        require(self.query(['sudo', '-n', 'stat', '-c', '%u:%g:%a:%h', self.helper]) == '0:0:500:1',
                'Operator helper ownership, mode or links changed')

    def native(self, operation, value, *, data=None, timeout=90):
        return self.remote(['sudo', '-n', self.helper, operation, value], data=data, timeout=timeout)

    def operation(self, name, value, *, data=None):
        # Reserve evidence before remote invocation. An empty result after
        # interruption is uncertain, never permission to repeat a mutation.
        with (self.directory / (name + '.json')).open('x') as out, \
             (self.directory / (name + '.err')).open('xb') as err:
            out.flush()
            os.fsync(out.fileno())
            sync_directory(self.directory)
            result = self.native(name, value, data=data)
            out.write(result.stdout.decode())
            out.flush()
            os.fsync(out.fileno())
            err.write(result.stderr)
        save(self.directory / (name + '-exit.json'), {'returncode': result.returncode})
        return result

    def stores(self):
        script = '''set -eu
for p in /data/spacewars/config /data/spacewars/roms /data/spacewars-access /data/NetworkManager/system-connections /data/bluetooth /data/ssh; do
  if [ -d "$p" ]; then
    find "$p" -type f -exec sha256sum {} +
    find "$p" -exec stat -c '%F:%u:%g:%a:%n' {} +
    find "$p" -type l -exec /bin/sh -c 'for f do printf "link %s -> " "$f"; readlink "$f"; done' sh {} +
  else
    printf 'absent %s\n' "$p"
  fi
done'''
        return sorted(self.query(['sudo', '-n', '/bin/sh', '-c', script]).splitlines())

    def health(self, image):
        expected = dict(build_id=image['build_id'], build_revision=image['source']['revision'],
                        executable_bytes=image['binaries']['client']['bytes'],
                        executable_sha256=list(bytes.fromhex(image['binaries']['client']['sha256'])))
        response = json.loads(self.query(['sudo', '-n', '/usr/bin/engine-os-manager',
                                         'kiosk-health-worker', json.dumps(expected, separators=(',', ':'))]))
        require('Ok' in response, 'Native kiosk health probe failed: ' + str(response))
        return response['Ok']


def inspect(args):
    require(args.host and args.tool and args.reference, 'inspect requires --host, --tool and --reference')
    reference = read(args.reference)
    require(reference['device'] == args.host and reference['handoff']['baseline_generation'] == 0,
            'Reference must identify this device and its completed bootstrap')
    baseline_bytes = (Path(reference['private_evidence']) / 'canonical-boot-probe.json').read_bytes()
    require(sha(baseline_bytes) == reference['evidence_files']['canonical-boot-probe.json']['sha256'],
            'Independent baseline boot evidence changed')
    baseline = json.loads(baseline_bytes)
    binary = args.tool.read_bytes()
    digest = sha(binary)
    helper = '/data/spacewars-recovery-tools/' + digest + '/recovery_qualify'
    args.output.mkdir(mode=0o700)
    runner = Runner(args.output, args.host, helper)
    require(json.loads(runner.query(['cat', '/etc/spacewars/image.json'])) == reference['image'],
            'Installed image differs from the chosen baseline')
    require(runner.query(['sudo', '-n', 'sha256sum', '/data/spacewars-system/recovery/state.json']).split()[0]
            == reference['handoff']['baseline_sha256'], 'Production baseline differs before helper upload')
    runner.query(['sudo', '-n', 'install', '-d', '-m', '700', str(Path(helper).parent)])
    existing = runner.remote(['sudo', '-n', 'test', '-e', helper])
    if existing.returncode != 0:
        require(existing.returncode == 1, 'Could not inspect operator helper destination')
        uploaded = runner.remote(['sudo', '-n', 'tee', helper], data=binary)
        require(uploaded.returncode == 0 and uploaded.stdout == binary, 'Operator helper upload failed')
        runner.query(['sudo', '-n', 'chmod', '500', helper])
    runner.helper_identity(digest)
    identity = secrets.token_hex(32)
    result = runner.operation('inspect', identity)
    require(result.returncode == 0, 'Native inspection failed; retained stderr has the reason')
    inspected = json.loads(result.stdout)
    plan = inspected['plan']
    binding = baseline['observation']['binding']
    require(plan['stamp']['device'] == binding['device_id'] and plan['stamp']['layout'] == binding['layout_id'],
            'Board, firmware, USB medium or layout differs from the baseline')
    require(plan['stamp']['image'] == reference['image'] and plan['stamp']['kernel'] == reference['kernel_build_id'],
            'Loaded image or kernel differs')
    require(plan['recovery_sha256'] == reference['handoff']['baseline_sha256']
            and plan['completed_bootstrap_sha256'] == reference['handoff']['completed_trial_sha256'],
            'Persistent baseline or completed journal differs')
    save(args.output / 'plan.json', plan)
    save(args.output / 'reference.json', reference)
    save(args.output / 'runner.json', dict(host=args.host, helper=helper, helper_sha256=digest,
         id=identity, plan_sha256=inspected['plan_sha256'],
         host_boot_id=Path('/proc/sys/kernel/random/boot_id').read_text().strip(),
         created_utc=datetime.now(timezone.utc).isoformat(), runner_sha256=sha(Path(__file__).read_bytes()),
         source_head=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(),
         source_status=subprocess.check_output(['git', 'status', '--porcelain'], cwd=REPO, text=True)))
    print('Inspected the exact baseline and saved the plan; no selector, root image or recovery record changed.', flush=True)


def prepare(runner, metadata, plan):
    save(runner.directory / 'stores-before.json', runner.stores())
    save(runner.directory / 'health-before.json', runner.health(plan['stamp']['image']))
    save(runner.directory / 'ui-before.json', json.loads(runner.query(CLI + ['ui', 'state', '--json'])))
    result = runner.operation('prepare', metadata['plan_sha256'], data=json.dumps(plan).encode())
    require(result.returncode == 0, 'Preparation failed; do not activate; inspect and restore the recorded attempt')
    require(json.loads(result.stdout)['state']['phase'] == 'prepared', 'Preparation did not complete')
    print('Missing-root selector prepared; normal boot and production baseline unchanged.', flush=True)


def activate(runner, metadata, plan):
    require(read(runner.directory / 'prepare.json')['state']['phase'] == 'prepared', 'No successful preparation')
    require(Path('/proc/sys/kernel/random/boot_id').read_text().strip() == metadata['host_boot_id'],
            'Workstation rebooted; inspect timing and state before activation')
    save(runner.directory / 'activation-start.json', dict(monotonic=time.monotonic(),
         utc=datetime.now(timezone.utc).isoformat(), host_boot_id=metadata['host_boot_id']))
    result = runner.operation('activate', metadata['id'])
    require(result.returncode in [0, 255], 'Activation failed; inspect consumed state before further action')
    if result.returncode == 0:
        require(json.loads(result.stdout)['reboot_job_accepted'], 'No accepted reboot job')
    print('Activation invoked once. Observe the result; never repeat an ambiguous request.', flush=True)


def observe(runner, metadata, plan):
    started = read(runner.directory / 'activation-start.json')
    require(Path('/proc/sys/kernel/random/boot_id').read_text().strip() == started['host_boot_id'],
            'Workstation rebooted; automatic elapsed-time evidence is unavailable')
    require(not (runner.directory / 'returned.json').exists(), 'Return already observed')
    disconnected = False
    last_notice = -30
    deadline = started['monotonic'] + 420
    with (runner.directory / 'observations.jsonl').open('x') as log:
        while time.monotonic() < deadline:
            elapsed = time.monotonic() - started['monotonic']
            try:
                result = runner.native('status', metadata['id'], timeout=min(45, deadline - time.monotonic()))
                sample = dict(elapsed_seconds=round(elapsed, 3), returncode=result.returncode,
                              stdout=result.stdout.decode(errors='replace'), stderr=result.stderr.decode(errors='replace'))
            except subprocess.TimeoutExpired:
                sample = dict(elapsed_seconds=round(elapsed, 3), returncode=None, timeout=True)
            log.write(json.dumps(sample) + '\n')
            log.flush()
            os.fsync(log.fileno())
            if sample['returncode'] in [None, 255]:
                disconnected = True
            if sample['returncode'] == 0:
                report = json.loads(sample['stdout'])
                if report['current_boot_id'] != plan['origin_boot_id']:
                    require(report['state']['request_consumed'] and report['state']['phase'] == 'requested',
                            'Returned diagnostic state differs; inspect it')
                    completed = time.monotonic() - started['monotonic']
                    require(completed <= 420, 'Normal return was verified after the observation deadline')
                    returned = dict(elapsed_seconds=round(completed, 3),
                         normal_boot_report=report, connection_loss_observed=disconnected,
                         watchdog_interval_consistent=180 <= completed <= 420,
                         trial_userspace_observed=False, runner_requested_additional_reboot=False,
                         scope='Normal return observed after one diagnostic request; no serial trace of missing-root trial or exact watchdog reset time')
                    save(runner.directory / 'returned.json', returned)
                    runner.helper_identity(metadata['helper_sha256'])
                    print('Returned to unchanged normal image after', returned['elapsed_seconds'], 'seconds.', flush=True)
                    return
            if elapsed - last_notice >= 30:
                print('Waiting for bounded normal return:', round(elapsed), 'seconds', flush=True)
                last_notice = elapsed
            time.sleep(max(0, min(3, deadline - time.monotonic())))
    raise RuntimeError('No verified normal return within 420 seconds. Do not issue another trial; use independent recovery and inspect the retained attempt.')


def restore(runner, metadata, plan):
    # Native restoration is idempotent, including after a lost reply. Keep each
    # cleanup/verification run separate so a failed postcheck can be repeated
    # without overwriting evidence or re-enabling a trial.
    original_directory = runner.directory
    attempts = original_directory / 'restorations'
    attempts.mkdir(mode=0o700, exist_ok=True)
    attempt = attempts / (datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ-') + secrets.token_hex(4))
    attempt.mkdir(mode=0o700)
    runner = Runner(attempt, runner.host, runner.helper)
    result = runner.operation('restore', metadata['id'])
    require(result.returncode == 0, 'Restoration stopped; inspect retained state before further action')
    restored = json.loads(result.stdout)
    require(restored['state']['phase'] == 'restored', 'Restoration not complete')
    runner.query(['sudo', '-n', 'systemctl', 'restart', 'spacewars-recovery.service'])
    service = json.loads(runner.query(['sudo', '-n', 'cat', '/run/spacewars-updater/status.json']))
    require(service['boot_id'] == restored['current_boot_id'] and service['progress']['stage'] == 'healthy',
            'Production recovery service did not accept the unchanged baseline')
    save(runner.directory / 'recovery-after.json', service)
    after = runner.stores()
    save(runner.directory / 'stores-after.json', after)
    require(after == read(original_directory / 'stores-before.json'), 'Persistent store inventory changed; compare retained manifests')
    samples = [runner.health(plan['stamp']['image'])]
    time.sleep(2)
    samples.append(runner.health(plan['stamp']['image']))
    save(runner.directory / 'health-after.json', samples)
    require(all(samples[0][key] == samples[1][key] for key in ['boot_id', 'process_id', 'start_ticks', 'invocation_id', 'build_id']),
            'Kiosk restarted between health observations')
    require(samples[0]['boot_id'] == restored['current_boot_id']
            and all(samples[1][key] > samples[0][key] for key in ['uptime_ms', 'control_response', 'presented_frames']),
            'Kiosk control/render progress did not advance')
    ui = json.loads(runner.query(CLI + ['ui', 'state', '--json']))
    save(runner.directory / 'ui-after.json', ui)
    require(ui['error'] is None and not ui['paused'], 'Game did not return to unpaused operation')
    shot = '/run/spacewars-kiosk/recovery-qualification-' + metadata['id'][:16] + '.png'
    runner.query(CLI + ['screenshot', shot])
    png = runner.remote(['sudo', '-n', 'cat', shot])
    require(png.returncode == 0 and png.stdout.startswith(b'\x89PNG\r\n\x1a\n'), 'Screenshot capture failed')
    with (runner.directory / 'screenshot.png').open('xb') as out:
        out.write(png.stdout)
    retry = runner.native('activate', metadata['id'])
    save(runner.directory / 'repeat-rejected.json', dict(returncode=retry.returncode,
         stdout=retry.stdout.decode(), stderr=retry.stderr.decode()))
    require(retry.returncode != 0 and 'already consumed or not prepared' in retry.stderr.decode(),
            'Consumed diagnostic did not refuse repetition')
    final = json.loads(runner.query(['sudo', '-n', runner.helper, 'status', metadata['id']]))
    require(final['record_sha256'] == restored['record_sha256']
            and final['current_boot_id'] == restored['current_boot_id'], 'Replay rejection changed completed state')
    require(runner.query(['sudo', '-n', 'busctl', 'get-property', 'org.freedesktop.login1',
                         '/org/freedesktop/login1', 'org.freedesktop.login1.Manager', 'RebootParameter']) == 's ""',
            'Unexpected pending reboot parameter')
    save(runner.directory / 'verification.json', dict(boot_id=final['current_boot_id'],
         diagnostic_id=metadata['id'], original_selector_restored=True, production_baseline_unchanged=True,
         selected_persistent_stores_unchanged=True, recovery_service_healthy=True, kiosk_progress_verified=True,
         repeated_activation_rejected=True, automatic_ota_enabled=False,
         candidate_quarantine_tested=False, physical_failure_matrix_qualified=False))
    print('Original selector, baseline, persistent stores and kiosk progress verified; repeat activation refused. Evidence:', attempt, flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['inspect', 'prepare', 'activate', 'observe', 'restore', 'status'])
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--host')
    parser.add_argument('--tool', type=Path)
    parser.add_argument('--reference', type=Path)
    args = parser.parse_args()
    os.umask(0o077)
    args.output = args.output.resolve()
    if args.operation == 'inspect':
        inspect(args)
        return
    info = args.output.lstat()
    require(args.output.is_dir() and info.st_uid == os.getuid() and info.st_mode & 0o077 == 0,
            'Use the original private evidence directory')
    metadata, plan = read(args.output / 'runner.json'), read(args.output / 'plan.json')
    runner = Runner(args.output, metadata['host'], metadata['helper'])
    # The board can already be in its deliberately unbootable trial. The
    # observer must start offline, then recheck the helper after normal return.
    if args.operation != 'observe':
        runner.helper_identity(metadata['helper_sha256'])
    if args.operation == 'status':
        print(runner.query(['sudo', '-n', runner.helper, 'status', metadata['id']]))
    else:
        globals()[args.operation](runner, metadata, plan)


if __name__ == '__main__':
    main()

