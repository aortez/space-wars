# Frozen text in RAM and display memory

This follows the profiler-overhead checkpoint `632bc6e` and the
[text drawing investigation](text-drawing-profile.md). With identical glyphs,
mapped display memory accounts for about **3 ms** of extra text drawing on
`sw-picade.local`. Skipping unnecessary destination reads for transparent and
opaque pixels saves **1.36–1.40 ms per complete draw** in the controlled probe.
The LinuxKMS XRGB/ARGB pixel implementation now uses those shortcuts. Partially
transparent pixels retain Slint's existing integer blend calculation.

## Matched workload

`--benchmark-presentation --presentation-memory` freezes the seed-42 material
match after 120 simulation ticks, rasterizes its geometry at 1×, and retains
the full UI's text model. Every variant uses the same window, image, 336 glyph
texture operations, XRGB layout, output dimensions and row stride. Simulation,
geometry preparation, allocation and comparison are outside the timers.

Without a device argument it runs entirely in RAM. With
`--presentation-drm-device /dev/dri/card0`, it opens that explicit device and
creates a private XRGB8888 DRM dumb buffer using the same allocation/mapping API
as the kiosk. It never creates a framebuffer, requests DRM master, changes a
display mode or presents the buffer. Mapping and allocation release on exit.
The driver on this Pi is `vc4-drm`; the running kiosk also uses `card0`.

Each variant gets ten warmups and 40 measured draws, in three blocks with
alternating forward/reverse order. Runs without detailed hooks bracket a run
with hooks. All outputs are compared exactly against the original blend path
after every block, including the X byte, row padding and unused mapping tail.
The two blend variants differ only in handling alpha 0 and 255. A staged
variant renders the whole frame in RAM and includes the full copy to its output
in `total_ms`; it does not pretend a buffer copy is free.

The diagnostic executable ran alongside the installed kiosk, which was paused
and temporarily suspended under a resume trap to exclude concurrent rendering
and simulation. Its normal match resumed afterward. Settings were unchanged.
The 18 thermal/frequency samples were **69.6–71.6°C and 1.5 GHz**.

## Pi results, 1024×768

Means of three blocks, without detailed observer hooks:

| Destination / blend | First run, total | Repeated run, total |
| --- | ---: | ---: |
| RAM / reference | 9.62 ms | 9.56 ms |
| RAM / shortcuts | 9.35 ms | 9.32 ms |
| DRM / reference | 12.59 ms | 12.57 ms |
| DRM / shortcuts | 11.24 ms | 11.17 ms |
| RAM + copy to DRM / reference | 10.99 ms | 10.98 ms |
| RAM + copy to DRM / shortcuts | 10.74 ms | 10.70 ms |

The middle, detailed run shows where the destination-memory gap occurs:

| Stage | RAM reference | DRM reference | DRM shortcuts |
| --- | ---: | ---: | ---: |
| Complete draw | 9.69 ms | 12.67 ms | 11.35 ms |
| All text | 2.43 ms | 5.42 ms | 4.18 ms |
| Glyph lookup, placement and painting | 0.99 ms | 4.00 ms | 2.75 ms |
| Nested glyph fallback | 0.77 ms | 3.79 ms | 2.53 ms |

These are the **same glyphs**, unlike the previous comparison between a live
match and a different frozen HUD. Nearly all of the 2.98 ms whole-draw gap
appears inside glyph drawing. The shortcuts remove about one-third of mapped
glyph fallback time. They preserve premultiplied source-over semantics: zero
alpha leaves the destination untouched; full alpha replaces it without first
reading it. This establishes a destination-memory cost, not a particular
kernel cache-policy implementation.

Desktop RAM results also preserve pixels: full draw is 1.23 ms with reference
blending and 1.19 ms with shortcuts. That smaller difference reinforces why
desktop-only measurements would understate the Pi benefit.

## Why staging remains an experiment

A full 1024×768 XRGB copy costs **1.45–1.47 ms** and needs another 3 MiB buffer.
It still helps the generic renderer in this frozen workload, with the shortcut
version about 0.5 ms faster than shortcut drawing directly into DRM.

