#!/bin/sh
# Read-only, fixed-unit journal access. No caller-supplied journalctl options.
set -eu

usage() {
    echo "Usage: spacewars-logs --lines <1-1000> [--follow]" >&2
    exit 2
}

[ "$#" -eq 2 ] || [ "$#" -eq 3 ] || usage
[ "$1" = "--lines" ] || usage
case "$2" in
    [1-9]|[1-9][0-9]|[1-9][0-9][0-9]|1000) ;;
    *) usage ;;
esac
lines=$2
if [ "$#" -eq 3 ]; then
    [ "$3" = "--follow" ] || usage
    set -- --follow
else
    set --
fi

# Clear journal/pager environment overrides before executing as root. Restrict
# history to the current boot; retain journald's existing storage/rotation policy.
exec /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /usr/bin/journalctl \
    --no-pager --quiet --boot=0 --unit=spacewars-kiosk.service \
    --output=short-iso-precise --lines="$lines" "$@"
