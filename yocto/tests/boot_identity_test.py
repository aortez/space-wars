import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "meta-spacewars/lib"))
from spacewars.boot_identity import boot_identity


class BootIdentityTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="spacewars-boot-test-")
        self.addCleanup(self.temporary.cleanup)
        self.deploy = Path(self.temporary.name)
        (self.deploy / "Image").write_bytes(b"common-kernel")
        (self.deploy / "board.dtb").write_bytes(b"board")
        (self.deploy / "spacewars-profiles").mkdir()
        (self.deploy / "spacewars-profiles/picade.txt").write_text("dtoverlay=picade\n")
        self.entries = "Image;kernel8.img Image;kernel_2712.img board.dtb spacewars-profiles/*;spacewars-profiles/ spacewars-device.txt spacewars-boot-id"

    def test_deterministic_regardless_of_entry_order(self):
        self.assertEqual(boot_identity(self.deploy, self.entries), boot_identity(self.deploy, " ".join(reversed(self.entries.split()))))

    def test_kernel_content_and_destination_are_significant(self):
        before = boot_identity(self.deploy, self.entries)
        self.assertNotEqual(before, boot_identity(self.deploy, self.entries.replace("kernel8.img", "other.img")))
        (self.deploy / "Image").write_bytes(b"different-kernel")
        self.assertNotEqual(before, boot_identity(self.deploy, self.entries))

    def test_device_choice_and_boot_marker_do_not_change_identity(self):
        before = boot_identity(self.deploy, self.entries)
        (self.deploy / "spacewars-device.txt").write_text("include spacewars-profiles/hyperpixel.txt\n")
        (self.deploy / "spacewars-boot-id").write_text("old-identity\n")
        self.assertEqual(before, boot_identity(self.deploy, self.entries))

    def test_profile_definitions_do_change_identity(self):
        before = boot_identity(self.deploy, self.entries)
        (self.deploy / "spacewars-profiles/picade.txt").write_text("dtoverlay=picade,noaudio\n")
        self.assertNotEqual(before, boot_identity(self.deploy, self.entries))

    def test_globs_match_wic_basename_and_directory_rules(self):
        self.assertEqual(boot_identity(self.deploy, "spacewars-profiles/*"), boot_identity(self.deploy, "spacewars-profiles/picade.txt;picade.txt"))
        self.assertEqual(boot_identity(self.deploy, "spacewars-profiles/*;spacewars-profiles/"), boot_identity(self.deploy, "spacewars-profiles/picade.txt"))

    def test_missing_files_empty_globs_and_conflicting_destinations_fail(self):
        for entries in ("", "absent", "missing/*", "Image;boot board.dtb;boot", "Image;../escape", "Image;", "Image;x;y"):
            with self.subTest(entries=entries), self.assertRaises(ValueError):
                boot_identity(self.deploy, entries)

    def test_deploy_symlinks_identify_contents_not_build_paths(self):
        (self.deploy / "Image-link").symlink_to("Image")
        self.assertEqual(boot_identity(self.deploy, "Image;kernel8.img"), boot_identity(self.deploy, "Image-link;kernel8.img"))


if __name__ == "__main__":
    unittest.main()
