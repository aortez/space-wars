"""Persistent host-key startup policy in a private filesystem/user namespace."""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


YOCTO = Path(__file__).resolve().parents[1]
FILES = YOCTO / "meta-spacewars/recipes-connectivity/openssh/files"
KEYS = [f"ssh_host_{kind}_key" for kind in ("rsa", "ecdsa", "ed25519")]


class SshHostKeysTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.bwrap = shutil.which("bwrap")
        reason = "Install bubblewrap to test persistent host keys"
        if cls.bwrap:
            result = subprocess.run(
                [cls.bwrap, "--ro-bind", "/", "/", "--unshare-user", "--uid", "0", "--gid", "0", "--", "true"],
                capture_output=True, text=True,
            )
            reason = result.stderr if result.returncode else None
        if reason:
            if os.environ.get("SPACEWARS_REQUIRE_UPDATE_SANDBOX") == "1":
                raise RuntimeError(reason)
            raise unittest.SkipTest(reason)

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="spacewars-host-keys-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.data = self.root / "data"
        self.seed = self.root / "seed"
        self.data.mkdir()
        self.seed.mkdir()
        for name in KEYS:
            (self.seed / name).write_text(f"fake fixture {name}\n")
            (self.seed / (name + ".pub")).write_text(f"fake public {name}\n")

    def run_helper(self, mounted=True):
        args = [
            self.bwrap, "--ro-bind", "/usr", "/usr", "--ro-bind", "/lib", "/lib",
            "--ro-bind-try", "/lib64", "/lib64", "--symlink", "usr/bin", "/bin",
            "--ro-bind", "/etc/passwd", "/etc/passwd", "--ro-bind", "/etc/group", "/etc/group",
            "--unshare-user", "--uid", "0", "--gid", "0", "--unshare-pid", "--unshare-net",
            "--die-with-parent", "--proc", "/proc", "--dev", "/dev",
            "--ro-bind", str(self.seed), "/etc/ssh",
            "--ro-bind", str(FILES / "spacewars-ssh-host-keys"), "/helper.sh",
        ]
        args += ["--bind", str(self.data), "/data"] if mounted else ["--dir", "/data"]
        args += ["--chdir", "/", "--", "/bin/sh", "/helper.sh"]
        return subprocess.run(args, capture_output=True, text=True, timeout=10)

    def test_fresh_import_preserves_exact_keys_and_existing_keys_win(self):
        result = self.run_helper()
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in KEYS:
            path = self.data / "ssh" / name
            original = path.read_bytes()
            self.assertEqual(original, (self.seed / name).read_bytes())
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            (self.seed / name).write_text("obsolete root image key")
        (self.data / "spacewars-bootstrap/active").mkdir(parents=True)
        before = {path.name: path.read_bytes() for path in (self.data / "ssh").iterdir()}
        result = self.run_helper()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(before, {path.name: path.read_bytes() for path in (self.data / "ssh").iterdir()})

    def test_missing_stores_are_not_imported_for_active_or_provisioned_devices(self):
        for marker in ("spacewars-bootstrap/active", "spacewars-system/recovery"):
            with self.subTest(marker=marker):
                record = self.data / marker
                record.parent.mkdir()
                record.symlink_to("missing-record")
                result = self.run_helper()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Missing preserved SSH host-key store", result.stderr)
                self.assertFalse((self.data / "ssh").exists())
                record.unlink()
                record.parent.rmdir()

    def test_missing_empty_or_symlinked_key_blocks_without_repair(self):
        self.assertEqual(self.run_helper().returncode, 0)
        (self.data / "spacewars-system/recovery").mkdir(parents=True)
        key = self.data / "ssh" / KEYS[0]
        original = key.read_bytes()
        for kind in ("missing", "empty", "symlink", "fifo"):
            with self.subTest(kind=kind):
                key.unlink()
                if kind == "empty":
                    key.touch()
                elif kind == "symlink":
                    key.symlink_to(KEYS[1])
                elif kind == "fifo":
                    os.mkfifo(key)
                result = self.run_helper()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Missing or invalid preserved SSH host key", result.stderr)
                if key.exists() or key.is_symlink():
                    key.unlink()
                key.write_bytes(original)

    def test_missing_data_mount_and_unsafe_state_paths_are_rejected(self):
        self.assertNotEqual(self.run_helper(mounted=False).returncode, 0)
        (self.data / "ssh").symlink_to("/etc/ssh")
        self.assertNotEqual(self.run_helper().returncode, 0)
        (self.data / "ssh").unlink()
        for kind in ("file", "symlink"):
            state = self.data / "spacewars-system"
            if kind == "file":
                state.write_text("corrupt")
            else:
                state.symlink_to("missing")
            self.assertNotEqual(self.run_helper().returncode, 0)
            self.assertFalse((self.data / "ssh").exists())
            state.unlink()


class PersistenceUnitsTest(unittest.TestCase):
    def test_failed_host_store_prevents_key_generation_and_recovery_observes_bind_results(self):
        generator = (FILES / "sshdgenkeys-spacewars.conf").read_text()
        self.assertIn("Requires=spacewars-ssh-host-keys.service", generator)
        unit = (YOCTO / "meta-spacewars/recipes-spacewars/spacewars/files/spacewars-recovery.service").read_text()
        for directive in ("After=", "Wants="):
            line = next(line for line in unit.splitlines() if line.startswith(directive))
            for service in ("persistent-data-bind.service", "spacewars-bluetooth-data.service", "spacewars-ssh-host-keys.service"):
                self.assertIn(service, line)
        # Recovery must still run to observe failed initializers and roll back.
        requires = next(line for line in unit.splitlines() if line.startswith("Requires="))
        self.assertNotIn("spacewars-ssh-host-keys.service", requires)


if __name__ == "__main__":
    unittest.main()
