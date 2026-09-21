#!/usr/bin/env python3
"""Bounded Picade button injection over SSH; no remote installation or device grab.

This exercises Linux input -> gilrs/libinput -> application routing, not the
physical switch or GPIO wiring. Do not operate the cabinet during verification.
Power is intentionally unavailable. Each press releases on exit/interruption.
"""

import argparse
import ipaddress
import json
import re
import shlex
import struct
import subprocess
import time


PAD = "Space-Wars Picade"
KEYS = "Space-Wars Picade Keys"
BUTTONS = {
    "south": (PAD, 304), "east": (PAD, 305), "west": (PAD, 308),
    "north": (PAD, 307), "left-shoulder": (PAD, 310),
    "right-shoulder": (PAD, 311), "select": (PAD, 314),
    "start": (PAD, 315), "escape": (KEYS, 1), "enter": (KEYS, 28),
}
EVENT = struct.Struct("<qqHHi")  # 64-bit, little-endian Pi input_event.


def discover(inventory, host):
    hostname, architecture, inventory = inventory.split("\n", 2)
    if hostname != host.removesuffix(".local") or architecture != "aarch64":
        raise ValueError(f"Unexpected device: {hostname}, {architecture}")
    devices = {}
    for block in inventory.split("\n\n"):
        name = re.search(r'^N: Name="([^"]+)"$', block, re.M)
        if not name or name[1] not in (PAD, KEYS):
            continue
        handler = re.search(r"^H: Handlers=.*\b(event\d+)\b", block, re.M)
        if not handler or name[1] in devices:
            raise ValueError("Missing or ambiguous Picade input handler")
        devices[name[1]] = handler[1]
    if set(devices) != {PAD, KEYS}:
        raise ValueError("Expected separate Picade gamepad and utility-key devices")
    return devices


def packet(code, value):
    return EVENT.pack(0, 0, 1, code, value) + EVENT.pack(0, 0, 0, 0, 0)


def shell_bytes(data):
    # printf's octal escapes avoid transmitting binary through shell arguments.
    return "".join(f"\\{byte:03o}" for byte in data)


def press_command(host, device, button, hold_ms):
    if not 50 <= hold_ms <= 2000:
        raise ValueError("Hold must be between 50 and 2000 milliseconds")
    if not re.fullmatch(r"event\d+", device):
        raise ValueError("Invalid input handler")
    name, code = BUTTONS[button]
    # Revalidate device identity before opening, not only during discovery.
    # The timeout and release trap execute on the Pi, independent of SSH latency.
    script = f"""set -eu
test "$(hostname)" = {shlex.quote(host.removesuffix('.local'))}
test "$(uname -m)" = aarch64
test "$(cat /sys/class/input/{device}/device/name)" = {shlex.quote(name)}
exec 3>/dev/input/{device}
release() {{ printf '{shell_bytes(packet(code, 0))}' >&3; }}
trap release EXIT
trap 'exit 130' HUP INT TERM
printf '{shell_bytes(packet(code, 1))}' >&3
sleep {hold_ms / 1000:.3f}
"""
    return "timeout 5s sh -c " + shlex.quote(script)


class Picade:
    def __init__(self, host, address=None):
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9.-]*", host):
            raise ValueError("Invalid hostname")
        if address:
            ipaddress.ip_address(address)
        self.host = host
        self.ssh = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=5",
                    "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=2",
                    "-o", f"HostKeyAlias={host}", f"spacewars@{address or host}"]
        self.devices = discover(
            self.run("hostname; uname -m; cat /proc/bus/input/devices"), host)

    def run(self, command):
        return subprocess.check_output(self.ssh + [command], text=True, timeout=15)

    def cli(self, *args):
        return json.loads(self.run(shlex.join(["spacewars-cli", *args, "--json"])))

    def guard(self, screen):
        ui = self.cli("ui", "state")
        if ui["active_scenario"] != "clock" or ui["screen"] != screen:
            raise RuntimeError(f"Expected Clock on {screen}, got "
                               f"{ui['active_scenario']} on {ui['screen']}")
        return ui

    def press(self, button, screen, hold_ms=120):
        # This is a preflight, not an atomic lock against another user's input.
        self.guard(screen)
        device = self.devices[BUTTONS[button][0]]
        self.run(press_command(self.host, device, button, hold_ms))
        print(f"PRESS {button} ({BUTTONS[button][1]}), {hold_ms} ms; released", flush=True)

    def wait_screen(self, screen):
        return self.cli("ui", "wait", "--screen", screen, "--scenario", "clock",
                        "--timeout", "5s")


