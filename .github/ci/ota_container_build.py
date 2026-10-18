"""Run one exact-source Pi build in a disposable, unprivileged kas container.

No source/home/cache/socket mount or host credential enters the container. The
source is a clean Git clone, including Git identity but excluding untracked and
ignored host files. Outputs remain unsigned; this helper grants no publication
or source admission. The caller must select source and runner resources first.
"""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile

from ota_build_inputs import clean_repository
from ota_candidate import (ARTIFACT_FILES, build_record, canonical, read, regular,
                           rename_new, sync_directory)
from ota_delivery import verify_stream
from ota_image_policy import from_source

KAS_IMAGE = 'ghcr.io/siemens/kas/kas@sha256:0911a72a5474d5287ecf546c9aa039aea0f04326531271996c2efb18a7e9569a'
CONTAINER_ID = re.compile(r'[0-9a-f]{64}\Z')
DEPLOY = '/builder/build/tmp/deploy/images/raspberrypi-spacewars/'
ENTRY = '''set -eu
umask 022
test "$(id -u)" -eq 30000
mkdir /builder/source /builder/out
tar -xzf /tmp/ota-source.tar.gz -C /builder/source --no-same-owner
cd /builder/source
test "$(git rev-parse HEAD)" = "$1"
test -z "$(git status --porcelain=v1 --untracked-files=all)"
exec python3 .github/ci/ota_build_inputs.py --source "$1" \\
  --build-directory /builder/build --cache /builder/cache \\
  --output /builder/out/build-inputs.json --direct-kas
'''


class Commands:
    def __init__(self, home):
        home.mkdir()
        (home / 'docker').mkdir()
        self.env = dict(PATH=os.environ.get('PATH', '/usr/bin:/bin'), HOME=str(home),
                        LC_ALL='C.UTF-8', GIT_CONFIG_NOSYSTEM='1',
                        GIT_CONFIG_GLOBAL='/dev/null', DOCKER_CONFIG=str(home / 'docker'))

    def query(self, args, timeout=120):
        result = subprocess.run(args, env=self.env, check=True, capture_output=True,
                                text=True, timeout=timeout)
        if len(result.stdout.encode()) > 1024 * 1024:
            raise ValueError('Build-tool response exceeds its bound')
        return result.stdout.strip()

    def run(self, args, timeout=120):
        subprocess.run(args, env=self.env, check=True, timeout=timeout)


def source_archive(repo, source, destination, commands):
    """A shallow local clone never copies caller config, hooks or ignored data."""
    clean_repository(repo, source)
    clone = destination.parent / 'source'
    commands.run(['git', '-c', 'init.templateDir=', 'clone', '--no-local', '--no-checkout',
                  '--depth', '1', repo.as_uri(), str(clone)])
    commands.run(['git', '-C', str(clone), 'checkout', '--detach', source])
    clean_repository(clone, source)
    with tarfile.open(destination, 'w:gz', dereference=False) as archive:
        archive.add(clone, arcname='.')
    destination.chmod(0o644)  # Public source, readable by the container's UID.
    clean_repository(repo, source)


def collect(container, policy, output, commands):
    """Collect only the named/hash-bound image after a successful build exit."""
    if not CONTAINER_ID.fullmatch(container):
        raise ValueError('Select the exact created container ID')
    state = json.loads(commands.query(['docker', 'inspect', container, '--format', '{{json .State}}']))
    if state.get('Status') != 'exited' or state.get('ExitCode') != 0 or state.get('OOMKilled') is not False:
        raise ValueError('Container build did not complete successfully')
    output = output.parent.resolve(strict=True) / output.name
    if output.exists() or output.is_symlink():
        raise ValueError('Build output already exists')
    with tempfile.TemporaryDirectory(prefix='.ota-build-result-', dir=output.parent) as temporary:
        work = Path(temporary)
        staged = work / 'result'
        staged.mkdir(mode=0o700)
        record_path = staged / 'build.json'
        commands.run(['docker', 'cp', container + ':/builder/out/build-inputs.json', str(record_path)])
        raw = read(record_path, ARTIFACT_FILES['build.json'])
        record = build_record(raw, policy)
        image = staged / 'rootfs.ext4.gz'
        commands.run(['docker', 'cp', container + ':' + DEPLOY + record['output']['name'], str(image)], timeout=300)
        with regular(image) as stream:
            verify_stream(stream, record['output']['blob'])
        # This is the caller's original policy, retained for its own subsequent
        # packaging step. A publisher must independently derive its own policy.
        (staged / 'policy.json').write_bytes(canonical(policy))
        for path in staged.iterdir():
            with regular(path) as stream:
                os.fsync(stream.fileno())
        sync_directory(staged)
        rename_new(staged, output)
        sync_directory(output.parent)
    return record


