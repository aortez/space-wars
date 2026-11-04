"""Exercise the real renderer helper with isolated files and fake service control."""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


FILES = Path(__file__).resolve().parents[1] / "meta-spacewars/recipes-spacewars/spacewars/files"


class RendererSwitchTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        reason = "Install bubblewrap to test the renderer switch safely"
        if shutil.which("bwrap"):
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
        directory = tempfile.TemporaryDirectory(prefix="spacewars-renderer-test-")
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.bin = self.root / "bin"
        self.control = self.root / "control"
        for path in (self.bin, self.control, self.root / "run/lock", self.root / "boot"):
            path.mkdir(parents=True)
        self.override = self.root / "run/spacewars-renderer/renderer.env"
        for name in ("bash", "chmod", "env", "flock", "id", "install", "mktemp", "mv", "rm", "timeout"):
            (self.bin / name).symlink_to(f"/host/usr/bin/{name}")
        (self.control / "state").write_text("active\n")
        (self.control / "pid").write_text("100\n")
        (self.root / "boot/spacewars-kiosk.env").write_text("SLINT_KMS_ROTATION=90\n")
        self.command("sleep", ":")
        self.command("systemctl", '''
[[ $1 == --no-pager && ${*: -1} == spacewars-kiosk.service ]] || exit 90
[[ -z ${DBUS_SYSTEM_BUS_ADDRESS:-} && -z ${SYSTEMD_BUS_ADDRESS:-} ]] || exit 91
shift
echo "$*" >> /test/calls
mode=software
if [[ -f /run/spacewars-renderer/renderer.env ]]; then
    setting=$(< /run/spacewars-renderer/renderer.env)
    mode=${setting#SLINT_BACKEND=linuxkms-}
fi
case "$1" in
    stop)
        [[ ! -f /test/fail-stop ]] || exit 1
        echo stopped > /test/state ;;
    reset-failed) : ;;
    start)
        [[ $mode != femtovg || ! -f /test/fail-start ]] || exit 1
        echo $(( $(< /test/pid) + 1 )) > /test/pid
        echo active > /test/state ;;
    is-active) [[ $(< /test/state) == active ]] ;;
    show)
        if [[ $mode == femtovg && -f /test/crash-loop ]]; then
            echo $(( $(< /test/pid) + 1 )) > /test/pid
        fi
        echo "$(< /test/pid)" ;;
    *) exit 92 ;;
esac
''')
        self.command("setpriv", '''
[[ $1 == --reuid=spacewars && $2 == --regid=spacewars
   && $3 == --init-groups && $4 == --no-new-privs ]] || exit 93
shift 4
[[ $* == 'timeout 2 /usr/bin/spacewars-cli status' ]] || exit 94
exec "$@"
''')
        self.command("spacewars-cli", '''
echo status >> /test/calls
mode=$(< /run/spacewars-renderer/renderer.env)
mode=${mode#SLINT_BACKEND=linuxkms-}
if [[ $mode == femtovg ]]; then
    [[ ! -f /test/fail-health ]] || exit 1
    if [[ -f /test/wrong-renderer ]]; then mode=software; fi
fi
printf 'scenario=clock\\nkms_renderer=%s\\n' "$mode"
''')

    def command(self, name, body):
        target = self.bin / name
        target.write_text("#!/bin/bash\nset -eu\n" + body + "\n")
        target.chmod(0o755)

    def run_helper(self, *args):
        command = ["/bin/bash", "/helper.sh", *args]
        result = subprocess.run([
            "bwrap", "--ro-bind", "/usr", "/host/usr",
            "--ro-bind", "/lib", "/lib", "--ro-bind-try", "/lib64", "/lib64",
            "--ro-bind", "/etc", "/etc", "--symlink", "usr/bin", "/bin",
            "--unshare-user", "--uid", "0", "--gid", "0", "--unshare-pid", "--unshare-net",
            "--die-with-parent", "--proc", "/proc", "--dev", "/dev",
            "--bind", str(self.bin), "/usr/bin",
            "--bind", str(self.root / "run"), "/run",
            "--ro-bind", str(self.root / "boot"), "/boot",
            "--bind", str(self.control), "/test",
            "--ro-bind", str(FILES / "spacewars-renderer.sh"), "/helper.sh",
            "--chdir", "/", "--", *command,
        ], capture_output=True, text=True, timeout=15,
            env=dict(os.environ, DBUS_SYSTEM_BUS_ADDRESS="not-the-system-bus",
                     SYSTEMD_BUS_ADDRESS="not-the-system-bus"))
        self.assertNotIn("bwrap:", result.stderr, result.stderr)
        return result

    def test_only_two_exact_modes_are_accepted_without_side_effects(self):
        for args in [(), ("gpu", "extra"), ("gpu; reboot",), ("GPU",),
                     ("/boot/config.txt",), ("software\nX=bad",), ("--help",)]:
            with self.subTest(args=args):
                result = self.run_helper(*args)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Usage:", result.stderr)
                self.assertFalse((self.control / "calls").exists())
                self.assertFalse(self.override.exists())

    def test_switch_both_ways_is_temporary_preserves_boot_and_checks_actual_renderer(self):
        for mode, backend in [("gpu", "femtovg"), ("software", "software")]:
            with self.subTest(mode=mode):
                result = self.run_helper(mode)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.override.read_text(), f"SLINT_BACKEND=linuxkms-{backend}\n")
                self.assertEqual(self.override.stat().st_mode & 0o777, 0o644)
                self.assertIn(f"Verified {backend}", result.stdout)
                self.assertEqual(list(self.override.parent.glob(".renderer.*")), [])
        self.assertEqual((self.root / "boot/spacewars-kiosk.env").read_text(), "SLINT_KMS_ROTATION=90\n")
        self.assertEqual((self.control / "calls").read_text().splitlines().count("status"), 6)

    def test_failed_gpu_start_health_wrong_backend_and_crash_loops_restore_software(self):
        for failure in ("fail-start", "fail-health", "wrong-renderer", "crash-loop"):
            with self.subTest(failure=failure):
                (self.control / failure).touch()
                result = self.run_helper("gpu")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Software renderer restored and verified", result.stderr)
                self.assertEqual(self.override.read_text(), "SLINT_BACKEND=linuxkms-software\n")
                self.assertEqual((self.control / "state").read_text(), "active\n")
                (self.control / failure).unlink()

    def test_failed_stop_does_not_modify_configuration(self):
        (self.control / "fail-stop").touch()
        result = self.run_helper("gpu")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(self.override.exists())

    def test_recovery_from_stopped_service_and_symlink_rejection(self):
        (self.control / "state").write_text("stopped\n")
        result = self.run_helper("software")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.override.unlink()
        self.override.symlink_to("/boot/spacewars-kiosk.env")
        result = self.run_helper("gpu")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unsafe renderer override", result.stderr)
        self.assertEqual((self.root / "boot/spacewars-kiosk.env").read_text(), "SLINT_KMS_ROTATION=90\n")