def check(condition, message):
    if not condition:
        raise RuntimeError(message)


def verify_clock(picade):
    """Verify the running session without changing settings or restarting it."""
    initial_ui = picade.guard("gameplay")
    initial = picade.cli("clock", "state")
    check(any(event["enabled"] for event in initial["events"]),
          "Enable at least one Clock event before this test")
    settings_hash = picade.run("sha256sum /data/spacewars/config/settings.toml")

    def clock():
        state = picade.cli("clock", "state")
        check(state["scenario_revision"] == initial["scenario_revision"],
              "Clock instance changed during verification")
        check(state["settings"] == initial["settings"],
              "Clock preferences changed during verification")
        return state

    try:
        before = clock()
        picade.press("west", "gameplay", hold_ms=1200)
        after = clock()
        check(after["event_id"] == before["event_id"] + 1,
              "Holding West must advance exactly once (ensure no other input)")
        print(f"PASS held Next Event: {before['event_id']} -> {after['event_id']}", flush=True)
        # Allow the gamepad poll to observe neutral before a new press.
        time.sleep(0.1)
        picade.press("west", "gameplay")
        again = clock()
        check(again["event_id"] == after["event_id"] + 1,
              "Released/repressed West must advance once more")
        print(f"PASS fresh Next Event: {after['event_id']} -> {again['event_id']}", flush=True)

        picade.press("escape", "gameplay")
        menu = picade.wait_screen("pause.main")
        check(menu["selected_control"] == "pause.resume", "Resume must initially be selected")
        paused = clock()
        check(paused["paused"], "Escape did not pause Clock")
        picade.press("west", "pause.main", hold_ms=1200)
        still = clock()
        check(still["event_id"] == paused["event_id"]
              and still["simulation_tick"] == paused["simulation_tick"],
              "Next Event or simulation advanced while in the pause menu")
        check(picade.guard("pause.main")["selected_control"] == "pause.resume",
              "West changed the pause selection")
        print("PASS Next Event ignored while paused; simulation remains frozen", flush=True)
        picade.press("enter", "pause.main")
        picade.wait_screen("gameplay")
        check(not clock()["paused"], "Enter did not resume Clock")
        print("PASS Escape/Enter pause/resume", flush=True)

        time.sleep(0.1)
        picade.press("escape", "gameplay")
        picade.wait_screen("pause.main")
        # BTN_SOUTH is absent from the physical capture. Test its software path
        # without claiming anything about that physical switch's wiring.
        picade.press("south", "pause.main")
        picade.wait_screen("gameplay")
        final = clock()
        check(not final["paused"], "Injected South/A did not confirm Resume")
        check(picade.run("sha256sum /data/spacewars/config/settings.toml") == settings_hash,
              "Saved preferences changed during verification")
        check(picade.guard("gameplay")["scenario_revision"] == initial_ui["scenario_revision"],
              "Session restarted during verification")
        print("PASS injected South/A confirms Resume; physical switch remains unverified", flush=True)
        print("PASS Clock running; same instance and unchanged saved preferences", flush=True)
    except BaseException:
        # Do not try unguarded recovery presses: a failed test can mean the UI
        # has changed unexpectedly. All injected buttons already auto-release.
        print("STOP: verification failed/interrupted; no further inputs sent. "
              "The menu may be paused.", flush=True)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", required=True)
    parser.add_argument("--address", help="IP fallback; retains the named SSH host identity")
    subparsers = parser.add_subparsers(dest="command", required=True)
    press = subparsers.add_parser("press", help="Inject one bounded Clock button press")
    press.add_argument("button", choices=BUTTONS)
    press.add_argument("--expect-screen", required=True, choices=("gameplay", "pause.main"))
    press.add_argument("--hold-ms", type=int, default=120, choices=range(50, 2001),
                       metavar="50..2000")
    subparsers.add_parser("verify-clock", help="Verify live Clock buttons; leave it running")
    args = parser.parse_args()
    picade = Picade(args.host, args.address)
    if args.command == "press":
        picade.press(args.button, args.expect_screen, args.hold_ms)
    else:
        verify_clock(picade)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        raise SystemExit("Stopped; any in-flight press releases on the Pi within two seconds.")
    except (ValueError, RuntimeError, subprocess.SubprocessError) as error:
        raise SystemExit(str(error))
