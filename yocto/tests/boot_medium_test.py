"""Keep one flashed image bootable from a USB disk or the Pi's card slot."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RECIPE = ROOT / "yocto/meta-spacewars/recipes-core/images/spacewars-image.bb"


class BootMediumTest(unittest.TestCase):
    def test_flashed_root_keeps_device_names_out_of_its_fstab(self):
        # wic would mount /boot and /data from /dev/sda1 and /dev/sda4. A card
        # boot then waits for /dev/sda4 and stops in emergency mode.
        recipe = RECIPE.read_text()
        self.assertIn('WIC_CREATE_EXTRA_ARGS:append = " --no-fstab-update"', recipe)
        mount = re.search(r"boot\.mount << 'EOF'\n(.*?)\nEOF", recipe, re.S).group(1)
        self.assertIn("What=/dev/disk/by-label/boot", mount)


if __name__ == "__main__":
    unittest.main()
