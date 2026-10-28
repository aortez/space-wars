"""Persistent logging with real journald in disposable mount/PID namespaces."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


YOCTO = Path(__file__).resolve().parents[1]
RECIPE = YOCTO / "meta-spacewars/recipes-support/spacewars-journal"
FILES = RECIPE / "files"
MACHINE_A = "a" * 32
MACHINE_B = "b" * 32


class JournalTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.bwrap = shutil.which("bwrap")
        reason = "Install bubblewrap and systemd to test persistent journald"
        if cls.bwrap and Path("/usr/lib/systemd/systemd-journald").exists():
            probe = subprocess.run(
                [cls.bwrap, "--ro-bind", "/", "/", "--unshare-user", "--uid", "0", "--gid", "0",
                 "--unshare-pid", "--unshare-net", "--proc", "/proc", "--", "true"],
                capture_output=True, text=True,
            )
            reason = probe.stderr if probe.returncode else None
        if reason:
            if os.environ.get("SPACEWARS_REQUIRE_UPDATE_SANDBOX") == "1":
                raise RuntimeError(reason)
            raise unittest.SkipTest(reason)

    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="spacewars-journal-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.data = self.root / "data"
        self.data.mkdir()
        self.machine = self.root / "machine-id"
        self.boot_id = self.root / "boot-id"
        self.machine.write_text(MACHINE_A + "\n")
        self.boot_id.write_text("11111111-1111-1111-1111-111111111111\n")
        # Map the journal group to namespace root; no host accounts are modified.
        self.group = self.root / "group"
        self.group.write_text("root:x:0:\nsystemd-journal:x:0:\n")
        self.fixture_config = self.root / "fixture.conf"
        self.fixture_config.write_text("[Journal]\nReadKMsg=no\nRateLimitIntervalSec=0\n")

    def sandbox(self, command, mounted=True, bind_journal=False, timeout=30):
        args = [
            self.bwrap, "--clearenv", "--setenv", "PATH", "/usr/bin:/bin", "--setenv", "LC_ALL", "C",
            "--ro-bind", "/usr", "/usr", "--ro-bind", "/lib", "/lib",
            "--ro-bind-try", "/lib64", "/lib64", "--symlink", "usr/bin", "/bin",
            "--ro-bind", "/etc/passwd", "/etc/passwd", "--ro-bind", str(self.group), "/etc/group",
            "--ro-bind", str(self.machine), "/etc/machine-id",
            "--ro-bind", str(FILES / "spacewars-journal.conf"), "/etc/systemd/journald.conf.d/spacewars-journal.conf",
            "--ro-bind", str(self.fixture_config), "/etc/systemd/journald.conf.d/zz-fixture.conf",
            "--unshare-user", "--uid", "0", "--gid", "0", "--unshare-pid", "--unshare-net",
            "--unshare-uts", "--hostname", "journal-fixture", "--die-with-parent",
            "--proc", "/proc", "--dev", "/dev", "--ro-bind", "/sys", "/sys",
            "--ro-bind", str(self.boot_id), "/proc/sys/kernel/random/boot_id",
            "--tmpfs", "/run", "--tmpfs", "/tmp", "--tmpfs", "/var/tmp",
            "--dir", "/var/volatile/log", "--symlink", "volatile/log", "/var/log",
            "--ro-bind", str(FILES / "spacewars-journal-prepare.sh"), "/prepare.sh",
            "--ro-bind", str(Path(__file__).with_name("journal_fixture.py")), "/fixture.py",
        ]
        args += ["--bind", str(self.data), "/data"] if mounted else ["--dir", "/data"]
        if bind_journal:
            machine = self.machine.read_text().strip()
            args += ["--bind", str(self.data / "spacewars-journal"), f"/var/log/journal/{machine}"]
        return subprocess.run(args + ["--chdir", "/", "--", *command], capture_output=True, text=True, timeout=timeout)

    def boot(self, machine, number, crash=False, flood=False):
        self.machine.write_text(machine + "\n")
        self.boot_id.write_text(f"{number:08x}-1111-1111-1111-111111111111\n")
        prepared = self.sandbox(["sh", "/prepare.sh"])
        self.assertEqual(prepared.returncode, 0, prepared.stderr)
        result = self.sandbox(
            ["python3", "/fixture.py", f"boot-{number}", "crash" if crash else "clean", "flood" if flood else "quiet"],
            bind_journal=True, timeout=120,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)

    def read(self, *args):
        result = self.sandbox(["journalctl", "--directory=/data/spacewars-journal", "--no-pager", *args])
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def test_reboot_trial_crash_and_rollback_preserve_readable_boots(self):
        self.boot(MACHINE_A, 1)
        self.boot(MACHINE_A, 2)
        self.boot(MACHINE_B, 3, crash=True)
        self.boot(MACHINE_A, 4)
        history = self.read("--output=json", "SYSLOG_IDENTIFIER=journal-fixture")
        entries = [json.loads(line) for line in history.splitlines()]
        for number in range(1, 5):
            for suffix in ("-early", "", "-last"):
                found = [e for e in entries if e["MESSAGE"] == f"boot-{number}{suffix}"]
                self.assertEqual(len(found), 1, history)
                self.assertEqual(found[0]["_MACHINE_ID"], MACHINE_B if number == 3 else MACHINE_A)
                self.assertEqual(found[0]["_BOOT_ID"], f"{number:08x}" + "1" * 24)
        self.assertNotIn("shutdown-ram", history)
        previous = self.read("--boot=-1", "--output=cat")
        self.assertIn("boot-3-last", previous)
        self.assertNotIn("boot-4-last", previous)
        boots = self.read("--list-boots")
        for number in range(1, 5):
            self.assertIn(f"{number:08x}" + "1" * 24, boots)
        self.read("--verify")

    def test_rotation_caps_apply_to_combined_slot_history(self):
        self.boot(MACHINE_A, 1, flood=True)
        result = self.boot(MACHINE_B, 2, flood=True)
        store = self.data / "spacewars-journal"
        usage = sum(p.stat().st_blocks * 512 for p in store.iterdir())
        self.assertLessEqual(result["peak_bytes"], 128 * 1024 * 1024)
        self.assertLessEqual(usage, 128 * 1024 * 1024)
        self.assertLessEqual(len(list(store.iterdir())), 32)
        self.assertGreater(len(list(store.iterdir())), 1, "Fixture never rotated")
        self.assertIn("boot-2-last", self.read("--output=cat", "--lines=1"))
        expired = self.sandbox([
            "journalctl", "--directory=/data/spacewars-journal", "--no-pager",
            "--output=cat", "--grep=boot-1-early",
        ])
        self.assertEqual(expired.returncode, 1, expired.stderr)
        self.assertEqual(expired.stdout, "")
        self.read("--verify")

    def test_missing_mount_invalid_identity_and_unsafe_paths_fail(self):
        result = self.sandbox(["sh", "/prepare.sh"], mounted=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("requires mounted /data", result.stderr)
        self.assertEqual(list(self.data.iterdir()), [])
        for machine in ("", "0" * 32, "../bad", "a" * 31, "g" * 32):
            self.machine.write_text(machine)
            self.assertNotEqual(self.sandbox(["sh", "/prepare.sh"]).returncode, 0)
        self.machine.write_text(MACHINE_A)
        store = self.data / "spacewars-journal"
        store.symlink_to("/var/log")
        result = self.sandbox(["sh", "/prepare.sh"])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Invalid journal directory", result.stderr)
        store.unlink()
        store.write_text("not a directory")
        result = self.sandbox(["sh", "/prepare.sh"])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Invalid journal directory", result.stderr)


class JournalPackagingTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="spacewars-journal-image-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        recipe = (RECIPE / "spacewars-journal.bb").read_text()
        install = recipe.split("do_install() {\n", 1)[1].split("\n}", 1)[0]
        variables = {
            "D": str(self.root), "WORKDIR": str(FILES), "sbindir": "/usr/sbin",
            "systemd_system_unitdir": "/usr/lib/systemd/system", "sysconfdir": "/etc",
        }
        install = re.sub(r"\$\{(\w+)\}", lambda match: variables[match[1]], install)
        subprocess.run(["sh", "-ec", install], check=True)
        self.units = self.root / "usr/lib/systemd/system"

    def test_image_installs_configuration_and_flush_dependency(self):
        image = (YOCTO / "meta-spacewars/recipes-core/images/spacewars-image.bb").read_text()
        self.assertIn("    spacewars-journal \\\n", image)
        self.assertTrue(os.access(self.root / "usr/sbin/spacewars-journal-prepare", os.X_OK))
        config = self.root / "etc/systemd/journald.conf.d/spacewars-journal.conf"
        self.assertEqual(config.read_bytes(), (FILES / "spacewars-journal.conf").read_bytes())
        unit = (self.units / "spacewars-journal.service").read_text()
        self.assertIn("Requires=data.mount", unit)
        self.assertIn("After=data.mount", unit)
        self.assertIn("--bind /data/spacewars-journal /var/log/journal/%m", unit)
        flush = (self.units / "systemd-journal-flush.service.d/spacewars-persistent-journal.conf").read_text()
        self.assertIn("Requires=spacewars-journal.service", flush)
        self.assertIn("After=spacewars-journal.service", flush)
        self.assertIn("ExecStop=/usr/bin/journalctl --relinquish-var", flush)

    def test_early_flush_graph_has_no_ordering_cycle(self):
        analyze = shutil.which("systemd-analyze")
        if not analyze:
            self.skipTest("Install systemd for unit dependency verification")
        # Model the upstream early-boot ordering: flush precedes tmpfiles,
        # tmpfiles precedes sysinit, and mounts precede local-fs. A normal
        # service's implicit After=sysinit would create an ordering cycle.
        units = {
            "sysinit.target": "[Unit]\nWants=local-fs.target systemd-tmpfiles-setup.service systemd-journal-flush.service\nAfter=local-fs.target systemd-tmpfiles-setup.service\n",
            "local-fs.target": "[Unit]\nDefaultDependencies=no\nWants=data.mount var-volatile.mount\nAfter=data.mount var-volatile.mount\n",
            "local-fs-pre.target": "[Unit]\nDefaultDependencies=no\n",
            "shutdown.target": "[Unit]\nDefaultDependencies=no\n",
            "umount.target": "[Unit]\nDefaultDependencies=no\n",
            "systemd-remount-fs.service": "[Unit]\nDefaultDependencies=no\n[Service]\nType=oneshot\nExecStart=/bin/true\n",
            "systemd-tmpfiles-setup.service": "[Unit]\nDefaultDependencies=no\nAfter=local-fs.target systemd-journal-flush.service\nBefore=sysinit.target\n[Service]\nType=oneshot\nExecStart=/bin/true\n",
            "systemd-journal-flush.service": "[Unit]\nDefaultDependencies=no\nAfter=systemd-remount-fs.service\nBefore=systemd-tmpfiles-setup.service\nRequiresMountsFor=/var/log/journal\n[Service]\nType=oneshot\nExecStart=/usr/bin/journalctl --flush\nExecStop=/usr/bin/journalctl --smart-relinquish-var\nRemainAfterExit=yes\n",
        }
        for name, where in (("data.mount", "/data"), ("var-volatile.mount", "/var/volatile")):
            units[name] = f"[Unit]\nAfter=local-fs-pre.target\nBefore=local-fs.target\n[Mount]\nWhat=tmpfs\nWhere={where}\nType=tmpfs\n"
        for name, body in units.items():
            (self.units / name).write_text(body)
        (self.root / "etc/machine-id").write_text(MACHINE_A + "\n")
        (self.root / "var/volatile/log").mkdir(parents=True)
        (self.root / "var/log").symlink_to("volatile/log")
        for name in ("bin/true", "bin/mount", "bin/umount", "usr/bin/journalctl"):
            executable = self.root / name
            executable.parent.mkdir(parents=True, exist_ok=True)
            executable.write_text("#!/bin/sh\nexit 0\n")
            executable.chmod(0o755)
        result = subprocess.run(
            [analyze, f"--root={self.root}", "--man=no", "verify", "sysinit.target", "spacewars-journal.service"],
            capture_output=True, text=True, timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("cycle", result.stderr.lower())


if __name__ == "__main__":
    unittest.main()
