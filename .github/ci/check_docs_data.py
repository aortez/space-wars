"""Keep raw results and private data out of docs/.

Prose and images belong under docs/. A data file there (JSON, CSV, text, HTML,
a compressed archive and the like) stays under the size limit unless a source
file names it, and no text file under docs/ names a device or carries a
secret.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

LIMIT = 256 * 1024
PROSE = {".md"}
IMAGES = {".gif", ".jpeg", ".jpg", ".png", ".svg", ".webp"}
UNSCANNED = {".gz", ".tar", ".zip"} | IMAGES
# Source files whose mention of a data file counts as code reading it. Data,
# text and Markdown files do not count.
SOURCE = [
    "*.bb",
    "*.bbappend",
    "*.bbclass",
    "*.c",
    "*.cpp",
    "*.h",
    "*.hpp",
    "*.inc",
    "*.js",
    "*.mjs",
    "*.ps1",
    "*.py",
    "*.rs",
    "*.sh",
    "*.slint",
    "*.ts",
    "*.yaml",
    "*.yml",
]
MAC = re.compile(r"\b(?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}\b")
PLACEHOLDER_MACS = {
    "00:00:00:00:00:00",
    "00:11:22:33:44:55",
    "01:23:45:67:89:ab",
    "aa:bb:cc:dd:ee:ff",
    "ff:ff:ff:ff:ff:ff",
}
SERIAL = re.compile(r'(?i)\b(?:[a-z]+_)?serial(?:_number)?\b"?\s*[:=]\s*"?([0-9A-Za-z_-]{6,})')
SECRET = re.compile(
    r"""(?i)\b(?:passphrase|password|psk|wpa_passphrase)\b"?\s*[:=]\s*(?:"([^"]*)"|'([^']*)'|(<[^<>]*>)|(\S+))"""
)
# A whole credential value that stands in for a real one: a variable, a
# template, a description in angle brackets, an ellipsis or a masked run.
VARIABLE = re.compile(
    r"^(?:\$\{?[A-Za-z_][A-Za-z0-9_]*\}?|%[A-Za-z_][A-Za-z0-9_]*%|\{\{[^}]*\}\}|<[^<>]+>|\.\.\.|…|[*xX•]{3,})$"
)
PLACEHOLDER_WORDS = {
    "a",
    "an",
    "change",
    "changeme",
    "dummy",
    "example",
    "here",
    "key",
    "me",
    "my",
    "network",
    "new",
    "none",
    "null",
    "old",
    "omitted",
    "owner",
    "owners",
    "passphrase",
    "password",
    "placeholder",
    "psk",
    "redacted",
    "secret",
    "the",
    "unset",
    "value",
    "wifi",
    "your",
}
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


def is_placeholder(value):
    """Whether a whole credential value only stands in for a real one."""
    if VARIABLE.match(value):
        return True
    tokens = [token.lower() for token in re.split(r"(?<=[a-z])(?=[A-Z])|[-_ ]+", value) if token]
    return bool(tokens) and all(token in PLACEHOLDER_WORDS for token in tokens)


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
            value = next((group for group in match.groups() if group is not None), "").strip()
            if len(value) >= 4 and not is_placeholder(value):
                findings.append(f"{path}:{number}: credential")
        if PRIVATE_KEY.search(line):
            findings.append(f"{path}:{number}: private key")
    return findings


def check(files, read, referenced):
    """Check tracked docs files: (path, size) pairs, a reader and the names source files mention."""
    problems = []
    read_by_code = 0
    scanned = 0
    for path, size in files:
        if is_data(path) and size > LIMIT:
            if names(path) & referenced:
                read_by_code += 1
            else:
                problems.append(
                    f"{path}: {size / 1024:.0f} KiB of data that no source file names; keep raw results"
                    " on the workstation and summarize them in prose"
                )
        if Path(path).suffix.lower() not in UNSCANNED:
            scanned += 1
            problems.extend(identifier_findings(path, read(path)))
    if problems:
        raise ValueError("\n".join(problems))
    return (
        f"Docs data check passed: {len(files)} files, {scanned} scanned for identifiers,"
        f" {read_by_code} oversized data files read by code."
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
    """Names of oversized data files that tracked source files outside docs/ mention."""
    candidates = set()
    for path, size in files:
        if is_data(path) and size > LIMIT:
            candidates |= names(path)
    if not candidates:
        return set()
    result = subprocess.run(
        ["git", "grep", "-o", "-h", "-I", "-F", "-f", "-", "--", ":!docs", *SOURCE],
        input="\n".join(sorted(candidates)) + "\n",
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode not in (0, 1):
        raise ValueError(result.stderr.strip())
    return set(result.stdout.split())


def read_text(path):
    return Path(path).read_bytes().decode("utf-8", "replace")


def main():
    argparse.ArgumentParser(description=__doc__).parse_args()
    try:
        files = tracked_docs()
        print(check(files, read_text, referenced_names(files)))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Docs data check failed:\n{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
