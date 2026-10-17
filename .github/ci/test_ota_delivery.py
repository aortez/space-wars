"""Delivery transaction tests. Fake metadata here is intentionally unsigned;
Rust publication_tests separately exercise actual TUF signatures and export one
real package through this boundary. None of these tests contacts GitHub.
"""
import copy
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
import urllib.error
import urllib.request
from unittest.mock import patch

import ota_delivery as helper


class FakeGitHub:
    repo = helper.REPOSITORY
    tag = helper.TAG

    def __init__(self):
        self.release = None
        self.files = {}
        self.next_id = 1
        self.calls = []
        self.failure = None
        self.hook = None
        self.starter = None
        self.corrupt = None

    def operation(self, name, operation):
        index = len(self.calls)
        self.calls.append(name)
        if self.failure == (index, 'before'):
            self.failure = None
            raise OSError('Request failed')
        result = operation()
        if self.hook:
            self.hook(name)
        if self.failure == (index, 'after'):
            self.failure = None
            raise OSError('Response lost')
        return copy.deepcopy(result)

    def get_release(self):
        return self.operation('get', lambda: self.release)

    def create_release(self, source, content):
        def create():
            if self.release:
                raise ValueError('Release already exists')
            self.release = dict(id=1, tag_name=self.tag, name=helper.TITLE,
                                draft=False, prerelease=True, body=content, target_commitish=source)
            return self.release
        return self.operation('create', create)

    def update_release(self, release, content):
        return self.operation('update', lambda: self.release.update(body=content))

    def assets(self, release):
        return self.operation('assets', lambda: [item[0] for item in self.files.values()])

    def insert(self, name, data, state='uploaded'):
        if any(item[0]['name'] == name for item in self.files.values()):
            raise ValueError('Duplicate asset')
        asset = dict(id=self.next_id, name=name, size=len(data), state=state)
        self.next_id += 1
        self.files[asset['id']] = (asset, data)
        return asset

    def upload(self, release, name, stream, length):
        def upload():
            data = stream.read()
            if len(data) != length:
                raise ValueError('Invalid upload length')
            if self.starter == name:
                self.starter = None
                self.insert(name, b'', 'starter')
                raise OSError('502 after creating empty starter')
            if self.corrupt == name:
                self.corrupt = None
                data = bytes([data[0] ^ 1]) + data[1:]
            return self.insert(name, data)
        return self.operation('upload:' + name, upload)

    def delete_asset(self, asset):
        return self.operation('delete:' + asset['name'], lambda: self.files.pop(asset['id']))

    def verify(self, asset, expected):
        current, data = self.operation('verify:' + asset['name'], lambda: self.files[asset['id']])
        if current['state'] != 'uploaded' or current['size'] != expected['bytes']:
            raise ValueError('Remote metadata mismatch')
        helper.verify_stream(io.BytesIO(data), expected)

    def named(self, name):
        return next((item for item in self.files.values() if item[0]['name'] == name), None)


def package(directory, version=1, payload=b'compressed test payload'):
    directory.mkdir()
    files = {'1.root.json': b'trusted fixture root',
             f'{version}.snapshot.json': b'snapshot' + bytes([version]),
             f'{version}.targets.json': b'targets' + bytes([version]),
             f'{version}.release.json': b'release' + bytes([version]),
             f'{version}.nightly.json': b'nightly' + bytes([version]),
             'timestamp.json': json.dumps({'signed': {'version': version}}).encode()}
    hashed = helper.digest(payload)
    files[f'{hashed}.nightly.{hashed}.ext4.zst'] = payload
    for name, data in files.items():
        (directory / name).write_bytes(data)
    manifest = dict(schema=1, trusted_root_sha256=helper.digest(files['1.root.json']),
                    timestamp_version=version,
                    files={name: dict(bytes=len(data), sha256=helper.digest(data))
                           for name, data in files.items()})
    encoded = json.dumps(manifest, sort_keys=True).encode()
    return helper.Bundle(directory, encoded, helper.digest(encoded)), encoded


class DeliveryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.first, self.manifest = package(self.directory / 'first')
        self.second, _ = package(self.directory / 'second', 2, b'next compressed image')
        self.api = FakeGitHub()

    def publish(self, bundle=None):
        return helper.publish(self.api, bundle or self.first, 'a' * 40)

    def assert_committed(self, bundle):
        self.assertEqual(helper.parse_state(self.api.release),
                         dict(schema=1, current=bundle.record, pending=None))
        self.assertEqual(self.api.named('timestamp.json')[1],
                         (bundle.directory / 'timestamp.json').read_bytes())
        for name, blob in bundle.files.items():
            self.api.verify(self.api.named(name)[0], blob)

    def test_initial_publication_repeat_and_new_version_preserve_all_history(self):
        self.assertEqual(self.publish(), 'published')
        self.assert_committed(self.first)
        before = len(self.api.calls)
        self.assertEqual(self.publish(), 'unchanged')
        self.assertFalse(any(call.startswith(('upload:', 'delete:', 'update', 'create'))
                             for call in self.api.calls[before:]))
        old = {name: data for asset, data in self.api.files.values()
               if (name := asset['name']) != 'timestamp.json'}
        self.publish(self.second)
        self.assert_committed(self.second)
        for name, data in old.items():
            self.assertEqual(self.api.named(name)[1], data)
        self.assertEqual([name for name in self.api.calls if name.startswith('delete:')],
                         ['delete:timestamp.json'])

    def test_current_timestamp_is_written_after_every_immutable_readback(self):
        self.publish()
        position = self.api.calls.index('upload:timestamp.json')
        for name in set(self.first.files) - {'timestamp.json'} | {self.first.archive_name}:
            self.assertLess(self.api.calls.index('verify:' + name), position)

    def pending_second(self, alias):
        self.api = FakeGitHub()
        self.publish()
        self.api.release['body'] = helper.body(dict(schema=1, current=self.first.record, pending=self.second.record))
        raw = (self.second.directory / 'timestamp.json').read_bytes()
        self.api.insert(self.second.archive_name, raw)
        if alias != 'old':
            self.api.delete_asset(self.api.named('timestamp.json')[0])
            if alias == 'starter':
                self.api.insert('timestamp.json', b'', 'starter')
            elif alias == 'pending':
                self.api.insert('timestamp.json', raw)
            elif alias == 'unknown':
                self.api.insert('timestamp.json', b'unrecognized metadata')

    def test_abandonment_at_every_request_boundary_preserves_history_and_resumes(self):
        for alias in ('old', 'missing', 'starter'):
            self.pending_second(alias)
            self.api.calls.clear()
            self.assertEqual(helper.abandon(self.api, self.first.record, self.second.record), 'abandoned')
            count = len(self.api.calls)
            for offset in range(count):
                for when in ('before', 'after'):
                    with self.subTest(alias=alias, offset=offset, when=when):
                        self.pending_second(alias)
                        retained = {asset['name']: data for asset, data in self.api.files.values()
                                    if asset['name'] != 'timestamp.json'}
                        self.api.failure = (len(self.api.calls) + offset, when)
                        with self.assertRaises(OSError):
                            helper.abandon(self.api, self.first.record, self.second.record)
                        self.assertEqual(helper.abandon(self.api, self.first.record, self.second.record), 'abandoned')
                        self.assertEqual(helper.parse_state(self.api.release),
                                         dict(schema=1, current=self.first.record, pending=None))
                        for name, raw in retained.items():
                            self.assertEqual(self.api.named(name)[1], raw)
                        actual = self.api.named('timestamp.json')
                        if alias == 'old':
                            self.assertEqual(actual[1], (self.first.directory / 'timestamp.json').read_bytes())
                        else:
                            self.assertIsNone(actual)

    def test_visible_or_committed_pending_timestamp_must_be_reconciled_never_abandoned(self):
        for committed in ('pending', 'committed', 'repairing'):
            self.pending_second('pending')
            if committed != 'pending':
                self.api.release['body'] = helper.body(dict(schema=1, current=self.second.record,
                    pending=self.second.record if committed == 'repairing' else None))
            self.api.calls.clear()
            before = copy.deepcopy((self.api.release, self.api.files))
            self.assertEqual(helper.abandon(self.api, self.first.record, self.second.record), 'already-visible')
            self.assertEqual((self.api.release, self.api.files), before)
            self.assertFalse(any(call.startswith(('delete:', 'upload:', 'update')) for call in self.api.calls))

    def test_abandonment_rejects_unknown_bytes_and_different_reservations_without_mutation(self):
        for alias, mismatch in (('unknown', None), ('old', 'current'), ('old', 'pending')):
            self.pending_second(alias)
            current, pending = copy.deepcopy(self.first.record), copy.deepcopy(self.second.record)
            if mismatch:
                (current if mismatch == 'current' else pending)['manifest_sha256'] = 'f' * 64
            before = copy.deepcopy((self.api.release, self.api.files))
            with self.assertRaises(ValueError):
                helper.abandon(self.api, current, pending)
            self.assertEqual((self.api.release, self.api.files), before)

    def test_lost_response_at_every_boundary_resumes_first_and_later_publication(self):
        for replacement in (False, True):
            baseline = FakeGitHub()
            self.api = baseline
            if replacement:
                self.publish()
            start = len(baseline.calls)
            candidate = self.second if replacement else self.first
            self.publish(candidate)
            count = len(baseline.calls) - start
            for offset in range(count):
                for when in ('before', 'after'):
                    with self.subTest(replacement=replacement, offset=offset, when=when):
                        self.api = FakeGitHub()
                        if replacement:
                            self.publish()
                        self.api.failure = (len(self.api.calls) + offset, when)
                        with self.assertRaises(OSError):
                            self.publish(candidate)
                        alias = self.api.named('timestamp.json')
                        if alias and alias[1] == (candidate.directory / 'timestamp.json').read_bytes():
                            for name, blob in candidate.files.items():
                                self.api.verify(self.api.named(name)[0], blob)
                        self.publish(candidate)
                        self.assert_committed(candidate)

    def test_empty_failed_uploads_can_resume_including_alias_gap(self):
        for name in ['2.snapshot.json', self.second.archive_name, 'timestamp.json']:
            with self.subTest(name=name):
                self.api = FakeGitHub()
                self.publish()
                self.api.starter = name
                with self.assertRaises(OSError):
                    self.publish(self.second)
                self.assertEqual(self.api.named(name)[0]['state'], 'starter')
                self.publish(self.second)
                self.assert_committed(self.second)

    def test_immutable_conflict_never_replaces_the_conflicting_asset_or_timestamp(self):
        self.publish()
        self.api.insert('2.snapshot.json', b'conflicting bytes')
        before = len(self.api.calls)
        with self.assertRaises(ValueError):
            self.publish(self.second)
        self.assertEqual(self.api.named('2.snapshot.json')[1], b'conflicting bytes')
        self.assertEqual(self.api.named('timestamp.json')[1],
                         (self.first.directory / 'timestamp.json').read_bytes())
        self.assertFalse(any(call.startswith('delete:') for call in self.api.calls[before:]))

    def test_upload_corruption_is_detected_before_advancing_timestamp(self):
        for name in ['2.snapshot.json', self.second.archive_name,
                     next(name for name in self.second.files if name.endswith('.zst'))]:
            with self.subTest(name=name):
                self.api = FakeGitHub()
                self.publish()
                self.api.corrupt = name
                with self.assertRaises(ValueError):
                    self.publish(self.second)
                self.assertEqual(self.api.named('timestamp.json')[1],
                                 (self.first.directory / 'timestamp.json').read_bytes())

    def test_changed_local_payload_before_or_during_upload_never_advances(self):
        self.publish()
        name = next(name for name in self.second.files if name.endswith('.zst'))
        path = self.second.directory / name
        data = path.read_bytes()
        path.write_bytes(b'changed')
        before = list(self.api.calls)
        with self.assertRaises(ValueError):
            self.publish(self.second)
        self.assertEqual(before, self.api.calls)
        path.write_bytes(data)
        def change(call):
            if call == 'update':
                path.write_bytes(b'x' * len(data))
                self.api.hook = None
        self.api.hook = change
        with self.assertRaises(ValueError):
            self.publish(self.second)
        self.assertEqual(self.api.named('timestamp.json')[1],
                         (self.first.directory / 'timestamp.json').read_bytes())

    def test_stale_same_version_conflict_and_unfinished_reservation_are_rejected(self):
        self.publish(self.second)
        with self.assertRaisesRegex(ValueError, 'Stale'):
            self.publish()
        other, _ = package(self.directory / 'conflict', 2, b'different')
        with self.assertRaisesRegex(ValueError, 'conflicting'):
            self.publish(other)
        self.api = FakeGitHub()
        self.api.failure = (1, 'after')  # lost create response, durable pending plan
        with self.assertRaises(OSError):
            self.publish()
        with self.assertRaisesRegex(ValueError, 'Resume'):
            self.publish(self.second)
        self.publish()
        self.publish(self.second)

    def test_reservation_change_is_detected_before_alias_mutation(self):
        self.publish()
        def replace(call):
            if call == 'verify:' + self.second.archive_name:
                self.api.release['body'] = 'changed by another writer'
                self.api.hook = None
        self.api.hook = replace
        with self.assertRaises(ValueError):
            self.publish(self.second)
        self.assertEqual(self.api.named('timestamp.json')[1],
                         (self.first.directory / 'timestamp.json').read_bytes())

    def test_capacity_failure_keeps_all_existing_assets_and_reservation(self):
        self.publish()
        while len(self.api.files) < helper.MAX_ASSETS:
            self.api.insert(f'historical-{len(self.api.files)}', b'old')
        before = copy.deepcopy((self.api.release, self.api.files))
        with self.assertRaisesRegex(ValueError, 'capacity'):
            self.publish(self.second)
        self.assertEqual(before, (self.api.release, self.api.files))

    def test_unknown_alias_is_not_deleted_or_overwritten(self):
        self.publish()
        asset = self.api.named('timestamp.json')[0]
        self.api.files[asset['id']] = (asset, b'x' * asset['size'])
        before = copy.deepcopy((self.api.release, self.api.files))
        with self.assertRaisesRegex(ValueError, 'Current timestamp'):
            self.publish(self.second)
        self.assertEqual(before, (self.api.release, self.api.files))

    def test_missing_committed_alias_can_be_repaired_from_exact_committed_bundle(self):
        self.publish()
        self.api.delete_asset(self.api.named('timestamp.json')[0])
        self.publish()
        self.assert_committed(self.first)

    def test_windows_release_and_unmanaged_destinations_are_refused(self):
        self.publish()
        original = copy.deepcopy(self.api.release)
        for key, value in [('tag_name', 'nightly'), ('name', 'Nightly release'),
                           ('prerelease', False), ('draft', True), ('body', 'manual')]:
            self.api.release = {**original, key: value}
            before = copy.deepcopy(self.api.files)
            with self.assertRaises(ValueError):
                self.publish(self.second)
            self.assertEqual(before, self.api.files)
        self.api.release = original
        self.api.repo = 'someone/another-repository'
        with self.assertRaises(ValueError):
            self.publish()

    def test_bad_manifest_digest_names_limits_and_symlinks_are_refused(self):
        with self.assertRaises(ValueError):
            helper.Bundle(self.first.directory, self.manifest, '0' * 64)
        for name, blob in [('../root.json', dict(bytes=1, sha256='0' * 64)),
                           ('windows-client.zip', dict(bytes=1, sha256='0' * 64)),
                           ('1.root.json', dict(bytes=2 ** 31, sha256='0' * 64)),
                           ('1.root.json', dict(bytes=True, sha256='0' * 64))]:
            changed = json.loads(self.manifest)
            changed['files'][name] = blob
            data = json.dumps(changed).encode()
            with self.assertRaises(ValueError):
                helper.Bundle(self.first.directory, data, helper.digest(data))
        path = self.first.directory / '1.root.json'
        path.unlink()
        path.symlink_to('timestamp.json')
        with self.assertRaises((ValueError, OSError)):
            self.publish()
        self.assertEqual(self.api.calls, [])


