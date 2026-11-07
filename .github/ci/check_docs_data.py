"""Keep raw results and private data out of docs/.

Prose and images belong under docs/. A data file there (JSON, CSV, text, HTML,
a compressed archive and the like) stays under the size limit unless code reads
it, and no text file under docs/ names a device or carries a secret. Oversized
files that predate the rule are listed in docs-data-grandfathered.txt, which
only shrinks.
"""

import re
import subprocess
import sys
from pathlib import Path

LIMIT = 256 * 1024
GRANDFATHERED = Path(".github/ci/docs-data-grandfathered.txt")
PROSE = {".md"}
IMAGES = {".gif", ".jpeg", ".jpg", ".png", ".svg", ".webp"}
UNSCANNED = {".gz", ".tar", ".zip"} | IMAGES
MAC = re.compile(r"\b(?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}\b")
PLACEHOLDER_MACS = {
    "00:00:00:00:00:00",
    "00:11:22:33:44:55",
    "01:23:45:67:89:ab",
    "aa:bb:cc:dd:ee:ff",
    "ff:ff:ff:ff:ff:ff",
}
SERIAL = re.compile(r'(?i)\b(?:[a-z]+_)?serial(?:_number)?\b"?\s*[:=]\s*"?([0-9A-Za-z_-]{6,})')
SECRET = re.compile(r'(?i)\b(?:passphrase|password|psk|wpa_passphrase)\b"?\s*[:=]\s*(?:"([^"]*)"|(\S.*))')
PLACEHOLDER = re.compile(r"(?i)secret|example|placeholder|your|change|xxx|\.\.\.|<[^>]*>|\$")
PRIVATE_KEY = re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")


def is_data(path):
    suffix = Path(path).suffix.lower()
    return suffix not in PROSE and suffix not in IMAGES


def names(path):
    """The names code would use for the file: its file name, and the stem of a compressed one."""
    base = Path(path).name
    result = {base}
    if base.endswith(".gz"):
        result.add(base[: -len(".gz")])
    return result


def identifier_findings(path, text):
    """Device identifiers and secrets in one text file's contents, as messages."""
    findings = []
    for number, line in enumerate(text.splitlines(), 1):
        for mac in MAC.findall(line):
            if mac.lower() not in PLACEHOLDER_MACS:
                findings.append(f"{path}:{number}: MAC address")
                break
        if SERIAL.search(line):
            findings.append(f"{path}:{number}: serial number")
        match = SECRET.search(line)
        if match:
            value = (match.group(1) or match.group(2) or "").strip()
            if len(value) >= 4 and not PLACEHOLDER.search(value):
                findings.append(f"{path}:{number}: credential")
        if PRIVATE_KEY.search(line):
            findings.append(f"{path}:{number}: private key")
    return findings


def check(files, read, referenced, grandfathered):
    """Check tracked docs files: (path, size) pairs, a reader, names code references, and the grandfathered paths."""
    problems = []
    listed = set(grandfathered)
    sizes = dict(files)
    for path in grandfathered:
        if path not in sizes:
            problems.append(f"{path}: no longer exists; remove it from {GRANDFATHERED}")
        elif sizes[path] <= LIMIT or not is_data(path):
            problems.append(f"{path}: no longer needs grandfathering; remove it from {GRANDFATHERED}")
    read_by_code = 0
    scanned = 0
    for path, size in files:
        if is_data(path) and size > LIMIT and path not in listed:
            if names(path) & referenced:
                read_by_code += 1
            else:
                problems.append(
                    f"{path}: {size / 1024:.0f} KiB of data that no code reads; keep raw results"
                    " on the workstation and summarize them in prose"
                )
        if Path(path).suffix.lower() not in UNSCANNED:
            scanned += 1
            problems.extend(identifier_findings(path, read(path)))
    if problems:
        raise ValueError("\n".join(problems))
    return (
        f"Docs data check passed: {len(files)} files, {scanned} scanned for identifiers,"
        f" {read_by_code} oversized data files read by code, {len(listed)} grandfathered."
    )


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout


def tracked_docs():
    """Tracked files under docs/ with their sizes in the working tree."""
    files = []
    for line in git("ls-files", "-z", "--", "docs").split("\0"):
        if line:
            files.append((line, Path(line).stat().st_size))
    return files


def referenced_names(files):
    """Names of oversized data files that tracked code outside docs/ mentions."""
    candidates = set()
    for path, size in files:
        if is_data(path) and size > LIMIT:
            candidates |= names(path)
    if not candidates:
        return set()
    result = subprocess.run(
        ["git", "grep", "-o", "-h", "-I", "-F", "-f", "-", "--", ".", ":!docs", ":!*.md"],
        input="\n".join(sorted(candidates)) + "\n",
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode not in (0, 1):
        raise ValueError(result.stderr.strip())
    return set(result.stdout.split())


def grandfathered_paths():
    if not GRANDFATHERED.exists():
        return []
    lines = GRANDFATHERED.read_text().splitlines()
    return [line.strip() for line in lines if line.strip() and not line.startswith("#")]


def read_text(path):
    return Path(path).read_bytes().decode("utf-8", "replace")


def main():
    try:
        files = tracked_docs()
        print(check(files, read_text, referenced_names(files), grandfathered_paths()))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Docs data check failed:\n{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
