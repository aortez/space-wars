"""Run the real recovery command in an unprivileged, disposable Linux root.

Requires `cargo build -p engine-os-manager`, unshare, mount and ldd. This is an
explicit functional test because CI hosts may disable unprivileged namespaces.
No real boot/data paths or device nodes are exposed inside the private root.
"""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


def inside(root, parent_namespace):
    assert os.readlink("/proc/self/ns/mnt") != parent_namespace
    subprocess.run(["mount", "--make-rprivate", "/"], check=True)
    subprocess.run(["mount", "-t", "proc", "proc", str(root / "proc")], check=True)
    os.chroot(root)
    os.chdir("/")
    assert os.geteuid() == 0
    assert not Path("/dev").exists()
    command = ["/usr/bin/engine-os-manager", "recover"]
    report = Path("/run/spacewars-updater/status.json")
    journal = Path("/data/spacewars-system/recovery")

    first = subprocess.run(command, capture_output=True, text=True)
    unprovisioned = json.loads(report.read_text())
    assert first.returncode == 0, (first.returncode, first.stderr)
    assert unprovisioned["progress"]["stage"] == "unprovisioned", unprovisioned
    assert not journal.exists()
    assert not Path("/run/spacewars-install").exists()

    # An existing directory must not be interpreted as permission to bootstrap.
    # This root has no installed image identity, so inspection must fail closed.
    journal.mkdir(mode=0o700)
    state = journal / "state.json"
    state.write_bytes(b"corrupt fixture")
    second = subprocess.run(command, capture_output=True, text=True)
    blocked = json.loads(report.read_text())
    assert second.returncode == 1, (second.returncode, second.stderr)
    assert blocked["progress"]["stage"] == "blocked", blocked
    assert state.read_bytes() == b"corrupt fixture"
    print(json.dumps({
        "uid": os.geteuid(), "pid": os.getpid(),
        "unprovisioned": unprovisioned, "existing_state": blocked,
        "existing_state_preserved": True, "production_device_nodes_present": False,
    }, indent=2))


def main():
    repo = Path(__file__).resolve().parents[3]
    root = Path(tempfile.mkdtemp(prefix="ota-recover-command-", dir=repo / "target"))
    for name, mode in [
        ("proc", 0o755), ("run", 0o755), ("run/spacewars-updater", 0o750),
        ("data", 0o755), ("data/spacewars-system", 0o700), ("usr/bin", 0o755),
    ]:
        path = root / name
        path.mkdir(parents=True, exist_ok=True)
        path.chmod(mode)
    binary = repo / "target/debug/engine-os-manager"
    shutil.copy2(binary, root / "usr/bin/engine-os-manager")
    libraries = subprocess.check_output(["ldd", str(binary)], text=True)
    for name in re.findall(r"(/[^\s]+)", libraries):
        path = root / name.lstrip("/")
        path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(name, path)
    result = subprocess.run([
        "unshare", "--user", "--map-root-user", "--mount", "--pid", "--fork", "--",
        sys.executable, str(Path(__file__).resolve()), "--inside", str(root),
        os.readlink("/proc/self/ns/mnt"),
    ], capture_output=True, text=True)
    print(json.dumps({
        "fixture_root": str(root), "exit_code": result.returncode,
        "stdout": result.stdout, "stderr": result.stderr,
    }, indent=2))
    return result.returncode


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--inside":
        inside(Path(sys.argv[2]), sys.argv[3])
    else:
        raise SystemExit(main())
