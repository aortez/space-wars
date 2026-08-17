#!/bin/bash
# Build and deploy Space-Wars to the Raspberry Pi over SSH.
#
# Usage:
#   ./update.sh                         # Build + OTA flash + reboot + verify
#   ./update.sh --skip-build            # Deploy the existing image
#   ./update.sh --target 192.168.1.108   # Override the target host
#   ./update.sh --user spacewars        # Override the SSH user
#   ./update.sh --dry-run               # Show actions without changing the Pi
#   ./update.sh --prompt                # Require final confirmation
#   ./update.sh --help                  # Show all updater options

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
YOCTO_DIR="$SCRIPT_DIR/yocto"
TARGET_HOST="spacewars.local"
REMOTE_USER="spacewars"
DRY_RUN=false
SHOW_HELP=false
UPDATE_ARGS=("$@")

usage() {
    cat <<'EOF'
Space-Wars update

Usage:
  ./update.sh [options]

Options:
  --target <host>       Target host [default: spacewars.local]
  --host <host>         Alias for --target
  --user <user>         SSH user [default: spacewars]
  --remote-tmp <path>   Remote staging directory [default: /tmp]
  --image <path>        Use a specific rootfs .ext4.gz image
  --ssh-key <path>      Public SSH key to inject into the updated slot
  --skip-build          Deploy the existing image without rebuilding
  --dry-run             Print update actions without changing the Pi
  --prompt              Ask for final confirmation before flashing
  -h, --help            Show this help

Examples:
  ./update.sh
  ./update.sh --skip-build
  ./update.sh --target 192.168.1.108 --dry-run
EOF
}

fail() {
    echo "Error: $*" >&2
    exit 1
}

while (( $# > 0 )); do
    case "$1" in
        --target|--host)
            (( $# >= 2 )) || fail "Missing value for $1"
            TARGET_HOST="$2"
            shift 2
            ;;
        --user)
            (( $# >= 2 )) || fail "Missing value for --user"
            REMOTE_USER="$2"
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        -h|--help)
            SHOW_HELP=true
            shift
            ;;
        *)
            shift
            ;;
    esac
done

if [ "$SHOW_HELP" = true ]; then
    usage
    exit 0
fi

[ -d "$YOCTO_DIR" ] || fail "Yocto directory not found at $YOCTO_DIR"
command -v npm >/dev/null 2>&1 || fail "npm is required to run the updater"

(cd "$YOCTO_DIR" && npm run update -- "${UPDATE_ARGS[@]}")

if [ "$DRY_RUN" = true ]; then
    exit 0
fi

REMOTE_TARGET="${REMOTE_USER}@${TARGET_HOST}"
SSH_OPTIONS=(
    -o BatchMode=yes
    -o ConnectTimeout=3
    -o ConnectionAttempts=1
    -o LogLevel=ERROR
)

echo "Verifying spacewars-kiosk.service on ${REMOTE_TARGET}..."

deadline=$((SECONDS + 30))
while (( SECONDS < deadline )); do
    if ssh "${SSH_OPTIONS[@]}" "$REMOTE_TARGET" \
        "systemctl is-active --quiet spacewars-kiosk.service"; then
        ssh "${SSH_OPTIONS[@]}" "$REMOTE_TARGET" \
            'printf "Boot root: "; sed -n "s/.* root=\([^ ]*\).*/\1/p" /proc/cmdline; systemctl show spacewars-kiosk.service --property=ActiveState --property=MainPID --property=NRestarts'
        echo "Space-Wars update verified."
        exit 0
    fi
    sleep 2
done

echo "Error: spacewars-kiosk.service did not become active on ${REMOTE_TARGET}" >&2
ssh "${SSH_OPTIONS[@]}" "$REMOTE_TARGET" \
    "systemctl --no-pager --full status spacewars-kiosk.service" || true
exit 1
