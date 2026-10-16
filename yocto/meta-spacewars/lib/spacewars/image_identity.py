"""Payload-contained identity for the final, packaged root filesystem."""

import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess


IDENTITY_PATH = "etc/spacewars/image.json"
SOURCE_PATH = "usr/share/spacewars/source.json"
MAX_EXECUTABLE_BYTES = 256 * 1024 * 1024


def source_identity(srcroot):
    root = Path(srcroot).resolve()

    def git(*args):
        return subprocess.check_output(
            ["git", "-C", str(root), *args], text=True, timeout=30,
            env={**os.environ, "GIT_OPTIONAL_LOCKS": "0"},
        ).strip()

    if not (root / ".git").exists() or Path(git("rev-parse", "--show-toplevel")).resolve() != root:
        raise ValueError("image source must be the explicitly selected Git checkout")
    commit = git("rev-parse", "HEAD")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("image source requires a complete Git SHA-1")
    dirty = bool(git("status", "--porcelain=v1", "--untracked-files=all"))
    return {"schema": 1, "commit": commit, "dirty": dirty,
            "revision": commit[:12] + ("-dirty" if dirty else "")}


def validate_source(value):
    if (not isinstance(value, dict) or set(value) != {"schema", "commit", "dirty", "revision"}
            or type(value["schema"]) is not int or value["schema"] != 1
            or not isinstance(value["commit"], str)
            or not re.fullmatch(r"[0-9a-f]{40}", value["commit"])
            or type(value["dirty"]) is not bool
            or value["revision"] != value["commit"][:12] + ("-dirty" if value["dirty"] else "")):
        raise ValueError("invalid packaged source identity")


def file_blob(path):
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode):
        raise ValueError(f"image identity requires a regular file: {path}")
    digest = hashlib.sha256()
    size = 0
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            size += len(block)
            digest.update(block)
        after = os.fstat(source.fileno())
    fields = ("st_dev", "st_ino", "st_mode", "st_uid", "st_gid", "st_size", "st_mtime_ns", "st_ctime_ns")
    if size != before.st_size or any(getattr(before, f) != getattr(after, f) for f in fields):
        raise ValueError(f"image input changed during hashing: {path}")
    return {"bytes": size, "sha256": digest.hexdigest()}


def rootfs_tree_identity(root):
    """Hash contents, names and install metadata; omit only the identity itself.

    Timestamps and allocated inode numbers do not identify a logical image.
    Hardlink relationships do, so bind them by the first sorted path instead.
    Symlinks are hashed without following them, including absolute /data links.
    """
    root = Path(root)
    digest = hashlib.sha256(b"spacewars-rootfs-tree-v1\0")
    hardlinks = {}

    def visit(path, relative):
        if relative == IDENTITY_PATH:
            return
        metadata = path.lstat()
        record = {"path": relative, "mode": metadata.st_mode,
                  "uid": metadata.st_uid, "gid": metadata.st_gid}
        # File capabilities and ACLs can change executable behavior without
        # changing its bytes or basic mode. Bind those install attributes too.
        attributes = {
            name: hashlib.sha256(os.getxattr(path, name, follow_symlinks=False)).hexdigest()
            for name in sorted(os.listxattr(path, follow_symlinks=False))
        }
        if attributes:
            record["xattrs"] = attributes
        if stat.S_ISREG(metadata.st_mode):
            record["blob"] = file_blob(path)
            key = (metadata.st_dev, metadata.st_ino)
            if metadata.st_nlink > 1:
                if key in hardlinks:
                    record["hardlink"] = hardlinks[key]
                else:
                    hardlinks[key] = relative
        elif stat.S_ISLNK(metadata.st_mode):
            record["target"] = os.readlink(path)
        elif stat.S_ISCHR(metadata.st_mode) or stat.S_ISBLK(metadata.st_mode):
            record["device"] = [os.major(metadata.st_rdev), os.minor(metadata.st_rdev)]
        elif not stat.S_ISDIR(metadata.st_mode) and not stat.S_ISFIFO(metadata.st_mode):
            raise ValueError(f"unsupported object in packaged image: {relative}")
        digest.update(json.dumps(record, sort_keys=True, separators=(",", ":"),
                                 ensure_ascii=True).encode("ascii") + b"\n")
        if stat.S_ISDIR(metadata.st_mode):
            for child in sorted(path.iterdir()):
                visit(child, str(child.relative_to(root)))

    visit(root, ".")
    return digest.hexdigest()


def create_image_identity(root, machine, architecture):
    root = Path(root)
    for value in (machine, architecture):
        if not re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", value):
            raise ValueError("invalid image compatibility identifier")
    source = json.loads((root / SOURCE_PATH).read_text())
    validate_source(source)
    boot = (root / "etc/spacewars/boot-id").read_text().strip()
    if not re.fullmatch(r"[0-9a-f]{64}", boot):
        raise ValueError("image has no valid boot identity")
    binaries = {}
    for field, executable in (("client", "engine-client"), ("os_manager", "engine-os-manager")):
        path = root / "usr/bin" / executable
        metadata = path.lstat()
        if not metadata.st_mode & 0o111 or metadata.st_mode & 0o022:
            raise ValueError(f"unsafe image executable: {executable}")
        binaries[field] = file_blob(path)
        if not 0 < binaries[field]["bytes"] <= MAX_EXECUTABLE_BYTES:
            raise ValueError(f"invalid image executable length: {executable}")
    tree = rootfs_tree_identity(root)
    return {
        "schema": 1, "build_id": "rootfs-" + tree, "rootfs_tree": tree,
        "source": source, "machine": machine, "architecture": architecture,
        "layout": "spacewars-ab-v1", "boot_identity": boot,
        "updater_protocol": 1, "recovery_protocol": 2,
        "data": {"reads": {"min": 1, "max": 1}, "writes": 1}, "binaries": binaries,
    }


def write_image_identity(root, machine, architecture, source_date_epoch):
    identity = create_image_identity(root, machine, architecture)
    path = Path(root) / IDENTITY_PATH
    # IMAGE_ROOTFS is build-owned. Replace a previous generated file without
    # following a link, and never include the output in its own fingerprint.
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise ValueError("invalid previous generated image identity")
    if path.exists():
        path.unlink()
    path.write_text(json.dumps(identity, indent=2, sort_keys=True) + "\n")
    path.chmod(0o644)
    os.utime(path, (source_date_epoch, source_date_epoch))
    return identity
