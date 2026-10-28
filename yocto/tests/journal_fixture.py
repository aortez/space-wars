"""Real journald boot fixture. Invoked only inside journal_test.py's namespace."""

import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import time


def journalctl(*args):
    return subprocess.run(
        ["journalctl", "--no-pager", *args], check=True, capture_output=True, text=True, timeout=30,
    ).stdout


def boot(marker, crash, flood):
    store = Path("/data/spacewars-journal")

    def disk_state():
        return {p.name: (p.stat().st_size, p.stat().st_mtime_ns) for p in store.glob("*")}

    before = disk_state()
    log = open("/daemon.log", "w+")
    daemon = subprocess.Popen(["/usr/lib/systemd/systemd-journald"], stdout=log, stderr=log)
    try:
        deadline = time.monotonic() + 10
        while not Path("/run/systemd/journal/socket").exists():
            if daemon.poll() is not None or time.monotonic() >= deadline:
                log.seek(0)
                raise RuntimeError(log.read())
            time.sleep(0.01)
        with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as sender:
            sender.settimeout(10)
            sender.connect("/run/systemd/journal/socket")

            def send(message):
                sender.send(b"PRIORITY=5\nSYSLOG_IDENTIFIER=journal-fixture\nMESSAGE=" + message + b"\n")

            send((marker + "-early").encode())
            journalctl("--sync")
            assert list(Path("/run/log/journal").rglob("*.journal")), "No early RAM journal"
            assert disk_state() == before, "Disk changed before flush"
            journalctl("--flush")
            send(marker.encode())
            peak = 0
            if flood:
                # Incompressible, unique messages exceed the real production cap.
                for batch in range(192):
                    for index in range(64):
                        send(f"flood-{batch}-{index}:".encode() + os.urandom(8192).hex().encode())
                    journalctl("--sync")
                    used = sum(p.stat().st_blocks * 512 for p in store.iterdir())
                    peak = max(peak, used)
            send((marker + "-last").encode())
            journalctl("--sync")
            # Kill only after sync so the crash test has a known durable marker.
            if crash:
                daemon.kill()
                daemon.wait(timeout=10)
            else:
                journalctl("--relinquish-var")
                send((marker + "-shutdown-ram").encode())
                journalctl("--sync")
                runtime = journalctl("--directory=/run/log/journal", "--output=cat")
                assert marker + "-shutdown-ram" in runtime, runtime
            print(json.dumps({"peak_bytes": peak}))
    finally:
        if daemon.poll() is None:
            daemon.send_signal(signal.SIGTERM)
            daemon.wait(timeout=10)
        log.seek(0)
        diagnostic = log.read()
        if sys.exc_info()[0] is not None:
            print(diagnostic, file=sys.stderr)
        if any(word in diagnostic for word in ("Unknown key", "Failed to parse")):
            raise RuntimeError(diagnostic)


if __name__ == "__main__":
    boot(sys.argv[1], sys.argv[2] == "crash", sys.argv[3] == "flood")
