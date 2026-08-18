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

"$compiler" -std=c11 -Wall -Wextra -Werror \
    -I"$dirtsim_checkout/apps/src" \
    -I"$dirtsim_checkout/apps/external" \
    "${sdl_cflags[@]}" \
    "$tool_dir/trace_dirt_sim_startup.c" \
    -c -o "$build_dir/trace_dirt_sim_startup.o"

"$compiler" -std=gnu11 -O0 -g \
    -I"$dirtsim_checkout/apps/src" \
    -I"$dirtsim_checkout/apps/external" \
    "${sdl_cflags[@]}" \
    "$build_dir/trace_dirt_sim_startup.o" \
    "$dirtsim_checkout/apps/src/core/scenarios/nes/SmolnesRuntimeBackend.c" \
    "$dirtsim_checkout/apps/src/core/scenarios/nes/SmolnesApu.c" \
    -o "$build_dir/trace_dirt_sim_startup" \
    "${sdl_libs[@]}" -lpthread -lm

gdb -q -batch \
    -x "$tool_dir/startup_trace.gdb" \
    --args "$build_dir/trace_dirt_sim_startup" "$falling_rom"
