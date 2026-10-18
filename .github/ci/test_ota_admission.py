"""Origin/adversarial transport fixtures, not production image qualification."""
import base64
import copy
from dataclasses import replace
from datetime import timedelta
import io
import json
from pathlib import Path
import stat
import struct
import tempfile
import unittest
import urllib.error
from unittest.mock import patch
import warnings
import zipfile

import ota_admission as helper

SOURCE = 'a' * 40
NOW = helper.timestamp('2026-10-16T12:30:00Z')
BUILD_FILE = b'reviewed OTA workflow fixture'
CI_FILE = b'reviewed CI workflow fixture'
POLICY = helper.Policy(
    build=helper.Workflow('.github/workflows/ota-build.yml', helper.digest(BUILD_FILE),
                          ('schedule', 'workflow_dispatch'), (('Build image', ('Build', 'Qualify', 'Upload')),)),
    ci=helper.Workflow('.github/workflows/ci.yml', helper.digest(CI_FILE), ('push',),
                       (('Linux', ('Tests',)), ('Pi', ('Installer tests',)))),
    producer_job='Build image', files=(('candidate.json', 1024), ('image.ext4.zst', 4096)))
SELECTION = helper.Selection(SOURCE, 10, 2, 11, 1, 12)


def archive_bytes(entries=None, compression=zipfile.ZIP_DEFLATED):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, 'w', compression=compression) as archive:
        for name, data in entries or [('candidate.json', b'{"untrusted":"metadata"}'),
                                     ('image.ext4.zst', b'untrusted image bytes')]:
            archive.writestr(name, data)
    return stream.getvalue()


class FakeGitHub:
    def __init__(self):
        self.calls = []
        self.hook = None
        self.runs, self.exact, self.job_lists, self.workflows, self.files = {}, {}, {}, {}, {}
        for workflow, run, attempt, workflow_id, contents in (
                (POLICY.build, 10, 2, 101, BUILD_FILE), (POLICY.ci, 11, 1, 102, CI_FILE)):
            self.workflows[workflow.path] = dict(id=workflow_id, path=workflow.path, state='active')
            self.files[workflow.path] = contents
            self.runs[run] = dict(
                id=run, run_number=run, run_attempt=attempt, workflow_id=workflow_id, path=workflow.path,
                head_sha=SOURCE, head_branch='main', event=workflow.events[0], status='completed',
                conclusion='success', referenced_workflows=[], head_commit=dict(id=SOURCE),
                repository=dict(id=helper.REPOSITORY_ID, full_name=helper.REPOSITORY),
                head_repository=dict(id=helper.REPOSITORY_ID, full_name=helper.REPOSITORY),
                created_at='2026-10-16T11:00:00Z', run_started_at='2026-10-16T12:00:00Z',
                updated_at='2026-10-16T12:20:00Z')
            self.job_lists[run] = [dict(
                id=run * 100 + index, name=name, run_id=run, run_attempt=attempt, head_sha=SOURCE,
                status='completed', conclusion='success', started_at='2026-10-16T12:01:00Z',
                completed_at='2026-10-16T12:19:00Z',
                steps=[dict(name=step, number=number, status='completed', conclusion='success')
                       for number, step in enumerate(steps, 1)])
                for index, (name, steps) in enumerate(workflow.jobs)]
        self.exact = copy.deepcopy(self.runs)
        self.data = archive_bytes()
        self.asset = dict(id=12, name=f'ota-candidate-{SOURCE}-10-2', expired=False,
                          size_in_bytes=len(self.data), digest='sha256:' + helper.digest(self.data),
                          created_at='2026-10-16T12:18:00Z', expires_at='2026-10-17T12:18:00Z',
                          workflow_run=dict(id=10, repository_id=helper.REPOSITORY_ID,
                                            head_repository_id=helper.REPOSITORY_ID,
                                            head_branch='main', head_sha=SOURCE))

    def call(self, name, result):
        self.calls.append(name)
        return copy.deepcopy(result)

    def workflow(self, path):
        return self.call('workflow', self.workflows[path])

    def file(self, path, source):
        assert source == SOURCE
        return self.call('file', self.files[path])

    def run(self, run, attempt=None):
        return self.call('run' if attempt is None else 'attempt',
                         self.runs[run] if attempt is None else self.exact[run])

    def jobs(self, run, attempt):
        return self.call('jobs', self.job_lists[run])

    def workflow_runs(self, path, source):
        return self.call('workflow_runs', [run for run in self.runs.values()
                         if run['path'] == path and run['head_sha'] == source])

    def artifact(self, artifact):
        assert artifact == 12
        return self.call('artifact', self.asset)

    def download(self, artifact, expected, target):
        self.calls.append('download')
        target.write(self.data)
        if self.hook:
            self.hook(self)

    def replace_archive(self, data):
        self.data = data
        self.asset.update(size_in_bytes=len(data), digest='sha256:' + helper.digest(data))


class AdmissionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.scratch = Path(self.temporary.name)
        self.api = FakeGitHub()

    def admit(self, policy=POLICY, selection=SELECTION, clock=lambda: NOW):
        return helper.candidate(self.api, policy, selection, self.scratch, clock)

    def rejects(self, **kwargs):
        with self.assertRaises((ValueError, zipfile.BadZipFile)):
            with self.admit(**kwargs):
                self.fail('Invalid candidate was admitted')
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_success_records_both_exact_attempts_and_verified_members_then_cleans(self):
        with self.admit() as (directory, receipt):
            self.assertEqual(receipt['scope'], 'github-artifact-origin')
            self.assertEqual(receipt['source'], SOURCE)
            self.assertEqual(receipt['build']['run_attempt'], 2)
            self.assertEqual(receipt['ci']['run_attempt'], 1)
            self.assertEqual(receipt['artifact']['sha256'], helper.digest(self.api.data))
            for name, blob in receipt['files'].items():
                data = (directory / name).read_bytes()
                self.assertEqual(blob, dict(bytes=len(data), sha256=helper.digest(data)))
                self.assertFalse((directory / name).stat().st_mode & 0o111)
            self.assertEqual(directory.stat().st_mode & 0o777, 0o700)
            self.assertEqual(self.api.calls.count('run'), 4)
            self.assertEqual(self.api.calls.count('attempt'), 4)
            self.assertEqual(self.api.calls.count('artifact'), 2)
        self.assertFalse(directory.exists())

    def test_rejects_untrusted_events_refs_sources_repositories_and_reusable_workflows(self):
        mutations = [('event', 'pull_request'), ('event', 'pull_request_target'),
                     ('event', 'workflow_run'), ('event', 'repository_dispatch'),
                     ('head_branch', 'feature'), ('head_sha', 'b' * 40),
                     ('head_commit', dict(id='b' * 40)), ('path', '.github/workflows/other.yml'),
                     ('workflow_id', 999), ('id', 999), ('run_attempt', 1),
                     ('repository', dict(id=123, full_name=helper.REPOSITORY)),
                     ('head_repository', dict(id=helper.REPOSITORY_ID, full_name='fork/space-wars')),
                     ('referenced_workflows', [{'sha': SOURCE}])]
        for run in (10, 11):
            for field, value in mutations:
                with self.subTest(run=run, field=field, value=value):
                    self.api = FakeGitHub()
                    # CI's selected attempt is already one.
                    if field == 'run_attempt':
                        value = 3
                    self.api.runs[run][field] = value
                    self.rejects()
                    self.assertNotIn('download', self.api.calls)

    def test_failed_skipped_pending_or_cancelled_run_never_downloads(self):
        for run in (10, 11):
            for status, conclusion in [('completed', 'failure'), ('completed', 'cancelled'),
                                       ('completed', 'skipped'), ('queued', None), ('in_progress', None)]:
                with self.subTest(run=run, conclusion=conclusion):
                    self.api = FakeGitHub()
                    self.api.runs[run].update(status=status, conclusion=conclusion)
                    self.rejects()
                    self.assertNotIn('download', self.api.calls)

    def test_attempt_endpoint_and_current_run_must_agree(self):
        self.api.exact[10]['event'] = 'workflow_dispatch'
        self.rejects()

    def test_newer_failed_pending_cancelled_or_successful_build_cannot_be_bypassed(self):
        for status, conclusion in [('completed', 'failure'), ('completed', 'cancelled'),
                                   ('queued', None), ('in_progress', None), ('completed', 'success')]:
            with self.subTest(status=status, conclusion=conclusion):
                self.api = FakeGitHub()
                self.api.runs[20] = dict(self.api.runs[10], id=20, run_number=20,
                                         status=status, conclusion=conclusion)
                self.rejects()
                self.assertNotIn('download', self.api.calls)

    def test_new_build_appearing_during_download_invalidates_admission(self):
        self.api.hook = lambda api: api.runs.update({20: dict(api.runs[10], id=20, run_number=20,
                                                           status='in_progress', conclusion=None)})
        self.rejects()
        self.assertIn('download', self.api.calls)

    def test_history_is_sorted_by_run_number_and_duplicate_or_foreign_entries_reject(self):
        self.api.runs[9] = dict(self.api.runs[10], id=9, run_number=9, conclusion='failure')
        with self.admit():
            pass
        for field, value in [('run_number', 10), ('id', 10), ('head_branch', 'topic')]:
            self.api.runs[9] = dict(self.api.runs[10], id=9, run_number=9, conclusion='failure')
            self.api.runs[9][field] = value
            self.rejects()

    def test_workflow_file_and_registration_are_independently_checked(self):
        for path in (POLICY.build.path, POLICY.ci.path):
            with self.subTest(path=path):
                self.api = FakeGitHub()
                self.api.files[path] += b' altered'
                self.rejects()
                self.api = FakeGitHub()
                self.api.workflows[path]['state'] = 'disabled_manually'
                self.rejects()

    def test_run_and_job_time_bounds_reject_old_attempts_and_future_results(self):
        for field, value in [('created_at', '2026-10-01T12:00:00Z'),
                             ('run_started_at', '2026-10-16T12:21:00Z'),
                             ('updated_at', '2026-10-16T12:31:00Z'),
                             ('updated_at', 'not a timestamp')]:
            with self.subTest(field=field):
                self.api = FakeGitHub()
                self.api.runs[10][field] = value
                self.rejects()
        for field, value in [('started_at', '2026-10-16T11:00:00Z'),
                             ('completed_at', '2026-10-16T12:21:00Z')]:
            self.api = FakeGitHub()
            self.api.job_lists[10][0][field] = value
            self.rejects()

    def test_required_jobs_cannot_be_missing_duplicated_skipped_or_foreign(self):
        for run in (10, 11):
            for field, value in [('name', 'Other'), ('conclusion', 'skipped'), ('status', 'queued'),
                                 ('run_attempt', 9), ('run_id', 9), ('head_sha', 'b' * 40)]:
                with self.subTest(run=run, field=field):
                    self.api = FakeGitHub()
                    self.api.job_lists[run][0][field] = value
                    self.rejects()
        self.api = FakeGitHub()
        self.api.job_lists[11][1] = copy.deepcopy(self.api.job_lists[11][0])
        self.rejects()
        self.api = FakeGitHub()
        self.api.job_lists[11].pop()
        self.rejects()

    def test_required_steps_cannot_be_skipped_and_failed_steps_are_rejected(self):
        for field, value in [('name', 'Other'), ('conclusion', 'skipped'),
                             ('conclusion', 'failure'), ('status', 'in_progress')]:
            with self.subTest(field=field, value=value):
                self.api = FakeGitHub()
                self.api.job_lists[10][0]['steps'][1][field] = value
                self.rejects()
        self.api = FakeGitHub()
        self.api.job_lists[10][0]['steps'].append(copy.deepcopy(self.api.job_lists[10][0]['steps'][0]))
        self.rejects()

    def test_optional_failure_report_step_can_be_skipped(self):
        self.api.job_lists[10][0]['steps'].append(
            dict(name='Upload failure logs', number=4, status='completed', conclusion='skipped'))
        with self.admit():
            pass

    def test_artifact_identity_digest_sizes_expiry_and_attempt_window(self):
        for field, value in [('id', 999), ('name', f'ota-candidate-{SOURCE}-10-1'),
                             ('expired', True), ('size_in_bytes', 0), ('size_in_bytes', True),
                             ('size_in_bytes', helper.MAX_ARCHIVE_BYTES + 1), ('digest', None),
                             ('digest', 'sha1:' + 'a' * 64), ('digest', 'sha256:' + 'a' * 63),
                             ('created_at', '2026-10-16T11:59:59Z'),
                             ('created_at', '2026-10-16T12:19:01Z'),
                             ('expires_at', '2026-10-16T12:30:00Z')]:
            with self.subTest(field=field, value=value):
                self.api = FakeGitHub()
                self.api.asset[field] = value
                self.rejects()
        for field, value in [('id', 99), ('repository_id', 99), ('head_repository_id', 99),
                             ('head_sha', 'b' * 40), ('head_branch', 'feature')]:
            self.api = FakeGitHub()
            self.api.asset['workflow_run'][field] = value
            self.rejects()

    def test_archive_bytes_are_checked_even_if_transport_does_not_verify(self):
        for changed in (self.api.data + b'extra', self.api.data[:-1],
                        bytes([self.api.data[0] ^ 1]) + self.api.data[1:]):
            self.api = FakeGitHub()
            self.api.data = changed
            self.rejects()

    def test_changed_build_ci_or_artifact_after_download_never_yields_data(self):
        for hook in (lambda api: api.runs[10].update(run_attempt=3),
                     lambda api: api.runs[11].update(run_attempt=2),
                     lambda api: api.asset.update(expired=True),
                     lambda api: api.asset.update(digest='sha256:' + 'f' * 64),
                     lambda api: api.job_lists[10][0].update(id=9000)):
            self.api = FakeGitHub()
            self.api.hook = hook
            self.rejects()

    def test_expiry_at_final_read_and_backwards_clock_reject(self):
        for finished in (NOW + timedelta(days=2), NOW - timedelta(seconds=1)):
            times = iter([NOW, NOW, NOW, finished])
            self.rejects(clock=lambda: next(times))

    def test_zip_paths_symlinks_empty_files_and_decompression_size_are_rejected(self):
        link = zipfile.ZipInfo('candidate.json')
        link.create_system = 3
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        for name, data in [('../escape', b'x'), ('/absolute', b'x'), ('nested/file', b'x'),
                           ('candidate.json\x00ignored', b'x'), (link, b'image.ext4.zst'),
                           ('candidate.json', b''), ('candidate.json', b'x' * 1025)]:
            with self.subTest(name=name):
                self.api = FakeGitHub()
                value = archive_bytes([(name, data), ('image.ext4.zst', b'payload')])
                # zipfile itself truncates NUL names when writing; exercise the
                # on-wire form by modifying equal-length central/local names.
                if isinstance(name, str) and '\x00' in name:
                    value = archive_bytes([('candidate.jsonx', data), ('image.ext4.zst', b'payload')])
                    value = value.replace(b'candidate.jsonx', b'candidate.json\x00')
                self.api.replace_archive(value)
                self.rejects()

    def test_zip_duplicate_extra_and_unsupported_compression_members_are_rejected(self):
        for entries, compression in [([('candidate.json', b'x'), ('candidate.json', b'y')], 0),
                                     ([('candidate.json', b'x')], 0),
                                     ([('candidate.json', b'x'), ('image.ext4.zst', b'y'), ('extra', b'z')], 0),
                                     (None, zipfile.ZIP_BZIP2)]:
            self.api = FakeGitHub()
            with warnings.catch_warnings():
                warnings.simplefilter('ignore', UserWarning)
                self.api.replace_archive(archive_bytes(entries, compression))
            self.rejects()

    def test_zip_encryption_crc_and_directory_attributes_are_rejected(self):
        entry = zipfile.ZipInfo('candidate.json')
        entry.create_system = 0
        entry.external_attr = 0x10
        self.api.replace_archive(archive_bytes([(entry, b'x'), ('image.ext4.zst', b'y')]))
        self.rejects()
        for encrypted in (True, False):
            data = bytearray(archive_bytes(compression=zipfile.ZIP_STORED))
            central = data.index(b'PK\x01\x02')
            if encrypted:
                struct.pack_into('<H', data, central + 8, 1)
            else:
                struct.pack_into('<I', data, central + 16, 0)
            self.api.replace_archive(bytes(data))
            self.rejects()

    def test_bounded_zip_reads_support_streaming_zip64_and_large_regular_members(self):
        stream = io.BytesIO()
        payload = bytes(range(256)) * 8192
        with zipfile.ZipFile(stream, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr('candidate.json', b'{}')
            with archive.open('image.ext4.zst', 'w', force_zip64=True) as member:
                member.write(payload)
        self.api.replace_archive(stream.getvalue())
        policy = replace(POLICY, files=(('candidate.json', 1024), ('image.ext4.zst', len(payload))))
        with self.admit(policy=policy) as (directory, receipt):
            self.assertEqual((directory / 'image.ext4.zst').read_bytes(), payload)
            self.assertEqual(receipt['files']['image.ext4.zst']['sha256'], helper.digest(payload))

    def test_consumer_failure_cleans_verified_files(self):
        with self.assertRaisesRegex(RuntimeError, 'qualification failed'):
            with self.admit():
                raise RuntimeError('qualification failed')
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_zip_central_directory_allocation_is_bounded_before_parsing(self):
        data = bytearray(self.api.data)
        end = data.rfind(b'PK\x05\x06')
        struct.pack_into('<I', data, end + 12, 512 * 1024 * 1024)
        self.api.replace_archive(bytes(data))
        self.rejects()
        # A real large directory is rejected as well, before allocating records.
        self.api.replace_archive(archive_bytes([(f'item-{number:05}', b'x') for number in range(3000)]))
        self.rejects()

    def test_malformed_trusted_policy_and_selection_fail_before_any_network(self):
        for policy in (replace(POLICY, build=replace(POLICY.build, events=('pull_request',))),
                       replace(POLICY, build=replace(POLICY.build, path='.github/workflows/other.yml')),
                       replace(POLICY, files=(('../escape', 10),)),
                       replace(POLICY, files=(('receipt.json', 10),)),
                       replace(POLICY, max_age_hours=169), replace(POLICY, producer_job='Other')):
            self.rejects(policy=policy)
            self.assertEqual(self.api.calls, [])
        for selection in (replace(SELECTION, source='HEAD'), replace(SELECTION, artifact=True),
                          replace(SELECTION, build_attempt=0)):
            self.rejects(selection=selection)
            self.assertEqual(self.api.calls, [])


class Response(io.BytesIO):
    def __init__(self, data, status=200, headers=None):
        super().__init__(data)
        self.status = status
        self.headers = headers or {}


class Opener:
    def __init__(self, responses):
        self.responses = iter(responses)
        self.requests = []

    def open(self, request, timeout):
        self.requests.append((request, timeout))
        result = next(self.responses)
        if isinstance(result, Exception):
            raise result
        return result


class TransportTests(unittest.TestCase):
    def setUp(self):
        self.api = helper.GitHub('test-token-not-a-secret')

    def redirect(self, location='https://cdn.example/artifact.zip?temporary-secret'):
        return urllib.error.HTTPError('https://api.github.com/api', 302, 'Found',
                                      {'Location': location}, io.BytesIO())

    def test_download_binds_github_digest_and_strips_api_auth_at_redirect(self):
        payload = b'archive data'
        self.api.api_opener = Opener([self.redirect()])
        self.api.public_opener = Opener([Response(payload)])
        target = io.BytesIO()
        self.api.download(12, dict(bytes=len(payload), sha256=helper.digest(payload)), target)
        self.assertEqual(target.getvalue(), payload)
        request = self.api.api_opener.requests[0][0]
        self.assertEqual(request.full_url, f'https://api.github.com/repos/{helper.REPOSITORY}/actions/artifacts/12/zip')
        self.assertEqual(request.get_header('Authorization'), 'Bearer test-token-not-a-secret')
        public = self.api.public_opener.requests[0][0]
        self.assertIsNone(public.get_header('Authorization'))
        self.assertEqual(public.get_header('Accept-encoding'), 'identity')

    def test_api_redirect_handler_never_follows_automatically(self):
        self.assertIsNone(helper.NoRedirect().redirect_request(None, None, 302, '', {}, 'https://other'))

    def test_redirect_location_rejects_credentials_http_fragments_and_missing_host(self):
        for location in ('http://cdn.example/a', 'https://user:pass@cdn.example/a',
                         'https://cdn.example/a#fragment', '//cdn.example/a', 'https:///a',
                         'https://cdn.example/' + 'a' * 8200):
            self.api.api_opener = Opener([self.redirect(location)])
            with self.assertRaises(ValueError):
                self.api.download(12, dict(bytes=1, sha256='a' * 64), io.BytesIO())

    def test_download_rejects_wrong_size_digest_encoding_and_redacts_cdn_errors(self):
        for response in (Response(b'x'), Response(b'payload extra'), Response(b'xxxxxxx'),
                         Response(b'payload', headers={'Content-Encoding': 'gzip'}),
                         urllib.error.HTTPError('https://cdn.example/?temporary-secret', 403, 'Forbidden', {}, None)):
            self.api.api_opener = Opener([self.redirect()])
            self.api.public_opener = Opener([response])
            with self.assertRaises((ValueError, OSError)) as caught:
                self.api.download(12, dict(bytes=7, sha256=helper.digest(b'payload')), io.BytesIO())
            self.assertNotIn('temporary-secret', str(caught.exception))
            self.assertNotIn('test-token', str(caught.exception))
            self.assertTrue(response.closed)

    def test_api_json_rejects_redirects_duplicates_excess_size_and_non_objects(self):
        for response in (self.redirect(), Response(b'{"id":1,"id":2}'), Response(b'[]'),
                         Response(b'x' * (helper.MAX_JSON_BYTES + 1)),
                         Response(b'{}', headers={'Content-Encoding': 'gzip'})):
            self.api.api_opener = Opener([response])
            with self.assertRaises((ValueError, OSError)):
                self.api.json('/actions/runs/10')
            self.assertTrue(response.closed)

    def test_workflow_file_decodes_exact_bounded_bytes(self):
        content = b'name: reviewed\n'
        value = dict(path=POLICY.build.path, type='file', encoding='base64', size=len(content),
                     content=base64.b64encode(content).decode())
        self.api.api_opener = Opener([Response(json.dumps(value).encode())])
        self.assertEqual(self.api.file(POLICY.build.path, SOURCE), content)
        request = self.api.api_opener.requests[0][0]
        self.assertTrue(request.full_url.endswith(POLICY.build.path + '?ref=' + SOURCE))
        value['size'] += 1
        self.api.api_opener = Opener([Response(json.dumps(value).encode())])
        with self.assertRaises(ValueError):
            self.api.file(POLICY.build.path, SOURCE)

    def test_job_pagination_is_bounded_and_cannot_hide_missing_jobs(self):
        for value in (dict(total_count=101, jobs=[{}] * 100), dict(total_count=2, jobs=[{}]),
                      dict(total_count=True, jobs=[{}]), dict(total_count=0, jobs=[])):
            self.api.api_opener = Opener([Response(json.dumps(value).encode())])
            with self.assertRaises(ValueError):
                self.api.jobs(10, 2)

    def test_build_history_is_source_bound_and_incomplete_pagination_fails_closed(self):
        for value in (dict(total_count=101, workflow_runs=[{}] * 100),
                      dict(total_count=2, workflow_runs=[{}]),
                      dict(total_count=True, workflow_runs=[{}]),
                      dict(total_count=0, workflow_runs=[])):
            self.api.api_opener = Opener([Response(json.dumps(value).encode())])
            with self.assertRaises(ValueError):
                self.api.workflow_runs(POLICY.build.path, SOURCE)
        self.api.api_opener = Opener([Response(json.dumps(dict(total_count=1, workflow_runs=[{'id': 10}])).encode())])
        self.assertEqual(self.api.workflow_runs(POLICY.build.path, SOURCE), [{'id': 10}])
        url = self.api.api_opener.requests[0][0].full_url
        self.assertIn(f'ota-build.yml/runs?head_sha={SOURCE}&branch=main&per_page=100', url)
        self.assertNotIn('status=', url)  # Failed and unfinished runs must remain visible.

    def test_session_deadline_prevents_further_network(self):
        self.api.deadline = 0
        with self.assertRaises(TimeoutError):
            self.api.json('/actions/runs/10')

    def test_deadline_is_checked_between_partial_response_reads(self):
        response = Response(b'{}')
        self.api.api_opener = Opener([response])
        with patch.object(self.api, 'timeout', side_effect=[30, 30, TimeoutError('expired')]):
            with self.assertRaises(TimeoutError):
                self.api.json('/actions/runs/10')
        self.api.api_opener = Opener([self.redirect()])
        self.api.public_opener = Opener([Response(b'x' * (helper.CHUNK + 1))])
        with patch.object(self.api, 'timeout', side_effect=[30, 30, 30, TimeoutError('expired')]):
            with self.assertRaises(TimeoutError):
                self.api.download(12, dict(bytes=helper.CHUNK + 1, sha256='a' * 64), io.BytesIO())


if __name__ == '__main__':
    unittest.main()
