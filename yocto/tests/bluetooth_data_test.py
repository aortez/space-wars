"""Exercise Bluetooth key migration in an isolated filesystem, never host BlueZ."""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


YOCTO = Path(__file__).resolve().parents[1]
RECIPE = YOCTO / "meta-spacewars/recipes-connectivity/spacewars-bluetooth"
FILES = RECIPE / "files"


class BluetoothDataTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.bwrap = shutil.which("bwrap")
        reason = "Install bubblewrap to test Bluetooth state migration"
        if cls.bwrap:
            probe = subprocess.run(
                [cls.bwrap, "--ro-bind", "/", "/", "--unshare-user", "--uid", "0", "--gid", "0", "--", "true"],
                capture_output=True, text=True,
            )
            reason = probe.stderr if probe.returncode else None
        if reason:
            if os.environ.get("SPACEWARS_REQUIRE_UPDATE_SANDBOX") == "1":
                raise RuntimeError(reason)
            raise unittest.SkipTest(reason)

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="spacewars-bluetooth-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.data = self.root / "data"
        self.state = self.root / "state"
        self.data.mkdir()
        self.state.mkdir()
        self.key = self.state / "adapter" / "controller" / "info"
        self.key.parent.mkdir(parents=True)
        self.key.write_text("test pairing key")

    def run_helper(self, mounted=True, fail_copy=False):
        args = [
            self.bwrap, "--ro-bind", "/usr", "/usr", "--ro-bind", "/lib", "/lib",
            "--ro-bind-try", "/lib64", "/lib64", "--symlink", "usr/bin", "/bin",
            "--ro-bind", "/etc/passwd", "/etc/passwd", "--ro-bind", "/etc/group", "/etc/group",
            "--unshare-user", "--uid", "0", "--gid", "0", "--unshare-pid", "--unshare-net",
            "--die-with-parent", "--proc", "/proc", "--dev", "/dev",
            "--bind", str(self.state), "/var/lib/bluetooth",
            "--ro-bind", str(FILES / "spacewars-bluetooth-data-init.sh"), "/helper.sh",
        ]
        args += ["--bind", str(self.data), "/data"] if mounted else ["--dir", "/data"]
        if fail_copy:
            helper = self.root / "fail-copy"
            helper.write_text("#!/bin/sh\nexit 7\n")
            helper.chmod(0o755)
            args += ["--ro-bind", str(helper), "/usr/bin/cp"]
        args += ["--chdir", "/", "--", "/bin/sh", "/helper.sh"]
        return subprocess.run(args, capture_output=True, text=True, timeout=10)

    def test_first_migration_preserves_keys_and_restricts_permissions(self):
        result = self.run_helper()
        self.assertEqual(result.returncode, 0, result.stderr)
        destination = self.data / "bluetooth"
        self.assertEqual((destination / "adapter/controller/info").read_text(), "test pairing key")
        self.assertEqual(destination.stat().st_mode & 0o777, 0o700)
        self.assertEqual((destination / "adapter/controller/info").stat().st_mode & 0o077, 0)
        self.assertEqual(self.key.read_text(), "test pairing key")

    def test_existing_data_wins_over_older_os_slot_and_forgotten_keys_stay_forgotten(self):
        self.assertEqual(self.run_helper().returncode, 0)
        persistent_key = self.data / "bluetooth/adapter/controller/info"
        persistent_key.write_text("new pairing key")
        self.assertEqual(self.run_helper().returncode, 0)
        self.assertEqual(persistent_key.read_text(), "new pairing key")
        persistent_key.unlink()
        self.assertEqual(self.run_helper().returncode, 0)
        self.assertFalse(persistent_key.exists())

    def test_failed_copy_is_not_published_and_can_be_retried(self):
        self.assertNotEqual(self.run_helper(fail_copy=True).returncode, 0)
        self.assertFalse((self.data / "bluetooth").exists())
        self.assertEqual(list(self.data.iterdir()), [])
        self.assertEqual(self.run_helper().returncode, 0)

    def test_missing_data_mount_and_symlinks_are_rejected(self):
        self.assertNotEqual(self.run_helper(mounted=False).returncode, 0)
        (self.data / "bluetooth").symlink_to("/var/lib/bluetooth")
        result = self.run_helper()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("symlink", result.stderr)


class BluetoothPackagingTest(unittest.TestCase):
    def test_persistence_is_required_before_bluez_and_installed_in_image(self):
        unit = (FILES / "spacewars-bluetooth-data.service").read_text()
        dropin = (FILES / "persistent-data.conf").read_text()
        self.assertIn("Requires=data.mount", unit)
        self.assertIn("After=data.mount", unit)
        self.assertIn("Before=bluetooth.service", unit)
        self.assertIn("--bind /data/bluetooth /var/lib/bluetooth", unit)
        self.assertIn("Requires=spacewars-bluetooth-data.service", dropin)
        self.assertIn("After=spacewars-bluetooth-data.service", dropin)
        image = (YOCTO / "meta-spacewars/recipes-core/images/spacewars-image.bb").read_text()
        self.assertIn("spacewars-bluetooth", image)
        recipe = (RECIPE / "spacewars-bluetooth.bb").read_text()
        for asset in ("spacewars-bluetooth-data-init.sh", "spacewars-bluetooth-data.service", "persistent-data.conf"):
            self.assertIn(f"file://{asset}", recipe)
            self.assertIn(f"${{WORKDIR}}/{asset}", recipe)


if __name__ == "__main__":
    unittest.main()
