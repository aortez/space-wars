import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "meta-spacewars/lib"))
from spacewars.fast_update import compatibility_id


class FastCompatibilityTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="spacewars-fast-compat-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "usr/lib").mkdir(parents=True)
        self.library = self.root / "usr/lib/libexample.so.1"
        self.library.write_bytes(b"runtime-library")
        self.asset = self.root / "spacewars-kiosk.service"
        self.asset.write_text("ExecStart=/usr/bin/engine-client\n")
        self.variables = {"MACHINE": "raspberrypi-spacewars", "TCLIBC": "glibc"}

    def identity(self):
        return compatibility_id(self.root, [self.asset], self.variables)

    def test_order_and_build_directory_do_not_matter(self):
        first = self.identity()
        self.variables = dict(reversed(list(self.variables.items())))
        self.assertEqual(first, self.identity())
        moved = self.root / "different-build-location"
        moved.mkdir()
        (self.root / "usr").rename(moved / "usr")
        self.assertEqual(first, compatibility_id(moved, [self.asset], self.variables))

    def test_library_service_and_machine_changes_require_full_update(self):
        first = self.identity()
        self.library.write_bytes(b"different-library")
        self.assertNotEqual(first, self.identity())
        second = self.identity()
        self.asset.write_text("ExecStart=/usr/bin/engine-client --fullscreen\n")
        self.assertNotEqual(second, self.identity())
        third = self.identity()
        self.variables["MACHINE"] = "raspberrypi5"
        self.assertNotEqual(third, self.identity())

    def test_shared_library_links_matter_but_headers_do_not(self):
        first = self.identity()
        (self.root / "usr/lib/header.h").write_text("not a runtime file")
        self.assertEqual(first, self.identity())
        (self.root / "usr/lib/libexample.so").symlink_to("libexample.so.1")
        self.assertNotEqual(first, self.identity())

    def test_empty_sysroot_is_not_compatible(self):
        self.library.unlink()
        with self.assertRaises(ValueError):
            self.identity()


if __name__ == "__main__":
    unittest.main()
