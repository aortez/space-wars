"""Record the tools actually selected inside the recipe's build environment.

This unsigned record is part of the hashed rootfs. Publisher qualification must
compare it with reviewed build policy; a build's own claims are not approval.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import stat
import subprocess

from spacewars.image_identity import validate_source

PROVENANCE_PATH = "usr/share/spacewars/build.json"
INPUTS_PATH = "usr/share/spacewars/build-inputs.json"


def selected_inputs(path, expected_sha256, source):
    """Public build inputs only; absence is allowed for development images."""
    if not path and not expected_sha256:
        return None
    if not path or not re.fullmatch(r"[0-9a-f]{64}", expected_sha256 or ""):
        raise ValueError("select both OTA build inputs and their SHA-256")
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or not 0 < before.st_size <= 16 * 1024:
            raise ValueError("invalid OTA build input file")
        raw = stream.read(16 * 1024 + 1)
    if len(raw) != before.st_size or hashlib.sha256(raw).hexdigest() != expected_sha256:
        raise ValueError("OTA build inputs differ from the selected digest")
    value = json.loads(raw)
    if (not isinstance(value, dict) or type(value.get("schema")) is not int or value["schema"] != 1
            or value.get("source_commit") != source["commit"] or source["dirty"]
            or value.get("machine") != "raspberrypi-spacewars"
            or value.get("target") != "spacewars-image"):
        raise ValueError("OTA build inputs belong to a different source or image")
    return raw


def capture(source, environment, c_compiler, rust_target, profile, feature, run=subprocess.run):
    validate_source(source)
    if (rust_target != "aarch64-unknown-linux-gnu" or profile != "release"
            or feature not in ("pi-kiosk", "pi-kiosk-gpu")):
        raise ValueError("unsupported packaged build configuration")

    def version(command):
        result = run(command, env=environment, check=True, capture_output=True,
                     text=True, timeout=15)
        output = result.stdout.strip()
        if not 1 <= len(output.encode()) <= 8192 or "\x00" in output:
            raise ValueError("invalid toolchain version response")
        return output

    compiler = shlex.split(c_compiler)
    if not compiler:
        raise ValueError("missing target C compiler")
    rust = version(["rustc", "--version", "--verbose"])
    cargo = version(["cargo", "--version", "--verbose"])
    cc = version([*compiler, "--version"])
    rust_version = re.search(r"^release: ([0-9]+\.[0-9]+\.[0-9]+)$", rust, re.MULTILINE)
    cargo_version = re.match(r"cargo ([0-9]+\.[0-9]+\.[0-9]+) ", cargo)
    if not rust_version or not cargo_version:
        raise ValueError("unrecognized Rust/Cargo version response")
    return {
        "schema": 1, "source": source,
        "rust_target": rust_target, "profile": profile, "kiosk_feature": feature,
        "tools": {
            "rustc": {"version": rust_version[1], "details": rust},
            "cargo": {"version": cargo_version[1], "details": cargo},
            "cc": {"details": cc},
        },
    }


def write(path, value):
    path = Path(path)
    if path.is_symlink() or (path.exists() and not path.is_file()):
        raise ValueError("invalid previous build provenance output")
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n")
    path.chmod(0o644)