def build(repo, source, output, cpus=4, memory_gib=12, minutes=330):
    if sys.platform != 'linux' or os.getuid() == 0:
        raise ValueError('Use an unprivileged Linux workstation/runner')
    if (type(cpus) is not int or not 1 <= cpus <= 16
            or type(memory_gib) is not int or not 8 <= memory_gib <= 48
            or type(minutes) is not int or not 1 <= minutes <= 1440):
        raise ValueError('Invalid bounded container resource selection')
    available = sorted(os.sched_getaffinity(0))
    if cpus > len(available):
        raise ValueError('Requested CPUs exceed the available CPU affinity')
    cpu_set = ','.join(str(cpu) for cpu in available[:cpus])
    repo = repo.resolve(strict=True)
    output = output.parent.resolve(strict=True) / output.name
    if output.exists() or output.is_symlink():
        raise ValueError('Build output already exists')
    policy = from_source(repo, source)
    with tempfile.TemporaryDirectory(prefix='.ota-container-build-', dir=output.parent) as temporary:
        work = Path(temporary)
        commands = Commands(work / 'home')
        archive = work / 'source.tar.gz'
        source_archive(repo, source, archive, commands)
        commands.run(['docker', 'pull', KAS_IMAGE], timeout=600)
        container = None
        try:
            container = commands.query([
                'docker', 'create', '--platform', 'linux/amd64', '--network', 'bridge',
                '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges',
                '--security-opt', 'seccomp=unconfined', '--pids-limit', '4096',
                '--cpus', str(cpus), '--cpuset-cpus', cpu_set,
                '--memory', f'{memory_gib}g', '--memory-swap', f'{memory_gib}g',
                '--label', 'spacewars.ota-build=' + source,
                KAS_IMAGE, '/bin/sh', '-ec', ENTRY, 'ota-build', source])
            if not CONTAINER_ID.fullmatch(container):
                container = None
                raise ValueError('Docker did not return an exact container ID')
            configuration = json.loads(commands.query(['docker', 'inspect', container]))
            if (not isinstance(configuration, list) or len(configuration) != 1
                    or configuration[0].get('Mounts') != []
                    or configuration[0].get('Config', {}).get('User') != 'builder'
                    or configuration[0].get('HostConfig', {}).get('Privileged') is not False):
                raise ValueError('Unexpected container isolation or user')
            commands.run(['docker', 'cp', str(archive), container + ':/tmp/ota-source.tar.gz'], timeout=300)
            commands.run(['docker', 'start', '--attach', container], timeout=minutes * 60)
            record = collect(container, policy, output, commands)
        finally:
            if container is not None:
                # Only the exact ID created above is removed, including on a
                # timeout/interruption. No caller container or volume is pruned.
                commands.run(['docker', 'rm', '--force', container])
    return record


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-checkout', type=Path, required=True)
    parser.add_argument('--source', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cpus', type=int, default=4)
    parser.add_argument('--memory-gib', type=int, default=12)
    parser.add_argument('--timeout-minutes', type=int, default=330)
    args = parser.parse_args()
    result = build(args.source_checkout, args.source, args.output,
                   args.cpus, args.memory_gib, args.timeout_minutes)
    print('Built exact source ' + result['source_commit'] + '; image qualification is still required')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
