#!/bin/sh
# BlueZ owns pairing keys; never put them in the app-readable settings tree.
set -eu
umask 077

mountpoint -q /data || { echo "Bluetooth persistence requires mounted /data" >&2; exit 1; }
source=/var/lib/bluetooth
destination=/data/bluetooth
if [ -L "$source" ] || [ -L "$destination" ]; then
    echo "Refusing symlinked Bluetooth state directories" >&2
    exit 1
fi

for state in /data/spacewars-bootstrap /data/spacewars-system; do
    if [ -L "$state" ] || { [ -e "$state" ] && [ ! -d "$state" ]; }; then
        echo "Invalid persistent migration state" >&2
        exit 1
    fi
done

if [ ! -e "$destination" ]; then
    for record in /data/spacewars-bootstrap/active /data/spacewars-system/recovery; do
        if [ -e "$record" ] || [ -L "$record" ]; then
            echo "Missing Bluetooth store during migration or after provisioning" >&2
            exit 1
        fi
    done
    install -d -m 0700 "$source"
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
install -d -m 0700 "$source"
