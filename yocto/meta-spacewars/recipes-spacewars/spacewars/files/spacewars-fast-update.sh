#!/bin/bash
# Root/operator worker, invoked only by engine-os-manager under the OTA lock.
# Never installs services, libraries, arbitrary paths or boot assets.
set -euo pipefail
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
umask 077

fail() { echo "Fast update: $*" >&2; exit 1; }
[[ $(id -u) == 0 ]] || fail "Separate root operator access is required."
compat_file=/usr/share/spacewars/fast-update-compat
[[ -f $compat_file ]] || fail "Missing compatibility identity; perform a full update."
compat=$(<"$compat_file")
[[ $compat =~ ^[a-f0-9]{64}$ ]] || fail "Invalid installed compatibility identity."

if [[ $# == 1 && $1 == --check ]]; then
    echo "spacewars-fast-update-v2 $compat"
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

[[ $(uname -m) == aarch64 ]] || fail "Only aarch64 targets are supported."
service_command() {
    env -i PATH="$PATH" timeout 15 systemctl --no-pager "$@" "$service"
}
service_command is-active --quiet || fail "Kiosk must be running before a fast update."

# SCP is performed by the separately authenticated root operator. A private
# stage and regular, single-link files prevent following user-controlled links.
[[ -d $stage && ! -L $stage && $(stat -c '%u:%a' -- "$stage") == 0:700 ]] \
    || fail "Staging must be a private root-owned directory."

# Keep new files and backups on the destination filesystem. Operator staging
# remains root-owned and must not be shared with the kiosk account.
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
        if ! service_command stop; then
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
        if ! service_command start; then
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
    [[ -f $stage/$name && ! -L $stage/$name \
        && $(stat -c '%u:%h:%a' -- "$stage/$name") =~ ^0:1:(600|644|700|755)$ ]] \
        || fail "$name must be a root-owned regular staging file."
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
service_command stop
for name in "${names[@]}"; do
    mv -f -- "$transaction/$name" "/usr/bin/$name"
done
sync
service_command start

# A live control socket verifies application startup, not just systemd's PID.
# Require three consecutive responses from the same process; a crash loop is
# not a healthy deployment. Bound each call as well as the overall poll count.
healthy=0
previous_pid=
for ((attempt = 0; attempt < 20; attempt++)); do
    sleep 1
    pid=$(service_command show --property=MainPID --value)
    if [[ $pid != 0 ]] && service_command is-active --quiet \
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
