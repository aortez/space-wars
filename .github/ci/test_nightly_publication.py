"""Exercise real publisher behavior with an in-memory GitHub boundary."""
import copy
import io
import zipfile
import urllib.request
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

import publish_nightly as helper


class FakeGitHub:
    repo = 'aortez/space-wars'

    def __init__(self):
        self.release = None
        self.files = {}
        self.next_id = 1
        self.calls = []
        self.failure = None

    def operation(self, name, mutation):
        self.calls.append(name)
        if self.failure == (name, 'before'):
            self.failure = None
            raise OSError('API rejected request')
        result = mutation()
        if self.failure == (name, 'after'):
            self.failure = None
            raise OSError('API response lost')
        return copy.deepcopy(result)

    def get_release(self):
        return self.operation('get', lambda: self.release)

    def create_release(self, sha, content):
        def create():
            if self.release:
                raise ValueError('Duplicate release')
            self.release = dict(id=1, tag_name='nightly', name='Nightly release',
                                prerelease=True, draft=False, body=content, target_commitish=sha)
            return self.release
        return self.operation('create', create)

    def update_release(self, release, content):
        return self.operation('update', lambda: self.release.update(body=content))

    def assets(self, release):
        return self.operation('assets', lambda: [dict(id=i, name=v[0]) for i, v in self.files.items()])

    def upload(self, release, name, data):
        def upload():
            if any(v[0] == name for v in self.files.values()):
                raise ValueError('Duplicate asset')
            asset = dict(id=self.next_id, name=name)
            self.files[self.next_id] = [name, data]
            self.next_id += 1
            return asset
        return self.operation('upload', upload)

    def download(self, asset):
        return self.operation('download', lambda: self.files[asset['id']][1])

    def delete_asset(self, asset):
        return self.operation('delete', lambda: self.files.pop(asset['id']))

    def rename_asset(self, asset, name):
        def rename():
            self.files[asset['id']][0] = name
        return self.operation('rename', rename)


def candidate(run=100, attempt=1, source='a', data=b'validated ZIP'):
    return dict(source=source * 40, run=run, attempt=attempt, sha256=helper.digest(data))


