"""Build the pinned Pi image and record inputs before/after the real Yocto build.

This runs on an unprivileged builder, without signing/publishing credentials.
The output records build provenance; it is not an image qualification receipt
or permission to publish. The publisher must independently inspect the image.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

KAS_VERSION = '5.4'
MACHINE = 'raspberrypi-spacewars'
CONFIG = 'kas-spacewars.yml'
REPOSITORIES = {
    'poky': 'https://git.yoctoproject.org/poky',
    'meta-raspberrypi': 'https://git.yoctoproject.org/meta-raspberrypi',
    'meta-openembedded': 'https://git.openembedded.org/meta-openembedded',
    'meta-rust-bin': 'https://github.com/rust-embedded/meta-rust-bin.git',
    'meta-pi-base': 'https://github.com/aortez/sparkle-duck-shared.git',
}
COMMIT = re.compile(r'[0-9a-f]{40}\Z')
CONFIGURATION_FILES = ('yocto/kas-spacewars.yml', 'Cargo.lock',
                       'yocto/meta-spacewars/conf/machine/raspberrypi-spacewars.conf')


def command(args, *, cwd, env=None, timeout=120):
    result = subprocess.run(args, cwd=cwd, env=env, check=True, text=True,
                            capture_output=True, timeout=timeout)
    if len(result.stdout.encode()) > 4 * 1024 * 1024:
        raise ValueError('Build input query exceeds output bound')
    return result.stdout.strip()


def git(path, *args):
    return command(['git', '-C', str(path), *args], cwd=path,
                   env={**os.environ, 'GIT_OPTIONAL_LOCKS': '0'})


def clean_repository(path, expected=None):
    if (not (path / '.git').exists()
            or Path(git(path, 'rev-parse', '--show-toplevel')).resolve() != path.resolve()):
        raise ValueError('Build input is not the selected Git repository')
    source = git(path, 'rev-parse', 'HEAD')
    if (not COMMIT.fullmatch(source) or (expected is not None and source != expected)
            or git(path, 'status', '--porcelain=v1', '--untracked-files=all')):
        raise ValueError('Build input has wrong commit or uncommitted files')
    return source


def blob(path, maximum=1024 * 1024):
    if path.is_symlink() or not path.is_file():
        raise ValueError('Build configuration must be a regular file')
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, 'rb') as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= maximum:
            raise ValueError('Build input size/type is invalid')
        hashed, size = hashlib.sha256(), 0
        while chunk := stream.read(1024 * 1024):
            size += len(chunk)
            if size > before.st_size:
                raise ValueError('Build input grew during hashing')
            hashed.update(chunk)
        after = os.fstat(stream.fileno())
    if (size != before.st_size or any(getattr(before, field) != getattr(after, field)
            for field in ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_ctime_ns'))):
        raise ValueError('Build input changed during hashing')
    return dict(bytes=size, sha256=hashed.hexdigest())


def configured_repositories(configuration):
    repositories = configuration.get('repos', {})
    if (configuration.get('machine') != MACHINE or configuration.get('target') != 'spacewars-image'
            or configuration.get('distro') != 'poky'
            or set(repositories) != set(REPOSITORIES) | {'meta-spacewars'}
            or repositories['meta-spacewars'] != {'path': 'meta-spacewars'}):
        raise ValueError('Unexpected OTA build configuration')
    for name, url in REPOSITORIES.items():
        item = repositories[name]
        if (item.get('url') != url or not isinstance(item.get('commit'), str)
                or not COMMIT.fullmatch(item['commit']) or 'path' in item or 'patches' in item
                or set(item) - {'url', 'branch', 'commit', 'layers'}):
            raise ValueError('OTA layers must use complete reviewed commit pins')
    return repositories


def expected_inputs(repo, source, configuration):
    """Derive expected inputs from an independently selected clean source tree."""
    clean_repository(repo, source)
    repositories = configured_repositories(configuration)
    layers = {name: dict(url=url, commit=repositories[name]['commit'])
              for name, url in REPOSITORIES.items()}
    encoded = json.dumps(configuration, sort_keys=True, separators=(',', ':')).encode()
    return dict(schema=1, source_commit=source, kas_version=KAS_VERSION,
                machine=MACHINE, target='spacewars-image', layers=layers,
                local_layer=dict(path='yocto/meta-spacewars', source_commit=source),
                configuration={name: blob(repo / name) for name in CONFIGURATION_FILES},
                expanded_kas_sha256=hashlib.sha256(encoded).hexdigest())


def snapshot(repo, source, configuration):
    """Read-only: never repair/reset a changed layer before recording its state."""
    expected = expected_inputs(repo, source, configuration)
    for name, layer in expected['layers'].items():
        directory = repo / 'yocto' / name
        if git(directory, 'remote', 'get-url', 'origin') != layer['url']:
            raise ValueError('Layer checkout has a different origin')
        clean_repository(directory, layer['commit'])
    return expected


def environment(repo, build_directory, cache):
    # Do not inherit implicit kas/BitBake overrides, GitHub tokens, SSH agents or
    # unrelated task credentials. kas only needs the host's ordinary tool paths.
    result = {name: os.environ[name] for name in ('PATH', 'HOME', 'USER', 'LOGNAME', 'SHELL',
                                                 'LANG', 'LC_ALL', 'TERM', 'TMPDIR') if name in os.environ}
    result.update(KAS_WORK_DIR=str(repo / 'yocto'), KAS_BUILD_DIR=str(build_directory),
                  SPACEWARS_CACHE_ROOT=str(cache), SPACEWARS_GPU='1', SPACEWARS_BUILD_CACHE='off')
    return result


def check_build_directory(build_directory):
    conf = build_directory / 'conf'
    if conf.exists():
        allowed = {'bblayers.conf', 'local.conf', 'conf-notes.txt', 'conf-summary.txt', 'templateconf.cfg'}
        if any(path.name not in allowed or path.is_symlink() or not path.is_file() for path in conf.iterdir()):
            raise ValueError('OTA build has additional or linked BitBake configuration')


def build(repo, source, build_directory, output, cache, direct_kas=False):
    repo = repo.resolve()
    build_directory, output, cache = build_directory.resolve(), output.absolute(), cache.resolve()
    if os.getuid() == 0:
        raise ValueError('OTA image builds require an unprivileged account')
    if not COMMIT.fullmatch(source):
        raise ValueError('Select one complete source commit')
    if build_directory == repo or repo in build_directory.parents:
        raise ValueError('Use a separate build directory outside the source checkout')
    if output.exists() or output.is_symlink():
        raise ValueError('Build record output already exists')
    clean_repository(repo, source)
    check_build_directory(build_directory)
    # kas may create missing checkouts, but must not hide edits in existing ones.
    for name in REPOSITORIES:
        path = repo / 'yocto' / name
        if path.exists():
            clean_repository(path)
    env = environment(repo, build_directory, cache)
    version = command(['kas', '--version'], cwd=repo, env=env)
    if not version.startswith('kas ' + KAS_VERSION + ' ('):
        raise ValueError('Use the pinned kas version ' + KAS_VERSION)
    configuration = json.loads(command(['kas', 'dump', '--format', 'json', CONFIG],
                                       cwd=repo / 'yocto', env=env))
    before = snapshot(repo, source, configuration)
    # Keep this immutable record alive through recipe parsing/install. It is
    # public provenance, not a secret or a caller-provided executable hook.
    build_directory.mkdir(parents=True, exist_ok=True)
    raw_inputs = json.dumps(before, sort_keys=True, separators=(',', ':')).encode()
    with tempfile.NamedTemporaryFile(prefix='ota-build-inputs-', suffix='.json', dir=build_directory) as selected:
        selected.write(raw_inputs)
        selected.flush()
        env.update(SPACEWARS_OTA_BUILD_INPUTS=selected.name,
                   SPACEWARS_OTA_BUILD_INPUTS_SHA256=hashlib.sha256(raw_inputs).hexdigest())
        # The normal local wrapper supplies GNU host tools where needed. The
        # pinned kas container already supplies them and needs no Node runtime.
        driver = ['kas', 'build', CONFIG] if direct_kas else ['node', 'scripts/build.mjs', '--gpu']
        subprocess.run(driver, cwd=repo / 'yocto', env=env, check=True)
    # Do not run kas checkout/dump here: it could reset evidence of layer changes.
    after = snapshot(repo, source, configuration)
    if after != before:
        raise ValueError('Build source/configuration changed during Yocto execution')
    check_build_directory(build_directory)
    conf = build_directory / 'conf'
    after['generated_configuration'] = {name: blob(conf / name) for name in ('local.conf', 'bblayers.conf')}
    after['embedded_inputs'] = dict(bytes=len(raw_inputs), sha256=hashlib.sha256(raw_inputs).hexdigest())
    deploy = build_directory / 'tmp/deploy/images' / MACHINE
    image = deploy / f'spacewars-image-{MACHINE}.rootfs.ext4.gz'
    image = image.resolve(strict=True)
    if image.parent != deploy or not image.name.endswith('.ext4.gz'):
        raise ValueError('Yocto image alias escapes its deployment directory')
    identity = json.loads(Path(str(image) + '.image.json').read_text())
    if identity.get('source') != dict(schema=1, commit=source, dirty=False, revision=source[:12]):
        raise ValueError('Deployed image sidecar belongs to a different or dirty source')
    # Binding the build output prevents accidentally packaging a later build's
    # mutable alias. These sidecar checks do not qualify the actual ext4 image.
    after['output'] = dict(name=image.name, blob=blob(image, 2 * 1024 ** 3 - 1), image_identity=identity)
    with output.open('x') as stream:
        json.dump(after, stream, sort_keys=True, indent=2)
        stream.write('\n')
    print('Built pinned Pi image; input/output record: ' + str(output))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', required=True)
    parser.add_argument('--build-directory', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--cache', type=Path, required=True)
    parser.add_argument('--direct-kas', action='store_true', help='Use the pinned kas container toolchain directly')
    args = parser.parse_args()
    build(Path(__file__).resolve().parents[2], args.source, args.build_directory, args.output, args.cache, args.direct_kas)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
