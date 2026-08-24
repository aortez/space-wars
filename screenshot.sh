#!/bin/bash
set -euo pipefail

HOST="${1:-spacewars.local}"
REMOTE_FILE="${2:-/tmp/spacewars-screenshot.png}"
LOCAL_FILE="${3:-screenshot.png}"
LOG_FILE="${HOST}.log"

quote_for_shell() {
    printf "'%s'" "$(printf "%s" "$1" | sed "s/'/'\\\\''/g")"
}

REMOTE_FILE_QUOTED="$(quote_for_shell "${REMOTE_FILE}")"

ssh "spacewars@${HOST}" "spacewars-cli screenshot ${REMOTE_FILE_QUOTED}"
scp "spacewars@${HOST}:${REMOTE_FILE}" "${LOCAL_FILE}"
if ! ssh "spacewars@${HOST}" \
    "journalctl -u spacewars-kiosk --no-pager" > "${LOG_FILE}" 2>/dev/null; then
    ssh "spacewars@${HOST}" \
        "systemctl --no-pager --full status spacewars-kiosk; systemctl show spacewars-kiosk --property=MainPID --property=NRestarts --property=ActiveState --property=SubState" \
        > "${LOG_FILE}" 2>&1
fi

copy_to_clipboard() {
    if [[ -n "${WAYLAND_DISPLAY-}" || -n "${SWAYSOCK-}" ]]; then
        if command -v wl-copy >/dev/null 2>&1; then
            wl-copy --type image/png < "${LOCAL_FILE}"
            return 0
        fi
    fi

    if command -v xclip >/dev/null 2>&1; then
        xclip -selection clipboard -t image/png -i "${LOCAL_FILE}"
        return 0
    fi

    return 0
}

copy_to_clipboard
echo "Screenshot saved to ${LOCAL_FILE}. Logs saved to ${LOG_FILE}."