class PublicationTests(unittest.TestCase):
    def setUp(self):
        self.api = FakeGitHub()
        self.data = b'validated ZIP'
        self.first = candidate()

    def publish(self, item=None, data=None):
        return helper.publish(self.api, item or self.first, self.data if data is None else data)

    def assert_committed(self, item, data):
        self.assertEqual(helper.parse_state(self.api.release), dict(current=item, pending=None))
        self.assertEqual(list(self.api.files.values()), [['windows-client.zip', data]])
        self.assertEqual(self.api.calls.count('create'), 1)

    def test_creation_replacement_and_unchanged_source(self):
        self.assertEqual(self.publish(), 'published')
        self.assert_committed(self.first, self.data)
        before = len(self.api.calls)
        self.assertEqual(self.publish(), 'unchanged')
        self.assertFalse({'upload', 'delete', 'update', 'rename'} & set(self.api.calls[before:]))
        second = candidate(101)  # main has not changed
        self.assertEqual(self.publish(second), 'published')
        self.assert_committed(second, self.data)
        third = candidate(102, source='b', data=b'new ZIP')
        self.publish(third, b'new ZIP')
        self.assert_committed(third, b'new ZIP')

    def test_stale_runs_and_attempts_cannot_mutate(self):
        self.publish(candidate(102, 2))
        for old in (candidate(100), candidate(102, 1)):
            before = len(self.api.calls)
            self.assertEqual(self.publish(old), 'stale')
            self.assertEqual(self.api.calls[before:], ['get'])

    def test_newer_failed_publication_reserves_watermark(self):
        self.publish()
        self.api.failure = ('upload', 'before')
        with self.assertRaises(OSError):
            self.publish(candidate(102, data=b'new'), b'new')
        self.assertEqual(self.publish(candidate(101)), 'stale')
        self.assertEqual(list(self.api.files.values()), [['windows-client.zip', self.data]])

    def test_same_run_attempt_conflicts(self):
        self.publish()
        for item in (candidate(source='b'), candidate(data=b'other')):
            with self.assertRaisesRegex(ValueError, 'conflicting'):
                self.publish(item)

    def test_conflicting_or_unmanaged_release(self):
        self.publish()
        original = copy.deepcopy(self.api.release)
        for key, value in [('name', 'Other'), ('prerelease', False), ('draft', True),
                           ('tag_name', 'v1'), ('body', 'User release')]:
            self.api.release = {**original, key: value}
            with self.assertRaises(ValueError):
                self.publish(candidate(101))
        self.assertEqual(list(self.api.files.values()), [['windows-client.zip', self.data]])

    def test_uncertain_creation_and_upload_reconcile_without_duplicates(self):
        for operation in ('create', 'upload', 'rename', 'update'):
            for when in ('before', 'after'):
                with self.subTest(operation=operation, when=when):
                    self.api = FakeGitHub()
                    self.api.failure = (operation, when)
                    with self.assertRaises(OSError):
                        self.publish()
                    self.publish()
                    self.assertEqual(len(self.api.files), 1)
                    self.assertEqual(helper.parse_state(self.api.release)['current'], self.first)
                    self.assertEqual(list(self.api.files.values()), [['windows-client.zip', self.data]])
                    if operation == 'create' and when == 'before':
                        self.assertEqual(self.api.calls.count('create'), 2)  # first never applied
                    else:
                        self.assertEqual(self.api.calls.count('create'), 1)

    def test_replacement_failures_before_and_after_every_api_operation(self):
        for operation in ('get', 'update', 'assets', 'upload', 'download', 'delete', 'rename'):
            for when in ('before', 'after'):
                with self.subTest(operation=operation, when=when):
                    self.api = FakeGitHub()
                    self.publish()
                    self.api.failure = (operation, when)
                    new = candidate(101, source='b', data=b'new')
                    with self.assertRaises(OSError):
                        self.publish(new, b'new')
                    self.publish(new, b'new')
                    self.assert_committed(new, b'new')

    def test_final_commit_response_loss(self):
        self.publish()
        original = self.api.update_release
        def final_failure(release, content):
            if '"pending": null' in content:
                self.api.failure = ('update', 'after')
            return original(release, content)
        self.api.update_release = final_failure
        new = candidate(101, data=b'new')
        with self.assertRaises(OSError):
            self.publish(new, b'new')
        self.api.update_release = original
        self.assertEqual(self.publish(new, b'new'), 'unchanged')
        self.assert_committed(new, b'new')

    def test_uploaded_corruption_is_not_committed(self):
        original = self.api.upload
        def corrupt(release, name, data):
            return original(release, name, b'truncated')
        self.api.upload = corrupt
        with self.assertRaisesRegex(ValueError, 'differs'):
            self.publish()
        self.assertIsNone(helper.parse_state(self.api.release)['current'])
        self.api.upload = original
        self.publish()
        self.assert_committed(self.first, self.data)

    def test_committed_asset_tampering_fails(self):
        self.publish()
        next(iter(self.api.files.values()))[1] = b'changed externally'
        with self.assertRaisesRegex(ValueError, 'differs'):
            self.publish()

    def test_unexpected_assets_preserved_and_error_visible(self):
        self.publish()
        self.api.files[88] = ['user-file', b'data']
        with self.assertRaisesRegex(ValueError, 'Unexpected'):
            self.publish(candidate(101))
        self.assertIn(88, self.api.files)


class ValidationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / 'software').mkdir()
        identity = dict(repository=FakeGitHub.repo, source_commit='a' * 40,
                        tested_commit='a' * 40, source_kind='head', run_id=100, run_attempt=1)
        self.info = dict(**identity, target='x86_64-pc-windows-msvc', profile='release')
        self.record = dict(**identity, status='captured', required_mode='software',
                           software_launch=dict(help_exit_code=0, sha256=helper.digest(b'wrapper')),
                           package=dict(file='windows-client.zip', executable_sha256=helper.digest(b'exe')))
        self.probe = dict(**identity, status='captured', backend_requested='software',
                          help_exit_code=0, executable_sha256=helper.digest(b'exe'))
        self.manifest = dict(**identity)
        self.make_zip()
        (self.root / 'build.log').write_text('build succeeded')
        self.save()

    def make_zip(self):
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, 'w') as archive:
            archive.writestr('build-info.json', json.dumps(self.info))
            archive.writestr('engine-client.exe', b'exe')
            archive.writestr('run-software.cmd', b'wrapper')
        self.zip = stream.getvalue()
        (self.root / 'windows-client.zip').write_bytes(self.zip)
        self.record['package'].update(sha256=helper.digest(self.zip), size_bytes=len(self.zip))

    def save(self):
        (self.root / 'release.json').write_text(json.dumps(self.record))
        (self.root / 'software/result.json').write_text(json.dumps(self.probe))
        self.manifest['files'] = {str(p.relative_to(self.root)): helper.digest(p.read_bytes())
                                  for p in self.root.rglob('*') if p.is_file() and p.name != 'manifest.json'}
        (self.root / 'manifest.json').write_text(json.dumps(self.manifest))

    def validate(self, **kwargs):
        return helper.validate(self.root, FakeGitHub.repo, 'a' * 40, 100, 1, kwargs.get('result', 'success'))

    def test_valid_evidence(self):
        self.assertEqual(self.validate(), (candidate(data=self.zip), self.zip))

    def test_failed_build_or_validation(self):
        for result in ('failure', 'cancelled', 'skipped'):
            with self.assertRaises(ValueError):
                self.validate(result=result)
        for record, key, value in ((self.record, 'status', 'failed'),
                                   (self.record, 'required_mode', 'default'),
                                   (self.probe, 'status', 'failed')):
            old = record[key]
            record[key] = value
            self.save()
            with self.assertRaises(ValueError):
                self.validate()
            record[key] = old

    def test_all_evidence_identity_fields(self):
        for record in (self.record, self.manifest):
            for key, value in [('source_commit', 'b' * 40), ('tested_commit', 'b' * 40),
                               ('run_id', 101), ('run_attempt', 2), ('repository', 'other/repo'),
                               ('source_kind', 'merge')]:
                with self.subTest(key=key):
                    old = record[key]
                    record[key] = value
                    self.save()
                    with self.assertRaises(ValueError):
                        self.validate()
                    record[key] = old
        for key, value in [('source_commit', 'b' * 40), ('run_id', 101), ('run_attempt', 2)]:
            old = self.probe[key]
            self.probe[key] = value
            self.save()
            with self.assertRaises(ValueError):
                self.validate()
            self.probe[key] = old

    def test_bundled_source_and_run_identity(self):
        for key, value in [('source_commit', 'b' * 40), ('run_id', 999), ('run_attempt', 2),
                           ('target', 'linux'), ('profile', 'debug')]:
            old = self.info[key]
            self.info[key] = value
            self.make_zip()
            self.save()
            with self.assertRaisesRegex(ValueError, 'Bundled'):
                self.validate()
            self.info[key] = old

    def test_executable_and_launcher_identity(self):
        for record, key in [(self.record['package'], 'executable_sha256'),
                            (self.record['software_launch'], 'sha256'),
                            (self.probe, 'executable_sha256')]:
            old = record[key]
            record[key] = '0' * 64
            self.save()
            with self.assertRaises(ValueError):
                self.validate()
            record[key] = old

    def test_package_hash_size_and_manifest_integrity(self):
        for key, value in [('sha256', 'f' * 64), ('size_bytes', 42), ('file', 'other.zip')]:
            old = self.record['package'][key]
            self.record['package'][key] = value
            self.save()
            with self.assertRaises(ValueError):
                self.validate()
            self.record['package'][key] = old
        self.save()
        (self.root / 'build.log').write_text('tampered')
        with self.assertRaisesRegex(ValueError, 'digest'):
            self.validate()


