#!/bin/bash
# Owner-only native test launcher. Caller supplies private bind mounts and a
# fresh root-owned directory; production NetworkManager remains running.
set -euo pipefail
umask 077
directory=$1
binary=$2
test "$EUID" = 0
[[ "$directory" =~ ^/data/spacewars-nm-qualification-[a-z0-9-]+/(baseline|candidate)$ ]]
test "$(realpath "$directory")" = "$directory"
test "$(stat -c '%u:%a' "$directory")" = '0:700'
# rfkill is global, even in a private network namespace. These checks must run
# before dbus-daemon or NetworkManager can start, not inside the later Rust test.
test ! -e /dev/rfkill
test ! -L /dev/rfkill
for namespace in net mnt; do
    current=$(readlink "/proc/self/ns/$namespace")
    host=$(readlink "/proc/1/ns/$namespace")
    test -n "$current"
    test -n "$host"
    test "$current" != "$host"
done
for pair in 'etc /etc/NetworkManager' 'run /run/NetworkManager' 'state /var/lib/NetworkManager' \
    'bus-denied /run/dbus' 'udev-denied /run/udev'; do
    read -r local bound <<< "$pair"
    test -d "$directory/$local"
    test ! -L "$directory/$local"
    test "$(stat -c '%u:%a' "$directory/$local")" = '0:700'
    test "$(stat -c '%d:%i' "$directory/$local")" = "$(stat -c '%d:%i' "$bound")"
done
for blocked in /run/dbus /run/udev; do
    test -z "$(find "$blocked" -mindepth 1 -print -quit)"
done
test "$(stat -c '%u:%a' "$binary")" = '0:500'
test ! -e "$directory/bus"
export DBUS_SYSTEM_BUS_ADDRESS="unix:path=$directory/bus"
export SPACEWARS_NM_QUALIFICATION_ROOT="$directory"
bus_pid=
nm_pid=
trace_pid=
cleanup() {
    result=$?
    trap - EXIT
    for process in "$trace_pid" "$nm_pid" "$bus_pid"; do
        if test -n "$process"; then kill "$process" 2>/dev/null || true; fi
    done
    wait || true
    printf '%s\n' "$result" > "$directory/exit-code"
    exit "$result"
}
trap cleanup EXIT
dbus-daemon --session --nofork --nopidfile --address="$DBUS_SYSTEM_BUS_ADDRESS" > "$directory/bus.log" 2>&1 &
bus_pid=$!
for _attempt in {1..50}; do
    test -S "$directory/bus" && break
    sleep 0.1
done
test -S "$directory/bus"
cat > "$directory/etc/NetworkManager.conf" <<EOF
[main]
plugins=keyfile
auth-polkit=false
dns=none
rc-manager=unmanaged
no-auto-default=*
[keyfile]
path=$directory/profiles
unmanaged-devices=interface-name:*
[logging]
level=TRACE
domains=SETTINGS,AGENTS
backend=debug
EOF
"$binary" --no-daemon --config="$directory/etc/NetworkManager.conf" \
    --config-dir="$directory/etc/conf.d" --system-config-dir="$directory/etc/conf.d" \
    --state-file="$directory/state/NetworkManager.state" --pid-file="$directory/run/NetworkManager.pid" \
    > "$directory/manager.log" 2>&1 &
nm_pid=$!
for _attempt in {1..50}; do
    kill -0 "$nm_pid"
    if busctl --address="$DBUS_SYSTEM_BUS_ADDRESS" get-property org.freedesktop.NetworkManager \
        /org/freedesktop/NetworkManager org.freedesktop.NetworkManager Startup | grep -q 'false'; then break; fi
    sleep 0.1
done
busctl --address="$DBUS_SYSTEM_BUS_ADDRESS" get-property org.freedesktop.NetworkManager \
    /org/freedesktop/NetworkManager org.freedesktop.NetworkManager Startup | grep -q 'false'
dbus-monitor --address "$DBUS_SYSTEM_BUS_ADDRESS" \
    "type='signal',sender='org.freedesktop.NetworkManager',path_namespace='/org/freedesktop/NetworkManager/Settings'" \
    > "$directory/signals.log" 2> "$directory/signals.err" &
trace_pid=$!
"${directory%/*}/nm-profile-refresh" --ignored --exact \
    real_daemon_preserves_only_identical_internal_secret_refreshes --nocapture \
    > "$directory/tests.log" 2>&1
