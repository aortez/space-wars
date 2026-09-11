"""Identify the boot assets required by a rootfs-only A/B update.

Match bootimg-partition's IMAGE_BOOT_FILES layout. Device selection and the
identity itself are excluded; profile definitions and all kernel/firmware
assets are included. This is a compatibility guard, not a signature scheme.
"""

import hashlib
from pathlib import Path


def boot_identity(deploy_dir, entries):
    deploy_dir = Path(deploy_dir)
    files = {}
    for entry in entries.split():
        parts = entry.split(";")
        if len(parts) > 2 or not all(parts):
            raise ValueError(f"Invalid boot file entry: {entry}")
        source = parts[0]
        destination = parts[-1]
        if source in ("spacewars-boot-id", "spacewars-device.txt"):
            continue
        matches = sorted(deploy_dir.glob(source)) if "*" in source else [deploy_dir / source]
        if not matches:
            raise ValueError(f"Boot file pattern matched nothing: {source}")
        for path in matches:
            if not path.is_file():
                raise ValueError(f"Missing boot file: {path}")
            if "*" in source:
                target = path.name if destination == source else str(Path(destination) / path.name)
            else:
                target = destination
            if Path(target).is_absolute() or ".." in Path(target).parts:
                raise ValueError(f"Invalid boot destination: {target}")
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            if target in files and files[target] != digest:
                raise ValueError(f"Conflicting boot destination: {target}")
            files[target] = digest
    if not files:
        raise ValueError("Boot file list is empty")
    manifest = "spacewars-boot-v1\n" + "".join(
        f"{target}\0{digest}\n" for target, digest in sorted(files.items())
    )
    return hashlib.sha256(manifest.encode()).hexdigest()
