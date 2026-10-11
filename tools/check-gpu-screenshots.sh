#!/bin/bash
# Run on a kiosk using LinuxKMS/FemtoVG, with the launcher open and auto-start
# disabled. Exercises real scanout buffers, not an offscreen rendering fixture.
# Does not change saved settings. On success leaves the launcher open; restores the
# previous session-only glyph policy. Captures/logs remain in a new /tmp folder.
# Set SPACEWARS_EXPECT_SCREENSHOT_SIZE=800x480 on a rotated HyperPixel to check
# logical capture dimensions too. Repeat after enabling/disabling GPU scenes.
set -euo pipefail
cli=spacewars-cli
status=$("$cli" status)
state=$("$cli" ui state)
grep -qx 'kms_renderer=femtovg' <<< "$status" || { echo 'Requires LinuxKMS/FemtoVG.' >&2; exit 1; }
grep -qx 'autostart_enabled=false' <<< "$status" || { echo 'Turn auto-start Off for this test; restore it afterward.' >&2; exit 1; }
grep -qx 'screen=launcher.main' <<< "$state" || { echo 'Open the launcher before this test.' >&2; exit 1; }
previous_policy=$(sed -n 's/^gpu_text_requested=//p' <<< "$status")
case "$previous_policy" in vector|cpu|cpu-run-faces|cpu-shape-faces) ;; *) echo 'Unknown text policy.' >&2; exit 1;; esac
trap '"$cli" host gpu-text "$previous_policy" >/dev/null' EXIT
capture_dir=$(mktemp -d /tmp/spacewars-freshness.XXXXXXXX)
echo "artifacts=$capture_dir"
expected_size=${SPACEWARS_EXPECT_SCREENSHOT_SIZE:-}
[[ -z $expected_size || $expected_size =~ ^[1-9][0-9]*x[1-9][0-9]*$ ]] || {
    echo 'Invalid SPACEWARS_EXPECT_SCREENSHOT_SIZE (expected WIDTHxHEIGHT)' >&2
    exit 1
}

check_size() {
    [[ -n $expected_size ]] || return 0
    local w0 w1 w2 w3 h0 h1 h2 h3 width height
    # PNG IHDR dimensions are eight big-endian bytes at offset 16. No image
    # tooling dependency is needed on the minimal kiosk image.
    read -r w0 w1 w2 w3 h0 h1 h2 h3 < <(od -An -v -t u1 -j 16 -N 8 "$1")
    width=$((w0 * 16777216 + w1 * 65536 + w2 * 256 + w3))
    height=$((h0 * 16777216 + h1 * 65536 + h2 * 256 + h3))
    [[ ${width}x${height} == "$expected_size" ]] || {
        echo "FAIL: capture is ${width}x${height}, expected $expected_size" >&2
        exit 1
    }
}

capture_three() {
    local name=$1
    for n in 1 2 3; do
        "$cli" screenshot "$capture_dir/$name-$n.png"
        check_size "$capture_dir/$name-$n.png"
    done
    cmp "$capture_dir/$name-1.png" "$capture_dir/$name-2.png"
    cmp "$capture_dir/$name-1.png" "$capture_dir/$name-3.png"
    sha256sum "$capture_dir/$name-1.png"
}

for policy in vector cpu-shape-faces; do
    "$cli" host gpu-text "$policy"
    for cycle in 1 2 3 4 5; do
        "$cli" ui activate launcher.sound >/dev/null
        capture_three "$policy-$cycle-settings"
        "$cli" ui activate sound.back >/dev/null
        capture_three "$policy-$cycle-launcher"
        if cmp -s "$capture_dir/$policy-$cycle-settings-1.png" "$capture_dir/$policy-$cycle-launcher-1.png"; then
            echo 'FAIL: two different screens produced the same image' >&2
            exit 1
        fi
    done
done

if "$cli" screenshot /proc/spacewars-ui-unwritable.png > "$capture_dir/write-error.txt" 2>&1; then
    echo 'FAIL: invalid output path was accepted' >&2
    exit 1
fi
capture_three after-write-error
pids=()
for n in 1 2 3 4; do
    "$cli" screenshot "$capture_dir/concurrent-$n.png" > "$capture_dir/concurrent-$n.txt" 2>&1 &
    pids+=("$!")
done
success=0
busy=0
for index in 0 1 2 3; do
    n=$((index + 1))
    if wait "${pids[$index]}"; then
        success=$((success + 1))
    elif grep -q 'a GPU screenshot request is already pending' "$capture_dir/concurrent-$n.txt"; then
        busy=$((busy + 1))
    else
        cat "$capture_dir/concurrent-$n.txt"
        exit 1
    fi
done
test "$success" -gt 0
echo "concurrent_success=$success concurrent_busy=$busy"
capture_three after-concurrent
echo 'PASS: immediate menu screenshots, error recovery and bounded concurrent requests'