class AdapterTests(unittest.TestCase):
    def test_public_download_is_credential_free_and_streamed_with_exact_digest(self):
        api = helper.GitHub('private-token-for-test')
        payload = b'payload'
        asset = dict(id=3, name='1.root.json', size=len(payload), state='uploaded')
        response = io.BytesIO(payload)
        response.status, response.headers = 200, {}
        class Opener:
            def open(inner, request, timeout):
                self.assertNotIn('Authorization', request.headers)
                self.assertEqual(request.full_url,
                    'https://github.com/aortez/space-wars/releases/download/ota-feed/1.root.json')
                self.assertEqual(request.get_header('Accept-encoding'), 'identity')
                return response
        with patch.object(helper.urllib.request, 'build_opener', return_value=Opener()):
            api.verify(asset, dict(bytes=len(payload), sha256=helper.digest(payload)))

    def test_authenticated_uploads_are_fixed_destination_and_length_bounded(self):
        api = helper.GitHub('private-token-for-test')
        class Opener:
            def open(inner, request, timeout):
                self.assertEqual(request.get_header('Authorization'), 'Bearer private-token-for-test')
                self.assertEqual(request.full_url,
                    'https://uploads.github.com/repos/aortez/space-wars/releases/7/assets?name=1.root.json')
                self.assertEqual(request.get_header('Content-length'), '3')
                self.assertEqual(request.data.read(100), b'abc')
                self.assertEqual(request.data.read(100), b'')
                return io.BytesIO(b'{"id": 8}')
        with patch.object(helper.urllib.request, 'build_opener', return_value=Opener()):
            self.assertEqual(api.upload(dict(id=7), '1.root.json', io.BytesIO(b'abc-extra'), 3), {'id': 8})

    def test_redirects_never_forward_authorization_or_downgrade_https(self):
        request = urllib.request.Request('https://github.com/asset',
                                         headers={'Authorization': 'must-not-leak'})
        handler = helper.PublicRedirect()
        redirected = handler.redirect_request(request, None, 302, '', {}, 'https://cdn.example/a?sig=x')
        self.assertNotIn('Authorization', redirected.headers)
        for url in ['http://cdn.example/a', 'https://user:pass@cdn.example/a', 'https://cdn.example/a#part']:
            with self.assertRaises(ValueError):
                handler.redirect_request(request, None, 302, '', {}, url)
        with self.assertRaises(ValueError):
            helper.NoRedirect().redirect_request(request, None, 302, '', {}, 'https://api.github.com/other')

    def test_public_response_failures_reject_partial_encoding_and_redact_redirect_queries(self):
        api = helper.GitHub('unused')
        asset = dict(id=1, name='1.root.json', size=3, state='uploaded')
        expected = dict(bytes=3, sha256=helper.digest(b'abc'))
        for status, headers in [(206, {}), (200, {'Content-Encoding': 'gzip'})]:
            response = io.BytesIO(b'abc')
            response.status, response.headers = status, headers
            with patch.object(helper.urllib.request, 'build_opener') as opener:
                opener.return_value.open.return_value = response
                with self.assertRaises(ValueError):
                    api.verify(asset, expected)
        with patch.object(helper.urllib.request, 'build_opener') as opener:
            opener.return_value.open.side_effect = urllib.error.HTTPError(
                'https://cdn.example/asset?signature=do-not-log', 403, 'forbidden', {}, None)
            with self.assertRaises(OSError) as error:
                api.verify(asset, expected)
            self.assertEqual(str(error.exception), 'Public OTA asset download failed')
            self.assertTrue(error.exception.__suppress_context__)
            self.assertTrue(opener.return_value.open.side_effect.closed)

    def test_pagination_limits_duplicates_and_incorrect_public_lengths_fail(self):
        api = helper.GitHub('unused')
        with patch.object(api, 'request', return_value=[{}] * 100):
            with self.assertRaises(ValueError):
                api.assets(dict(id=1))
        fake = FakeGitHub()
        asset = fake.insert('first', b'first')
        fake.files[2] = (asset, b'second')
        with self.assertRaises(ValueError):
            helper.asset_map(fake, None)
        for payload in (b'shor', b'longer', b'wrong'):
            with self.assertRaises(ValueError):
                helper.verify_stream(io.BytesIO(payload), dict(bytes=5, sha256=helper.digest(b'right')))


def signed_fixture(directory, manifest_path):
    encoded = Path(manifest_path).read_bytes()
    bundle = helper.Bundle(Path(directory), encoded, helper.digest(encoded))
    api = FakeGitHub()
    assert helper.publish(api, bundle, 'a' * 40) == 'published'
    assert helper.publish(api, bundle, 'a' * 40) == 'unchanged'
    assert helper.parse_state(api.release)['current'] == bundle.record


if __name__ == '__main__':
    if len(sys.argv) == 4 and sys.argv[1] == '--signed-fixture':
        signed_fixture(sys.argv[2], sys.argv[3])
    else:
        unittest.main()
