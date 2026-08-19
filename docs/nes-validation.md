# Rust NES milestone validation

This document records the M22h validation performed on 2026-08-18. The tested
change set is based on M22g commit `0cac1eb`. Build and test commands use the
repository's Rust 1.89 MSRV; local Clippy used 1.92, while CI installs the Rust
1.89 Clippy component.

## Reproducible benchmark

`falling-benchmark` embeds the same pinned Falling ROM as the playable
scenario. It runs one full video/audio machine and one output-disabled machine,
then rejects any difference in scheduler work or authoritative state. Each
version-2 NDJSON row records the ROM identity, host OS/architecture, warmup and
measured frame counts, core and wall throughput, frame-time distribution,
output hashes, and final state hash.

Run the desktop artifact with:

```sh
cargo run --locked --release -p scenario-falling \
  --bin falling-benchmark -- 2000 120
```

The Yocto image installs the identical workload as:

```sh
falling-benchmark 2000 120
```

Core time measures `NesMachine::run_frame_with_input`; wall time additionally
includes benchmark-only video/audio checksum validation. Neither number
includes pacing, sleeping, a window, or an audio device.

## Results

The desktop was an AMD Ryzen 7 9800X3D running x86-64 Linux. The target was a
Raspberry Pi 5 Model B Rev 1.1 running the project's Poky 5.0.18 AArch64 image.
The Pi rows came from the installed `/usr/bin/falling-benchmark` while the kiosk
process was suspended and automatically resumed around each run.

| Target and mode | Frames/s | Core realtime | Wall realtime | p99 frame |
| --- | ---: | ---: | ---: | ---: |
| Desktop, full video/audio | 1,018.3 | 16.944x | 16.922x | 1.056 ms |
| Desktop, headless | 1,168.8 | 19.448x | 19.447x | 0.935 ms |
| Pi 5, full video/audio, 3-run range | 246.7-247.4 | 4.104-4.117x | 4.101-4.113x | 4.070-4.138 ms |
| Pi 5, headless, 3-run range | 273.0-277.0 | 4.542-4.609x | 4.542-4.609x | 3.632-3.698 ms |

All rows produced these deterministic identities:

```text
ROM FNV-1a 64:    16a4d7eebe1afc30
video FNV-1a 64:  4b44a595f473f325
audio FNV-1a 64:  bb9fc7caa1f14dae
state v2 FNV-1a:  87b71f4c3787d446
```

The initial ordinary release build measured about 3.72x realtime on the same
Pi. Fat LTO with one code-generation unit raised the installed full-machine
wall result to about 4.11x, a roughly 10.6% gain. An explicit Cortex-A76 target
was also measured and was approximately 3% slower, so it was rejected. The
accepted profile trades longer release links for runtime headroom without
adding target-specific code or changing machine state.

The installed benchmark SHA-256 was
`d231ce5a02fa4e6982b65a96cbf5e441bd543819b86a9a09a9d62bfc916bba5b`.
The A/B image build completed all 6,608 Yocto tasks, booted from `/dev/sda2`,
and returned `spacewars-kiosk.service` active with zero restarts after the
benchmark run.

## Pinned DirtSim comparison

The reference capture was rebuilt from DirtSim commit
`0db5f847e7c059b807eb982702ba26fe9f004bf9` on the same desktop, using the same
ROM, 100 warmup frames, 1,000 neutral-input frames, and the checked-in
`capture-dirt-sim.sh` tool. Its full mode measured 2,706.6 frames/s, palette
without APU measured 2,711.9 frames/s, and headless without APU measured
2,677.7 frames/s. The corresponding Rust run measured 1,019.6 frames/s with
video/audio and 1,172.3 frames/s headless.

This is an optimization comparison, not an assertion of equivalent work.
DirtSim and the Rust core have different APU, scheduler, framebuffer, and
hardware-accuracy models; the Rust full benchmark also leaves host RGB
conversion outside core timing. Exact visible Falling frames and RAM are
compared separately in `crates/engine-nes/REFERENCE.md`. The result establishes
that the safe scalar Rust implementation has ample target headroom while still
leaving a measured 2.3-2.7x throughput gap to investigate where workloads can
be made comparable.

## Build and lifecycle coverage

The local validation set passed:

```sh
cargo fmt --all -- --check
cargo test --locked --workspace --all-targets
cargo clippy --locked -p engine-nes -p scenario-falling \
  --all-targets --no-deps -- -D warnings
cargo check --locked -p engine-nes -p scenario-falling \
  --target aarch64-unknown-linux-gnu
cargo check --locked -p engine-client -p engine-nes -p scenario-falling \
  --target x86_64-pc-windows-gnu --all-targets
```

The new CI workflow repeats the formatter, core/golden/state tests, clippy,
bundled-ROM release benchmark, Linux workspace tests, native Windows checks,
and AArch64 core/scenario check without downloading a ROM or DirtSim.

Client tests cover the three-slot newest-frame handoff, one pending UI wakeup,
latest-state input, bounded catch-up, audio ring overflow/underrun/flush,
pause/restart/launcher/relaunch ownership, and a repeated 12-cycle worker/audio
start-stop soak. Desktop manual play previously confirmed native presentation,
controller input, 48 kHz audio, and live bounded-queue telemetry.

The remaining physical-device sign-off is a repeated Falling launcher/play/
pause/restart/launcher/relaunch soak on the Pi with its real gamepad and audio
output. This is deliberately recorded as a manual check rather than inferred
from the unpaced benchmark.
