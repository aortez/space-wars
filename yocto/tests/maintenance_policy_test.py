"""Legacy mutation entry points must reject before inspecting devices or paths."""
from pathlib import Path
import subprocess
import unittest

FILES = Path(__file__).resolve().parents[1] / "meta-spacewars/recipes-support/ab-boot/files"


class LegacyMaintenanceTest(unittest.TestCase):
    def test_unsigned_installer_and_direct_slot_switch_are_disabled(self):
        for name, args in [
            ("spacewars-legacy-update-disabled", []),
            ("spacewars-legacy-update-disabled", ["/dev/sda2", "/tmp/arbitrary-key", "root"]),
            ("ab-boot-manager", ["switch", "a"]),
            ("ab-boot-manager", ["switch", "b"]),
            ("ab-boot-manager", ["switch", "a;reboot"]),
        ]:
            with self.subTest(name=name, args=args):
                result = subprocess.run(["/bin/sh", str(FILES / name), *args], capture_output=True, text=True, timeout=2)
                self.assertEqual(result.returncode, 1)
                self.assertIn("disabled", result.stderr)
                self.assertFalse(result.stdout)


if __name__ == "__main__":
    unittest.main()
