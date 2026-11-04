import copy
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "meta-spacewars/lib"))
from spacewars.ota_bundle import DESTINATION, MAX_BYTES, install_selected_bundle, read_selected_bundle, validate_public_bundle
from spacewars.image_identity import rootfs_tree_identity


class OtaBundleTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name)
        # Structural fixture only; Rust tests use cryptographically signed roots.
        self.bundle = {"schema": 1, "location": {"metadata": "https://example.org/metadata/", "targets": "https://example.org/targets/"},
                       "root": {"signed": {"_type": "root", "spec_version": "1.0.0", "version": 1,
                                           "expires": "2027-10-16T00:00:00Z", "consistent_snapshot": True,
                                           "keys": {"key": {"keytype": "ed25519", "scheme": "ed25519", "keyval": {"public": "00" * 32}}},
                                           "roles": {role: {"keyids": ["key"], "threshold": 1} for role in ("root", "targets", "snapshot", "timestamp")}},
                                "signatures": [{"keyid": "key", "sig": "00" * 64}]}}
        self.source = self.path / "public.json"
        self.source.write_bytes(json.dumps(self.bundle).encode())
        self.digest = hashlib.sha256(self.source.read_bytes()).hexdigest()

    def test_selected_public_bytes_are_embedded_exactly_and_identity_changes(self):
        image = self.path / "image"
        image.mkdir()
        self.assertEqual(install_selected_bundle(image, str(self.source), self.digest), self.digest)
        self.assertEqual((image / DESTINATION).read_bytes(), self.source.read_bytes())
        self.assertEqual((image / DESTINATION).stat().st_mode & 0o777, 0o644)
        with self.assertRaises(FileExistsError):
            install_selected_bundle(image, str(self.source), self.digest)
        before = rootfs_tree_identity(image)
        (image / DESTINATION).write_bytes(self.source.read_bytes() + b"\n")
        self.assertNotEqual(rootfs_tree_identity(image), before)

    def test_missing_selection_changed_source_and_unsafe_source_are_rejected(self):
        for source, digest in [("", self.digest), (str(self.source), ""), (str(self.source), "a" * 64), ("relative.json", self.digest)]:
            with self.assertRaises(ValueError):
                read_selected_bundle(source, digest)
        alias = self.path / "alias"
        alias.symlink_to(self.source)
        with self.assertRaises(OSError):
            read_selected_bundle(str(alias), self.digest)
        alias.unlink()
        os.link(self.source, alias)
        with self.assertRaises(ValueError):
            read_selected_bundle(str(self.source), self.digest)
        alias.unlink()
        self.source.write_bytes(b"x" * (MAX_BYTES + 1))
        with self.assertRaises(ValueError):
            read_selected_bundle(str(self.source), self.digest)

    def test_private_material_credential_urls_and_unknown_fields_are_rejected(self):
        for fault in ("private", "pem", "url", "extra", "role", "threshold", "unsigned", "schema", "duplicate"):
            bundle = copy.deepcopy(self.bundle)
            if fault == "private":
                bundle["root"]["signed"]["keys"]["key"]["keyval"]["private"] = "secret"
            elif fault == "pem":
                bundle["root"]["signed"]["keys"]["key"]["keyval"]["public"] = "-----BEGIN PRIVATE KEY-----"
            elif fault == "url":
                bundle["location"]["metadata"] = "https://user:token@example.org/"
            elif fault == "extra":
                bundle["script"] = "unsupported"
            elif fault == "role":
                bundle["root"]["signed"]["roles"]["root"]["keyids"] = ["missing"]
            elif fault == "threshold":
                bundle["root"]["signed"]["roles"]["root"]["threshold"] = True
            elif fault == "unsigned":
                bundle["root"]["signatures"] = []
            elif fault == "schema":
                bundle["schema"] = True
            content = json.dumps(bundle).encode()
            if fault == "duplicate":
                content = b'{"schema":1,' + content[1:]
            with self.subTest(fault=fault), self.assertRaises(ValueError):
                validate_public_bundle(content)

    def test_image_symlinks_cannot_redirect_publication(self):
        image = self.path / "image"
        image.mkdir()
        outside = self.path / "outside"
        outside.mkdir()
        (image / "etc").symlink_to(outside)
        with self.assertRaises(ValueError):
            install_selected_bundle(image, str(self.source), self.digest)
        self.assertEqual(list(outside.iterdir()), [])
