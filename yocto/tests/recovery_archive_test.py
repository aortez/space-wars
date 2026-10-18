import hashlib
import importlib.util
import io
import os
from pathlib import Path
import tarfile
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "recovery_archive", Path(__file__).parents[1] / "scripts/recovery_archive.py")
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)


class RecoveryArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.archive = self.base / "recovery.tar"
        self.destination = self.base / "offline-data"
        self.destination.mkdir()

    def fixture(self, extra=(), owner=None):
        uid, gid = owner or (os.getuid(), os.getgid())
        entries = [("data", "dir", None), ("data/app", "dir", None),
                   ("data/app/save", "file", b"retained save\x00\xff"),
                   ("data/app/copy", "hard", "data/app/save"),
                   ("data/app/link", "sym", "save"),
                   ("etc", "dir", None), ("etc/old-root", "file", b"do not import")]
        with tarfile.open(self.archive, "w", format=tarfile.PAX_FORMAT) as archive:
            for name, kind, value in entries + list(extra):
                item = tarfile.TarInfo(name)
                item.uid, item.gid = uid, gid
                item.mtime = 1234
                item.pax_headers["mtime"] = "1234.123456789"
                item.mode = 0o750 if kind == "dir" else 0o777 if kind == "sym" else 0o640
                item.type = {"dir": tarfile.DIRTYPE, "file": tarfile.REGTYPE,
                             "hard": tarfile.LNKTYPE, "sym": tarfile.SYMTYPE,
                             "fifo": tarfile.FIFOTYPE}[kind]
                if kind in ["hard", "sym"]:
                    item.linkname = value
                if name in ["data/app/save", "data/app/copy"]:
                    item.pax_headers["SCHILY.xattr.user.spacewars"] = "restore fixture"
                if kind == "file":
                    item.size = len(value)
                    archive.addfile(item, io.BytesIO(value))
                else:
                    archive.addfile(item)
        with self.archive.open("rb") as stream:
            return {"bytes": self.archive.stat().st_size,
                    "sha256": hashlib.file_digest(stream, "sha256").hexdigest()}

    def test_real_restore_keeps_metadata_links_and_only_data(self):
        expected = self.fixture()
        report = recovery.restore_archive(self.archive, expected, self.destination)
        self.assertTrue(report["numeric_owners_verified"])
        self.assertTrue(report["extended_attributes_verified"])
        self.assertEqual(report["data_entries"], 5)
        self.assertEqual((self.destination / "app/save").read_bytes(), b"retained save\x00\xff")
        self.assertFalse((self.destination / "etc").exists())
        self.assertEqual((self.destination / "app/save").stat().st_mtime_ns, 1234123456789)
        # A stopped, interrupted restoration can safely be repeated on its offline tree.
        recovery.restore_archive(self.archive, expected, self.destination)

    @unittest.skipUnless(os.geteuid() == 0, "Numeric foreign owners require an isolated root test")
    def test_real_foreign_ownership(self):
        expected = self.fixture(owner=(1001, 1000))
        recovery.restore_archive(self.archive, expected, self.destination)
        observed = (self.destination / "app/save").stat()
        self.assertEqual((observed.st_uid, observed.st_gid), (1001, 1000))

    def test_checksum_failure_leaves_destination_empty(self):
        expected = self.fixture()
        expected["sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "checksum"):
            recovery.restore_archive(self.archive, expected, self.destination)
        self.assertEqual(list(self.destination.iterdir()), [])

    def test_unsafe_archives_fail_before_writes(self):
        cases = [
            ("data/../escape", "file", b"bad"),
            ("/data/escape", "file", b"absolute spelling"),
            ("./data/escape", "file", b"indirect spelling"),
            ("data/app/save", "file", b"duplicate"),
            ("data/app/link/escape", "file", b"through symlink"),
            ("data/cross-root", "hard", "etc/old-root"),
            ("data/special", "fifo", None),
            ("data/spacewars-system/recovery", "dir", None),
            ("data/spacewars-bootstrap", "dir", None),
        ]
        for item in cases:
            with self.subTest(item=item[0]):
                expected = self.fixture([item])
                with self.assertRaises(ValueError):
                    recovery.restore_archive(self.archive, expected, self.destination)
                self.assertEqual(list(self.destination.iterdir()), [])

    def test_existing_directory_symlink_cannot_redirect_restore(self):
        expected = self.fixture()
        outside = self.base / "outside"
        outside.mkdir()
        (self.destination / "app").symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "different type"):
            recovery.restore_archive(self.archive, expected, self.destination)
        self.assertEqual(list(outside.iterdir()), [])

    def test_existing_unrelated_data_is_not_overwritten(self):
        expected = self.fixture()
        (self.destination / "newer-save").write_bytes(b"keep me")
        with self.assertRaisesRegex(ValueError, "Unexpected existing"):
            recovery.restore_archive(self.archive, expected, self.destination)
        self.assertEqual((self.destination / "newer-save").read_bytes(), b"keep me")

    def test_verification_detects_post_restore_damage(self):
        expected = self.fixture()
        recovery.restore_archive(self.archive, expected, self.destination)
        path = self.destination / "app/save"
        path.write_bytes(b"damaged save\x00\xff!")
        os.utime(path, ns=(1234123456789, 1234123456789))
        with tarfile.open(self.archive) as archive:
            with self.assertRaisesRegex(ValueError, "contents differ"):
                recovery.verify_tree(archive, recovery.data_members(archive), self.destination)


if __name__ == "__main__":
    unittest.main()