class HTTPAdapterTests(unittest.TestCase):
    """Use the production adapter; only replace the opener's HTTP transport."""

    def setUp(self):
        transport = patch.object(helper.urllib.request, 'build_opener')
        self.opener = transport.start().return_value
        self.addCleanup(transport.stop)
        self.api = helper.GitHub('aortez/space-wars', 'test-token')
        self.release = dict(id=7, upload_url='https://uploads.github.com/repos/aortez/space-wars/releases/7/assets{?name,label}')

    def request(self):
        request = self.opener.open.call_args.args[0]
        self.assertEqual(self.opener.open.call_args.kwargs, {'timeout': 120})
        self.assertEqual(request.get_header('Authorization'), 'Bearer test-token')
        self.assertEqual(request.get_header('X-github-api-version'), '2022-11-28')
        return request

    def test_upload_metadata_then_verify_binary_download(self):
        metadata = dict(id=123, name='candidate name.zip', size=3)
        self.opener.open.side_effect = [io.BytesIO(json.dumps(metadata).encode()), io.BytesIO(b'zip')]
        asset = self.api.upload(self.release, metadata['name'], b'zip')
        self.assertEqual(asset, metadata)
        request = self.request()
        self.assertEqual(request.get_method(), 'POST')
        self.assertEqual(request.full_url, 'https://uploads.github.com/repos/aortez/space-wars/releases/7/assets?name=candidate%20name.zip')
        self.assertEqual(request.data, b'zip')
        self.assertEqual(request.get_header('Content-type'), 'application/zip')
        self.assertEqual(request.get_header('Accept'), 'application/vnd.github+json')
        helper.verify_asset(self.api, asset, b'zip')
        request = self.request()
        self.assertEqual(request.get_method(), 'GET')
        self.assertEqual(request.full_url, self.api.base + '/releases/assets/123')
        self.assertIsNone(request.data)
        self.assertEqual(request.get_header('Accept'), 'application/octet-stream')
        self.assertEqual(self.opener.open.call_count, 2)

    def test_download_preserves_non_json_non_utf8_bytes(self):
        data = b'PK\x03\x04\xff\x00binary ZIP'
        self.opener.open.return_value = io.BytesIO(data)
        self.assertEqual(self.api.download({'id': 123}), data)
        self.assertIsNone(self.request().data)

    def test_release_creation_sends_json_and_decodes_metadata(self):
        metadata = dict(id=7, **{'name': 'Nightly release'})
        self.opener.open.return_value = io.BytesIO(json.dumps(metadata).encode())
        self.assertEqual(self.api.create_release('a' * 40, 'notes'), metadata)
        request = self.request()
        self.assertEqual(request.get_method(), 'POST')
        self.assertEqual(request.full_url, self.api.base + '/releases')
        self.assertEqual(request.get_header('Content-type'), 'application/json')
        self.assertEqual(request.get_header('Accept'), 'application/vnd.github+json')
        self.assertEqual(json.loads(request.data), dict(tag_name='nightly', target_commitish='a' * 40,
                         name='Nightly release', prerelease=True, draft=False, body='notes'))

    def test_rename_json_and_empty_delete_response(self):
        self.opener.open.side_effect = [io.BytesIO(b'{"id":123,"name":"windows-client.zip"}'), io.BytesIO(b'')]
        self.api.rename_asset({'id': 123}, 'windows-client.zip')
        request = self.request()
        self.assertEqual(request.get_method(), 'PATCH')
        self.assertEqual(json.loads(request.data), {'name': 'windows-client.zip'})
        self.api.delete_asset({'id': 123})
        request = self.request()
        self.assertEqual(request.get_method(), 'DELETE')
        self.assertEqual(request.full_url, self.api.base + '/releases/assets/123')
        self.assertIsNone(request.data)

    def test_only_release_lookup_404_means_absent(self):
        error = urllib.error.HTTPError(self.api.base, 404, 'Not found', {}, None)
        self.addCleanup(error.close)
        self.opener.open.side_effect = error
        self.assertIsNone(self.api.get_release())
        with self.assertRaises(urllib.error.HTTPError) as raised:
            self.api.download({'id': 123})
        self.assertIs(raised.exception, error)
        with self.assertRaises(urllib.error.HTTPError):
            self.api.upload(self.release, 'candidate.zip', b'zip')

    def test_http_and_transport_errors_propagate_without_retry(self):
        for error in (urllib.error.HTTPError(self.api.base, 403, 'Forbidden', {}, None),
                      urllib.error.HTTPError(self.api.base, 500, 'Server error', {}, None),
                      urllib.error.URLError('response lost')):
            if isinstance(error, urllib.error.HTTPError):
                self.addCleanup(error.close)
            for operation in (self.api.get_release,
                              lambda: self.api.upload(self.release, 'candidate.zip', b'zip'),
                              lambda: self.api.download({'id': 123})):
                with self.subTest(error=error, operation=operation):
                    self.opener.open.reset_mock()
                    self.opener.open.side_effect = error
                    with self.assertRaises(type(error)) as raised:
                        operation()
                    self.assertIs(raised.exception, error)
                    self.assertEqual(self.opener.open.call_count, 1)

    def test_malformed_upload_metadata_fails_visibly(self):
        self.opener.open.return_value = io.BytesIO(b'incomplete JSON response')
        with self.assertRaises(json.JSONDecodeError):
            self.api.upload(self.release, 'candidate.zip', b'zip')


class RedirectTests(unittest.TestCase):
    def test_signed_asset_redirect_does_not_forward_authorization(self):
        request = urllib.request.Request('https://api.github.com/repos/x/y/releases/assets/1',
                                         headers={'Authorization': 'Bearer fake'})
        redirect = helper.DownloadRedirect().redirect_request(
            request, None, 302, '', {}, 'https://release-assets.githubusercontent.com/asset')
        self.assertIsNone(redirect.get_header('Authorization'))
        with self.assertRaises(ValueError):
            helper.DownloadRedirect().redirect_request(request, None, 302, '', {}, 'http://host/asset')


if __name__ == '__main__':
    unittest.main()
