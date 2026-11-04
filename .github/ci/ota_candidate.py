"""Assemble an unsigned OTA artifact from an exact Yocto build output.

The caller supplies policy and a reviewed validator independently of the build
record. This runs without signing/publishing credentials. A publisher must admit
the GitHub artifact and revalidate its image; builder reports are not approval.
"""
import argparse
import ctypes
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile

from ota_build_inputs import COMMIT, MACHINE
from ota_delivery import digest, strict_json, valid_blob, verify_stream

MAX_GZIP_BYTES = 2 * 1024**3 - 1
MAX_ROOT_BYTES = 3 * 1024**3
# Leave room for the JSON files and the upload action's ZIP headers.
ARTIFACT_FILES = {'image.ext4.zst': 2 * 1024**3 - 256 * 1024,
                  'candidate.json': 4096, 'qualification.json': 128 * 1024,
                  'build.json': 64 * 1024}
INPUT_FIELDS = {'schema', 'source_commit', 'kas_version', 'machine', 'target',
                'layers', 'local_layer', 'configuration', 'expanded_kas_sha256'}
CHUNK = 1024 * 1024


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def regular(path):
    descriptor = os.open(path, os.O_PATH | os.O_NOFOLLOW | os.O_CLOEXEC)
    try:
        if not stat.S_ISREG(os.fstat(descriptor).st_mode):
            raise ValueError('Artifact input must be a regular file, not a link or device')
        return open(f'/proc/self/fd/{descriptor}', 'rb')
    finally:
        os.close(descriptor)


