"""Join live GitHub admission to independent validation of the same image bytes.

Run pinned publisher tools without signing or release-writing credentials.
The result remains evidence: generation/history, promotion, signing and delivery
must still be admitted independently. No candidate-provided command is run.
"""
import argparse
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import datetime
import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import ota_admission as admission
from ota_candidate import (ARTIFACT_FILES, CHUNK, INPUT_FIELDS, MAX_ROOT_BYTES,
                           build_record, canonical, read, regular, rename_new, sync_directory)
from ota_delivery import strict_json, valid_blob, verify_stream
from ota_image_policy import from_source
from ota_publisher_policy import from_checkout


@dataclass(frozen=True)
class Qualified:
    directory: Path
    origin: dict
    policy: dict
    candidate: dict
    qualification: dict
    checked_at: str


@contextmanager
def qualified(api, policy, selection, source_checkout, validator, scratch,
              clock=lambda: datetime.now(admission.UTC)):
    """Keep verified bytes alive only while the caller consumes this context."""
    if sys.platform != 'linux' or dict(policy.files) != ARTIFACT_FILES:
        raise ValueError('Use Linux and the reviewed candidate artifact inventory')
    image_policy = from_source(source_checkout, selection.source)
    validator = validator.resolve(strict=True)
    with admission.candidate(api, policy, selection, scratch, clock) as (directory, origin):
        record = build_record(read(directory / 'build.json', ARTIFACT_FILES['build.json']), image_policy)
        candidate = strict_json(read(directory / 'candidate.json', ARTIFACT_FILES['candidate.json']))
        if (not isinstance(candidate, dict) or set(candidate) != {'schema', 'format', 'compressed', 'expanded'}
                or type(candidate['schema']) is not int or candidate['schema'] != 1
                or candidate['format'] != 'rootfs-ext4-zstd'
                or not valid_blob(candidate['expanded'], MAX_ROOT_BYTES)
                or not valid_blob(candidate['compressed'], ARTIFACT_FILES['image.ext4.zst'])
                or candidate['compressed'] != origin['files']['image.ext4.zst']):
            raise ValueError('Candidate descriptor differs from the admitted artifact')
        claimed = strict_json(read(directory / 'qualification.json', ARTIFACT_FILES['qualification.json']))
        # Publisher policy and the validator output live outside the artifact.
        # The unsigned builder qualification is only compared after reinspection.
        with tempfile.TemporaryDirectory(prefix='ota-image-check-', dir=scratch) as temporary:
            work = Path(temporary)
            selected = work / 'policy.json'
            selected.write_bytes(canonical(image_policy))
            report = work / 'checked.json'
            with report.open('xb') as stream:
                subprocess.run([str(validator), 'verify', str(selected),
                    str(directory / 'candidate.json'), str(directory / 'image.ext4.zst'), str(work)],
                    env={'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'}, stdout=stream, check=True, timeout=600)
            actual = strict_json(read(report, ARTIFACT_FILES['qualification.json']))
        if (not isinstance(actual, dict) or type(actual.get('schema')) is not int or actual['schema'] != 1
                or canonical(actual) != canonical(claimed)
                or canonical(actual.get('image')) != canonical(record['output']['image_identity'])
                or canonical(actual.get('build_inputs')) != canonical({name: record[name] for name in INPUT_FIELDS})
                or actual.get('build_inputs_sha256') != image_policy['build_inputs_sha256']
                or actual.get('expanded') != candidate['expanded']):
            raise ValueError('Independent image validation disagrees with candidate provenance')
        if from_source(source_checkout, selection.source) != image_policy:
            raise ValueError('Selected source policy changed during qualification')
        # Include local bytes too: a later producer restart or modified sidecar
        # cannot be hidden by the earlier download or validator success.
        if set(path.name for path in directory.iterdir()) != set(ARTIFACT_FILES):
            raise ValueError('Admitted artifact inventory changed')
        for name, blob in origin['files'].items():
            with regular(directory / name) as stream:
                verify_stream(stream, blob)
        checked_at = admission.recheck(api, policy, selection, origin, clock)
        yield Qualified(directory, origin, image_policy, candidate, actual, checked_at)


def materialize(value, destination, tooling_source):
    """Save evidence atomically; this is not a credential-bearing signing input."""
    if (set(value.origin['files']) != set(ARTIFACT_FILES)
            or any(not valid_blob(blob, ARTIFACT_FILES[name]) for name, blob in value.origin['files'].items())):
        raise ValueError('Qualification output has an unexpected artifact inventory')
    destination = destination.parent.resolve(strict=True) / destination.name
    if destination.exists() or destination.is_symlink():
        raise ValueError('Qualification output already exists')
    with tempfile.TemporaryDirectory(prefix='.ota-qualified-', dir=destination.parent) as temporary:
        staged = Path(temporary) / 'candidate'
        staged.mkdir(mode=0o700)
        for name, expected in value.origin['files'].items():
            with regular(value.directory / name) as source, (staged / name).open('xb') as target:
                hashed, size = hashlib.sha256(), 0
                while chunk := source.read(CHUNK):
                    size += len(chunk)
                    if size > expected['bytes']:
                        raise ValueError('Admitted file grew during evidence copy')
                    hashed.update(chunk)
                    target.write(chunk)
                if size != expected['bytes'] or hashed.hexdigest() != expected['sha256']:
                    raise ValueError('Admitted file changed during evidence copy')
                target.flush()
                os.fsync(target.fileno())
        reports = {'origin.json': value.origin, 'image-policy.json': value.policy,
                   'publisher.json': dict(schema=1, scope='origin-and-image-evidence',
                       tooling_source=tooling_source, checked_at=value.checked_at,
                       qualification=value.qualification)}
        for name, value in reports.items():
            with (staged / name).open('xb') as stream:
                stream.write(canonical(value))
                stream.flush()
                os.fsync(stream.fileno())
        sync_directory(staged)
        rename_new(staged, destination)
        sync_directory(destination.parent)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('tooling-checkout', 'source-checkout', 'validator', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    for name in ('tooling-source', 'source'):
        parser.add_argument('--' + name, required=True)
    for name in ('build-run', 'build-attempt', 'ci-run', 'ci-attempt', 'artifact'):
        parser.add_argument('--' + name, type=int, required=True)
    args = parser.parse_args()
    token = os.environ.get('GH_TOKEN')
    if not token:
        raise ValueError('A read-only Actions/Contents GH_TOKEN is required')
    policy = from_checkout(args.tooling_checkout, args.tooling_source)
    selection = admission.Selection(args.source, args.build_run, args.build_attempt,
                                     args.ci_run, args.ci_attempt, args.artifact)
    with qualified(admission.GitHub(token), policy, selection, args.source_checkout,
                   args.validator, args.output.parent) as result:
        materialize(result, args.output, args.tooling_source)
    print('Verified GitHub origin and actual image; signing/publication admission is still required')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
