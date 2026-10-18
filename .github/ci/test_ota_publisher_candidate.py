"""Exercise real origin admission with a stub for the independent Rust validator.

Rust ext4/decoder tests and the actual-image smoke cover real image validation;
these fixtures exercise the joining boundary and late origin/byte substitution.
"""
import copy
from dataclasses import replace
from datetime import timedelta
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

import ota_publisher_candidate as helper
import test_ota_admission as origin_fixture
import test_ota_candidate as image_fixture


class PublisherCandidateTests(unittest.TestCase):
    save = image_fixture.CandidateTests.save

    def setUp(self):
        image_fixture.CandidateTests.setUp(self)
        self.scratch = self.directory / 'scratch'
        self.scratch.mkdir()
        self.payload = b'stub compressed image; independently checked by Rust in image tests'
        self.candidate = dict(schema=1, format='rootfs-ext4-zstd',
                              compressed=image_fixture.blob(self.payload), expanded=image_fixture.blob(self.raw))
        self.actual = dict(schema=1, image=self.identity, build_inputs=self.inputs, compiler={},
                           build_inputs_sha256=self.policy['build_inputs_sha256'], expanded=self.candidate['expanded'])
        self.claimed = copy.deepcopy(self.actual)
        self.api = origin_fixture.FakeGitHub()
        self.origin_policy = replace(origin_fixture.POLICY, files=tuple(helper.ARTIFACT_FILES.items()))
        self.origin_selection = origin_fixture.SELECTION
        self.hook = None
        self.validation_calls = 0
        self.clock = lambda: origin_fixture.NOW
        self.archive()

    def archive(self):
        values = {'candidate.json': helper.canonical(self.candidate), 'image.ext4.zst': self.payload,
                  'qualification.json': helper.canonical(self.claimed), 'build.json': helper.canonical(self.record)}
        self.api.replace_archive(origin_fixture.archive_bytes(list(values.items())))

    def validator(self, args, **kwargs):
        self.validation_calls += 1
        self.assertEqual(args[:2], [str(Path('/usr/bin/true').resolve()), 'verify'])
        self.assertEqual(kwargs['env'], {'PATH': '/usr/bin:/bin', 'LC_ALL': 'C'})
        self.assertEqual(kwargs['timeout'], 600)
        self.assertTrue(kwargs['check'])
        self.assertEqual(Path(args[4]).read_bytes(), self.payload)
        kwargs['stdout'].write(helper.canonical(self.actual))
        if self.hook:
            self.hook(Path(args[4]).parent)
        return subprocess.CompletedProcess(args, 0)

    def qualify(self):
        return helper.qualified(self.api, self.origin_policy, self.origin_selection,
            self.directory, Path('/usr/bin/true'), self.scratch, clock=self.clock)

    def patches(self):
        self.enterContext(patch.object(helper, 'from_source', return_value=self.policy))
        self.enterContext(patch.object(helper.subprocess, 'run', side_effect=self.validator))

    def rejects(self):
        with self.assertRaises((ValueError, subprocess.SubprocessError)):
            with self.qualify():
                self.fail('Invalid artifact was qualified')
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_origin_and_actual_bytes_are_checked_then_private_work_is_removed(self):
        self.patches()
        with self.qualify() as result:
            self.assertEqual(result.qualification, self.actual)
            self.assertEqual(result.origin['source'], origin_fixture.SOURCE)
            self.assertEqual(result.checked_at, '2026-10-16T12:30:00Z')
            self.assertEqual(result.candidate, self.candidate)
            self.assertTrue((result.directory / 'image.ext4.zst').is_file())
            self.assertEqual(self.validation_calls, 1)
            self.assertEqual(self.api.calls.count('artifact'), 3)
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_failed_ci_or_foreign_build_stops_before_validator(self):
        self.patches()
        for field, value in (('conclusion', 'failure'), ('event', 'pull_request'), ('head_sha', 'b' * 40)):
            before = copy.deepcopy(self.api.runs[11])
            self.api.runs[11][field] = value
            self.rejects()
            self.api.runs[11] = before
        self.assertEqual(self.validation_calls, 0)

    def test_wrong_input_digest_or_payload_descriptor_stops_before_validator(self):
        self.patches()
        self.record['layers'] = {'unreviewed': {}}
        self.archive()
        self.rejects()
        self.record['layers'] = {}
        self.candidate['compressed'] = image_fixture.blob(b'other image')
        self.archive()
        self.rejects()
        self.assertEqual(self.validation_calls, 0)

    def test_builder_report_is_not_used_as_independent_image_approval(self):
        self.patches()
        self.claimed['compiler'] = {'self_asserted': 'passing'}
        self.archive()
        self.rejects()
        self.assertEqual(self.validation_calls, 1)
        self.claimed = copy.deepcopy(self.actual)
        self.archive()
        def failure(_):
            raise subprocess.CalledProcessError(1, 'reviewed image validator')
        self.hook = failure
        self.rejects()

    def test_changed_ci_attempt_artifact_or_local_payload_after_validation_fails(self):
        self.patches()
        for hook in (lambda _: self.api.runs[10].update(run_attempt=3),
                     lambda _: self.api.asset.update(expired=True),
                     lambda directory: (directory / 'image.ext4.zst').write_bytes(b'changed'),
                     lambda directory: (directory / 'extra').write_bytes(b'unlisted')):
            self.hook = hook
            self.rejects()
            self.api = origin_fixture.FakeGitHub()
            self.archive()

    def test_changed_source_checkout_policy_after_validation_fails(self):
        with patch.object(helper, 'from_source', side_effect=[self.policy, self.policy | {'source_commit': 'b' * 40}]), \
                patch.object(helper.subprocess, 'run', side_effect=self.validator):
            self.rejects()

    def test_backwards_time_or_expiry_during_validation_fails(self):
        self.patches()
        for duration in (timedelta(seconds=-1), timedelta(days=2)):
            now = [origin_fixture.NOW]
            self.clock = lambda: now[0]
            self.hook = lambda _, delay=duration: now.__setitem__(0, origin_fixture.NOW + delay)
            self.rejects()

    def test_saved_evidence_never_overwrites_and_checks_bytes_again(self):
        self.patches()
        with self.qualify() as result:
            output = self.directory / 'saved'
            helper.materialize(result, output, 'c' * 40)
            self.assertEqual(set(p.name for p in output.iterdir()), set(helper.ARTIFACT_FILES) |
                             {'origin.json', 'image-policy.json', 'publisher.json'})
            self.assertEqual((output / 'image.ext4.zst').read_bytes(), self.payload)
            with self.assertRaises(ValueError):
                helper.materialize(result, output, 'c' * 40)
            (result.directory / 'image.ext4.zst').write_bytes(b'replaced')
            with self.assertRaises(ValueError):
                helper.materialize(result, self.directory / 'bad', 'c' * 40)
            self.assertFalse((self.directory / 'bad').exists())
            self.assertEqual(list(self.directory.glob('.ota-qualified-*')), [])


if __name__ == '__main__':
    unittest.main()
