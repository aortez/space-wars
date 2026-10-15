"""Run the real recovery command in an unprivileged, disposable Linux root.

Requires `cargo build -p engine-os-manager`, unshare, mount and ldd. This is an
explicit functional test because CI hosts may disable unprivileged namespaces.
No real boot/data paths or device nodes are exposed inside the private root.
"""

import json
import fcntl
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

    fast = ["/usr/sbin/spacewars-fast-update", "/tmp/spacewars-fast.A123456789", *(["a" * 64] * 3)]
    renderer = ["/usr/sbin/spacewars-renderer", "gpu"]
    check = subprocess.run([fast[0], "--check"], capture_output=True, text=True)
    assert check.returncode == 0 and check.stdout.strip() == "check", check
    assert not Path("/run/spacewars-install").exists()
    for helper in [fast, renderer]:
        ran = subprocess.run(helper, capture_output=True, text=True)
        assert ran.returncode == 0 and ran.stdout.strip() == "inherited-lock-held", ran
        with open("/run/spacewars-install/lock", "r+") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            blocked_helper = subprocess.run(helper, capture_output=True, text=True)
            assert blocked_helper.returncode == 1, blocked_helper
            assert "already in progress" in blocked_helper.stderr, blocked_helper.stderr

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
    state.chmod(0o600)
    for helper in [fast, renderer]:
        rejected = subprocess.run(helper, capture_output=True, text=True)
        assert rejected.returncode == 1 and not rejected.stdout, rejected
        assert state.read_bytes() == b"corrupt fixture"
    print(json.dumps({
        "uid": os.geteuid(), "pid": os.getpid(),
        "unprovisioned": unprovisioned, "existing_state": blocked,
        "existing_state_preserved": True, "production_device_nodes_present": False,
        "maintenance_wrappers_hold_shared_lock": True,
        "busy_lock_and_corrupt_journal_reject_both_helpers": True,
    }, indent=2))


def main():
    repo = Path(__file__).resolve().parents[3]
    root = Path(tempfile.mkdtemp(prefix="ota-recover-command-", dir=repo / "target"))
    for name, mode in [
        ("proc", 0o755), ("run", 0o755), ("run/spacewars-updater", 0o750),
        ("data", 0o755), ("data/spacewars-system", 0o700), ("usr/bin", 0o755),
        ("usr/sbin", 0o755), ("usr/libexec/spacewars", 0o700), ("bin", 0o755),
    ]:
        path = root / name
        path.mkdir(parents=True, exist_ok=True)
        path.chmod(mode)
    binary = repo / "target/debug/engine-os-manager"
    for source, destination in [(binary, "usr/bin/engine-os-manager"), ("/bin/sh", "bin/sh"), ("/usr/bin/flock", "usr/bin/flock"), ("/usr/bin/true", "usr/bin/true")]:
        shutil.copy2(source, root / destination)
        libraries = subprocess.check_output(["ldd", str(source)], text=True)
        for name in re.findall(r"(/[^\s]+)", libraries):
            path = root / name.lstrip("/")
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(name, path)
    files = repo / "yocto/meta-spacewars/recipes-spacewars/spacewars/files"
    for worker in ["fast-update", "renderer"]:
        shutil.copy2(files / f"spacewars-{worker}-wrapper.sh", root / f"usr/sbin/spacewars-{worker}")
        (root / f"usr/sbin/spacewars-{worker}").chmod(0o755)
        helper = root / f"usr/libexec/spacewars/{worker}"
        helper.write_text("#!/bin/sh\nset -eu\n"
                          "if [ \"$1\" = --check ]; then echo check; exit 0; fi\n"
                          "if /usr/bin/flock -n /run/spacewars-install/lock /usr/bin/true; then exit 87; fi\n"
                          "echo inherited-lock-held\n")
        helper.chmod(0o700)
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
