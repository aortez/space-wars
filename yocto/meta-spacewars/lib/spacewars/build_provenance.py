"""Record the tools actually selected inside the recipe's build environment.

This unsigned record is part of the hashed rootfs. Publisher qualification must
compare it with reviewed build policy; a build's own claims are not approval.
"""
import json
from pathlib import Path
import re
import shlex
import subprocess

from spacewars.image_identity import validate_source

PROVENANCE_PATH = "usr/share/spacewars/build.json"


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