def unchanged(before, after):
    return all(getattr(before, key) == getattr(after, key) for key in
               ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_ctime_ns'))


def read(path, maximum):
    with regular(path) as stream:
        before = os.fstat(stream.fileno())
        if not 0 < before.st_size <= maximum:
            raise ValueError('Artifact metadata exceeds its size bound')
        raw = stream.read(maximum + 1)
        if len(raw) != before.st_size or not unchanged(before, os.fstat(stream.fileno())):
            raise ValueError('Artifact metadata changed while reading')
        return raw


def build_record(raw, policy):
    value = strict_json(raw)
    if (not isinstance(policy, dict) or not isinstance(policy.get('source_commit'), str)
            or not COMMIT.fullmatch(policy['source_commit'])
            or not isinstance(value, dict)
            or set(value) != INPUT_FIELDS | {'generated_configuration', 'embedded_inputs', 'output'}
            or type(value['schema']) is not int or value['schema'] != 1
            or value['source_commit'] != policy['source_commit']
            or value['machine'] != MACHINE or value['target'] != 'spacewars-image'):
        raise ValueError('Build record differs from selected source or schema')
    inputs = canonical({name: value[name] for name in INPUT_FIELDS})
    embedded = dict(bytes=len(inputs), sha256=digest(inputs))
    if (embedded != value['embedded_inputs']
            or embedded['sha256'] != policy.get('build_inputs_sha256')):
        raise ValueError('Build inputs differ from independently selected policy')
    generated = value['generated_configuration']
    output = value['output']
    if (not isinstance(generated, dict) or set(generated) != {'local.conf', 'bblayers.conf'}
            or any(not valid_blob(blob, 1024 * 1024) for blob in generated.values())
            or not isinstance(output, dict) or set(output) != {'name', 'blob', 'image_identity'}
            or not isinstance(output['name'], str)
            or not re.fullmatch(r'spacewars-image-raspberrypi-spacewars\.rootfs-[0-9]{14}\.ext4\.gz', output['name'])
            or not valid_blob(output['blob'], MAX_GZIP_BYTES)
            or not isinstance(output['image_identity'], dict)
            or canonical(output['image_identity'].get('source')) != canonical(dict(schema=1,
                commit=policy['source_commit'], dirty=False, revision=policy['source_commit'][:12]))):
        raise ValueError('Invalid exact build output or generated configuration')
    return value


def expand(image, expected, destination):
    """Sparse bounded expansion of one pinned regular input, never a mount."""
    with regular(image) as source:
        before = os.fstat(source.fileno())
        if before.st_size != expected['bytes']:
            raise ValueError('Gzip image length differs from the build record')
        verify_stream(source, expected)
        source.seek(0)
        size, hashed = 0, hashlib.sha256()
        with gzip.GzipFile(fileobj=source, mode='rb') as decoded, destination.open('xb') as target:
            while chunk := decoded.read(CHUNK):
                size += len(chunk)
                if size > MAX_ROOT_BYTES:
                    raise ValueError('Expanded image exceeds the rootfs bound')
                hashed.update(chunk)
                if chunk.strip(b'\0'):
                    target.write(chunk)
                else:
                    target.seek(len(chunk), os.SEEK_CUR)
            target.truncate(size)
            target.flush()
            os.fsync(target.fileno())
        if size < 4096 or not unchanged(before, os.fstat(source.fileno())):
            raise ValueError('Gzip image is too small or changed during expansion')
    return dict(bytes=size, sha256=hashed.hexdigest())


def rename_new(source, destination):
    # Linux renameat2 protects against a racing writer as well as an output
    # that existed before validation. The private work directory shares its FS.
    library = ctypes.CDLL(None, use_errno=True)
    rename = library.renameat2
    rename.argtypes = (ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint)
    rename.restype = ctypes.c_int
    if rename(-100, os.fsencode(source), -100, os.fsencode(destination), 1):
        number = ctypes.get_errno()
        raise OSError(number, os.strerror(number))


def sync_directory(path):
    descriptor = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


def assemble(policy_path, record_path, image, validator, output):
    if sys.platform != 'linux':
        raise ValueError('Image qualification requires Linux')
    output = output.parent.resolve(strict=True) / output.name
    if output.exists() or output.is_symlink():
        raise ValueError('Candidate output already exists')
    policy_bytes = read(policy_path, 16 * 1024)
    policy = strict_json(policy_bytes)
    raw_record = read(record_path, ARTIFACT_FILES['build.json'])
    record = build_record(raw_record, policy)
    validator = validator.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix='.ota-candidate-work-', dir=output.parent) as temporary:
        work = Path(temporary)
        selected = work / 'policy.json'
        selected.write_bytes(policy_bytes)
        expanded = expand(image, record['output']['blob'], work / 'rootfs.ext4')
        staged = work / 'candidate'
        # pack independently decodes/readbacks its own Zstandard output. No
        # build-record field is ever a command, argument or executable path.
        subprocess.run([str(validator), 'pack', str(selected), str(work / 'rootfs.ext4'), str(staged)],
                       env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'}, check=True,
                       stdout=subprocess.DEVNULL, timeout=600)
        if set(path.name for path in staged.iterdir()) != set(ARTIFACT_FILES) - {'build.json'}:
            raise ValueError('Validator produced an unexpected candidate inventory')
        candidate = strict_json(read(staged / 'candidate.json', ARTIFACT_FILES['candidate.json']))
        qualification = strict_json(read(staged / 'qualification.json', ARTIFACT_FILES['qualification.json']))
        if (not isinstance(candidate, dict) or type(candidate.get('schema')) is not int or candidate['schema'] != 1
                or set(candidate) != {'schema', 'format', 'compressed', 'expanded'}
                or candidate.get('format') != 'rootfs-ext4-zstd' or candidate.get('expanded') != expanded
                or not valid_blob(candidate.get('expanded'), MAX_ROOT_BYTES)
                or not valid_blob(candidate.get('compressed'), ARTIFACT_FILES['image.ext4.zst'])
                or not isinstance(qualification, dict) or qualification.get('schema') != 1
                or canonical(qualification.get('image')) != canonical(record['output']['image_identity'])
                or qualification.get('expanded') != expanded
                or qualification.get('build_inputs_sha256') != policy['build_inputs_sha256']):
            raise ValueError('Qualified image differs from the exact build output')
        with regular(staged / 'image.ext4.zst') as payload:
            verify_stream(payload, candidate['compressed'])
        with (staged / 'build.json').open('xb') as saved:
            saved.write(raw_record)
            saved.flush()
            os.fsync(saved.fileno())
        sync_directory(staged)
        rename_new(staged, output)
        sync_directory(output.parent)
    return candidate


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('policy', 'build-record', 'gzip-image', 'validator', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    result = assemble(args.policy, args.build_record, args.gzip_image, args.validator, args.output)
    print(json.dumps(result, sort_keys=True, indent=2))


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, EOFError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
