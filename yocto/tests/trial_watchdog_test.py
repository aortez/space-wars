"""Exercise the actual early-watchdog helper with all device I/O intercepted."""
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE = ROOT / "meta-spacewars/recipes-support/spacewars-trial-watchdog/files/spacewars-trial-watchdog.c"
FIXTURE = pathlib.Path(__file__).with_name("trial_watchdog_fixture.c")
WRAPPED = ("geteuid", "open", "read", "write", "close", "ioctl", "mkdir", "lstat", "fsync", "clock_gettime")


class TrialWatchdogTest(unittest.TestCase):
    def test_actual_helper_with_intercepted_syscalls(self):
        with tempfile.TemporaryDirectory(prefix="spacewars-watchdog-") as directory:
            obj = pathlib.Path(directory) / "helper.o"
            executable = pathlib.Path(directory) / "fixture"
            flags = ["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-Wconversion", "-Wshadow"]
            subprocess.run(["gcc", *flags, "-Dmain=watchdog_entry", "-c", str(SOURCE), "-o", str(obj)], check=True)
            subprocess.run(["gcc", *flags, str(FIXTURE), str(obj), "-o", str(executable),
                            *[f"-Wl,--wrap={name}" for name in WRAPPED]], check=True)
            result = subprocess.run([str(executable)], text=True, capture_output=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("21 syscall-fixture cases passed", result.stdout)


if __name__ == "__main__":
    unittest.main()
