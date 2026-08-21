#!/usr/bin/env bash
# Sync local, gitignored Space-Wars data to the persistent kiosk data directory.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REMOTE_HOST="${SPACEWARS_HOST:-spacewars.local}"
REMOTE_USER="${SPACEWARS_USER:-spacewars}"
LOCAL_DATA_DIR="${SPACEWARS_DATA_DIR:-$SCRIPT_DIR/data}"
REMOTE_DATA_DIR="${SPACEWARS_REMOTE_DATA_DIR:-/var/lib/spacewars}"
DRY_RUN=false
DELETE=false

usage() {
    cat <<'EOF'
Space-Wars data sync

Syncs the local data/roms directory to the kiosk's persistent ROM library.
The repository ignores data/, so user-owned ROMs cannot be committed normally.

Usage:
  ./sync-data.sh [options]

Options:
  --host <host>              Target host [default: spacewars.local]
  --target <host>            Alias for --host
  --user <user>              SSH user [default: spacewars]
  --data-dir <path>          Local data root [default: <repo>/data]
  --remote-data-dir <path>   Remote data root [default: /var/lib/spacewars]
  --dry-run                  Show the proposed transfer without changing files
  --delete                   Delete remote ROM files absent from local data/roms
  -h, --help                 Show this help

Environment overrides:
  SPACEWARS_HOST
  SPACEWARS_USER
  SPACEWARS_DATA_DIR
  SPACEWARS_REMOTE_DATA_DIR

Examples:
  mkdir -p data/roms
  ./sync-data.sh --dry-run
  ./sync-data.sh
  ./sync-data.sh --host 192.168.1.108
EOF
}

fail() {
    printf 'Error: %s\n' "$*" >&2
    exit 1
}

shell_quote() {
    printf "'"
    printf '%s' "$1" | sed "s/'/'\\\\''/g"
    printf "'"
}

while (( $# > 0 )); do
    case "$1" in
        --host|--target)
            (( $# >= 2 )) || fail "Missing value for $1"
            REMOTE_HOST="$2"
            shift 2
            ;;
        --user)
            (( $# >= 2 )) || fail "Missing value for --user"
            REMOTE_USER="$2"
            shift 2
            ;;
        --data-dir)
            (( $# >= 2 )) || fail "Missing value for --data-dir"
            LOCAL_DATA_DIR="$2"
            shift 2
            ;;
        --remote-data-dir)
            (( $# >= 2 )) || fail "Missing value for --remote-data-dir"
            REMOTE_DATA_DIR="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --delete)
            DELETE=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "Unknown option: $1"
            ;;
    esac
done

[ -n "$REMOTE_HOST" ] || fail "Remote host must not be empty"
[ -n "$REMOTE_USER" ] || fail "Remote user must not be empty"

REMOTE_DATA_DIR="${REMOTE_DATA_DIR%/}"
case "$REMOTE_DATA_DIR" in
    ""|/|/var|/var/lib)
        fail "Refusing unsafe remote data directory: ${REMOTE_DATA_DIR:-<empty>}"
        ;;
    /*) ;;
    *)
        fail "Remote data directory must be an absolute path: $REMOTE_DATA_DIR"
        ;;
esac

LOCAL_ROM_DIR="${LOCAL_DATA_DIR%/}/roms"
REMOTE_ROM_DIR="$REMOTE_DATA_DIR/roms"
[ -d "$LOCAL_ROM_DIR" ] || fail \
    "Local ROM directory does not exist: $LOCAL_ROM_DIR (create it with: mkdir -p data/roms)"

command -v ssh >/dev/null 2>&1 || fail "ssh is required"
command -v rsync >/dev/null 2>&1 || fail "rsync is required"

REMOTE_TARGET="${REMOTE_USER}@${REMOTE_HOST}"
SSH_OPTIONS=(
    -o BatchMode=yes
    -o ConnectTimeout=10
    -o ConnectionAttempts=1
    -o LogLevel=ERROR
)
RSYNC_RSH="ssh -o BatchMode=yes -o ConnectTimeout=10 -o ConnectionAttempts=1 -o LogLevel=ERROR"
RSYNC_ARGS=(
    --recursive
    --times
    --compress
    --checksum
    --partial
    --protect-args
    --human-readable
    --itemize-changes
    '--chmod=Du=rwx,Dgo=rx,Fu=rw,Fgo=r'
    --rsh="$RSYNC_RSH"
)

if [ "$DRY_RUN" = true ]; then
    RSYNC_ARGS+=(--dry-run)
fi
if [ "$DELETE" = true ]; then
    RSYNC_ARGS+=(--delete-delay)
fi

printf 'Syncing Space-Wars ROM data\n'
printf '  local:  %s/\n' "$LOCAL_ROM_DIR"
printf '  remote: %s:%s/\n' "$REMOTE_TARGET" "$REMOTE_ROM_DIR"
if [ "$DRY_RUN" = true ]; then
    printf '  mode:   dry run\n'
elif [ "$DELETE" = true ]; then
    printf '  mode:   mirror, including remote deletions\n'
else
    printf '  mode:   additive\n'
fi

if [ "$DRY_RUN" = false ]; then
    remote_prepare="mkdir -p $(shell_quote "$REMOTE_ROM_DIR")"
    # The destination is validated above and shell-quoted for the remote shell.
    # shellcheck disable=SC2029
    ssh "${SSH_OPTIONS[@]}" "$REMOTE_TARGET" "$remote_prepare"
fi

rsync "${RSYNC_ARGS[@]}" "$LOCAL_ROM_DIR/" "$REMOTE_TARGET:$REMOTE_ROM_DIR/"

if [ "$DRY_RUN" = true ]; then
    printf 'Dry run complete; no files were changed.\n'
else
    printf 'ROM data synced. Return to the launcher to refresh the library.\n'
fi
