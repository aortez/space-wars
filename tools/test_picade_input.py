"""Host-only tests; never connect to a device or inject live input."""

import unittest
from unittest.mock import patch

import picade_input as picade


INVENTORY = """sw-picade-2
aarch64
N: Name="Space-Wars Picade Keys"
H: Handlers=kbd event3

N: Name="Space-Wars Picade"
H: Handlers=event4 js0
"""


class PicadeInputTests(unittest.TestCase):
    def test_discovers_named_devices_not_fixed_event_numbers(self):
        inventory = INVENTORY.replace("event3", "event7").replace("event4", "event8")
        self.assertEqual(picade.discover(inventory, "sw-picade-2.local"),
                         {picade.KEYS: "event7", picade.PAD: "event8"})

    def test_rejects_wrong_host_architecture_or_ambiguous_devices(self):
        for inventory in (
            INVENTORY.replace("sw-picade-2", "sw-picade"),
            INVENTORY.replace("aarch64", "armv7l"),
            INVENTORY.replace("Space-Wars Picade Keys", "Another keyboard"),
            INVENTORY + '\nN: Name="Space-Wars Picade"\nH: Handlers=event5\n',
        ):
            with self.subTest(inventory=inventory), self.assertRaises(ValueError):
                picade.discover(inventory, "sw-picade-2.local")

    def test_press_and_release_packets_end_with_sync(self):
        for value in (0, 1):
            events = list(picade.EVENT.iter_unpack(picade.packet(308, value)))
            self.assertEqual(events, [(0, 0, 1, 308, value), (0, 0, 0, 0, 0)])
            self.assertEqual(len(picade.packet(308, value)), 48)

    def test_shell_bytes_round_trip(self):
        data = picade.packet(308, 1)
        encoded = picade.shell_bytes(data)
        self.assertEqual(bytes(int(part, 8) for part in encoded.split("\\")[1:]), data)

    def test_every_button_is_bounded_and_released_by_remote_trap(self):
        for button in picade.BUTTONS:
            command = picade.press_command("sw-picade-2.local", "event4", button, 1200)
            self.assertTrue(command.startswith("timeout 5s sh -c "))
            self.assertIn("trap release EXIT", command)
            self.assertIn("HUP INT TERM", command)
            self.assertIn("sleep 1.200", command)
            self.assertIn("/sys/class/input/event4/device/name", command)
            self.assertIn(picade.shell_bytes(picade.packet(picade.BUTTONS[button][1], 0)), command)

    def test_power_and_unbounded_holds_are_unavailable(self):
        self.assertNotIn("power", picade.BUTTONS)
        self.assertNotIn(116, [code for _, code in picade.BUTTONS.values()])
        for hold in (0, 49, 2001):
            with self.assertRaises(ValueError):
                picade.press_command("sw-picade-2.local", "event4", "west", hold)
        with self.assertRaises(ValueError):
            picade.press_command("sw-picade-2.local", "../elsewhere", "west", 120)

    def test_screen_guard_prevents_injection(self):
        client = object.__new__(picade.Picade)
        for scenario, screen in (("spacewars", "gameplay"), ("clock", "launcher.main")):
            with patch.object(client, "cli", return_value={
                "active_scenario": scenario, "screen": screen,
            }), patch.object(client, "run") as run:
                with self.assertRaises(RuntimeError):
                    client.press("west", "gameplay")
                run.assert_not_called()

    def test_ssh_retains_named_host_verification_with_ip_fallback(self):
        with patch.object(picade.Picade, "run", return_value=INVENTORY):
            client = picade.Picade("sw-picade-2.local", "192.0.2.1")
        self.assertIn("HostKeyAlias=sw-picade-2.local", client.ssh)
        self.assertIn("spacewars@192.0.2.1", client.ssh)
        self.assertNotIn("StrictHostKeyChecking=no", client.ssh)


if __name__ == "__main__":
    unittest.main()
