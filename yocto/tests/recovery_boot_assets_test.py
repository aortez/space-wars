import hashlib
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "meta-spacewars/lib"))
from spacewars.boot_identity import boot_paths, MUTABLE_BOOT_FILES, prepare_recovery_boot, recovery_boot_identity


class RecoveryBootAssetsTest(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "firmware").mkdir()
        (self.root / "firmware/config.txt").write_text("[all]\ninitramfs spacewars-recovery.cpio.gz followkernel\ninclude spacewars-device.txt\n")
        (self.root / "firmware/cmdline.txt").write_text("root=/dev/sda2 rootfstype=ext4 rootwait roottimeout=30 console=tty1\n")
        for name in ("kernel8.img", "kernel_2712.img", "spacewars-recovery.cpio.gz"):
            (self.root / name).write_bytes(b"immutable fixture")
        (self.root / "spacewars-kernel-build-id").write_text("a" * 40 + "\n")
        (self.root / "spacewars-device.txt").write_text("include spacewars-profiles/hdmi.txt\n")
        self.entries = "firmware/* kernel8.img kernel_2712.img spacewars-recovery.cpio.gz spacewars-kernel-build-id spacewars-device.txt spacewars-cmdline.txt spacewars-trial.txt tryboot.txt spacewars-boot-id spacewars-boot-manifest.json"

    def test_generated_trial_uses_the_same_boot_configuration_and_separate_root(self):
        manifest, identity = prepare_recovery_boot(self.root, self.entries)
        self.assertEqual(manifest["schema"], 2)
        self.assertEqual(identity, recovery_boot_identity(manifest))
        self.assertTrue((self.root / "tryboot.txt").read_text().startswith((self.root / "firmware/config.txt").read_text()))
        self.assertTrue((self.root / "tryboot.txt").read_text().endswith("[all]\ncmdline=spacewars-trial.txt\n"))
        self.assertIn("root=@SPACEWARS_ROOT@", (self.root / "spacewars-cmdline.txt").read_text())
        self.assertEqual((self.root / "spacewars-trial.txt").read_text(), "spacewars.trial=unprepared\n")
        self.assertTrue(MUTABLE_BOOT_FILES.isdisjoint(manifest["files"]))

    def test_slot_changes_leave_immutable_identity_intact_but_template_changes_do_not(self):
        manifest, original = prepare_recovery_boot(self.root, self.entries)
        for name in ("firmware/cmdline.txt", "spacewars-trial.txt", "spacewars-device.txt", "spacewars-boot-id"):
            (self.root / name).write_text("mutable fixture\n")
        files = {name: {"bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                 for name, path in boot_paths(self.root, self.entries, MUTABLE_BOOT_FILES).items()}
        self.assertEqual(recovery_boot_identity({"files": files}), original)
        (self.root / "spacewars-cmdline.txt").write_text("changed template\n")
        files["spacewars-cmdline.txt"]["sha256"] = hashlib.sha256(b"changed template\n").hexdigest()
        self.assertNotEqual(recovery_boot_identity({"files": files}), original)

    def test_ambiguous_or_recovery_bypassing_command_lines_are_rejected(self):
        path = self.root / "firmware/cmdline.txt"
        valid = path.read_text()
        for addition in (" root=/dev/sda3", " rootfstype=xfs", " roottimeout=999", " init=/bin/sh", " init", " rdinit=/bin/sh", " rootdelay=0", " init_fatal_sh", " spacewars.trial=bad", " x=$(command)"):
            with self.subTest(addition=addition):
                path.write_text(valid.strip() + addition + "\n")
                with self.assertRaises(ValueError):
                    prepare_recovery_boot(self.root, self.entries)

    def test_device_hostname_is_preserved_outside_immutable_boot_identity(self):
        _, original = prepare_recovery_boot(self.root, self.entries)
        (self.root / "hostname.txt").write_text("sw-picade\n")
        manifest, named = prepare_recovery_boot(self.root, self.entries + " hostname.txt")
        self.assertEqual(original, named)
        self.assertNotIn("hostname.txt", manifest["files"])
        self.assertEqual((self.root / "hostname.txt").read_text(), "sw-picade\n")

    def test_fat_collisions_and_changed_kernel_build_id_are_rejected(self):
        (self.root / "KERNEL8.IMG").write_bytes(b"case collision")
        with self.assertRaises(ValueError):
            prepare_recovery_boot(self.root, self.entries + " KERNEL8.IMG")
        (self.root / "spacewars-kernel-build-id").write_text("unknown\n")
        with self.assertRaises(ValueError):
            prepare_recovery_boot(self.root, self.entries)


if __name__ == "__main__":
    unittest.main()
