"""Restore only /data from a selected bootstrap archive into an offline tree.

The caller selects and mounts the offline filesystem. Root configuration and
disk-bound OTA journals are never imported. Numeric ownership requires root.
"""

from collections import Counter
from decimal import Decimal
import hashlib
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess
import tarfile


FORBIDDEN = ("data/spacewars-bootstrap", "data/spacewars-system/recovery")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def under(name, parent):
    return name == parent or name.startswith(parent + "/")


def nanoseconds(member):
    return int(Decimal(member.pax_headers.get("mtime", str(member.mtime))) * 1_000_000_000)


def attributes(member):
    prefix = "SCHILY.xattr."
    return {name[len(prefix):]: value.encode("utf-8", "surrogateescape")
            for name, value in member.pax_headers.items() if name.startswith(prefix)}


def data_members(archive):
    members = archive.getmembers()
    require(len(members) <= 100_000, "Excessive archive entries")
    names = set()
    # Tar normalizes some names before selection. Check the entire inventory so
    # an absolute or indirect spelling cannot evade validation then match data.
    for member in members:
        path = PurePosixPath(member.name)
        require(str(path) == member.name and not path.is_absolute()
                and ".." not in path.parts, "Unsafe archive path")
        require(member.name not in names, "Duplicate archive entry")
        names.add(member.name)
    selected = [member for member in members if under(member.name, "data")]
    require(0 < len(selected) <= 100_000, "Missing or excessive data entries")
    entries = {}
    for member in selected:
        require(not any(under(member.name, prefix) for prefix in FORBIDDEN),
                "Disk-bound recovery state cannot be restored onto a replacement disk")
        require(member.isdir() or member.isreg() or member.issym() or member.islnk(),
                "Unsupported special file in data archive")
        require(member.uid >= 0 and member.gid >= 0, "Invalid numeric owner")
        require(not member.sparse, "Sparse archives are not supported")
        require(not any(name.startswith("GNU.sparse") for name in member.pax_headers),
                "Sparse archives are not supported")
        if member.islnk():
            target = entries.get(member.linkname)
            require(target is not None and (target.isreg() or target.islnk()),
                    "Hard link must refer to an earlier data file")
        entries[member.name] = member
    require("data" in entries and entries["data"].isdir(), "Missing data root directory")
    for member in selected:
        for parent in PurePosixPath(member.name).parents:
            if str(parent) == ".":
                continue
            require(str(parent) in entries and entries[str(parent)].isdir(),
                    "Missing directory or link in archive ancestry")
    return selected


def open_verified(path, expected):
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    stream = os.fdopen(descriptor, "rb")
    try:
        observed = os.fstat(descriptor)
        require(stat.S_ISREG(observed.st_mode), "Expected a regular archive file")
        require(observed.st_size == expected["bytes"], "Archive length mismatch")
        require(hashlib.file_digest(stream, "sha256").hexdigest() == expected["sha256"],
                "Archive checksum mismatch")
        stream.seek(0)
        return stream
    except BaseException:
        stream.close()
        raise


def summarize(members, expected):
    types = Counter("directories" if m.isdir() else "links" if m.issym() or m.islnk()
                    else "files" for m in members)
    return {"archive": expected, "data_entries": len(members),
            "data_payload_bytes": sum(m.size for m in members), **types,
            "root_configuration_imported": False, "recovery_journals_imported": False}


def inspect_archive(path, expected):
    with open_verified(path, expected) as stream, tarfile.open(fileobj=stream, mode="r:") as archive:
        return summarize(data_members(archive), expected)


def target_path(destination, member):
    return destination.joinpath(*PurePosixPath(member.name).parts[1:])


