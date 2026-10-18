import json
import os
from pathlib import Path
import stat
import tempfile
import unittest

from cryptography.hazmat.primitives.serialization import load_der_private_key
import ota_authority as authority
import ota_publisher_journal as journal


class AuthorityTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.parent = Path(self.temp.name)
        self.output = self.parent / 'authority'
        self.verifier = Path(os.environ.get('SPACEWARS_OTA_INVENTORY',
                             authority.REPO / 'target/release/examples/feed_inventory')).resolve()
        if not self.verifier.is_file():
            self.fail('Build the native feed_inventory example before these tests')
        self.digest = authority.delivery.digest(self.verifier.read_bytes())

    def create(self, **kwargs):
        return authority.create(kwargs.get('output', self.output), self.verifier,
                                kwargs.get('digest', self.digest), '2026-10-17T12:00:00Z')

    def test_real_keys_and_native_verified_genesis_are_private_and_never_overwritten(self):
        result = self.create()
        self.assertTrue((self.output / 'COMPLETE.json').is_file())
        self.assertEqual(result['roles']['root']['threshold'], 2)
        self.assertEqual(len(result['roles']['root']['keyids']), 3)
        self.assertEqual(result['roles']['release']['threshold'], 2)
        self.assertEqual(result['roles']['nightly']['threshold'], 1)
        self.assertEqual(result['images'], 0)
        before = {}
        public = (self.output / 'public/ota-bootstrap.json').read_bytes()
        for path in self.output.rglob('*'):
            mode = stat.S_IMODE(path.stat().st_mode)
            self.assertEqual(mode, 0o700 if path.is_dir() else 0o600)
            if path.is_file():
                before[path] = path.read_bytes()
        for name, key_ids in ((name, result['roles'][role]['keyids'])
                              for role, names in authority.MEMBERS.items() for name in names):
            raw = (self.output / 'keys' / (name + '.pkcs8')).read_bytes()
            key = load_der_private_key(raw, None)
            public_key = dict(keytype='ed25519', scheme='ed25519',
                              keyval=dict(public=key.public_key().public_bytes_raw().hex()))
            self.assertIn(authority.delivery.digest(journal.encode(public_key)), key_ids)
            self.assertNotIn(raw.hex().encode(), public)
        state = journal.load((self.output / 'public/publisher-state.json').read_bytes())
        self.assertEqual(state['generation'], 0)
        self.assertEqual(state['images'], {})
        self.assertFalse(result['published'])
        self.assertFalse(result['devices_provisioned'])
        with self.assertRaises(FileExistsError):
            self.create()
        self.assertEqual(before, {path: path.read_bytes() for path in before})

    def test_bad_verifier_unsafe_parent_and_repository_output_do_not_create_keys(self):
        with self.assertRaisesRegex(ValueError, 'verifier differs'):
            self.create(digest='f' * 64)
        self.assertFalse(self.output.exists())
        os.chmod(self.parent, 0o770)
        with self.assertRaisesRegex(ValueError, 'owner-controlled'):
            self.create()
        os.chmod(self.parent, 0o700)
        with self.assertRaises(ValueError):
            self.create(output=authority.REPO / 'target/forbidden-key-output')
        self.assertFalse(self.output.exists())

    def test_symlink_or_incomplete_output_is_never_adopted(self):
        other = self.parent / 'other'
        other.mkdir(mode=0o700)
        self.output.symlink_to(other, target_is_directory=True)
        with self.assertRaises(FileExistsError):
            self.create()
        self.output.unlink()
        self.output.mkdir(mode=0o700)
        with self.assertRaises(FileExistsError):
            self.create()
        self.assertEqual(list(self.output.iterdir()), [])

    def test_failed_native_verification_retains_keys_without_a_completion_marker(self):
        failing = self.parent / 'verifier'
        failing.write_text('#!/bin/sh\nexit 1\n')
        failing.chmod(0o700)
        self.verifier = failing
        self.digest = authority.delivery.digest(failing.read_bytes())
        with self.assertRaisesRegex(ValueError, 'verification failed'):
            self.create()
        self.assertEqual(len(list((self.output / 'keys').iterdir())), 9)
        self.assertFalse((self.output / 'COMPLETE.json').exists())
        self.assertFalse((self.output / 'public/ota-bootstrap.json').exists())
        with self.assertRaises(FileExistsError):
            self.create()


if __name__ == '__main__':
    unittest.main()
