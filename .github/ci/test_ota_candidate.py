"""Assembly regressions, with a stub for the separately tested ext4 validator.

These test the real gzip, file, metadata and atomic-output boundaries. The stub
is deliberately not an image validator; real ext4/decoder tests live in Rust.
"""
import copy
import gzip
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import ota_candidate as helper


def blob(raw):
    return dict(bytes=len(raw), sha256=helper.digest(raw))


class CandidateTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.policy_path = self.directory / 'policy.json'
        self.record_path = self.directory / 'record.json'
        self.gzip = self.directory / 'image.gz'
        self.output = self.directory / 'output'
        self.raw = b'fixture ext4 contents' + b'\0' * 16384
        self.gz = gzip.compress(self.raw)
        self.gzip.write_bytes(self.gz)
        self.inputs = dict(schema=1, source_commit='a' * 40, kas_version='5.4',
                           machine=helper.MACHINE, target='spacewars-image',
                           layers={}, local_layer={}, configuration={}, expanded_kas_sha256='b' * 64)
        inputs = helper.canonical(self.inputs)
        self.policy = dict(schema=1, source_commit='a' * 40, build_inputs_sha256=helper.digest(inputs),
                           rust_version='1.94.0', cargo_version='1.94.0',
                           cc_version_line='aarch64-poky-linux-gcc (GCC) 13.5.0')
        self.identity = dict(source=dict(schema=1, commit='a' * 40, dirty=False, revision='a' * 12))
        self.record = self.inputs | dict(embedded_inputs=blob(inputs), generated_configuration={
            'local.conf': blob(b'configuration'), 'bblayers.conf': blob(b'layers')}, output=dict(
                name='spacewars-image-raspberrypi-spacewars.rootfs-20000101000000.ext4.gz',
                blob=blob(self.gz), image_identity=self.identity))
        self.save()
        self.hook = None

    def save(self):
        self.policy_path.write_bytes(helper.canonical(self.policy))
        self.record_path.write_bytes(helper.canonical(self.record))

    def validator(self, args, **kwargs):
        self.assertEqual(args[:2], [str(Path('/usr/bin/true').resolve()), 'pack'])
        self.assertEqual(kwargs['env'], {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'})
        self.assertTrue(kwargs['check'])
        self.assertEqual(kwargs['timeout'], 600)
        selected, root, staged = map(Path, args[2:])
        self.assertEqual(json.loads(selected.read_bytes()), self.policy)
        self.assertEqual(root.read_bytes(), self.raw)
        staged.mkdir()
        payload = b'stub validator payload; Rust tests cover actual Zstandard/ext4'
        candidate = dict(schema=1, format='rootfs-ext4-zstd', compressed=blob(payload), expanded=blob(self.raw))
        qualification = dict(schema=1, image=self.identity, expanded=blob(self.raw),
                             build_inputs_sha256=self.policy['build_inputs_sha256'])
        (staged / 'image.ext4.zst').write_bytes(payload)
        (staged / 'candidate.json').write_bytes(helper.canonical(candidate))
        (staged / 'qualification.json').write_bytes(helper.canonical(qualification))
        if self.hook:
            self.hook(staged)
        return subprocess.CompletedProcess(args, 0)

    def assemble(self):
        with patch.object(helper.subprocess, 'run', side_effect=self.validator) as run:
            result = helper.assemble(self.policy_path, self.record_path, self.gzip,
                                     Path('/usr/bin/true'), self.output)
            self.assertEqual(run.call_count, 1)
        return result

    def assert_clean_failure(self, error=(ValueError, OSError, EOFError, subprocess.SubprocessError)):
        with self.assertRaises(error):
            self.assemble()
        self.assertFalse(self.output.exists())
        self.assertEqual(list(self.directory.glob('.ota-candidate-work-*')), [])

    def test_exact_gzip_and_build_record_produce_one_flat_complete_output(self):
        result = self.assemble()
        self.assertEqual(result['expanded'], blob(self.raw))
        self.assertEqual(set(p.name for p in self.output.iterdir()), set(helper.ARTIFACT_FILES))
        self.assertEqual((self.output / 'build.json').read_bytes(), self.record_path.read_bytes())
        self.assertEqual(list(self.directory.glob('.ota-candidate-work-*')), [])
        before = {p.name: p.read_bytes() for p in self.output.iterdir()}
        with self.assertRaises(ValueError):
            self.assemble()
        self.assertEqual({p.name: p.read_bytes() for p in self.output.iterdir()}, before)

    def test_unselected_source_inputs_output_or_command_claims_stop_before_validator(self):
        original = copy.deepcopy(self.record)
        for change in (lambda r: r.update(source_commit='b' * 40),
                       lambda r: r.update(layers={'unreviewed': {}}),
                       lambda r: r.update(command='untrusted executable'),
                       lambda r: r['output'].update(name='../../other.gz'),
                       lambda r: r['output'].update(name='$(untrusted).ext4.gz'),
                       lambda r: r['output']['image_identity']['source'].update(dirty=True),
                       lambda r: r['generated_configuration'].pop('local.conf')):
            self.record = copy.deepcopy(original)
            change(self.record)
            self.save()
            with patch.object(helper.subprocess, 'run') as run:
                with self.assertRaises(ValueError):
                    helper.assemble(self.policy_path, self.record_path, self.gzip,
                                    Path('/usr/bin/true'), self.output)
                run.assert_not_called()

    def test_corrupt_truncated_and_oversized_gzip_leave_no_output(self):
        self.gzip.write_bytes(self.gz[:-1] + bytes([self.gz[-1] ^ 1]))
        self.assert_clean_failure()
        for bad in (b'not a gzip', self.gz[:-5]):
            self.gzip.write_bytes(bad)
            self.record['output']['blob'] = blob(bad)
            self.save()
            self.assert_clean_failure()
        self.gzip.write_bytes(self.gz)
        self.record['output']['blob'] = blob(self.gz)
        self.save()
        with patch.object(helper, 'MAX_ROOT_BYTES', 8192):
            self.assert_clean_failure()

    def test_mismatched_validator_inventory_identity_or_payload_is_not_published(self):
        for hook in (lambda p: (p / 'extra').write_bytes(b'unlisted'),
                     lambda p: (p / 'qualification.json').write_bytes(b'{}'),
                     lambda p: (p / 'candidate.json').write_bytes(b'{}'),
                     lambda p: (p / 'image.ext4.zst').write_bytes(b'replaced bytes'),
                     lambda p: (p / 'image.ext4.zst').unlink()):
            self.hook = hook
            self.assert_clean_failure()
        def failed(staged):
            raise subprocess.CalledProcessError(1, 'trusted validator fixture')
        self.hook = failed
        self.assert_clean_failure()

    def test_racing_output_is_preserved(self):
        def race(staged):
            self.output.mkdir()
            (self.output / 'keep').write_bytes(b'other writer')
        self.hook = race
        with self.assertRaises(FileExistsError):
            self.assemble()
        self.assertEqual([p.name for p in self.output.iterdir()], ['keep'])
        self.assertEqual((self.output / 'keep').read_bytes(), b'other writer')
        self.assertEqual(list(self.directory.glob('.ota-candidate-work-*')), [])

    def test_duplicate_oversized_linked_and_nonregular_metadata_are_rejected(self):
        self.record_path.write_bytes(b'{"schema":1,"schema":1}')
        self.assert_clean_failure()
        self.record_path.write_bytes(b' ' * (helper.ARTIFACT_FILES['build.json'] + 1))
        self.assert_clean_failure()
        self.record_path.unlink()
        self.record_path.symlink_to(self.policy_path)
        self.assert_clean_failure()
        self.record_path.unlink()
        os.mkfifo(self.record_path)
        self.assert_clean_failure()


if __name__ == '__main__':
    unittest.main()
