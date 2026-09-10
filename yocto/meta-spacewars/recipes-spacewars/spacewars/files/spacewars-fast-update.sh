#!/bin/bash
# Restricted application-only installer. Never installs services, libraries,
# arbitrary paths, or boot assets. A/B updates remain the OS upgrade mechanism.
set -euo pipefail
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
umask 077

fail() { echo "Fast update: $*" >&2; exit 1; }
[[ $(id -u) == 0 ]] || fail "Run through sudo."
compat_file=/usr/share/spacewars/fast-update-compat
[[ -f $compat_file ]] || fail "Missing compatibility identity; perform a full update."
compat=$(<"$compat_file")
[[ $compat =~ ^[a-f0-9]{64}$ ]] || fail "Invalid installed compatibility identity."

if [[ $# == 1 && $1 == --check ]]; then
    echo "spacewars-fast-update-v1 $compat"
    exit 0
fi
[[ $# == 4 ]] || fail "Expected staging directory, compatibility ID and two SHA-256 hashes."
stage=$1
[[ $stage =~ ^/tmp/spacewars-fast\.[a-zA-Z0-9]{10}$ ]] || fail "Invalid staging directory."
[[ $2 == "$compat" ]] || fail "Runtime or service configuration changed; perform a full update."
[[ $3 =~ ^[a-f0-9]{64}$ && $4 =~ ^[a-f0-9]{64}$ ]] || fail "Invalid SHA-256 hash."
expected=("$3" "$4")
names=(engine-client spacewars-cli)
service=spacewars-kiosk.service

exec 9>/run/lock/spacewars-fast-update.lock
flock -n 9 || fail "Another fast update is running."
[[ $(uname -m) == aarch64 ]] || fail "Only aarch64 targets are supported."
systemctl is-active --quiet "$service" || fail "Kiosk must be running before a fast update."

# Keep new files and backups on the destination filesystem. Read untrusted
# staging paths as the kiosk user, never as root (including symlink targets).
storage=/usr/lib/spacewars-fast-update
install -d -m 0700 "$storage"
transaction=$(mktemp -d "$storage/transaction.XXXXXXXXXX")
rollback=false
cleanup() {
    result=$?
    trap - EXIT HUP INT TERM
    if [[ $rollback == true ]]; then
        echo "Fast update failed; restoring the previous client and CLI." >&2
        # If stopping fails, retain both backups rather than modifying a live app.
        if ! systemctl stop "$service"; then
            echo "Could not stop kiosk; recovery files retained at $transaction" >&2
            exit 1
        fi
        for name in "${names[@]}"; do
            if ! mv -f -- "$transaction/$name.previous" "/usr/bin/$name"; then
                echo "Restore failed; recovery files retained at $transaction" >&2
                exit 1
            fi
        done
        sync
        if ! systemctl start "$service"; then
            echo "Previous binaries restored, but kiosk restart failed." >&2
            result=1
        fi
    fi
    for name in "${names[@]}"; do
        rm -f -- "$transaction/$name" "$transaction/$name.previous"
    done
    rmdir -- "$transaction"
    exit "$result"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM

for index in 0 1; do
    name=${names[$index]}
    setpriv --reuid=spacewars --regid=spacewars --init-groups --no-new-privs \
        cat -- "$stage/$name" > "$transaction/$name"
    actual=$(sha256sum "$transaction/$name")
    [[ ${actual%% *} == "${expected[$index]}" ]] || fail "$name checksum mismatch."
    header=$(od -An -tx1 -N7 "$transaction/$name" | tr -d ' \n')
    arch=$(od -An -tx1 -j18 -N2 "$transaction/$name" | tr -d ' \n')
    [[ $header == 7f454c46020101 && $arch == b700 ]] || fail "$name is not an AArch64 ELF executable."
    chmod 0755 "$transaction/$name"
    cp -p -- "/usr/bin/$name" "$transaction/$name.previous"
done
sync

rollback=true
echo "Restarting the kiosk with the new client and CLI (no reboot)."
systemctl stop "$service"
for name in "${names[@]}"; do
    mv -f -- "$transaction/$name" "/usr/bin/$name"
done
sync
systemctl start "$service"

# A live control socket verifies application startup, not just systemd's PID.
# Require three consecutive responses from the same process; a crash loop is
# not a healthy deployment. Bound each call as well as the overall poll count.
healthy=0
previous_pid=
for ((attempt = 0; attempt < 20; attempt++)); do
    sleep 1
    pid=$(systemctl show "$service" --property=MainPID --value)
    if [[ $pid != 0 ]] && systemctl is-active --quiet "$service" \
        && setpriv --reuid=spacewars --regid=spacewars --init-groups --no-new-privs \
            timeout 2 /usr/bin/spacewars-cli status >/dev/null 2>&1; then
        if [[ $pid == "$previous_pid" ]]; then
            healthy=$((healthy + 1))
        else
            healthy=1
        fi
        [[ $healthy == 3 ]] && break
    else
        healthy=0
    fi
    previous_pid=$pid
done
[[ $healthy == 3 ]] || fail "New kiosk did not become healthy."

# Keep one known-good pair for manual recovery. These are root-owned, and a
# subsequent A/B rootfs update replaces this slot's fast-installed binaries.
for name in "${names[@]}"; do
    cp -p -- "$transaction/$name.previous" "$storage/$name.previous"
done
sync
rollback=false
echo "Fast update verified: kiosk PID $pid. Previous binaries: $storage"
