"""Exercise prelaunch isolation checks with private mounts and inert daemons."""
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

LAUNCHER = Path(__file__).with_name("nm-profile-refresh.sh")
REMOTE = "/data/spacewars-nm-qualification-fixture"
BINDS = {
    "etc": "/etc/NetworkManager",
    "run": "/run/NetworkManager",
    "state": "/var/lib/NetworkManager",
    "bus-denied": "/run/dbus",
    "udev-denied": "/run/udev",
}


class NmLauncherTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.bwrap = shutil.which("bwrap")
        reason = "Install bubblewrap for native launcher isolation tests"
        if cls.bwrap:
            probe = subprocess.run([cls.bwrap, "--ro-bind", "/", "/", "--unshare-user",
                                    "--uid", "0", "--gid", "0", "--unshare-net", "--", "true"],
                                   capture_output=True, text=True)
            reason = probe.stderr if probe.returncode else None
        if reason:
            if os.environ.get("SPACEWARS_REQUIRE_UPDATE_SANDBOX") == "1":
                raise RuntimeError(reason)
            raise unittest.SkipTest(reason)

    def run_case(self, fault=None):
        with tempfile.TemporaryDirectory(prefix="nm-launcher-") as temporary:
            root = Path(temporary)
            case = root / "candidate"
            case.mkdir(mode=0o700)
            for name in BINDS:
                (case / name).mkdir(mode=0o700)
            (case / "etc/conf.d").mkdir()
            (root / "out").mkdir()
            stubs = root / "stubs"
            stubs.mkdir()
            # PID 1 namespace links are often unreadable in a user namespace.
            # These synthetic links exercise the launcher's checks; bwrap also
            # provides real private network/mount/device isolation independently.
            for process, suffix in [("1", "host"), ("self", "private")]:
                ns = root / "proc" / process / "ns"
                ns.mkdir(parents=True)
                for kind in ["net", "mnt"]:
                    (ns / kind).symlink_to(f"{kind}:[{suffix}]")
            scripts = {
                "dbus-daemon": '''#!/usr/bin/python3
import os, socket, time
open('/out/bus-started', 'w').close()
bus = socket.socket(socket.AF_UNIX)
bus.bind(os.environ['DBUS_SYSTEM_BUS_ADDRESS'].removeprefix('unix:path='))
time.sleep(20)
''',
                "busctl": "#!/bin/sh\nprintf 'b false\\n'\n",
                "dbus-monitor": "#!/bin/sh\nsleep 20\n",
            }
            for name, content in scripts.items():
                (stubs / name).write_text(content)
                (stubs / name).chmod(0o500)
            for name, content in {
                "NetworkManager": "#!/bin/sh\ntouch /out/nm-started\nsleep 20\n",
                "nm-profile-refresh": "#!/bin/sh\ntouch /out/test-started\n",
            }.items():
                (root / name).write_text(content)
                (root / name).chmod(0o500)
            args = [self.bwrap, "--ro-bind", "/usr", "/usr", "--ro-bind", "/lib", "/lib",
                    "--ro-bind-try", "/lib64", "/lib64", "--symlink", "usr/bin", "/bin",
                    "--unshare-user", "--uid", "0", "--gid", "0",
                    "--unshare-net", "--die-with-parent", "--dev", "/dev", "--tmpfs", "/run",
                    "--tmpfs", "/data", "--bind", str(root), REMOTE,
                    "--bind", str(root / "out"), "/out", "--ro-bind", str(stubs), "/stubs",
                    "--ro-bind", str(root / "proc"), "/proc", "--ro-bind", str(LAUNCHER), "/launcher",
                    "--setenv", "PATH", "/stubs:/usr/bin:/bin"]
            for name, destination in BINDS.items():
                if fault == name:
                    args += ["--dir", destination]
                else:
                    args += ["--bind", str(case / name), destination]
            if fault == "rfkill":
                (root / "fake-rfkill").write_text("inert ordinary file, not a device")
                args += ["--ro-bind", str(root / "fake-rfkill"), "/dev/rfkill"]
            if fault in ("bus-content", "udev-content"):
                name = "bus-denied" if fault == "bus-content" else "udev-denied"
                (case / name / "unexpected").touch()
            if fault == "missing-host-namespace":
                (root / "proc/1/ns/net").unlink()
            if fault == "same-namespace":
                (root / "proc/self/ns/net").unlink()
                (root / "proc/self/ns/net").symlink_to("net:[host]")
            args += ["--", "/bin/bash", "/launcher", REMOTE + "/candidate", REMOTE + "/NetworkManager"]
            result = subprocess.run(args, capture_output=True, text=True, timeout=10)
            markers = {path.name for path in (root / "out").iterdir()}
            return result, markers

    def test_complete_isolation_reaches_only_the_inert_daemons_and_test(self):
        result, markers = self.run_case()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(markers, {"bus-started", "nm-started", "test-started"})

    def test_incomplete_isolation_rejects_before_bus_or_daemon_start(self):
        for fault in ["rfkill", *BINDS, "bus-content", "udev-content",
                      "missing-host-namespace", "same-namespace"]:
            with self.subTest(fault=fault):
                result, markers = self.run_case(fault)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn("bwrap:", result.stderr)
                self.assertEqual(markers, set(), result.stderr)


if __name__ == "__main__":
    unittest.main()
