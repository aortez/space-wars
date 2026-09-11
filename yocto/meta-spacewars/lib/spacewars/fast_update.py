"""Conservative runtime compatibility identity for application-only updates."""

import hashlib
from pathlib import Path


def compatibility_id(sysroot, assets, variables):
    digest = hashlib.sha256(b"spacewars-fast-update-v1\n")
    for key, value in sorted(variables.items()):
        digest.update(f"{key}={value}\n".encode())
    root = Path(sysroot)
    libraries = sorted(
        path for directory in (root / "lib", root / "usr/lib")
        if directory.exists()
        for path in directory.rglob("*")
        if ".so" in path.name and (path.is_file() or path.is_symlink())
    )
    if not libraries:
        raise ValueError("No runtime libraries found for fast-update compatibility")
    for path in libraries:
        digest.update(f"library:{path.relative_to(root)}\n".encode())
        if path.is_symlink():
            digest.update(f"link:{path.readlink()}\n".encode())
        else:
            digest.update(hashlib.sha256(path.read_bytes()).digest())
    for path in sorted(map(Path, assets)):
        digest.update(f"asset:{path.name}\n".encode())
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()
