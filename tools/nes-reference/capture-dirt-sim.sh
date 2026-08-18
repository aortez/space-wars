#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 DIRTSIM_CHECKOUT FALLING_ROM" >&2
    exit 2
fi

dirtsim_checkout=$(realpath "$1")
falling_rom=$(realpath "$2")
tool_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
build_dir=$(mktemp -d)
trap 'rm -rf -- "$build_dir"' EXIT

expected_commit=0db5f847e7c059b807eb982702ba26fe9f004bf9
actual_commit=$(git -C "$dirtsim_checkout" rev-parse HEAD)
if [[ "$actual_commit" != "$expected_commit" ]]; then
    echo "DirtSim must be checked out at $expected_commit (found $actual_commit)" >&2
    exit 1
fi
if ! git -C "$dirtsim_checkout" diff --quiet \
    || ! git -C "$dirtsim_checkout" diff --cached --quiet; then
    echo "DirtSim checkout has tracked changes; use a clean reference tree" >&2
    exit 1
fi

compiler=${CC:-cc}
read -r -a sdl_cflags <<<"$(pkg-config --cflags sdl2)"
read -r -a sdl_libs <<<"$(pkg-config --libs sdl2)"

"$compiler" -std=c11 -D_POSIX_C_SOURCE=200809L -Wall -Wextra -Werror \
    -I"$dirtsim_checkout/apps/src" \
    -I"$dirtsim_checkout/apps/external" \
    "${sdl_cflags[@]}" \
    "$tool_dir/capture_dirt_sim.c" \
    -c -o "$build_dir/capture_dirt_sim.o"

"$compiler" -std=gnu11 -O3 -DNDEBUG \
    -I"$dirtsim_checkout/apps/src" \
    -I"$dirtsim_checkout/apps/external" \
    "${sdl_cflags[@]}" \
    "$build_dir/capture_dirt_sim.o" \
    "$dirtsim_checkout/apps/src/core/scenarios/nes/SmolnesRuntimeBackend.c" \
    "$dirtsim_checkout/apps/src/core/scenarios/nes/SmolnesApu.c" \
    -o "$build_dir/capture_dirt_sim" \
    "${sdl_libs[@]}" -lpthread -lm

echo "dirtsim_commit=$actual_commit"
echo "falling_sha256=$(sha256sum "$falling_rom" | cut -d' ' -f1)"
echo "compiler=$($compiler --version | head -1)"
uname -a
"$build_dir/capture_dirt_sim" "$falling_rom"
