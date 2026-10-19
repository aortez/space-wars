"""Keep service namespace evidence and unused listener masks aligned with Rust."""
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


class NetworkServiceContractTest(unittest.TestCase):
    def test_namespace_name_and_graceful_read_only_handoff(self):
        rust = (ROOT / "crates/engine-os-manager/src/network_sessions.rs").read_text()
        name = re.search(r'const HOST_NAMESPACE_NAME: &str = "([^"]+)";', rust).group(1)
        unit = (ROOT / "yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-system.service").read_text()
        directives = [line for line in unit.splitlines() if line.startswith("OpenFile=")]
        self.assertEqual(directives, [f"OpenFile=/proc/1/ns/net:{name}:read-only,graceful"])
        self.assertIn("ExecStart=/usr/bin/engine-os-manager serve", unit)

    def test_image_masks_all_three_standalone_listeners(self):
        recipe = (ROOT / "yocto/meta-spacewars/recipes-core/images/spacewars-image.bb").read_text()
        body = re.search(r'disable_unused_network_servers\(\) \{(.*?)\n\}', recipe, re.S).group(1)
        units = re.search(r'for unit in ([^;]+); do', body).group(1).split()
        self.assertEqual(set(units), {"dnsmasq.service", "rpcbind.service", "rpcbind.socket"})
        self.assertIn('ln -sf /dev/null ${IMAGE_ROOTFS}/etc/systemd/system/$unit', body)
        self.assertIn('ROOTFS_POSTPROCESS_COMMAND:append = " disable_unused_network_servers;"', recipe)


if __name__ == "__main__":
    unittest.main()
