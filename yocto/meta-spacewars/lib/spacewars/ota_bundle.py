"""Embed explicitly selected public bootstrap material; never generate keys.

This build check validates structure, public-only fields and the selected file
digest. The root-operator provisioner performs TUF signature verification before
establishing trust. Neither check enables installation or changes device state.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
from urllib.parse import urlsplit

MAX_BYTES = 512 * 1024
DESTINATION = Path("etc/spacewars/ota-bootstrap.json")


def _object(value, fields):
    if not isinstance(value, dict) or set(value) != set(fields):
        raise ValueError("unsupported fields in public OTA bootstrap bundle")
    return value


def _unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate field in public OTA bootstrap bundle")
        result[key] = value
    return result


def validate_public_bundle(content):
    if len(content) > MAX_BYTES:
        raise ValueError("OTA bootstrap bundle exceeds size bound")
    try:
        bundle = json.loads(content, object_pairs_hook=_unique)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise ValueError("invalid public OTA bootstrap JSON") from error
    _object(bundle, ("schema", "root", "location"))
    if type(bundle["schema"]) is not int or bundle["schema"] != 1:
        raise ValueError("unsupported OTA bootstrap bundle schema")
    location = _object(bundle["location"], ("metadata", "targets"))
    for value in location.values():
        if not isinstance(value, str) or len(value) > 4096:
            raise ValueError("invalid public OTA feed location")
        try:
            parsed = urlsplit(value)
            valid = (parsed.scheme == "https" and parsed.hostname and
                     parsed.username is None and parsed.password is None and
                     not parsed.query and not parsed.fragment and
                     parsed.path.endswith("/") and (parsed.port is None or parsed.port > 0))
        except ValueError:
            valid = False
        if not valid:
            raise ValueError("expected credential-free HTTPS OTA feed bases")
    root = _object(bundle["root"], ("signed", "signatures"))
    signed = _object(root["signed"], ("_type", "spec_version", "version", "expires",
                                      "consistent_snapshot", "keys", "roles"))
    if signed["_type"] != "root" or signed["consistent_snapshot"] is not True:
        raise ValueError("expected a consistent-snapshot TUF root")
    if not isinstance(signed["keys"], dict) or not signed["keys"]:
        raise ValueError("missing public TUF keys")
    for key in signed["keys"].values():
        _object(key, ("keytype", "scheme", "keyval"))
        public = _object(key["keyval"], ("public",))["public"]
        if not isinstance(public, str) or not public or "PRIVATE KEY" in public:
            raise ValueError("OTA bootstrap accepts public key material only")
    for role in _object(signed["roles"], ("root", "targets", "snapshot", "timestamp")).values():
        _object(role, ("keyids", "threshold"))
        if (type(role["threshold"]) is not int or role["threshold"] < 1 or
                not isinstance(role["keyids"], list) or
                not all(isinstance(key, str) for key in role["keyids"]) or
                len(set(role["keyids"])) != len(role["keyids"]) or
                len(role["keyids"]) < role["threshold"] or
                not set(role["keyids"]).issubset(signed["keys"])):
            raise ValueError("invalid public TUF role keys or threshold")
    if not isinstance(root["signatures"], list) or not root["signatures"]:
        raise ValueError("missing TUF root signatures")
    for signature in root["signatures"]:
        _object(signature, ("keyid", "sig"))
    return bundle


def read_selected_bundle(source, expected):
    if not source or not expected:
        raise ValueError("OTA bootstrap source and independently selected SHA-256 are both required")
    if not Path(source).is_absolute() or not re.fullmatch(r"[0-9a-f]{64}", expected):
        raise ValueError("expected absolute bootstrap path and lowercase SHA-256")
    descriptor = os.open(source, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_nlink != 1 or before.st_size > MAX_BYTES:
            raise ValueError("unsafe or oversized OTA bootstrap source")
        content = stream.read(MAX_BYTES + 1)
        after = os.fstat(stream.fileno())
        if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (
                after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns, after.st_ctime_ns):
            raise ValueError("OTA bootstrap source changed while reading")
    if hashlib.sha256(content).hexdigest() != expected:
        raise ValueError("OTA bootstrap source differs from selected SHA-256")
    validate_public_bundle(content)
    return content


def install_selected_bundle(rootfs, source, expected):
    content = read_selected_bundle(source, expected)
    rootfs = Path(rootfs)
    for relative in ("etc", "etc/spacewars"):
        directory = rootfs / relative
        directory.mkdir(exist_ok=True, mode=0o755)
        if directory.is_symlink() or not directory.is_dir():
            raise ValueError("unsafe OTA bundle image directory")
    destination = rootfs / DESTINATION
    descriptor = os.open(destination, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    with os.fdopen(descriptor, "wb") as stream:
        stream.write(content)
        os.fchmod(stream.fileno(), 0o644)
    return expected
