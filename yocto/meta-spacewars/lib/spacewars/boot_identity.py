"""Identify the boot assets required by a rootfs-only A/B update.

Match bootimg-partition's IMAGE_BOOT_FILES layout. Device selection and the
identity itself are excluded; profile definitions and all kernel/firmware
assets are included. This is a compatibility guard, not a signature scheme.
"""

import hashlib
import json
from pathlib import Path
import re


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


# v1 remains for previously built/manual-update images. Recovery images use v2:
# root selection is constrained separately, while its template stays immutable.
MUTABLE_BOOT_FILES = {
    "cmdline.txt", "spacewars-trial.txt", "spacewars-device.txt",
    "spacewars-boot-id", "spacewars-boot-manifest.json",
}
GENERATED_BOOT_FILES = {
    "spacewars-cmdline.txt", "spacewars-trial.txt", "tryboot.txt",
    "spacewars-boot-id", "spacewars-boot-manifest.json",
}
UNPREPARED_TRIAL = "spacewars.trial=unprepared\n"


def boot_paths(deploy_dir, entries, exclude):
    deploy_dir = Path(deploy_dir)
    result = {}
    for entry in entries.split():
        parts = entry.split(";")
        if len(parts) > 2 or not all(parts):
            raise ValueError(f"Invalid boot file entry: {entry}")
        source, destination = parts[0], parts[-1]
        if "*" not in source and destination in exclude:
            continue
        matches = sorted(deploy_dir.glob(source)) if "*" in source else [deploy_dir / source]
        if not matches:
            raise ValueError(f"Boot file pattern matched nothing: {source}")
        for path in matches:
            target = (path.name if destination == source else str(Path(destination) / path.name)) if "*" in source else destination
            if target in exclude:
                continue
            if (not re.fullmatch(r"[A-Za-z0-9_./-]{1,160}", target)
                    or any(part in ("", ".", "..") for part in target.split("/"))
                    or not path.is_file()):
                raise ValueError(f"Missing or invalid boot asset: {target}")
            if target in result and result[target].read_bytes() != path.read_bytes():
                raise ValueError(f"Conflicting boot asset: {target}")
            result[target] = path
    if len({name.lower() for name in result}) != len(result):
        raise ValueError("Boot assets collide on the FAT filesystem")
    return result


def recovery_boot_identity(manifest):
    data = "spacewars-boot-v2\n" + "".join(
        f"{name}\0{blob['bytes']}\0{blob['sha256']}\n"
        for name, blob in sorted(manifest["files"].items())
    )
    return hashlib.sha256(data.encode("ascii")).hexdigest()


def prepare_recovery_boot(deploy_dir, entries):
    deploy_dir = Path(deploy_dir)
    original = boot_paths(deploy_dir, entries, GENERATED_BOOT_FILES)
    config = original["config.txt"].read_text()
    active = [line.strip() for line in config.splitlines() if line.strip() and not line.lstrip().startswith("#")]
    if (active.count("initramfs spacewars-recovery.cpio.gz followkernel") != 1
            or sum(line.startswith("initramfs ") for line in active) != 1
            or any(re.match(r"^(cmdline|os_prefix|tryboot_a_b|ramfsfile|ramfsaddr)\s*=", line) for line in active)):
        raise ValueError("Unsupported base boot configuration for rootfs trials")
    kernel_id = original["spacewars-kernel-build-id"].read_text().strip()
    if not re.fullmatch(r"[0-9a-f]{40}", kernel_id):
        raise ValueError("Missing exact kernel build identity")
    arguments = original["cmdline.txt"].read_text().split()
    if (not arguments or len(" ".join(arguments)) > 4096
            or any(not re.fullmatch(r"[A-Za-z0-9_=.,:/-]+", arg) for arg in arguments)
            or any(sum(arg.startswith(prefix) for arg in arguments) != 1 for prefix in ("root=", "rootfstype=", "roottimeout="))
            or any(arg.split("=", 1)[0] in ("init", "rdinit", "rootdelay", "init_fatal_sh", "spacewars.trial") for arg in arguments)
            or any(arguments.count(arg) != 1 for arg in ("rootfstype=ext4", "rootwait", "roottimeout=30"))):
        raise ValueError("Unsupported base kernel command line for rootfs trials")
    template = " ".join("root=@SPACEWARS_ROOT@" if arg.startswith("root=") else arg for arg in arguments) + "\n"
    (deploy_dir / "spacewars-cmdline.txt").write_text(template)
    (deploy_dir / "spacewars-trial.txt").write_text(UNPREPARED_TRIAL)
    # Copy the complete configuration, since some firmware settings cannot be
    # supplied through include. Only the candidate root command line differs.
    (deploy_dir / "tryboot.txt").write_text(config.rstrip() + "\n[all]\ncmdline=spacewars-trial.txt\n")
    files = {}
    for name, path in boot_paths(deploy_dir, entries, MUTABLE_BOOT_FILES).items():
        content = path.read_bytes()
        files[name] = {"bytes": len(content), "sha256": hashlib.sha256(content).hexdigest()}
    manifest = {"schema": 2, "files": files}
    (deploy_dir / "spacewars-boot-manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
    return manifest, recovery_boot_identity(manifest)
