import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "meta-spacewars/lib"))
from spacewars.image_identity import (
    IDENTITY_PATH, SOURCE_PATH, create_image_identity, rootfs_tree_identity,
    source_identity, write_image_identity,
)


class ImageIdentityTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        for directory in ("etc/spacewars", "usr/share/spacewars", "usr/bin", "var/lib"):
            (self.root / directory).mkdir(parents=True)
        (self.root / "etc/spacewars/boot-id").write_text("b" * 64 + "\n")
        (self.root / SOURCE_PATH).write_text(json.dumps({
            "schema": 1, "commit": "a" * 40, "dirty": False, "revision": "a" * 12,
        }))
        for name in ("engine-client", "engine-os-manager"):
            path = self.root / "usr/bin" / name
            path.write_bytes(b"packaged stripped " + name.encode())
            path.chmod(0o755)
        # Build hashing must not follow a link into the workstation's /data.
        (self.root / "var/lib/spacewars").symlink_to("/data/spacewars/config")

    def identity(self):
        return create_image_identity(self.root, "raspberrypi-spacewars", "aarch64")

    def test_identity_is_stable_across_regeneration_and_timestamp_changes(self):
        first = write_image_identity(self.root, "raspberrypi-spacewars", "aarch64", 1234)
        self.assertEqual(first["build_id"], "rootfs-" + first["rootfs_tree"])
        self.assertEqual(self.identity(), first)
        path = self.root / "usr/bin/engine-client"
        os.utime(path, (5678, 5678))
        self.assertEqual(self.identity(), first)
        self.assertEqual(write_image_identity(self.root, "raspberrypi-spacewars", "aarch64", 4567), first)

    def test_packaged_binary_and_os_changes_both_change_build_identity(self):
        before = self.identity()
        (self.root / "usr/bin/engine-client").write_bytes(b"different final package")
        after = self.identity()
        self.assertNotEqual(before["binaries"]["client"], after["binaries"]["client"])
        self.assertNotEqual(before["build_id"], after["build_id"])
        (self.root / "etc/os-release").write_text("VERSION_ID=new-os\n")
        os_changed = self.identity()
        self.assertNotEqual(after["build_id"], os_changed["build_id"])
        self.assertEqual(after["binaries"], os_changed["binaries"])

    def test_modes_links_names_and_hardlink_relationships_are_bound(self):
        before = rootfs_tree_identity(self.root)
        (self.root / "etc/spacewars").chmod(0o700)
        self.assertNotEqual(before, rootfs_tree_identity(self.root))
        before = rootfs_tree_identity(self.root)
        link = self.root / "var/lib/spacewars"
        link.unlink()
        link.symlink_to("/data/spacewars/different")
        self.assertNotEqual(before, rootfs_tree_identity(self.root))
        first = self.root / "etc/first"
        second = self.root / "etc/second"
        first.write_bytes(b"same contents")
        os.link(first, second)
        before = rootfs_tree_identity(self.root)
        second.unlink()
        shutil.copyfile(first, second)
        self.assertNotEqual(before, rootfs_tree_identity(self.root))
        before = rootfs_tree_identity(self.root)
        second.rename(self.root / "etc/renamed")
        self.assertNotEqual(before, rootfs_tree_identity(self.root))

    def test_missing_unsafe_or_symlinked_binaries_and_bad_source_are_rejected(self):
        binary = self.root / "usr/bin/engine-client"
        binary.chmod(0o777)
        with self.assertRaisesRegex(ValueError, "unsafe image executable"):
            self.identity()
        binary.unlink()
        binary.symlink_to("engine-os-manager")
        with self.assertRaises(ValueError):
            self.identity()
        binary.unlink()
        with self.assertRaises(OSError):
            self.identity()
        (self.root / SOURCE_PATH).write_text('{"schema":1,"commit":"unknown"}')
        with self.assertRaisesRegex(ValueError, "invalid packaged source identity"):
            self.identity()

    def test_extended_attributes_are_part_of_the_build_identity(self):
        path = self.root / "usr/bin/engine-client"
        before = self.identity()
        os.setxattr(path, "user.spacewars-fixture", b"first")
        first = self.identity()
        self.assertNotEqual(before["build_id"], first["build_id"])
        self.assertEqual(before["binaries"], first["binaries"])
        os.setxattr(path, "user.spacewars-fixture", b"changed")
        self.assertNotEqual(first["build_id"], self.identity()["build_id"])

    def test_generated_identity_never_follows_a_symlink(self):
        identity = self.root / IDENTITY_PATH
        target = self.root / "user-data"
        target.write_bytes(b"keep this")
        identity.symlink_to(target)
        with self.assertRaisesRegex(ValueError, "previous generated image identity"):
            write_image_identity(self.root, "raspberrypi-spacewars", "aarch64", 1234)
        self.assertEqual(target.read_bytes(), b"keep this")

    def test_source_tracks_clean_commits_dirty_edits_and_untracked_files(self):
        repo = self.root / "source"
        repo.mkdir()

        def git(*args):
            return subprocess.check_output(["git", "-C", str(repo), *args], text=True,
                                           stderr=subprocess.DEVNULL).strip()

        git("init", "-b", "fixture")
        (repo / "code").write_text("first\n")
        git("add", "code")
        git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
            "-c", "commit.gpgsign=false", "commit", "-m", "fixture")
        clean = source_identity(repo)
        self.assertFalse(clean["dirty"])
        self.assertEqual(clean["commit"], git("rev-parse", "HEAD"))
        (repo / "untracked").write_text("new source\n")
        self.assertTrue(source_identity(repo)["dirty"])
        (repo / "untracked").unlink()
        (repo / "code").write_text("edited\n")
        self.assertTrue(source_identity(repo)["dirty"])
        nested = repo / "archive"
        nested.mkdir()
        with self.assertRaisesRegex(ValueError, "explicitly selected Git checkout"):
            source_identity(nested)


if __name__ == "__main__":
    unittest.main()
