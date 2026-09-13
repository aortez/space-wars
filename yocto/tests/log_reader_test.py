"""Run the real restricted log reader with a fake journal in a mount namespace."""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


FILES = Path(__file__).resolve().parents[1] / "meta-spacewars/recipes-spacewars/spacewars/files"
HELPER = FILES / "spacewars-logs.sh"


class LogReaderTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.bwrap = shutil.which("bwrap")
        reason = "Install bubblewrap to test privileged log-reader isolation"
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
        temporary = tempfile.TemporaryDirectory(prefix="spacewars-log-reader-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.journal = self.root / "journalctl"
        self.journal.write_text("#!/bin/sh\nprintf 'ARG:%s\\n' \"$@\"\n/usr/bin/env\n")
        self.journal.chmod(0o755)

    def run_helper(self, *args, environment=None):
        return subprocess.run([
            self.bwrap, "--ro-bind", "/usr", "/usr",
            "--ro-bind", "/lib", "/lib", "--ro-bind-try", "/lib64", "/lib64",
            "--symlink", "usr/bin", "/bin",
            "--unshare-user", "--uid", "0", "--gid", "0", "--unshare-pid", "--unshare-net",
            "--die-with-parent", "--proc", "/proc", "--dev", "/dev",
            "--ro-bind", str(self.journal), "/usr/bin/journalctl",
            "--ro-bind", str(HELPER), "/helper.sh",
            "--chdir", "/", "--", "/bin/sh", "/helper.sh", *args,
        ], capture_output=True, text=True, timeout=10, env=environment)

    def test_tail_is_fixed_unit_current_boot_bounded_and_without_pager(self):
        for count in ("1", "200", "1000"):
            with self.subTest(count=count):
                result = self.run_helper("--lines", count)
                self.assertEqual(result.returncode, 0, result.stderr)
                arguments = [line[4:] for line in result.stdout.splitlines() if line.startswith("ARG:")]
                self.assertEqual(arguments, [
                    "--no-pager", "--quiet", "--boot=0", "--unit=spacewars-kiosk.service",
                    "--output=short-iso-precise", f"--lines={count}",
                ])

    def test_follow_is_the_only_optional_argument(self):
        result = self.run_helper("--lines", "20", "--follow")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("ARG:--follow\n", result.stdout)

    def test_invalid_inputs_never_invoke_journal(self):
        cases = [(), ("--follow",), ("--lines",), ("--lines", "200", "--follow", "extra"),
                 ("--unit", "ssh.service"), ("--lines", "20", "--file=/tmp/private"),
                 ("--lines", "20", "--vacuum-size=1M"), ("--lines", "20", "--unit=ssh.service")]
        cases += [("--lines", value) for value in (
            "", "0", "1001", "9999999999999999999999999", "020", "+1", "-1", "all",
            "20 --follow", "20; id", "20\n--unit=ssh.service", "$(id)",
        )]
        for args in cases:
            with self.subTest(args=args):
                result = self.run_helper(*args)
                self.assertEqual(result.returncode, 2, result.stderr)
                self.assertEqual(result.stdout, "")
                self.assertIn("Usage:", result.stderr)

    def test_caller_environment_cannot_change_journal_or_pager(self):
        environment = dict(os.environ, PATH="/not/a/path", SYSTEMD_PAGER="bad-pager",
                           PAGER="bad-pager", SYSTEMD_LOG_TARGET="file:/tmp/private",
                           SYSTEMD_COLORS="1", JOURNAL_STREAM="bad-stream")
        result = self.run_helper("--lines", "200", environment=environment)
        self.assertEqual(result.returncode, 0, result.stderr)
        for key in ("SYSTEMD_PAGER", "PAGER", "SYSTEMD_LOG_TARGET", "SYSTEMD_COLORS", "JOURNAL_STREAM"):
            self.assertNotIn(key + "=", result.stdout)
        self.assertIn("PATH=/usr/bin:/bin\n", result.stdout)

    def test_journal_failure_is_not_reported_as_success(self):
        self.journal.write_text("#!/bin/sh\necho 'journal unavailable' >&2\nexit 7\n")
        result = self.run_helper("--lines", "200")
        self.assertEqual(result.returncode, 7)
        self.assertIn("journal unavailable", result.stderr)


class LogReaderPackagingTest(unittest.TestCase):
    def test_sudo_rule_is_valid_and_only_grants_the_fixed_helper(self):
        rule = FILES / "spacewars-logs.sudoers"
        entries = [line for line in rule.read_text().splitlines() if line and not line.startswith("#")]
        self.assertEqual(entries, ["spacewars ALL=(root) NOPASSWD: /usr/sbin/spacewars-logs *"])
        visudo = shutil.which("visudo")
        if visudo:
            result = subprocess.run([visudo, "-cf", str(rule)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_helper_and_policy_are_installed_and_fingerprinted(self):
        recipe = (FILES.parent / "spacewars_git.bb").read_text()
        for asset in ("spacewars-logs.sh", "spacewars-logs.sudoers"):
            self.assertIn(f"file://{asset}", recipe)
            self.assertIn(f'"{asset}"', recipe.split("assets =", 1)[1].split("variables =", 1)[0])
            self.assertIn(f"/files/{asset}:True", recipe)
        self.assertIn("install -m 0755 ${WORKDIR}/spacewars-logs.sh", recipe)
        self.assertIn("install -m 0440 ${WORKDIR}/spacewars-logs.sudoers", recipe)


if __name__ == "__main__":
    unittest.main()
