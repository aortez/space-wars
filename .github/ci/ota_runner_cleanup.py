"""Fast, bounded SDK cleanup only on a disposable GitHub-hosted Linux runner.

No package purge, Docker prune, disk relocation or swap setup. Only the fixed
SDK directories below are removed, only for this repository's main build or an
explicitly selected manual runner-qualification branch.
The caller must still qualify actual runner capacity with a successful build.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

SDK_PATHS = ('/usr/local/lib/android', '/usr/share/dotnet',
             '/opt/hostedtoolcache/CodeQL', '/opt/ghc', '/usr/local/.ghcup',
             '/usr/share/swift', '/usr/local/share/boost')
GIB = 1024 ** 3
# Includes Yocto's 20 GiB stop threshold, the isolated source/toolchain copies,
# and temporary image qualification space. Recheck against cold-build evidence.
MINIMUM_FREE = 60 * GIB
CLEANUP = ('sudo', '-n', 'timeout', '--signal=TERM', '--kill-after=5s', '60s',
           'rm', '--recursive', '--force', '--one-file-system', '--preserve-root=all',
           '--', *SDK_PATHS)


def allowed(env, qualification_branch=None):
    expected = dict(GITHUB_ACTIONS='true', RUNNER_ENVIRONMENT='github-hosted',
                    RUNNER_OS='Linux', GITHUB_REPOSITORY='aortez/space-wars')
    if sys.platform != 'linux' or any(env.get(key) != value for key, value in expected.items()):
        raise ValueError('SDK cleanup is restricted to this repository on GitHub-hosted Linux')
    if qualification_branch is None:
        if (env.get('GITHUB_REF') != 'refs/heads/main'
                or env.get('GITHUB_EVENT_NAME') not in ('schedule', 'workflow_dispatch')):
            raise ValueError('SDK cleanup requires the main OTA build or explicit manual qualification')
    elif (not re.fullmatch(r'test/ota-runner-capacity-[1-9][0-9]*', qualification_branch)
            or env.get('GITHUB_REF') != 'refs/heads/' + qualification_branch
            or env.get('GITHUB_EVENT_NAME') != 'workflow_dispatch'):
        raise ValueError('SDK cleanup qualification requires the exact selected manual test branch')


def filesystem_space(paths):
    return {str(path): dict(free_bytes=shutil.disk_usage(path).free,
                            free_inodes=os.statvfs(path).f_favail) for path in paths}


def cleanup(env, qualification_branch=None):
    allowed(env, qualification_branch)  # Before querying Docker, touching files or running sudo.
    temporary = Path(env['RUNNER_TEMP']).resolve(strict=True)
    docker_root = Path(subprocess.check_output(
        ['docker', 'info', '--format', '{{.DockerRootDir}}'], text=True, timeout=15).strip())
    if not docker_root.is_absolute() or not docker_root.is_dir():
        raise ValueError('Docker storage directory is unavailable')
    paths = tuple(dict.fromkeys((temporary, docker_root.resolve(strict=True))))
    before = filesystem_space(paths)
    # timeout runs as root too, so its signals reach rm even under sudo. The
    # five-second kill grace also bounds cleanup that does not honor SIGTERM.
    result = subprocess.run(CLEANUP, check=False, timeout=75)
    after = filesystem_space(paths)
    report = dict(schema=1, cleanup_exit=result.returncode, paths=list(SDK_PATHS),
                  before=before, after=after, minimum_free_bytes=MINIMUM_FREE,
                  qualification_branch=qualification_branch)
    print(json.dumps(report, sort_keys=True), flush=True)
    if result.returncode not in (0, 124, 137):
        raise ValueError('Fast SDK cleanup failed; see its exit code above')
    if any(value['free_bytes'] < MINIMUM_FREE or value['free_inodes'] < 500_000
           for value in after.values()):
        raise ValueError('Fast cleanup did not leave 60 GiB and 500,000 free inodes '
                         'on both build filesystems; refusing to start the image build')
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--qualification-branch', help='Exact manual test/ota-runner-capacity-N branch')
    args = parser.parse_args()
    try:
        cleanup(os.environ, args.qualification_branch)
    except (KeyError, OSError, ValueError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
