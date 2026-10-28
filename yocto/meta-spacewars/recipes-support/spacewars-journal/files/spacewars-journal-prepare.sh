#!/bin/sh
# Run only after the real data filesystem and volatile /var/log are mounted.
set -eu

mountpoint -q /data || { echo "Journal persistence requires mounted /data" >&2; exit 1; }

machine_id=$(cat /etc/machine-id)
case "$machine_id" in
    ''|*[!0-9a-f]*) echo "Invalid journal machine ID" >&2; exit 1 ;;
esac
[ "${#machine_id}" -eq 32 ] || { echo "Invalid journal machine ID" >&2; exit 1; }
[ "$machine_id" != 00000000000000000000000000000000 ] || exit 1

for directory in /data/spacewars-journal /var/log/journal "/var/log/journal/$machine_id"; do
    if [ -L "$directory" ] || { [ -e "$directory" ] && [ ! -d "$directory" ]; }; then
        echo "Invalid journal directory: $directory" >&2
        exit 1
    fi
    if [ -d "$directory" ] && [ "$(stat -c %u "$directory")" != 0 ]; then
        echo "Journal directory must be root-owned: $directory" >&2
        exit 1
    fi
    install -d -m 0750 -o root -g root "$directory"
done
