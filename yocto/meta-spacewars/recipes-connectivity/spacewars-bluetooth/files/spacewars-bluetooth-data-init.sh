#!/bin/sh
# BlueZ owns pairing keys; never put them in the app-readable settings tree.
set -eu
umask 077

mountpoint -q /data || { echo "Bluetooth persistence requires mounted /data" >&2; exit 1; }
source=/var/lib/bluetooth
destination=/data/bluetooth
[ ! -L "$source" ] && [ ! -L "$destination" ] || {
    echo "Refusing symlinked Bluetooth state directories" >&2
    exit 1
}
install -d -m 0700 "$source"

if [ ! -e "$destination" ]; then
    # Publish the complete migration atomically. A failed/interrupted copy must
    # not become the authoritative persistent state on the next boot.
    migration=$(mktemp -d /data/.spacewars-bluetooth.XXXXXXXX)
    trap 'rm -rf -- "$migration"' EXIT
    trap 'exit 1' HUP INT TERM
    cp -a "$source/." "$migration/"
    chmod -R go-rwx "$migration"
    chown root:root "$migration"
    sync -f "$migration"
    mv -T "$migration" "$destination"
    sync -f /data
    trap - EXIT HUP INT TERM
fi

# Existing persistent state always wins over an old A/B slot's stale keys.
[ -d "$destination" ] || { echo "Invalid Bluetooth data directory" >&2; exit 1; }
chmod 0700 "$destination"
chown root:root "$destination"
