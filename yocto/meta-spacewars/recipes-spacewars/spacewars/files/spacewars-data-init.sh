#!/bin/sh
# Initialize persistent Space-Wars data.

set -eu

DATA_DIR="/data/spacewars"
CONFIG_DIR="$DATA_DIR/config"
COREDUMP_DIR="$DATA_DIR/coredumps"
VAR_LIB_LINK="/var/lib/spacewars"

mkdir -p "$CONFIG_DIR" "$COREDUMP_DIR"
chown -R spacewars:spacewars "$DATA_DIR"
chmod 755 "$DATA_DIR" "$CONFIG_DIR" "$COREDUMP_DIR"

mkdir -p /var/lib
if [ -e "$VAR_LIB_LINK" ] && [ ! -L "$VAR_LIB_LINK" ]; then
    rm -rf "$VAR_LIB_LINK"
fi
if [ ! -L "$VAR_LIB_LINK" ]; then
    ln -s "$CONFIG_DIR" "$VAR_LIB_LINK"
fi

echo "Space-Wars persistent data initialized at $DATA_DIR"
