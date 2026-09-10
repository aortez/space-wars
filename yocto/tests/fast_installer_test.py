"""Exercise the real privileged shell helper in a disposable mount namespace.

Only service control, user switching, architecture and waits are mocked. File
copy, checksums, atomic renames, locking and rollback use the real host tools.
Host tools/libraries are read-only; writable locations are test-owned fixtures.
"""

import hashlib
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


HELPER = Path(__file__).resolve().parents[1] / "meta-spacewars/recipes-spacewars/spacewars/files/spacewars-fast-update.sh"
COMPAT = "a" * 64
STAGE = "/tmp/spacewars-fast.A123456789"


class FastInstallerTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not shutil.which("bwrap"):
            reason = "Install bubblewrap to run isolated installer/rollback tests"
        else:
            probe = subprocess.run(
                ["bwrap", "--ro-bind", "/", "/", "--unshare-user", "--uid", "0", "--gid", "0", "--", "true"],
                capture_output=True, text=True,
            )
            reason = probe.stderr if probe.returncode else None
        if reason:
            if os.environ.get("SPACEWARS_REQUIRE_UPDATE_SANDBOX") == "1":
                raise RuntimeError(reason)
            raise unittest.SkipTest(reason)

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="spacewars-installer-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name in ("bin", "storage", "share", "tmp", "control"):
            (self.root / name).mkdir()
        self.stage = self.root / "tmp" / Path(STAGE).name
        self.stage.mkdir()
        self.bin = self.root / "bin"
        self.control = self.root / "control"
        for name in ("bash", "cat", "chmod", "cp", "flock", "id", "install", "mktemp", "od", "rm", "rmdir", "sha256sum", "tr"):
            (self.bin / name).symlink_to(f"/host/usr/bin/{name}")
        (self.root / "share/fast-update-compat").write_text(COMPAT + "\n")
        self.hashes = []
        for name in ("engine-client", "spacewars-cli"):
            binary = bytearray(64)
            binary[:7] = bytes.fromhex("7f454c46020101")
            binary[18:20] = bytes.fromhex("b700")
            self.hashes.append(hashlib.sha256(binary).hexdigest())
            (self.stage / name).write_bytes(binary)
            (self.bin / name).write_bytes(b"old-" + name.encode())
            (self.bin / name).chmod(0o755)
        self.command("uname", "echo aarch64")
        self.command("sync", ":")
        self.command("sleep", ":")
        self.command("systemctl", '''
echo "$1" >> /test/calls
case "$1" in
    is-active) [[ $(< /test/state) == active ]] ;;
    show) echo 424 ;;
    stop) echo stopped > /test/state ;;
    start)
        if [[ -f /test/fail-start && ! -f /test/start-failed ]]; then
            echo yes > /test/start-failed
            exit 1
        fi
        echo active > /test/state ;;
    *) exit 90 ;;
esac
''')
        self.command("setpriv", '''
[[ $1 == --reuid=spacewars && $2 == --regid=spacewars
   && $3 == --init-groups && $4 == --no-new-privs ]] || exit 91
shift 4
if [[ $1 == cat ]]; then
    exec /host/usr/bin/cat "${@:2}"
fi
[[ $* == 'timeout 2 /usr/bin/spacewars-cli status' ]] || exit 92
echo status >> /test/calls
[[ ! -f /test/fail-health ]]
''')
        self.command("mv", '''
if [[ -f /test/fail-second-rename && ! -f /test/rename-failed
      && ${*: -1} == /usr/bin/spacewars-cli && $* != *previous* ]]; then
    echo yes > /test/rename-failed
    exit 1
fi
exec /host/usr/bin/mv "$@"
''')
        (self.control / "state").write_text("active\n")

    def command(self, name, body):
        file = self.bin / name
        file.write_text("#!/bin/bash\nset -eu\n" + body + "\n")
        file.chmod(0o755)

    def run_helper(self, *args):
        result = subprocess.run([
            "bwrap", "--ro-bind", "/usr", "/host/usr",
            "--ro-bind", "/lib", "/lib", "--ro-bind-try", "/lib64", "/lib64",
            "--ro-bind", "/etc", "/etc", "--symlink", "usr/bin", "/bin",
            "--unshare-user", "--uid", "0", "--gid", "0", "--unshare-pid", "--unshare-net",
            "--proc", "/proc", "--dev", "/dev", "--tmpfs", "/run", "--dir", "/run/lock",
            "--bind", str(self.bin), "/usr/bin",
            "--bind", str(self.root / "storage"), "/usr/lib/spacewars-fast-update",
            "--bind", str(self.root / "share"), "/usr/share/spacewars",
            "--bind", str(self.root / "tmp"), "/tmp",
            "--bind", str(self.control), "/test",
            "--ro-bind", str(HELPER), "/helper.sh",
            "--chdir", "/", "--", "/bin/bash", "/helper.sh", *args,
        ], capture_output=True, text=True, timeout=15)
        self.assertNotIn("bwrap:", result.stderr, result.stderr)
        return result

    def install(self, *args):
        return self.run_helper(*(args or (STAGE, COMPAT, *self.hashes)))

    def assert_old_pair(self):
        for name in ("engine-client", "spacewars-cli"):
            self.assertEqual((self.bin / name).read_bytes(), b"old-" + name.encode())

    def test_probe_is_read_only(self):
        result = self.run_helper("--check")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), f"spacewars-fast-update-v1 {COMPAT}")
        self.assertFalse((self.control / "calls").exists())
        self.assert_old_pair()

    def test_success_installs_pair_and_retains_previous_binaries(self):
        result = self.install()
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in ("engine-client", "spacewars-cli"):
            self.assertEqual((self.bin / name).read_bytes(), (self.stage / name).read_bytes())
            self.assertEqual((self.root / "storage" / f"{name}.previous").read_bytes(), b"old-" + name.encode())
        calls = (self.control / "calls").read_text().splitlines()
        self.assertEqual(calls.count("stop"), 1)
        self.assertEqual(calls.count("start"), 1)
        self.assertEqual(calls.count("status"), 3)
        self.assertIn("Fast update verified", result.stdout)

    def test_corrupt_copy_is_rejected_before_stopping(self):
        (self.stage / "spacewars-cli").write_bytes(b"corrupt")
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("checksum mismatch", result.stderr)
        self.assertNotIn("stop", (self.control / "calls").read_text())
        self.assert_old_pair()

    def test_incompatible_runtime_and_unsafe_staging_are_rejected(self):
        for stage, compat in ((STAGE, "b" * 64), ("/tmp", COMPAT), ("/tmp/spacewars-fast.A123456789/../", COMPAT)):
            with self.subTest(stage=stage, compat=compat):
                self.assertNotEqual(self.install(stage, compat, *self.hashes).returncode, 0)
                self.assertFalse((self.control / "calls").exists())
                self.assert_old_pair()

    def test_wrong_architecture_is_rejected_even_with_matching_checksum(self):
        binary = bytearray((self.stage / "engine-client").read_bytes())
        binary[18:20] = bytes.fromhex("3e00")
        (self.stage / "engine-client").write_bytes(binary)
        self.hashes[0] = hashlib.sha256(binary).hexdigest()
        result = self.install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not an AArch64", result.stderr)
        self.assertNotIn("stop", (self.control / "calls").read_text())
        self.assert_old_pair()

    def test_rollback_after_partial_install_failed_start_or_unhealthy_app(self):
        for failure in ("fail-second-rename", "fail-start", "fail-health"):
            with self.subTest(failure=failure):
                (self.control / failure).touch()
                result = self.install()
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("restoring the previous", result.stderr)
                self.assert_old_pair()
                self.assertEqual((self.control / "state").read_text().strip(), "active")
                self.assertEqual(list((self.root / "storage").glob("transaction.*")), [])
                (self.control / failure).unlink()


if __name__ == "__main__":
    unittest.main()