def check_destination(destination, members):
    require(destination.is_absolute() and destination.is_dir() and not destination.is_symlink(),
            "Expected an absolute offline directory")
    require(destination.resolve() == destination and destination not in [Path("/"), Path("/data")],
            "Refusing a live or indirect data destination")
    entries = {str(PurePosixPath(m.name).relative_to("data")): m for m in members}
    filesystem = destination.stat().st_dev
    for root, directories, files in os.walk(destination, followlinks=False):
        for name in directories + files:
            path = Path(root) / name
            relative = str(path.relative_to(destination))
            observed = path.lstat()
            require(observed.st_dev == filesystem, "Unexpected mount inside restore destination")
            if relative == "lost+found" and relative not in entries:
                require(stat.S_ISDIR(observed.st_mode) and not any(path.iterdir()),
                        "Existing lost+found is not empty")
                continue
            member = entries.get(relative)
            require(member is not None, "Unexpected existing file in restore destination")
            compatible = (member.isdir() and stat.S_ISDIR(observed.st_mode)
                          or member.issym() and stat.S_ISLNK(observed.st_mode)
                          or (member.isreg() or member.islnk()) and stat.S_ISREG(observed.st_mode))
            require(compatible, "Existing path has a different type from the archive")


def verify_tree(archive, members, destination):
    check_destination(destination, members)
    for member in members:
        path = target_path(destination, member)
        observed = path.lstat()
        require((observed.st_uid, observed.st_gid) == (member.uid, member.gid),
                "Restored numeric ownership differs from archive")
        require(stat.S_IMODE(observed.st_mode) == stat.S_IMODE(member.mode),
                "Restored permissions differ from archive")
        require(observed.st_mtime_ns == nanoseconds(member),
                "Restored timestamp differs from archive")
        actual_attributes = {name: os.getxattr(path, name, follow_symlinks=False)
                             for name in os.listxattr(path, follow_symlinks=False)}
        require(actual_attributes == attributes(member), "Restored attributes differ from archive")
        if member.issym():
            require(stat.S_ISLNK(observed.st_mode) and os.readlink(path) == member.linkname,
                    "Restored symbolic link differs from archive")
        elif member.islnk():
            other = destination.joinpath(*PurePosixPath(member.linkname).parts[1:]).lstat()
            require((observed.st_dev, observed.st_ino) == (other.st_dev, other.st_ino),
                    "Restored hard link differs from archive")
        elif member.isdir():
            require(stat.S_ISDIR(observed.st_mode), "Restored directory type differs")
        else:
            require(stat.S_ISREG(observed.st_mode) and observed.st_size == member.size,
                    "Restored file type or size differs")
            with archive.extractfile(member) as expected, path.open("rb") as actual:
                require(hashlib.file_digest(expected, "sha256").digest()
                        == hashlib.file_digest(actual, "sha256").digest(),
                        "Restored file contents differ from archive")


def restore_archive(path, expected, destination):
    destination = Path(destination)
    with open_verified(path, expected) as stream, tarfile.open(fileobj=stream, mode="r:") as archive:
        before = os.fstat(stream.fileno())
        members = data_members(archive)
        check_destination(destination, members)
        # A pinned descriptor keeps tar on the exact archive already inspected.
        # Strip only the data prefix, including hard-link targets.
        stream.seek(0)
        result = subprocess.run([
            "tar", "--extract", "--file", f"/proc/self/fd/{stream.fileno()}",
            "--directory", str(destination), "--strip-components=1", "--numeric-owner",
            "--same-owner", "--same-permissions", "--xattrs", "--xattrs-include=*",
            "--acls", "--delay-directory-restore", "--", "data",
        ], pass_fds=(stream.fileno(),), capture_output=True)
        require(result.returncode == 0, "GNU tar could not restore the selected data tree")
        root = next(m for m in members if m.name == "data")
        os.chown(destination, root.uid, root.gid)
        os.chmod(destination, stat.S_IMODE(root.mode))
        for name in os.listxattr(destination):
            if name not in attributes(root):
                os.removexattr(destination, name)
        for name, value in attributes(root).items():
            os.setxattr(destination, name, value)
        os.utime(destination, ns=(nanoseconds(root), nanoseconds(root)))
        verify_tree(archive, members, destination)
        after = os.fstat(stream.fileno())
        require((before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns)
                == (after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns),
                "Archive changed during restoration")
        subprocess.run(["sync", "-f", str(destination)], check=True)
        return {**summarize(members, expected), "contents_verified": True,
                "numeric_owners_verified": True, "modes_verified": True,
                "links_verified": True, "nanosecond_timestamps_verified": True,
                "extended_attributes_verified": True}