The headless probe uses Slint's generic texture drawing for the frozen geometry
image. The kiosk already has an accelerated opaque RGB image path and background
fill. Those paths are common to the two memory variants in this experiment,
but their production implementation differs. Therefore the *text* memory
comparison is controlled; the full-frame numbers do not select the best live
buffer strategy. Keep the existing direct-buffer default. Before revisiting
`SPACEWARS_KMS_BUFFER=ram`, repeat a frozen comparison through the actual
LinuxKMS image fast path, include its transfer cost and exercise launcher/clock
workloads. Small HUD or glyph-region staging is another bounded alternative.

The probe's `Xrgb` reference deliberately retains the old blend calculation so
future comparisons do not silently optimize their own oracle. It is not the
production pixel implementation.

## Gameplay evidence and remaining stalls

Before deploying the shortcut change, ten live samples span 48.28 seconds of
the same automatic match and give **9.55 FPS / 47.74 updates/s** from counters.
Control/physics averages **82.94 ms per displayed frame** (multiple simulation
updates can run in one callback), while scene construction averages 0.98 ms,
host preparation 5.41 ms, KMS rendering 15.43 ms and text 6.41 ms.

`live-before.png` shows P1 settling on rear feet and P2 choosing sheltered ground
with about three minutes left. The saved settings have launch seed 0. Raw status
and settings are preserved, but this is not a serialized simulation snapshot.
Do not attribute the stall to a specific AI/physics function without a trace.
Use this evidence with the [sensor attribution notes](on-foot-survey-fix.md) to
reopen that investigation. Rendering savings alone will not remove these stalls.

Fast deployment installed client
`9ff2c74cb2f6df3a0a1878e461b96ea95597dc00a21ece196600e0b146633d4b`;
the installed and running executable hashes match. The service is active with
zero crash restarts. Automatic two-bot Spacewars resumed at 1×, 1024×768,
direct buffers and the existing RGB image fast path. Saved settings remained
byte-for-byte unchanged. No reboot was needed.

The ten post-deployment samples span 48.12 seconds of a fresh match and give
**42.16 FPS / 59.31 updates/s**. Rolling FPS ranges from 37.7 to 51.6. KMS rendering
averages **13.27 ms**, text **5.04 ms**, and glyph work **3.31 ms**, including
**3.00 ms** of fallback. About 390 glyph operations run per draw, versus 388 in
the pre-deployment capture. The roughly 1.4 ms glyph reduction agrees with the
controlled experiment.

The total FPS change is **not** a matched before/after comparison: deployment
started a new match, and control/physics in this sample averages 2.87 ms rather
than the old match's 82.94 ms. It does not show that the simulation stall is
fixed. `live-after.png` is a later visual check, outside this sampling window.

## Validation and reproduction

- Six presentation integration tests pass. The new test checks pixels and
  padding at two sizes and all four rotations; it also verifies that settings
  are untouched and regular files cannot be used as DRM devices.
- The prototype and production blend tests each compare 196,608 combinations,
  covering every source alpha and destination byte with valid premultiplied
  colors and varying destination alpha.
- Twenty-one standalone LinuxKMS tests pass, including the existing format,
  clipping, rotation, alpha, resize, partial damage and image-fast-path checks.
  The explicit timing benchmark remains ignored.
- All 102 Pi comparison blocks pass, including real DRM mappings at all four
  rotations and a second output size. No settings changes or kiosk crash
  restarts occurred during the private-buffer experiment.

```sh
cargo +1.89.0 test --locked --release -p engine-client --test presentation_probe
cargo +1.89.0 test --locked --release -p engine-client --bin engine-client \
  presentation_probe::memory::tests
cargo +1.89.0 test --locked --manifest-path vendor/i-slint-backend-linuxkms/Cargo.toml \
  --features renderer-software --lib
SLINT_BACKEND=nonexistent-backend ./engine-client \
  --benchmark-presentation --presentation-memory \
  --presentation-drm-device /dev/dri/card0 \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 40 --presentation-repeats 3 --presentation-detail
```

Omit `--presentation-drm-device` for the RAM-only variant. Omit
`--presentation-detail` to remove observer clock overhead. Use
`--presentation-rotation 90` (also 0, 180, 270) to check transposed rendering.
Backend test linker prerequisites are described in the previous text report.

Exact probe client SHA-256:
`3aa4feb9473b23fcc58ec2c18b5c4e402d93f3ea575e61ad0b23fed548347481`.
Scripts, raw results, settings, source patches and binaries:
`/home/oldman/.codex/visualizations/2026/09/12/rounder-planets/text-memory/`.
