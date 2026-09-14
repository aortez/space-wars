"""Fail an unchanged Cargo build if it recompiles instead of reusing artifacts."""

import json
import sys


def assert_fresh_build(lines):
    count = 0
    client_seen = False
    finished = False
    rebuilt = []
    for line in lines:
        message = json.loads(line)
        if message.get("reason") == "build-finished":
            finished = message.get("success") is True
        if message.get("reason") != "compiler-artifact":
            continue
        count += 1
        target = message["target"]
        client_seen |= target["name"] == "engine-client" and "bin" in target["kind"]
        if message.get("fresh") is not True:
            rebuilt.append(f"{target['name']} ({', '.join(target['kind'])})")
    if not finished or not client_seen:
        raise ValueError("Expected a successful workspace build including the client binary")
    if rebuilt:
        raise ValueError("Unchanged build recompiled: " + "; ".join(rebuilt))
    return f"Build reuse verified: all {count} compiler artifacts were fresh."


def main():
    try:
        print(assert_fresh_build(sys.stdin))
    except (ValueError, KeyError) as error:
        print(f"Build reuse check failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
