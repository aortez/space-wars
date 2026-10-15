#!/bin/bash
# Experimental-image only: a temporary, fixed renderer override. Never edit
# boot/user settings or accept caller-selected paths, units or environment.
set -euo pipefail
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
umask 077

fail() { echo "Renderer switch: $*" >&2; exit 1; }
[[ $# == 1 ]] || fail "Usage: spacewars-renderer software|gpu"
case "$1" in
    software) renderer=software ;;
    gpu) renderer=femtovg ;;
    *) fail "Usage: spacewars-renderer software|gpu" ;;
esac
[[ $(id -u) == 0 ]] || fail "Run through sudo."

# The fixed engine-os-manager entry point retains the shared OTA lock.
state_dir=/run/spacewars-renderer
[[ ! -L $state_dir ]] || fail "Unsafe renderer state directory."
install -d -o root -g root -m 0755 "$state_dir"
override=$state_dir/renderer.env
[[ ! -L $override ]] || fail "Unsafe renderer override."
service=spacewars-kiosk.service
temporary=
rollback=false

# Do not inherit caller-controlled D-Bus/systemd variables. All service calls
# target this one system unit and have a deadline.
service_command() {
    env -i PATH="$PATH" timeout 15 systemctl --no-pager "$@" "$service"
}

write_renderer() {
    temporary=$(mktemp "$state_dir/.renderer.XXXXXXXXXX") || return 1
    printf 'SLINT_BACKEND=linuxkms-%s\n' "$1" > "$temporary" || return 1
    chmod 0644 "$temporary" || return 1
    mv -f -- "$temporary" "$override" || return 1
    temporary=
}

healthy() {
    local expected=$1 previous_pid='' good=0 pid status attempt deadline=$((SECONDS + 30))
    for ((attempt = 0; attempt < 12 && SECONDS < deadline; attempt++)); do
        sleep 1
        pid=$(service_command show --property=MainPID --value) || return 1
        if [[ $pid =~ ^[1-9][0-9]*$ ]] && service_command is-active --quiet \
            && status=$(env -i PATH="$PATH" setpriv --reuid=spacewars --regid=spacewars \
                --init-groups --no-new-privs timeout 2 /usr/bin/spacewars-cli status) \
            && [[ $'\n'$status$'\n' == *$'\n'kms_renderer="$expected"$'\n'* ]]; then
            if [[ $pid == "$previous_pid" ]]; then
                good=$((good + 1))
            else
                good=1
            fi
            [[ $good == 3 ]] && return 0
        else
            good=0
        fi
        previous_pid=$pid
    done
    return 1
}

cleanup() {
    local result=$?
    trap - EXIT HUP INT TERM
    if [[ -n $temporary ]]; then rm -f -- "$temporary"; fi
    if [[ $rollback == true ]]; then
        echo "Renderer trial failed; restoring software rendering." >&2
        if service_command stop && write_renderer software \
            && service_command reset-failed && service_command start && healthy software; then
            echo "Software renderer restored and verified." >&2
        else
            echo "Software recovery failed; check spacewars-cli logs. A reboot clears this override." >&2
        fi
        result=1
    fi
    if [[ -n $temporary ]]; then rm -f -- "$temporary"; fi
    exit "$result"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

echo "Switching kiosk to $renderer (ends the current session; no reboot)."
service_command stop
rollback=true
write_renderer "$renderer"
service_command reset-failed
service_command start
healthy "$renderer" || fail "Requested renderer did not become healthy."
rollback=false
echo "Verified $renderer. The temporary override clears at reboot."
