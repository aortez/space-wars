#!/usr/bin/env python3
"""Read only Picade input devices over SSH and label a guided physical-button capture.

No grab, settings changes, or software installation. The app also sees these
presses. Press/release each requested button once; do not press Power.
"""

import argparse
import re
import selectors
import shlex
import struct
import subprocess
import time


POSITIONS = (
    "top left", "top middle", "top right",
    "bottom left", "bottom middle", "bottom right",
    "left utility", "right utility",
)
CODES = {
    304: "Button 1 / South / A", 305: "Button 2 / East / B",
    308: "Button 3 / West", 307: "Button 4 / North",
    310: "Button 5 / left shoulder", 311: "Button 6 / right shoulder",
    314: "Coin / Select", 315: "Start", 28: "Enter", 1: "Escape",
}
EVENT = struct.Struct("<qqHHi")  # Linux input_event on the 64-bit Pi image.


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="sw-picade-2.local")
    parser.add_argument("--address", help="IP fallback; retain --host for SSH identity")
    parser.add_argument("--seconds", type=int, default=120)
    parser.add_argument("--raw", action="store_true", help="print press/release events without assigning positions")
    args = parser.parse_args()
    if not 1 <= args.seconds <= 600:
        parser.error("--seconds must be between 1 and 600")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9.-]*", args.host):
        parser.error("invalid hostname")
    if args.address and not re.fullmatch(r"[A-Fa-f0-9:.]+", args.address):
        parser.error("--address must be an IP address")
    ssh = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=5",
           "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=2",
           "-o", f"HostKeyAlias={args.host}", f"spacewars@{args.address or args.host}"]
    inventory = subprocess.check_output(
        ssh + ["hostname; uname -m; cat /proc/bus/input/devices"], text=True)
    hostname, architecture, inventory = inventory.split("\n", 2)
    if hostname != args.host.removesuffix(".local") or architecture != "aarch64":
        raise SystemExit(f"Unexpected device: {hostname}, {architecture}; refusing capture")
    devices = []
    for block in inventory.split("\n\n"):
        if re.search(r'^N: Name="Space-Wars Picade(?: Keys)?"$', block, re.M):
            handler = re.search(r"^H: Handlers=.*\b(event\d+)\b", block, re.M)
            if handler:
                devices.append("/dev/input/" + handler[1])
    if len(devices) != 2:
        raise SystemExit("Expected separate Picade gamepad and utility-key devices")

    processes = []
    selector = selectors.DefaultSelector()
    rows, seen = [], set()
    try:
        for device in devices:
            # Acknowledge only after opening the device so the first press cannot
            # be lost between printing the instructions and attaching readers.
            command = (f"set -e; exec 3<{shlex.quote(device)}; printf 'READY\\n'; "
                       f"exec timeout {args.seconds}s dd bs=24 status=none <&3")
            process = subprocess.Popen(ssh + [command], stdout=subprocess.PIPE, bufsize=0)
            processes.append(process)
            ready = bytearray()
            while len(ready) < 6:
                chunk = process.stdout.read(6 - len(ready))
                if not chunk:
                    raise SystemExit(f"Could not open {device}")
                ready.extend(chunk)
            if ready != b"READY\n":
                raise SystemExit(f"Unexpected reader response for {device}")
            selector.register(process.stdout, selectors.EVENT_READ, bytearray())
        print(f"READY: Capturing {hostname} for up to {args.seconds}s. App still receives input.", flush=True)
        if not args.raw:
            print("Press and fully release ONCE each, about one second apart:", flush=True)
            print("  " + " -> ".join(POSITIONS), flush=True)
        print("Do not press Power. Ctrl-C stops capture.\n", flush=True)
        deadline = time.monotonic() + args.seconds
        while (args.raw or len(rows) < len(POSITIONS)) and time.monotonic() < deadline and selector.get_map():
            for key, _ in selector.select(timeout=min(1, max(0, deadline - time.monotonic()))):
                chunk = key.fileobj.read(4096)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                buffer = key.data
                buffer.extend(chunk)
                while len(buffer) >= EVENT.size:
                    _, _, kind, code, value = EVENT.unpack(buffer[:EVENT.size])
                    del buffer[:EVENT.size]
                    if args.raw and kind == 1:
                        print(f"code={code:3} value={value} {CODES.get(code, 'unknown key')}", flush=True)
                        continue
                    if kind != 1 or value != 1 or code not in CODES:
                        continue
                    if code in seen:
                        print(f"Repeated {CODES[code]}; not advancing position", flush=True)
                        continue
                    if len(rows) == len(POSITIONS):
                        break
                    if (len(rows) < 6) != (code >= 304):
                        raise SystemExit("Unexpected utility/front button: capture order is incomplete. Please repeat.")
                    seen.add(code)
                    row = (POSITIONS[len(rows)], code, CODES[code])
                    rows.append(row)
                    print(f"{row[0]:14} -> {row[1]:3}  {row[2]}", flush=True)
    finally:
        selector.close()
        for process in processes:
            process.terminate()
        for process in processes:
            process.wait(timeout=5)
    if args.raw:
        return
    if len(rows) != len(POSITIONS):
        raise SystemExit(f"Incomplete: {len(rows)}/{len(POSITIONS)} buttons. Repeat capture when ready.")
    print("\nConfirm the order was followed before recording this as a verified mapping.")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        raise SystemExit("Capture stopped.")
