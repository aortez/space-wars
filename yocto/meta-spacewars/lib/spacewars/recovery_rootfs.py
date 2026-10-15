"""Check the immutable initramfs without executing its target binaries."""

import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess


MODULES = (
    "00-spacewarswatchdog", "01-udev", "90-rootfs", "98-spacewarsrun", "99-finish",
)
PROGRAMS = (
    "/bin/sh", "/bin/mount", "/bin/mountpoint", "/bin/umount", "/bin/mkdir",
    "/bin/cat", "/bin/grep", "/bin/sed", "/bin/sleep", "/bin/touch",
    "/usr/bin/awk", "/usr/bin/basename", "/usr/bin/cut", "/usr/bin/killall",
    "/sbin/switch_root", "/usr/bin/udevadm", "/lib/systemd/systemd-udevd",
    "/usr/sbin/spacewars-trial-watchdog",
)


def resolve_in_root(root: Path, name: str) -> Path:
    """Resolve usrmerge and absolute links relative to the image, never the host."""
    pending = list(PurePosixPath(name).parts)
    parts = []
    links = 0
    while pending:
        part = pending.pop(0)
        if part in ("/", "."):
            continue
        if part == "..":
            if not parts:
                raise ValueError(f"initramfs path escapes its root: {name}")
            parts.pop()
            continue
        path = root.joinpath(*parts, part)
        try:
            mode = path.lstat().st_mode
        except OSError as error:
            raise ValueError(f"missing initramfs dependency {name}: {path}") from error
        if stat.S_ISLNK(mode):
            links += 1
            if links > 40:
                raise ValueError(f"initramfs symlink loop: {name}")
            target = PurePosixPath(os.readlink(path))
            if target.is_absolute():
                parts = []
            pending = list(target.parts) + pending
        else:
            if pending and not stat.S_ISDIR(mode):
                raise ValueError(f"initramfs path component is not a directory: {path}")
            parts.append(part)
    return root.joinpath(*parts)


def verify_programs(root, programs, readelf="readelf", machine=183, owner=0):
    """Check executable/interpreter/library closure for AArch64 (ELF EM=183)."""
    root = Path(root).resolve()
    pending = [(name, True) for name in programs]
    checked = set()
    while pending:
        name, executable = pending.pop()
        path = resolve_in_root(root, name)
        metadata = path.stat()
        if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != owner
                or metadata.st_mode & 0o022 or (executable and not metadata.st_mode & 0o111)):
            raise ValueError(f"unsafe initramfs dependency: {name}")
        if path in checked:
            continue
        checked.add(path)
        with path.open("rb") as source:
            header = source.read(256)
        if header.startswith(b"#!"):
            interpreter = header.split(b"\n", 1)[0][2:].split()[0].decode("ascii")
            if not interpreter.startswith("/"):
                raise ValueError(f"relative initramfs interpreter: {name}")
            pending.append((interpreter, True))
            continue
        if (len(header) < 20 or header[:6] != b"\x7fELF\x02\x01"
                or int.from_bytes(header[18:20], "little") != machine):
            raise ValueError(f"wrong initramfs ELF architecture: {name}")
        try:
            info = subprocess.run([readelf, "-l", "-d", str(path)], check=True,
                                  capture_output=True, text=True, timeout=30,
                                  env={**os.environ, "LC_ALL": "C"}).stdout
        except (OSError, subprocess.SubprocessError) as error:
            raise ValueError(f"cannot inspect initramfs ELF {name}: {error}") from error
        interpreter = re.search(r"Requesting program interpreter: ([^\]]+)\]", info)
        if interpreter:
            pending.append((interpreter[1], True))
        origin = "/" + str(path.parent.relative_to(root))
        search = []
        for entry in re.findall(r"\((?:RUNPATH|RPATH)\).*\[([^\]]*)\]", info):
            for directory in entry.split(":"):
                directory = directory.replace("${ORIGIN}", origin).replace("$ORIGIN", origin)
                if not directory.startswith("/") or "$" in directory:
                    raise ValueError(f"unsupported initramfs library path: {name}: {directory}")
                search.append(directory)
        search.extend(("/lib", "/usr/lib", "/lib64", "/usr/lib64"))
        for library in re.findall(r"\(NEEDED\).*\[([^\]]+)\]", info):
            if "/" in library:
                raise ValueError(f"unsupported initramfs library name: {library}")
            for directory in search:
                dependency = directory + "/" + library
                try:
                    resolve_in_root(root, dependency)
                except ValueError:
                    continue
                pending.append((dependency, False))
                break
            else:
                raise ValueError(f"missing initramfs library: {name}: {library}")
    return sorted("/" + str(path.relative_to(root)) for path in checked)


def verify_recovery_rootfs(root, readelf="readelf"):
    root = Path(root)
    modules = sorted(path.name for path in (root / "init.d").iterdir())
    if modules != list(MODULES):
        raise ValueError(f"unexpected recovery initramfs module order: {modules}")
    return verify_programs(root, ("/init", *PROGRAMS, *(
        "/init.d/" + module for module in MODULES)), readelf)