class RendererPackagingTest(unittest.TestCase):
    def test_sudo_only_permits_two_exact_fixed_helper_invocations(self):
        rule = FILES / "spacewars-renderer.sudoers"
        lines = [line for line in rule.read_text().splitlines() if line and not line.startswith("#")]
        self.assertEqual(lines, ["spacewars ALL=(root) NOPASSWD: /usr/sbin/spacewars-renderer software, /usr/sbin/spacewars-renderer gpu"])
        if shutil.which("visudo"):
            result = subprocess.run(["visudo", "-cf", str(rule)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_only_experimental_image_installs_and_fingerprints_helper_and_dropin(self):
        recipe = (FILES.parent / "spacewars_git.bb").read_text()
        install = recipe.split('if [ "${SPACEWARS_GPU}" = "1" ]; then', 1)[1].split("fi", 1)[0]
        assets = recipe.split('if d.getVar("SPACEWARS_GPU") == "1":', 1)[1].split("variables =", 1)[0]
        for name in ("spacewars-renderer.sh", "spacewars-renderer.sudoers", "spacewars-renderer.conf"):
            self.assertIn(name, install)
            self.assertIn(f'"{name}"', assets)
            self.assertIn(f"/files/{name}:True", recipe)
            self.assertIn(f"file://{name}", recipe)
        self.assertEqual((FILES / "spacewars-renderer.conf").read_text().splitlines()[-1],
                         "EnvironmentFile=-/run/spacewars-renderer/renderer.env")


if __name__ == "__main__":
    unittest.main()
