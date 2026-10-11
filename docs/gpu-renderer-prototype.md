# Raspberry Pi GPU renderer

## GPU-default milestone

The investigation has a defined endpoint: ship one supported GPU profile on
the Pi 4 Picade and Pi 5 HyperPixel, keep software recovery, and move further
performance experiments into follow-up work. GPU rendering is now the normal
Pi image/build choice, not a collection of commands needed after each launch.

The supported configuration is:

| Layer | Default |
| --- | --- |
| Pi output | LinuxKMS / FemtoVG / Mesa V3D |
| 1× geometry scenes | Cached ordered triangles (`retained-culled`) |
| Geometry preparation | Prepared transforms and bounded scratch/source pools |
| Upload buffers | Bounded per-pass bucketed buffers |
| Outlines | Conservative offscreen rejection, exact joins, `bulk-static` writer |
| UI text | CPU-shaped glyph masks, cached and composed by the GPU |
| Raster fallback | Aligned RGBA publication, ordinary Slint texture lifetime |
| NES / Falling | Existing native-video presentation, with GPU UI/output |

Scenarios still emit renderer-independent scene descriptions. Direct geometry
bypasses CPU scene rasterization and per-object Slint items. CPU simulation,
scene construction and geometry encoding still exist; this is not a claim that
all rendering work now runs on the GPU. Explicit vector and non-1× raster
preferences remain respected instead of being silently rewritten.

The profile is selected once at scenario start. CLI overrides last for that
session; a new scenario restores the supported profile. A geometry failure
logs an error and falls back to raster for the session, without retrying every
frame. Backend initialization errors remain explicit startup failures.

### Build, update and recovery

`npm --prefix yocto run build` and ordinary `./update.sh` now select the GPU
image and the existing `-gpu` build directory. `--gpu` remains a build alias.
Build/download caches are reused, not copied to another directory. Devices
with an older software-only image need one full image update; the fast updater
continues rejecting incompatible runtime/service fingerprints.

`SLINT_BACKEND=linuxkms` (or unset) selects GPU in a `pi-kiosk-gpu` client and
software in an explicit `pi-kiosk` build. Use
`SLINT_BACKEND=linuxkms-software` for software recovery/comparison. On the GPU
image, `sudo /usr/sbin/spacewars-renderer software` selects software temporarily;
`sudo /usr/sbin/spacewars-renderer gpu` returns to GPU. The helper health-checks
each switch and rolls back failed trials. A reboot clears its temporary
override. For persistent software recovery, set the explicit backend in
`/boot/spacewars-kiosk.env`.

An OS image without GPU libraries remains available through
`npm --prefix yocto run build -- --software`, or `SPACEWARS_GPU=0` for build,
flash and update commands. Its artifacts use the separate unsuffixed directory.
Do not mix software/GPU `KAS_BUILD_DIR` overrides or bypass compatibility checks.

### Finish line and follow-ups

Release checks are default-selection/session tests, software recovery, Pi
builds, real-device Clock/Spacewars/Pizza/native-video/menu transitions, fresh
screenshots and bounded resource use on both displays. These checks validate
the default route without manually enabling the renderer after startup.

New stroke variants, GPU-resident mesh experiments, isolated GPU execution
timers and eliminating every long frame are follow-ups, not acceptance gates.
Pi 4 zoom still has measured host/timer and geometry-encoding spikes. Physical
HyperPixel touch-corner validation requires a person at the device. The software
reference and diagnostic controls remain available; they are not promised as
individually supported production configurations.

### Milestone validation (2026-10-10)

The frozen release candidate passed the same finite, private-settings smoke
suite on Pi 4 Picade (1024×768) and Pi 5 HyperPixel (800×480, rotated output).
The tests launch with `SLINT_BACKEND` unset, not a manually selected GPU mode:

- Clock, Spacewars and Pizza automatically use the supported geometry profile;
  Falling keeps native video. Explicit software and the generic `linuxkms`
  alias also pass.
- All nine Clock events render without reported geometry, text or upload
  errors. Spacewars passes a live zoom sweep; screenshots cover gameplay,
  pause, launcher and settings on both displays.
- Restart and launcher/relaunch restore the supported profile after a
  deliberate session-only GPU opt-out. Reported scratch/source-pool retention
  stays within its configured limits. This is a smoke check, not a long soak
  or a measurement of total GPU memory.
- The private tests leave installed binaries and user settings unchanged.
  Screenshots were inspected for scene composition, minimaps, text, native
  video and rotated dimensions.

Local checks passed: 725 GPU client tests, 32 CLI tests, four startup tests,
664 desktop client tests, five software-only policy tests, 20 FemtoVG tests,
three Slint GPU-renderer tests, 209 tessellator tests and 46 LinuxKMS tests.
Manual audits remain ignored. Yocto tooling passed 37 Node and 44 Python tests;
the full normal GPU image built successfully. CI also runs the GPU startup
tests, rather than relying only on the desktop-feature test build.
The GPU client also checks successfully with the CI/MSRV Rust 1.89.0 toolchain.

The local evidence bundle is `target/gpu-rollout.NR7rqcNP/`, including the
private runner/fixture, logs and 36 smoke screenshots per device. The candidate
client SHA-256 is
`f7b260be33366b20a6b935f464eec64bda0fba5e4d3991d6c2a25460cf914ed4`.
These are acceptance checks of the selected defaults; no additional FPS or CPU
improvement is inferred from them.

The normal A/B image update subsequently passed on `sw-picade-2.local` and
`spacewars.local`. Both rebooted into the candidate binary with
`SLINT_BACKEND=linuxkms`, no temporary renderer override, unchanged settings
hashes, zero service restarts and the supported geometry profile active in
Clock. The Pi 5 also passed `tools/check-gpu-screenshots.sh` (immediate menu
captures, write-error recovery and bounded concurrent requests). One earlier
immediate settings screenshot had a displaced heading; it did not recur in
the post-update transition checks. Keep that observation as a follow-up if it
reappears, rather than claiming that screenshots prove every UI frame correct.

### Current-main integration acceptance

The rollout then merged `main` through `1cbc4ed` and repeated acceptance at
`da72409`, rather than treating the earlier candidate's results as evidence for
the newly integrated Clock and terrain changes. The full optimized desktop
workspace/all-target run passed 2,940 tests (61 explicitly ignored). GPU tests
passed 747 client, 32 CLI, four startup, 20 FemtoVG and three Slint renderer
tests; the Rust 1.89.0 GPU check also passed. Yocto tooling passed 38 Node and
44 Python tests, including shader-only rebuild coverage.

The merged Pi release build completed all 4,185 requested tasks. Normal fast
updates passed on both already GPU-capable devices. Each device then passed
eight private-settings cases: Clock, Spacewars, Pizza, Falling, Scorched Earth,
Terrain Grains, explicit software, and the generic `linuxkms` alias. Coverage
includes all nine Clock events, a Spacewars zoom sweep, deliberate GPU opt-out
followed by restart, launcher/settings/relaunch transitions, pool limits and
44 dimension-checked screenshots per device. Representative captures were
visually inspected. These remain finite presentation checks, not long-running
terrain/gameplay tests or physical input validation.

Both installed services recovered with zero automatic restarts; the private
acceptance runs preserved installed binaries and settings byte-for-byte. The
merged Pi 5 also passed the immediate-menu screenshot, write-error recovery
and bounded concurrent-request checks. The new evidence bundle is
`target/gpu-integration.MuJ7NIkE/`. The tested client
SHA-256 is
`eaf26583ca2b340dc5ce5a58ab0e172b98dbbe6aaa3c9e40d19c9225525210db`.
See [rollout PR #188](https://github.com/aortez/space-wars/pull/188) for CI status.

## Historical investigation

The chronological notes below retain the policy and defaults used for each
experiment. Statements such as “opt-in” and “defaults unchanged” describe
those measurements, not the GPU-default milestone above. Percentages from
different experiments must not be added together.

This opt-in experiment compares Slint's direct-KMS FemtoVG renderer with our
software renderer, at native 1× resolution. It preserves scenario inputs and
software fallback. Hardware-accelerated output and explicit software fallback
now work on the Pi 4 Picade. Clock measurements show **higher CPU cost with the
current RGB upload path**. An opt-in live RGBA path cuts upload CPU cost and
restores heavy rain from roughly 53 to 60 FPS. After aligning the conversion
loop's destination, that path uses about **7% less application CPU than software**
in the tested idle/rain Clock fixtures. Reusing one texture or rotating three
removes allocation churn but does not meaningfully improve live CPU use beyond
that RGBA path. Most scene work still runs on the CPU in those paths.
The first direct-geometry Clock adapter removed CPU rasterization and image
uploads, but submitting each shape through a separate FemtoVG path was slower:
about 40 FPS idle and 19 FPS in rain. The subsequent **ordered triangle batch**
holds 60 FPS in both fixtures, using 32% of one core when idle and 55.5% in rain,
versus 80.5% and 92.5% for software in the matched runs. It also removes the
first adapter's internal water-cell seams. This establishes useful CPU headroom
on the tested Clock workloads, not general renderer readiness.
The [UI correctness follow-up](#rounded-borders-and-fresh-kms-screenshots) fixes
the missing rounded borders and stale KMS captures on Pi 4. Reusing the vector adapter's
UI model helps but still leaves it well below 60 FPS; enabling GPU output alone
is not a scene-renderer optimization.
The [Pi 5 rotation follow-up](#pi-5-rotation-fix-and-matched-clock-comparison)
corrects non-square output and screenshot orientation before comparing all three
paths on HyperPixel. The subsequent
[Pi 5 Spacewars and Pizza comparison](#pi-5-spacewars-and-pizza-comparison)
finds that the best tested path depends on the workload: aligned RGBA leads the
live Spacewars fixture, while instanced circles lead dense Pizza. Keep software
as the default; device-specific measurements and physical input checks still
matter.

## Rendering paths

Scene rendering and Slint presentation are separate choices. `--renderer`
selects our scene adapter; `SLINT_BACKEND` selects Slint's output renderer.

| Path | Scene adapter | Slint backend | Work being compared |
| --- | --- | --- | --- |
| A | `raster` | `linuxkms-software` | Current CPU rasterization and software UI/output |
| B | `raster` | `linuxkms-femtovg` | Same CPU image, uploaded for GPU presentation and UI drawing |
| C | `vector` | `linuxkms-femtovg` | Existing scene-to-Slint geometry adapter, with GPU drawing and presentation |
| D paths | `raster` + `host gpu-scene paths` | `linuxkms-femtovg` | Per-path FemtoVG Clock reference, rendered into a retained GPU texture |
| D triangles | `raster` + `host gpu-scene triangles` | `linuxkms-femtovg` | Ordered triangle batches, clipped camera passes, grouped minimaps, then Slint text and UI composition |
| D retained | `raster` + `host gpu-scene retained` | `linuxkms-femtovg` | Same passes with bounded terrain tessellation reuse; `retained-cold` rebuilds each pass |
| D adaptive | `raster` + `host gpu-scene retained-adaptive` | `linuxkms-femtovg` | Same exact-scale meshes, but only cache keys observed at least twice within bounded probation |
| D zoom | `raster` + `host gpu-scene retained-zoom` | `linuxkms-femtovg` | Scale-independent fills and guarded fixed-pixel outlines; unsafe pieces use ordinary tessellation |
| D hybrid | `raster` + `host gpu-scene retained-hybrid` | `linuxkms-femtovg` | Reusable templates at new zooms; exact-scale meshes after reuse across frames, under one budget |
| D outlines | `raster` + `host gpu-scene retained-outlines` | `linuxkms-femtovg` | Same hybrid with a tighter conservative domain for round outline joins; the older guard remains a control |
| D paced | `raster` + `host gpu-scene retained-paced` | `linuxkms-femtovg` | Refined outlines with a shared per-frame construction allowance; latency/warmup tradeoff |
| D memoized | `raster` + `host gpu-scene retained-memoized` | `linuxkms-femtovg` | Refined outlines with bounded exact repeated-polygon reuse during each template build |
| D culled | `raster` + `host gpu-scene retained-culled` | `linuxkms-femtovg` | CPU-expanded ranges with conservative cached-triangle visibility checks against each camera pane |
| D resident | `raster` + `host gpu-scene retained-resident` | `linuxkms-femtovg` | GPU atlas plus ordered indexed vertex references and streamed placements/guarded geometry |
| D resident passes | `raster` + `host gpu-scene retained-resident-passes` | `linuxkms-femtovg` | Same atlas/shader/references, with separate dynamic-data texture objects for camera passes |
| D resident buffers | `raster` + `host gpu-scene retained-resident-buffers` | `linuxkms-femtovg` | Read-only storage-buffer atlas/frame data; requires vertex-stage SSBO support, unavailable on tested Pi 4 |
| D resident uniforms | `raster` + `host gpu-scene retained-resident-uniforms` | `linuxkms-femtovg` | Texture atlas with a bounded uniform-buffer transform table and texture overflow path |

Path B cannot remove CPU scene-rasterization cost. Path C exposes the existing
vector adapter on the Pi; it is not a new batched GPU scene renderer. Measure
its scene-item construction and submission costs before deciding whether it
is the long-term implementation. No physics or Clock event behavior changes
are part of this experiment.
Path D bypasses both CPU scene rasterization and scene-to-Slint item generation.
Its first implementation still issues many path-level GPU commands; one canvas
flush does **not** mean one GPU draw call. The triangle implementation replaces
that submission path. `host gpu-scene on` now aliases `triangles`; use `paths`
explicitly to reproduce the earlier adapter.
The triangle and circle modes now accept 1× raster scenarios with multiple
camera frames, including Spacewars. The `paths` reference remains Clock-only.
See [live viewports and grouped minimaps](#live-viewports-and-grouped-minimaps)
for the integration boundary and checks. Optional caching is described in
[live terrain reuse](#live-terrain-reuse), [adaptive terrain retention](#adaptive-terrain-retention)
and [scale independent terrain tessellation](#scale-independent-terrain-tessellation).
The [hybrid experiment](#hybrid-geometry-reuse) combines their admission policies.
The [outline experiment](#refined-outline-reuse) reduces unnecessary guarded rebuilding.
The [construction experiment](#exact-repeated-polygon-construction-reuse) measures
and skips duplicate tessellation during template creation, with unchanged guards.
The [resident experiment](#gpu-resident-ordered-references) tests GPU placement
and upload savings, including a control for cross-pass texture dependencies.
None changes the software default.

## Application selection and fallback

Build the experimental client with:

```sh
cargo build --locked --release -p engine-client \
  --no-default-features --features pi-kiosk-gpu
```

This includes both software and FemtoVG. Missing `SLINT_BACKEND`, an empty value,
`linuxkms`, and `linuxkms-software` all select software explicitly. Only
`linuxkms-femtovg` selects GPU presentation. The ordinary `pi-kiosk` build rejects
that request with a feature-specific error. Unknown backend names also fail;
there is no silent renderer substitution that could invalidate a comparison.

A requested GPU path that fails to initialize exits. Fallback means explicitly
restarting with `linuxkms-software`, not automatically hiding the failure.
Backend/window initialization precedes rendering-preference writeback. A startup
selection failure therefore leaves valid saved settings unchanged. Headless
benchmarks return before backend selection and still work without a display.

The software renderer continues to enforce raster scenes because its Slint Path
drawing is unavailable. FemtoVG permits both adapters. Use a separate test config
directory for A/B/C runs so software normalization cannot overwrite a user's
saved vector choice.

## Image requirements and build

The September 30 audit found V3D kernel devices on both the Pi 4 Picade and Pi 5
HyperPixel, but no EGL, GLES, GBM or Mesa DRI userspace libraries in their running
images. A fast binary update is insufficient for this first experiment.

The pinned Slint 1.13.1 LinuxKMS backend already provides EGL/GBM output and
rotated rendering through FemtoVG. The opt-in KAS overlay selects Mesa 25.1.6
from the existing meta-raspberrypi layer, with Gallium, EGL, GLES and GBM. Its
packaged drivers include VC4 and V3D. The normal image configuration is unchanged.

```sh
cd yocto
npm run build -- --gpu
```

By default this uses a sibling build directory named
`.space-wars-yocto-build-raspberrypi-spacewars-gpu`; the checkout name in the
prefix follows the checkout being built. Downloads and sstate remain shared,
but image artifacts and the fast-update bundle are separate. An explicit
`KAS_BUILD_DIR` overrides this location: do not point it at the standard build.

The image includes both renderers but the kiosk service still defaults to
`SLINT_BACKEND=linuxkms-software`. The first completed GPU image has the same
boot identity as the standard image, but a different fast-update compatibility
ID, reflecting its new libraries. Verify identities again for each deployment.
Never bypass the updater's compatibility check to install the GPU binary onto
a software-only rootfs.

After scheduling a device test, perform a normal A/B rootfs update using the GPU
build directory and `--skip-build`. The update wrapper does not accept `--gpu`;
omitting `--skip-build` would invoke the ordinary build configuration. For
example, from the repository root on this workstation:

```sh
KAS_BUILD_DIR=/home/data/workspace/.space-wars-yocto-build-raspberrypi-spacewars-gpu \
  ./update.sh --skip-build --target sw-picade-2.local
```

That command replaces the inactive OS slot and reboots the cabinet. The first
GPU image was installed on sw-picade-2 on September 30. Its binary checksum and
unchanged saved app settings were verified after reconnecting it to the
workstation's LAN; it booted with software rendering at 60 FPS/UPS. The initial
updater health check timed out because the device selected a different saved
Wi-Fi network, not because the app failed to boot. Follow the normal
[update and recovery runbook](pi-kiosk.md), preserving settings, Bluetooth
pairings and boot overrides. Confirm software startup on the new image first.

The experimental image now includes a restricted renderer-switch helper. It
requires a normal rootfs update to install its helper, sudo rule and service
drop-in; fast binary updates cannot install these system files.

```sh
ssh spacewars@sw-picade-2.local sudo -n /usr/sbin/spacewars-renderer gpu
ssh spacewars@sw-picade-2.local sudo -n /usr/sbin/spacewars-renderer software
```

Each switch ends the current session and restarts only the kiosk. The helper
accepts exactly these two modes, shares the fast-update lock, and requires three
healthy replies from one PID reporting the requested renderer. Failed trials
restore and verify software rendering. It writes only a root-owned override in
`/run/spacewars-renderer/renderer.env`; a reboot clears it. User settings and
`/boot/spacewars-kiosk.env` are untouched. Hardware rotation and output selection
still come from the hardware profile/boot overrides. The normal, non-GPU image
does not install this extra privilege or service drop-in.

## Diagnostics and measurement limits

`spacewars-cli status` includes `kms_renderer=software|femtovg`. GPU startup also
records `kms_gl_vendor`, `kms_gl_renderer` and `kms_gl_version` when the driver
provides them. Confirm V3D rather than a CPU Mesa renderer before claiming
hardware acceleration. A missing identity is unverified, not a pass.

The optional **Performance Overlay** shows FPS/UPS, process CPU usage (one core
= 100%), scene preparation wall time, and CPU-side draw time. The final line
identifies the scene adapter and output: for example, `raster -> CPU software`
or `raster -> GPU (V3D)`. Selecting FemtoVG without a recognized driver remains
`GL (unverified)`; known software Mesa drivers show `GL software`. `GPU n/a`
means GPU execution time is not measured, not that the GPU is idle. The small
overlay snapshot avoids formatting/sorting the full status report. Missing
samples or disabled profiling produce `--`, not fabricated zero timings.

The bounded KMS profiler retains completed-loop samples and adds `egl_swap`.
For FemtoVG, `draw` covers CPU command generation and driver calls, including
nested `flip_wait` and `egl_swap`; `submit` covers GBM front-buffer acquisition
and DRM submission. Do not add nested timings to their parent. These are CPU
and wall-clock measurements, **not GPU execution time**.

Profile version 8 separates CPU time inside `glTexImage2D`, `glTexSubImage2D`,
`glDeleteTextures` and `glBufferData` into `gl_texture_allocate`,
`gl_texture_upload`, `gl_texture_delete` and `gl_buffer_upload`. These stages
are also nested inside `draw`. The wrappers forward the driver's arguments
unchanged and add no readback or GPU synchronization. Disabling profiling at
startup leaves the real entry points in place. `gl_upload_bytes` counts logical
uncompressed texture payload, not bus traffic, allocation size or vertex data.
`kms_gl_stencil_bits` queries the window framebuffer once at startup.
`kms_gpu_memory_measured=false` makes clear that zero software-buffer byte counts
do not mean zero GPU memory use.
Version 9 additionally counts `gl_draw_calls` from `glDrawArrays` and
`glDrawElements`, and `gl_buffer_bytes` from non-null `glBufferData` payloads.
Draw counters add no per-draw clock reads. Blits, instanced entry points and
driver-internal commands are not included in that draw count.

Use the existing [presentation lab](presentation-performance-lab.md) and
[Clock lab](clock-performance-lab.md) for fixed workloads and reference captures.
Keep viewport, seed, Clock style/event, UI state and 1× resolution identical.
Warm up each path, repeat measurements in both orders, and exclude screenshot
readback from timed windows. Report scene preparation, KMS stages, total process
CPU, frame pacing, and process memory separately. Process RSS alone does not
measure GPU allocations. Stable 60 FPS without reduced CPU cost is not enough
to establish an improvement.

## Validation and next device checks

Local checks cover renderer selection, settings preservation after failed
selection, headless operation with GPU compiled in, and KMS profiling. The
Linux CI job compiles the opt-in client and runs the vendored software/FemtoVG
backend tests without requiring graphics hardware. The separate ARM image build
and its package manifest confirm Mesa/EGL/GLES/GBM inclusion; the standard
software bundle remains untouched.

Strict client Clippy on the local Rust 1.94 toolchain still reports 24 errors in
unchanged code (including argument counts, nested conditions and manual clamps).
Those existing lints were not suppressed or folded into this rendering change.
Formatting, client/backend tests and the GPU feature compile check pass.

The renderer-switch/overlay slice passed 470 client tests in each desktop and
GPU-enabled build (one ignored), 25 backend tests with and without FemtoVG (one
ignored), 11 startup/headless/presentation integration tests, and 36 Node plus
44 Python Yocto tests. The latter include isolated real-shell helper tests for
rollback, crash loops, wrong-renderer replies, locks and invalid arguments.
The controller-driven UI functional test compiles but was not run locally
because this session has no display or Xvfb. Overlay layout and repaint tests
were rendered and inspected at the Picade/HyperPixel sizes.

## Pi 4 device results

Remote access to sw-picade-2 was restored before the October 1 trials. The
helper/Mesa image was already installed in slot B; only the experimental app
and CLI needed redeployment. The installed compatibility ID and deployed binary
checksums matched the GPU build bundle. No OS update or reboot was needed.

Both GPU starts reported `Broadcom`, `V3D 4.2.14.0` and
`OpenGL ES 3.1 Mesa 25.1.6`. Each explicit switch back to software passed the
helper's health check. The boot ID stayed unchanged, both measured GPU sessions
retained their original app PID, and no page-flip read errors were reported.
Clock, heavy rain/water, Clock-wave marquee, launcher and settings screenshots
were captured successfully. Pause, menu navigation, scenario restart and
return-to-launcher were exercised through CLI UI actions; these checks do not
substitute for physical controller testing.

The captures also reveal a visual defect: rounded menu outlines have missing
corners in FemtoVG, while the corresponding software-rendered settings screen
has continuous borders. Text and the raster Clock scene are visible, but this
is not a visual-parity pass. Diagnose the border rendering before enabling GPU
output by default.

### Preliminary Clock timings

The sequence was GPU, software, GPU, software at 1024×768, raster 1×, seed 0,
Classic digits and the performance overlay enabled. Automatic events were
temporarily Off. Each run warmed up, sampled idle four times, then manually
triggered Heavy rain and sampled raining-phase ticks 600, 780, 960 and 1140.
Screenshots were taken outside these sample windows.

These are medians of eight live samples per cell, not a fixed-time benchmark:
the wall clock advanced, changing digit geometry, and the CPU governor remained
unlocked. Idle scenes had 508–576 primitives; rain had 1359–1579. Sampled CPU
frequencies ranged from 1.1 to 1.5 GHz, temperatures from 64.8 to 69.1°C, and
the hwmon undervoltage alarm was zero at every environment capture.

| Scene | Output | Process CPU | Scene preparation | Draw CPU | FPS |
| --- | --- | --- | --- | --- | --- |
| Idle Clock | Software | 77% | 5.7 ms | 6.6 ms | 60 |
| Idle Clock | FemtoVG/V3D | 87% | 5.5 ms | 8.6 ms | 60 |
| Heavy rain | Software | 91% | 7.6 ms | 6.0 ms | 60 |
| Heavy rain | FemtoVG/V3D | 98% | 7.2 ms | 8.1 ms | 59.9 |

Process CPU is relative to one core. Preparation is wall time; Draw CPU is
CPU time, not GPU execution time. GPU rain samples ranged from 58 to 60 FPS;
software rain samples all reported 60. Median per-window loop p95 was about
17.3 ms during GPU rain versus 16.7 ms in software. End-of-rain process RSS was
56–62 MiB with FemtoVG versus about 39 MiB with software; GPU allocations are
not separately measured, and the first GPU process had a longer warm history.

The additional cost is inside the GPU draw stage, not principally EGL swap:
its measured CPU cost was only about 0.2 ms. These first trials did not separate
upload, image conversion, driver work and UI drawing. Path B still performs all
scene rasterization on the CPU, so these results do not establish how a
geometry-based GPU scene renderer will perform.

The cabinet was returned to software rendering with Demo events running. Its
settings file matched the pre-test backup byte for byte, including controller
assignments, volume and automatic-event preferences. The other Picade and Pi 5
were not touched. Workstation captures, raw status samples, logs and the
collector script are under
`target/gpu-experiments/picade-gpu-trial.mkRHxZMK/`.

The earlier intermittent boot/power problem remains unexplained; a successful
app-only trial does not establish that it is fixed. Read the `rpi_volt` hwmon
alarm for subsequent power checks: the deprecated firmware `get_throttled`
sysfs node itself emitted a kernel warning during the pre-GPU inspection.

### Upload attribution and vector model reuse

The follow-up used the same cabinet, native resolution, Classic clock, seed 0,
Off automatic events and manually triggered Heavy rain. GL profiling identifies
about **7.3 ms of an 8.6 ms idle draw stage inside `glTexSubImage2D`**. Each frame
uploads a new 1024×768 RGB image: 2,359,296 logical bytes, or about 142 MB/s at
60 FPS. Texture allocation and deletion cost about 0.09 and 0.02 ms respectively;
vertex upload costs about 0.04 ms. Repeated raster trials gave similar results.
This isolates the expensive API call, not its internal cause. RGB conversion,
driver copying and texture tiling remain hypotheses, not measured sub-stages.

Path C eliminates these full-screen texture uploads, but the existing adapter
creates hundreds or thousands of individual Slint items. It also previously
replaced the entire main-scene `VecModel` every frame. Main-scene publication
now reuses the same model and notifies Slint only for changed rows, using the
same mechanism as the retained raster text overlay. Minimap models are not
changed in this experiment. Regression tests cover unchanged frames, geometry
and kind changes, reordering, growth, shrinkage and empty frames. The existing
retained-text pixel comparison also passes.

| Vector scene publication | Idle FPS | Rain FPS | Idle draw CPU | Rain draw CPU |
| --- | --- | --- | --- | --- |
| Replace model each frame | 30.8 | 10.0 | 29.0 ms | 89.9 ms |
| Retain model and update rows | about 41 | about 14 | about 18 ms | about 59 ms |

The replaced-model row uses four samples per scene; the retained-model result
was repeated twice with four samples per scene each time. These remain live
trials, not a fixed-geometry A/B benchmark: the changing clock digits produced
roughly 560 idle items before versus 490–510 after, and about 1510 rain items
before versus 1380–1400 after. Do not attribute an exact percentage speedup to
model reuse. The conclusion is narrower: reuse helps, but this adapter still
cannot sustain 60 FPS even on the idle clock.

During retained-model rain, scene preparation took about 11.4 ms and draw took
about 59 ms of CPU time, with roughly one core fully occupied. Texture-upload
calls were zero in these sample windows; vertex-buffer upload was only about
0.8 ms. The remaining cost is not yet divided between Slint item processing,
path parsing/tessellation, command generation and other GL driver calls. The
GPU's actual execution time is still unmeasured. A batched geometry experiment
should measure those boundaries rather than assuming that either the current
item adapter or the hardware establishes the performance ceiling.

The final software cross-check held 60 FPS/UPS for idle and Heavy rain, at
about 77% and 90% of one CPU core respectively. Its draw CPU times were about
6.6 and 6.0 ms. Sampled temperatures across the follow-up were 65–71°C and
frequencies were 1.1–1.5 GHz; every captured undervoltage alarm was zero.

Startup reports eight window-framebuffer stencil bits. This rules out a missing
stencil buffer as the explanation for the missing rounded border corners; it
does not establish that all stencil operations are correct. The border issue
remains open, and no visual workaround has been applied.

The follow-up passed 471 desktop client tests (one ignored), 28 backend tests
with FemtoVG (one ignored), and 25 software-only backend tests (one ignored).
Formatting and diff checks passed. The ARM release build and app-only update
succeeded. Raw samples, screenshots, environment checks, the collector and the
previous client binary are retained locally under
`target/gpu-experiments/picade-gpu-detail.CORD6T2l/`.

The updated app remains installed on sw-picade-2, restored to **software,
raster 1× and Demo events**, reporting 60 FPS/UPS. The settings file again
matches its original SHA-256 exactly. The boot ID stayed unchanged; the final
service has no automatic restarts or page-flip read errors. No other cabinet
was changed. These successful rendering trials do not resolve the earlier
intermittent boot/power issue.

## Texture format probe

Investigate upload options separately before building a new scene renderer.
The first probe changes only RGB versus RGBA texture format, using the same
pinned FemtoVG texture API as Slint. Both variants allocate, upload, draw and
delete a fresh input texture each frame. RGBA expands the RGB source into a
preallocated buffer **on every frame**, and includes that conversion in the
timed total. It is not a comparison against free, preconverted RGBA pixels.

Run the GPU-enabled release client on a machine with Mesa EGL render-device
support. No Slint window, settings, IPC service, DRM-master ownership or scanout
is opened. The device must match an enumerated EGL render node; known software
GL renderers are rejected. Pause any other graphics workload first.

```sh
engine-client --benchmark-presentation --presentation-upload \
  --presentation-upload-device /dev/dri/renderD128 \
  --presentation-upload-fixture clock-1024 \
  --presentation-frames 120 --presentation-repeats 4
```

The frozen production Clock image is 1024×768 at 08:08:00 with events Off. The
alternative `falling-256` fixture is the bundled NES cartridge's frozen frame.
Fixture simulation/rasterization, context creation, shader setup, warmup and
readback are outside timing. Each format warms up for 16 frames. The first pair
runs RGB then RGBA; the next reverses the order. Each block verifies exact RGB
channels and opaque alpha against the source, failing instead of reporting a
fast but incorrect render.

CSV reports wall and calling-thread CPU time separately for conversion,
allocation, upload, draw submission, deletion, end-of-block GPU drain and the
whole block. The drain is included in the total; there is no per-frame
`glFinish`. All time columns are averages per frame. The checksum identifies
the input pixels, and payload bytes describe logical texture data, not actual
memory-bus traffic. CPU time does not include driver worker threads, and these
measurements are not GPU execution timings.

This is an unpaced offscreen throughput experiment, not live FPS. It excludes
the rest of Slint's UI drawing, scenario rasterization, buffer swaps and scanout.
A winning format still needs a live end-to-end test before changing defaults.
Texture reuse, buffering, partial updates and scene geometry are unchanged by
this experiment.

### Pi 4 format comparison

On October 1, two independent release-probe invocations ran four paired
120-frame blocks each on sw-picade-2. Both reported Broadcom V3D 4.2.14.0,
OpenGL ES 3.1, Mesa 25.1.6. Every block's readback matched the same source
checksum exactly. The table gives medians across eight blocks per format,
960 measured frames each, including all blocks rather than discarding slower
ones. Times are milliseconds per frame.

| Stage | RGB | RGBA |
| --- | --- | --- |
| CPU expansion into RGBA | 0.00 | 1.22 |
| CPU texture upload | 6.91 | 2.01 |
| Total calling-thread CPU | 7.21 | 3.57 |
| Total wall time including GPU drain | 8.88 | 5.24 |

RGBA uploaded 3,145,728 bytes per frame instead of RGB's 2,359,296, yet remained
faster after charging for expansion. Total calling-thread CPU fell by about
3.63 ms, or 50%; whole-block wall time fell about 41%. Allocation cost roughly
0.07 ms for RGB and 0.11 ms for RGBA; draw submission was about 0.20 ms for both.
This supports trying the RGBA path in the live client. It does not isolate the
driver's internal copying, conversion or tiling, or prove an application FPS
improvement. In particular, these totals exclude CPU scene rasterization.

There is a repeatable startup effect despite the 16-frame warmup: each process's
first measured RGB block took about 13.5 ms wall/11.85 ms CPU per frame. Later
RGB blocks settled at 8.87–8.90 ms wall; RGBA blocks ranged from 5.20–5.29 ms.
The raw data retains this effect, whose cause is not yet established. The RGBA
advantage also holds against the settled RGB blocks, not just the slow starts.
Each block's final GPU drain contributed about 1.67 ms wall per frame to both
formats. This unpaced queue/drain behavior is another reason not to convert
the result into a predicted display frame rate.

Sampled temperatures were 67–70°C, CPU frequency was 1.5 GHz at every environment
capture, and all captured undervoltage alarms were zero. The temporary probe
executable never replaced the installed kiosk binary. After resuming the
clock, software raster 1×/Demo events again reported 60 FPS/UPS, with the same
PID, boot ID, binary checksum and settings checksum, no automatic restarts,
and no page-flip read errors. The other devices were untouched.

Local validation passed 474 GPU-enabled client tests (one ignored), both
startup-isolation integration tests, the normal desktop feature check, and a
pixel-exact desktop GPU smoke test. The ARM release build, formatting and diff
checks passed. Probe source is in
`crates/engine-client/src/presentation_probe/upload.rs`; raw CSV, environment
checks, collector script and tested binary are retained under
`target/gpu-experiments/picade-upload-format.LoWsj4VE/` on the workstation.

### Live format control

The experimental client now supports a session-only A/B switch:

```sh
spacewars-cli host raster-upload rgb
spacewars-cli host raster-upload rgba
```

This requires a `pi-kiosk-gpu` build, an active `linuxkms-femtovg` backend and
an active raster scene. It rejects software output, launcher transitions,
vector scenes and native NES/Falling video. The reply acknowledges selection;
it applies on the next raster frame. `spacewars-cli status` reports the selected
`raster_upload_format`. It is not a saved preference: restarting the application
restores RGB. The command does not change the scene renderer or start a scenario.

Both choices still rasterize into the same RGB buffers. RGBA expands the final
opaque pixels before publishing the Slint image, with all alpha bytes set to
255. Conversion and allocation are included in `host_raster_image` and its
parent preparation/callback timings. The RGBA pool follows the three existing
CPU buffer slots and uses copy-on-write for retained images. At 1024×768 it
adds up to 9 MiB of retained CPU pixel storage; this is not a GPU-memory count.
The pool remains allocated when switching back to RGB for the comparison and
is released on resize or renderer destruction. An RGB-only session never
allocates these extra buffers. GPU texture allocation and upload policy remain
unchanged; this is not the persistent-texture experiment.

Image publication exposes four wall-time sub-stages under `host_raster_image`:
`buffers` for pool acquisition/resize, `make_mut` for shared-buffer detachment,
`convert` for pixel expansion, and `wrap` for constructing the Slint image.
For example, `host_raster_image_convert_avg_ms` reports conversion wall time.
The matching `host_raster_image_new_buffers_window` and
`host_raster_image_cow_copies_window` count actual pool allocations/replacements
and copy-on-write pointer changes in the same bounded 120-callback window.
`host_raster_image_alignment_prefix_pixels_window` counts the pixels converted
before the aligned bulk loop; it does not count extra copies or allocations.
These sub-stages are already inside publication and host preparation; do not
add them again. Counts and timings reset with the host sample window. No extra
per-frame logs, readbacks or GPU synchronization are introduced.

The live collection uses diagnostic-only commands for the existing headless
fixtures, without adding a benchmark button to Clock's menu:

```sh
spacewars-cli host clock-benchmark idle
spacewars-cli host clock-benchmark rain
```

These require an active Clock, restart at deterministic 08:08, and use the same
fixture driver as the headless runner. A normal scenario restart exits the
fixture. The host does not publish or save fixture settings as user preferences.

The comparison uses native 1024×768 raster 1× and heavy rain. This avoids changing
hour/minute digit geometry between trials; seconds/colon animation continues normally. Samples
are synchronized with Clock phase ticks and follow a warmup longer than the
120-loop timing window. Reference software runs bracket the paired GPU order
RGB, RGBA, RGBA, RGB. Screenshot capture and a paused RGB/RGBA pixel comparison
are outside timed windows. The collector backs up settings as a precaution,
checks that they remain unchanged after each fixture, and verifies their exact
checksum after returning to the normal software clock, including on failure.

Persistent textures and buffering remain separate experiments. The new geometry
renderer has not been started.

### Pi 4 live format results

The October 1 live comparison used release client `37115984916d`, CLI
`aa11d2857023`, V3D 4.2.14.0 / Mesa 25.1.6 and profile version 8 on sw-picade-2.
Each run sampled idle and heavy-rain fixtures at phase ticks 600, 780, 960 and
1140, after resetting each fixture. Tables report medians of eight sampled
windows per path/workload (four windows in each of two runs). These are medians
of window averages, not pooled per-frame percentiles. CPU percentages below
come from the process-wide overlay; 100% means one CPU core. FPS is the host's
submitted-frame rate, independently corroborated by completed page flips.

| Path | Idle CPU | Idle FPS | Heavy-rain CPU | Heavy-rain FPS |
| --- | ---: | ---: | ---: | ---: |
| Raster → software | 81% | 60.0 | 95% | 60.0 |
| Raster RGB → GPU | 93% | 60.0 | 96% | 52.9 |
| Raster RGBA → GPU | 82% | 60.0 | 95.5% | 60.0 |

| Stage, milliseconds per sampled drawing/callback iteration | Idle RGB | Idle RGBA | Rain RGB | Rain RGBA |
| --- | ---: | ---: | ---: | ---: |
| Host image publication, wall time | 0.001 | 2.64 | 0.001 | 2.63 |
| GL texture upload, CPU time | 7.09 | 2.20 | 6.92 | 2.09 |
| Entire backend draw, CPU time | 8.31 | 3.46 | 8.10 | 3.29 |
| Entire KMS loop, CPU time | 15.34 | 13.59 | 18.06 | 15.81 |

Upload is nested in draw, which is nested in the loop; **do not add those rows**.
Publication includes RGBA buffer acquisition/copy-on-write, expansion and image
construction, and is already included in host preparation and loop cost.
Software's loop CPU was 13.49 ms idle and 15.83 ms in rain. This explains the
nearly equal software/RGBA CPU totals. RGB's rain CPU percentage is not directly
a throughput comparison: it consumes about a core while drawing fewer frames.
Completed-flip rates were approximately 53.3/s for RGB rain and 60.0/s for both
other rain paths. The median sampled host-interval p95 improved from 22.46 ms
to 16.84 ms with RGBA (software: 16.76 ms). Simulation remained near 60 UPS.

The format change is real: logical upload payload changed from 2.25 to 3.00 MiB
per draw. GPU texture allocation remained about 0.09 ms CPU; deletion remained
0.02 ms. These small call costs alone cannot explain the original penalty.
The paused live RGB/RGBA captures differ at **zero pixels**. This verifies parity
between formats, not parity with software: the previously observed incomplete
rounded menu borders remain visible with both GPU formats.

Process RSS was about 39 MiB for software, 53–54 MiB for the first RGB run and
64–66 MiB after using RGBA. The later RGB run retained the RGBA pool and similar
RSS. These figures do not measure GPU allocations. Temperature samples ranged
64.8–69.6°C, CPU frequency samples 1.2–1.5 GHz, and all sampled undervoltage alarms
were zero. Frequency was not pinned; these results describe normal governor
behavior, not an isolated fixed-frequency throughput benchmark. Paired runs
agreed, and the software reference before/after differed by less than one CPU
percentage point in the longer `/proc` intervals. Those intervals include
warmup/fixture startup, so use the steady-window table for active-rain cost.

At this point publication cost was roughly double the isolated probe's 1.22 ms
expansion time. The next experiment separated buffer acquisition, pixel
conversion and image wrapping, then tested destination alignment; see below.
Texture reuse remains a separate experiment. These format results do not
identify a specific internal driver conversion or establish its copying policy.

All fixture settings checks passed. The collector returned to the normal,
non-benchmark Demo clock on software raster 1×, reporting 60 FPS/UPS, original
settings checksum, unchanged boot ID, zero automatic restarts and no sampled
page-flip read errors. Only this cabinet was updated; no default changed.
Validation passed 481 GPU-enabled client tests (one ignored), 20 CLI tests,
desktop checks, ARM release build, formatting and diff checks. Tests cover
pixel preservation and retained buffers, strict commands, matching visible and
headless fixtures, normal restart and suppression of fixture preference saves.
Scripts, raw status/Clock JSON, environment samples, captures and the local
summary tool are in `target/gpu-experiments/picade-upload-live.kPOyDEAy/`.

### Publication attribution and alignment

The October 2 instrument-only release (`cc38b7005ca0`) repeated the same six-run
live comparison before changing conversion. Median RGBA publication remained
2.639 ms idle and 2.651 ms in rain. The conversion loop accounted for 2.635 ms
and 2.648 ms respectively; pool lookup, mutable-buffer acquisition and image
wrapping together cost only a few microseconds. Every sampled steady-state
window reported **zero new RGBA buffers and zero copy-on-write detachments**.
The expensive work was conversion, not repeated allocation or a retained-image
copy. Software and GPU RGBA both used about 80% of one core idle and 92.5% in
heavy rain, at 60 FPS.

Disassembly of both the isolated probe and live client showed vectorized
`ld3`/`st4` loops. The discrepancy was not simply a missing SIMD implementation.
Slint's shared pixel storage follows a 24-byte shared-vector header on this
target, unlike the probe's separately allocated `Vec`. That motivated an
isolated test varying the destination address modulo 16 bytes.

The alignment probe converted 1024×768 RGB pixels into opaque RGBA. It tested
source offsets of 0/8 bytes and destination offsets of 0/4/8/12 bytes modulo
16, with eight warmup frames and 60 measured frames per block. Four repeats
reversed variant order. Every output pixel was checked outside the timed
region. The kiosk was paused and briefly stopped during this bounded probe,
with a trap to resume it; no GL work was involved.

| Destination address modulo 16 | Original loop, ms/frame | Aligned-prefix loop, ms/frame |
| --- | ---: | ---: |
| 0 bytes | about 1.03 | about 1.03 |
| 4, 8 or 12 bytes | about 2.29–2.33 | about 1.03 |

One early unaligned block reached 2.49 ms and one aligned block 1.10 ms; the raw
data retains these startup outliers. Both source offsets showed the same
pattern. This isolates a destination-alignment penalty in this conversion
kernel on the Pi 4, not a general claim about Slint or GPU memory.

The revised safe-Rust helper converts a short prefix first, then runs the bulk
loop on a 16-byte-aligned destination. It is shared by the live publisher and
offscreen upload probe. It adds no scratch buffer, extra full-image copy,
allocator/layout change, unsafe code or architecture-specific intrinsics.
Unit tests cover source/destination offsets, short images, vector tails,
untouched boundaries, exact alpha/color output and mismatched lengths.

Instrumentation results, alignment probe source/binary, raw measurements and
disassembly are in `target/gpu-experiments/picade-publication.wAztuu3Z/`.

### Aligned conversion in the live client

Release client `d0f6f2b294b5`, with unchanged CLI `aa11d2857023`, repeated the
same bracketed software/RGB/RGBA comparison on October 2. Each workload/path
again has eight sampled windows. Results include conversion, rasterization,
Slint UI work and presentation; they are not just upload microbenchmarks.

| Path | Idle CPU | Idle FPS | Heavy-rain CPU | Heavy-rain FPS |
| --- | ---: | ---: | ---: | ---: |
| Raster → software | 80% | 60.0 | 94% | 60.0 |
| Raster RGB → GPU | 91% | 60.0 | 96% | 54.0 |
| Raster RGBA → GPU, aligned conversion | 74% | 60.0 | 87.5% | 60.0 |

CPU is process-wide, with 100% meaning one core. At the same 60 FPS, aligned
RGBA saves 6–6.5 percentage points, or about 7% of application CPU use, against
the matched software references. Whole-loop CPU was 12.302/14.543 ms idle/rain
for RGBA versus 13.271/15.674 ms for software. This is a modest but repeatable
benefit, not removal of CPU rasterization. The original RGB GPU path still
misses the rain frame budget.

| RGBA stage | Before alignment, idle/rain ms | After alignment, idle/rain ms |
| --- | ---: | ---: |
| Conversion, wall time | 2.635 / 2.648 | 1.473 / 1.449 |
| Entire publication, wall time | 2.639 / 2.651 | 1.476 / 1.452 |
| GL upload, CPU time | 2.065 / 1.935 | 2.082 / 1.893 |
| Entire KMS loop, CPU time | 13.226 / 15.337 | 12.302 / 14.543 |

Stages overlap: conversion is inside publication, and publication/upload are
inside the loop. Do not sum these rows. Conversion is about 44–45% faster;
GL upload itself is essentially unchanged. Compared with the instrument-only
RGBA run, idle CPU fell from 79.5% to 74%, and rain from 92.5% to 87.5%.
Other stage costs vary between runs/builds, so the matched software comparison
above is the better measure of the net benefit.

Every RGBA window reported 240 alignment-prefix pixels across 120 callbacks:
two pixels per image before the aligned bulk loop. All sampled publication
allocation and copy-on-write counts remained zero. Paused RGB/RGBA screenshots
matched at **zero differing pixels**, and visual inspection found no new
format artifacts. Existing incomplete rounded GPU menu borders remain; this
does not establish software/GPU visual parity. There were no sampled page-flip
read errors, and RGBA completed page flips at approximately 60/s in both phases.

Process RSS was about 39 MiB for software, 53–54 MiB for initial RGB, and
63–66 MiB after RGBA allocation. The aligned helper adds no additional buffer.
Environment samples ranged 64.3–70.1°C and 1.1–1.5 GHz, with zero undervoltage
alarms. Frequency was not pinned. Software references before/after agreed
within 0.2 percentage points in longer `/proc` intervals; those intervals include
fixture startup and are distinct from the steady-window CPU table.

Validation passed **485 GPU-enabled client tests** (one ignored), the normal
desktop client/CLI check, ARM release build, formatting and diff checks. Tests
also verify retained-image immutability, resize behavior, released-slot reuse,
and bounded timing/count aggregation. The collector restored the non-benchmark
Demo clock on software raster 1× at 60 FPS/UPS, with the original settings
checksum, unchanged boot ID and zero automatic restarts. Only sw-picade-2 was
updated. Software remains the default, and RGBA remains session-only opt-in.

Scripts, raw status/Clock JSON, environment samples, captures and the summary
are in `target/gpu-experiments/picade-aligned-publication.trpRpg5k/`.
The following experiment isolates texture reuse; a geometry renderer has not
been started.

### Texture lifetime controls and method

The opt-in client can hold RGBA publication fixed while changing texture
lifetime. These commands require an active raster scene on LinuxKMS FemtoVG;
they reject software rendering and native emulator video:

```sh
spacewars-cli host raster-upload rgba
spacewars-cli host raster-textures fresh
spacewars-cli host raster-textures single
spacewars-cli host raster-textures triple
spacewars-cli host raster-textures slint
```

`fresh` allocates a new borrowed RGBA texture per frame; `single` updates one
texture repeatedly; `triple` rotates through three. `slint` restores ordinary
Slint image upload. Selection applies on the next raster frame and is never
saved. Returning to RGB requires selecting `slint` first.

This uses Slint's supported [borrowed OpenGL texture API](https://docs.slint.dev/latest/docs/rust/slint/struct.BorrowedOpenGLTextureBuilder),
verified against the pinned 1.13.1 implementation. Textures are created/uploaded
only in the window's current-context rendering callback. Touched texture and
pixel-unpack state is restored. Replaced textures are retired after the new
image's drawing is submitted, and teardown releases the remaining pool. A
latest-frame-only queue bounds pending CPU storage. There are no per-frame
readbacks, fences or `glFinish` calls in this live path. A failure logs its cause,
reports `raster_texture_error`, and explicitly falls back to Slint publication;
collectors reject any such fallback.

Borrowed textures use linear filtering, matching the existing scene Image's
default smooth sampling. An initial nearest-filtered trial differed from the
Slint reference at 51–63 pixels by one color level, even at 1×. Matching the
filter removed those differences. That trial is retained separately; final
live measurements use the pixel-matched implementation. The offscreen probe
uses nearest filtering consistently across all of its variants for exact
source-pixel readback.

The first texture command installs a rendering notifier. It stays installed
even after selecting `slint`; restart the application to recover the exact
no-notifier baseline. The experiment therefore includes both ordinary Slint
upload and Slint upload with the notifier installed, as well as the borrowed
`fresh` control. This distinguishes integration overhead from texture lifetime.
Status reports requested/applied modes, notifier installation, cumulative
uploads/allocations/deletions, and resident **texture object count**, not measured
GPU memory. Upload call CPU/wall time remains inside the existing Draw/loop
timings; do not add it again.

The independent offscreen comparison uses:

```sh
engine-client --benchmark-presentation --presentation-upload \
  --presentation-upload-reuse --presentation-upload-fixture clock-1024 \
  --presentation-frames 120 --presentation-repeats 6
```

This holds aligned RGBA conversion fixed, balances all six orderings of
fresh/single/triple, and validates every output pixel. Sixteen marker pixels
change each frame to detect stale texture contents; that small, identical
mutation is included in all variants' total time. Each block warms its own
textures for 16 frames and drains warmup before timing; pool setup and final
readback are excluded. End-of-block GPU drain is included in total wall time.
The probe is unpaced and never owns scanout, so it cannot by itself establish
the live application's performance. The upload CSV now includes `lifetime`;
the earlier format experiment remains available without `--presentation-upload-reuse`.

The live collector uses the same 1024×768, 1×, 08:08 idle/heavy-rain fixtures as
before, with four steady 120-callback windows in each of two runs per path.
Software references and no-notifier GPU references bracket paired notifier
Slint/fresh/single/triple runs in reverse order. Pixel comparisons and captures
are outside measurement windows. Selection, allocation counts, dimensions,
driver identity, fixture phase, settings and fallback/error state are checked.

### Texture lifetime results: no useful live CPU improvement

The October 2 matched-filter release is client `f33834f928ac`, CLI
`ac728b86b0eb`. The full trial ran only on sw-picade-2, on V3D 4.2 with Mesa
25.1.6. Every path sustained 60 FPS/UPS and approximately 60 completed flips/s.
The table reports medians of eight steady windows per workload/path; CPU is
process-wide, with 100% representing one core.

| Path | Idle CPU | Heavy-rain CPU | Whole-loop CPU, idle/rain ms |
| --- | ---: | ---: | ---: |
| Software reference | 80% | 93% | 13.320 / 15.428 |
| RGBA Slint upload, no notifier | 74% | 86.5% | 12.268 / 14.401 |
| RGBA Slint upload, notifier installed | 75% | 87% | 12.422 / 14.482 |
| Borrowed texture, fresh each frame | 75% | 87% | 12.396 / 14.424 |
| Borrowed texture, reuse one | 74% | 86% | 12.338 / 14.392 |
| Borrowed texture, rotate three | 74% | 86.5% | 12.332 / 14.407 |

Reuse recovers a small part of the borrowed/notifier path's overhead, but does
not beat the unmodified RGBA reference by a meaningful amount. The whole-loop
differences against that reference are about +0.06–0.07 ms idle and within
0.01 ms in rain. The half-point difference in rounded rain CPU samples is not
evidence of a new improvement. The notifier-only control also costs roughly
0.08–0.15 ms; installing a callback is not a neutral baseline change.

The counters confirm that reuse actually occurred. Across each roughly
nine-second sampled interval, single/triple uploaded more than 500 new frames
with **zero new texture allocations**, holding exactly one/three texture
objects. The fresh control allocated once per upload. Returning to Slint
released all experiment-owned textures. Each GPU path still uploaded 3 MiB per
draw, and every sampled CPU publication allocation/copy-on-write count was zero.

Ordinary Slint texture allocation plus deletion accounts for about 0.10 ms per
draw. Eliminating those calls does not eliminate the upload: it remains about
2.09 ms CPU idle and 1.92 ms in rain for single/triple, essentially matching
Slint's 2.09/1.91 ms. Aligned RGB→RGBA conversion still takes about 1.49/1.47 ms.
These stages are nested inside preparation/draw and the complete loop, not
additional costs to sum onto it. CPU rasterization remains in every variant.

The independent unpaced probe is a useful warning against choosing by allocation
counts alone. Two fresh processes each ran all six orderings, giving twelve
blocks per lifetime policy. Every readback matched, including changing markers.

| Offscreen RGBA lifetime | Median total wall ms/frame | Block range ms/frame |
| --- | ---: | ---: |
| Fresh | 5.204 | 5.038–9.644 |
| Single | 6.824 | 6.731–6.887 |
| Triple | 5.696 | 5.574–7.129 |

Single/triple spend about 5.23/4.16 ms wall time inside upload but only
2.44/2.24 ms CPU; fresh defers about 1.87 ms/frame of completion waiting to the
end-of-block drain. This is consistent with reuse moving waiting into the
upload calls, not proof of a particular driver memory-management mechanism.
Fresh is highly variable: the earlier retained trial had a 9.58 ms median
and 5.14–10.48 ms range. Its offscreen code used the same filtering policy.
The ordering of whole-run medians therefore changes between trials. Do not
use either isolated ranking as a live-app throughput claim, or mistake these
CPU/wall timings for measured GPU execution time.

All six final paused comparisons (three modes at two scene states) have **zero
differing pixels** against normal Slint upload. Resuming between comparisons
changes 32,062 pixels, checking that reuse does not leave a stale frame. This
establishes parity among these GPU upload paths, not full software/GPU parity;
the existing rounded-border issue remains. The software/RGB/active-texture
command guards also rejected invalid combinations on the device.

Process RSS was roughly 38.5–39.7 MiB for software and 59.0–60.1 MiB for GPU
paths; this is not GPU memory usage. Environment samples ranged 65.2–69.6°C
and 1.2–1.5 GHz with no sampled undervoltage alarm. Frequency was not pinned.
Software before/after longer `/proc` intervals agreed within 0.3 CPU percentage
points; those intervals include fixture startup, unlike the steady-window table.
No sample reported a page-flip read error or texture fallback.

Validation passed 492 GPU-enabled client tests (one ignored), 21 CLI tests,
two startup-isolation tests, desktop/software-kiosk checks, ARM release build,
formatting and diff checks. The collector restored the normal non-benchmark
Demo Clock on software raster 1× at 60 FPS/UPS, with the original settings hash,
unchanged boot ID and zero automatic restarts. No other device was touched.

Final scripts, raw measurements, captures and summary are in
`target/gpu-experiments/picade-texture-linear.bURFviBD/`; the summary script and
earlier nearest-filter trial are in
`target/gpu-experiments/picade-texture-reuse.vjPWxrxL/`. The texture choices remain
session-only diagnostic controls. Keep ordinary Slint upload for the RGBA
presentation path. The next useful scene experiment is batched GPU geometry,
which can avoid CPU rasterization and full-frame pixel conversion/upload,
rather than another texture-pool default change.

## Direct Clock geometry: first adapter and negative result

The October 2 release is client `098d759b7567`, CLI `362c8f3e236e`. The opt-in
adapter consumes unchanged `RenderFrame` primitives, projects typed circles,
lines and polygons, and preserves layer/primitive ordering. It creates no SVG
strings or Slint item per scene primitive. A separate FemtoVG canvas draws the
geometry into one retained RGBA GPU texture with its own stencil attachment.
Slint composites that borrowed texture with the existing text, HUD and menus.
All graphics calls stay inside the window's current-context rendering notifier;
the notifier is shared with the earlier upload experiment.

This tests a useful integration boundary, not the final batching implementation.
The canvas uploads its vertex buffer once per flush, but executes separate
fill/stroke commands for individual paths. Different colors, opacity and
overlapping primitives are not merged or reordered to obtain better numbers.

On the GPU-enabled image, with an active raster Clock at scale 1×:

```sh
spacewars-cli host gpu-scene paths
spacewars-cli status
spacewars-cli host gpu-scene off
```

The choice is session-only and resets when leaving/restarting the scenario.
Software output, other scenarios, multiple viewports and non-1× raster scales
are unsupported. Enabling the path requires the normal `slint` texture-lifetime
mode; changing to a borrowed-upload experiment while direct geometry is enabled
is rejected. Invalid geometry or GL setup/rendering errors disable the adapter,
report `gpu_scene_error`, and fall back to CPU rasterization. Collectors reject
such fallbacks. Only the latest prepared scene is queued; targets are retired
after Slint has submitted a replacement and deleted with the context current.

### Matched live results

The sw-picade-2 trial used native 1024×768 output, 1×, the fixed 08:08 idle and
heavy-rain fixtures, and the same V3D 4.2 / Mesa 25.1.6 driver. Software runs and
no-notifier RGBA runs bracketed the notifier/direct-geometry trials. Captures
were separate from timed runs. Each row is the median of eight status snapshots
across two runs, at phase ticks 600/780/960/1140. Timings use 120-loop rolling
windows. **Slow rain windows overlap**, so these are descriptive snapshots,
not eight independent trials or statistical confidence intervals.

| Path | Idle / rain FPS | Idle / rain CPU | Whole-loop CPU, idle / rain ms |
| --- | ---: | ---: | ---: |
| Software reference | 60 / 60 | 80% / 93% | 13.265 / 15.401 |
| Raster → RGBA GPU, no notifier | 60 / 60 | 74% / 86.5% | 12.267 / 14.319 |
| Raster → RGBA GPU, shared notifier | 60 / 60 | 74.5% / 87% | 12.398 / 14.469 |
| Direct FemtoVG scene paths | 40 / 18.9 | 89% / 99% | 22.254 / 52.844 |

CPU 100% means one core. Completed page flips agree with the reported FPS;
simulation remains around 60 updates/s. Scene preparation falls from 8.40/9.95 ms
to 0.33/1.03 ms. Sampled full-frame texture uploads fall from **3 MiB per draw to
zero**, and all CPU raster/image-conversion timings are zero. The same scene
target is reused across 360 sampled idle frames and 171 rain frames per run,
with no new target allocations.

Those savings are overwhelmed by the new rendering work. Typed geometry
preparation takes 0.28/0.77 ms; path encoding/tessellation takes 2.33/5.17 ms;
the canvas flush takes 17.54/41.21 ms. Encoding and flush are CPU-side wall
measurements nested inside the KMS draw, **not GPU execution time**. KMS draw
itself consumes 21.26/49.23 ms of CPU. Buffer upload accounts for only
0.31/1.01 ms CPU inside that draw. These measurements locate the expensive stage
in path submission/driver work, not pixel upload. We have not separately timed
each GL draw/uniform call or the GPU shaders, so attribution within that stage
remains to be measured. There are roughly 574 idle and 1,668 rain primitives.

Process RSS is about 39 MiB for software, 59–60 MiB for the fresh GPU baseline,
and 64 MiB after creating the extra canvas. Turning the adapter off retires its
target, but keeps its canvas/shader resources available until context teardown;
RSS therefore stays higher in the later notifier control. This is bounded
process memory, not measured GPU memory. Samples span 64.8–70.1°C and
1.0–1.5 GHz; frequency was not pinned, and no sampled undervoltage alarm was set.
The software before/after longer process-CPU intervals agree within 0.2
percentage points. No GL fallback or page-flip read error was reported.

### Visual and lifecycle checks

Idle and rain are correctly oriented, with scene colors, text and pause controls
present. Raster captures before and after a paused direct-geometry round trip
have **zero differing pixels** in both fixtures. Resuming changes the scene,
and toggling off releases the direct target. These checks establish the tested
context-state restoration and texture lifetime, not complete visual parity.

The direct rain image has thin visible seams between neighboring water polygons
that are absent from the raster reference. The separate path antialias fringes
are the likely cause: shared internal edges must not be treated as exposed
silhouettes. Existing incomplete rounded menu borders also remain. The adapter
therefore fails both the performance and visual-quality acceptance gates and
must remain experimental.

Validation passed 499 GPU-enabled client tests (one ignored), 22 CLI tests, two
startup-isolation tests, desktop/software-kiosk checks, ARM release build,
formatting and diff checks. Typed-geometry tests cover projection, logical stroke
widths, concave/offscreen polygons, translucent order, invalid input and all
eight production Clock fixtures at both Picade and HyperPixel viewport sizes.
The latter is input validation, **not** Pi 5 GPU hardware validation.

The collector restored the normal non-benchmark Demo Clock using software at
1×/60 FPS/UPS. Settings hash and boot ID are unchanged, with zero automatic
service restarts. No other device was touched. Scripts, raw results, screenshots
and the validation/summary script are retained under
`target/gpu-experiments/picade-direct-geometry.JAN32kzc/`.

### Follow up

This result rejected the first **adapter**, not GPU geometry in general. The
next experiment kept the scene input, projection, current-context bridge and
borrowed-texture lifetime checks, replacing only the tessellation/submission
path as described below.

## Batched Clock triangles

The October 2 triangle trial uses client `a9755efef2e1` and CLI `3d78b6f5e016`.
It replaces per-path submission with Lyon tessellation into a reusable vertex
and index buffer. Vertices carry premultiplied color, and indices preserve
layer order, primitive order and fill-before-stroke order. Concave polygons are
tessellated rather than assumed to be triangle fans. No scene-specific rectangle
fast path, scene-to-UI objects, physics change or resolution reduction is used.

One indexed GL draw renders the entire Clock scene into a retained four-sample
RGBA8 target. A framebuffer blit resolves it into the retained texture borrowed
by Slint; Slint still draws the text, performance overlay and menus. Multisample
coverage replaces independent path antialias fringes, so adjacent water cells
do not acquire artificial transparent edges. Output stays native 1024×768 at
1×; four coverage samples are not a return to 2× CPU raster resolution.

The prototype requires GLES 3.1 or desktop OpenGL 3.3 and four-sample support.
Enable it only on the GPU-enabled image with an active raster Clock at 1×:

```sh
spacewars-cli host gpu-scene triangles
spacewars-cli status
spacewars-cli host gpu-scene paths       # Slow reference, not a recommendation
spacewars-cli host gpu-scene off
```

The existing session guards and fallback behavior still apply. GL resources
are created, used and released with the owning context current. Switching paths
retires the old target only after a replacement succeeds, and deletion waits
until Slint submits that replacement. Only one scene target remains resident
after each completed frame. Programs and buffer objects remain cached until
context teardown; their contents are uploaded each frame. This is not yet a
static-geometry cache.

### Matched performance

The same Pi 4, V3D/Mesa driver, fixed 08:08 fixtures and native viewport were
used. Software and fresh no-notifier RGBA runs bracket the shared-notifier,
paths and triangle runs, with two runs per choice. Each run samples phase ticks
600 and 1140. These nine-second gaps advance at least 120 KMS loops and 120
direct-render frames even for the slow path reference, so published windows
do not overlap within a fixture. The table gives medians of four observations
per workload and path, not confidence intervals. Captures are outside timed runs.

| Path | Idle / rain FPS | Idle / rain process CPU | Whole-loop CPU, idle / rain ms |
| --- | ---: | ---: | ---: |
| Software reference | 60 / 60 | 80.5% / 92.5% | 13.402 / 15.442 |
| Raster → RGBA GPU, no notifier | 60 / 60 | 75% / 86.5% | 12.366 / 14.380 |
| Raster → RGBA GPU, shared notifier | 60 / 60 | 75% / 87.5% | 12.456 / 14.479 |
| Direct FemtoVG paths | 40 / 18.9 | 89% / 99% | 22.227 / 52.134 |
| Direct triangle batch | 60 / 60 | 32% / 55.5% | 5.318 / 9.282 |

CPU 100% means one core. The triangle path uses about **60% less idle CPU and
40% less rain CPU than software**, or 57%/36% less than the RGBA GPU-presentation
reference. Rain CPU observations range 54–58%. Page-flip completion rates agree
with 60 FPS and simulation stays at 60 updates/s. Triangle host callback interval
p95 is about 16.8 ms, with the largest sampled maximum 17.7 ms.

Actual GL draws fall from about **1,440 idle / 3,580 rain per frame to 12**,
including Slint's text/UI draws. The scene itself is one draw plus a resolve;
the counter does not count framebuffer blits or hidden driver work as draws.
Scene vertex/index uploads are about 130 KiB idle and 269 KiB rain, compared
with the RGBA reference's 3 MiB full-frame pixel upload. Warm triangle windows
have zero texture upload bytes, zero texture allocations and zero CPU raster
image/conversion time. The same target survives each sampled 537–541-frame
interval without allocation. KMS profiling version 9 counts draws without
adding a timer around each one and records actual `glBufferData` payload bytes.

Typed scene preparation takes 0.38/1.03 ms, tessellation 1.90/4.00 ms and
triangle upload/draw/resolve submission 0.52/0.54 ms, idle/rain respectively.
These are CPU-side wall durations, **not GPU execution times**, and are nested
inside other reported scopes. Complete KMS draw CPU is 4.23/6.33 ms. Its wall
duration is larger because it includes waits; adding these nested values would
double-count work. CPU tessellation and the remaining UI/composition work are
now more useful investigation targets than per-shape GL submission.

Process RSS is about 39 MiB for software, 59–60 MiB for a fresh GPU baseline and
64.5 MiB for triangles after both renderers have been initialized. It stays up
to 65.5 MiB after switching back to the path/UI controls. This includes retained
reference-renderer resources and is **not measured GPU memory usage**. Samples
span 65.2–69.6°C and 1.0–1.5 GHz; frequency is not pinned. No sampled undervoltage
alarm or page-flip read error is set. The software before/after longer CPU
intervals differ by 1.6 percentage points idle and 1.4 points rain, much less
than the triangle savings; those intervals also include fixture startup.

### Visual checks and validation

Both Clock captures have the correct orientation, digits, water, scene colors
and text/UI. The former vertical water-cell stripes are absent. In a 430-pixel
interior-water row of the same paused rain frame, the path reference has 212
color transitions; triangles have zero, with the expected uniform fill.
This checks that particular seam, not all possible fluid geometry.

The paused raster image before/after a raster → triangles → paths → triangles →
raster round trip is pixel-identical in both fixtures. The two triangle captures
are also pixel-identical. Pausing, resuming, returning to the launcher and
automatic Clock relaunch pass; direct mode resets and the target is released.
Software and launcher requests are rejected without activating the prototype.
Existing incomplete rounded Slint menu borders remain a separate issue.

Validation passes 504 GPU-enabled client tests (one ignored), 22 CLI tests,
29 LinuxKMS tests (one ignored), two startup-isolation tests, software-kiosk
and desktop checks, the ARM release build, formatting and diff checks. The
mesh tests cover concave winding, translucent fill/stroke order, shared edges,
degenerate/offscreen geometry, storage reuse and the eight Clock fixtures at
both viewport sizes. Strict Clippy is not clean: it reports existing warnings
in scenario/client code; the new renderer's two style warnings were corrected.

The collector restores the normal software Clock with unchanged saved settings
and boot ID, and zero automatic service restarts. No other device is touched.
The final lint-only cleanup build, client `a9bd3f73f8f2`, repeats the full
device smoke checks and exact paused-image comparisons successfully; the timing
table above remains from `a9755efef2e1`. The deployed final build is back on normal
software Clock at 60 FPS/UPS with no direct target resident.
Scripts, raw results, screenshots and their validator are retained under
`target/gpu-experiments/picade-triangle-batch.Z98xBbIi/`.

This is a positive result for the tested Clock renderer boundary. Keep it opt-in
while checking the other Clock effects on hardware, resize/error cleanup and
the menu-border issue. Then extend the adapter to multiple viewports, clipped
translucent minimaps and Spacewars; do not assume that passing a single full-screen
Clock validates those paths or the rotated Pi 5 HyperPixel.

## Uncapped geometry capacity lab

The `--presentation-geometry` probe replays frozen scenario frames through the
same ordered triangle renderer as the live prototype. It opens only an explicit
EGL render node, rejects software GL, and does not open a window, load settings,
start IPC, swap buffers, or wait for a display refresh. This measures scene
renderer capacity, **not complete game FPS**.

Build with `--no-default-features --features pi-kiosk-gpu`, then run a release
binary on a quiet machine:

```sh
./engine-client --benchmark-presentation --presentation-geometry \
  --geometry-device /dev/dri/renderD128 \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 120 --presentation-repeats 4 \
  --geometry-output geometry-captures > geometry.csv 2> geometry.log
```

The output directory must not exist. It receives the driver/configuration
manifest, frozen input frames, and paired GPU/software PNGs. CSV goes to stdout;
progress and GPU timer availability go to stderr. Use `--geometry-cases` with a
comma-separated subset of:

- `clock-idle`, `clock-rain`: seed 7, default benchmark settings, frozen at tick
  600. Rain includes wet digits, flowing water, and the passive rubber duck.
- `pizza-300`, `pizza-1000`, `pizza-5000`, `pizza-10000`: seed 7, dense Rapier/Fast
  benchmark fixture, frozen after 120 steps. Physics runs only during setup.
- `overlap-1`, `overlap-8`, `overlap-32`, `overlap-128`: full-screen transparent
  polygons in alternating colors. This separately stresses pixel work and blend
  order, rather than increasing simulation load.
- `mixed-grouped`, `mixed-interleaved`: 32 precomputed animation frames with 192
  circles, 192 concave polygons and 192 lines. Positions, polygon shape/rotation,
  circle radii/colors and the camera change. One fixture groups circles ahead of
  polygons; the other interleaves them to stress order-preserving submission.
- `moving-circles`: 32 frames of 1024 moving/resizing circles, including fill-only,
  outline-only and translucent filled/outlined styles, plus camera pan and zoom.

The default `--geometry-modes triangles` projects, tessellates, uploads, draws,
and resolves geometry again every frame. There is no frozen-scene shortcut.
Use `--geometry-modes triangles,circles` for paired blocks; the optional circle
mode retains templates but still updates and uploads all instances every frame.
Sixteen warmup frames precede each
measured block; alternating forward/reverse case order helps expose ordering
effects. Paired mode order alternates with case position and repeat too.
Native 1× geometry and four-sample MSAA match the production prototype.
Text is removed from both inputs and counted explicitly; Slint UI, scene
construction by the simulation, physics, readback, and PNG encoding are excluded.

The completion queue holds at most `--geometry-in-flight` frames (default 3,
range 1–8). Each frame has a fence; all fences drain before the block ends.
`completed_wall_avg_ms` therefore includes finished GPU work, not just enqueue
time. CPU stage columns separate projection, tessellation, submission, and queue
waits; per-frame wall percentiles are p50/p95/p99. `cpu_frame` excludes queue
waits, while `total_thread_cpu` includes them. These overlapping stage and total
columns must not all be added together.

Elapsed GPU queries are collected after their completion fences. Missing timer
support yields empty GPU columns, not zero; a disjoint indication invalidates
the entire block's GPU samples. Compare with `--geometry-no-gpu-timing` to check
instrumentation cost. The elapsed query includes upload/draw/resolve work; it is
not a claim about GPU utilization. See the
[Khronos timer-query specification](https://registry.khronos.org/OpenGL/extensions/EXT/EXT_disjoint_timer_query.txt).

Counts include mean scene draw calls and uploaded geometry bytes; `vertices` and
`indices` are streamed triangle counts for the last measured frame, excluding
shared circle templates. `circle_instances` is the last frame's instance count;
`circle_template_vertices_per_frame` counts template vertices times instances,
averaged over the block, not measured GPU shader invocations. It exposes extra
vertex work from using the highest detail level of each consecutive circle run.
`resident_circle_template_bytes` reports the fixed template vertex/index payload,
not driver allocation; it stays resident when switching back to triangles.
`process_rss_kib` includes the process, retained fixtures, and allocator/driver
state; it is not isolated renderer memory. `nominal_attachment_bytes` counts the
RGBA target and MSAA samples only, not actual driver/GPU allocation. Captures
must remain stable across repeats and pass a coarse software-reference check;
inspect the pairs as well because antialiasing is intentionally different.

Dense Pizza circles reveal a specific reference limitation: the native software
renderer rounds scanline ends and stroke widths to whole pixels. Its dark
outlines cover a larger fraction of a tiny circle than a centered GPU/MSAA
stroke. The lab retains that native pair and its `rgb_mae`/difference fraction,
but validates Pizza against an additional software image rendered at 4× width
and height, with stroke widths scaled too, then box-filtered to native size.
`reference_scale` identifies this check, and `reference_*` columns report its
errors. Reference rendering is untimed; it does not change the benchmark's
native resolution or geometry. Clock and overlap retain their native reference.
The animated synthetic fixtures also use the scaled-stroke 4× reference, now
supporting polygon and line primitives. Every animated frame is additionally
compared across GPU modes outside timing before accepting benchmark results.
Mode comparisons use the existing coarse image gate (RGB MAE at most 6 and at
most 20% of pixels with summed RGB error above 30), not pixel equality between
different circle tessellations. CSV reports direct baseline error as well as
software-reference error. Captures include the mode in their filenames; the
input checksum covers the entire sequence, while JSON/PNG captures describe its
last measured frame. This is still a coarse check plus visual inspection, not a
proof of equivalence for every possible scene.

Spacewars is deliberately not included yet: the prototype does not support its
multiple viewports and clipped translucent minimaps. Its earlier CPU raster
measurements are not evidence of triangle-renderer capacity.

### Capacity results on Pi 4 and workstation

Measured 2026-10-02 with 120 frames per block, four alternating-order repeats,
three frames in flight, and native 1024×768 output. Values below are medians of
the four block averages, in milliseconds per frame. The workstation is a Ryzen
7 9800X3D / Radeon RX 7800 XT, Mesa 26.0.8; the Pi is `sw-picade-2`, V3D 4.2.14.0,
Mesa 25.1.6. These are release builds. The Pi's installed kiosk was suspended
after pausing, preventing its continuous redraws from competing with the probe.

| Frozen workload | Pi completed | Pi tessellation CPU | Workstation completed | Workstation GPU elapsed |
| --- | ---: | ---: | ---: | ---: |
| Clock idle | 5.16 | 1.50 | 0.158 | 0.054 |
| Clock rain | 4.56 | 2.77 | 0.406 | 0.061 |
| Pizza 300 | 4.13 | 3.00 | 0.427 | 0.047 |
| Pizza 1000 | 9.99 | 8.40 | 1.124 | 0.087 |
| Pizza 5000 | 34.20 | 29.84 | 4.160 | 0.256 |
| Pizza 10000 | 61.14 | 53.44 | 7.598 | 0.447 |
| Transparent layers 1 | 5.83 | 0.013 | 0.027 | 0.025 |
| Transparent layers 8 | 9.57 | 0.037 | 0.097 | 0.093 |
| Transparent layers 32 | 21.82 | 0.114 | 0.256 | 0.249 |
| Transparent layers 128 | 70.74 | 0.427 | 0.880 | 0.837 |

The Pi driver does **not** advertise `EXT_disjoint_timer_query`. Its GPU elapsed
columns are unavailable, not inferred by subtracting CPU time from wall time.
Completed throughput still waits for real fences. GPU elapsed and CPU work can
overlap on the workstation, so they are not additive parts of the completed
time. Its query-disabled control gives the same bottleneck pattern; 10000 balls
take 7.39 ms without queries versus 7.60 ms with them. This is a control run, not
a precise causal estimate of timer overhead.

Two limits are now distinguishable:

- **Dense circles are CPU dominated.** On Pi, 10000 balls spend 53.44 of 61.14 ms
  in tessellation; total render-thread CPU is 60.87 ms. That frame uploads
  356976 vertices and 950928 indices, 12.37 MB, despite using just one scene draw
  plus resolve. Workstation tessellation alone takes 7.00 ms. The Pi block
  averages range from 60.89–61.37 ms; median per-block CPU-frame p95 is 61.34 ms.
  Rebuilding generalized circle meshes every frame is the next concrete target,
  with instanced/analytic circles a candidate to test against this baseline.
- **Deep transparent overlap is pixel limited.** On Pi, 128 full-screen layers
  use 1.36 ms of thread CPU but take 70.74 ms to complete. At 512×384, completion
  falls to 17.78 ms, a 3.98× improvement for one-quarter the pixels. The 32-layer
  case similarly falls from 21.82 to 5.41 ms. Clock rain changes only from 4.56
  to 4.04 ms. Reducing geometry work and reducing pixel work are different jobs;
  a single display-capped FPS number hides that distinction.

All frozen input checksums and mesh counts match between machines. Each GPU's
output is stable across repeats; the paired captures pass the image checks.
The 10000-circle Pi image has native-raster RGB MAE 16.06, versus 4.21 against
the supersampled reference, consistent with the inspected outline difference.
These are geometry-only results, not a new software-versus-GPU whole-app race or
a prediction that a 10000-body simulation runs at the reciprocal frame time.

The Picade resumed ordinary software Clock at 60 FPS / 60 UPS. Settings, installed
binaries, boot ID, and service PID were unchanged; no service restart or reboot
was needed. The probe used a temporary binary, not a kiosk installation.
Before/after power-alarm reads were zero, and the monitored temperatures were
approximately 65–70°C; no governor was changed.

Raw CSVs, manifests, captures, and the restoring device runner are retained at
`target/gpu-experiments/geometry-capacity.EZRx4Qzz/`; successful matrices are
`host-native-final.csv`, `host-no-query.csv`, `host-quarter.csv`,
`picade/native.csv`, and `picade/quarter-pixels.csv`. Earlier smoke/failing native
edge comparisons are retained separately, not used as performance results.
Benchmark binary SHA-256 prefixes: workstation `3ab4a3a7feb8`, Pi `6897e74c9281`.

Verification for that baseline: 512 client tests passed, one existing test
ignored; three startup-isolation tests passed; desktop and software-kiosk
checks passed. GPU clippy retained 26 pre-existing warnings, none in the new
probe modules.

## Switchable shared circle experiment

The new `circles` mode tests reuse inside the renderer without adding a generic
geometry cache or changing `RenderFrame` or scenarios. Software remains the
default; `on` continues to mean the original triangle mode. On an active Clock
using the GPU kiosk build and FemtoVG presentation, switch the session with:

```sh
spacewars-cli host gpu-scene triangles
spacewars-cli host gpu-scene circles
spacewars-cli host gpu-scene off
```

Changes apply from the next frame, are not saved, and reset with the session.
Status reports requested/applied mode, streamed geometry bytes, draw counts,
circle instances, and resident template payload. The standalone lab uses:

```sh
./engine-client --benchmark-presentation --presentation-geometry \
  --geometry-modes triangles,circles \
  --geometry-cases clock-idle,clock-rain,mixed-grouped,mixed-interleaved,moving-circles,pizza-1000,pizza-10000,overlap-32 \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 120 --presentation-repeats 4 \
  --geometry-output circle-comparison > circle-comparison.csv
```

Eight circle templates (8 through 1024 segments) occupy 146976 bytes of explicit
vertex/index payload. They upload once on first use in the owning GL context;
each circle then streams a 48-byte placement/style record. A conservative
screen-space sagitta bound selects detail for at most 1/8-pixel geometric error.
Circles too large for that bound use ordinary tessellation instead. Fill precedes
stroke, outlines remain logical-pixel widths, and consecutive circles use the
highest detail needed by that run. This may do more GPU vertex work for small
circles next to large ones; it is a deliberate measurable tradeoff, not an
assumption that retained geometry always wins.

Polygons and lines retain the ordinary tessellated path. Source order is never
changed: alternating a circle and a polygon creates additional draws. The
interleaved synthetic fixture makes that cost visible. Template storage is
bounded, instance count is capped at 100000, and resources die with their GL
context. No per-object cache grows as entities move, spawn, or disappear.

The benchmarks distinguish actual Clock snapshots, synthetic changing inputs,
and the extreme Pizza case. They do not establish the most demanding future game
scene, complete simulation speed, or Spacewars GPU readiness. Spacewars still
needs multiple viewports and minimap clipping/compositing before live use.

### Pi comparison results

Measured on `sw-picade-2` with the same V3D driver, native 1024×768 output,
four-sample MSAA, 120 frames per block, four repeats and three frames in flight.
These are medians of completed renderer-frame block averages, in milliseconds;
physics and UI are excluded. The existing kiosk was temporarily paused and
suspended so its rendering did not compete with the offscreen probe.

| Workload | Triangles baseline | Shared circles |
| --- | ---: | ---: |
| Clock idle | 5.17 | 5.42 |
| Clock rain | 4.55 | 4.85 |
| Animated mixed grouped | 3.34 | 4.06 |
| Animated mixed interleaved | 3.31 | 8.13 |
| Animated 1024 circles | 7.25 | 6.99 |
| Pizza 300 | 4.15 | 6.98 |
| Pizza 1000 | 10.10 | 7.44 |
| Pizza 5000 | 34.29 | 14.32 |
| Pizza 10000 | 61.03 | 16.62 |
| Transparent layers 1 | 5.83 | 5.87 |
| Transparent layers 8 | 9.52 | 9.56 |
| Transparent layers 32 | 21.82 | 21.84 |
| Transparent layers 128 | 70.79 | 70.77 |

**Keep the mode experimental, not the default.** It is a large improvement for
extreme circle populations, but does not improve Clock and regresses smaller or
mixed workloads on this Pi. These differences persist across repeats: Clock
rain is 4.54–4.56 versus 4.82–4.86 ms; mixed interleaved is 3.30–3.34 versus
8.11–8.31 ms; Pizza 10000 is 60.87–61.09 versus 16.44–16.67 ms.

For Pizza 10000, CPU mesh preparation falls from 53.30 to 1.01 ms and per-frame
geometry uploads from 12.37 MB to 0.48 MB. Total render-thread CPU falls from
60.74 to 4.72 ms, but completion is still 16.62 ms. Removing CPU tessellation
does not remove downstream rendering costs. The instanced run references
490000 template vertices, versus 356976 baseline streamed vertices; these are
nominal geometry counts, not GPU counter measurements.

Mixed interleaved preserves ordering by issuing 384 draws instead of one;
submission now dominates its CPU time. Even mixed grouped gets slower despite
reducing CPU work and using only two draws. Shared-template vertex/shader and
driver costs need separate investigation; unavailable Pi GPU timers prevent an
exact GPU-stage attribution. Transparent polygon-only controls are essentially
unchanged, as expected. This is evidence against assuming geometry reuse alone
will improve every workload, not evidence against a GPU renderer generally.

All 104 Pi and 104 workstation rows passed the existing software-reference
checks, with stable captures per mode across repeats and matching input
checksums between machines. The Pi's maximum software-reference RGB MAE is
4.383, and maximum direct mode-to-mode MAE is 2.206; outlines/antialiasing are
not byte-identical. All 32 frames of each of the three synthetic animations
passed mode comparisons; Clock rain and interleaved captures were inspected on
both GPUs. The workstation had concurrent unrelated CPU workloads, so its raw
timings are retained but are not presented as controlled capacity measurements.

Artifacts, captures, CSVs and the restoring runner are under
`target/gpu-experiments/circle-comparison.8ejHvNVn/`. Host binary SHA-256 prefix:
`41a37eedea65`; Pi: `e6ec0e13369e`. The Pi has no elapsed GPU queries; its GPU-time
columns remain empty. Monitored temperatures were approximately 66–70°C and
power-alarm reads were zero. The installed binary, settings, boot ID and kiosk
PID remained unchanged. Normal software Clock resumed at 60 FPS / 60 UPS.

Verification: 517 client tests passed with one existing ignored test, 22 CLI
tests passed, and three startup-isolation tests passed. Desktop and software
kiosk checks, GPU release builds for host/Pi, formatting and diff checks passed.
GPU clippy reports the same 26 pre-existing client warnings, none in the new
circle or geometry-lab code. Live borrowed-texture UI switching on the device
has not been manually retested in this iteration; the production draw path and
mode switching were exercised offscreen without replacing the installed app.

## Mixed scene diagnostic controls

Two additional `--geometry-modes` values separate draw fragmentation from the
instanced-template path. They are offscreen benchmark controls, not live IPC
modes or new defaults:

- `triangles-split` keeps the baseline Lyon vertices, indices, shaders, upload
  bytes and source order. It only splits draws at the circle/non-circle run
  boundaries used by instancing. Captures must be byte-identical to the baseline,
  including every frame of each selected synthetic animation.
- `circles-expanded` uses the same cached circle templates and highest detail
  level per consecutive run as `circles`, but transforms their vertices on the
  CPU and combines them with other geometry into one ordinary triangle draw.
  Unused fill/stroke triangles are retained to avoid changing template geometry
  in this comparison. This exchanges instancing and shader work for CPU
  transforms and larger uploads; it does not isolate one GPU instruction cost.

The existing `triangles` and `circles` modes remain unchanged in intent. Compare
all four with the same inputs, completion fences and image checks:

```sh
./engine-client --benchmark-presentation --presentation-geometry \
  --geometry-modes triangles,triangles-split,circles,circles-expanded \
  --geometry-cases clock-idle,clock-rain,mixed-grouped,mixed-interleaved,moving-circles,pizza-300,pizza-1000,pizza-10000,overlap-32 \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 120 --presentation-repeats 4 \
  --geometry-output mixed-controls > mixed-controls.csv
```

Expansion retains a bounded CPU copy of the eight templates and reusable index
scratch storage. It enforces the same one-million-vertex/three-million-index
stream budget before expansion; exceptionally large populations may therefore
be supported by instancing but rejected by this diagnostic control. Expanded
vertices appear in the ordinary streamed counts and upload bytes, not the
instance counters. The resident-template CSV field still refers to GPU payload,
not these CPU allocations. Mode changes clear geometry while retaining bounded
scratch capacity, and an empty scene must not retain old draws.

### Pi results from the diagnostic controls

The controls rule out draw count alone as the cause of the mixed-scene slowdown.
Keep the original triangle batch as the general reference; neither template
variant improves the tested Pi Clock rain or mixed workloads. Instancing remains
useful for very large circle populations, not an unconditional renderer policy.

Measured on the same Pi 4, V3D 4.2.14.0 / Mesa 25.1.6, at 1024×768 with four-sample
MSAA, three frames in flight, 16 warmup frames and four 120-frame measured blocks
per case/mode. Values are medians of completed renderer-frame block averages,
in milliseconds. These exclude physics, UI, scanout and reference captures.

| Workload | Triangles | Split triangles | Instanced templates | Expanded templates |
| --- | ---: | ---: | ---: | ---: |
| Clock idle | 5.16 | 5.17 | 5.44 | 5.13 |
| Clock rain | 4.57 | 4.56 | 4.82 | 5.21 |
| Animated mixed grouped | 3.35 | 3.35 | 4.04 | 3.75 |
| Animated mixed interleaved | 3.32 | 3.77 | 8.23 | 3.73 |
| Animated 1024 circles | 7.14 | 7.20 | 6.95 | 7.84 |
| Pizza 300 | 4.14 | 4.13 | 6.97 | 5.05 |
| Pizza 1000 | 10.01 | 10.06 | 7.46 | 7.00 |
| Pizza 10000 | 61.82 | 62.21 | 16.62 | 28.00 |
| Transparent layers 32 | 21.79 | 21.80 | 21.80 | 21.79 |

In mixed interleaved, changing only draw boundaries from one to 384 increases
completion by approximately 0.45 ms, not the instanced path's 4.91 ms increase.
CPU submission is 0.71 ms for baseline, 1.21 ms for split triangles, and 6.83 ms
for instanced circles. Total thread CPU for the latter is 8.12 ms, very close to
its 8.23 ms completion. The leading follow-up is therefore the cost of switching
programs/vertex layouts and setting instance attributes in this driver path,
not simply reducing a draw counter. This comparison has not isolated which
individual GL state operation is responsible.

Expansion brings that mixed case back to 3.73 ms while preserving the same
template geometry. However, it streams 742464 bytes per frame instead of the
baseline's 417168 or instancing's 175104. CPU mesh work falls from 2.15 to 1.67 ms
relative to baseline, but submission grows from 0.71 to 1.34 ms. Removing repeated
tessellation is not enough when expansion and transfers consume the savings.

There is also a separate single-batch issue: Pizza 300 takes 6.97 ms instanced
despite only 0.55 ms of total thread CPU. Expanding those same templates takes
5.05 ms, including 4.69 ms of CPU, and produces identical captured pixels. Both
paths have one draw. This implicates the instanced shader/attribute/driver path
beyond mere template triangle count, without establishing an exact GPU-stage
cause. Compare leaner instance data/shaders separately from mixed state changes.
Pi GPU elapsed-query support is still absent; do not label wall-minus-CPU as
measured GPU time. The polygon-only control is unchanged within repeat noise.

The workstation's RX 7800 XT behaves differently. In the final rerun, grouped
mixed frames fall from 0.305 to 0.167 ms with instancing; Pizza 300 falls from
0.420 to 0.050 ms. Mixed interleaved is 0.317 ms baseline, 0.344 split, 0.338
instanced and 0.225 expanded. Its valid elapsed GPU queries show 0.037, 0.031,
0.139 and 0.044 ms respectively for that mixed case. Builds were finished before
this workstation run; ordinary desktop applications were still active. These
results are useful cross-device evidence, not a reason to substitute desktop
performance for Pi measurements.

All 144 Pi rows and 144 final workstation rows passed the software-reference
gate and repeat-stability checks, with matching input checksums across devices.
Splitting preserves exact baseline geometry, uploads and pixels. The captured
Pi instanced/expanded pairs are also byte-identical in all nine cases. All 32
frames of each of the three synthetic animations passed baseline comparisons.
Clock rain and mixed interleaved captures were visually inspected. The maximum
Pi software-reference RGB MAE is 4.383, and changed-pixel fraction is 0.1372;
these remain coarse checks, not claims of pixel equivalence to software.

Artifacts are under `target/gpu-experiments/mixed-ablation.LiBp0BnN/`:
`picade/native.csv` is the four-repeat Pi comparison; `host-final.csv` is the
final four-repeat workstation comparison. The earlier host run and initial
smoke captures are retained separately. Timed Pi binary SHA prefix:
`af50d9ba18c9`; final host: `d298153fa174`. After a semantics-preserving lint
cleanup of nested conditions, the final Pi binary (`dbccb49c6300`) passed an
additional four-case, 32-frame smoke run in `picade-final/`; do not mix its
single-repeat timings into the table. Both runs use temporary executables, not
an installed application update. The restoring runner checks settings/binary
hashes, boot ID and kiosk PID. Temperatures observed during the full run were
approximately 68–71°C, power-alarm reads were zero, and the original Clock
resumed with the same service process and settings.

Verification after the cleanup: 522 client tests and three startup-isolation
tests passed, with one existing ignored client test. GPU release builds passed
for host and Pi; the software kiosk check, formatting and diff checks passed.
GPU clippy reports the same 26 pre-existing client warnings, none in these new
diagnostic controls. There are no changes to the normal rendering default,
scenario APIs, or live geometry-mode commands in this iteration.

## Submission state controls

These controls keep mesh generation, shaders, draw order, instance records
and uploads fixed while changing how draw state is supplied. These remain
offscreen-only `--geometry-modes` choices:

- `triangles-programs`: the same mesh and split batches as `triangles-split`,
  but alternate between two programs linked from the identical vertex/fragment
  shaders. The VAO and vertex layout do not change. This tests program-object
  switching without introducing a different shader; it is not a full proxy for
  switching between the actual circle and triangle shader implementations.
- `circles-uniforms`: the original instanced geometry and attribute-pointer
  setup, with viewport uniforms set once per program per frame instead of again
  at every batch. Program and VAO switches remain.
- `circles-bindings`: the original uniforms and shaders, but a separate owned
  VAO fixes the attribute formats and associates the three instance attributes
  with one buffer binding. Each circle batch changes one buffer offset instead
  of repeating three attribute-pointer definitions. The
  [Khronos vertex binding specification](https://registry.khronos.org/OpenGL/extensions/ARB/ARB_vertex_attrib_binding.txt)
  describes this separation of format from buffer binding. This control requires
  GLES 3.1, GL 4.3, or the desktop ARB extension; unsupported contexts fail
  explicitly. There is one extra VAO, not a growing cache of per-object VAOs.
- `circles-submission`: combines the uniform and buffer-binding changes.

Each new circle control must produce byte-identical pixels to `circles`, not
merely pass the coarse comparison with the differently tessellated baseline.
The program control must match `triangles` exactly. The probe verifies all 32
frames of selected synthetic animations and native/half/native target sizes
while switching modes, outside measurement timers. Each measured block also
checks its final capture against the appropriate reference. Unit tests require
unchanged vertices, indices, instances, batches and uploads for the controls.
Normal renderer defaults and live IPC modes remain unchanged.

```sh
./engine-client --benchmark-presentation --presentation-geometry \
  --geometry-modes triangles,triangles-split,triangles-programs,circles,circles-uniforms,circles-bindings,circles-submission \
  --geometry-cases clock-rain,mixed-grouped,mixed-interleaved,moving-circles,pizza-300,pizza-1000,overlap-8 \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 120 --presentation-repeats 4 \
  --geometry-output submission-controls > submission-controls.csv
```

### Results: program switching is expensive on Pi

On the Pi 4, switching between two programs linked from identical triangle
shaders reproduces most of the mixed-scene CPU regression. Reducing uniform
updates and attribute-pointer calls provides a much smaller improvement. This
is evidence about the driver submission path, not proof of a particular Mesa
function or a measured GPU-stage cost.

The seven-mode comparison uses the same native 1024×768 offscreen method as
above: four-sample MSAA, three frames in flight, 16 warmup frames, four 120-frame
blocks, alternating case/mode order, no physics/UI/scanout timing. Values below
are medians of block averages in milliseconds for animated mixed interleaved:

| Mode | Completed frame | Total thread CPU | Submission CPU |
| --- | ---: | ---: | ---: |
| `triangles` | 3.287 | 3.190 | 0.693 |
| `triangles-split` | 3.708 | 3.638 | 1.181 |
| `triangles-programs` | 8.692 | 8.593 | 6.129 |
| `circles` | 8.176 | 8.109 | 6.831 |
| `circles-uniforms` | 8.019 | 7.952 | 6.673 |
| `circles-bindings` | 8.067 | 8.001 | 6.722 |
| `circles-submission` | 7.890 | 7.817 | 6.539 |

`triangles-programs` and `triangles-split` have identical mesh, uploads, vertex
layout, 384 draws and captured pixels. Program-object changes alone add about
4.98 ms to completion in that controlled pair. Combining both circle submission
changes recovers only about 0.29 ms, and leaves the separate single-batch problem
unchanged: Pizza 300 is 6.976 ms with `circles` and 6.980 ms with
`circles-submission`, versus 4.183 ms with streamed triangles. The combined
control also leaves Clock rain near 4.8 ms versus about 4.55 ms baseline.

All 196 rows on each device passed the reference and repeat-stability checks,
including exact circle-control and split/program-control comparisons. All 32
frames of the selected synthetic animations and native/half/native resize
checks passed. The workstation does not reproduce the Pi's large submission CPU
cost: its mixed-case submission CPU is 0.026 ms for split triangles and 0.031 ms
for alternating programs. Device-specific results remain necessary.

Artifacts: `target/gpu-experiments/submission-controls.GhKcrnGK/`, with
`picade/native.csv` and `host-native.csv` for the complete four-repeat runs. The
earlier smoke and partial-progress files are not the reported comparison. Pi
binary SHA prefix: `0e3c69ea3682`; host: `029ebaee0a60`. The restoring runner
verified unchanged installed binary/settings, boot ID and kiosk PID; the
original Clock resumed. Before/after temperatures were approximately 68–70°C
and power-alarm reads were zero.

### Shared-program follow-up

`circles-unified` retains the `circles-submission` mesh, instance records,
uploads and batch order, but uses one program for both streamed triangles and
instanced circles. The program is selected once before the batches. Circle
template vertices carry a nonzero role tag; the existing vec2 triangle position
attribute supplies an implicit zero z tag. Thus no per-frame vertex bytes or
per-batch mode uniforms are added. Circle/triangle VAO changes, draw boundaries
and the circle shader's radius/color work remain. This is a focused experiment,
not a new scenario-facing geometry API or an automatic renderer policy.

The original and unified templates can coexist when switching comparison modes:
their total resident GPU payload is bounded at 293952 bytes (two copies of
146976 bytes). The CSV reports both retained copies, including when a later
mode does not use them; it does not claim to measure driver allocations. The
unified path uses the same binding API capability gate and exact-pixel checks
as the submission controls, including the polygon-only fixture.

```sh
./engine-client --benchmark-presentation --presentation-geometry \
  --geometry-modes triangles,circles,circles-submission,circles-unified \
  --geometry-cases clock-rain,mixed-grouped,mixed-interleaved,moving-circles,pizza-300,pizza-1000,overlap-8 \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 120 --presentation-repeats 4 \
  --geometry-output unified-controls > unified-controls.csv
```

The shared program removes a substantial part of the mixed-scene penalty, but
does not make instancing the general Pi winner. Same method and device as the
submission controls; this is a separate paired run of the final executable:

| Workload | Triangles | Circles | Combined submission control | Shared program |
| --- | ---: | ---: | ---: | ---: |
| Clock rain | 4.584 | 4.858 | 4.837 | 4.999 |
| Animated mixed grouped | 3.343 | 4.034 | 4.025 | 4.047 |
| Animated mixed interleaved | 3.328 | 8.182 | 7.906 | 5.301 |
| Animated 1024 circles | 7.182 | 6.937 | 6.995 | 7.032 |
| Pizza 300 | 4.163 | 6.979 | 7.005 | 7.042 |
| Pizza 1000 | 10.001 | 7.471 | 7.467 | 7.517 |
| Transparent layers 8 | 9.467 | 9.465 | 9.482 | 9.498 |

Mixed interleaved retains 384 draws and 175104 uploaded bytes per frame in all
three circle modes. Sharing the program lowers completion by about 35% versus
the original circle path, with submission CPU falling from 6.840 to 3.970 ms and
total thread CPU from 8.098 to 5.229 ms. Its four completion block averages span
5.290–5.877 ms, versus 8.160–8.202 ms for the original circle path; the improvement
is not dependent on dropping the slower repeat. Triangle baseline remains
faster at 3.328 ms, including 3.180 ms of CPU.

The result does not remove the single-batch circle cost or establish a Clock
improvement. Shared-program Clock rain is slightly slower in this run, and
Pizza 300 still takes about 7 ms despite only 0.522 ms of total thread CPU. Pi
elapsed GPU queries are unavailable; the completion/CPU gap is not a GPU timer.
The next mixed-scene control should isolate the remaining vertex-input/instance
state and driver work, not assume that one program means one internal driver
pipeline. Investigate the single-batch vertex/shader path separately. These
results favor a common submission path for future reusable geometry while
retaining streamed batching; they do not justify a global cache or per-object
draw policy yet.

On the workstation, this paired run reduces mixed interleaved from 0.417 ms for
`circles` to 0.250 ms for `circles-unified` (triangle baseline 0.332 ms). Valid GPU
queries fall from 0.190 to 0.101 ms. Its shared-program completion repeats span
0.221–0.294 ms, with normal desktop applications still active; do not interpret
small desktop differences or results from separate runs as controlled gains.

All 112 Pi rows and 112 workstation rows pass the reference gates and repeat
stability checks. Shared-program captures are byte-identical to the original
circle renderer in all seven cases; the 32-frame animations and resize checks
also pass. Inputs match across devices. Pi and host Clock rain and mixed captures
were visually inspected, including the transparent overlaps and water seams.
The coarse software-reference maximum RGB MAE is 7.276 on Pi (changed-pixel
fraction 0.1694); the exact mode-to-mode check is separate from this different
tessellation/antialiasing comparison.

Final artifacts: `target/gpu-experiments/submission-controls.GhKcrnGK/unified/`,
with `picade/native.csv`, `host-native.csv`, captures and manifests. Pi binary
SHA prefix: `40a7780c00e6`; host: `aeccb4426156`. The temporary remote runner again
verified unchanged settings, installed binary, boot ID and kiosk PID. Clock
resumed at 60 FPS/UPS using software; before/after temperatures were about
67–70°C, power-alarm reads zero, and the service did not restart. Nothing was
installed or made the new default.

Final verification: 524 client tests and three startup-isolation tests passed
(one existing client test ignored), host/Pi release builds and software kiosk
check passed, formatting/diff checks passed. Clippy retains the existing 26
client warnings (35 in the test target including duplicates), with none in the
new controls. All five new experimental modes remain selectable in the
offscreen probe only; the scenario interface and live IPC choices are unchanged.

## Vertex input controls

Three further offscreen modes test why sharing a shader program did not remove
all mixed-scene submission cost. Each retains the original instance records,
streamed triangles, source order, template topology, LOD selection and draw
boundaries, and must pass the same exact-pixel, animation and resize checks:

- `circles-unified-instanced` changes only the shared-program triangle draw
  command to an instanced draw with one instance. The program, VAOs, attribute
  formats and buffers remain those of `circles-unified`. This isolates the draw
  API choice rather than attributing its cost to vertex-layout changes.
- `circles-unified-layout` uses the same attribute enable mask, formats, binding
  associations, strides and divisors for both kinds of geometry, with two VAOs.
  Every draw uses the instanced API. Both consume a 24-byte xy/rgba vertex plus
  a 48-byte instance binding. Triangles use their unchanged existing vertex
  stream and a static dummy instance; circle templates encode their role in a
  negative alpha slot. Projected colors are clamped to [0, 1], so the tags cannot
  collide with triangle colors. One shared shader interprets these inputs.
- `circles-unified-vao` uses the exact shader and representation of the layout
  control but keeps one VAO bound through all batches. It changes buffer bindings
  and the index buffer for each draw instead of switching between the two VAOs.
  This is not automatically fewer GL calls; it isolates VAO switching from
  changing the underlying buffers with a stable format.

The fixed-layout representation changes shader input fetches as well as layout
state; it is not a one-GL-call ablation against the earlier shared shader. The
two fixed-layout modes, however, use the same resources and shader, making their
comparison a narrower control. The original API/format variants remain available.
The [Khronos binding specification](https://registry.khronos.org/OpenGL/extensions/ARB/ARB_vertex_attrib_binding.txt)
defines the separation of attribute formats, buffer bindings and divisors used
here; none of these choices is assumed to be equally cheap on every driver.

Per-frame upload bytes are unchanged. The fixed-layout templates take 220512
resident payload bytes instead of 146976 because circle vertices grow from 12
to 24 bytes; there is also one static 48-byte dummy instance. Selecting all
comparison paths can retain three template sets totaling 514464 bytes, reported
by the existing template counter (which excludes the dummy record and driver
overhead). There are no per-object VAOs or persistent per-entity cache entries.
Unit tests check role encoding, template payload size and unchanged mesh/batches.

```sh
./engine-client --benchmark-presentation --presentation-geometry \
  --geometry-modes triangles,triangles-split,circles,circles-unified,circles-unified-instanced,circles-unified-layout,circles-unified-vao \
  --geometry-cases clock-rain,mixed-grouped,mixed-interleaved,moving-circles,pizza-300,pizza-1000,overlap-8 \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 120 --presentation-repeats 4 \
  --geometry-output vertex-input-controls > vertex-input-controls.csv
```

### Results from the common vertex layout

The common layout removes much more of the remaining mixed-scene cost than
changing the draw API alone. Keeping one VAO bound offers no clear additional
win. On Pi 4 / V3D 4.2.14.0 / Mesa 25.1.6, at 1024×768, four-sample MSAA and
three frames in flight, the four 120-frame blocks give these median averages
for mixed interleaved (milliseconds; 16 warmup frames, simulation/UI/scanout
excluded):

| Mode | Completed frame | Total thread CPU | Submission CPU |
| --- | ---: | ---: | ---: |
| `triangles` | 3.296 | 3.180 | 0.693 |
| `triangles-split` | 3.696 | 3.628 | 1.173 |
| `circles` | 8.097 | 8.029 | 6.772 |
| `circles-unified` | 5.276 | 5.197 | 3.938 |
| `circles-unified-instanced` | 5.221 | 5.157 | 3.896 |
| `circles-unified-layout` | 3.665 | 3.613 | 2.362 |
| `circles-unified-vao` | 3.706 | 3.634 | 2.374 |

All circle variants still issue 384 draws and upload 175104 bytes per frame.
Their streamed vertices/indices, instances and template vertex-work counters
match exactly within each workload. The common layout lowers completion about
55% from the original circle path, but remains about 11% slower than the single
triangle batch. It saves CPU tessellation (about 2.17 to 1.05 ms), while its
submission CPU still exceeds the baseline (about 0.69 to 2.36 ms). This is not
evidence that the entire remaining difference is GPU execution or a specific
Mesa function.

The comparison across other workloads still argues against a global policy:

| Workload | Triangles | Shared program | Common layout | Single VAO |
| --- | ---: | ---: | ---: | ---: |
| Clock rain | 4.545 | 4.969 | 4.998 | 5.028 |
| Animated mixed grouped | 3.327 | 4.082 | 4.086 | 4.081 |
| Animated 1024 circles | 7.138 | 7.074 | 7.207 | 7.194 |
| Pizza 300 | 4.171 | 6.998 | 7.119 | 7.112 |
| Pizza 1000 | 9.949 | 7.525 | 7.681 | 7.689 |
| Transparent layers 8 | 9.556 | 9.534 | 9.320 | 9.544 |

The polygon-only layout repeats span 9.079–9.547 ms, overlapping the baseline's
9.484–9.610 ms; do not treat that small median difference as an established
gain. The single-batch circle cases still show the separate completion/CPU gap
and modestly regress with the larger template inputs. Pi elapsed GPU queries
remain unavailable. The grouped/interleaved fixtures must each be compared to
their own reference: changing source order also changes consecutive-run LOD
choices, so they are not interchangeable geometry controls.

On the workstation, mixed interleaved completes in 0.318 ms baseline, 0.348 ms
with original circles, 0.215 ms with the shared program, 0.216 ms with the draw
API control, 0.212 ms with the common layout and 0.209 ms with one VAO. Valid GPU
queries for the shared-program and common-layout variants fall from 0.075 to
0.032 ms. The latter's completion repeats span 0.209–0.217 ms. This supports
keeping a common input layout even when the CPU savings differ by device;
timings do not identify the driver's internal mechanism.

All 196 rows on each device passed the quality and repeat-stability checks.
Every new control matches the original circle renderer byte for byte, including
the selected 32-frame animations, native/half/native resize checks and all seven
final captures. Input checksums match across devices. Pi Clock rain and mixed
captures were visually inspected. Normal rendering defaults, live IPC options
and scenario interfaces are unchanged.

Artifacts: `target/gpu-experiments/vertex-input.3StYB8KI/`; `picade/native.csv` and
`host-native.csv` are the complete four-repeat comparisons. Smoke captures and
partial progress are separate files, not included in those tables. Binary SHA
prefixes: Pi `7c87fa54b113`, host `3ac2a4d4489c`. Source hashes, images, manifests
and the restoring runner are retained. The first Pi run's before/after settings,
installed binary, boot ID and PID matched, with temperatures about 68–70°C and
zero power-alarm reads.

Verification: 525 client tests plus three startup-isolation tests passed (one
existing client test ignored), as did host/Pi GPU release builds, software kiosk
check, formatting and diff checks. Clippy has the same existing warning counts,
none in the new controls. No application update or renderer-default change was
made on the device.

### Longer mixed scene confirmation

Some initial 120-frame blocks were slower (common layout 3.641–4.301 ms,
shared program 5.230–6.034 ms). A separate run of six 512-frame blocks per mode
checks the conclusion over longer intervals and complete 32-frame animation
cycles. It uses the same executables, viewport, MSAA, in-flight bound and 16-frame
warmup; do not merge these rows with the shorter seven-workload run.

| Pi mode | Median completed ms | Block range ms | Median submission CPU ms |
| --- | ---: | ---: | ---: |
| `triangles` | 3.297 | 3.270–3.334 | 0.710 |
| `triangles-split` | 3.727 | 3.665–3.750 | 1.199 |
| `circles` | 8.098 | 8.078–8.124 | 6.787 |
| `circles-unified` | 5.221 | 5.193–5.546 | 3.917 |
| `circles-unified-instanced` | 5.205 | 5.175–5.228 | 3.892 |
| `circles-unified-layout` | 3.729 | 3.699–4.070 | 2.426 |
| `circles-unified-vao` | 3.705 | 3.674–3.803 | 2.395 |

The longer run confirms the large input-layout benefit, not a clear winner
between the two VAO strategies. It also preserves the streamed baseline's lead.
First-block variation has not disappeared, so the exact small deltas should not
drive policy. All 42 rows pass parity/stability checks, and all four unified
variants match the original circle capture exactly. The analogous 42-row host
run completes in 0.318 ms baseline, 0.340 original circles, 0.215 shared program,
0.213 instanced draw API, and 0.207 for either fixed-layout mode. Its fixed-layout
GPU time is approximately 0.033 ms versus 0.076 ms for the shared program.

These are `picade-confirm/native.csv` and `host-confirm.csv` under the same
artifact directory. The second restoring runner again verified settings,
installed binary, boot ID and PID; temperatures before/after were about 67–73°C,
power-alarm reads zero, and Clock resumed at 60 FPS/UPS with no service restart.
No shader, source or executable changed between the full and confirmation runs.

The architectural result is to keep a common shader/input format for reusable
and streamed geometry, not require one VAO or infer that every shape benefits
from instancing. A useful next experiment is bounded reuse of non-circular
polygon meshes alongside dynamic geometry, with this common layout and the
streamed triangle reference still selectable. That broadens the evidence beyond
Pizza without prematurely choosing a global cache API. Remaining buffer-binding
and per-draw driver work can be isolated further if that mixed path needs it;
the separate circle-only shader/vertex cost is not a prerequisite for a general
mesh design. Keep production defaults unchanged until representative scenes
show a net benefit.

## Reusable polygon meshes

The next offscreen experiment tests explicit immutable polygon definitions
alongside streamed, deforming geometry. It keeps one shader and the same vertex
format/divisors for both kinds of draw. This is a private renderer-lab interface,
not a new scenario API: ordinary `RenderFrame` polygons contain already
transformed points and do not identify reusable shapes. Existing scenarios and
live renderer defaults are unchanged.

Three selectable modes distinguish the costs:

- `triangles`: tessellate ordinary world-space input and upload one ordered
  triangle batch, as before.
- `meshes-streamed`: tessellate each unique explicit template once per frame,
  upload it, and draw instances alongside streamed shapes.
- `meshes`: use the same geometry, shader, instances and ordered draw commands,
  but retain templates between frames and upload only when the cache changes.

The cache owns immutable definitions and keys them by identity, projected scale,
fill presence and stroke width/presence. Translation, rotation and colors are
instance data. Baking projected scale into the template preserves logical-pixel
stroke width under zoom; exact scale changes deliberately invalidate it. This
is not a quantized LOD policy. At most 32 entries and 1 MiB of template
vertex/index payload are retained. A template accepts up to 64 points. Entries
used anywhere in the current frame are pinned before eviction; unsupported or
over-budget meshes fall back to ordinary tessellation instead of disappearing.
Only adjacent identical-template instances are combined. Transparent source
order is never sorted for batching.

The GPU has one template vertex/index allocation, one instance stream, and two
VAOs with identical formats/divisors; there are no per-entity GPU objects.
Changed buffer contents use replacement storage, not unsynchronized writes over
in-flight data. Reported resident bytes describe current template payload, not
driver memory, allocation capacity, CPU copies or temporary in-flight backing.

The new 32-frame fixtures contain four non-circular definitions (concave ship,
star, irregular rock and notched polygon), up to 384 animated instances, and
384 deforming translucent ribbons. `meshes-grouped` places the ribbons after
the instance groups; `meshes-interleaved` inserts one after each instance.
These are different authored draw orders, each compared with its own baseline.
`meshes-churn` additionally replaces definition identities every frame, changes
projected scale and stroke width, and removes/reintroduces objects. It is an
intentional churn control, including frames whose new definitions have the same
point values; this identity cache does not silently deduplicate by content.
Existing Clock rain and mixed
fixtures supply controls without reuse hints. No deforming polygon is treated
as an immutable template.

Fixture construction verifies that explicit transforms reproduce the ordinary
projected polygon points at native and half resolution. The GPU lab checks all
32 animation frames against the triangle baseline with tighter polygon limits
(RGB MAE ≤ 0.05/255 and at most 0.1% of pixels exceeding summed RGB error 30),
then separately checks 192
frames of retained history across native/half/native targets against a second
presenter rebuilding templates. Keeping that reference in a separate presenter
avoids accidentally clearing the very cache whose invalidation is under test.
Counters record hits, misses, evictions, fallbacks, resident entries and bytes,
instance count, actual upload bytes and ordered draw count.

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --benchmark-width 1024 --benchmark-height 768 \
  --geometry-modes triangles,meshes-streamed,meshes \
  --geometry-cases meshes-grouped,meshes-interleaved,meshes-churn,mixed-interleaved,clock-rain \
  --presentation-frames 128 --presentation-repeats 4 \
  --geometry-output /tmp/spacewars-mesh-comparison
```

### Polygon comparison results

The October 3, 2026 UTC comparison uses the same 1024×768, four-sample MSAA,
three-frame in-flight bound as the earlier geometry experiments: 16 warmup
frames, four alternating blocks of 128 measured frames per mode/fixture. These
are medians of block averages, in milliseconds per completed geometry frame.
Simulation, Slint composition, scanout, capture/readback and independent image
comparisons are outside the timers. Pi elapsed GPU queries are still unavailable.

| Pi 4 workload | Ordinary triangles | Templates rebuilt each frame | Retained templates |
| --- | ---: | ---: | ---: |
| Grouped reusable polygons | 4.970 | 4.632 | 4.682 |
| Interleaved reusable polygons | 5.054 | 6.038 | 5.942 |
| Replaced definitions and changing styles | 4.557 | 5.549 | 5.704 |
| Existing mixed animation with no reuse hints | 3.312 | 3.362 | 3.368 |
| Clock rain with no reuse hints | 4.568 | 4.734 | 4.733 |

Reuse has a substantial CPU benefit in the grouped scene: total benchmark
thread CPU falls from 4.876 to 2.363 ms, tessellation/build CPU from 3.492 to
1.021 ms, and uploads from 474,617 to 75,257 bytes/frame. Five ordered draws
replace one. Completed throughput improves only about 6%, and rebuilding four
templates each frame is slightly faster than retaining them in this run despite
using more CPU (2.448 ms). Do not label the gap between CPU and completion time
as a measured GPU duration, or claim that retention itself supplies the large
gain: most of the tessellation reduction comes from sharing four definitions
among 384 instances within the frame.

Interleaving changes the tradeoff. Retained-template submission CPU rises from
the baseline's 0.603 to 4.428 ms with 768 draws; reduced tessellation cannot make
up for it on Pi. Three retained blocks complete in roughly 5.9 ms, while the
first reaches 7.401 ms. The churn case misses and evicts four entries every
frame, with zero fallback; rebuilding the combined buffer also uploads inactive
retained entries. Uploads rise from 72,724 bytes/frame in the rebuilt control to
102,899 bytes/frame with retention. This exposes a cost of the simple bounded
buffer design, not evidence that all caches should behave this way.

On the RX 7800 XT workstation, completed time improves from 0.486 to 0.139 ms
grouped and from 0.502 to 0.327 ms interleaved. Interleaved GPU query time rises
from 0.049 to 0.137 ms despite lower CPU work. Replacing definitions yields
0.449 ms baseline, 0.306 rebuilt and 0.314 retained. Clock rain is essentially
unchanged/slightly worse (0.407 versus 0.411 ms). The host was not isolated from
other checkout workloads; Pi measurements suspend only the paused kiosk and
are the deployment-target evidence. Neither result justifies a universal
instancing or retention policy.

Both devices completed all 60 comparison rows. Retained and freshly rebuilt
template captures match exactly, including the separate history/resize tests;
ordinary triangle comparisons differ only at tiny edges (Pi maximum captured
RGB MAE 0.000786/255, changed-pixel fraction 0.000037). Input checksums match
across devices. Captures of the interleaved and changing-geometry fixtures were
visually checked. The cache stays within 32 entries, with maximum observed
resident payload 33,996 bytes; the selected workloads have no fallback draws.
Unit tests separately exercise capacity fallback and eviction without dropping
or reordering primitives.

Artifacts are in `target/gpu-experiments/polygon-reuse.CfOwM8sZ/`:
`host-native.csv` and `picade/native.csv`, manifests, captures, source hashes and
the restoring runner. Final executable SHA prefixes are host `19103c6a9cdc` and
Pi `f28c4b2a4295`. The preliminary `host-smoke` run predates the final validation
gate/refactor and is not included in the table. The Pi runner verified unchanged
settings, installed executable, boot ID and service PID 8253; Clock resumed with
no service restart. Before/after temperatures were about 67–71°C and both power
alarm readings were zero. No renderer default or installed application changed.

Verification: 532 client tests and three startup-isolation tests pass (one
existing client test ignored), plus host/Pi release builds, software-kiosk
check, formatting and diff checks. Clippy retains the existing 26 binary and
35 test warnings (26 duplicates); none are in the new mesh work.

### Longer polygon confirmation

Six separate 512-frame blocks per mode on the same executables confirm the
grouped/interleaved result. These are not pooled with the shorter five-workload
comparison above.

| Pi workload and mode | Median completed ms | Block range ms | Median thread CPU ms |
| --- | ---: | ---: | ---: |
| Grouped triangles | 4.963 | 4.902–5.015 | 4.901 |
| Grouped rebuilt templates | 4.612 | 4.599–4.628 | 2.438 |
| Grouped retained templates | 4.664 | 4.647–4.692 | 2.352 |
| Interleaved triangles | 5.055 | 4.975–5.096 | 4.987 |
| Interleaved rebuilt templates | 6.057 | 6.038–6.098 | 5.991 |
| Interleaved retained templates | 5.959 | 5.939–6.427 | 5.901 |

Retention reduces grouped thread CPU by about 52% but completed time by only
6%; interleaved completed time regresses about 18%. The analogous host medians
are 0.502/0.146/0.138 ms grouped and 0.509/0.336/0.327 ms interleaved for
triangles/rebuilt/retained. Both devices pass all 36 additional rows, exact
rebuilt-template parity, reference-image gates, input-checksum agreement and
repeat stability. Some first-block variation persists, but does not change the
conclusion.

These are `host-confirm.csv` and `picade-confirm/native.csv` under the same
artifact directory. The second Pi run again restores unchanged settings,
installed binary, boot ID and service PID. Temperatures are about 67–70°C with
zero power-alarm readings. The governor remains `ondemand`; the experiment did
not force CPU clocks or change power policy.

The following experiment compares CPU expansion of these same cached polygon
templates into one ordered streamed batch. It retains tessellation reuse while
testing whether avoiding hundreds of tiny GPU draws is worth the extra CPU
transform/upload work. Direct instancing and ordinary tessellation remain
selectable controls. Do not infer shape identity from already transformed points
or sort translucent objects to win a benchmark.

## Expanded polygon batches

`meshes-expanded` tests whether cached tessellation remains useful when the
renderer cannot combine consecutive instances. It uses the same definitions,
cache keys, bounded eviction, fallback shapes and ordered commands as `meshes`,
but expands cached vertices on the CPU and submits one ordinary triangle batch.
It never sorts translucent primitives. Dynamic vertices stay in their existing
buffer; rebuilding the index sequence interleaves their triangles with each
transformed mesh's fill/stroke triangles. Template data is not modified.

This mode uses the baseline triangle shader and vertex layout, not the instanced
mesh shader. Only the finished vertex/index stream is uploaded: no template or
instance-buffer upload. It deliberately trades CPU transform/copy and bandwidth
for less submission work. Before expansion, checked counts enforce the existing
one-million-vertex / three-million-index prototype budgets; an oversized frame
fails rather than allocating an unbounded buffer or silently dropping shapes.
Buffers retain capacity between frames. Normal rendering defaults remain
unchanged; the new mode is only available in the offscreen probe.

`tessellate_cpu_avg_ms` includes cache lookup, dynamic tessellation and expansion.
The new `mesh_expansion_wall_avg_ms` is the wall-time subinterval for expansion
and final geometry validation, not an additional cost to add to that total.
`mesh_instances` counts logical template uses even in this non-instanced mode;
`resident_mesh_bytes` is cached template payload, not measured GPU residency.
Draw counts and actual upload bytes describe the selected submission path.

The GPU-only template controls still require byte-identical reference pixels.
CPU expansion moves arithmetic from GLSL to Rust, so its separate instanced
reference check permits only tiny subpixel-edge differences: RGB MAE ≤ 0.01/255
and at most 0.02% of pixels exceeding summed RGB error 30. The CSV explicitly
reports `submission_reference_exact=false` and both error measurements for this
mode. The independent triangle/software checks remain in place. A second
presenter validates 192 frames of retained history across native/half/native
targets, including one-draw verification, without clearing the tested cache.

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --benchmark-width 1024 --benchmark-height 768 \
  --geometry-modes triangles,meshes,meshes-expanded \
  --geometry-cases meshes-grouped,meshes-interleaved,meshes-churn,mixed-interleaved,clock-rain \
  --presentation-frames 128 --presentation-repeats 4 \
  --geometry-output /tmp/spacewars-expanded-mesh-comparison
```

### Expanded batch comparison results

The October 3, 2026 UTC comparison uses 1024×768, four-sample MSAA, three frames
in flight and 16 warmup frames. Each entry below is the median of four alternating
128-frame block averages, in milliseconds per completed geometry frame. These
are unpaced offscreen renderer measurements, not live scenario FPS: simulation,
Slint composition, scanout, readback and image comparisons are outside timing.

| Pi 4 workload | Ordinary triangles | Retained instancing | Expanded cached meshes |
| --- | ---: | ---: | ---: |
| Grouped reusable polygons | 5.020 | 4.660 | 3.974 |
| Interleaved reusable polygons | 5.126 | 6.046 | 3.990 |
| Replaced definitions and changing styles | 4.650 | 5.744 | 3.857 |
| Existing mixed animation with no reuse hints | 3.321 | 3.345 | 3.512 |
| Clock rain with no reuse hints | 4.586 | 4.713 | 4.720 |

Expansion improves completed time by 17–22% over ordinary tessellation in these
three reusable-polygon fixtures. It is not universally better: no-hint scenes
still pay cache-path bookkeeping, index copying and geometry validation without
avoiding tessellation. Keep ordinary streaming for those scenes. The first
interleaved instancing block takes 7.312 ms versus 5.950–6.078 ms for its remaining
blocks; longer confirmation runs below test whether the result depends on that
outlier.

The separate six-block, 512-frame Pi confirmation preserves the result:

| Pi 4 workload | Ordinary triangles | Retained instancing | Expanded cached meshes |
| --- | ---: | ---: | ---: |
| Grouped reusable polygons | 4.939 | 4.674 | 4.014 |
| Interleaved reusable polygons | 5.056 | 5.969 | 4.012 |
| Replaced definitions and changing styles | 4.605 | 5.738 | 3.910 |

That is 15–21% less completed time than ordinary tessellation and 14–33% less
than instancing. Expanded block averages range from 3.961–4.021 ms grouped,
3.979–4.032 ms interleaved and 3.865–3.928 ms with churn. Interleaved instancing
still has a slower first block (6.496 ms versus 5.917–6.006 ms afterward), but
even its fastest block is slower than every expanded block. Both block lengths
are recorded separately; these tables do not pool them.

The four-block comparison also exposes the CPU and bandwidth tradeoff. In the
grouped case, total benchmark-thread CPU is 4.929 ms for ordinary triangles,
2.388 ms for instancing and 3.012 ms for expansion. Instancing therefore saves
more CPU even though expansion completes sooner. Expansion uploads the same
474,617 bytes/frame as ordinary triangles, versus instancing's 75,257 bytes.
For interleaved shapes it reduces 768 ordered draws to one, and submission CPU
falls from 4.466 to 0.888 ms. Churn produces approximately 698 instanced draws
versus one expanded draw. All reusable fixtures report zero fallback. Pi GPU
timer queries remain unavailable; completion time minus CPU is not GPU time.

On the RX 7800 XT workstation, a separate six-block, 512-frame confirmation
gives 0.497 / 0.133 / 0.169 ms for grouped geometry, 0.506 / 0.325 / 0.177 ms
interleaved and 0.454 / 0.310 / 0.172 ms with churn (ordinary / instanced /
expanded). Instancing still wins for grouped work there. Expanded interleaved
GPU query time is 0.049 ms versus 0.138 ms instanced, while CPU is slightly higher
(0.176 versus 0.155 ms). This is evidence for retaining selectable submission
paths, not choosing one globally from a single device or synthetic scene.

Both devices passed the independent reference, animation and retained-history
resize checks. Maximum recorded expanded-versus-instanced capture RGB MAE is
0.000100/255 on the workstation and 0.000222/255 on Pi; changed-pixel fractions
above summed RGB error 30 are 0.000004 and 0.000011 respectively. These are tiny
edge differences, not byte-identical images. Pi interleaved baseline/expanded
captures were also inspected side by side for ordering, strokes and alpha.

Artifacts, CSVs, manifests, images and source/binary hashes are under
`target/gpu-experiments/polygon-expand.vEGjIqSQ/` (ignored, local artifacts).
`host-native.csv` and `picade/native.csv` contain the four-block comparison;
`host-confirm.csv` and `picade-confirm/native.csv` contain the separate longer
confirmations. Host binary SHA-256 begins `daf5bf7906e9dd49`, Pi binary
`1b4036ae0640816f`. The Pi benchmarks ran from temporary directories while the
paused software kiosk was stopped; the runner resumed that same process and
verified unchanged settings, installed binary and boot ID. The confirmation's
before/after temperature readings were 67.7/71.6°C, with zero power-alarm
readings and no service restarts. Its software Clock subsequently reported
60 FPS / 60 UPS. This does not install the experimental mode or change the
rendering default. CPU tests cover ordered expansion, transform/color selection,
preallocation budget rejection, recovery and empty/no-hint frames. The GPU
feature test run passes 537 client tests and three startup-isolation tests, with
one existing ignored test; software-feature checking and the Yocto build pass.

The following real-producer trial measures preparation as well as submission.
The synthetic results support reusing tessellation independently of how it is
submitted; they do not establish an automatic batching threshold or justify
caching arbitrary transformed points.

## Production ship artwork trial

The `ships-pair`, `ships-fleet` and `ships-zoom` fixtures call the actual
Spacewars ship renderer inside each timed frame, rather than replaying its
already-built draw list. They exercise a ship/pod pair, a 96-ship/pod stress
population, and the same population at four camera zoom levels. Poses, wing
sweep, loaded/reloading missiles and camera motion are prescribed in a 32-frame
sequence outside timing. These are artwork workloads, not a full Spacewars
match: no simulation, terrain, minimaps, text/UI or scanout is measured, and 96
ships is not a claim about current gameplay populations.

A small `ship_geometry::Sink` boundary names seven immutable local hull/cockpit
parts before their transform. The ordinary `RenderFrame` implementation emits
the same polygons as before; the probe records explicit part identity, pose and
style alongside that output. Definitions are created once when constructing
the fixture, then shared without hashing transformed points. Wings, rails,
missile bodies/tips and cockpit circles remain ordinary geometry. Unsupported
source transforms retain the complete ordinary polygon without a reuse claim.
The serialized `RenderFrame` and scenario interface do not change.

This intentionally conservative trial still constructs and projects all world
polygons, even for reusable parts, keeping a complete fallback. Its cost is
included, not hidden: manifest version 8 identifies `producer_timed_cases` and
each CSV row has `producer_timed`. New `produce_*` columns measure artwork
emission and reuse-metadata collection; `prepare_*` measures camera projection,
ordered scene construction and hint conversion. Both contribute to total frame
and benchmark-thread time. Existing frozen fixtures report no producer samples.
`triangles` bypasses metadata collection; `meshes` and `meshes-expanded` pay it.

Every fixture/mode/repeat now starts with a fresh presenter before warm-up,
recorded as `fresh_presenter_per_block=true`. This keeps caches populated by
earlier reference checks or fixtures out of the comparison. Allocation, shader
creation and 16 warmup frames remain outside timing; within a measured block,
cache entries persist normally. Retained-history correctness is tested
separately, rather than relying on incidental test order.

Before timing, every generated frame must exactly equal the stock producer's
reference frame. At native and half resolution, each reuse transform must
reproduce its ordinary projected polygon. The existing independent software,
triangle, animation and 192-frame retained-history resize gates also run for
these fixtures. Normal game rendering and its defaults remain unchanged.

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --benchmark-width 1024 --benchmark-height 768 \
  --geometry-modes triangles,meshes,meshes-expanded \
  --geometry-cases ships-pair,ships-fleet,ships-zoom \
  --presentation-frames 128 --presentation-repeats 4 \
  --geometry-output /tmp/spacewars-ship-art-comparison
```

### Measured result

The confirmation runs use six blocks of 512 frames per case/mode at 1024×768,
4× MSAA, three frames in flight and no pacing. Values below are medians of the
six block averages, in milliseconds per completed offscreen frame. This is
producer + projection + geometry construction + submission + completion, not
display FPS. Pi 4 has no usable GPU timer query here, so completed time must not
be labeled GPU execution time.

| Pi 4 workload | Ordinary triangles | Cached, instanced | Cached, expanded into one batch |
| --- | ---: | ---: | ---: |
| One ship + one pod | 5.625 | 5.541 | 5.657 |
| 72 ships + 24 pods | 7.207 | 9.062 | 6.014 |
| Same fleet, four zoom levels | 7.218 | 9.102 | 6.049 |
| Frozen Clock rain, no reuse hints | 4.571 | 4.678 | 4.626 |

The fleet's expanded path improves completed time by 16.6% and benchmark-thread
CPU from 7.133 to 5.936 ms. Across the six blocks, completed time ranges are
7.140–7.240 ms for ordinary triangles and 5.933–6.077 ms for expansion. Geometry
construction CPU falls from 4.951 to 3.565 ms. Collecting identities is not free:
producer CPU increases from 0.407 to 0.463 ms, and projection from 0.455 to
0.484 ms. The saving survives both costs and still materializing the complete
ordinary world geometry. Submission remains about 0.76 ms in one ordered draw.

Instancing saves more construction work but requires 745 draws to preserve
this artwork's order, with submission taking 4.380 ms. It uploads fewer bytes
(507,456 versus 762,624 per fleet frame) yet is slower overall. The expanded
path uploads the same amount as ordinary triangles: this win comes from
avoiding repeated tessellation, not reducing transfers or changing pixels.
The pair's expanded path saves only 0.045 ms of thread CPU (0.663 to 0.619 ms)
and does not improve completed throughput. Clock rain also does not benefit.

The workstation agrees on the fleet direction: ordinary / instanced / expanded
completed times are 0.748 / 0.605 / 0.537 ms; the zooming fleet is 0.743 / 0.610 /
0.538 ms. Other checkouts were active on the workstation, so the isolated Pi
comparison is the primary device result. Neither run benchmarks a full match
or establishes a universal submission policy.

Each cached mode retains seven entries (3,516 geometry bytes) for the pair and
fixed-zoom fleet, with no measured cache misses after warm-up. Four zoom levels
use 28 entries (14,064 bytes); 14 misses occur during each measured block because
the 16-frame warm-up covers only two of the four scales. Both cached modes show
zero evictions and fallback counts in all confirmation rows. These byte counts
describe cached geometry, not total process or GPU memory.

All source-frame hashes match between platforms. Animated and retained-history
resize checks pass. In captured Pi frames, expanded versus ordinary triangles
has maximum RGB MAE 0.000075/255 and changed-pixel fraction 0.000004; expanded
versus instanced has 0.000019/255 and 0.000001 respectively. The existing
changed-pixel metric requires summed RGB error greater than 30. The independent
software rasterizer has different edge coverage and is checked separately.
The pair and zooming fleet captures were also visually inspected.

Artifacts are in `target/gpu-experiments/ship-producer.eR1696zl/`:
`host-confirm.csv` and `picade-confirm/native.csv` contain the 72-row version-8
runs, with adjacent manifests, logs and images. Earlier version-7 smoke/initial
runs are retained for diagnostics but are not pooled into these results because
their presenter caches could include earlier fixtures. `source.sha256` and
`binaries.sha256` identify the measured code; host binary SHA-256 begins
`968929e0d27a2841`, Pi binary `058e66520b997fc0`.

The Pi runner paused and stopped the software kiosk only during the temporary
offscreen run, then resumed the same process. Settings, installed binary and
boot ID are unchanged, with no service restarts. Before/after temperature was
67.2/68.7°C, frequency 1.5 GHz and power-alarm readings zero; the restored Clock
reports 60 FPS / 60 UPS. No experimental renderer was installed as the default.
Verification passes 539 GPU-feature client tests, three startup-isolation tests
(one existing client test remains ignored), the focused ship/render/weapons
tests, software-feature checking, formatting and the Yocto GPU build. Clippy
completes with existing warnings.

This supports carrying immutable local geometry and per-instance transforms
through the rendering boundary, independently of submission strategy. The
following representation experiment avoids eagerly materializing duplicate
world polygons while retaining software compatibility and an ordinary dynamic
path. Keep triangles, instancing and ordered expansion selectable for comparison;
do not generalize from seven reusable ship parts to a claim that all geometry
should be cached. Whole-scene Spacewars GPU integration still needs the viewport,
minimap and lifecycle work below.

## Local geometry through projection

The opt-in `ships-pair-local`, `ships-fleet-local` and `ships-zoom-local` fixtures
use the same production artwork and poses as their world-polygon counterparts.
They retain the seven immutable definitions plus per-draw transforms and styles
through projection. There is no transformed point list or parallel reuse-hint
vector for those parts. Dynamic wings and missile artwork remain ordinary
primitives, interleaved in the original layer and draw order.

The private `authored::Frame` is a renderer-lab input, not a new serialized
`RenderPrimitive` or public scenario contract. Its software adapter materializes
an ordinary `RenderFrame` on demand, using the original transform arithmetic.
Every generated fallback frame must exactly match the production reference
before timing. Nonuniform, reflected and degenerate transforms take the ordinary
projection path. Positive uniform transforms keep local geometry and logical-pixel
stroke widths. Immutable definitions precompute a conservative bound so finite
placement checks do not walk every source point on every frame.

The cache accepts either the earlier explicit hints or the new local draw;
both share its budgets, invalidation and submission code. If a local polygon
does not fit the cache, its points are expanded into the ordinary tessellator's
scratch storage on demand. The `triangles` mode uses that uncached path for all
local polygons, providing a control for the representation independently of
caching. No missing-mesh placeholder or eagerly stored fallback is needed.

Manifest version 9 and the CSV `geometry_input` column distinguish world/local
inputs. Producer, projection, geometry construction, submission and completion
remain timed separately. Generated input destruction remains inside total
frame timing. Ordinary reference images come from the independent world-frame
sequence, never from the local path's own expansion. The local triangle path
also runs the tight polygon parity check for every animation frame. Retained
history and native/half/native resize checks still run for both cached modes.

To compare representation and submission within one binary:

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --benchmark-width 1024 --benchmark-height 768 \
  --geometry-modes triangles,meshes,meshes-expanded \
  --geometry-cases ships-fleet,ships-fleet-local,ships-zoom,ships-zoom-local \
  --presentation-frames 512 --presentation-repeats 6 \
  --geometry-output /tmp/spacewars-local-geometry-comparison
```

### Local geometry results

Six 512-frame blocks per fixture/mode, 1024×768, 4× MSAA, three frames in flight
and fresh presenters before warm-up produce 126 rows on each platform. Medians
of block-average completed offscreen milliseconds on Pi 4:

| Workload | Input | Ordinary triangles | Cached instancing | Cached single batch |
| --- | --- | ---: | ---: | ---: |
| Ship + pod | World | 5.556 | 5.541 | 5.643 |
| Ship + pod | Local | 5.698 | 5.556 | 5.654 |
| 72 ships + 24 pods | World | 7.196 | 9.002 | 5.984 |
| 72 ships + 24 pods | Local | 6.955 | 8.507 | 5.490 |
| Fleet with changing zoom | World | 7.205 | 9.046 | 6.020 |
| Fleet with changing zoom | Local | 6.996 | 8.474 | 5.558 |

Local input improves the cached fleet batch by another 8.3%, or 23.7% versus
ordinary world triangles in the same run. The six block averages range from
5.952–6.089 ms for cached world input and 5.376–5.565 ms for local input. The
zooming fleet improves another 7.7%. Uncached triangles also get a smaller
representation benefit at fleet size, but still spend about 5 ms on geometry
construction. The pair does not gain completed throughput; its cached thread
CPU falls only from 0.636 to 0.614 ms. These remain artwork workloads, not full
matches or evidence about normal two-player FPS.

For the cached fleet batch, producer CPU falls from 0.481 to 0.294 ms and
projection from 0.500 to 0.347 ms. The median of their per-block sums falls from
0.972 to 0.643 ms. Geometry construction stays close (3.509 to 3.470 ms), as
does submission (0.792 to 0.794 ms). Total benchmark-thread CPU falls from
5.932 to 5.408 ms; it also includes generated-input destruction. Both inputs
still upload 762,624 bytes and issue one draw. Instancing uses 745 draws and
remains slower despite its own preparation saving. Clock rain has no local
geometry and remains a control: 4.559 / 4.673 / 4.753 ms for ordinary / instanced /
expanded modes, so reuse is not a blanket choice for dynamic scenes.

The workstation's cached fleet improves from 0.560 to 0.526 ms (6.1%), and the
zooming fleet from 0.563 to 0.533 ms (5.3%). Background work was active on the
workstation; use the isolated-kiosk Pi comparison as the primary device result.
The Pi used its existing `ondemand` governor, not a locked frequency: before /
mid-run / after readings were 1.5 / 1.5 / 1.1 GHz. The GPU-bound pair was also
observed at 1.3 GHz. These are operating-device results, not fixed-frequency
instruction-cost measurements. Pi GPU timer queries remain unavailable.

Captured cached images are byte-identical between world and local inputs on
both platforms. Uncached local projection can round differently at edges;
all animated comparisons pass the tight polygon gate. Maximum captured error
against ordinary world triangles is RGB MAE 0.000075/255 and changed-pixel
fraction 0.000004 on Pi, and 0.000046/255 and 0.000003 on the workstation. All
input hashes match their corresponding world fixture and the other platform.
Both cached paths retain the same 7/28 entries and 3,516/14,064 geometry bytes,
with no evictions or fallbacks in timing blocks. The separate tests exercise
cache exhaustion, ordinary expansion, recovery, invalid data, layer/filter
ordering, pivots, scale fallback and exact software materialization.

Artifacts are in `target/gpu-experiments/local-geometry.JfO66bxv/`, including
`host-confirm.csv`, `picade-confirm/native.csv`, manifests, images and a small
`summarize.mjs` reader. Only these complete version-9 runs supply the table;
the initial smoke run and partial progress copy are not pooled into it.
`source.sha256`, `host-binary.sha256` and `pi-binary.sha256` identify the tested
code. Binary SHA-256 prefixes are `362667c4456f58ed` (host) and
`9eb689abab15bddc` (Pi). The final build passes 545 client tests plus three
startup-isolation tests, with one existing ignored test. Formatting,
software-feature checking and the Yocto GPU build pass; Clippy completes with
existing warnings and none in the newly added local-input code.

The Pi's installed binary, settings and boot ID are unchanged. Its same kiosk
process resumed at 60 FPS / 60 UPS, with zero restarts. Temperature readings
were 66.2°C before, 71.6°C mid-run and 68.2°C after; sampled power-alarm values
were zero. No renderer default or ordinary scenario behavior changed.

The useful outcome is a backend-independent data boundary: immutable geometry,
per-draw placement/style, and lazy expansion when needed. This trial now measures
both tessellation reuse and removal of duplicate point work. Keep the comparison
paths, but move the next integration work toward real scene/layer lifecycle and
viewport handling rather than adding more ship-specific caching. It still does
not justify automatically caching arbitrary deforming geometry or selecting
instancing globally.

## Live viewports and grouped minimaps

The live triangle/circle adapter now consumes the existing frame layouts without
requiring scenario changes. Player cameras draw directly into one shared
four-sample window target, with each camera's actual aspect ratio and a matching
scissor. Layer order, primitive order, logical stroke widths and premultiplied
source-over blending are preserved. A full-window geometry overlay draws after
the camera and minimap passes. Text still uses the existing Slint overlay.

A minimap is rendered over an opaque dark backing into a small retained target.
The resolved image is then circularly clipped and faded once, at 0.74 opacity,
onto the window target. Each minimap retains its own target through the final
window resolve; there is no separate full-window texture per player. Legacy
overview debris uses the existing minimum-size policy. Unlike the legacy
software overview cache, the GPU draws the current overview every frame.

Steady split-screen rendering retains three target sets: the window and two
minimaps. Each set contains its multisample color buffer and resolved texture.
Dimensions are validated against GL limits; failed allocation does not retire
the previous image. After a resized draw fails, the previous resolved texture
stays alive until Slint submits a successful GPU or CPU replacement. Returning
to the launcher or switching to vector/native video clears the borrowed image
and retires its targets. Programs and geometry scratch buffers remain bounded
and reusable until context teardown.

The opt-in layout diagnostic exercises the same compositor without taking over
scanout:

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-layouts --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 128 --presentation-repeats 4 \
  --geometry-output /tmp/spacewars-layout-check
```

It uses synthetic clipping/transparency probes and production Spacewars seed 42
snapshots at ticks 120 and 1800. Native, half, odd-sized and native-again passes
are checked against software; targeted interior pixel checks catch missing dark
backing, per-item alpha, swapped layers and rectangular minimap clipping. It also
checks caller GL-state restoration, failed-resize retention, CPU replacement,
context teardown, and no target allocations after warmup. The ordinary geometry
lab and its cache/input comparison modes remain available.

Layout timings rebuild/project/tessellate the frozen frames and wait for each
GPU frame to complete with `glFinish`. This is a **single-frame synchronous
latency measurement**, not the earlier three-frame-in-flight throughput metric.
Simulation, source-frame production, Slint text/UI, swap and scanout are excluded.
Readbacks, validation and PNG writing are outside measurement. Do not compare
these numbers directly with the earlier cache trials or call them gameplay FPS.

### Pi validation and scene cost

The first shared-scratch implementation passed the workstation checks but failed
on V3D: the first minimap's center sampled the second, empty map. The explicit
interior test caught `[70, 4, 15]` instead of `[114, 1, 99]`; a whole-image average
alone did not catch it. Retaining a separate target per map fixes the tested Pi
case without inserting a per-map synchronization wait. The additional 136×136
RGBA8 texture plus four-sample color buffer is nominally 369,920 bytes, excluding
driver padding/metadata. This is an observed same-frame reuse failure on the
tested path, not a proven driver root cause.

The revised Pi run passes 24 image comparisons across native/half/odd/native
layouts, the targeted interior probes, GL-state checks and resource-lifecycle
checks. No targets are allocated in measured steady-state blocks. Four repeats
of 128 frames at 1024×768 give these median block averages:

| Frozen Spacewars snapshot | GPU mode | Completed wall ms | Thread CPU ms | Triangle building wall ms | Geometry draws |
| --- | --- | ---: | ---: | ---: | ---: |
| Tick 120 | Triangles | 21.80 | 17.42 | 13.97 | 7 |
| Tick 120 | Circles | 22.41 | 17.99 | 13.99 | 33 |
| Tick 1800 | Triangles | 30.02 | 25.44 | 20.95 | 7 |
| Tick 1800 | Circles | 30.76 | 26.14 | 20.90 | 43 |

This is a correctness milestone, **not a Spacewars performance win**. The
ordinary raster probe, independently generating the same seed/tick selections,
spends about 3.75/5.09 ms rasterizing and 9.74/11.13 ms drawing the software UI.
Those are separate reference runs with different output/measurement boundaries,
not a frame-identical end-to-end speedup calculation. In the direct path, CPU
tessellation already dominates at 14–21 ms. Seven draw calls do not make
rebuilding thousands of terrain polygons cheap. Circle instancing adds commands
without addressing that cost. Keep software as the default; the next useful
experiment is reusable terrain geometry with explicit invalidation, not more
ship-only caching or a renderer-wide instancing switch.

The workstation's corresponding triangle block medians are 1.97/4.05 ms, with
1.48/3.22 ms building triangles. These are not cross-device scaling ratios:
the two architectures' simulated tick-1800 snapshots differ in geometry count.
Freeze/export identical frames before making that comparison.

### Live check and reproducibility

The same tested Pi binary also ran through the actual KMS display: Clock with
triangles, return to launcher, launch Spacewars, pause, then switch between
triangles, ordinary raster, circles, triangles and ordinary raster again.
Captured gameplay and paused images show the clipped cameras, circular maps,
HUD and menu together. The pre-existing FemtoVG rounded-menu-border defect
remains; this slice does not fix UI rendering or test physical controller input.

Diagnostics report one resident target in Clock, three in split-screen GPU
mode, and zero after CPU replacement or returning to the launcher. All mode
switches applied without a scene-renderer error. The brief live Spacewars sample
was 60 FPS / 60 UPS, with 2,008 shapes and about 8.05 ms average triangle-building
time. That is a lighter live scene, not a contradiction of the larger frozen
snapshots above or a full-match performance result.

The final code passes 550 client tests plus three startup-isolation tests, with
one existing ignored test. Formatting, the software-only feature check and the
Yocto GPU build pass. Clippy completes with existing warnings and none in the
new layout/compositor/diagnostic modules. The old single-frame Clock and local
ship-geometry controls also pass their image/history gates after the refactor.

Artifacts are in `target/gpu-experiments/live-layouts.uR4HKgsE/`: corrected
`host-maps.csv`, `native.csv`, `cpu.csv`, `pi-maps/`, `live/`, and the
`host-single-control` results. `verified-source.sha256`,
`verified-host-binaries.sha256` and `verified-pi-binaries.sha256` identify the
final code; client SHA-256 prefixes are `ce28d73717a84744` (host) and
`f00b0f5d8a2e4f29` (Pi). The earlier `native/` capture is the failed shared-map
scratch experiment and is deliberately not pooled into the successful results.
Scripts and a small `summarize.mjs` reader are retained beside the measurements.

After the live test, the original installed binaries and settings were restored
and verified against their saved copies. The boot ID is unchanged; the kiosk
is active with zero automatic restarts and software Clock at 60 FPS / 60 UPS.
No rendering default or persistent device configuration changed. Pi 5 GPU
validation and a sustained Spacewars match remain separate follow-ups.

## Retained terrain chunk experiment

This opt-in lab tests the CPU tessellation bottleneck identified above. It
retains meshes for immutable groups of terrain polygons, then expands them into
the ordinary ordered triangle stream. Uploads, shader, draw count, MSAA and
resolution remain the same kind of work. This is not a rectangular shape fast
path, a retained image, or a change to live Spacewars rendering.

The source is the production terrain artwork, extracted into `terrain_art`
without changing its polygon points, styles or order. The lab draws one block
planet or two/four contour planets with prescribed camera and body motion.
Additional translucent primitives interleave with terrain and draw over it.
These are controlled terrain scenes, not the previous full-match snapshots.

Chunk identity is assigned before world projection. The authoring adapter
replaces an immutable definition when `ChunkGeometry::generation` changes;
using only the chunk's own cell revision would miss neighboring contour edits.
A new world or restored field also needs a new namespace, because chunk IDs
and revision numbers can repeat. The renderer owns an `Arc` to each definition
while using its identity, so allocator address reuse cannot produce a false hit.
Placement and rotation do not invalidate a mesh. Scale does, because the
existing outlines remain logical-pixel widths rather than scaling with terrain.

The experimental LRU retains at most 128 entries and 16 MiB of accounted source
and mesh payload. That is not a process/GPU memory measurement: temporary
tessellation/expansion buffers and container overhead are separate. A definition
that does not fit uses the complete ordinary geometry fallback. Source/style
changes require replacement, not mutation through a cached identity. The
ordinary scenario `RenderFrame` contract remains unchanged.

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-terrain --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 128 --presentation-repeats 4 \
  --geometry-output /tmp/spacewars-terrain-check
```

The paired controls are ordinary triangles, chunk tessellation rebuilt every
frame, and retained chunk tessellation. Each measured frame includes production
world-polygon construction, authoring-cache synchronization, projection,
tessellation/expansion, submission and synchronous GPU completion. Terrain
edits and contour reconstruction are prepared outside timing; their resulting
definition replacements and renderer misses are inside it. Source-frame
disposal is included in total time. Readback, comparison, PNG writing,
simulation, UI, swap and scanout are excluded.

The edit sequence cuts two holes, replaces the world with different material
art at reset revision numbers, and repeats. Separate sequences exercise zoom
and native/half/native target changes. Image checks compare moving cached
geometry with a freshly built ordinary GPU reference at a tighter gate than
the independent software comparison: RGB MAE at most 0.05/255 and at most 0.1%
of pixels with summed RGB error above 30. Unit tests also cover adjacent-chunk
invalidation, identity replacement, style changes, cache eviction, budget
fallback and dynamic draw order. All three modes remain explicit controls;
there is no automatic caching policy yet.

### Terrain results on Pi 4

Four paired 128-frame blocks at 1024×768 produce the following median block
averages. These are synchronous rendering costs, not gameplay FPS or per-frame
tail-latency measurements. Targets and shaders are warmed before measurement.

| Prescribed terrain scene | Ordinary completed ms | Retained completed ms | Ordinary thread CPU ms | Retained thread CPU ms |
| --- | ---: | ---: | ---: | ---: |
| One block planet, moving camera/body | 6.72 | 6.42 | 1.94 | 0.98 |
| Four contour planets, moving camera/bodies | 19.61 | 10.79 | 14.98 | 5.61 |
| Two contour planets, alternating zoom levels | 12.06 | 8.08 | 8.02 | 3.37 |
| Two contour planets, cuts and world replacement | 12.68 | 9.10 | 8.59 | 4.47 |

For the four-planet case, triangle preparation drops from 11.27 to 1.52 ms.
Completed time improves by 45%, and thread CPU by 63%. Every mode still issues
one geometry draw and uploads 1,403,064 bytes per frame. The cache holds 64
entries with 1.57 MiB of accounted payload. The edit sequence retains a benefit
while averaging 448 misses and 352 evictions per measured block, with 89% hits,
128 entries and at most 3.40 MiB of accounted payload. No measured case falls
back because of the budget. A separate unit test verifies exact ordinary-mesh
fallback under an intentionally insufficient budget.

The cold-cache control is slower than ordinary triangles: 21.28 versus 19.61 ms
for the four-planet case. Rebuilding meshes is not free. Continuous
zoom with fixed-pixel strokes could likewise remove the reuse advantage; this
probe deliberately tests two recurring scales, not arbitrary zoom every frame.
The small block scene saves CPU but improves completed time by only 0.30 ms.
Do not infer GPU time by subtracting thread CPU from wall time; these runs do
not use GPU timer queries.

The workstation also improves: four contour planets take 1.98 to 0.62 ms, and
the edited pair takes 1.15 to 0.44 ms. These numbers are same-device paired
comparisons, not cross-device scaling ratios. The Pi retained path still spends
about 2.38 ms producing/projecting ordinary world polygons in the four-planet
case, so retaining local definitions through that work is a plausible further
optimization, not a gain already included here.

Both devices pass 192 cached-versus-ordinary history image checks and 20
independent software comparisons. The worst Pi cached/reference RGB MAE is
0.000173/255; the largest fraction of pixels with summed RGB error above 30 is
0.000010. Captures show the two holes after edits and the changed material art
after world replacement, with no stale geometry. The shared production-art
test covers blocks, contours and interpolated surfaces; the timed scenes cover
blocks/contours only. Client tests pass 555 with one existing ignored test,
plus three startup-isolation tests and the separate source-art regression.
Formatting, desktop/software feature checking and the Yocto GPU build pass.
Clippy completes with existing warnings and none in the new terrain/cache code.

Evidence is retained in `target/gpu-experiments/terrain-reuse.LX43B9jt/`:
`host-confirm.csv`, `pi/native.csv`, matching logs/captures, `summarize.mjs`,
the remote recovery script, test/build logs and source/binary SHA-256 files.
Client hash prefixes are `50fa806bd4c4d649` (workstation) and
`ba83ae9dee4e1989` (Pi). The initial host smoke run is not pooled into the table.
The Pi used its existing `ondemand` governor, not a forced fixed frequency;
sampled temperatures were 63.8°C before/after measurement and 63.3°C after
resuming, with sampled power alarms zero. The installed binaries, settings,
boot ID and kiosk PID are unchanged. Software Clock resumed at 60 FPS / 60 UPS
with zero automatic restarts.

This established useful reuse for terrain, including edits, but not yet an
end-to-end Spacewars win. It motivated the live integration below: preserve
backend-neutral chunk definitions and placements across the scene boundary,
then compare actual split-screen/minimap views. The uncached and cold-cache
controls, software fallback and current default remain; chunk identity is not
inferred by hashing transformed screen points.

## Live terrain reuse

The retained path now accepts actual Spacewars frames, including culled chunks
and moving detached terrain, through the split-screen compositor. Paired Pi 4
replay windows save **42–45% of completed render time** and **49–57% of render
thread CPU**. The visible match also exposes cache churn with changing views.
This remains a session-only experiment, not a software-default replacement.

### Source and renderer boundary

`engine-common` supplies immutable `PolygonGeometry` and optional frame hints:
a shared definition, local-to-world pose, and a primitive range in one layer.
The complete ordinary world primitives remain authoritative. Hints are omitted
from serialization and equality, preserving the existing wire/replay format.
There are no GPU handles in terrain or simulation state; a renderer can ignore
the hints entirely.

Each derived chunk has a process-local identity that changes on reconstruction,
including neighbor-dependent edits. Replacement fields/caches get new identities
even when revisions repeat. The scenario source-art cache keeps identities alive
and retains at most 128 definitions / 4 MiB of accounted polygon payload.
The `render-hints` scenario feature is enabled by the experimental GPU client,
not the ordinary software build.

The client validates ranges against the projected draw list. Layer sorting, text
omission, minimap filtering, overlapping ranges and edits to public frame lists
cannot silently redirect a group. Invalid hints use ordinary geometry. The CPU
mesh cache remains bounded to 128 entries / 16 MiB of accounted source and mesh
payload, shared across camera passes. These limits exclude temporary buffers,
container overhead and GPU memory. Mode changes and leaving gameplay release
the renderer's retained definitions.

Translation and rotation reuse a mesh. **Exact projected scale remains in the
key**, because outlines have fixed logical-pixel widths. This does not quantize
cameras, change strokes, add per-chunk GPU draws, or add a CPU rectangle shortcut.
Cached vertices still expand into the normal triangle stream and upload per frame.

```sh
spacewars-cli host gpu-scene triangles
spacewars-cli host gpu-scene retained-cold
spacewars-cli host gpu-scene retained
spacewars-cli status
spacewars-cli host gpu-scene off
```

These modes require the GPU build, FemtoVG output and raster scenes at 1×.
They are not saved settings. Status reports hits, misses, evictions and budget
fallbacks for the published 120-frame window, plus final entries/bytes.
`gpu_scene_cache_scale_misses` counts misses with the same definition still
resident at another scale; it is a lower bound after evictions. Per-view heights
and source/accepted group counts describe the latest staged frame. Before 120
frames, timing is unavailable: zero published counts do not mean an empty cache.

### Paired moving match measurements

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-live-retained --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 128 --presentation-repeats 4 \
  --geometry-output /tmp/spacewars-live-retained-check
```

The harness recreates seed 42 with two rule bots for each mode/block, advances
to tick 120 or 1800, then renders once per simulation tick. There are eight
warmup frames, with rotating ordinary/cold/retained ordering. The table contains
medians of four 128-frame block means on Pi 4. Measured intervals include source
generation/cache maintenance, projection/hint validation, tessellation/expansion,
uploads, `glFinish` and frame disposal. Simulation, Slint text/UI, readback and
scanout are excluded. These are moving replay **windows**, not whole-match FPS
or GPU timer measurements.

| Start tick | Mode | Completed ms | Thread CPU ms | Source ms | Project and validate ms | Encode ms | Submit ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 120 | Triangles | 22.61 | 18.46 | 0.99 | 1.33 | 13.23 | 2.02 |
| 120 | Cold | 24.76 | 20.65 | 0.96 | 1.58 | 15.35 | 1.97 |
| 120 | Retained | 12.50 | 7.98 | 1.01 | 1.70 | 2.22 | 2.03 |
| 1800 | Triangles | 32.19 | 27.84 | 1.65 | 2.19 | 20.14 | 2.45 |
| 1800 | Cold | 35.10 | 30.75 | 1.66 | 2.61 | 22.61 | 2.46 |
| 1800 | Retained | 18.62 | 14.15 | 1.61 | 2.62 | 5.78 | 2.59 |

All modes issue seven draws per completed frame. Average uploads are about
1.73 / 2.77 MB; retained versus ordinary differs by less than 0.004%, rather
than being byte-identical. Local/world floating-point tessellation can produce
slightly different meshes; the pixel checks below gate output. Hint-validation
cost is included. The wall/CPU difference is **not** measured GPU duration.

The first retained window has 3,082 hits / no measured misses per block, with
27 entries / 2,016,688 accounted bytes. The later window has 6,794 hits, 104
misses and 44 evictions, ending at 128 entries / 7,732,584 bytes. Neither has
budget fallbacks. Cold mode clears before each pass, including the final HUD
pass, hence zero entries in its end-of-frame sample.

The workstation control (64 frames × three blocks) improves 2.09 → 0.85 ms
and 3.40 → 1.46 ms. Do not directly compare absolute throughput with the Pi:
captures show different late-match trajectories across architectures despite
the same seed. Modes are paired **within** each device; these runs also use
different frame counts.

### Output and lifecycle checks

Each device passes 144 retained-versus-ordinary image comparisons through the
production adapter, including motion, fresh scenarios and native/641×361/native
resize history. Maximum Pi error is 0.000591/255 per RGB channel; at most
0.003457% of pixels exceed the existing RGB-sum threshold of 30. Host maxima are
0.000170/255 and 0.001017%. Captures were also inspected visually. These images
exclude Slint text; separate live captures include HUD and menus.

Checks cover source artwork, neighbor invalidation, replacement worlds, unchanged
observations/serialized frames, bounded caches, fallback and invalid/modified
ranges. Passing suites: 556 client tests (one existing ignored), three startup
tests, 22 CLI tests, 10 common tests, 21 terrain tests, three source-art tests
and 11 scenario terrain tests. Formatting, the software-only feature check,
Clippy with existing warnings and optimized host/Yocto builds also pass.
After adding the miss-reason diagnostics, the 47 GPU-specific tests and both
optimized builds pass again, as does the software-only feature check.

On `sw-picade-2`, the live trial covers Clock → launcher → Spacewars,
retained/cold/ordinary/circle/CPU-image switching, pause/resume and returning to
the launcher. No scene error or automatic restart is observed. Resident target
counts are one for Clock, three for split-screen GPU and zero after CPU image
replacement or returning to the launcher. In the confirmation run's paused
5,280-shape scene, ordinary reports 27.9 FPS, cold 25.3 FPS and retained
44.9–45.0 FPS. Encoding drops from 21.33–21.39 to 5.76–5.79 ms. CPU raster
presented through FemtoVG is about 42 FPS there. That is **not** the software
KMS backend, nor proof that GPU geometry is always fastest. Each direct mode
waits for its own published 120-frame window rather than sampling immediately
after the switch.

The unpaused trial (seed 0, human P1 / legacy-v9 bot P2, not the seed-42 replay)
starts at 60 FPS but later reaches zero hits / 6,036 misses per 120 rendered
frames and about 25 FPS. Of those misses, **6,006 (99.5%) are confirmed scale
misses**: the same definition is still resident at another exact scale.
P2 accepts all 60 source groups in the late snapshots, but its camera height
continues changing (393.94 → 412.29 → 378.97 world units). Once paused, that
view's 60 groups produce 7,140 hits after 60 initial misses in 120 frames.
Snapshots can share a published window; do not sum repeated counters.

In that initial integration P1 stayed at camera height 44 but accepted none of
its 3–6 source groups, even when paused. Those polygons rendered through the
ordinary path. The [follow-up below](#adaptive-terrain-retention) reproduces
the failure, identifies projection rounding as the cause, and compares bounded
cache admission during continuous zoom. Scale-safe reuse must preserve
fixed-pixel outlines; merely dropping scale from the key is incorrect.

Raw CSV, captures, status/logs, scripts and summaries live under
`target/gpu-experiments/live-retained.bnoGMsZV/`. The final measurements above
are `host-confirm.csv` and `pi-confirm/native.csv`; initial runs are retained
separately, not pooled. `pi-confirm/running-*.txt` contains miss reasons and
per-view snapshots; `paused-*.txt` and matching PNGs contain mode comparisons.
The client hash prefixes are `bc5a3eb62c891c2c` (host) and `8f8a8065d56bb748`
(Pi). The Pi uses Mesa 25.1.6/V3D 4.2.14.0 and its existing ondemand governor.
Final offscreen samples are 62.3°C before and 66.7°C after, at 1.5/1.2 GHz;
both sampled power alarms are zero. Frequency is not locked.

The new binaries remain installed; preferences are restored byte-for-byte and
the ordinary software Clock is running at 60 FPS / 60 UPS. The final service
has zero automatic restarts; boot ID is unchanged. No reboot, boot-file change
or deployment to another device is part of this experiment.

## Adaptive terrain retention

The close-up view now validates exact world-space source points before
projection. A new session-only `retained-adaptive` mode tests cache admission
for views whose zoom changes every frame. The existing `triangles`, `retained`
and `retained-cold` controls remain available; no rendering default changes.

### Close up validation

The regression `close_up_seed_zero_terrain_hints_survive_projection` runs seed
0 with a human P1 and bot P2 for 180 ticks at 1024×768. Before the fix, all
three P1 terrain hints were rejected. Their equivalent projection expressions
differed by 0.0031–0.0037 pixels, exceeding the old 0.002-pixel comparison.

Validation now reconstructs each definition's world-space points with its
declared pose and requires exact equality with the authoritative polygons,
including styles, counts and order. This avoids camera-dependent cancellation
without widening a screen-space epsilon. A one-ULP edit to the ordinary source
invalidates its hint, even if projection would hide that edit. Layer/range and
skipped-draw checks remain in place. The ordinary frame data is unchanged.
The cached affine projection is still not bit-identical to ordinary f32
projection; paired rendered-image checks remain essential.

### Bounded admission

```sh
spacewars-cli host gpu-scene retained-adaptive
spacewars-cli status
```

An exact definition/scale key must be observed twice before adaptive mode
constructs and retains its mesh. On first observation it tessellates the
ordinary projected geometry directly. Existing cache hits behave as before;
repeated scales across views can qualify, and settled zoom resumes caching.

Probation holds at most 128 keys in FIFO order. Weak identity tokens prevent
pointer reuse without keeping discarded polygon payloads alive. The existing
128-entry / 16-MiB accounted mesh-cache budget is unchanged. Mode changes and
leaving gameplay clear both caches. Status distinguishes `admissions`,
`bypasses`, budget `fallbacks`, and `probation_entries`.

This is an admission heuristic, **not scale-independent tessellation**. A
larger working set can exceed probation and get little reuse. Continuously
changing scales still pay ordinary tessellation cost; the goal is to avoid
additional local-mesh construction, copying, expansion and cache eviction for
one-use entries. Stroke widths, primitive order, uploads and GPU draw structure
are not intentionally changed.

### Expanded comparison harness

`--geometry-live-retained` now runs three fixtures: the two existing seed-42
bot windows and seed 0 at tick 1500 with human P1 / bot P2. The latter combines
a parked close-up with continuously changing zoom in the approaching bot's
view. All four modes recreate the same scenario and use rotating order.
CSV version 2 adds fixture names, source/accepted group counts and cache
admission statistics. Do not feed it to the earlier numeric-only CSV parser.

Validation compares both cached modes with ordinary triangles across 24-frame
motion windows, fresh worlds and native/641×361/native histories: 432 paired
image checks per device. Captures include the first and last frame of native
windows. Timing still includes source generation through `glFinish`, but not
simulation, readback, text/UI or scanout. Use same-device paired comparisons;
late trajectories can differ across architectures.

### Measured results and remaining cost

Medians of four 128-frame blocks on Pi 4, following eight untimed warmup
frames per block. These are completed render milliseconds, not full-app frame
times. All modes in this table include the world-space validation fix.

| Replay window | Ordinary | Cold | Eager retention | Adaptive retention |
| --- | ---: | ---: | ---: | ---: |
| Seed 42, tick 120 | 22.75 | 24.92 | 12.60 | 12.51 |
| Seed 42, tick 1800 | 32.32 | 36.02 | 19.20 | 18.77 |
| Seed 0, close-up and changing zoom | 36.41 | 40.62 | 38.33 | 34.82 |

The new zoom fixture exposes a real regression from eager caching: it is
slower than ordinary rendering. Adaptive retention reduces completed time by
9.1% versus eager and 4.4% versus ordinary there. Thread CPU is 31.69 ms ordinary, 33.67 ms
eager and 30.14 ms adaptive. Eager mode builds and evicts 7,681 meshes per
measured block; adaptive mode builds one, bypasses 7,681 and evicts none.
Its ending accounted mesh/source payload falls from 7,128,348 to 436,668 bytes.
This excludes bounded probation/container overhead and GPU memory.

For the earlier two bot windows, adaptive retains the useful performance of
eager reuse, with CPU times of 8.03 and 14.29 ms versus ordinary 18.59 and
27.97 ms. The later window admits two new meshes instead of 642 and ends at
59 entries / 2,870,956 bytes rather than 128 / 9,667,940. No measured mode
hits the budget fallback. Every source hint is accepted in the timed cached
modes across all three fixtures, rather than only the lower-zoom player view.

The workstation uses the same 128 × four timing schedule. Its zoom case takes
3.46 ms ordinary, 3.47 ms eager and 3.27 ms adaptive. Its early/late adaptive
windows take 0.71/1.39 ms versus ordinary 1.98/3.31 ms. These are controls, not
a ratio of Pi and workstation capability; the late seed-42 trajectories differ.

All modes retain seven draws per frame. Uploads remain approximately
1.73/2.77/3.17 MB on Pi, with tiny floating-point tessellation differences
between local and ordinary meshes. **Most zoom-case CPU time still goes to
tessellation**: adaptive encoding alone costs 20.91 ms, plus 2.00 ms for source
generation and 3.40 ms for projection/validation. Admission avoids extra work;
it does not make this workload cheap. This motivates the subsequent
[scale independent trial](#scale-independent-terrain-tessellation), rather than
simply increasing cache limits or rounding camera zoom.

### Validation and device state

Both devices pass the 432 image checks. Maximum Pi RGB MAE is 0.000685/255;
at most 0.003560% of pixels exceed RGB-sum error 30. Host maxima are
0.000348/255 and 0.002289%. Representative paired captures and the live Pi
screen were inspected. Offscreen captures omit Slint text; live captures
include text, HUD and menus.

Unit tests reproduce the original close-up rejection, reject one-ULP source
edits, verify exact ordinary meshes on adaptive bypass, bound probation and
check that its weak tokens release source payloads. They also cover alternating
camera scales, zoom settling, and replacement geometry/materials. The client
suite passes 559 tests with one existing ignored test, plus 22 CLI tests and
three backend-startup tests. Software-only checking, formatting, host/Yocto
release builds and Clippy pass; Clippy retains the existing warnings.

The live Pi trial covers Clock, launcher, Spacewars, triangle/circle/retained
modes and CPU image fallback, pause/resume, and returning to the launcher. Both views accept
all their hints (6/6 and 60/60 in the late snapshot). A changing-zoom window
records 462 hits, 7,203 bypasses, three admissions and no evictions. It reports
about 29 FPS. This is corroborating live evidence, not a matched performance
ratio against the previous session.

On the same paused 5,280-shape view, ordinary reports 27.9–28.0 FPS and eager
retention 50.0 FPS. Adaptive reports 50.2–51.1 FPS, with 66 first-use bypasses,
66 admissions and 7,788 hits in its first 120-frame window. The small FPS
difference between the two cached modes is not evidence of a steady-view
speedup. CPU raster through FemtoVG reports about 46 FPS; this still does not
compare against software KMS. Scene targets retire correctly after fallback
and leaving gameplay, with no renderer error or automatic service restart.

Evidence lives in `target/gpu-experiments/zoom-retention.pT4SjqWJ/`: `host.csv`,
`pi/native.csv`, logs/captures, tests/build logs, checksums and the bounded
device scripts. Client hash prefixes are `8dc0940591d92227` (host) and
`debf65ecfed62584` (Pi). The Pi used its existing ondemand governor: sampled
temperatures 62.3°C before and 67.2°C after, frequencies 1.5/1.4 GHz, sampled
power alarms zero. Frequency is not locked. Boot ID and saved preferences are
unchanged. Updated binaries remain installed on `sw-picade-2`; ordinary
software Clock resumes at 60 FPS / 60 UPS. No other device is updated.

## Scale independent terrain tessellation

The session-only `retained-zoom` experiment removes exact camera scale from the
mesh key. One immutable source definition can supply multiple camera zooms.
It caches local fill positions and separates stroke centerline positions from
fixed-pixel extrusion offsets. Expansion scales only the positions, then
rotates/translates the result into the same ordered triangle stream. GPU vertex
format, submission, multisampling and compositing are unchanged.

```sh
spacewars-cli host gpu-scene retained-zoom
spacewars-cli status
```

This is not unrestricted path reuse. Inspection of the pinned Lyon stroke
tessellator found two scale-dependent operations: merging very short edges and
folding sharp joins when outlines are wide relative to adjacent edges. The
prototype computes conservative validity limits per polygon and uses ordinary
projected tessellation below those limits. It also rejects duplicate,
backtracking and self-intersecting paths, near reversals and excessively large
reference coordinates. This optimization must not silently change those shapes.

Fill and stroke are independently reusable, but stay interleaved in their
original fill-then-stroke order. A distant polygon can reuse its fill while
rebuilding its subpixel outline. The first implementation fell back for the
whole polygon; the initial workstation smoke trial showed that this discarded
most reuse in the changing-zoom view, motivating the split before measurement.

The cache still owns source identities and has a 128-entry / 16-MiB accounted
payload limit. Larger template vertices and per-piece ranges count toward that
limit. Mode switches and session teardown clear it. No screen-point hashing,
rounded zoom keys or extra per-chunk GPU draws are introduced. Ordinary, cold,
eager exact-scale and adaptive modes remain selectable controls.

Diagnostics distinguish full-polygon reuse from polygons needing any fallback,
and count reused fills and strokes separately. These counters describe pieces
inside admitted definitions; `cache_fallbacks` remains the separate group-level
budget counter. A group-cache hit alone does not prove that all its pieces
avoided tessellation.

The live replay CSV advances to version 3 with these counters and a fifth mode.
Five rotated repeats balance each mode's ordering. Validation adds 210 small
outline fixtures (thin/wide strokes, concavity, sharp corners, duplicate points,
self-intersections, transparent overlap and fill-only shapes), plus 648 moving
real-game comparisons across the three existing fixtures and resize histories.
The same tight image limits remain: RGB MAE at most 0.05/255 and at most 0.1%
of pixels with summed RGB error above 30. Simulation, UI/text, readback and
scanout are still excluded from render timings.

### Workstation control

Medians of five 128-frame blocks on the workstation, with eight untimed warmup
frames per block. The table reports source generation through completed GPU
rendering, in milliseconds, not whole-application frame time.

| Replay window | Ordinary | Cold exact scale | Eager exact scale | Adaptive exact scale | Across zoom |
| --- | ---: | ---: | ---: | ---: | ---: |
| Seed 42, tick 120 | 2.068 | 2.189 | 0.731 | 0.735 | 0.989 |
| Seed 42, tick 1800 | 3.391 | 3.643 | 1.466 | 1.440 | 1.525 |
| Seed 0, close-up and changing zoom | 3.562 | 3.773 | 3.501 | 3.332 | 2.348 |

Across-zoom reuse reduces the zoom window by 29.5% versus adaptive caching,
but is slower in the two steadier windows. Their stable exact-scale caches
can retain even subpixel outline tessellation; the guarded mode rebuilds those
outlines. Do not automatically replace the existing strategy with this mode.

Initial template construction occurs during warmup in this steady-state
comparison. The `cold` column rebuilds the **old exact-scale** templates, not
the new scale-independent ones. First-use cost for the new representation and
a broader destructive-edit workload still need dedicated measurements before
adoption. Measured misses/admissions during replay remain inside timing.

All seven GPU draws remain, and upload sizes differ only by small tessellation
rounding effects. This experiment retains CPU templates and expands them into
the existing stream; it does not yet retain vertex buffers on the GPU.

### Pi 4 results

The same five-repeat, 128-frame schedule at 1024×768 on `sw-picade-2`, with
Broadcom V3D and four-sample antialiasing. Completed render milliseconds:

| Replay window | Ordinary | Cold exact scale | Eager exact scale | Adaptive exact scale | Across zoom |
| --- | ---: | ---: | ---: | ---: | ---: |
| Seed 42, tick 120 | 22.882 | 24.935 | 12.616 | 12.641 | 14.910 |
| Seed 42, tick 1800 | 32.858 | 35.943 | 19.333 | 18.868 | 20.093 |
| Seed 0, close-up and changing zoom | 36.874 | 40.455 | 38.497 | 35.078 | 27.878 |

The changing-zoom window improves by **20.5% versus adaptive** and 24.4% versus
ordinary rendering. Thread CPU falls from 30.229 to 22.986 ms versus adaptive,
a 24.0% reduction. Encoding falls from 21.012 to 13.769 ms. Source generation
still costs 1.982 ms, projection/validation 3.321 ms, and submission 2.523 ms.
These are measured improvements, not a claim of 60 FPS for the full game.

That window has 8,148 group-cache hits, zero measured misses/admissions/evictions
and 60 resident entries. It reuses all 641,144 hinted fills but only 143,784
outlines (22.4%). The other 497,360 outlines still tessellate normally—about
3,886 per frame. Accounted source/template payload is 5,940,088 bytes, versus
436,668 for adaptive and 7,128,348 for eager exact-scale caching. These figures
exclude container/tessellator scratch and GPU memory; this is not total RSS.

In the earlier two windows, across-zoom caching reuses every hinted fill and
74.5%/77.1% of outlines, but is 17.9%/6.5% slower than adaptive caching. Ending
payloads are 4,054,200/6,368,616 bytes versus adaptive's 2,016,688/2,870,956.
All modes keep seven draws and approximately 1.73/2.77/3.17 MB of uploads per
frame. All source groups are accepted by the cached modes; none hits the
group-level budget fallback. Same-device paired comparisons remain essential:
the late seed-42 trajectories differ between the workstation and Pi.

The [hybrid follow-up](#hybrid-geometry-reuse) combines adaptive exact-scale
reuse for stable views with the scalable representation for changing zoom,
under a shared memory budget. It preserves the individual controls and measures
first-use costs. This is not a production policy yet. Further
outline work should focus on safely reusing subpixel stroke topology rather
than weakening the guard or changing terrain artwork to flatter the benchmark.

### Validation and evidence

Both devices pass 210 outline and 648 real-game image comparisons. The Pi's
outline captures are byte-identical; its live replay maximum RGB MAE is
0.000685/255 and maximum fraction above RGB-sum error 30 is 0.003560%. The
workstation's outline maxima are 0.000291/255 and 0.001302%; its live maxima
are 0.000348/255 and 0.002289%. Representative outline and split-screen captures
were inspected. No image gate was relaxed.

The client suite passes 563 tests with one existing ignored test, including
53 focused geometry tests. New tests cover fixed-width outlines across zoom,
part-level fallback, source replacement, ordering, pathological paths and
accounting for the larger templates under the cache limit. The 22 CLI tests,
three backend-startup tests, software-only checking, formatting, host/Yocto
release builds and Clippy pass; Clippy retains the existing 35 client-test
warnings, with none in the new code.

Evidence is in `target/gpu-experiments/scale-independent.5UiTJrSO/`: paired
host/Pi CSVs and captures, the earlier whole-polygon smoke run, the split
fill/stroke smoke run, test/build logs, binary hashes and bounded device scripts.
The measured client hash prefixes are `4f8dfde5f1fcea1e` (host) and
`388951141cbb1595` (Pi). The Pi keeps its ondemand governor: 64.8°C before and
66.7°C after the offscreen run, sampled frequency 1.5 GHz and power alarm zero
at both ends. Frequency is not locked.

The live trial verifies Clock, launcher, Spacewars, pause/resume, all retained
controls, circles, CPU-image fallback and return to the launcher. Scene targets
retire to zero on fallback/launcher and return to three for split views. The
moving close-up window reports about 38 FPS / 60 UPS, 7,794 group hits, no
misses or evictions in its last 120-frame window, and 9,925,840 accounted bytes
after earlier visible chunks accumulate. This is corroborating live evidence,
not a matched ratio against the previous adaptive session.

On the same paused 5,256-shape frame, ordinary reports 27 FPS, cold 25 FPS,
eager/adaptive about 49.5–49.7 FPS, and across-zoom 36.4–36.5 FPS. That confirms
the steady-view regression; there is no reason to discard the exact-scale path.
CPU raster through FemtoVG reports 41.6 FPS, which is not a comparison against
software KMS. Paired offscreen and live captures were inspected.

No renderer error or automatic service restart occurred. Boot ID and saved
preferences are unchanged. The new binaries remain installed, and ordinary
software Clock resumes at approximately 60 FPS / 60 UPS. No other device,
boot configuration or default renderer is changed.

## Hybrid geometry reuse

The session-only `retained-hybrid` mode combines exact-scale meshes for stable
views with guarded templates for changing zoom. It uses the same ordinary
triangle stream, GPU uploads, clipping and compositing as the earlier modes.
No zoom quantization or new per-chunk GPU draws are introduced.

```sh
spacewars-cli host gpu-scene retained-hybrid
spacewars-cli status
```

A resident exact-scale mesh is the first choice. Otherwise, the first
observation of a source/scale pair uses its scalable template immediately.
If that exact scale is observed again in a later presentation frame, the cache
promotes it to a mesh with the scaling and guarded outline work already done.
All camera passes share a frame counter: two cameras seeing the same new zoom
in one frame do not qualify it for promotion. Translation and rotation do not
invalidate either representation, but replacing the immutable source does.

Promotion reuses the existing template's safe pieces and tessellates only
guarded pieces, in scaled local coordinates. Fills and strokes keep their
source ordering. Existing width, degeneracy and topology guards remain intact;
an inadmissible group still uses ordinary tessellation.

Both representations share the 128-entry / 16-MiB accounted payload limit and
LRU eviction. Weak source identities in a separate 128-key probation FIFO do
not retain source-art payloads. The payload limit includes source and mesh
data, conservatively charging source data again for each representation. It
does not claim to bound allocator/container overhead, tessellator scratch or
GPU memory. Retaining both forms can use more memory than either alone.

Live diagnostics and CSV output count hybrid exact, scalable and ordinary
fallback groups, promotions, and resident exact/template entries separately.
Their group counts describe the path actually used. Generic hit/miss counters
describe cache lookups; a rejected exact admission followed by a template lookup
can contribute more than one lookup for a single group. `scale_misses` includes
an existing source resident under a different scale or template key.

### Measurement scope

The live replay CSV is version 4. Six rotated repeats balance the six modes,
with the same three scenario windows, native 1024×768 viewport, eight startup
frames and 128 subsequent measured frames. Each block explicitly empties the
geometry cache and creates a new scenario. GPU targets are warmed beforehand
using a disposable source; this is geometry first use, not process startup.

First-frame wall/CPU/encode times and admissions are now recorded rather than
discarded. The first eight frames also report total wall/CPU/encode time,
maximum wall time, admissions and promotions. Steady-state rows retain all
previous averages/counters and add per-block p95 and maximum wall time. All
admissions during the measured frames remain inside timing. The render interval
still includes source generation through `glFinish`, but excludes simulation,
text/UI, readback and scanout. It is not full-game frame time.

The image gate is unchanged. Validation covers 420 small outline comparisons
across zoom/rotation and 864 real-game comparisons, including source replacement
and resize histories. The real-game checks require both hybrid routes and
promotion to execute; visual parity alone cannot conceal a missing promotion
path. Unit tests separately cover simultaneous zooming cameras, stable views,
resumed zoom, material replacement, guarded-piece order and shared budgets.

### Workstation hybrid results

Medians of the six 128-frame blocks, in completed render milliseconds:

| Replay window | Ordinary | Adaptive exact | Across zoom | Hybrid |
| --- | ---: | ---: | ---: | ---: |
| Seed 42, tick 120 | 1.987 | 0.728 | 0.984 | 0.729 |
| Seed 42, tick 1800 | 3.317 | 1.414 | 1.519 | 1.137 |
| Seed 0, close-up and changing zoom | 3.497 | 3.282 | 2.353 | 2.369 |

The hybrid matches exact caching in the early window and preserves the
across-zoom benefit in the changing-zoom window: 27.8% less completed render
time than adaptive exact caching. It also benefits the later moving window,
which uses 7,864 exact groups and 869 scalable groups during measurement.
That window reaches the entry limit and has 53 admissions/evictions; those
costs are included. Its median per-block maximum is 2.903 ms versus adaptive's
2.327 ms, despite its lower average. Cache statistics and tail times are
necessary alongside the mean.

First-frame hybrid times are 2.957 / 5.950 / 5.321 ms, compared with adaptive's
2.230 / 4.635 / 3.696 ms. The corresponding eight-frame totals are
8.791 / 16.599 / 22.024 ms versus 9.118 / 17.172 / 26.884 ms. Initial template
construction is a real spike even though the following frames repay that cost.
These are the first frames of fresh replay blocks, not application launch times.

### Pi 4 hybrid results

The same six-repeat schedule on `sw-picade-2`, in completed render milliseconds:

| Replay window | Ordinary | Cold exact | Eager exact | Adaptive exact | Across zoom | Hybrid |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Seed 42, tick 120 | 23.782 | 25.449 | 12.522 | 12.554 | 14.790 | 12.498 |
| Seed 42, tick 1800 | 34.051 | 36.883 | 19.440 | 19.080 | 20.120 | 16.474 |
| Seed 0, close-up and changing zoom | 38.183 | 41.387 | 38.941 | 36.009 | 27.780 | 27.690 |

The hybrid recovers the stable-view performance lost by template-only caching,
while preserving its benefit during zoom. Against adaptive exact caching, the
later window improves by 13.7% and the changing-zoom window by 23.1%. Zoom
thread CPU falls 31.332 → 23.011 ms (26.6%), and encoding 22.004 → 13.883 ms.
The zoom window's median per-block p95 falls 37.131 → 28.712 ms and maximum
41.150 → 32.909 ms. These are uncapped offscreen results, not an FPS ceiling.

The zoom window uses 467 exact groups and 7,681 scalable groups. It has 8,147
hits, one measured admission/promotion, zero evictions and no budget fallback.
Its final 64 entries comprise four exact meshes and 60 templates, with
6,376,756 accounted bytes. The earlier windows finish at 6,070,960 and
9,239,608 bytes, versus adaptive's 2,016,688 and 2,870,956. The later window
uses 6,794 exact and 642 scalable groups, with four measured admissions,
two promotions and no evictions. All modes keep seven GPU draws; uploads
remain about 1.73 / 2.77 / 3.17 MB per frame. This is still CPU mesh expansion
and streaming, not resident GPU geometry.

First use is not free. Fresh hybrid blocks take 30.516 / 55.881 / 52.177 ms
on their first frame, versus adaptive's 24.826 / 54.364 / 39.367 ms. For the
zoom window, first-frame encoding alone rises 21.873 → 34.220 ms. But the
eight-frame startup totals improve from 131.339 / 205.347 / 293.443 ms to
125.293 / 183.602 / 245.448 ms. The cache repays its construction cost quickly
in these windows while still producing an undesirable initial spike.

The remaining zoom cost is substantial: 495,620 polygon outlines take the
guarded ordinary path over 128 frames, about 3,872 per frame. Reusable fills
do not eliminate those outline rebuilds. Source generation, projection and
submission also cost 1.897 / 3.212 / 2.393 ms. Keep the hybrid experimental;
investigate safe reuse of those small outlines and bound first-use/admission
work before treating it as a finished zoom-performance solution. A broader
terrain-edit/visibility-churn workload is still needed.

### Hybrid verification and evidence

Both devices pass all 1,284 image comparisons. Outline maxima are
0.012422/255 RGB MAE and 0.028646% above the error threshold on the workstation;
the Pi reports 0.012747/255 and 0.031250%. Real-game maxima remain
0.000348/255 and 0.002289% on the workstation, and 0.000685/255 and 0.003560%
on the Pi. The same 0.05/255 MAE and 0.1% pixel-fraction limits apply. Outlines
are not claimed byte-identical after promotion and rotation.

The final client suite passes 567 tests with one existing ignored test, plus
22 CLI and three backend-startup tests. Formatting, software-only compilation,
Clippy and host/Yocto release builds pass. Existing Clippy/vendor warnings
remain; none is introduced by this change. Representative outline and real-game
image pairs were inspected.

Raw CSVs, image pairs, scripts, checksums and test/build logs are in
`target/gpu-experiments/hybrid-zoom.cdDs3UZG/`. Client hash prefixes are
`7f308e72a266bdf3` (host) and `ad40af7b66a90a53` (Pi). The Pi offscreen run
starts at 62.8°C and ends at 66.7°C, with power alarm zero and sampled frequency
1.5 GHz at both ends. The ondemand governor remains in use, not locked frequency.
Settings and boot ID are verified unchanged after the benchmark.

The live check covers Clock, launcher, Spacewars, pause/resume, all retained
modes, ordinary triangles, circles, CPU-image fallback and return to launcher.
Targets retire to zero on fallback/launcher and return to three for split views.
The final moving window reports 36.9 FPS / 59.8 UPS, 457 exact groups and 7,203
scalable groups, with no renderer error. This live sample is corroboration,
not a matched ratio against earlier sessions.

On the same paused 5,280-shape frame, ordinary triangles report 26.7–26.8 FPS,
template-only reuse 36.1 FPS, eager/adaptive exact caching 48.0–49.1 FPS and
the hybrid 50.8–51.2 FPS over three switches back to it. Corresponding hybrid
encoding is 3.73–3.78 ms versus adaptive's 3.81 ms. CPU raster through FemtoVG
reports 48.2 FPS, not a comparison with software KMS. Live and paired paused
captures were inspected; the pre-existing rounded-menu-border issue remains.

No renderer error, automatic service restart or reboot occurred. Preferences
are restored byte-for-byte and the new binaries remain installed on
`sw-picade-2`. Normal software Clock is running at 60 FPS / 60 UPS. No other
device or default-renderer setting is changed.

## Refined outline reuse

The next experiment targets the small outlines still rebuilt during continuous
zoom. `retained-outlines` changes only the reuse domain, not canonical mesh
construction, source polygons, outline widths, camera scales or GPU submission.
The previous hybrid and all earlier controls remain available. Switching between
outline policies clears templates, exact meshes and probation so a permissive
entry cannot leak into the older control. Both representations still share the
128-entry and 16 MiB accounted-payload limits.

The guard follows the pinned `lyon_tessellation` 1.0.20 fixed-width, round-join
implementation in `stroke.rs`. The old rule used the full corner extension
divided by the shorter neighboring edge at every corner. Lyon folds a join only
at a sharp turn, and only when its projection reaches past **both** adjacent
edges. For unit-tangent dot product `d`, half-width `h`, and adjacent edge lengths
`a` and `b`, the relevant scale threshold is:

```text
merge = sqrt(max(1e-8, min(0.25² / 2, width² * 0.05)))
edge guard = 2 * merge / min(a, b)
fold guard = 2 * h * sqrt((1 - d) / (1 + d)) / max(a, b)
```

The new guard takes the maximum edge guard at all corners and fold guard where
`d < 1e-4`. That includes nearly right-angle turns conservatively, because
projected f32 arithmetic can change the sign near zero. It retains the 2× margin
and rounds the result upward. Duplicate/backtracking edges, near reversals,
self-intersections, invalid canonical coordinates and too-small scales still
fall back. Fill safety is unchanged. This is general straight-edge polygon
reuse, not a rectangle fast path, and must be rechecked when upgrading Lyon.

An explicit ignored diagnostic, `audit_zoom_outline_domains`, compares the two
domains on the real seed-zero close-up replay. In the zooming player's view,
4,445 hinted outlines include 3,872 rejected by the older guard. The tighter
guard rejects 908 / 964 / 908 at sampled ticks 1500 / 1560 / 1636; edge merging
alone accounts for 416 / 448 / 416. This is an acceptance audit, not a timing
result or a reason to discard the remaining safeguards.

The offscreen lab keeps the same error limits and adds unequal-edge and
shallow-turn fixtures. It samples either side of both guards and their half-scale
transitions, with rotation and overlapping transparency. It also records the
worst outline captures and per-mode errors. Real-game comparisons continue
through camera motion, scenario replacement, promotion and viewport changes.

With a GPU-enabled release binary, run the complete paired experiment using
a new output directory:

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-live-retained --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 128 --presentation-repeats 7 \
  --geometry-output /tmp/spacewars-outline-comparison
```

The output directory must not already exist. CSV timings go to stdout;
validation results and metadata go to stderr. On the Pi, complete the update
first, then pause/suspend the kiosk with guaranteed resumption as in the saved
experiment script; do not run the timed probe concurrently with kiosk rendering.

### Workstation outline results

The same three replay windows run for seven rotated repeats, with eight startup
frames and 128 measured frames per block. All seven modes see the same
within-device replay. Simulation, image readback, text/UI and scanout remain
outside the measured interval; source generation through `glFinish` remains
inside. Reported values are medians of the seven block averages, in milliseconds:

| Replay window | Ordinary | Cold exact | Eager exact | Adaptive exact | Across zoom | Previous hybrid | Refined outlines |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Seed 42, tick 120 | 1.983 | 2.171 | 0.715 | 0.716 | 0.979 | 0.719 | 0.721 |
| Seed 42, tick 1800 | 3.294 | 3.601 | 1.445 | 1.396 | 1.508 | 1.128 | 1.117 |
| Seed 0, close-up and changing zoom | 3.489 | 3.749 | 3.479 | 3.262 | 2.339 | 2.353 | 1.480 |

The changing-zoom window improves 37.1% against the previous hybrid and 54.6%
against adaptive exact caching. Thread CPU falls 2.029 → 1.171 ms and encoding
1.416 → 0.561 ms. Median block p95 falls 2.419 → 1.515 ms and maximum
2.728 → 1.741 ms. The steadier windows retain the earlier hybrid's benefit;
their small differences do not establish an additional steady-view speedup.

Outline fallback falls from 495,620 to 120,138 polygons over 128 measured
frames: about 3,872 → 939 per frame, a 75.8% reduction. Reused outlines rise
73,474 → 448,956. Cache payload remains 6,376,756 bytes in the zoom window,
and 6,070,960 / 7,372,392 in the earlier two windows. Canonical templates did
not change or grow to obtain this benefit. The later host window still has
53 evictions, unlike the smaller Pi geometry in earlier measurements; compare
policies within each device, not host/Pi absolute times.

The first refined frames cost 2.866 / 5.713 / 4.562 ms versus the previous
hybrid's 2.989 / 6.071 / 5.367 ms. Eight-frame startup totals are
8.531 / 15.925 / 15.128 ms versus 8.708 / 16.601 / 22.071 ms. Reduced guarded
work helps first use too, but does not eliminate template construction cost.

### Pi 4 outline results

The same seven-repeat schedule on `sw-picade-2`, in completed render milliseconds:

| Replay window | Ordinary | Cold exact | Eager exact | Adaptive exact | Across zoom | Previous hybrid | Refined outlines |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Seed 42, tick 120 | 22.991 | 25.141 | 12.542 | 12.590 | 14.753 | 12.469 | 12.456 |
| Seed 42, tick 1800 | 33.006 | 36.315 | 19.306 | 18.871 | 20.027 | 16.632 | 16.572 |
| Seed 0, close-up and changing zoom | 37.181 | 41.080 | 38.575 | 35.070 | 27.568 | 27.415 | 21.570 |

The zoom window improves 21.3% against the previous hybrid and 38.5% against
adaptive exact caching. Thread CPU falls 22.768 → 16.846 ms (26.0%); encoding
falls 13.622 → 7.294 ms. Median block p95 falls 28.307 → 22.264 ms and maximum
32.655 → 26.810 ms. The two steadier windows remain essentially unchanged.
These are uncapped, completed offscreen render times, not full application
frame times or a claim of sustained 60 FPS.

The Pi now rebuilds 120,146 guarded outlines over 128 measured zoom frames,
about 939 per frame versus 3,872 before. Reused outlines rise 73,474 → 448,948.
The acceptance audit and measured fallback totals are different views of the
same workload: the former samples single camera passes, while the latter
includes every measured frame and hybrid route. All 147 timing blocks pass
checks for matched workloads and unchanged hybrid cache counts, draws and
upload volume. The zoom window still has 64 entries, 6,376,756 accounted bytes,
one measured admission/promotion and no evictions. The earlier windows retain
6,070,960 / 9,239,608 bytes and no evictions. All modes use seven draws; uploads
are still roughly 1.73 / 2.77 / 3.17 MB per frame.

First-frame refined times are 30.040 / 57.333 / 46.199 ms versus the previous
hybrid's 30.819 / 61.019 / 51.710 ms. Eight-frame startup totals improve from
126.415 / 185.456 / 243.691 ms to 123.549 / 178.912 / 195.159 ms. This pays back
faster but is not a bounded-admission solution: the zoom fixture's first frame
still exceeds adaptive's 38.357 ms, and the later window still has a 57 ms
initial spike. Next prioritize admission cost and visibility/edit churn rather
than dropping safeguards to chase the remaining tiny outlines.

The remaining zoom costs include 2.006 ms source generation, 3.454 ms projection,
7.294 ms encoding and 2.420 ms submission. CPU expansion and per-frame vertex
upload remain; this experiment does not introduce persistent GPU geometry.
The clean Pi run starts at 66.2°C and ends at 68.7°C, with sampled frequency
1.5 GHz and power alarm zero at both ends. The governor remains ondemand, not
locked. Kiosk rendering is suspended during measurement, then resumed; settings
and boot ID are verified unchanged. An earlier attempt was stopped while the
update helper finished its backup copy and is excluded from all reported timing.

### Outline verification

Both devices pass 2,322 paired images: 1,242 outline cases and 1,080 real-game
frames. The new and previous hybrid have the same worst outline errors on each
device: host 0.024818/255 RGB MAE and 0.057292% above the pixel-error threshold;
Pi 0.024666/255 and 0.058594%. These expanded boundary checks expose a larger
worst case than the previous, smaller suite, not a new regression from the
refined guard. The stored worst pair is a tiny thick outline after exact-scale
promotion and rotation. The unchanged limits are 0.05/255 MAE and 0.1% of pixels
with summed RGB error above 30. This is bounded image agreement, not a claim of
byte-identical tessellation after rotation.

Real-game maxima remain host 0.000348/255 and 0.002289%, Pi 0.000685/255 and
0.003560%. Tests cover both hybrid policies for continuous zoom in two views,
cross-frame promotion, fill/stroke ordering, geometry replacement and bounded
memory fallback. New tests cover the refined threshold, winding, scale changes,
identical canonical construction and clearing both cache representations when
switching outline policies. The full client suite passes 570 tests with two
ignored tests, including the new explicit domain diagnostic. The diagnostic
also passes when invoked manually. CLI tests (22), backend startup tests (3),
software-only compilation, formatting, Clippy and both release builds pass;
existing warnings remain and none points to the changed cache/probe modules.

Raw CSVs, checks, captures and build/test logs are retained in
`target/gpu-experiments/outline-domains.y6Gd6xMn/`. Client hash prefixes are
`25f03d0a616060cb` (host) and `7d0cd2e4ac78f3f9` (Pi). The replay metadata version
is 5. Seven rotated repeats balance all seven rendering policies; timing results
are from release binaries after this task's builds/tests completed. Other
workstation tasks were active, so the host result is corroborating evidence,
not a claim of an isolated-system benchmark.

The live Pi check passes Clock, launcher, a moving close-up Spacewars session,
pause/resume, all retained policies, ordinary triangles, circles, CPU-image
fallback and return to launcher. Targets retire to zero on fallback/launcher
and return to three for split-screen GPU rendering. The final moving sample
reports 48.0 FPS / 59.8 UPS, 6.303 ms encoding, 720 exact groups and 7,200
scalable groups, with no renderer error. This corroborates the replay result;
it is not a matched speed ratio against earlier live sessions.

On one paused 5,264-shape frame, ordinary triangles report 27.3 FPS, adaptive
exact caching 47.8 FPS, the previous hybrid 49.8 FPS and refined outlines
49.3–50.0 FPS across three switches. Refined encoding is 3.619–3.722 ms versus
the previous hybrid's 3.823 ms. Stable-scale promotion makes both hybrid modes
similar here, as expected. Paired synthetic, real replay and live screenshots
were inspected; the previously documented rounded-menu-border issue remains.

No renderer error, automatic service restart or reboot occurs. The new client
and CLI remain installed, preferences are restored byte-for-byte, and normal
software Clock is running at 60 FPS / 60 UPS with no resident scene-GPU targets.
Only `sw-picade-2` is updated. Software remains the default.

## Paced geometry admission and terrain churn

This experiment measures cache construction separately and adds an optional
`retained-paced` policy. It uses the same refined outlines and exact-scale
promotion as `retained-outlines`, but spreads new cache admissions across frames.
The reference, cold, eager, adaptive and earlier hybrid policies remain available.
No application defaults change.

An ignored diagnostic, `profile_cold_admissions`, resolves production replay
groups individually. The debug audit finds 27 / 92 / 60 initial admissions in
the early, late and zoom windows, respectively. The largest individual groups
have a few hundred polygons; the total spans many groups. Debug timing is used
only to identify that distribution, not for the release performance comparison.

The work estimate is the sum of squared polygon edge counts, accounting for
the scalable builder's pairwise shape validation. An 8,192-unit allowance is
shared by every camera pass in a presentation frame. Hits are free. Both new
templates and exact-scale promotions consume the allowance. A deferred group
still draws the authoritative ordinary geometry, or its existing template if
only promotion was deferred. There is no pending queue or retained future scene.

This is a deterministic pacing heuristic, **not a millisecond deadline**. One
indivisible oversized group may build in an otherwise unused frame, so size alone
does not exclude it; it is counted separately. This is not a fairness scheduler:
earlier groups under sustained churn can keep later groups on the ordinary path.
Failed cache-size admissions
also consume work. Entry and accounted-payload limits remain 128 and 16 MiB.
Changing geometry or material still replaces its immutable identity immediately.

New diagnostics expose aggregate build time, maximum single-admission time,
estimated work, deferred attempts and oversized admissions. Deferred attempts
are not unique groups: a denied promotion and denied template can count twice.
Live status aggregates over the existing 120-frame reporting window. The replay
CSV separates first-frame and eight-frame startup build/defer counters from
the subsequent measured interval.

The new `--geometry-churn` probe uses production terrain artwork with prescribed
motion, zoom, chunk hiding/revealing, two circular edits, and complete world
replacement. This visibility schedule is a stress fixture, not a new culling
implementation. Field edits and collision reconstruction are prepared outside
timing; source artwork construction, generation-to-definition updates,
validation, projection, mesh encoding, upload and `glFinish` remain inside.
All frames are measured, with no discarded cache warmup. GPU targets are
prewarmed separately. Per-frame CSVs expose the 16-frame phase boundaries.

Run both release probes with new output directories; timings go to stdout and
validation/metadata to stderr:

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-live-retained --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 64 --presentation-repeats 8 \
  --geometry-output /tmp/spacewars-paced-replay

target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-churn --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 128 --presentation-repeats 4 \
  --geometry-output /tmp/spacewars-paced-churn
```

Eight replay repeats rotate all eight policies through every ordering position;
four churn repeats do the same for ordinary, adaptive, outlines and paced modes.
The replay window is shorter than the earlier seven-policy trial, so compare
policies within this run rather than interpreting cross-run differences as
improvements. Churn runs cover two complete visibility/edit cycles.

### Workstation admission results

Medians of eight release replay blocks, in milliseconds. The next 64 frames
are reported after the separate eight-frame startup interval; that does not
imply the paced cache has finished warming up by frame eight.

| Replay | First frame outlines | First frame paced | Startup total outlines | Startup total paced | Next 64 average outlines | Next 64 average paced |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Early | 2.882 | 2.400 | 8.506 | 12.723 | 0.746 | 0.752 |
| Late | 5.758 | 4.754 | 15.986 | 29.850 | 1.163 | 1.245 |
| Changing zoom | 4.609 | 3.632 | 15.030 | 23.210 | 1.488 | 1.522 |

The first-frame build subtotal falls from 1.832 / 3.779 / 2.868 ms to about
0.324 / 0.323 / 0.326 ms. The total frame improves much less: deferred groups
still need ordinary rendering, and startup totals become worse. In the late
window, next-64 p95 rises 1.332 → 2.144 ms. In the zoom window it rises
1.552 → 1.677 ms, and maximum rises 1.625 → 2.064 ms. Lower initial latency
alone therefore does not establish smoother overall rendering.

The four-repeat terrain stress run includes cold start and every phase boundary:

| Fixture | Ordinary average | Adaptive average | Outlines average | Paced average | Outlines maximum | Paced maximum |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Reveal and continuous zoom | 1.240 | 1.265 | 0.661 | 0.665 | 2.629 | 1.912 |
| Edits and world replacement | 0.723 | 0.282 | 0.279 | 0.301 | 1.300 | 1.111 |

These maxima are medians of block maxima. Pacing reduces the largest stalls
but increases edit-fixture average cost and both fixtures' p95. Continuous zoom
correctly bypasses adaptive exact caching entirely; the reusable templates
still nearly halve average render time. Edit/replacement caching remains useful
despite eviction: outlines admit 384 entries and evict 256 over 128 frames;
pacing admits 308 and evicts 180. Different admission histories can change
resident payload without changing the rendered scene. All 192 replay and 32
churn blocks pass source-work and memory-bound checks.

### Pi 4 admission results

The same eight-repeat replay on `sw-picade-2`, in completed render milliseconds:

| Replay | First frame outlines | First frame paced | Startup total outlines | Startup total paced | Next 64 average outlines | Next 64 average paced |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Early | 30.415 | 25.745 | 125.484 | 159.573 | 12.853 | 12.887 |
| Late | 49.450 | 42.043 | 170.335 | 235.144 | 16.407 | 16.675 |
| Changing zoom | 45.185 | 37.214 | 193.878 | 259.343 | 21.355 | 21.642 |

Pacing reduces the first frame by 15–18%, but increases the eight-frame total
by 27–38%. First-frame cache building falls from 15.059 / 23.214 / 23.012 ms to
2.697 / 2.600 / 2.601 ms. The remaining frame still pays for uncached groups;
several following frames continue to build and render them. In the zoom window,
next-64 p95 rises 22.145 → 22.857 ms and maximum 22.722 → 26.069 ms. Thread CPU
rises slightly, 16.685 → 16.943 ms. This is not an overall speed improvement.

The earlier refinement still holds in this matched run: changing zoom takes
37.048 ms ordinary, 35.416 ms adaptive, 27.444 ms previous hybrid and 21.355 ms
with refined outlines. Pacing is a separate latency/memory tradeoff, not a
replacement for that improvement. Its later admissions sometimes skip templates
and directly cache now-stable exact scales: early/late resident payload falls
6.071 / 8.827 MB → 2.803 / 3.576 MB. Zoom ends at 6.377 MB in both modes.
These are accounted payloads, not process RSS. All 192 blocks have matched source
work, bounded cache memory and seven GPU draws per frame.

The four-repeat terrain run includes all cold and edited frames:

| Fixture | Ordinary average | Adaptive average | Outlines average | Paced average | Outlines maximum | Paced maximum |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Reveal and continuous zoom | 16.393 | 16.541 | 11.569 | 11.620 | 27.602 | 23.245 |
| Edits and world replacement | 10.994 | 8.097 | 8.151 | 8.233 | 19.579 | 17.596 |

Pacing reduces median block maxima 16% / 10%, with small average-cost penalties.
Reveal/zoom p95 rises 14.322 → 14.925 ms; edit/replacement p95 falls slightly,
12.294 → 11.915 ms. The result is workload-dependent, not a universal tail-latency
improvement. Both devices pass checks on all 4,096 per-frame churn records,
including work allowances, counters, phase boundaries and memory bounds. No
oversized admission is needed by these measured fixtures; the oversized path
is covered separately by a unit test.

Prefer `retained-outlines` for the next experiment. Keep `retained-paced`
available to reproduce this tradeoff, but do not enable it by default. First
measure where template construction spends its time, then reduce the work or
avoid repeated ordinary construction during warmup. Simply tightening the
allowance cannot eliminate that cost. Persistent GPU geometry remains a longer
term option, with the earlier ordered/interleaved submission results still
relevant: trading CPU work for hundreds of small draws was not a general win.

### Admission verification and evidence

Each device passes 3,432 image comparisons: 1,656 outline cases, 1,296 real-game
frames and 480 terrain-history frames. Worst outline errors are unchanged:
host 0.024818/255 MAE and 0.057292% differing pixels; Pi 0.024666/255 and 0.058594%.
Real-game maxima are host 0.000348/255 and 0.002289%, Pi 0.000685/255 and 0.003560%.
Terrain-history maxima are host 0.000094/255 and 0.000636%, Pi 0.000148/255 and
0.000763%. The same 0.05/255 MAE and 0.1% pixel-fraction limits apply throughout;
the latter counts pixels with summed RGB error above 30. No threshold was relaxed.

The 572 client tests, 22 CLI tests and three backend-startup tests pass. Three
client tests remain explicitly ignored, including the new admission audit,
which also passes when invoked manually. Hybrid regression tests now include
the paced policy. Focused tests cover shared view budgets, ordinary fallback,
immediate edited-material replacement, later admission, oversized progress,
free hits and counter aggregation. Formatting, software-only checking, Clippy
and host/Pi release builds pass with existing warnings.

Raw CSVs, per-frame traces, paired captures and build/test/deployment logs are
in `target/gpu-experiments/admission-churn.g5XCo0aV/`. The replay metadata is
version 6; the terrain-history probe is version 1. Client hash prefixes are
`243681864ff12525` (host) and `5c24f818b84224fb` (Pi). All measured comparisons
use release executables after this task's builds/tests finished; other host
jobs were active, so the workstation is not an isolated machine. Pi kiosk
rendering is suspended for the offscreen run and resumed by a cleanup trap.
Preferences and boot ID remain unchanged. Sampled temperatures are 64.3°C
before, 68.7°C after replay and 66.7°C after churn, with power alarm zero. The
governor remains ondemand; sampled frequencies vary 1.2–1.5 GHz, not fixed clocks.

The live check exercises Clock, moving Spacewars, pause, ordinary/adaptive/
outlines/paced/circles/software switches, resumption and return to launcher.
Use `spacewars-cli host gpu-scene retained-paced` or `retained-outlines` to
select the experimental paths on a running GPU backend; these requests do not
persist. The new counters report build/defer work during activation, with no
renderer errors. Split-screen uses three resident targets; software fallback
and the launcher release them. Live and paired captures were inspected; the
existing rounded-menu-border issue is unchanged.

Only `sw-picade-2` is updated. No reboot or automatic service restart occurs.
The new client/CLI remain installed, preferences are restored byte-for-byte,
and normal software Clock is back at 60 FPS / 60 UPS, with zero scene-GPU
targets. The post-check kiosk PID is 5482 and `NRestarts=0`.

## Exact repeated-polygon construction reuse

The construction probe, `--geometry-build-profile`, freezes the first source
frame of each production replay and builds every unique immutable definition.
It retains all templates until the batch ends, including their destruction in
wall/thread-CPU timing. Simulation, source generation, projection, cache
insertion, GL, UI and scanout are outside this microbenchmark. These are **not
frame times**. Detailed domain-check, point-preparation, fill, stroke, reuse and
finish timers are compiled out of unprofiled builds; compare the unprofiled
policies to evaluate speed, not instrumented subtotals alone.

`retained-memoized` keeps the refined-outline hybrid policy but reuses exact
repeated polygon tessellations while building one definition. It is general
polygon reuse, not a rectangle fast path. The construction-only dictionary is
bounded to 128 keys, uses ranges into the mesh already being built, and is
discarded at the end of that build. It adds no persistent template collection,
sorting, extra GPU draws, or framebuffer resolution changes.

Keys contain ordered canonical point bits, scale, fill/stroke presence, complete
colors, and outline width. There is no fuzzy matching. Every polygon still runs
its own original-coordinate safety checks, even on a hit. Copies use those
newly computed guards; rejected geometry keeps ordinary fallback. Coordinates
stay relative until construction completes, then receive their own polygon
origin exactly once, preserving the original arithmetic. Copying checks vertex
and index budgets before expansion. A full dictionary stops admitting new keys
but can still hit existing ones; misses tessellate normally.

The session-only CLI switch is:

```sh
spacewars-cli host gpu-scene retained-memoized
```

It requires the GPU build/backend, like the other modes. `retained-outlines`
remains the non-memoized comparison, and `retained-paced` remains the separate
admission-pacing experiment. Software stays the application default.

Reproduce release measurements with fresh output directories:

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-build-profile --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 32 --presentation-repeats 4 \
  --geometry-output /tmp/spacewars-construction-profile

target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-live-retained --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 64 --presentation-repeats 9 \
  --geometry-output /tmp/spacewars-memoized-replay

target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-churn --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 128 --presentation-repeats 5 \
  --geometry-output /tmp/spacewars-memoized-churn
```

The four construction modes rotate plain, profiled, memoized and
memoized-profiled through every position. Before timing, all production
definitions must produce equal vertices, indices and guard thresholds. Nine
replay and five churn repeats balance all respective policy orderings. The
replay separately reports cold first-use, eight-frame startup and the next 64
frames. Churn includes all 128 frames, including cold start and two complete
visibility/edit/replacement cycles. New `build_reuses` counters count polygons
whose construction tessellation was skipped, not per-frame cache hits.

### Construction measurements

Medians of four rotated blocks, 32 batches per block. Milliseconds below are
unprofiled whole-batch wall time, including destruction. Build inputs remain
equal across modes within each machine; the simulated late replay differs
between host and Pi, so do not interpret cross-platform ratios as a controlled
CPU comparison.

| Device / replay | Polygons | Reused construction | Original ms | Memoized ms |
| --- | ---: | ---: | ---: | ---: |
| Workstation early | 2,837 | 1,407 | 1.904 | 1.459 |
| Workstation late | 5,887 | 2,854 | 3.939 | 3.016 |
| Workstation zoom | 4,445 | 2,166 | 2.871 | 2.245 |
| Pi 4 early | 2,837 | 1,407 | 15.826 | 13.741 |
| Pi 4 late | 4,419 | 2,168 | 24.083 | 20.883 |
| Pi 4 zoom | 4,445 | 2,166 | 23.953 | 20.668 |

This saves 22–23% on the workstation and 13–14% on the Pi. It skips tessellation
for about half the polygons without changing mesh payload. The profiled Pi
baseline spends 4.99 / 7.64 / 7.78 ms on fill and 7.95 / 12.08 / 11.95 ms on
stroke in these windows. Together that is roughly 75% of instrumented batch
time. Domain checks take 1.95 / 3.11 / 3.10 ms; weakening guards is not the main
opportunity.

Reuse is not free. In the detailed Pi run, key lookup/copy/insertion costs
3.19 / 4.96 / 4.45 ms. Relative-coordinate finalization also increases the
finish stage. That consumes much of the tessellation saving. Detailed timers
themselves add roughly 8–10% to baseline Pi batches and 23–25% on the host;
the uninstrumented comparison above is the performance evidence. This does
not establish a win for definitions containing only unique polygons: those
still pay dictionary/finalization overhead. The mode remains optional.

### Workstation frame results

Medians of nine release replay blocks, comparing refined outlines with the new
construction reuse. All cache admissions and GPU upload bytes match.

| Replay | First frame original | Memoized | Eight-frame total original | Memoized | Next 64 average original | Memoized |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Early | 2.975 | 2.579 | 8.764 | 8.306 | 0.752 | 0.757 |
| Late | 6.128 | 5.259 | 16.587 | 15.569 | 1.156 | 1.151 |
| Changing zoom | 4.536 | 3.922 | 14.942 | 14.322 | 1.472 | 1.478 |

First-frame build time falls 1.876 → 1.464, 3.876 → 3.038 and 2.848 → 2.201 ms.
Unlike pacing, startup totals improve. Warm rendering is essentially unchanged,
as expected for a construction-only optimization; do not call tiny differences
a steady-state speedup. Zoom p95 is 1.516 → 1.536 ms, maximum 1.585 → 1.634 ms.

The five-repeat terrain run includes all 128 cold/edited frames. Reveal/zoom
average remains 0.652 ms, while median block maximum falls 2.400 → 1.877 ms.
Edit/replacement average improves 0.282 → 0.266 ms, p95 0.705 → 0.598 ms, and
median maximum 1.283 → 1.038 ms. It skips construction of 1,486 polygons in
the reveal fixture and 4,712 over repeated edits/replacements, with the same
admission/eviction history as refined outlines. These are matched within-run
comparisons, not claims about differences from earlier experiment binaries.

### Pi 4 frame results

Nine matched release replay repeats on `sw-picade-2`, in completed offscreen
render milliseconds. This includes source generation through `glFinish`, not
simulation, text/UI or scanout.

| Replay | First frame original | Memoized | Eight-frame total original | Memoized | Next 64 average original | Memoized |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Early | 30.168 | 28.437 | 125.600 | 122.032 | 12.795 | 12.783 |
| Late | 49.226 | 46.445 | 170.935 | 168.042 | 16.430 | 16.425 |
| Changing zoom | 45.008 | 42.415 | 193.986 | 191.706 | 21.438 | 21.509 |

First frames improve consistently by about 5.7%; startup totals improve 1–3%.
The build subtotals fall 14.885 → 12.923, 23.102 → 20.012 and 22.870 → 19.882 ms.
The total frame saves less than the construction microbenchmark because all
the other work still runs. Warm changing-zoom p95 is 22.183 → 22.178 ms and
median maximum 23.048 → 22.892 ms; thread CPU is 16.758 → 16.838 ms. Treat warm
performance as unchanged, not a new frame-rate improvement.

This is a modest first-use improvement without pacing's longer warmup. It does
not solve the remaining changing-zoom cost: mesh encoding still takes about
7.33 ms per warm zoom frame. The same run takes 36.663 ms ordinary, 34.708 ms
adaptive and 27.376 ms with the older hybrid. The earlier outline refinement
remains the larger steady-state improvement. Cache payload, source work,
admission history, upload bytes and seven draws per frame match between
refined outlines and memoized construction in all 243 replay blocks.

The Pi's five-repeat terrain-history run includes cold frames and edits:

| Fixture | Average original | Memoized | p95 original | Memoized | Median block maximum original | Memoized |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Reveal / changing zoom | 11.535 | 11.475 | 14.191 | 14.217 | 27.602 | 24.083 |
| Edits / world replacement | 8.112 | 8.034 | 11.823 | 10.806 | 20.446 | 18.766 |

The largest stalls improve about 13% and 8%; average improvement is under 1%.
First-frame build time falls 13.191 → 9.550 ms and 6.293 → 4.674 ms. The workload
still performs the same 128 frames and source updates, with the same 1,486 /
4,712 construction reuses as the host. No delayed-admission warmup is introduced.

### Construction verification

Both machines pass 4,492 paired image comparisons: 2,340 adversarial zoom checks
(including translated, overlapping translucent concave copies), 1,512 real-game
frames, and 640 terrain-history frames. Worst outline errors remain host
0.024818/255 MAE with 0.057292% differing pixels, Pi 0.024666/255 with 0.058594%.
Real-game maxima remain host 0.000348/255 and 0.002289%, Pi 0.000685/255 and
0.003560%. Terrain-history maxima remain host 0.000094/255 and 0.000636%, Pi
0.000148/255 and 0.000763%. Limits are unchanged: 0.05/255 MAE and 0.1% of pixels
with summed RGB error over 30. The construction probe also checks equal mesh
vertices, indices and guards directly, not only images.

All 576 client tests, 22 CLI tests and three backend-startup tests pass; the same
three diagnostic/visual audits remain explicitly ignored. New tests cover
profiled/unprofiled equality, disabled timers, translated/reversed/concave
geometry, transparent and changed styles, rejected polygons, bounded dictionary
admission and both copy-expansion budgets. Existing hybrid lifecycle tests now
include the new mode. Formatting, software-only checking, Clippy, and host/Pi
release builds pass with existing warnings. All 48 construction blocks, 243
replay blocks and 50 terrain blocks per device pass source/payload/counter
checks, including 6,400 per-frame terrain records on each device.

Evidence is in `target/gpu-experiments/build-cost.PZOhzfQF/`, including raw CSVs,
per-frame traces, captures and build/test/deploy logs. Metadata versions are
construction 2, replay 7 and terrain-history 2. Client hash prefixes are
`c8da879196318c77` (host) and `3a0d4369469e3100` (Pi). Host timing is not an
isolated-machine result; the initial host construction sample overlaps a Pi
cross-build. Pi measurements suspend kiosk rendering, then resume through a
cleanup trap. No clock-governor setting is changed.

Pi temperature samples are 64.8°C before, 68.7°C after the construction probe,
69.6°C after replay and 67.2°C after terrain churn. Current power alarm remains
zero; the ondemand governor samples 1.5 GHz during those boundaries. Live checks
exercise Clock, moving Spacewars, pause/resume, ordinary/adaptive/outlines/
memoized/circles/software switches and return to launcher. The new construction
counter is exercised, with no renderer errors. Clock uses one target, split
screen uses three, and software/launcher release them. Live captures were
inspected; the existing rounded-menu-border issue remains.

Only `sw-picade-2` was updated. The new client/CLI remain installed. Boot ID and
saved-preferences checksum are unchanged, the service is active with no automatic
restarts, and ordinary software Clock is restored at 60 FPS / 60 UPS with zero
resident scene-GPU targets.

Keep this as an optional construction experiment. The next section splits warm
mesh encoding into reused-vertex expansion versus remaining guarded
tessellation. Do not infer that a larger
dictionary, weaker geometry guards, or a new GPU submission scheme will help
from these measurements alone.

## Warm encoding breakdown and bulk validity scan

`--geometry-encode-profile` advances the same three production replays, but
times only CPU mesh encoding across their camera/minimap passes. Simulation,
source generation, projection, GPU upload/drawing, UI and scanout are excluded.
Each fresh scenario/cache receives eight untimed startup frames. Plain,
profiled, scan and scan-profiled blocks rotate through every order position;
per-piece clocks and diagnostic counters compile out of the unprofiled paths.
Do not confuse this timing boundary with the full renderer's encoding cost
under simultaneous GPU/memory traffic.

The initial Pi baseline (four repeats, 64 warm frames, 1024×768) measured:

| CPU encoding, ms/frame | Early bots | Late bots | Changing zoom |
| --- | ---: | ---: | ---: |
| Uninstrumented total | 1.949 | 2.895 | 6.156 |
| Instrumented total | 1.985 | 3.074 | 7.138 |
| Cache lookup/build | 0.011 | 0.034 | 0.104 |
| Exact-mesh expansion | 0.868 | 1.101 | 0.133 |
| Scalable-mesh expansion | 0 | 0.264 | 1.932 |
| Guarded stroke tessellation | 0 | 0.026 | 2.363 |
| Ordinary/unhinted geometry | 0.607 | 0.746 | 0.667 |
| Finished-mesh validity scan | 0.433 | 0.645 | 0.792 |

Substage clocks are diagnostic attribution, not additive estimates of the
uninstrumented total. Timer reads, loop bookkeeping and validation of retained
ranges are outside some substages. Detailed instrumentation adds about 16% to
Pi changing-zoom encoding (and 59% on the faster workstation). In this Pi window,
zoom averages 94,072 output vertices and 949 guarded stroke tessellations per
frame, with no guarded fill tessellations. Cache lookup/construction is small
once warm; expanding reused geometry and rebuilding guarded outlines remain
substantial. Host and Pi simulations can diverge numerically: comparisons must
use matched runs **on the same architecture**, not compare their late-match
vertex totals as identical workloads.

The next isolated experiment is `retained-scan`. It keeps the memoized hybrid's
geometry, styles, guards, cache budget, order, uploads and submission unchanged.
Only the final finite-number check changes. The original scan branches at each
number and exits at the first invalid one. The new scan uses `is_finite()` on
flat slices and a non-short-circuit boolean reduction. Infinity and every NaN
representation still fail; signed
zero, subnormals and finite extremes still pass. This uses flat borrowed slices,
with no allocation, unsafe code, floating-point arithmetic or relaxed-math
assumptions. The cost of invalid input is a complete bounded scan instead of an
early exit. Both vertices and circle instance fields remain checked.

```sh
spacewars-cli host gpu-scene retained-scan
# Original finite-number scan, otherwise the same renderer:
spacewars-cli host gpu-scene retained-memoized
```

Reproduce with fresh output directories:

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-encode-profile --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 64 --presentation-repeats 4 \
  --geometry-output /tmp/spacewars-warm-encoding

target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-live-retained --geometry-live-modes retained-memoized,retained-scan \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 64 --presentation-repeats 6 \
  --geometry-output /tmp/spacewars-scan-replay

target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-churn --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 128 --presentation-repeats 6 \
  --geometry-output /tmp/spacewars-scan-churn
```

`--geometry-live-modes` limits timed blocks only; every retained mode still
receives the complete image-parity suite. Omit it to time all modes as before.
The CPU probe compares exact vertices, indices, batches and cache decisions for
3,240 pass comparisons before timing. Unit tests check invalid values in every
vertex/circle component and retain the existing bounded-cache, two-view zoom,
promotion, tiny-shape fallback and painter-order checks for the new mode.

### Scan measurements and rejected variants

Matched release Pi runs, medians of four rotated CPU-profile blocks and six
alternating full-replay blocks. These compare `retained-memoized` with
`retained-scan` **in the same binary**. Full rendering includes source,
projection, mesh encoding, upload/draw and `glFinish`, but not simulation,
Slint text/UI or physical presentation. No frame-rate cap is used.

| Pi milliseconds | Early bots | Late bots | Changing zoom |
| --- | ---: | ---: | ---: |
| CPU-only encoding, old → scan | 1.995 → 1.928 | 2.904 → 2.766 | 6.203 → 6.088 |
| Instrumented validity stage | 0.441 → 0.380 | 0.642 → 0.494 | 0.799 → 0.676 |
| Full render wall/frame | 12.840 → 12.776 | 16.512 → 16.282 | 21.487 → 21.295 |
| Full render thread CPU/frame | 8.298 → 8.181 | 11.870 → 11.672 | 16.768 → 16.563 |
| Encoding inside full renderer | 2.306 → 2.201 | 3.417 → 3.153 | 7.254 → 7.013 |
| Render wall p95 | 13.795 → 13.595 | 17.304 → 16.965 | 22.514 → 22.118 |
| Render wall maximum | 14.552 → 14.380 | 17.970 → 18.439 | 23.613 → 22.640 |

This is a **small improvement**, not the main zoom bottleneck: 0.5–1.4% lower
full-render averages on Pi. Do not claim a universal tail-latency improvement;
the late-match block maximum increases in this sample. First-use construction
is unchanged and still dominates cold frames. Terrain stress is essentially
flat: reveal/zoom averages 11.392 → 11.429 ms, and edits/replacement average
8.045 → 8.025 ms, with no consistent tail/startup win. The cache decisions,
geometry, uploaded bytes and seven composite draw calls do not change.

On the workstation, uninstrumented CPU-only encoding changes 0.137 → 0.118 ms,
0.245 → 0.213 ms and 0.558 → 0.525 ms. Full rendering changes 0.742 → 0.723 ms,
1.171 → 1.134 ms and 1.550 → 1.511 ms (about 2.5–3.2%). Treat these as
corroborating measurements on a shared workstation, not an isolated hardware
benchmark. Pi measurements are the deployment evidence.

An attempted integer-maximum scan is **rejected**. It tested the maximum
sign-cleared float bits against `f32::MAX`. Despite equivalent results, it made
workstation zoom encoding worse (0.557 → 0.613 ms, versus 0.525 ms for the
simple boolean scan). A separate 94,072-vertex repeated-scan microbenchmark
found essentially memory-bound Pi costs: boolean 0.446 ms, integer maximum
0.438 ms, and exponent-carry/OR 0.450 ms. The latter bit trick is not worth
retaining either. These hot repeated-buffer timings are **not frame timings**.
Keep the clear `is_finite()` reduction; the Pi release disassembly confirms
NEON loads/comparisons/reduction without architecture-specific intrinsics.

### Encoding verification and device state

Both devices pass 3,240 exact CPU mesh comparisons and 5,336 image comparisons
(2,808 outline/zoom, 1,728 live-replay and 800 terrain-history checks).
The previous ordinary-renderer tolerances remain unchanged: 0.05/255 mean RGB
error and at most 0.1% of pixels with summed RGB error above 30. All 38 saved
offscreen memoized/scan PNG pairs on each device are byte-identical. Worst errors across
all compared policies are 0.024818/0.057292% on the workstation and
0.024666/0.058594% on Pi, from the existing outline/zoom suite, not introduced
by changing the scan.

For each device, raw-data checks cover 48 CPU-profile blocks, 36 timed live
blocks, 72 terrain blocks and 9,216 per-frame terrain rows. Geometry/upload/cache
counts match between scan and baseline, memory remains within 128 entries /
16 MiB, and recorded per-frame totals agree with block totals. The new tests
avoid wall-time assertions. Client tests: 578 passed, three existing ignored;
CLI tests: 22 passed; backend-startup tests: three passed. Software-only check,
formatting and whitespace checks pass; Clippy completes with the existing
35 client warnings and one vendored backend warning.

Artifacts are under `target/gpu-experiments/warm-encode.GPi3qqDp/`. The retained
implementation's results are `host-scan-profile.csv`, `host-native.csv`,
`host-churn.csv`, and `pi-boolean/`; the `*-reduction-*` files are the **rejected
integer-maximum experiment**, not the retained implementation. Live captures
and status are in `pi-live/`. The temporary kernel source/results are retained
for diagnosis but are not a new production benchmark API.

Only `sw-picade-2` was updated. Pi temperature samples are 66.2°C before the run,
68.2°C after CPU profiling, 67.7°C after replay and 65.2°C after terrain stress.
The ondemand governor samples 1.5 GHz, with no current power alarm. Live checks
exercise Clock, moving split-screen Spacewars, memoized/scan/triangles/software
switches while paused, resume, and return to launcher. Captures were inspected;
no new visual difference is apparent, and the existing rounded-menu-border
issue remains. GPU targets are released in software and launcher modes.

The new client/CLI remain installed. Boot ID and saved-preference checksum are
unchanged; ordinary software Clock is restored at 60 FPS / 60 UPS with zero
resident scene-GPU targets and no service restarts beyond explicit updates and
renderer switches. The next worthwhile investigation is the remaining guarded
outline work and/or cached-vertex expansion, not another validity-scan variant.
Keep the zoom safety margins and general ordered geometry path intact.

## Contiguous cached-geometry expansion

The ignored `audit_zoom_outline_domains` diagnostic now also checks whether the
remaining guarded outlines repeat within a view. At zoom-replay ticks 1500,
1560 and 1636, the bot view contains respectively 908, 964 and 908 guarded
outlines. Every exact translated local-point key is distinct, including after
removing color from the key. Exact translated projected-point keys are also
distinct. This rejects a frame-local dictionary of these ordered shapes as
the next useful optimization; it does not rule out reuse across time or under
other transformations. Do not quantize the tiny outlines to manufacture hits.

The alternative experiment, `retained-ranges`, keeps `retained-scan`'s
memoized construction, finite-number reduction, domain guards, cache budgets
and exact-scale promotion. It combines adjacent safe fill/stroke ranges within
one placement before transforming their vertices and rebasing their indices.
Every piece still passes its own scale guard. Flush before any guarded
tessellation, any gap in the source ranges, and the end of the placement.
Skipped styles cannot cause a nonempty range to be drawn accidentally. The
fill/stroke/polygon order is unchanged, including translucent interleaving.

This is CPU loop/append coalescing, not a new shape fast path or GPU batching
policy. The same arithmetic produces the same vertex bits and indices; GPU
draw count, uploaded bytes and cache payload are unchanged. The only pending
metadata is a pair of ranges on the stack. Both output-buffer budgets still
apply before expansion. `retained-scan` retains the per-piece comparison path:

```sh
spacewars-cli host gpu-scene retained-ranges
spacewars-cli host gpu-scene retained-scan
```

The CPU encoding probe now rotates six modes: plain/profiled, scan/scan-profiled
and ranges/ranges-profiled. Its version-3 CSV adds `scalable_ranges`, counting
expansion operations in the instrumented modes. In the changing-zoom replay,
about 7,946 per-piece operations become 1,001 contiguous operations, with the
same approximately 949 guarded strokes. Per-operation timers are therefore
much less intrusive in the coalesced path. **Use the uninstrumented totals for
speed comparisons**, not the reduction in instrumented expansion time alone.

Reproduce with new output directories:

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-encode-profile --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 64 --presentation-repeats 6 \
  --geometry-output /tmp/spacewars-range-encoding

target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-live-retained --geometry-live-modes retained-scan,retained-ranges \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 64 --presentation-repeats 6 \
  --geometry-output /tmp/spacewars-range-replay

target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-churn --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 128 --presentation-repeats 6 \
  --geometry-output /tmp/spacewars-range-churn
```

### Range-coalescing measurements

Pi 4 / V3D, native 1024×768, six rotated blocks of 64 warm frames. Values are
medians of block averages, comparing `retained-scan` → `retained-ranges` in the
same release binary. CPU-only measurements exclude source/projection/GPU work;
full rendering includes source through `glFinish`, but not simulation, UI or
display scanout.

| Pi measurement, ms/frame | Early bots | Late bots | Changing zoom |
| --- | ---: | ---: | ---: |
| CPU-only encoding, no detailed timers | 1.905 → 1.914 | 2.743 → 2.679 | 6.090 → 5.854 |
| Full-render wall | 12.702 → 12.630 | 16.348 → 16.260 | 21.282 → 20.991 |
| Full-render thread CPU | 8.168 → 8.128 | 11.666 → 11.598 | 16.488 → 16.253 |
| Full-render encoding | 2.206 → 2.202 | 3.155 → 3.097 | 7.040 → 6.802 |
| Block p95 wall, median | 13.637 → 13.555 | 17.204 → 17.022 | 22.242 → 21.970 |
| Block maximum wall, median | 14.190 → 14.107 | 18.552 → 17.920 | 22.922 → 22.302 |

This saves about 0.24 ms of zoom encoding (3.9% CPU-only) and 0.29 ms of full
zoom rendering (1.4%), not a large GPU throughput change. The early replay has
no scalable vertices during its warm measurement window, so its small movement
is a control/noise result, not a coalescing win. Late Pi expansion calls drop
from 1,188 to 15 per warm frame; continuous zoom drops 7,945.66 to 1,001.55.
Zoom still transforms the same 67,494 scalable vertices, tessellates the same
948.53 guarded strokes, and uploads the same 3,136,987.5 bytes/frame across
seven draws. No guarded fill tessellations are removed.

The host confirmation, after this task's builds/tests completed, changes
CPU-only zoom encoding 0.546 → 0.524 ms and full zoom rendering 1.524 → 1.478 ms.
Other host results are mixed: full early/late rendering changes 0.805 → 0.814
and 1.214 → 1.250 ms, while their CPU-only results are effectively flat. The
workstation is shared and source/projection costs also move; this does not
establish a universal speedup. Keep the mode opt-in and preserve the baseline.

The 128-frame cold/edit/visibility histories are also mixed. Pi reveal/zoom
averages 11.473 → 11.399 ms, with the median block maximum essentially flat
(23.617 → 23.652 ms). Edit/replacement averages 7.974 → 8.056 ms; its median
block maximum worsens 16.989 → 18.167 ms even though encoding is flat
(1.110 → 1.111 ms) and p95 improves. This is not a general startup/tail fix.
The corresponding host averages are 0.712 → 0.707 and 0.333 → 0.329 ms.

### Range-coalescing verification

Each device passes 5,400 exact CPU mesh comparisons and 6,180 image comparisons
(3,276 zoom/outline, 1,944 production replay and 960 terrain-history checks).
All 38 saved scan/ranges PNG pairs per device are byte-identical. The existing
ordinary-renderer tolerances remain 0.05/255 mean RGB error and at most 0.1% of
pixels with summed RGB error above 30; worst suite errors remain
0.024818/0.057292% on the host and 0.024666/0.058594% on Pi. These small reference
differences predate coalescing. Counts, uploads and cache decisions match in all
108 CPU-profile blocks, 36 full-render blocks, 84 terrain blocks and 10,752
per-frame terrain rows per device. Accounted cache storage remains bounded by
128 entries / 16 MiB.

Tests explicitly cover coalescing around guarded strokes/fills, missing styles,
reversed and degenerate polygons, translucent colors, rotation, very small and
large scales, preexisting output, and both frame-buffer budgets. Existing
hybrid zoom/promotion/budget tests include the new mode. Results: 580 client
tests, 22 CLI tests and three backend-startup tests pass (three existing ignored
client tests). GPU Clippy completes with the existing 35 client warnings;
software-only checking, formatting and whitespace checks pass.

Artifacts: `target/gpu-experiments/outline-reuse.9aSaYBYr/`. Use the
`host-final-*` results for the host confirmation, `pi/` for device captures,
CSV, per-frame histories and live-session checks, and `shape-audit.log` for the
no-reuse finding. The initial `host-*` pilot overlaps this task's builds/tests;
it is retained for diagnosis, not the reported host confirmation. The scripts
include raw-counter/budget checks and byte-identical PNG comparisons.

Only `sw-picade-2` was updated. During offscreen tests, the kiosk was paused and
temporarily stopped, with a trap to resume it; the service watchdog was already
disabled. The ondemand governor was unchanged (sampled 1.3–1.5 GHz), temperature
samples were 65.7–70.1°C, and current power alarms remained clear. CPU-only and
full-render pairs rotate within the same run; no governor tuning is included.

Live checks exercise Clock, moving split-screen Spacewars, scan/ranges/ordinary
triangles/software scene switches while paused, resume, and return to launcher.
The paused scan/ranges PNGs are byte-identical, including after switching back
from ordinary triangles and software. Screenshots were inspected; the
known rounded-menu-border issue remains. GPU targets return to zero in software
and launcher modes. The new binaries remain installed, while the exact saved
preferences and ordinary software Clock are restored at 60 FPS / 60 UPS.
Boot ID is unchanged and the service reports zero unexpected restarts.

## GPU-resident ordered references

`host gpu-scene retained-resident` is an additional opt-in experiment, compared
against `retained-ranges`. It uses the same source identities, exact-scale
promotion, outline guards, and construction cache. It changes where retained
vertices live and where their placement is applied, not what gets drawn.

The CPU admits each cached mesh to a bounded atlas once. Two RGBA32F texels per
vertex hold local position, fixed-pixel outline offset, and premultiplied color.
Each frame streams a small placement record per group and an ordered vertex
reference/index stream. The vertex shader fetches those records and applies
scale/rotation/translation. Ordinary shapes and genuinely guarded outlines
still tessellate on the CPU and stream their screen-space vertices through a
second data texture. Reference and index order is identical to the previous
CPU-expanded mesh: no sorting transparent shapes, per-piece GPU draws, zoom
quantization, or new shape-specific fast paths.

This is **not** transform-only submission for the entire scene. References,
indices and dynamic/guarded geometry still upload every frame, and source
projection still runs on the CPU. Vertex-texture lookups introduce GPU work;
the experiment must beat the removed CPU expansion/upload cost end to end,
not merely lower the encoding timer.

### Residency and safety boundaries

- One atlas holds at most 128 admissions and 16 MiB of vertex payload. A CPU
  mirror reserves another 16 MiB for upload staging and validation. These are
  additional to the existing CPU construction cache's 16 MiB budget. Atlas
  payload counters are **not** a process-RSS or driver-residency measurement.
- New admissions upload merged dirty rows; ordinary warm hits do not upload
  atlas vertices. An admission ID cannot be confused with an evicted/replaced
  source. Changing modes or clearing the retained cache clears its atlas too.
- All geometry referenced in any pass of the current frame is pinned. Only
  older admissions can be evicted. Allocation pressure or unsafe extreme
  transforms use the CPU expansion path rather than dropping geometry.
- Each pass retains the existing one-million-vertex/three-million-index limits,
  plus a 4096-placement limit. Cached vertices are checked for finite values on
  admission; conservative transform bounds keep their intermediate results
  finite. Streamed vertices retain ordinary bulk validation.
- The renderer uses indexed drawing, preserving post-transform vertex reuse.
  It relies on ordered GL writes for older in-flight work, not unsynchronized
  mapped memory. Dynamic buffer/texture storage is replaced each pass; driver
  synchronization and allocation costs remain part of measured submission.
- Texture and sampler bindings on units zero/one, unpack state, buffers,
  programs, framebuffers, blending and viewport/scissor state are restored for
  Slint. The existing four-sample antialiasing and minimap composition remain.

`retained-resident-passes` additionally tests a bounded pool of eight dynamic
data texture objects, reset to the first slot each frame. Current split-screen
composition uses five slots. This avoids rewriting the same texture between
those camera passes; it does not claim unsynchronized reuse across frames.
More than eight passes safely wrap through ordinary ordered GL writes. Both
variants use identical shaders, references, geometry, draw counts and uploaded
bytes. The pool can retain more dynamic texture storage than the one-object
control; per-frame upload bytes do not measure that allocation.

The offscreen live replay accepts
`--geometry-live-modes retained-ranges,retained-resident,retained-resident-passes`;
both resident paths are
also included in adversarial outline and terrain edit/reveal validation.
`--geometry-encode-profile` now additionally reconstructs the resident vertex
references on the CPU and compares exact vertex bits and triangle order against
the prior encoder. That reconstruction is not part of live rendering or timed
GPU runs. CSV/CLI diagnostics distinguish cached versus streamed vertices,
placements, atlas payload, dirty-row uploads and residency fallbacks.

### Resident measurements

Final three-way runs use native 1024×768, six rotated repeats, 64 warm replay
frames after eight separately recorded startup frames, and 128-frame terrain
histories with no hidden warmup. Timing includes source generation, projection,
encoding, submission and `glFinish`, but not simulation, Slint UI or scanout.
These are uncapped completed-render times, not a 60 FPS pass/fail test.

Pi 4 / V3D 4.2 / Mesa 25.1.6 means, milliseconds per frame:

| Workload | CPU-expanded `ranges` | Resident, one data texture | Resident, per-pass textures |
| --- | ---: | ---: | ---: |
| Early bot replay | 12.894 | 23.121 | 15.591 |
| Later bot replay | 16.473 | 18.920 | 18.906 |
| Changing close-up zoom | 21.175 | 25.006 | 25.053 |
| Terrain reveal/zoom | 11.429 | 14.304 | 14.173 |
| Terrain edits/replacement | 8.011 | 9.436 | 9.282 |

The separate-pass control substantially reduces early-replay submission time:
8.324 → 2.503 ms. Shader code, uploaded bytes, geometry and draw counts are
identical, supporting a cross-pass resource-dependency cost in the one-texture
path. It does not explain the remaining regression. The single-pass terrain
controls and zoom replay remain slower with either resident variant.

For Pi close-up zoom, comparing `ranges` to per-pass resident:

- Encoding improves **6.784 → 6.024 ms**, but complete render time worsens
  **21.175 → 25.053 ms** (18.3%). Total thread CPU is 16.453 → 16.978 ms;
  CPU encoding savings are not equivalent to application CPU savings.
- Uploads fall **3,136,987.5 → 2,227,043.5 bytes/frame** (29.0%), including
  padding and new atlas rows. Both variants still draw 94,071.5 vertices and
  219,817.875 indices on average, in the same seven ordered GPU draws.
- About 76,935 vertices/frame are atlas-backed; 17,137 still stream, with
  63.3 placements/frame. The ending atlas contains 64 entries / 2,960,096
  payload bytes, with 1,536 dirty-row bytes/frame averaged over warm frames.
  No residency-pressure fallbacks occur in these matched replays.
- Mean first-use time worsens 48.856 → 76.972 ms. That interval includes extra
  resident resource/atlas initialization; it is not process startup. Mean
  block p95 worsens 22.073 → 26.721 ms, and mean block maximum 23.129 → 28.101 ms.

The AMD workstation instead shows small warm-replay gains with per-pass
residency: 0.726 → 0.705 ms early, 1.100 → 1.034 ms later, and 1.411 → 1.382 ms
during zoom. Terrain reveal is almost unchanged (0.641 → 0.636 ms), while
edits regress 0.270 → 0.278 ms. Compare modes **within** each device: matching
scenario seeds/ticks do not promise identical cross-platform physics outcomes.

Conclusion: keep both variants as explicit controls, **not a new default**.
The resident representation works, but this texture-fetching implementation is
slower on Pi even after removing much of one cross-pass stall. Reduced upload
volume alone is insufficient. GPU lookup/memory costs and residual submission
costs are the next suspects, not individually proven GPU-time measurements.
A useful follow-up would compare compact buffer-backed transform/vertex access
while retaining ordered batching; returning to hundreds of per-piece draw calls
would repeat the earlier rejected instancing experiment.

### Resident verification and artifacts

Both host and Pi pass:

- 6,480 exact CPU mesh comparisons, including resident-reference reconstruction,
  promotion, stable views and changing zoom; ordinary/cached ordering is unchanged.
- 7,868 image comparisons: 4,212 adversarial outline cases, 2,376 live replay
  comparisons across viewport replacements, and 1,280 terrain-history comparisons.
  The existing 0.05/255 mean-error and 0.1% significant-pixel limits are unchanged.
- Foreign texture/sampler state restoration across repeated resident, per-pass,
  ordinary and cached mode transitions.
- All 38 saved single-texture/per-pass PNG pairs are byte-identical per device
  (12 replay captures and 26 terrain-history captures).

The final unit run passes 587 client tests, 22 CLI tests and three startup tests;
three existing tests remain ignored. Added tests cover frame/placement bounds,
atlas pinning/eviction, invalid admissions, abandoned-submission dirty metadata,
extreme-transform fallback, repeated instances, guarded interleaving and source
replacement. GPU Clippy and the software-only check complete; Clippy retains
existing unrelated warnings. Formatting and diff checks pass.

Local artifacts are under `target/gpu-experiments/resident-pull.zS4qlovS/`:
`host-passes-{profile,native,churn}.{csv,log}`, matching capture directories,
`pi-final/`, `compare.mjs`, `compare-images.mjs`, source/binary checksums and
safe device runner scripts. Earlier single-texture and hardening runs are kept
separately; the table uses the final three-way binaries, not mixed revisions.

Live `sw-picade-2` checks also pass: Clock → launcher → moving split-screen
Spacewars, pause, `ranges → resident → resident-passes → triangles →
resident-passes → off → resident-passes`, screenshots, resume and launcher.
No renderer error is reported; presentation targets return from three to zero
when leaving the scene. The moving gameplay capture was visually inspected,
including ships, terrain, minimaps and Slint text/HUD. This is not a full-match
gameplay certification or a new live performance comparison.

Only `sw-picade-2` was changed. Final client/CLI SHA-256 prefixes are
`57a0a3d3eabd` / `75d0540a1341`. New binaries remain installed; exact saved
preferences were restored and the final device runs software Clock, unpaused,
at 60 FPS / 60 UPS. Boot ID is unchanged and the service has zero restarts.
Recorded phase checkpoints show 65.2–68.7°C, the existing `ondemand` governor,
1.5 GHz sampled CPU frequency and power alarm zero. These are checkpoints,
not a continuous clock/power trace. No governor, sysctl or perf-privilege
changes were made.

## Buffer backed resident geometry

Two additional opt-in controls keep the resident atlas, reference/index order,
guards and ordered draw count. Neither changes the software default.

`retained-resident-buffers` uses read-only shader storage buffers for both the
atlas and frame data. It requires GLES 3.1 or desktop GL 4.3, two vertex-stage
storage blocks, two bindings and enough block capacity for the existing frame
budget. The shader reads the same two `vec4` records used by the texture path.
Per-camera storage, row padding and uploaded bytes match
`retained-resident-passes`; this is not a persistent-mapping experiment.

The Pi 4 / V3D 4.2.14.0 / Mesa 25.1.6 reports **zero vertex-stage storage
blocks**, despite supporting GLES 3.1 and 16 combined storage blocks. It also
does not advertise either texture-buffer extension. An explicit offscreen
capability query confirmed those limits; a storage-buffer run is rejected,
not silently timed as another renderer. Stage-specific limits matter: the
[GLES specification](https://registry.khronos.org/OpenGL/specs/es/3.1/es_spec_3.1.pdf)
defines separate stage and combined limits. Workstation support does not imply
Pi support. The Pi instead reports 64 KiB uniform blocks and 15 vertex uniform
blocks.

`retained-resident-uniforms` therefore tests the narrower Pi-compatible change:
keep the geometry and streamed-vertex textures, but read the first 512 placement
transforms per pass from one 16 KiB uniform block. Higher placement indices use
the existing texture lookup, without dropping geometry, reducing the 4096-pose
limit, changing admissions or adding draws. Eight buffer objects follow the
same per-camera lifetime as the texture control. Only populated transform
records are uploaded. They remain duplicated in the ordinary data texture,
deliberately isolating lookup behavior before trying to remove redundant data.

Every resident mode now shares a compact pose table and the same packed reference:
20 bits address the interleaved frame record, while 12 identify the placement.
The existing million-vertex limit plus 4096 poses fits that address space. CPU
reference reconstruction checks both addresses against the same transform.
The added CPU table holds at most 128 KiB; uniform storage is at most eight
16 KiB objects, separate from atlas/cache budgets and driver allocations.
Generic and indexed buffer bindings, including offsets and sizes, are restored
for the surrounding UI renderer. Unsupported modes are identified in probe
metadata and excluded from validation counts; explicitly requesting one for
timing fails rather than producing a misleading fallback result.

### Buffer access measurements

Final runs use the same 1024×768 workloads, six rotated repeats, 64 warm replay
frames after eight separately recorded startup frames, and 128-frame terrain
histories. The table reports the mean of completed-render block averages,
including source/projection, encoding, submission and `glFinish`. Simulation,
Slint UI, scanout and the 60 Hz presentation cap are excluded. Pi elapsed GPU
timer queries remain unavailable; these are not isolated GPU execution times.

Pi 4 means, milliseconds per frame:

| Workload | CPU-expanded ranges | Per-pass textures | Uniform transforms |
| --- | ---: | ---: | ---: |
| Early bot replay | 12.930 | 15.939 | 15.867 |
| Later bot replay | 16.564 | 19.055 | 19.197 |
| Changing close-up zoom | 21.639 | 25.478 | 25.614 |
| Terrain reveal/zoom | 11.393 | 14.150 | 14.381 |
| Terrain edits/replacement | 8.032 | 9.278 | 9.502 |

Moving only the transform lookup to a uniform buffer does not recover the Pi
regression. Zoom still takes 18.4% longer than CPU expansion. Encoding improves
6.983 → 6.385 ms, but submission rises 2.533 → 3.140 ms and render-thread CPU
rises 16.850 → 17.455 ms. Relative to textures, the uniform variant is roughly
unchanged in replay averages and slightly worse in the terrain controls. This
does **not** isolate all shader/memory costs or prove that transform reads are
free; it shows that this particular storage substitution does not solve them.

Zoom geometry remains exactly 94,071.5 vertices and 219,817.875 indices on
average, with seven draws. Uniform transforms add only 2026 uploaded bytes/frame
to the texture control: 63.3125 placements × 32 bytes, giving 2,229,069.5 total
versus 2,227,043.5 with textures and 3,136,987.5 with CPU expansion. The atlas
still ends at 64 entries / 2,960,096 payload bytes, with 1536 dirty-row upload
bytes/frame and no residency-pressure fallbacks. Cold use remains worse:
49.208 ms for ranges versus 79.420 ms for uniforms. Mean block p95 is
22.806 → 27.483 ms; mean block maximum is 24.186 → 28.511 ms.

AMD RX 7800 XT / Mesa 26.0.8 workstation means, milliseconds per frame:

| Workload | CPU-expanded ranges | Per-pass textures | Full storage buffers | Uniform transforms |
| --- | ---: | ---: | ---: | ---: |
| Early bot replay | 0.747 | 0.716 | 0.667 | 0.723 |
| Later bot replay | 1.107 | 1.035 | 0.978 | 1.052 |
| Changing close-up zoom | 1.434 | 1.380 | 1.324 | 1.397 |
| Terrain reveal/zoom | 0.636 | 0.624 | 0.614 | 0.624 |
| Terrain edits/replacement | 0.263 | 0.266 | 0.255 | 0.269 |

Full storage buffers improve the workstation's warm replays about 8–12% over
CPU expansion, and 4–7% over the texture-resident control. Geometry, cache
decisions and uploaded bytes match that control. Uniform transforms alone do
not improve the texture path there either. Builds/tests completed before the
final timing run; the workstation was not an exclusive benchmark machine, so
small changes should not be treated as precise universal gains. Compare modes
within each device, not physics trajectories across architectures.

Keep all controls opt-in and retain CPU-expanded `retained-ranges` as the Pi
comparison path. This establishes a useful hardware-dependent result, not a
reason to select the SSBO path without checking support or to select uniform
transforms automatically. On Pi, subsequent work should address remaining
geometry work or test a cheaper draw representation; another storage-format
substitution alone is not supported by these measurements.

### Buffer access verification and artifacts

The final client, CLI and startup tests pass (588, 22 and three respectively;
three pre-existing tests remain ignored). GPU Clippy retains the same 26 client
warnings plus one vendor warning; software-only compilation, formatting and
diff checks pass.

Both devices pass 6480 exact CPU mesh comparisons with the packed references.
The host passes 9562 image comparisons: 5148 outline cases, 2808 replay frames,
1600 terrain-history frames and six exact uniform-table boundary/overflow/shrink
images. Pi passes 8718 comparisons (4680 + 2592 + 1440 + six), excluding the
unsupported SSBO mode. The 0.05/255 mean-error and 0.1% significant-pixel gates
are unchanged. All 38 saved texture/uniform PNG pairs are byte-identical on the
host and on Pi; another 38 host texture/SSBO pairs also match exactly. Foreign texture/sampler,
uniform and storage-buffer bindings are checked across mode changes and teardown.

Artifacts live under `target/gpu-experiments/resident-buffers.S0edvugn/`.
Use `host-final-{profile,native,churn}.{csv,log}`, `host-final-*-captures/`,
`host-final-summary.json` and `pi-final-summary.json` for this revision.
Earlier `host-*` and `pi-unsupported/` artifacts record the initial SSBO-only
attempt, not the final packed-reference/UBO implementation. The standalone
`capabilities.rs` probe and `pi-capabilities.txt` document the device limit.
The debug smoke run hit its 150-second wrapper timeout after completing outline
and uniform-table checks; the final release validations completed in full.
Source/binary checksums, safe device runners and comparison scripts are retained
alongside the measurements.

`pi-final/` contains the final Pi logs, captures and live transition checks.
Clock → launcher → moving split-screen Spacewars, pause, repeated renderer
switching, resume and launcher all pass. An intentional request for the
unsupported SSBO mode reports the actual limit and returns to raster; switching
back to the uniform mode recovers cleanly. Presentation targets return from
three to zero when leaving gameplay. Live screenshots were visually inspected;
this is not full-match certification or a live FPS performance comparison.

Only `sw-picade-2` was deployed. Installed client/CLI SHA-256 prefixes are
`f57e3ad34e68` / `af06de71d1bc`. Those binaries remain installed, exact saved
preferences match their pre-run checksum, and the device is back on software
Clock at 60 FPS / 60 UPS, unpaused. Boot ID is unchanged and service restart
count is zero. Recorded checkpoints show 65.7–68.7°C, 1.5 GHz during the probe
phases, the existing `ondemand` governor and power alarm zero; frequency after
resuming Clock was 1.1 GHz. These are sampled checkpoints, not a continuous
power/clock trace. No governor, sysctl or perf-privilege changes were made.

## Cached triangle visibility

The next geometry audit asks how much prepared work is outside a player's
viewport. The scenario already rejects invisible terrain chunks, but a visible
chunk can contain many invisible polygons. The ignored `audit_viewport_work`
test tessellates production draws and checks their actual output against the
camera clip plus a one-pixel margin. In the initial close-up sample, the bot
view has 27,686 of 63,616 stroke vertices wholly outside its pane, plus 8,010 of
18,344 fill vertices. This is an opportunity estimate, not a source-outline
rule that is safe to use before tessellation.

Optional `retained-culled` builds on `retained-ranges`. It uses bounds of the
**actual cached vertices**, including fixed-pixel stroke offsets. It does not
change source artwork, simplify polygons, widen reuse domains or approximate
zoom. Guarded pieces and ordinary uncached draws still tessellate unchanged.

- Scalable meshes store separate position/offset bounds for each fill and
  stroke piece. Positive scale and signed interval transforms follow the CPU
  expansion's arithmetic order. Non-finite/empty bounds keep the piece; an
  extra pixel protects clip-edge rasterization and MSAA coverage.
- Exact-scale meshes use ordered blocks of 384 indices, always on triangle
  boundaries. Adjacent surviving blocks coalesce without reordering. If
  separated vertex intervals would duplicate more vertices than the original
  mesh, or exceed the frame budget, the whole-entry expansion remains available.
- Direct passes use their actual camera scissor, not the full shared target.
  Minimap passes use their own rectangular target bounds; circular composition
  and group opacity are unchanged. Source chunks stay cached when invisible,
  so later camera movement can reveal them immediately.
- Visibility metadata counts against the same 128-entry / 16 MiB payload
  budget. The transient span vector is reused and bounded by the number of
  index blocks; the earlier budget excludes allocator/container overhead and
  tessellator scratch, as before. `culled_pieces` and `culled_indices` report
  work skipped, not a new GPU draw count. A piece is a fill/stroke range for
  scalable meshes or an index block for exact meshes.

The live IPC and CLI expose `host gpu-scene retained-culled`; the existing
`retained-ranges` control is unchanged. Software is still the default. The
warm encoding probe now rotates eight plain/profiled variants and uses the
actual pass clips. Detailed stage timers do not separately cover visibility
selection, so compare the complete **unprofiled** CPU totals rather than
subtracting individual stage timers to estimate its cost.

### Visibility measurement method

Use native 1024×768 output, six rotated repeats of the 64-frame warm live
windows, and six 128-frame terrain-history runs. The live probe separately
records first use and eight startup frames. Total render time includes source,
projection, encoding, submission and `glFinish`; simulation, text/UI and
scanout remain excluded. It is neither live FPS nor isolated GPU-active time.
CPU-only encoding uses eight rotated repeats of 64 frames per mode; compare
those separately from full-render intervals and their GPU-memory traffic.

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-live-retained --geometry-live-modes retained-ranges,retained-culled \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 64 --presentation-repeats 6 \
  --geometry-output /tmp/spacewars-visibility-replay
```

Workstation means on AMD RX 7800 XT / Mesa 26.0.8, completed milliseconds:

| Workload | Ranges | Culled | Change |
| --- | ---: | ---: | ---: |
| Early bot replay | 0.735 | 0.705 | −4.1% |
| Later bot replay | 1.115 | 0.956 | −14.3% |
| Changing close-up zoom | 1.471 | 1.361 | −7.5% |
| Terrain reveal/zoom | 0.641 | 0.657 | +2.4% |
| Terrain edits/replacement | 0.264 | 0.275 | +4.1% |

The CPU-only early replay is slower on this workstation (0.181 → 0.193 ms),
despite the smaller full-render improvement. The terrain controls show the
cost of visibility metadata/testing when almost nothing can be removed:
reveal uploads change only 1,052,466 → 1,051,536 bytes/frame, and edit uploads
stay at 555,858. Culling is not a universal speedup. Local builds/tests finished
before timing; unrelated sibling-checkout build work was present, so the
workstation was not an exclusive benchmark host. Compare within-device paired
runs, not absolute render times or physics trajectories across architectures.

Pi 4 / V3D 4.2.14.0 / Mesa 25.1.6 means, completed milliseconds:

| Workload | Ranges | Culled | Change |
| --- | ---: | ---: | ---: |
| Early bot replay | 13.451 | 13.386 | −0.5%, effectively unchanged |
| Later bot replay | 16.682 | 14.948 | −10.4% |
| Changing close-up zoom | 21.977 | 21.281 | −3.2% |
| Terrain reveal/zoom | 11.632 | 11.853 | +1.9% |
| Terrain edits/replacement | 7.991 | 7.997 | +0.1%, effectively unchanged |

The Pi CPU-only means improve 1.912 → 1.610 ms, 2.713 → 1.693 ms and
5.898 → 5.213 ms for the three replays. Full-render zoom encoding also improves,
7.122 → 6.464 ms, but submission grows 2.676 → 3.196 ms. Render-thread CPU
changes only 17.102 → 16.652 ms. Less CPU mesh work therefore does **not** all
translate into completed-frame savings. This experiment does not isolate the
cause of the submission increase; buffer upload/allocation/synchronization
remain hypotheses to measure, not established explanations.

Zoom keeps seven draws and exactly the same 948.53 guarded outlines per frame.
Vertices fall 94,071.5 → 61,006.84, indices 219,817.875 → 143,708.344 and uploads
3,136,987.5 → 2,038,997.625 bytes/frame (35.0% less). Cache payload rises by
288,960 bytes, from 6,376,756 to 6,665,716, still below the existing limit.
Admissions, hits, promotions and guarded-fallback counts match within each
device's paired replays and terrain runs. Removed-index counters exactly
reconcile the replay output reductions.

Latency is mixed: Pi zoom mean block p95 worsens 22.984 → 24.589 ms and cold
first-use mean worsens 50.300 → 55.518 ms. Early-replay mean block maximum also
worsens 18.670 → 36.222 ms, despite essentially unchanged averages. The later
replay improves both average and measured tails. Do not describe culling as a
general smoothness improvement or enable it automatically from these means.
Keep both controls. The next useful investigation is the Pi submission cost
that offsets encoding savings; a broader bounds check could separately reduce
visibility overhead for already-visible geometry.

### Visibility verification

The client, CLI and startup suites pass (591, 22 and three tests). Four
diagnostics are ignored in the normal run, including the new explicit
visibility audit. New unit checks cover clip margins, fixed-pixel offsets,
rotations, invalid/empty bounds, cold/template/exact promotion, viewport
changes, reveal, translucent ordering and cache-budget fallback. Across a
60-frame synthetic history, retained visible triangles remain bit-identical
and ordered; every omitted triangle is wholly outside its clip. GPU Clippy
retains the existing 26 client warnings, along with existing dependency/vendor
warnings. Software-only compilation, formatting and diff checks pass.

Both devices pass 6,480 existing CPU mesh/reference reconstruction comparisons.
Those check the older encoding controls; culled meshes intentionally contain
fewer triangles and are checked separately. Host image validation passes
10,406 cases (5,616 outlines, 3,024 replay images, 1,760 terrain images and six
uniform-table images); Pi passes 9,562 (5,148 + 2,808 + 1,600 + six), explicitly
excluding unsupported vertex SSBOs. In addition to the unchanged ordinary
reference error limits, the new path must match `retained-ranges` readback
**byte for byte** in all 216 live replay and 160 terrain-history states per
device. Foreign GL binding checks include switches through the new mode.

Artifacts are under `target/gpu-experiments/visibility.wZXown4Z/`: the explicit
audit, host and Pi CSVs, paired comparison scripts, build/test logs, source and
binary checksums, safe device runners and captures. `host-smoke` is an earlier
validation-only run during build activity, not the timing table's source;
use `host-{profile,native,churn}` and the corresponding Pi files for timings.

All 38 saved ranges/culled PNG pairs are byte-identical on each device. The
`pi/` directory also contains live Clock → launcher → moving split-screen
Spacewars checks, pause, `ranges → culled → triangles → culled → off → culled`,
resume and launcher. Targets return from three to zero on software fallback
and launcher, with no new renderer errors. The historical log excerpt includes
the previous experiment's deliberately unsupported SSBO request; it is not a
new culling failure. Live screenshots were inspected, including terrain, ships,
minimaps and text/UI. These checks are not full-match certification or a live
performance comparison; the existing rounded-menu-border issue remains.

Only `sw-picade-2` was updated. Installed client/CLI SHA-256 prefixes are
`11bd3c5199c4` / `29a8461984de`. The new binaries remain installed; saved
preferences match exactly and the device is back on software Clock, unpaused,
at 60 FPS / 60 UPS. Boot ID is unchanged and there are no automatic service
restarts. Phase checkpoints record 67.2–71.6°C, the existing `ondemand`
governor and power alarm zero. Sampled frequency is 1.5 GHz during CPU/replay
phases and 1.3 GHz at the final churn/resume checkpoint. These are samples,
not a continuous frequency/power trace. No governor or privilege changes were
made.

## Submission timing and per pass streaming

The submission lab separates CPU wall time around GL calls from completed-frame
time. It compares the existing `retained-ranges` and `retained-culled` geometry
with and without detailed clocks. Add `--geometry-submit-passes` to also test
separate vertex/index-buffer objects for each nonempty pass. This is an
offscreen-only experiment, not a new live mode or a default change.

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-submit-profile --geometry-submit-passes \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 64 --presentation-repeats 8 \
  --geometry-output /tmp/submission-captures
```

The output directory must not already exist. Omitting the per-pass flag runs
only the shared-buffer controls. Every run validates before timing and reports
the first frame and eight startup frames separately
from the warm window. Each block creates a new renderer and scenario; a
disposable ordinary frame prewarms targets and stream objects before clearing
the geometry cache. This differs from the older live probe's renderer reuse
between blocks, so compare paired controls within this lab, not absolute
changes against earlier runs.

The measured stages are GL-state guard entry/exit, initial clear, per-pass
setup, vertex upload, index upload, batch submission, the pass's error check,
minimap resolve, minimap composition and final resolve. Guard time lies
outside the existing broad `submit_ms` interval. `finish_ms` is the final
completion wait, also outside submission. There are no extra intermediate
waits; any driver work or waiting inside an API is charged to that API's wall
time. These are **not GPU-active measurements**. Normal drawing and the plain
controls compile out the detailed clocks. Use plain controls for performance
decisions and profiled controls for attribution.

The historical results below use cyclic block order. The current lab uses
counterbalanced order as described in the capacity experiment below; use eight
repeats for a full eight-mode order cycle (four for the shared-only controls).

The buffer experiment changes only destination object identity. It keeps the
same `buffer_data` uploads with `STREAM_DRAW`, 24-byte vertices, 32-bit indices,
shader, MSAA, clips, geometry and draw order. The pool holds at most eight
VAO/VBO/EBO sets and 16 MiB of additional requested buffer payload. Oversized
or excess passes use the original shared pair. Empty passes need no pair;
unsupported geometry modes retain their original path. Disabling the control
or retiring the compositor releases the pool. Reported stream bytes describe
the requested stores still attached to objects at frame end, **not** peak
driver/VRAM consumption or a measurement of driver-side buffer renaming.

Validation compares RGBA bytes and geometry/upload/draw counts against the
same-policy shared-buffer reference. It exercises camera motion, zoom,
resize, scenario replacement, a doubled pass list exceeding the pool limit,
empty frames, toggling the control and teardown. A deliberately foreign VAO
and its element-buffer binding, plus an independent array-buffer binding,
must survive every transition. Slot/payload bounds and disabled clocks also
have unit coverage.

### Submission measurements

Both final runs use native 1024×768 output, 64 warm frames, six rotated repeats
and one completed frame in flight. A preliminary Pi run compares only the
shared-buffer controls; the second includes all eight plain/profiled and
shared/per-pass combinations. The workstation is AMD RX 7800 XT / Mesa 26.0.8;
the device is Pi 4 V3D 4.2.14.0 / Mesa 25.1.6. Own builds/tests finish before
the final timings. The earlier one-frame host smoke run is validation only.

The Pi zoom penalty is localized to the **vertex-upload API call**, not extra
draws or expensive error checks. In the preliminary profiled shared-buffer
controls, ranges → culled changes vertex upload 1.203 → 1.876 ms, while index
upload stays 0.465 → 0.467 ms, batches 0.289 → 0.285 ms and error checks
0.0042 → 0.0040 ms. The second run reproduces it: 1.203 → 1.934 ms for vertex
upload. This locates where the render thread spends time, but does not yet
distinguish allocation, copying or driver waits inside that call.

Separate per-pass objects do not establish a performance improvement. Pi
**uninstrumented completed-frame means**, shared → per-pass, in milliseconds:

| Replay and geometry | Shared pair | Per-pass pairs | Change |
| --- | ---: | ---: | ---: |
| Early ranges | 12.539 | 12.631 | +0.7% |
| Early culled | 12.766 | 12.803 | +0.3% |
| Later ranges | 16.258 | 16.248 | −0.1% |
| Later culled | 14.653 | 14.776 | +0.8% |
| Changing zoom ranges | 20.839 | 20.863 | +0.1% |
| Changing zoom culled | 20.453 | 20.380 | −0.4% |

The workstation also changes by less than 0.7% in every paired mean. Pi culled
zoom submission barely changes, 3.245 → 3.219 ms; its mean block p95 changes
23.788 → 23.698 ms. The profiled per-pass vertex upload still costs 1.889 ms,
versus 1.934 ms shared. The seven draws, vertices, indices, uploaded bytes,
admissions, promotions and cache payload match within each geometry policy.
At most five pool entries are used in the timed replays. Culled zoom retains
up to 2,080,488 requested stream bytes at frame end versus 2,160 for the shared
object's last pass. This does not measure transient driver allocations or
its own buffer cache. Keep the new control offscreen-only and preserve defaults.

Instrumentation is not neutral in every workload: early Pi culled means are
12.766 ms plain versus 12.386 ms profiled, and per-pass is 12.803 versus
12.206 ms. Do not interpret this as negative timer overhead or report the
profiled numbers as an optimization. Starting order rotates, but predecessor
order is not fully counterbalanced and process-wide driver caches are not
reset. This motivated the counterbalanced capacity lab below; small gains
need confirmation with that control.

The follow-up tests **bounded, stable or size-bucketed allocation capacity**,
while uploading only the used bytes. Object separation alone still
uses exact-size `buffer_data` every pass. Mesa's version-matched V3D code aligns
buffer allocations to 4096 bytes and looks for an idle cached buffer in that
size bucket before allocating a new one. That makes changing allocation sizes
a plausible lead, **not a proven cause** of these stalls. See the
[Mesa 25.1.6 buffer manager](https://gitlab.freedesktop.org/mesa/mesa/-/blob/mesa-25.1.6/src/gallium/drivers/v3d/v3d_bufmgr.c).
Do not skip error checks, weaken geometry guards or introduce persistent
mapping/fence machinery on the strength of this experiment.

### Submission verification

The client, CLI and startup suites pass: 595, 22 and three tests, with the same
four ignored diagnostics. GPU Clippy retains the existing 26 client warnings;
software-only compilation, formatting and diff checks pass. Each device's
full comparison validates 1,728 exact RGBA/count comparisons over 216 scene
states, including nine histories exceeding the eight-slot limit. The preliminary
Pi run adds 864 shared-path checks. These are same-policy equivalence tests,
not independent raster-reference or all-renderer certification.

All six saved shared/per-pass PNG pairs match byte for byte on each device.
Pi close-up and later-match images were inspected: ships, terrain, clipped
camera panes and minimaps are present. Text/UI is deliberately absent from
these offscreen captures. This is not a new live full-match/UI validation.

Artifacts are in `target/gpu-experiments/submission.AaH3LwNY/`: `host.csv`,
`pi/baseline.csv`, `pi/passes.csv`, summaries, per-frame traces and capture
directories, source/binary checksums, build/test logs and device wrappers.
Partial CSVs and `smoke` outputs are not the final comparison. The analyzer
checks geometry/admission invariants independently of timings.

Only `sw-picade-2` was updated; client/CLI SHA-256 prefixes are `bb6e6a1813f7` /
`29a8461984de`. It is back on software Clock, unpaused at 60 FPS / 60 UPS with
no renderer error. Exact saved preferences and boot ID match, and the service
has no automatic restarts. The new binaries remain installed. Phase checkpoints
record 67.2–69.6°C, the existing `ondemand` governor, 1.5 GHz during replay and
power alarm zero; additional in-run samples reached 70.1°C. Resume checkpoints
read 1.2–1.3 GHz. These are samples, not continuous thermal/power/frequency
traces. No governor, privilege or other-device changes were made.

## Stable streaming capacity experiment

`--geometry-submit-capacity` compares per-pass exact-size stores against
per-pass bucketed stores, each with ranges/culled geometry and plain/profiled
submission. It requires `--geometry-submit-profile` and is mutually exclusive
with `--geometry-submit-passes`. Shared-buffer controls remain available via
the previous flags. No live rendering mode or default changes.

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-submit-profile --geometry-submit-capacity \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 64 --presentation-repeats 8 \
  --geometry-output /tmp/capacity-captures
```

The exact control calls `buffer_data(slice)` as before. The new policy rounds
each vertex/index capacity up to a power of two, with a 4 KiB minimum, and grows
without shrinking. Every nonempty pass still discards its previous store using
`buffer_data(NULL, capacity)`, then uploads only used bytes through `subdata`.
It does not overwrite in-flight data without discarding, use persistent mapping,
or add a fence/ring-buffer mechanism. Both strategies use the same geometry,
shader, vertex format, draw order and actual upload bytes. This experiment
changes both storage sizes and the upload API sequence; a win alone would not
identify which change helped.

The pool limit remains eight VAO/VBO/EBO sets and 16 MiB, now explicitly charged
for reserved capacities rather than used bytes. Overflow uses the existing
shared exact-size pair. Policy changes release the pool; the shared pair keeps
its ordinary behavior. Counters distinguish `buffer_data` calls, `subdata`
calls, requested storage bytes, changes in storage size and fallback passes.
`upload_bytes` still counts only geometry actually transmitted. End-of-frame
capacity is not peak driver memory: discarded stores can remain in flight or
in a driver cache.

Detailed timings split bucketed vertex/index work into reserve and upload
stages. Add both stages when comparing against an exact control, whose upload
call includes storage and data together. Plain controls compile out all these
stage clocks; final completion remains a single `glFinish` per frame.

All submission probes now use an even-sized Williams balanced Latin square.
Over eight repeats, each of eight modes occupies every position once and each
unequal ordered pair appears once as adjacent blocks **within a row**. This does
not control cross-row boundaries or longer-lived driver state. CSVs record
position and preceding mode; manifests include the complete order and flag
incomplete cycles. Partial cycles are useful for smoke checks, not fully
counterbalanced comparisons. The disposable prewarm and eight startup frames
are unchanged; simulation, text/UI and scanout remain outside timings.

Pure tests cover growth, non-shrink, overflow arithmetic, capacity accounting,
upload counters, position/predecessor balance and CLI constraints. Device
validation compares exact RGBA and geometry/upload/draw counts against the
shared path, including changing zoom, resize, replacement, more than eight
passes, empty frames, policy toggles, teardown and foreign GL bindings. These
are same-policy equivalence tests, not independent renderer certification.

### Capacity measurements

The workstation comparison uses 1024×768, 64 warm frames after eight startup
frames, eight counterbalanced repeats and one completed frame in flight. All
own builds/tests finish before timing. The AMD RX 7800 XT / Mesa 26.0.8 remains
effectively tied: every plain completed-frame mean changes by less than 0.8%.
Culled zoom changes 1.342 → 1.332 ms. Its upload/submission work gets cheaper,
but submission's 0.136 → 0.106 ms is mostly offset by completion wait's
0.236 → 0.264 ms. A faster GL call does not imply an equally faster frame.

The mechanism does reduce storage-size changes: in the host culled zoom case,
3.703 size changes per warm frame become zero, while actual data stays
2,038,979 bytes/frame. Logical end-of-frame stores grow from at most
2,080,596 to 3,571,712 bytes. Culled later-game capacity peaks at 5,816,320 bytes
versus 2,008,236 exact. This includes capacities established by the disposable
ordinary prewarm, not just the smaller retained/culled mesh. The resource cost
must be weighed against completed-frame behavior on the target device.

The Pi 4 V3D / Mesa 25.1.6 comparison uses the same protocol and native output.
Plain completed-frame means, in milliseconds:

| Replay and geometry | Per-pass exact | Per-pass bucketed | Change |
| --- | ---: | ---: | ---: |
| Early ranges | 12.860 | 12.939 | +0.6% |
| Early culled | 12.869 | 12.409 | −3.6% |
| Later ranges | 16.356 | 16.206 | −0.9% |
| Later culled | 14.895 | 14.261 | −4.3% |
| Changing zoom ranges | 21.075 | 20.967 | −0.5% |
| Changing zoom culled | 20.704 | 19.661 | −5.0% |

Culled zoom's submission drops 3.297 → 2.180 ms, with completion wait nearly
unchanged at 4.538 → 4.564 ms. Its profiled vertex calls drop from 1.849 ms
to **0.268 ms reserve + 0.635 ms upload = 0.903 ms**; index calls change
0.470 → 0.059 + 0.257 = 0.316 ms. The gain is not extra geometry culling:
vertices, indices, uploaded bytes, seven draws, admissions, promotions and
geometry-cache payload match the exact control within each geometry policy.
Timed passes never use the fallback. Culled zoom has zero warm storage-size
changes instead of 3.734/frame. Its maximum logical store size grows
2,080,488 → 3,571,712 bytes; culled later-game grows 1,287,024 → 3,850,240 bytes.

Pooled warm p95 (512 frames per mode/fixture, excluding startup) improves for
all culled replays: early 14.684 → 13.433 ms, later 17.066 → 15.358 ms and
zoom 24.319 → 21.121 ms. Individual worst frames are not universally better:
early culled maximum grows 20.923 → 21.818 ms. Zoom's block-mean ranges are
20.601–20.802 ms exact versus 19.568–19.861 ms bucketed across eight repeats.
Cold-cache first-frame means remain expensive, about 49.47 → 46.71 ms for
culled zoom; eight-frame startup totals change 194.14 → 183.98 ms. This does
not eliminate guarded geometry work or first-use spikes.

The counterbalanced early culled plain/profiled means are now much closer:
12.869/12.917 ms exact and 12.409/12.435 ms bucketed. This removes the large
apparent negative profiling overhead from the earlier cyclic run in this
sample, not a guarantee that clocks or cache history are neutral. The analyzer
independently verifies all 64 mode/position combinations and 56 within-row
ordered predecessor pairs per fixture, as well as geometry/admission invariants
and non-shrinking bucket capacities throughout each block.

**Decision:** retain the strategy as an offscreen experimental control. It is
a useful Pi improvement for culled geometry, not a blanket buffer upgrade:
ranges means change less than 1%, and early ranges tails are worse. The
[exact-size split-upload follow-up](#separating-the-upload-api-from-capacity)
below separates the API change from the capacity policy. Live frame overlap
and terrain edit/reveal histories remain to be checked. These results alone
do not prove allocation-size churn was the root cause.

### Capacity verification and artifacts

The final client, CLI and startup suites pass: 598, 22 and three tests, with
four ignored diagnostics. GPU Clippy retains the existing 26 client warnings;
software-only compilation, formatting and diff checks pass. Host and Pi final
comparisons each pass 1,728 exact RGBA/count checks, nine overflow histories,
foreign-binding preservation and all six saved PNG-pair comparisons. Host
shared/per-pass smoke runs also pass 864/1,728 checks. Smoke timings are not
performance evidence. Pi zoom and later-game captures were visually inspected;
text/UI and actual scanout are outside this lab, not newly certified here.

Artifacts: `target/gpu-experiments/capacity.ZjRZ5pw0/`, including `host.csv`,
`pi/capacity.csv`, complete per-frame traces, captures, manifests, summaries,
pooled-tail reports, checksums and reproducible analysis/device wrappers.
`pi-partial*.csv` and smoke outputs are not the final comparison. Source and
binary hashes were checked after timing. The first cross-build wrapper exited
early while its compiler continued; the final recorded build completed with
exit zero before deployment.

Only `sw-picade-2` was updated. Client/CLI SHA-256 prefixes are `0a3629fec56d` /
`29a8461984de`; the new pair remains installed. The device is back on software
Clock, unpaused at 60 FPS / 60 UPS, with no renderer error or automatic service
restart. Saved preferences and boot ID are unchanged. Checkpoints span
64.8–70.1°C, retain `ondemand`, and sample 1.2–1.5 GHz with power alarm zero.
These are sampled health readings, not continuous traces. No other device,
governor or privilege setting changed.

## Separating the upload API from capacity

`--geometry-submit-api` adds the missing exact-size split-upload control. It
compares three per-pass strategies with both ranges and culled geometry:

| Strategy | Storage request | Data upload |
| --- | --- | --- |
| Exact combined (`passes`) | Used size | Same `buffer_data(slice)` call |
| Exact split (`split`) | `buffer_data(NULL, used)` | `buffer_sub_data(used)` |
| Bucketed split (`bucketed`) | `buffer_data(NULL, capacity)` | `buffer_sub_data(used)` |

Exact split preserves exact combined's sizes, including growth and shrink;
only the API sequence changes. All paths still discard stores rather than
overwriting possibly in-flight data. Geometry, shaders, draw order, pool limits
and shared-buffer fallback remain unchanged. The control is offscreen-only;
there is no new live mode or default change.

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-submit-profile --geometry-submit-api --geometry-submit-plain-only \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 64 --presentation-repeats 6 \
  --geometry-output /tmp/upload-api-captures
```

`--geometry-submit-api` requires the submission probe and conflicts with its
passes/capacity family selectors. `--geometry-submit-plain-only` times only
uninstrumented modes, but **does not reduce validation**: all twelve
plain/profiled variants are checked before timing. Without plain-only, twelve
repeats complete the twelve-mode order cycle. The older selectors retain their
original mode families. Output metadata now records timed and validated modes
separately.

These comparisons use 1024×768, six counterbalanced repeats, 64 warm frames
after eight startup frames and one completed frame in flight. All twelve stage
clocks are zero in the timed rows. Each fixture has 36 mode/position
combinations and 30 within-row ordered predecessor pairs, each occurring once.
Cross-row boundaries and longer driver history are still uncontrolled. Timing
includes source extraction through final `glFinish`, not simulation, text/UI or
scanout. First-use results describe an empty geometry cache after disposable
ordinary-geometry prewarm, not process startup. Own builds/tests finish before
the workstation comparison.

The AMD RX 7800 XT / Mesa 26.0.8 workstation remains effectively tied: every
plain completed-frame mean changes by less than 0.9% from exact combined.
Culled zoom is 1.348 / 1.350 / 1.345 ms for exact combined / exact split /
bucketed split. Submission changes 0.139 / 0.135 / 0.107 ms while the final
wait changes 0.234 / 0.242 / 0.270 ms. The shorter bucketed submission call
does not produce a corresponding completed-frame gain here.

### Pi upload control measurements

On Pi 4 V3D / Mesa 25.1.6, splitting the calls at the **same exact size** does
not reproduce the capacity improvement. All six exact-split means change less
than 0.9% from exact combined. Bucketed capacity retains a 3.8–4.4% improvement
for the culled replays. Plain completed-frame means in milliseconds:

| Replay and geometry | Exact combined | Exact split | Bucketed split |
| --- | ---: | ---: | ---: |
| Early ranges | 12.869 | 12.943 | 12.748 |
| Early culled | 12.837 | 12.723 | 12.346 |
| Later ranges | 16.372 | 16.342 | 16.230 |
| Later culled | 14.793 | 14.773 | 14.163 |
| Changing zoom ranges | 20.920 | 20.896 | 20.955 |
| Changing zoom culled | 20.579 | 20.620 | 19.671 |

Culled zoom's broad submission time is 3.284 / 3.288 / 2.202 ms, while final
completion waits stay close at 4.541 / 4.548 / 4.575 ms. Thread CPU time is
16.021 / 16.049 / 15.050 ms. The benefit is not a deferred wait merely moving
out of the upload call, nor is it different geometry: all three paths upload
the same 2,038,998 bytes/frame and issue seven draws. These timings still do
not measure GPU-active time or driver allocation events.

The analyzer verifies exact combined and exact split request the same storage,
make the same size changes and retain the same logical capacity on **every
frame**, including startup. For culled zoom both have 3.734 warm size changes
per frame and peak at 2,080,488 bytes of logical stores. Bucketed split has
zero warm size changes and peaks at 3,571,712 bytes. Culled later-game grows
from 1,287,024 to 3,850,240 bytes. That extra reservation includes capacities
established by the disposable ordinary-geometry prewarm; it is not a driver
memory measurement. Each timed frame uses five stream slots, no fallback and
ten `buffer_data` calls; both split variants add ten `subdata` calls.

Pooled warm p95 over 384 frames per mode/fixture, exact combined / exact split /
bucketed split, is 14.877 / 14.745 / 13.187 ms early, 17.167 / 17.114 / 14.864 ms
later, and 24.098 / 24.263 / 21.007 ms during culled zoom. Zoom block means span
20.441–20.721 / 20.496–20.745 / 19.549–19.775 ms across the six repeats.
Startup is not uniformly better: culled zoom's first-frame means are
51.012 / 54.358 / 53.616 ms, even though its eight-frame startup total improves
194.684 / 198.581 / 189.288 ms. This is not a general first-use latency fix.
Non-culling Pi completed means still change less than 1%.

**Decision:** the larger grow-only capacity policy matters on these culled Pi
replays; splitting reserve/upload calls alone is insufficient. This does not
separate stable sizes from larger stores, driver buffer-cache behavior or
other internal allocation choices. Keep both exact controls for future
comparisons. The [queued-history follow-up](#queued-uploads-across-moving-and-edited-scenes)
below tests overlap and terrain edits/reveals before considering a live default.
Preserve the original renderer paths and the bounded shared-buffer fallback.

### Upload control verification

The client, CLI and startup suites pass: 599, 22 and three tests, with the same
four ignored diagnostics. GPU Clippy retains the existing 26 client warnings;
software-only compilation, formatting and diff checks pass. Unit tests verify
exact-size growth/shrink and the new call counters; CLI tests cover required
and conflicting flags. Balanced-order tests now include the six-mode plain-only
and twelve-mode combined families.

Both devices pass 2,592 exact RGBA/count comparisons over 216 scene states,
including nine overflow histories, resize, empty frames, policy changes,
teardown and foreign VAO/VBO/EBO preservation. Profiled exact-split rendering is
validated even though it is not timed in this comparison. These are equivalence
checks against the existing shared-buffer geometry path, not independent
raster-reference or complete live-session certification.

Independent analysis of both final runs verifies 108 timing blocks and 7,776
frame records per device, exact geometry/admission/cache invariants, identical
exact combined/split storage metrics, all-zero stage clocks and monotonic
bucket capacities. All twelve saved PNG pairs per device are byte-identical.
The Pi close-up capture was inspected, including both camera panes, terrain,
ships and minimaps; text/UI is intentionally outside these images.

Artifacts are in `target/gpu-experiments/upload-api.a6NEgArz/`: `host.csv`,
`pi/upload-api.csv`, capture manifests and per-frame traces, analysis scripts
and summaries, source/binary hashes, build/test logs and the safe device runner.
`pi-partial.csv` is progress-only, not the final comparison. The recorded host
and cross-builds completed successfully before deployment/timing. The existing
cache-maintenance policy reclaimed 31.5 GiB of rebuildable workspace debug
artifacts before building; release binaries and experiment evidence were kept.

Only `sw-picade-2` was updated. Installed client/CLI SHA-256 prefixes are
`647207ea549b` / `29a8461984de`; the new binaries remain installed. The device is
back on software Clock, unpaused at 60 FPS / 60 UPS, with no renderer error or
automatic service restart. Saved preferences and boot ID are unchanged across
deployment and testing. Recorded phase checkpoints span 60.9–64.8°C, retain
`ondemand`, sample 1.5 GHz and report power alarm zero. These are checkpoints,
not continuous traces. No other device, governor or privilege setting changed.

## Queued uploads across moving and edited scenes

The offscreen `--geometry-stream-history` probe confirms that bucketed uploads
remain useful when completion can overlap subsequent CPU rendering on the Pi.
With three frames submitted but not yet reaped, culled Spacewars rendering
costs 10–18% less per completed frame than exact per-pass uploads. The terrain
histories are effectively unchanged. This supports an opt-in live comparison;
it does **not** justify a deeper gameplay queue or a new default.

```sh
target/release/engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stream-history --geometry-in-flight 3 \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 80 --presentation-repeats 4 \
  --geometry-output /tmp/stream-history-captures
```

The four controls are ranges/culled geometry crossed with exact/bucketed
per-pass uploads. The probe compares depth one with the requested depth
(1–8, default three). Its five default histories reuse early/later Spacewars,
changing camera zoom, terrain reveal/zoom, and terrain edit/removal/replacement
fixtures. `--geometry-stream-cases` selects a comma-separated subset. Existing
probes and live renderer defaults are unchanged.

Plans are prepared before timing. Each block uses a fresh compositor with
ordinary geometry prewarming targets and stream capacity, then an empty
geometry cache. The measured interval includes encoding, drawing, uploads,
flushes, bounded fence waits and the final drain, divided by **all 80 frames**.
Simulation, source extraction, projection, text/UI, presentation and scanout
are excluded. There are no capture commands in timed blocks. Do not compare
these absolute times with the preceding source-through-`glFinish` measurements.

Four repeats complete the four-mode Williams order at each depth: all sixteen
mode/position combinations and twelve within-row ordered predecessor pairs
occur once. Depth order alternates per repeat. Cross-depth boundaries and
longer driver history are not controlled. The sequence is limited to 128
frames and 256 MiB of accounted input payload; validation pixel-pack storage
is limited to 64 MiB. Accounting is not a complete heap/driver memory measure.
Prepared definitions remain alive across the sequence, so this is not a live
producer or dead-definition cache collection test.

### Pi queued rendering measurements

Pi 4 V3D / Mesa 25.1.6, 1024×768, mean completed-block cost per frame in
milliseconds. Depth denotes submitted fences not yet reaped by the CPU, not
necessarily that many unfinished GPU frames.

| History and geometry | Exact depth 1 | Bucketed depth 1 | Exact depth 3 | Bucketed depth 3 |
| --- | ---: | ---: | ---: | ---: |
| Early ranges | 9.845 | 9.909 | 5.674 | 5.429 |
| Early culled | 9.937 | 9.492 | 5.735 | 4.876 |
| Later ranges | 11.930 | 11.577 | 7.609 | 7.302 |
| Later culled | 10.153 | 9.704 | 6.183 | 5.063 |
| Changing zoom ranges | 15.326 | 15.219 | 10.070 | 9.769 |
| Changing zoom culled | 14.764 | 13.833 | 9.012 | 8.114 |
| Terrain reveal ranges | 9.858 | 9.900 | 4.907 | 4.925 |
| Terrain reveal culled | 10.000 | 9.937 | 4.990 | 4.997 |
| Terrain edit ranges | 7.242 | 7.277 | 4.555 | 4.549 |
| Terrain edit culled | 7.349 | 7.273 | 4.567 | 4.576 |

At depth three, bucketed culled rendering improves 15.0% early, 18.1% later and
10.0% during zoom. Render-thread CPU improves 7.9%, 10.4% and 8.8%, respectively:
5.114 → 4.711, 5.371 → 4.810 and 8.807 → 8.031 ms/frame. This is a rendering
subset, not whole-application CPU. The corresponding block-mean ranges are
5.393–6.338 → 4.833–4.984, 5.372–6.806 → 4.927–5.434, and
8.347–9.240 → 8.054–8.192 ms. Four short repeats are not a sustained-session
or statistical confidence guarantee. Terrain depth-three changes stay below
0.4%; bucketed capacity is not a general terrain acceleration.

Depth three improves throughput but increases CPU-observed completion age.
For the bucketed culled path, pooled warm p95 grows from 9.982 → 13.977 ms
early, 10.090 → 14.925 ms later, and 14.640 → 23.771 ms during zoom. These
are upper bounds measured when the CPU checks a fence, **not** GPU timestamps
or input-to-display latency. Warm excludes the first eight frames of each
block, not later edits/admissions. First-use spikes remain; early culled
depth-one maximum observed age even worsens from 26.430 to 34.531 ms with
bucketed uploads. Neither overlap nor capacity fixes every latency tail.

Logical stream capacity is independent of tested queue depth. Culled zoom
reserves 2,082,636 bytes exact versus 3,571,712 bucketed; later gameplay uses
1,373,244 versus 3,850,240 bytes. Actual geometry and uploaded bytes match on
every corresponding frame. These figures include prewarmed capacity and do
not measure driver orphan-store retention or peak in-flight GPU memory.
The existing eight-slot/16 MiB pool limit and shared-buffer fallback remain.

On the AMD RX 7800 XT / Mesa 26.0.8 workstation, every exact-to-bucketed
completed-cost change is below 1.8%. Culled zoom at depth three is effectively
tied, 0.660 → 0.661 ms. Overlap improves workstation throughput too, but the
capacity change remains primarily a measured Pi benefit.

### Queued history verification and next step

Each device passes 3,200 queued RGBA SHA-256/count comparisons against
synchronous shared-buffer references, with geometry policy held constant.
Validation captures every queued frame into fenced pixel-pack storage, maps
only after completion, and checks resize, empty frames, pool overflow, policy
switches and retirement while previous frames may still be pending. Hostile
VAO/VBO/EBO and pixel-pack state are preserved. These checks compare existing
geometry implementations; they are not an independent software-raster oracle.

Both final analyses verify 160 timed blocks and 12,800 frame records, matching
geometry, upload, admission and cache counts across policy/depth controls,
bounded queues, zero detailed submission-stage clocks, no timed pool fallbacks,
and nonshrinking bucket capacities. All 120 saved PNG pairs per device are
byte-identical. Pi zoom and terrain-replacement captures were visually checked.
The client/CLI/startup suites pass 602/22/3 tests, with four existing ignored
diagnostics. GPU Clippy retains 26 existing client warnings; software-only
compilation, formatting and diff checks pass.

Artifacts: `target/gpu-experiments/overlap.mKm4iKun/`, including complete
host/Pi CSVs, frame traces, captures, manifests, analysis/device scripts,
source/binary hashes and build/test logs. Smoke and `pi-progress.csv` outputs
are not the final comparison. Source hashes were rechecked after measurement.

Only `sw-picade-2` was updated; installed client/CLI SHA-256 prefixes are
`db1c7d1f7829` / `29a8461984de`. New binaries remain installed. Software Clock
resumed unpaused at 60 FPS / 60 UPS with no renderer error or automatic service
restart. Preferences and boot ID are unchanged. Recorded phase checkpoints
are 63.3–63.8°C, `ondemand`, 1.4–1.5 GHz, power alarm zero; these are not a
continuous health trace. No other device, governor or privilege setting changed.

The [live stream selector](#live-stream-policy-selection) below carries these
controls into normal application pacing. Do not deliberately queue three
gameplay frames to reproduce the offscreen throughput result. Preserve the
software and original geometry controls.

## Live stream policy selection

Upload policy can now be selected independently of geometry mode in a running
GPU-enabled kiosk. It is diagnostic, session-only and not saved:

```sh
spacewars-cli host gpu-scene retained-culled
spacewars-cli host gpu-stream bucketed
spacewars-cli status
spacewars-cli host gpu-stream exact
spacewars-cli host gpu-stream shared
```

`shared` retains the original shared exact-size buffers and remains the default.
`exact` uses separate exact-size vertex/index buffers per pass. `bucketed` uses
the bounded grow-only capacity policy, discarding stores before uploading used
bytes. The latter two retain the eight-slot/16 MiB pool limit and shared-buffer
overflow fallback. They work with `triangles` and CPU-expanded retained modes;
paths, instanced circles and GPU-resident geometry reject the command rather
than implying the selected strategy applies to them.

Policy changes do not change geometry mode, clear its tessellation cache or
alter frame pacing. GL changes occur only inside the current-context render
callback; no readbacks, fences or waits were added to live frames. Switching
to an incompatible geometry mode or off, leaving the session, rendering failure
or context teardown resets the requested policy to shared. A compatible
geometry-mode change or an in-session scenario restart can retain it. The
single-view and compositor paths release their per-pass pools when retired.

Status distinguishes `gpu_scene_stream_policy` from
`gpu_scene_stream_applied`, which is `inactive` when no supported GPU geometry
image is applied. `gpu_scene_stream_slots` and
`gpu_scene_stream_capacity_bytes` describe retained logical buffer stores,
including any dormant shared stores, not measured GPU memory. Average
`gpu_scene_stream_buffer_data_calls_avg`, `buffer_subdata_calls_avg`,
`buffer_store_bytes_avg`, `buffer_size_changes_avg` and `fallbacks_avg` all use
the `gpu_scene_stream_` prefix and the existing 120-frame measurement window.
A policy change resets that window so samples do not mix policies; requesting
the same policy is idempotent. The existing CPU timings are not GPU-active time.

### Live Pi comparison

The live comparison uses `sw-picade-2` at native 1024×768 and 1x scale, with
the normal host timer, simulation and KMS presentation. Each of Clock idle,
heavy rain and Spacewars runs all six permutations of shared, exact and
bucketed uploads: six blocks per policy, with positions and predecessors
balanced. Clock uses triangle geometry; Spacewars uses `retained-culled`.
No frame queue, simulation rate or governor was changed.

Each Clock block resets the existing 08:08 benchmark fixture, warms through
tick 240, then samples at phase ticks 360 and 480. Spacewars restarts the same
match, warms through tick 1380 and samples near ticks 1500 and 1620. GPU mode
is turned off between blocks to reset renderer resources; that reset is a
benchmark control, not a requirement for changing stream policy. Timed samples
use complete 120-frame GPU windows. Screenshots and transition checks run
outside these windows.

The primary live metric is whole-process CPU, measured from `/proc` CPU ticks
over roughly four seconds per block; 100% means one core. It includes live
simulation, scene production, UI and rendering. The host, GPU adapter and KMS
timing windows are recorded separately and are not exactly aligned, so their
averages must not be added to reconstruct a frame time. This is a paced
application comparison, not an unpaced throughput or GPU-active-time test.

Mean process CPU across the six blocks per policy, in percent of one core:

| Live fixture | Shared | Exact per pass | Bucketed per pass |
| --- | ---: | ---: | ---: |
| Clock idle | 32.13% | 32.22% | 32.24% |
| Clock heavy rain | 55.75% | 55.80% | 55.91% |
| Spacewars | 96.35% | 97.04% | 95.25% |

Clock stays at 60 FPS / 60 UPS for all 72 status samples. Its average CPU cost
is effectively unchanged. The largest sampled heavy-rain host interval is
20.55 / 20.74 / 16.92 ms for shared / exact / bucketed; this suggests a tail
improvement in these short windows, not an average-throughput gain.

In Spacewars the benefit appears mostly as more rendered frames, while process
CPU remains near one core. Across twelve samples per policy:

| Spacewars metric | Shared | Exact per pass | Bucketed per pass |
| --- | ---: | ---: | ---: |
| Submitted FPS | 45.12 | 45.42 | 48.38 |
| Simulation UPS | 60.02 | 59.98 | 59.79 |
| KMS draw CPU, ms | 12.922 | 12.912 | 10.968 |
| Geometry encoding wall time, ms | 4.623 | 4.629 | 4.528 |
| Geometry submission wall time, ms | 3.618 | 3.733 | 1.806 |
| Mean of host-window p95 intervals, ms | 25.361 | 24.871 | 23.652 |
| Largest sampled host interval, ms | 75.735 | 75.352 | 26.763 |

Relative to shared uploads, bucketed stores reduce draw CPU by 15.1% and
increase sampled FPS by 7.2%. Each of the six repeats has higher FPS and lower
submission time with bucketed stores than either exact-size control. The
application still misses 60 rendered FPS in this fixture. Submission timings
are CPU-observed GL-call wall time, not GPU-active time or proof of a particular
driver allocation mechanism. The maxima are observations from these windows,
not guaranteed bounds or pooled percentiles.

The Spacewars match uses seed 0, asteroids disabled, human P1 and
`material_mission_v9` P2. Actual samples are at ticks 1516–1520 and 1637–1642;
camera heights are 44 for P1 and roughly 410–431 for P2. Real scene production
and object lifetimes are exercised, including live camera changes, but this
is not a controlled zoom sweep or a full-match soak. The 120-render-frame
windows span different simulation intervals when FPS differs. Average uploads
are therefore similar, not identical: 1.730 / 1.736 / 1.684 MB per rendered
frame. Use the prior exact-history experiments for identical-workload evidence;
do not attribute every timing difference here solely to upload policy.

All Spacewars samples retain seven geometry draws, with five per-pass stream
slots for exact/bucketed and zero overflow fallbacks. Bucketed storage size
changes average 0.004 per frame versus 8.000 for shared and 3.372 for exact.
Sampled logical retained stores total 0.285 / 2.327 / 4.512 MB, including
dormant shared buffers. Those figures are not peak driver or GPU memory:
the shared path repeatedly replaces a smaller store within the frame. Process
RSS ranges overlap at roughly 76–82 MiB and do not isolate upload memory.

All nine full RGBA screenshot comparisons are identical: for each of idle
Clock, rain and Spacewars, the paused shared image matches exact, bucketed
and a return to shared. Rain and Spacewars captures were visually inspected.
These capture actual Slint text, menus, split views and minimaps, but the pause
menu darkens and obscures part of the world. They supplement rather than
replace the unobscured offscreen reference checks. The known rounded-border
issue remains; no physical controller or rotated Pi 5 test was performed.

Both host and Pi pass 24 layout/resize/raster-reference checks and the extended
live-presenter lifetime check: bucketed allocation, an exact-policy failed
resize retaining its borrowed texture, single/multiview replacement, zero
per-pass slots after CPU fallback, and context teardown. Live software,
launcher and incompatible-mode requests are rejected; leaving the session
clears the policy and releases its pool. All 54 blocks and 108 status samples
pass policy, scale, renderer and fallback validation. The client, CLI and
startup suites pass 605/23/3 tests, with four existing ignored diagnostics;
GPU Clippy retains 26 existing client warnings. Software-only compilation,
formatting and diff checks pass.

Artifacts are in `target/gpu-experiments/live-stream.xVwR1t7x/`: `pi/` contains
the complete device run, while `summary.json`, `analyze.mjs`,
`check-captures.py` and `capture-check.txt` provide the analysis and exact-image
checks. Build/test logs, layout captures and source/binary hashes are retained.
`pi-progress/` and its summary are partial snapshots, not final results.

Only `sw-picade-2` was deployed. Client/CLI SHA-256 prefixes are
`00d658834a1a` / `8a4845a8cb59`; the new binaries remain installed. The original
preferences and software Clock were restored, unpaused at 60 FPS / 60 UPS,
with no renderer error, automatic service restart or reboot. Health checkpoints
span 61.8–68.2°C with `ondemand` unchanged and power alarm zero. Spacewars
checkpoints are all at 1.5 GHz; Clock frequencies vary under normal governor
control. These are sampled health observations, not a continuous power trace.

Keep the stream and renderer defaults unchanged. The longer match and controlled
zoom follow-ups below extend this evidence. Bucketed uploads are now a credible
Pi comparison path, but they do not remove the remaining scene preparation and
geometry cost.

### Longer live match validation

A follow-up on the same Pi 4 keeps `retained-culled` and bucketed uploads active
for 608 measured seconds, without resetting the GPU resources between matches.
It uses the same seed, player configuration and normal cameras as above, with
status, process memory/CPU and health sampled every five seconds. Two matches
finish at tick 16458 with the same result, then a third match starts. Renderer
targets remain at three allocations, and the upload pool remains at five slots
with zero sampled overflow fallbacks.

Logical stream capacity grows from 3.18 to 6.35 MB during the first match; its
last observed growth is at 260 seconds, before either rematch. The tessellation
cache stays within 128 entries and peaks at 13.18 MB, below its 16 MiB limit.
Process RSS rises from 70.2 to 84.1 MiB, largely settling during the second
match. This supports bounded renderer resources over this run, not a proof of
zero leaks: allocator and driver memory are not isolated, and RSS is not fully
constant. No unexpected service restart or reboot occurs.

The run also exposes a limitation of the earlier short timing windows. The
largest sampled host interval is **122.58 ms**, at the first game-over screen;
that window contains 110.03 ms KMS draw wall time and 100.65 ms draw CPU, while
the host callback peaks at 14.50 ms. The second game-over does not repeat that
spike. Other 40–44 ms host intervals coincide with 30–36 ms simulation steps,
including a similar early-match spike after each restart. The upload pool is
stable at these checkpoints. Cold UI work is a hypothesis for the first
game-over spike, not an isolated cause from this soak alone; the
[aligned transition trace](#first-and-repeat-game-over-transitions) below
investigates it separately.
Bucketed stores do not make all application work predictable.

Across the whole run, including game-over/rematch intervals, process CPU is
84.62% of one core and submitted rendering averages 56.36 FPS. These are not
same-scene comparisons against shared uploads. Samples span 61.3–69.6°C, with
`ondemand` unchanged and the sampled power alarm zero. All three paused
shared/exact/bucketed screenshots match across every RGBA pixel; the bucketed
capture was also inspected visually. Software Clock and the exact original
settings were restored afterward.

The complete soak, analyzer, hashes and logs are in
`target/gpu-experiments/live-soak.fC4D7ZDg/`, with device data under `pi/` and
validated results in `summary.json`. It ran the preceding client/CLI binaries
(`00d658834a1a` / `8a4845a8cb59`), before the camera control below was deployed.

### Controlled live camera sweep

The diagnostic camera control is independent of renderer and stream policy:

```sh
# In an active material Spacewars session, using any rendering path:
spacewars-cli host camera-zoom sweep
spacewars-cli status
spacewars-cli host camera-zoom off
```

It changes only presentation: P1 camera height sweeps 44 → 640 → 44 world
units over 480 simulation ticks, with P2 in the opposite phase. Logarithmic
interpolation gives proportional scale changes. Absolute simulation ticks make
pause static and a same-world restart repeatable. Camera centers, normal camera
history, physics and saved settings are untouched. The override is applied
before scene culling and minimap camera footprints are generated, so those
remain consistent with the visible player panes.

`camera_zoom_mode` reports `off` or `sweep`; `camera_zoom_source_tick`
identifies the source tick when active and is `unavailable` otherwise. Use that
fresh presentation tick for phase matching, not the less frequently refreshed
match telemetry. The `gpu-zoom` command and GPU-prefixed status keys remain
compatibility aliases. Clock, native video and the launcher reject enabling
the sweep; disabling it is always safe. Leaving the session clears it.
Same-session restarts repeat it from tick zero. Renderer switches, GPU resource
retirement and fallback to raster leave it active, so comparisons keep the
same views. It is available in software-only builds too.

The original Pi 4 measurements below used the GPU-only version. The October 8
Pi 5 matched-zoom experiment moves the same camera formula into the shared
presentation path, before source culling, without changing that formula or
the rendering algorithms. No persistent option or default changes.

### Live camera sweep results

On the same Pi 4 at 1024×768 and 1x scale, four paired repeats compare shared
and bucketed uploads using `retained-culled`. Both policies appear first and
second twice. Each block resets renderer resources, restarts the same seed-0
match, warms for one eight-second sweep, then measures four sweeps: roughly
ticks 480–2400, or 32 seconds. Sixteen status samples per block cover alternate
extreme/midpoint phases; all 128 samples validate the actual camera heights
against their source tick. No simulation rate, presentation queue or governor
change is made.

Whole-window frame count, source ticks and process CPU come from cumulative
counters, not an average of the displayed FPS. The other means below average
the sixteen 120-frame status windows within each block, then four blocks per
policy. Those windows overlap, span different simulation intervals at different
render rates, and are not synchronized with the host/KMS windows. Do not add
these durations or interpret the mean of window p95s as a pooled percentile.

| Live zoom metric | Shared | Bucketed per pass |
| --- | ---: | ---: |
| Submitted FPS | 45.86 | 51.22 |
| Simulation UPS | 59.97 | 59.99 |
| Whole-process CPU, percent of one core | 94.68% | 93.24% |
| KMS draw CPU, ms | 13.370 | 10.820 |
| Geometry encoding wall time, ms | 4.816 | 4.856 |
| Geometry submission wall time, ms | 3.964 | 1.818 |
| Mean of host-window p95 intervals, ms | 30.549 | 25.172 |
| Largest sampled host interval, ms | 82.977 | 59.060 |

Bucketed uploads improve rendered FPS by **11.7%** and reduce draw CPU by
**19.1%**. Every paired repeat improves FPS and submission time. Encoding is
essentially unchanged; submission falls 54.2%. This is useful live changing-zoom
evidence, not merely steady-view reuse. Average uploads are almost equal at
1.535 MB/frame, but the rendered ticks and retained-cache histories still differ:
it is not a byte-identical workload comparison. Prior exact-history experiments
remain the control for that claim. CPU remains near one core and rendering
still falls short of 60 FPS.

All samples retain seven mesh draws. Bucketed uploads use five stream slots,
with zero overflow fallbacks and at most 8.53 MB of sampled logical stores,
including dormant shared buffers. Storage size changes average 0.0004 per frame
versus 8.602 for shared uploads. The cache remains at or below 128 entries and
13.03 MB. RSS checkpoints span 78.9–84.4 MiB across both policies; these include
allocator/driver retention between blocks and are not isolated upload-memory
measurements. Health checkpoints are 66.7–70.6°C and 1.5 GHz, with `ondemand`
unchanged and sampled power alarms zero. No unexpected restart or reboot occurs.

Paused captures near narrow/middle/wide phases compare shared, exact, bucketed
and a return to shared. All **nine full RGBA image comparisons match exactly**;
the three bucketed images were inspected visually. The captured ticks remain
identical across policy changes, as do camera heights and minimap footprints.
These checks include real UI composition, but the pause menu obscures the
middle of the world. They do not replace unobscured offscreen coverage or fix
the existing rounded-menu-border issue.

Camera control rejection, mode/session reset, normal-camera restoration and
pool/target retirement all pass on the device. The new unit/integration tests
also compare culled/reference images during a sweep and verify that restoring
normal cameras reproduces a scenario whose render method was never called.
The client/CLI/startup suites now pass **609/24/3 tests**, with four existing
ignored diagnostics. GPU Clippy has the same 26 existing client warnings;
software-only compilation, formatting and diff checks pass. The new host build
passes 24 layout/resize/raster-reference cases and the presenter lifetime check.
Those offscreen checks were not repeated on the Pi for this build; the preceding
build passed them on both host and Pi, and this build passes the live checks.

Complete zoom data, analysis and screenshots are in
`target/gpu-experiments/live-zoom.i73fVrHA/`: `pi/`, `summary.json`,
`analyze-zoom.mjs`, `check-captures.py` and `capture-check.txt`. Partial copies
under `pi-progress/` are not final evidence. Build/test logs and source hashes
remain in the soak artifact directory; the zoom directory includes the deployed
bundle manifest and host binary hashes. Only `sw-picade-2` was deployed, with
client/CLI SHA-256 prefixes `248497cf42e4` / `8b97cbcec95e`. Those binaries remain
installed. The original settings and software Clock were restored, unpaused at
60 FPS / 60 UPS, with zoom off and no renderer error.

Keep software, shared-upload and original-geometry controls available. The
focused first/repeat game-over trace below investigates the UI hitch; remaining
per-frame geometry/scene cost and recurring simulation spikes are separate
from upload performance. The controlled zoom run still has a 59 ms sampled
host interval. Neither result justifies a deeper gameplay frame queue, a
universal GPU default, or a claim of Pi 5 compatibility.

## Bounded live rendering traces

The rolling host and KMS maxima above can belong to different frames. For
transition diagnosis, an explicitly armed trace now records 240 complete KMS
event-loop iterations, preserving stage timings and counters from the same
iteration. It is session-local, does not write files on the device by itself,
and switches its extra clocks off automatically at the limit:

```sh
spacewars-cli host render-trace start
spacewars-cli status
spacewars-cli host render-trace read > render-trace.txt
# To end a capture early, retaining its completed iterations:
spacewars-cli host render-trace cancel
```

Start requires active LinuxKMS FemtoVG profiling and rejects an already-active
capture. It arms the next iteration, excluding the partial iteration handling
the command. Read returns metadata followed by CSV without clearing the result;
cancel preserves completed rows. A new explicit start replaces the previous
capture. Memory is allocated on start and bounded to 240 rows; no per-draw
allocation, file output, GPU readback, fence or new synchronization is added.
Normal status includes only capture state/count, not the CSV body. Disabled
profiling continues to use the original GL function pointers.

Since trace version 5, captures also record successful DRM submissions and consumed page-flip
events. `loop_start_at_ms`, `submit_at_ms` and `flip_observed_at_ms` share a
capture-relative CPU clock. `flip_kernel_us` is the raw timestamp from the
kernel event, in a separate clock domain; do not subtract it from the CPU
offsets. Consecutive kernel timestamps and wrapping `flip_sequence` values
show the actual page-flip intervals and intervening refreshes. They do not
measure GPU execution, input-to-display latency or the panel's response.
The event consumed in a loop belongs to an earlier submission, not the frame
submitted later in that loop. Empty fields mean no event, whereas sequence
zero is valid. Per-loop submission/completion/error/modeset counters make
those distinctions explicit. The added CPU clock reads occur only during a
bounded capture; no new fence, readback, event wait or scheduling change is
introduced.

Trace version 6 adds `geometry_*` columns from completed triangle preparation
passes in the same loop. Basic captures copy existing encoding times, geometry
counts and cache statistics; they do not enable the finer encoding timers.
Use `spacewars-cli host render-trace start-geometry` to add those timers to
the application/text detail enabled by `start-work`. This reuses the retained
renderer lab's resolve, expansion, fallback tessellation and validation clocks.
The detailed path also measures the lab's post-tessellation visibility scan;
that diagnostic work is not part of the ordinary renderer. Compare basic,
`start-work` and `start-geometry` captures to account for added overhead.

Geometry times are CPU-observed **wall** durations, not thread CPU time or GPU
execution. They nest inside encoding: cache construction is inside resolve,
and stroke preparation/tessellation/finalization are inside fallback-stroke
time. Do not sum nested columns. Camera/minimap passes accumulate in one loop;
`geometry_passes` and `geometry_detail_passes` identify their coverage. Detailed
cells are empty if unmeasured, including non-retained/resident paths that do
not expose these timers. RGBA/UI-only loops have zero geometry passes. The
extra fixed sample storage is bounded by the existing rolling/capture limits;
no per-shape rows, logging, GPU synchronization or persistent setting is added.

Alongside existing loop/timer/draw/upload/swap timings, the trace times GL draw,
shader-compile and program-link calls on the CPU. The FemtoVG adapter also
splits drawing into `femto_build_items` (render entry through the post-item
callback, including layout and the application's scene-rendering callback)
and `femto_after_items` (command flush, cleanup, post-render callback and swap).
These are nested spans: draw calls/uploads occur inside them, and flip-wait
and swap are inside the latter. Do not sum every column as independent work.
An empty CPU cell is unmeasured, not zero. Idle iterations may have no draw.

The active GPU scene/texture notifier marks the visible UI immediately before
drawing: 1 gameplay, 2 paused, 3 game-over, 4 launcher; 0 means no marker was
provided. This correlates the transition with its rendering iteration rather
than assuming a nearby status request describes the slow frame. Iteration
numbers identify ordering, not verified physical scanouts. Extra per-call
clock reads perturb traced rendering, so use this tool to attribute a hitch;
retain the ordinary untraced comparisons for performance claims.

### First and repeat game over transitions

Two natural seed-0 matches on `sw-picade-2` reproduce the first-use hitch in
one renderer process. Both end at tick 16,458 with Player 2 winning; the fixture
keeps asteroids disabled, P1 human, P2 `material_mission_v9`, native 1024×768
output, 1× rendering, `retained-culled` geometry, bucketed uploads and normal
camera zoom. Each capture is armed shortly before the ending and records
240 contiguous iterations spanning gameplay and the result screen. No forced
game-over, accelerated simulation or extra GPU synchronization is used.
"First" means the first result screen in this process, not an emptied disk
shader cache. The repeat changes round/score digits, so these are not
pixel-identical UI workloads.

The first game-over iteration is the slowest loop and draw in each capture:

| Same-iteration measurement | First appearance | Repeat appearance |
| --- | ---: | ---: |
| Event-loop wall time | 132.151 ms | 36.215 ms |
| Draw wall time | 119.832 ms | 23.561 ms |
| Draw CPU time | 109.302 ms | 16.267 ms |
| Build-items CPU time | 35.498 ms | 12.599 ms |
| After-items CPU time | 73.795 ms | 3.657 ms |
| GL draw calls | 2,653 | 125 |
| CPU time inside GL draw calls | 48.303 ms | 2.364 ms |
| Tracked buffer-upload bytes | 1,247,136 | 95,264 |
| Tracked texture-upload bytes | 0 | 0 |
| Explicit shader compile / program link calls | 0 / 0 | 0 / 0 |

The settled result screen uses 93 draw calls in both captures. Mean draw CPU
is 8.680 / 8.839 ms before the transition and 12.200 / 12.238 ms after its first
frame. The first appearance's flip wait and EGL swap take only 0.031 and
0.204 ms wall time. Its buffer uploads account for 4.178 ms CPU, much less than
the 109.302 ms drawing total. This locates the burst primarily in building and
executing drawing commands, rather than presentation waiting or buffer uploads.
Timings are diagnostic and include extra clock reads; do not treat the table
as an uninstrumented optimization comparison or sum nested stages.

The pinned source gives a strong candidate explanation. In FemtoVG 0.17.0,
`GlyphAtlas::render_atlas` generates missing glyphs through `render_glyph`.
For outline fonts, this renders each glyph eight times with subpixel offsets
and additive blending into an atlas texture, then reuses that texture on
later frames. Its cache key includes font, size, rendering mode and quantized
horizontal subpixel position. Slint 1.13.1's FemtoVG text renderer uses this
path for the result screen's ordinary text sizes. These atlas contents are
rendered on the GPU, so zero CPU-to-texture upload bytes does **not** mean
zero glyph-generation work. This internal font antialiasing is separate from
the application's 1×/2× raster scale.

The cold/warm draw-call difference and source inspection strongly implicate
on-demand glyph-atlas generation. They do not yet measure glyph-cache misses
directly or prove that every extra draw belongs to text. Zero explicit compile
and link calls also does not exclude deferred compilation inside the driver.
The smaller repeat burst may include newly needed score digits; that is an
inference, not a counted attribution.

The [text cache experiment](#isolating-the-text-atlas) below isolates first-use
glyph generation from cached drawing, including text size and subpixel layout
changes. It tests advance preparation in the actual renderer and a separate
canvas as a negative control. Do not reduce text quality or introduce hidden
full-screen warmup work as an unmeasured fix. This hitch is distinct from steady
geometry and simulation cost.

Validation passes 610 client, 25 CLI, 3 backend-startup and 32 vendored KMS
tests: 670 passed, with five existing ignored diagnostics. GPU Clippy and
software-only compilation pass with existing warnings; formatting, source
checksums and diff checks pass. Host layout/resize/reference checks pass,
including presenter lifetime. Both Pi result screenshots were inspected;
their changing scores intentionally prevent an equality claim. The live
capture also verifies duplicate-start rejection, automatic completion,
retained reads, early cancellation and rejection of start on software.

Evidence is in `target/gpu-experiments/game-over-trace.kwl9ayUB/`, including
complete `pi/` traces, screenshots, health records, `summary.json`, the
reproducible runner/analyzer, source/binary hashes and build/test logs.
`pi-progress/` is a partial copy, not the final comparison. KMS profile version
is 10 and trace schema version is 1. Only `sw-picade-2` was updated; installed
client/CLI SHA-256 prefixes are `ce2c67106982` / `511dcee1f6e7`. The new binaries
remain installed. Exact original preferences and software Clock were restored,
unpaused at 60 FPS / 60 UPS, with no renderer error, automatic service restart
or reboot. Measurement checkpoints read 69.1–69.6°C, 1.5 GHz, unchanged
`ondemand` governor and power alarm zero. These are samples, not a continuous
health trace. No other device, governor, renderer default or privilege setting
was changed.

## Isolating the text atlas

`--presentation-gpu-text` runs a text-only Slint fixture through the same pinned
FemtoVG renderer as the kiosk UI. It uses a private EGL pbuffer on an explicit
render node, never scanout, and refuses a software GL fallback. The fourteen
centered rows use result-screen text and representative sizes/weights with
Liberation Sans, the Pi's default sans-serif family. This is a focused cache
test, not the full game's layout or a reproduction of its total frame time.

```sh
cargo run --release -p engine-client --no-default-features --features pi-kiosk-gpu -- \
  --benchmark-presentation --presentation-gpu-text \
  --benchmark-width 1024 --benchmark-height 768 --presentation-repeats 6 \
  --gpu-text-output target/gpu-text-captures
```

The capture directory must not already exist. `--gpu-text-device` selects the
render node, defaulting to `/dev/dri/renderD128`. The fixture requires at least
640×480 and supports the presentation lab's 2048×2048 limit. It has a fixed
history rather than an arbitrary `--presentation-frames` count, and rejects
mixing other presentation experiments. Six repeats cover all six orders of
three modes:

- `cold`: new UI renderer with an empty glyph atlas, then reveal all rows.
- `row-prepared`: draw one complete row per preparation step in the same
  renderer, at its final position/style, then reveal all rows.
- `other-canvas`: perform that preparation, destroy its renderer, and reveal
  the rows in a new one. This controls for process-wide font/shaping caches
  and driver warmup without retaining the glyph atlas.

Every mode then repeats the screen, moves text items by half a pixel, changes
their centered layout width by one pixel, changes round/score digits, changes
font sizes by one pixel, and restores the original. Repeated intermediate
states check cache reuse. Each fresh renderer first draws an empty scene;
renderer/shader construction and model publication are outside the frame
timer. Counterbalanced repeats share process-level caches and do not clear
the driver's disk shader cache.

GL wrappers count draws to the default framebuffer separately from nonzero
framebuffers. In this fixture the latter are glyph-atlas targets: there are
no images, layers, opacity groups, shapes or shadows to create other offscreen
targets. Do **not** apply that classification unchanged to the full UI. The
counters allocate nothing and read no clocks per draw. Logical buffer/texture
payloads are also counted, not physical GPU memory or bus traffic.

The timer separates building items, whole draw submission and an explicit
`glFinish` drain after each frame. Complete wall time includes that drain;
CPU time is render-thread time, not GPU execution or all driver threads.
This deliberate synchronization belongs only to the offscreen latency test,
not live rendering. Preparation rows and reveal costs are both reported.
Readbacks, exact full-RGBA comparisons and PNG writes are outside the timers.
Six repeats produce 348 timed rows and 170 cross-mode/repeat image comparisons,
plus cold/warm/restored equality, nonempty-output and renderer-release checks.

### Text atlas results and limits

The Pi 4 test directly attributes the large draw burst to atlas population:
each fresh renderer issues **3,560 atlas draws and 14 output draws**. A repeated
screen issues the same 14 output draws and **zero atlas draws**. Preparing the
fourteen rows in the same renderer performs exactly those 3,560 atlas draws
before the reveal; the reveal then needs none. Preparing another renderer
does not help: its replacement performs all 3,560 again. All modes produce
identical RGBA output for each corresponding state, so this is not an
antialiasing or quality reduction.

Pi measurements below use render-thread CPU through draw submission and the
completion drain. A fresh atlas is distinct from first-process initialization:

| Work on Pi 4 | CPU time |
| --- | ---: |
| Reveal a fresh atlas after process warmup, mean of repeats 1–5 | 92.024 ms |
| Repeat the cached screen, mean of all six cold-mode blocks | 2.018 ms |
| Reveal after same-renderer row preparation | 1.246 ms |
| All row preparation plus its reveal | 182.638 ms |
| Largest single row-preparation step across all six blocks | 23.456 ms |
| Reveal in a new renderer after preparing another one | 91.881 ms |

Row preparation spreads the burst but roughly doubles total CPU work relative
to a fresh atlas in a warmed process. Its mean per-block largest step is
22.567 ms CPU / 25.639 ms complete wall, still over a 16.67 ms frame budget
before other app work. It is not a suitable general fix by itself. The host
shows the same tradeoff: 2.926 ms for a fresh atlas after process warmup versus
9.768 ms for preparation plus reveal, with a largest preparation step of
1.524 ms. Host timing was collected while a Pi cross-build was active and is
not an isolated-machine result.

Changing the text item's position by half a pixel reuses the atlas. Increasing
the centered text layout width by one pixel changes local glyph positions,
requiring **3,152 new atlas draws** and 81.207 ms complete CPU in the cold-mode
history. Increasing font sizes by one pixel requires **3,056 atlas draws** and
79.877 ms. Changing round/score digits adds only 48 atlas draws and costs
4.318 ms complete CPU. Restoring the original screen needs no atlas draws;
these are additional cached variants, not wholesale cache invalidations.
Avoid conflating item movement with text reflow or changing font sizes.

The initial fresh-disk-cache process frame costs 162.304 ms CPU, above the
recurring 92 ms atlas population cost. Two new-process controls reuse that
driver-cache directory and then use another fresh one: their first frames
cost 116.444 and 161.294 ms. Later fresh-atlas means remain 92.028 and
91.561 ms respectively. This points to additional driver-cache-sensitive
startup work, with other first-process work still present. It is not a direct
shader-execution or compiler profile, and one warm/fresh control pair does
not establish an exact universal startup penalty. Caching driver artifacts
does not remove the recurring glyph-generation cost.

Every fresh atlas records one 1,048,576-byte RGBA store. Subsequent tested
variants fit that store. CPU-to-texture upload bytes remain zero because
FemtoVG renders the glyph masks into the texture; the fresh frame instead
uploads 1,664,352 bytes of geometry, versus 27,936 bytes for cached output.
These are logical stores/payloads, not total resident GPU memory.

Keep production behavior unchanged. The CPU-mask comparison below tests
cheaper glyph generation with batched uploads, retaining this vector path
as its quality reference. Cold population, cached drawing, font/layout
changes and image quality are measured separately. Any future advance
preparation must share the actual renderer's atlas and enforce a work budget;
blindly warming whole screens or a separate renderer is not justified.

Validation passes 615 client, 25 CLI and 3 backend-startup tests, with four
existing ignored diagnostics. GPU Clippy retains the existing 26 client
warnings; software-only compilation, formatting, source checksums and diff
checks pass. Host and each of three Pi processes pass 348 timed-row checks
and 170 exact image comparisons. A host 640×480 smoke run also passes 20 image
comparisons. Twenty saved PNG pairs per full run are byte-identical across
modes; layout/score/size captures differ from the baseline. Half-pixel item
movement changes host pixels but not Pi pixels at the 1024×768 alignment;
zero new atlas draws alone does not prove visible subpixel movement.
An additional 1025×769 Pi run changes the pixels when moving the item by half
a pixel and still needs zero atlas draws, passing 20 image comparisons.
The pinned atlas uses nearest-neighbor sampling, so smooth fractional movement
must not be inferred from cache reuse alone.
Host and Pi captures were visually inspected. Font file hashes differ between
host and Pi, so image equality is asserted within each device, not across them.

Artifacts are in `target/gpu-experiments/glyph-atlas.Q9oKPGRa/`: final host/Pi
CSVs, summaries, captures, driver-cache controls, reproducible runner/analyzer,
source/font/binary checksums, and build/test logs. Smoke and preliminary build
logs are not the final timing evidence. The initial release smoke found a
benchmark-only window-retention error; explicitly hiding the temporary window
before destruction fixed it, and final runs verify release after every mode.
Final host/Pi builds completed after that correction. The standard build-cache
policy reclaimed 7.7 GiB of rebuildable debug artifacts; release binaries and
experiment evidence were retained.

Only `sw-picade-2` was updated, with client/CLI SHA-256 prefixes `20dabfa9c2aa` /
`511dcee1f6e7`. The new binaries remain installed. Each offscreen run temporarily
suspends the kiosk process and resumes it through a cleanup trap; no renderer
configuration, saved preferences or governor is changed. Software Clock is
back at 60 FPS / 60 UPS, unpaused, with matching preferences/boot ID and zero
automatic service restarts. Health checkpoints are recorded with each run;
they are not continuous power/frequency traces. No other device was changed.

## CPU glyph mask comparison

The opt-in `--gpu-text-masks` extension compares two glyph-atlas producers
through public FemtoVG APIs. It does **not** replace Slint's live text renderer.
Both paths use the same explicit regular/bold font files, FemtoVG shaping,
single-line layout, glyph bearings, tenth-pixel x quantization, padded atlas
rectangles and final glyph-quad drawing API. The reference still renders eight
vector samples per glyph. The alternative feeds the same font outlines through
`ttf-parser` and `ab_glyph_rasterizer`, writes analytical coverage into CPU RGBA
pages, and uploads one contiguous dirty row band per changed page. Pages remain
512×512 with nearest sampling in both paths; changing format or filtering is
not part of this comparison.

```sh
target/release/engine-client \
  --benchmark-presentation --presentation-gpu-text --gpu-text-masks \
  --gpu-text-font-regular /usr/share/fonts/ttf/LiberationSans-Regular.ttf \
  --gpu-text-font-bold /usr/share/fonts/ttf/LiberationSans-Bold.ttf \
  --benchmark-width 1024 --benchmark-height 768 --presentation-repeats 6 \
  --gpu-text-output /tmp/new-mask-captures
```

These font paths are for the Pi; use the corresponding local files on a
workstation. Font hashes are logged, and no fallback font is accepted. The
fixture has the same fourteen rows and ten-state history as the Slint text
probe. Every pair gets fresh atlases and alternates producer order. Font file
loading, parsing, shaping-cache preparation and empty Canvas initialization
are outside frame timing; the actual cached layout lookup, mask generation,
packing, CPU page allocation, GPU allocation/upload, quad building, submission
and completion drain are included. Font/shaping setup time is reported
separately. The CPU path reparses outlines on each mask miss; it does not get
a hidden precomputed bitmap cache.

Complete latency includes an explicit offscreen `glFinish`, not presentation,
scanout or a GPU timer-query result. Reports split build, upload, submit and
drain CPU/wall times and count logical payloads, mask misses, page count and
upload calls. Phase boundaries differ between the producers, so compare total
cost rather than interpreting their build columns as identical operations.
The first pair is reported separately from later warmed-process pairs.

Validation requires fourteen output draws, zero vector-atlas draws in the CPU
path, no population/uploads on cached states, and exact repeated/restored
images **within** each producer. Between producers, antialiasing is expected
to differ. The probe reports text-support pixel error, per-row coverage ratios
and brightness-centroid differences. It rejects missing rows, coverage ratios
outside 0.8–1.2, mean blue-channel error above 30/255 on text support, or any
row whose pixel error improves by translating the candidate one pixel in any
direction. These are gross regression guards, not a claim of visually
identical antialiasing. Native-size inspection remains necessary. Readbacks,
PNG writes and quality checks are outside all timers.

The experimental CPU atlas has a hard four-page limit and fails on exhaustion;
it has no eviction policy. It supports fill-only, nonvariable outline fonts,
face zero, positive local x and sizes 1–92 at scale one. It explicitly rejects
bitmap glyphs and unexpected fallback. It is not yet a general text backend:
strokes, font fallback, color/variable fonts, large text, transforms, persistent
cache eviction and live UI integration need separate work. Production
renderer and deployment defaults remain unchanged.

### CPU mask results on Pi 4

CPU-generated masks substantially reduce the recurring fresh-atlas hitch in
this fixture. These means exclude the first pair and use the remaining five
pairs at 1024×768, V3D 4.2.14.0 / Mesa 25.1.6 / GLES 3.1:

| Frame state | Vector CPU | CPU-mask CPU | Vector complete wall | CPU-mask complete wall |
| --- | ---: | ---: | ---: | ---: |
| Fresh atlas in a warmed process | 91.934 ms | 3.275 ms | 94.737 ms | 5.135 ms |
| Cached repeat | 0.871 ms | 0.774 ms | 2.796 ms | 2.677 ms |
| Centered layout width plus one pixel | 79.436 ms | 2.554 ms | 82.358 ms | 4.454 ms |
| Round and score change | 3.825 ms | 0.941 ms | 9.106 ms | 2.821 ms |
| Font sizes plus one pixel | 76.818 ms | 2.625 ms | 79.161 ms | 4.543 ms |

This is about **28× less render-thread CPU for the fresh-atlas frame**, not
a 28× general rendering or FPS claim. The alternative replaces 3,560 atlas
draws with one 143,360-byte texture upload containing 237 new masks. Both still
issue fourteen output draws. Geometry uploads fall from 1,664,352 to 27,936
bytes; both allocate the same 1 MiB GPU texture. The alternative also retains
one 1 MiB CPU pixel page plus cache metadata, and all tested histories stay
within that page. Counts are logical payloads, not physical traffic or peak
driver memory.

On the fresh CPU-mask frame, build/mask work costs 2.106 ms CPU, upload costs
0.198 ms, quad submission costs 0.798 ms and the explicit drain costs 0.161 ms
CPU. Its completed wall time is longer because GPU completion is awaited.
No atlas population or upload occurs on cached repeats or item-only movement.
Layout changes add 188 masks in one 151,552-byte upload; font-size changes add
190 in one 190,464-byte upload. The score change adds three masks and uploads
45,056 bytes. Dirty row bands deliberately include unchanged pixels, so these
are not the minimum possible upload sizes; optimizing that further is not
needed to establish this result.

A second process reusing the same driver-cache directory gives 92.176 →
3.290 ms fresh-atlas CPU and 94.961 → 5.166 ms complete wall, using the same
five-pair summary. An odd-sized 1025×769 run also passes the image/history
checks and visibly changes the half-pixel item-movement capture without new
atlas work. Its one post-first-pair observation is 93.193 → 3.366 ms CPU;
this smaller control is not a separate statistical estimate.

The host release run, collected after builds/tests finished, gives 3.393 →
0.457 ms CPU and 5.609 → 0.608 ms complete wall for a fresh atlas on the RX
7800 XT. This remains a per-frame completion test, not queued throughput.
The first process block always starts with the vector path; do not compare
that cold-driver frame to the CPU path that follows it as if both had isolated
cold startup. Font/shaping preparation for the entire history is separately
17.250 ms CPU on Pi. Canvas creation is also outside the frame timer. These
results do not claim to eliminate all first-launch or full-UI work.

All ten direct-vector captures match the earlier Slint captures exactly on
each device. CPU-mask output intentionally differs at antialiased edges:
on Pi, mean blue-channel error is 9.794–10.259/255 over text support, with
per-row coverage ratios 0.976–1.020. Zero translation minimizes row pixel
error in every tested state. Native-size host/Pi captures were inspected;
text stays aligned/readable, with subtly different edge shading. Host and Pi
font file versions differ, so cross-device equality is not asserted.

The initial debug smoke used a brightness-centroid guard that rejected the
host font-size case: unequal shading across a long row shifts its weighted
centroid by 0.907 pixels without moving the glyphs. That metric is now reported
only. A tested translation-sensitive pixel comparison replaces it, together
with coverage and edge-error limits. This is a change to the validation
method, not a claim of exact CPU/vector pixels or a hidden threshold relaxation.

The full host and two Pi runs each pass 120 timed-row checks and 100 exact
within-producer image comparisons, plus cached/restored equality. Host 640×480
and Pi 1025×769 controls each pass forty rows and twenty comparisons. Tests
pass for 620 client, 25 CLI and three backend-startup cases, with four existing
ignored diagnostics. Software-only compilation, formatting, source hashes
and diff checks pass. Clippy completes with 27 client warnings, including one
nonfunctional nested-if suggestion in the new capture code.

The next useful step is a **switchable live-UI CPU glyph-atlas prototype**,
keeping vector generation as the reference/fallback and bounding retained
resources. Test the real launcher, settings, result screens, font/layout
changes and repeated sessions before changing defaults. This experiment
does not exercise those full Slint lifecycles and does not solve unrelated
geometry, simulation, rounded-border or Pi 5 issues.

Artifacts are in `target/gpu-experiments/glyph-masks.Mgs09FEv/`: CSVs, summaries,
captures, the runner/analyzer, font/source/binary hashes and build/test logs.
Debug/smoke data are not the reported release timings. Only `sw-picade-2` was
updated; client/CLI SHA-256 prefixes are `46def1fc6200` / `511dcee1f6e7`. The new
binary remains installed, with CPU masks available only in the opt-in probe.
The software Clock, exact saved preferences and boot ID are preserved; the
offscreen runner resumes the same kiosk process through a cleanup trap.
Final checks show 60 FPS / 60 UPS, unpaused, no renderer error and zero
automatic service restarts. Sampled health remains around 66°C with power
alarm zero and the unchanged `ondemand` governor; those are checkpoints, not
a continuous power trace. No other device, governor, renderer default or
privilege setting was changed.

## Live UI CPU glyph cache

The CPU-mask path now runs inside the pinned FemtoVG 0.17.0 dependency, so
Slint can use it for ordinary UI text. It remains a session-only experiment;
software presentation and upstream vector glyph generation remain defaults.
The narrowly modified dependency, licenses, bounds and tests are described in
[`vendor/femtovg/SPACEWARS_PATCH.md`](../vendor/femtovg/SPACEWARS_PATCH.md).

With the GPU kiosk renderer already active, compare:

```sh
spacewars-cli host gpu-text cpu
spacewars-cli host gpu-text cpu-run-faces
spacewars-cli host gpu-text vector
spacewars-cli status
```

Each request asks for a fresh cache, even when repeating the same policy.
The change is applied before a canvas's first text draw after a flush; queued
text never loses its textures mid-frame. Software presentation rejects these
commands explicitly. Status reports the requested policy, current CPU-cache
pages/entries/canvases, rasterized masks, uploads, resets, fallback runs and
errors. These are counters, not GPU timing. No preferences are saved.

Each canvas retains at most four 512×512 RGBA pages and 4096 entries. That is
at most 4 MiB of CPU pixels and 4 MiB of GPU texture storage, excluding metadata,
temporary work and driver overhead. Capacity pressure falls back for the rest
of the frame, then recycles safely. Unsupported text uses the original path;
its atlas is frame-local in CPU mode and is not included in that persistent
page cap. An allocation/upload error disables the CPU cache until an explicit
reset. Canvas destruction releases residency. Ordinary outline fills at scaled
sizes 1–92 are supported; stroke, bitmap, variable-font and negative-position
runs are not covered by this experiment.

### Slint fixture and live screens

On October 4, 2026, the same real Slint text fixture on `sw-picade-2` measured
the following completed-frame render-thread CPU times. These are means of
five fresh-canvas repeats after discarding the first process repeat, with
software Clock stopped during the private-pbuffer runs. Each policy ran in
its own process; this is not an isolated first-driver-use comparison.

| Slint fixture state | Vector masks | CPU masks |
| --- | ---: | ---: |
| Fresh atlas | 91.567 ms | 13.526 ms |
| Cached repeat | 1.621 ms | 1.415 ms |
| Centered layout width plus one pixel | 79.982 ms | 10.632 ms |
| Font size plus one pixel | 77.791 ms | 10.917 ms |

Fresh-atlas complete wall time falls from 94.261 to 15.464 ms, and atlas draw
calls fall from 3560 to zero; both paths still issue 14 output draws. The
integrated CPU path uploads once per dirty page **per text run**, retaining
synchronous error handling. Here that is 14 uploads and 731136 bytes, versus
the standalone experiment's one upload and 143360 bytes. This implementation
does not yet reproduce the standalone 3.275 ms CPU result.

The host's corresponding cold means are 4.395 → 2.082 ms CPU and 6.755 →
2.344 ms completed wall. Host 640×480 and Pi 1025×769 controls also pass.
Repeated, row-prepared and newly created Slint renderers produce exact pixels
within each policy and release all CPU-cache residency at teardown. All ten
states also match the preceding standalone experiment exactly for each policy
on each device: 40 decoded-image comparisons. CPU/vector antialiasing still
differs as previously measured; this is not cross-policy pixel equality.

Live comparisons reset the cache on the actual launcher, App Settings and
settled result screens. Each row below is the mean of the largest draw-CPU
sample in each of three short reset windows; order alternates between policies.
The settings-entry case instead warms the launcher with a fresh cache before
opening App Settings. Performance text and auto-start were temporarily disabled
to keep the fixture stable, then their exact saved settings were restored.

| Live UI case | Vector draw CPU | CPU-mask draw CPU | GL draw calls before and after |
| --- | ---: | ---: | ---: |
| Cold launcher | 106.500 ms | 21.119 ms | 3107 → 59 |
| Enter App Settings from warmed launcher | 78.501 ms | 17.479 ms | 2120 → 72 |
| Cold App Settings | 90.165 ms | 18.994 ms | 2576 → 72 |
| Cold result screen | 162.398 ms | 27.242 ms | 4959 → 87 |

These instrumented draw timings are not end-to-end input latency or GPU
execution times. A natural CPU-text match ends at tick 16458, with its first
result frame using 30.934 ms draw CPU and 87 GL draws; later result draws average
11.887 ms. This is a useful functional/transition check, not a paired speedup
against the earlier natural-match capture, whose UI instrumentation differed.
The fully reset result comparison above is also different from a natural
transition that reuses some gameplay glyphs.

Sixteen launcher/settings/device-info cycles retain exactly one CPU page and
355 entries after the first cycle, with no further mask generation or uploads.
The subsequent full match finishes at one page and 1180 entries, with no CPU
errors or fallback runs. Live captures of launcher, settings and results were
visually inspected. Text remains aligned/readable; the pre-existing rounded
border defect remains in both policies. These checks do not establish coverage
for arbitrary fonts, UI scaling, unsupported text or the Pi 5.

The next focused experiment is **font-face reuse within a text run**. The new
cache currently calls `Font::face_ref()` for every missing glyph, and that
upstream method rebuilds a Rustybuzz face. In one cold CPU-text launcher draw,
GL uploads take only 1.600 ms of 20.099 ms draw CPU; most work remains inside
Slint/FemtoVG item construction. Repeated font parsing is a concrete candidate,
not yet a measured explanation of the whole remaining gap. Keep upload batching
as a separate variable and retain the vector reference.

Validation passes for 623 client tests, 26 CLI tests, three backend-startup
tests and nine dependency tests; four existing diagnostics remain ignored.
The two upstream dependency tests also pass with the CPU feature disabled.
The new dependency tests are wired into Linux CI without requiring a display.
Software-only compilation, formatting and source/diff checks pass. Clippy
finishes with the existing 26 client warnings; the new dependency path is clean.

Artifacts are under `target/gpu-experiments/live-glyphs.rhAHYNZX/`: probe CSVs,
live traces, captures, runners/analyzers, hashes, build/test logs and restoration
checks. The final client/CLI SHA-256 prefixes are `194cf276d744` / `0fc23b883f44`.
Only `sw-picade-2` was updated. The new binaries remain installed; its original
preferences, boot ID and software Clock are restored. Final status is unpaused,
60 FPS / 60 UPS, with zero automatic service restarts and no renderer error.
Sampled temperatures are about 62–67°C, power alarm remains zero and the governor
remains `ondemand`; these checkpoints are not a continuous power trace.

## Per-run font-face reuse

The October 4 follow-up isolates font parsing from atlas packing and upload
batching. `cpu-run-faces` borrows one parsed Rustybuzz face per needed font for
the duration of a text run; the existing `cpu` policy still parses per missing
glyph. An all-hit run does neither. Faces do not survive the run or duplicate
font bytes. Text shaping, masks, subpixel keys, atlas limits, uploads and output
quads are unchanged. Vector remains the default; both CPU policies are opt-in.

Use `spacewars-cli host gpu-text cpu-run-faces` on the GPU kiosk, or
`--presentation-gpu-text --gpu-text-policy cpu-run-faces` in the offscreen
probe. Both paths retain `cpu` and `vector` for comparisons. Status now includes
`gpu_text_face_parses_total`; probe CSV version 3 adds per-measurement mask
misses, face parses, upload calls and upload bytes. The parse counter covers
CPU mask generation only, not upstream shaping or every font parse in Slint.

### Controlled fixture

Each device ran two opposite-order blocks of six fresh-canvas repeats per CPU
policy in the same build, discarding repeat zero of each process for the
summary. These are means of ten cold-history samples, at 1024×768 and 1× scale.
The kiosk was stopped only during the Pi's private-pbuffer probes and resumed
immediately afterward. Governor and frame pacing were not changed; the probe
itself is unpaced. Timings include render-thread CPU through completion and
are not GPU execution timestamps or an application-wide FPS comparison.

| Pi Slint fixture state | Per-glyph faces (`cpu`) | Per-run faces (`cpu-run-faces`) |
| --- | ---: | ---: |
| Fresh atlas | 13.607 ms | 5.507 ms |
| Cached repeat | 1.448 ms | 1.379 ms |
| Half-pixel item movement | 1.427 ms | 1.390 ms |
| Centered layout width plus one pixel | 10.736 ms | 4.685 ms |
| Three new score glyphs | 2.032 ms | 1.994 ms |
| Font size plus one pixel | 11.015 ms | 5.111 ms |

Cold completed wall time falls from 15.496 to 7.369 ms. Fresh-atlas CPU means
in the separate order blocks are 13.460 → 5.383 and 13.753 → 5.630 ms, so the
win is present in both orders. The host's corresponding CPU means are
1.449 → 0.634 ms, with completed wall time 1.670 → 0.846 ms; host timing is
secondary because other workstation tasks were active.

The fresh fixture still generates 237 masks, uploads 14 row bands totaling
731136 bytes, and draws 14 output batches with zero atlas draws. Only the
mask-generation face parses change: **237 → 14**. Warm/item-movement samples
generate no masks, parses or uploads; the three new score glyphs occupy three
runs, so that case still parses three faces. All 696 paired measurement rows
per device have identical mask, upload, geometry-byte and draw workloads.

Across both devices, 240 decoded CPU-policy image pairs match exactly: ten
states, three cache histories, two normal-sized blocks and 640×480/1025×769
controls. Another 120 image comparisons confirm that the unchanged CPU and
vector controls reproduce the preceding build. CPU/vector antialiasing still
differs; this change does not alter either rasterizer's output. Renderer/cache
teardown, unsupported-text fallback and mask-allocation limits remain checked.

### Live menus, match and capture caveat

Four alternating-order cache-reset windows per policy on each screen give
these mean peak draw-CPU samples. Auto-start and performance text were disabled
during measurement. Counts and timings are from the actual GPU kiosk; they are
not input-to-display latency or GPU execution measurements.

| Live UI case | Per-glyph faces | Per-run faces | Mask-generation face parses |
| --- | ---: | ---: | ---: |
| Cold launcher | 21.632 ms | 13.693 ms | 186 → 13 |
| Cold App Settings | 18.021 ms | 13.660 ms | 145 → 15 |
| Cold result screen | 26.836 ms | 18.508 ms | 284 → 33 |
| Enter App Settings, matched warmed-parent focus | 20.816 ms | 12.391 ms | 111 → 14 |

Each pair generates identical masks, uploaded bytes and draw counts. Result
draws still issue 87 GL calls and upload 1374208 bytes in 33 calls. The original
settings-entry series is retained in the artifacts but not used in the table:
its first baseline started with a different launcher selection and generated
one extra mask. The replacement series explicitly matches that focus. Its
first baseline also contains a driver-buffer-upload spike; keeping it in the
mean avoids silently discarding it. Medians for that row are 17.197 → 12.431 ms.
The controlled offscreen results and parse counts are the clearer measure of
this change alone, not a claim that every menu draw now meets a deadline.

Sixteen launcher/settings/device-info cycles stabilize after the first cycle
at one page and 355 entries, with no further parses, masks or uploads. A full
seed-0 Spacewars match ends at tick 16458, retaining one page and 1180 entries
with zero glyph-cache errors or fallback runs. Its first natural result draw
takes 26.904 ms CPU; steady result draws average 11.884 ms. The transition's
whole-loop CPU is 39.259 ms, including 12.180 ms in timers. This still exceeds a
60 Hz frame budget and is not the same workload as resetting an already-shaped
result screen. Do not infer hitch-free transitions from the cold-cache win.

Live capture also exposed a validation hazard: Slint's FemtoVG snapshot reads
the existing swap-buffer history, and one idle-window redraw may not replace
every buffer. The first launcher `cpu` screenshot was exactly the preceding
vector-policy image, while a later `cpu-run-faces` capture showed CPU masks.
Repeated readbacks/redraws outside timing windows settle to identical pixels
across CPU policies for launcher, settings and results (four captures each),
including stable final consecutive captures. These were visually inspected.
Do not use the earlier single-shot live launcher pair as a CPU/vector
antialiasing comparison. The offscreen probe reads its current private target
and is unaffected. A general latest-frame screenshot guarantee remains a
separate renderer/CLI follow-up; this experiment does not implement one.

The next measurement should separate remaining CPU mask generation from
Slint text/layout work before broadening the cache or deferring uploads. Cold
result uploads now cost only about 2.125 ms of the 18.508 ms draw CPU; the natural
transition uploads cost 1.905 ms of 26.904 ms. Frame-wide upload batching remains
a candidate, but cannot explain the entire remaining cost and must preserve
queued-text safety and error/fallback handling. Keep simulation timing separate.

Validation passes for 623 client, 26 CLI, three backend-startup and 12 dependency
tests (664 total; four existing diagnostics ignored), plus the two upstream
tests with the CPU feature disabled. New tests cover mixed-font pixel identity,
per-run parse counts, cache hits, later-run misses, queued policy switches,
capacity fallback and injected upload failure. Software-only compilation,
formatting, diff checks and source/binary hashes pass. Clippy completes with
the existing 26 client warnings and existing KMS unreachable-pattern warning;
the modified dependency path is clean.

Artifacts: `target/gpu-experiments/run-faces.YQQmhV0K/`, including all raw CSVs,
capture histories, analyzers, source/binary hashes, build/test logs and device
checks. Pi client/CLI SHA-256 prefixes are `bb3a24f7d9d9` / `19133fc02d9c`.
Only `sw-picade-2` was deployed. It is left running GPU Clock with
`retained-culled`, `bucketed` and `cpu-run-faces` active, not restored to the
previous renderer. Temporary preference edits were reversed through the UI;
the saved file matches its original hash. Boot ID is unchanged, service restart
count is zero and final status is unpaused at 60 FPS / 60 UPS with no renderer
error. Sampled temperatures were about 63–70°C, power alarm zero, governor
`ondemand`. These are checkpoints, not a continuous power trace. Pi 5,
arbitrary-font coverage and the pre-existing rounded-border defect are still
outside this experiment.

## Separating text preparation from mask generation

The next diagnostic pass distinguishes a cold glyph atlas from genuinely new
text. This matters: atlas resets preserve FemtoVG's shaping caches. They expose
mask costs but omit a substantial part of the first result-screen transition.
No rendering policy, cache size, layout or rasterization algorithm changes in
this pass.

The pinned library now exposes optional coarse text/path hooks. Clocks are
enabled only by an explicit probe or bounded live capture:

```sh
spacewars-cli host render-trace start-text
# Exercise a transition; capture stops automatically after 240 complete loops.
spacewars-cli host render-trace read
# Or stop early, retaining completed loops:
spacewars-cli host render-trace cancel
```

Use `start` instead of `start-text` for the control without extra text clocks.
The offscreen equivalent is `--gpu-text-detail` alongside
`--benchmark-presentation --presentation-gpu-text --gpu-text-policy cpu-run-faces`.
Omit `--gpu-text-detail` for its control. KMS profile version 11, trace version 2
and text-probe version 4 append ten operation counts and CPU/wall spans. Empty
CPU fields still mean unmeasured, not zero. No per-glyph/word/vertex timers,
unbounded capture storage, text-content logging or extra GL synchronization
are added to live rendering.

### Isolated text and capture overhead

Picade 2 ran at 1024×768 with the same `cpu-run-faces` policy throughout.
Two opposite-order process blocks each ran six probe repeats; the table uses
ten post-first-repeat samples per case. All numbers are milliseconds of
complete render-thread CPU, including the offscreen completion boundary.

| Text fixture | Detail off | Detail on |
| --- | ---: | ---: |
| Fresh atlas with previously shaped text | 5.382 | 5.854 |
| Repeated cached text | 1.395 | 1.863 |
| Centered layout width changed by one pixel | 4.843 | 5.250 |
| Font size increased by one pixel | 5.070 | 5.550 |

Detailed clocks add about 0.47 ms in the first two cases. That is material for
the warm fixture; these spans diagnose work, not production throughput. The
fresh-atlas draw contains 2.791 ms of mask construction, 0.916 ms of mask uploads
and 0.185 ms of shaping API/cache lookup work. It has zero shaping-run misses.
These costs are nested within larger text stages and must not be added to
their parents. Mask construction includes lookup, rasterization, packing,
allocation and quad creation, not just rasterization. The work is unchanged:
237 masks, 14 face parses, 14 uploads totaling 731,136 bytes and 14 output draws.

Four alternating captures per mode also exercised live launcher, settings and
result screens, with automatic launch and the FPS overlay temporarily disabled.
Settings-entry captures normalize the launcher's selected control first. Mean
peak draw CPU and the nested detailed spans are:

| Live case | Detail off | Detail on | Mask build | Mask upload | Shaping API |
| --- | ---: | ---: | ---: | ---: | ---: |
| Launcher atlas reset | 16.012 | 14.483 | 5.195 | 2.041 | 0.456 |
| Enter settings from warmed launcher | 12.730 | 13.407 | 3.625 | 2.342 | 0.523 |
| Settings atlas reset | 13.733 | 14.805 | 5.241 | 2.084 | 0.543 |
| Result atlas reset | 18.687 | 20.111 | 4.529 | 2.288 | 0.601 |

The launcher mean is distorted by a 12.454 ms GL buffer-upload call in one
control capture; medians are 13.712 ms off and 14.565 ms on. This is not evidence
that enabling clocks speeds up drawing. Other live capture overhead is roughly
0.7–1.4 ms in these means. All paired work counts match, with no cache errors or
fallback. Path fill/stroke preparation totals only about 0.2–0.6 ms per captured
screen, so UI path tessellation is not the main cold cost here.

### The first natural result screen

The seed-zero match again reaches its result at tick 16,458. Its first detailed
result iteration costs 41.344 ms CPU: 11.691 ms in timers and 29.473 ms in
drawing, with small remaining callback/dispatch/submission costs. Within that
loop, shaping-run misses account for **8.760 ms across 51 calls**; all shaping
API work totals 9.650 ms. Mask construction is 2.374 ms, mask uploads 2.215 ms
and glyph-quad preparation 0.221 ms. There are 24 actual texture uploads and
87 GL draws. Subsequent result iterations have zero shaping-run misses and
average 13.364 ms draw CPU with the extra clocks still enabled.

Thus the mask-reset experiment alone misses an important first-use cost.
The next focused investigation should separate font-face setup, word shaping
and glyph-metric/outline discovery inside the uncached shaping path. The source
currently constructs a RustyBuzz face for each uncached word; that is a candidate,
not yet a measured explanation for all 8.760 ms. Preserve shaping/fallback
behavior and compare genuinely new text, not just atlas resets, before adding
a larger cache. Mask generation and per-run uploads remain separate targets.

Unclassified draw time is not automatically Slint layout. The result dialog
still draws the world behind it, and `femto_build_items` includes our
`BeforeRendering` scene callback. Text clocks cover the whole loop, including
measurement outside drawing; other profile windows have different boundaries.
Do not subtract their aggregates to manufacture an exact layout cost. These
measurements do not resolve the independent geometry or simulation costs.

All 240 host/Pi detail-off/detail-on fixture image pairs match exactly, as do
their previous `cpu-run-faces` references. The 1024×768, 640×480 and odd
1025×769 cases retain identical glyph/upload/draw work. Settled live launcher,
settings and result captures are stable; screenshots stay outside timing
windows. The full match uses one CPU atlas page with 1,212 entries and zero
errors/fallback before the subsequent explicit resets.

Artifacts are under `target/gpu-experiments/text-detail.D9ra1vLY/`, including
raw captures, counterbalanced runners, analyzers, pixel checks and build/test
logs. The initial measured client SHA-256 starts `4390c862f0ea`; the final
lint-cleanup build starts `d84586cdde5e`, with the same CLI `b7437f019318`.
Only early-return spelling in the diagnostic collectors changes between them;
the final build repeats the offscreen controls and bounded-trace checks. Its
480 capture files and 1,856 paired workload rows match the measured build;
a completed 240-loop detailed trace is followed by 32 ordinary loops with no
text clocks recorded, confirming automatic shutoff on the device.

Validation passes for 625 client, 26 CLI, three backend-startup, 34 KMS and
15 FemtoVG tests (703 total, five existing diagnostics ignored), plus the two
upstream tests with CPU-cache/diagnostic features disabled. Software-only
compilation, formatting and diff/source-hash checks pass. Clippy finishes with
the existing 26 client warnings and KMS unreachable-pattern warning; there are
no new client warnings from these hooks.

Only `sw-picade-2` was updated, without a reboot. It is left on the final build
running GPU Clock with `retained-culled`, `bucketed` and `cpu-run-faces`, unpaused
at a sampled 60 FPS / 60 UPS. Temporary preference changes were restored
through the UI; the saved settings hash is unchanged. The boot ID is unchanged,
service restart count is zero and there are no text/scene renderer errors.
Temperature checkpoints were about 64–70°C, power alarm zero and governor
`ondemand`; this is not a continuous hardware-health trace. No other target or
production renderer default was changed.

## Reusing font faces during uncached shaping

The fresh-context follow-up identifies repeated RustyBuzz-face construction
as a substantial cold-text cost. The new opt-in `cpu-shape-faces` mode reuses
faces across uncached words in one shaping run, on top of `cpu-run-faces` mask
generation. It reduces cold shaping CPU without changing glyphs, font fallback
order, line breaks, masks, uploads or the persistent shaping caches. It does
not improve already-shaped text by design.

The implementation borrows font data, parses lazily, and retains at most eight
faces until the shaping run returns. Overflow fonts use the original path.
There is no persistent face cache, duplicated font data, eviction policy or
new invalidation dependency. `vector`, `cpu` and `cpu-run-faces` remain controls;
the default is still `vector`. The live switch is:

```sh
spacewars-cli host gpu-text cpu-shape-faces
```

### Fresh context experiment

`--gpu-text-shaping` runs without a Slint window, EGL, display ownership or
glyph atlas. Each repeat registers explicit regular/bold font files into a
fresh `TextContext`, then measures and line-breaks the 14-row result fixture.
Font loading/registration, signature generation and validation are outside
the timers. Stages include identical repeats, a one-pixel width change, a score
change and a one-pixel font-size change. For example on the Pi:

```sh
engine-client --benchmark-presentation --presentation-gpu-text \
  --gpu-text-shaping --gpu-text-policy cpu-shape-faces \
  --gpu-text-font-regular /usr/share/fonts/ttf/LiberationSans-Regular.ttf \
  --gpu-text-font-bold /usr/share/fonts/ttf/LiberationSans-Bold.ttf \
  --presentation-repeats 20
```

Compare with `--gpu-text-policy cpu-run-faces`. Add `--gpu-text-detail` only
to diagnose the work: the isolated fixture then enables face-construction,
word-shaping and glyph-discovery clocks. Live KMS captures ignore these three
fine stages. There are still no per-glyph clocks. Text-probe CSV version 5
appends these columns; KMS profile/trace versions remain 11/2. Missing CPU
samples stay distinct from measured zero.

Two opposite-order process blocks per mode used 20 fresh contexts each.
The following means exclude each process's first repeat, leaving 38 samples
per case. These are thread CPU milliseconds **without detailed clocks**, not
GPU or complete-frame times. The Pi kiosk was temporarily stopped during the
standalone probes and resumed by an exit trap; its governor was not changed.

| Picade 2 shaping case | Per-word faces | Per-run faces |
| --- | ---: | ---: |
| Fresh text and glyph metrics | 6.796 | 3.606 |
| Identical cached repeat | 0.085 | 0.080 |
| Width changed by one pixel | 3.469 | 1.801 |
| Score changed | 0.535 | 0.529 |
| Font size changed by one pixel | 6.388 | 3.264 |

Cold shaping falls 46.9%; width and size changes fall 48.1% and 48.9%.
Cached repeats and the small score edit are effectively unchanged at this
scale. The workstation cold comparison is 0.700 → 0.381 ms. The pre-refactor
Pi control was 6.729 ms, close to the new 6.796 ms control.

Detailed Pi captures explain the change: face construction falls from 122
calls / 3.898 ms to 28 calls / 0.967 ms. RustyBuzz word shaping still runs 122
times at about 2.00 ms; glyph discovery stays about 0.50 ms. Those clocks are
intrusive: whole-fixture CPU rises from 6.796 to 8.364 ms in the control, and
from 3.606 to 4.981 ms with reuse. Do not add nested spans or use the detailed
totals as production throughput.

All 1,280 output signatures per device agree across policies, clocks and
fresh contexts, and with the pre-refactor references. Unit tests additionally
compare full glyph metrics and line boundaries, including mixed fonts,
combining marks, bidirectional text, missing glyphs and very narrow lines.

### Warm atlas control and visual checks

The existing Slint fixture deliberately preserves its shaping caches when
resetting the atlas. Its Pi fresh-atlas render is essentially unchanged:
5.408 → 5.452 ms CPU, with cached repeats 1.383 → 1.416 ms. This control helps
separate the new shaping improvement from the earlier mask-generation wins.

All 180 host and 180 Pi capture pairs match byte-for-byte at 1024×768,
640×480 and odd 1025×769 sizes. All 4,176 paired workload rows retain identical
mask, mask-face-parse, upload, draw and buffer-byte counts. The mask `face_parses`
counter intentionally excludes shaping-face parses. Those are separately
visible only through the isolated word-detail probe.

### Natural result transition

Two fresh application processes reached the same seed-zero result at tick
16,458, one with each policy. The first result iteration has the same 51
shaping-run misses, 87 GL draws, 24 texture-upload calls, buffer bytes and
other operation counts. Its settled screenshot is byte-identical; captures
were taken outside timing and verified stable across successive reads.

| First result iteration with coarse text clocks | Per-word faces | Per-run faces |
| --- | ---: | ---: |
| Uncached shaping CPU | 8.636 | 5.296 |
| All shaping API CPU | 9.511 | 6.261 |
| Draw CPU | 28.895 | 26.068 |
| Timer CPU | 11.528 | 11.927 |
| Whole-loop CPU | 40.594 | 38.177 |

This is one transition per policy, not a statistical full-match performance
claim. Texture bytes differ slightly, 976,896 versus 962,560, despite equal
upload calls and exact pixels. The processes had different pre-match Clock
histories; atlas packing/history is a possible explanation, not an isolated
cause. The counterbalanced fresh-context fixture is the cleaner comparison.
Mask construction/upload CPU is effectively unchanged at about 2.4/2.0 ms.
Subsequent result draws have no shaping misses and average 13.236/13.504 ms.

The first transition is improved, not hitch-free. The remaining timer work,
scene behind the dialog and UI preparation need separate accounting. Do not
label the remainder as layout or assume another text cache will remove it.

Artifacts, repeatable runners, CSVs, images, analyzers and validation logs are
under `target/gpu-experiments/shaping-cost.dht3otnd/`. These results cover the
measured fonts and screens, not arbitrary font formats or all possible UI text.

The measured client SHA-256 starts `9b35322e0781`; the final deployed build
starts `fdf45178ca49`, with CLI `c82201ec8638`. Only the spelling of a fixture
validation condition, outside its timers, changes between these builds. The
final Pi fresh-context comparison repeats at 6.784 → 3.603 ms. Its 60 GPU
capture files and 232 workload rows match the measured build, as do all 360
final host capture files. A complete 240-loop live text capture is followed
by 38 ordinary loops with zero extra text clocks, verifying automatic shutoff.

Validation passes for 627 client, 26 CLI, four backend-startup, 35 KMS and
19 FemtoVG tests: 711 total, with five existing diagnostics ignored. The
feature-disabled vendor suite also passes three tests. Software-only compilation,
formatting and source/diff checks pass. Clippy completes with the existing
26 client warnings and KMS unreachable-pattern warning; the new fixture's
style warning was fixed in the final build.

Only Picade 2 was updated, without rebooting. It is left on GPU Clock with
`retained-culled`, `bucketed` and `cpu-shape-faces`, unpaused at a sampled
60 FPS / 60 UPS, one CPU atlas page and no renderer errors or fallback.
Temporary preference changes were reversed through the UI and the saved
settings hash matches the original. Boot ID is unchanged, service restart
count is zero, the final temperature checkpoint is about 70°C, power alarm
zero and governor `ondemand`. These are health checkpoints, not continuous
power monitoring. Other targets and renderer defaults are unchanged.

## Frame-aligned application attribution

Picade 2's result-screen capture now separates the scenario-host timer and
direct scene-rendering callback from the broader timer and UI-renderer spans.
The largest recurring cost in this workload is rebuilding, staging and drawing
the unchanged world behind the dialog: 14.741 ms, about 70% of the instrumented
loop's CPU time. Publishing the scoreboard model itself is cheap. This is an
attribution experiment, not a renderer optimization or a GPU execution profile.

```sh
spacewars-cli host render-trace start-work
# Wait for kms_trace_state=complete (240 complete event-loop iterations).
spacewars-cli host render-trace read
```

`start-work` includes the coarse text/path clocks from `start-text` and adds:

- `app_host_tick`: the whole active scenario-host timer callback.
- `app_host_step`, `app_host_result`, `app_host_scene`, `app_host_publish`,
  `app_host_diagnostics`: nested simulation/input, result/menu properties,
  scene generation, presentation staging, and diagnostics/HUD publication.
- `app_before_render`, `app_after_render`: the application's rendering
  callbacks, including direct GPU scene work and resource retirement.

These are CPU/wall observations, not GPU execution times. The host children
are included in `app_host_tick`, which is included in `timers`. Subtracting
`app_before_render` from `femto_build_items` isolates work outside that
callback, **not layout alone**. Text clocks cover the full loop and can include
measurement outside drawing; they must not be subtracted wholesale from draw
time. Absent CPU measurements remain empty rather than being reported as zero.

The new application clocks are disabled for `start` and `start-text`, before
the first complete captured loop, after the 240-loop limit, and after cancel.
Software/non-GPU client builds compile the application spans into no-ops.
There are no per-object clocks, new GPU waits, persistent settings or automatic
trace files. KMS profile version 12 and trace CSV version 3 append the new
fields; existing commands remain available as overhead controls.

Validation passes for 627 client, 26 CLI, four backend-startup and 37 KMS
tests: 694 total, with five existing diagnostics ignored. The added backend
tests cover explicit opt-in, partial-loop exclusion, automatic shutdown,
early-return scope completion and cancellation. Software-only compilation,
formatting and diff checks pass. Clippy completes with existing warnings.

### Natural transition and recurring work

The normal Yocto GPU build and fast update completed after additional host
disk space was freed. No disk guard was lowered. The measured/deployed client
SHA-256 starts `1eaf06de50ce`, with CLI `9a38c3a4439a`. At 1024×768 and 1x, with
`retained-culled`, `bucketed` and `cpu-shape-faces`, the seed-zero match again
ends at tick 16,458. The 240-loop capture contains 100 gameplay draws, the first
result draw and 139 subsequent result draws. FPS overlay and autostart were
temporarily disabled through the UI; the governor remained `ondemand`.

The table partitions render-thread CPU into non-overlapping buckets. Its
first column is **one transition**, not an average. The steady column averages
480 result draws from two subsequent `start-work` captures. The world contains
7,260 source primitives / 7,232 GPU scene shapes in this particular workload;
these are not counts of visible objects or newly uploaded vertices.

| CPU work, milliseconds | First result loop | Steady result mean |
| --- | ---: | ---: |
| Simulation and input | 3.483 | 0.005 |
| Scene construction | 2.337 | 2.343 |
| Scene staging and publication | 5.168 | 5.183 |
| Result properties and diagnostics | 0.650 | 0.033 |
| Other timer work | 0.112 | 0.107 |
| Direct scene callback | 8.199 | 7.214 |
| UI item preparation outside callback | 15.811 | 4.519 |
| Post-item work including flush and swap | 1.930 | 1.611 |
| Other loop work | 0.163 | 0.180 |
| Whole loop | 37.853 | 21.195 |

The first result loop spends 11.750 ms in timers and 25.944 ms in drawing.
Only 0.029 ms is in the result/menu-property span, including scoreboard
publication; its remaining 0.621 ms diagnostic work is also far smaller than
scene construction/staging. Cold text still matters: the loop has 51 shaping
misses / 5.350 ms uncached shaping CPU, plus 2.442 ms in mask construction and
2.077 ms in its upload span. These text spans are already included in the
table, not additional work to add to it. They do not isolate layout cost.

The subsequent 139 result draws in that same capture average 21.183 ms CPU,
close to the later steady captures. Simulation is stopped, and there are no
further shaping misses, but the host still generates/stages the world and
requests another draw each tick. The result-screen cost is therefore not
primarily continuing physics, repeated scoreboard construction or fresh glyph
generation. The last 100 gameplay draws average 17.104 ms CPU; that short,
changing pre-result window is not a representative full-match benchmark.

### Instrumentation controls and visual verification

Four 240-loop captures of the frozen result use `start-text`, `start-work`,
`start-work`, `start-text` order. The text-only mean is 21.160 ms CPU versus
21.195 ms with application spans, a 0.036 ms difference within the observed
run variation, not a precise calibrated overhead. A subsequent basic `start`
capture, without either optional clock layer, averages 19.888 ms. Coarse text
clocks still have meaningful overhead; use basic captures for performance
comparisons and detailed captures for attribution.

Every steady comparison draw has 87 GL draws, 69,760 buffer-upload bytes,
zero texture-upload bytes and zero shaping misses. Application fields have
zero calls and absent CPU values in the text-only/basic controls; the basic
control also has no text clocks. Ordinary gameplay before the capture and
the final Clock check likewise report zero application-clock samples.
The analyzer verifies all 240-row limits and nested accounting. Settled
screenshots are byte-identical before/after the controls and to the previous
font-face-reuse build; visual inspection confirms the same result screen.

The next general rendering target is scene preparation and staging, followed
by the direct renderer's encoding work. Split out projection, allocation and
temporary-data lifetime costs before choosing a change. Reusing an unchanged
world on paused/result screens is a separate worthwhile opportunity, but it
does not establish a gain for moving cameras or active simulation. Retain
genuinely new-text transitions, live gameplay and zoom controls; these results
do not justify another broad text cache or changing renderer defaults.

Logs, raw traces, screenshots, bounded runners and the analysis script are
under `target/gpu-experiments/frame-attribution.tKQrOWfS/`. The matching remote
directory is `/tmp/spacewars-frame-attribution.P7Mj2RMD/`. Only Picade 2 was
updated, without a reboot. It is left on GPU Clock with the three experimental
policies above, unpaused at a sampled 60 FPS / 60 UPS and no renderer errors.
Temporary preferences were restored through the UI and the saved settings hash
matches the original. Boot ID is unchanged and service restart count is zero.
Start/end temperature checkpoints are about 64/67°C with power alarm zero;
these are not continuous hardware-health measurements. Other targets and
production defaults are unchanged.

## Scene preparation and temporary storage

The finer Picade 2 capture separates projection from plan publication, overlay
updates and destruction of temporary CPU data. Projection is substantial, but
cleanup is too: freeing the source frames and the projected plan costs a
combined 2.426 ms on the frozen result screen. This is not GPU resource
destruction or a synchronization wait.

`render-trace start-work` now adds six nested CPU/wall spans:

- `app_scene_project`: validation, allocation, projection and reuse-hint work.
- `app_scene_stage`: publishing the plan, including replacing a pending plan.
- `app_scene_overlay`: separate Slint text and presentation-property updates.
- `app_source_drop`: releasing source render frames after GPU staging.
- `app_scene_draw`: direct scene encoding/submission, before plan destruction.
- `app_plan_drop`: releasing or recycling that temporary CPU presentation plan.

The first four are inside `app_host_publish`; the last two are inside
`app_before_render`. They remain coarse, opt-in and bounded to 240 complete
loops. Basic/text-only captures have no application clocks. Profile version 13
and trace CSV version 4 append these fields. This distinguishes construction
from destruction, **not allocation from projection arithmetic within
construction**, and does not measure GPU execution.

### Measured construction and disposal costs

The instrumented client SHA-256 starts `bdf9c98124e2`; the CLI remains
`9a38c3a4439a`. The same natural seed-zero match ends at tick 16,458, with the
same 7,260 source primitives and 7,232 projected shapes, native 1024×768 output,
and `retained-culled`, `bucketed`, `cpu-shape-faces` policies. No rendering
algorithm changed for this capture. Its settled screenshot is byte-identical
to the preceding attribution build.

The steady column averages 480 result draws in two detailed captures. The
first column is one natural transition, not a statistical transition result.
These rows are selected nested components and must not all be added to the
whole-loop row.

| CPU work, milliseconds | First result loop | Steady result mean |
| --- | ---: | ---: |
| Source scene construction | 2.423 | 2.468 |
| Projection and plan construction | 4.090 | 3.947 |
| Plan publication | 0.003 | 0.003 |
| Overlay and presentation properties | 0.322 | 0.205 |
| Source frame destruction | 1.079 | 1.131 |
| Direct drawing excluding plan destruction | 6.805 | 5.744 |
| Projected plan destruction | 1.289 | 1.295 |
| UI item preparation outside direct callback | 15.873 | 4.544 |
| Whole loop | 38.284 | 21.322 |

Text-only/application captures in ABBA order average 21.294/21.322 ms CPU,
a 0.028 ms difference within run variation. The basic capture without either
optional clock layer averages 20.044 ms. Use basic captures for performance
comparisons and the detailed captures for attribution; the probe overhead is
not zero. GL workload and settled images agree across the controls.

### Switchable allocation experiment

`spacewars-cli host gpu-plan reference|reserved|scratch` selects a session-only CPU
preparation policy independently of geometry retention and buffer uploads.
`reference` remains the default. `reserved` preallocates the draw list and each
polygon's point list from known input lengths, and builds a primitive-to-draw
index only for layers with reuse hints. It also preallocates that index when
needed. It changes neither camera arithmetic nor tessellation, primitive order,
clipping, filtering or hint validation. There is no retained frame storage,
new GPU resource, persistent setting or rectangle-specific rendering path.

The policy requires an active GPU scene, clears a pending plan when changed,
and resets on scene exit, failure and teardown. Repeating the same request is
idempotent. `gpu_scene_plan_policy` reports the selected policy. Tests compare
complete projected plans from Clock, Pizza and Spacewars at multiple ticks,
zoom levels and odd-sized viewports. They also cover accepted/rejected hints,
skipped primitives and malformed coordinates/styles.

### Allocation comparison on the device

The measured/deployed client SHA-256 starts `d9e8d4962cdb`, with CLI
`3ee979fa8607`. The reference policy again reaches the same natural result
at tick 16,458, with a byte-identical screenshot. The comparison then switches
policies in the **same process and frozen match**, without resetting geometry
caches. Each detail level uses reference/reserved/reserved/reference followed
by the opposite order: four 240-loop captures per policy, 960 loops per cell.
Commands, three-second settling intervals and screenshots are outside captures.

| CPU work, milliseconds | Reference mean | Reserved mean |
| --- | ---: | ---: |
| Whole loop with basic clocks | 20.427 | 19.744 |
| Whole loop with detailed clocks | 21.739 | 21.308 |
| Projection with detailed clocks | 4.135 | 3.516 |
| Source construction with detailed clocks | 2.519 | 2.519 |
| Source destruction with detailed clocks | 1.127 | 1.148 |
| Projected plan destruction with detailed clocks | 1.325 | 1.350 |

The basic whole-loop reduction is 0.682 ms, **3.3%**. Every reserved basic
capture mean is below every reference capture mean in this run. Basic p95
falls from 22.145 to 21.611 ms, but maximum CPU is effectively unchanged at
23.753/23.798 ms. Detailed projection improves 15.0%; cleanup does not improve.
Detailed whole-loop CPU falls only 2.0%, with some offsetting drawing variation.
This is a small preparation improvement, not evidence that all remaining
allocation/lifetime cost has been removed or that tail latency is solved.

All 16 screenshots match exactly. Every captured loop has the same 87 GL
draws, 12 buffer-upload calls, 69,760 buffer bytes, zero texture uploads and
zero shaping misses. There are no renderer errors or fallback. Basic captures
have zero application/text clocks; detailed captures have one call to each
new scene boundary per draw. The subsequent ordinary Clock run again has zero
application-clock samples, confirming automatic shutoff on the deployed build.

This reservation-only result covers the frozen workload, not moving-camera
throughput or a general Clock/Pizza improvement. Reserved draw capacity is
based on input count and can include primitives later filtered out. The
bounded-storage follow-up below measures reuse separately, including moving
cameras. Paused/result-world reuse remains a separate task.

Validation passes for 632 client, 27 CLI, four backend-startup and 38 KMS tests:
701 total, with five existing diagnostics ignored. Software-only compilation,
formatting and diff checks pass; Clippy completes with the existing 26 client
warnings and KMS unreachable-pattern warning. One new fixture initially tried
to register an invalid two-point reuse hint; it was corrected to keep that
ordinary polygon while only registering valid hints, then the full suite passed.

Raw traces, scripts, images, analyses and validation logs are under
`target/gpu-experiments/scene-preparation.jtLVv5dy/`; the allocation comparison
is in its `reserved/` subtree. Remote artifacts are in
`/tmp/spacewars-scene-preparation.poQ7H2kX/` and
`/tmp/spacewars-scene-reserved.wU76eWwc/`. Only Picade 2 was updated, without
rebooting. It is left on GPU Clock with `retained-culled`, `bucketed`,
`cpu-shape-faces`, and the `reference` plan policy. Temporary preferences were
restored exactly through the UI; the settings SHA-256 still starts
`89a32bc36c24`. Boot ID is unchanged, restart count zero, renderer errors zero.
Comparison temperature checkpoints range from 69.6 to 71.6°C; the final check
is 67.7°C with power alarm zero and governor `ondemand`. These are checkpoints,
not continuous thermal or power monitoring. Other targets and production
renderer defaults are unchanged.

### Bounded scratch plan storage

`host gpu-plan scratch` extends the reserved policy by retaining at most one
completed CPU presentation plan. It reuses the draw vector and polygon point
vectors by output slot, but rewrites and validates every point, style, pass,
camera transform, clipping rectangle and reuse hint. It does not cache the
scene's contents. Primitive type changes can replace individual allocations;
skipped primitives and smaller scenes truncate the previous draw tail.

The retained owned-capacity limit is 8 MiB, including vector capacity rather
than just occupied length. An oversized plan is discarded; the next frame
still builds normally. Recycling releases immutable artwork references and
never retains source frames or GPU resources. If a second update replaces a
plan before it is drawn, the obsolete pending plan supplies the storage.
Switching plan policy or geometry mode, leaving the scene, rendering failure
and teardown release the storage. Source-frame construction and destruction
remain unchanged.

Diagnostics report `gpu_scene_scratch_limit_bytes`, `idle_bytes`,
`high_water_bytes`, `reuses` and `discards` with the same prefix. Idle bytes
are zero while the allocation is checked out for a pending draw; that does
not mean the live plan owns no memory. The high-water mark measures accepted
retained CPU capacity, not process RSS, allocator overhead, peak active-frame
storage or GPU memory.

The offscreen `--geometry-plan-profile` probe gives all policies the same
24-frame production histories, with separate exact-image checks for odd-sized
viewports, 44/640-unit camera heights and scenario replacement. Timed blocks
cycle those short histories after one complete warmup cycle. They measure
plan construction plus disposal/recycling, either alone or through completed
GL drawing. Simulation, source-frame generation, text/UI and scanout are
outside these timings. Six repeats balance mode positions and within-block
predecessors. This is an allocation comparison, not a long continuous match
or a latency benchmark.

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-plan-profile --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 96 --presentation-repeats 6 \
  --geometry-output /tmp/plan-profile-new
```

### Scratch storage on moving Pi replays

The Picade 2 release client SHA-256 starts `d9b2aa8e4cb7`, with CLI
`6d87c6e712b1`. Each cell contains 576 frames across six counterbalanced
96-frame blocks, at 1024×768 with `retained-culled` and bucketed streams.
The launcher remains idle during the probe: its draw and GL-draw counters
do not advance. Both workstation and Pi validate 252 draws across the three
policies, with exact within-device pixel and draw-count agreement.

| Production history | Reference plan CPU ms | Reserved plan CPU ms | Scratch plan CPU ms | Scratch retained capacity |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 1.739 | 1.599 | 0.827 | 583,224 bytes |
| Late bots | 3.016 | 2.786 | 1.426 | 527,808 bytes |
| Close-up zoom | 3.199 | 3.099 | 1.583 | 976,448 bytes |

These plan timings include disposal or recycling. Relative to reservation
alone, reuse removes 48–49% of that work. The timed storage stays below 1 MiB,
with zero budget discards. The early replay moves its cameras at constant
height; the late replay changes P1 zoom on all 24 sampled frames. The
close-up replay keeps P1 at 44 world units while P2 changes height on all
24 frames, from 418.693 to 427.670. The explicit 44/640 zoom jumps and resize
checks are additional correctness tests, not part of the timed history.

| Production history | Reference completed wall ms | Reserved completed wall ms | Scratch completed wall ms | Reference to scratch CPU ms |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 11.427 | 11.295 | 10.302 | 6.521 → 5.360 |
| Late bots | 15.094 | 14.813 | 13.478 | 10.500 → 8.797 |
| Close-up zoom | 16.913 | 16.665 | 15.220 | 12.065 → 10.245 |

Completed geometry rendering improves 9.9–10.7% in wall time and 15.1–17.8%
in thread CPU versus reference. Compared with reservation alone, the wall-time
reduction is 8.7–9.0%. Completed wall p95 improves in all three histories:
11.784→10.441, 16.654→14.807 and 18.035→16.312 ms. These timings include
`glFinish`, not GPU-active time or the full application loop. They exclude
source generation, text/UI and scanout, and reuse short warmed input histories.
Temperature checkpoints are 61.8 and 65.2°C, not continuous thermal telemetry.

### Scratch storage in the live application

The same binary runs the natural seed-zero match with scratch enabled, ending
at the reference tick 16,458. Its settled result image is byte-identical to
the preceding build. Scratch capacity peaks at 1,825,976 bytes (1.74 MiB), with
zero discards and no renderer/text errors. The first result loop still costs
36.727 ms CPU with 51 shaping-run misses: eliminating temporary allocation
does not eliminate the cold UI transition.

The frozen comparison stays in that process and match, changing only the
plan policy. Basic captures use reference/reserved/scratch, then rotate the
order twice; detailed captures reverse those orders. Each cell averages three
240-loop captures, 720 loops per policy and detail level. Commands, three-second
settling intervals and screenshots are outside the timed captures.

| CPU work in milliseconds | Reference | Reserved | Scratch |
| --- | ---: | ---: | ---: |
| Whole loop with basic clocks | 20.819 | 20.464 | 18.137 |
| Whole loop with detailed clocks | 21.985 | 21.716 | 19.875 |
| Projection with detailed clocks | 4.174 | 3.825 | 2.469 |
| Plan disposal or recycling with detailed clocks | 1.265 | 1.246 | 0.349 |
| Source construction with detailed clocks | 2.663 | 2.662 | 2.675 |
| Source destruction with detailed clocks | 1.258 | 1.299 | 1.346 |
| Direct drawing with detailed clocks | 5.722 | 5.763 | 6.037 |

Basic whole-loop CPU improves **12.9% versus reference and 11.4% versus
reservation alone**. Every scratch basic-block mean is below every control
mean in this run. Basic p95 falls from 22.374 to 20.857 ms versus reference;
maximum CPU falls from 24.773 to 23.952 ms, but remains above the reserved
maximum of 23.771 ms. Detailed whole-loop improvement is smaller, 9.6% versus
reference, with offsetting variation in drawing and source cleanup. Keep
basic clocks for the performance result and detailed clocks for attribution;
these are nested components, not rows to sum into a new total.

All 18 screenshots match. Every captured loop has 7,232 projected shapes from
7,260 source primitives, 87 GL draws, 12 buffer-upload calls, 69,760 buffer
bytes, no texture uploads and no shaping misses. The scratch policy does not
reduce GPU geometry or draw work. Retained CPU capacity is bounded rather
than accumulated across mode changes; process RSS checkpoints plateau at
85,988 KiB after starting at 85,900 KiB. That short sequence is not a memory
leak soak. Comparison temperatures range from 67.2 to 69.6°C.

The default remains `reference`. Scratch is now a supported experimental
control with both changing-camera and whole-app evidence, not a Clock/Pizza
performance result or a guarantee for every workload. The source-storage
follow-up below isolates the remaining source-frame creation and disposal,
about 4.0 ms in this run, while preserving the source/geometry interfaces. Unchanged
world reuse and cold UI preparation remain distinct follow-ups.

Validation passes for 637 client, 27 CLI, four backend-startup and 38 KMS
tests: 706 total, with five existing diagnostics ignored. Software-only
compilation, formatting and diff checks pass. Clippy completes with the
existing 26 client warnings and backend unreachable-pattern warning. Tests
cover allocation reuse, changing/shrinking/empty scenes, rejected input,
artwork release, exact budget boundaries, pending-plan replacement and
failure/session cleanup. Workstation and Pi moving-image checks pass.

Raw scripts, traces, images, analyses and build/test logs are under
`target/gpu-experiments/scene-reuse.hi0dhREz/`; device artifacts are also in
`/tmp/spacewars-scene-reuse.Xiegie4D/`. Deployment waited roughly two minutes
for a filesystem flush before replacing the app; it then completed its normal
health check, with no new kernel I/O errors observed. Only Picade 2 was updated,
without rebooting. Temporary preferences were restored exactly; settings SHA
still starts `89a32bc36c24`, boot ID is unchanged and restart count is zero.
It is left on GPU Clock with retained-culled geometry, bucketed streams,
CPU-shaped text and scratch plan storage. Ordinary Clock status has no active
application-trace samples or renderer errors. No other device or production
renderer default changed.

### Bounded source point storage

`host gpu-source reference|pooled` isolates allocation in the source producer,
independently of the projected-plan policy. Both return the same ordinary owned
`RenderFrame` values. The reference remains the default; nothing is saved in
settings. The initial opt-in producer is material Spacewars terrain, including
its fragments and both player views. Other adapters use their existing path.

`RenderPointPool` is renderer-independent storage in `engine-common`. It keeps
empty polygon point vectors in seven capacity classes, from four to 256 points.
The host supplies it to participating producers and recycles source frames
after projection and text-overlay preparation. Every coordinate and style is
generated afresh, with unchanged conservative culling and hint validation.
Text, layers, finished frames and immutable artwork references are not retained.
This is allocation reuse, not an unchanged-world or geometry-content cache.

The host's retained-capacity limit is 2 MiB, including the bucket directories.
Buffers outside the supported sizes use ordinary allocation. Over-budget
storage is released; the working frame still renders normally. Retention in
each class is limited to the current producer demand, so unrelated ship/HUD
polygons cannot accumulate. Shrinking scenes release excess point vectors;
small directory capacities can remain until reset. Policy changes, geometry
mode changes, scenario exit, renderer failure and teardown release the pool.
Diagnostics report idle bytes, high-water bytes, reuse, misses and budget
discards. Idle bytes exclude checked-out vectors owned by a working frame.

The source experiment runs with:

```sh
spacewars-cli host gpu-scene retained-culled
spacewars-cli host gpu-stream bucketed
spacewars-cli host gpu-plan scratch
spacewars-cli host gpu-source pooled
# Control with the same renderer and projected-plan policy:
spacewars-cli host gpu-source reference
```

`--geometry-source-profile` compares source creation plus disposal/recycling,
then source-through-completed-geometry wall/thread CPU. Unlike the prepared
plan probe, it generates fresh source frames on every measured iteration.
Matched scenarios advance one untimed simulation tick before each frame;
each block starts from the same seed/tick and has 16 warmup ticks. AB/BA
repeat order balances policy order. Completed geometry includes scratch
projection, retained-culled drawing, bucketed uploads and `glFinish`, but not
text, UI, scanout or simulation time. It is not input latency or GPU-active time.

Validation compares both producers on the same moving state: 24 ticks across
native, odd landscape and portrait viewports, for each of three production
fixtures. Renderer and pool resources survive fixture replacement. Ordinary
frame values, pixels and draw counts must match exactly. Separate tests cover
terrain edits, abrupt close/far cameras, empty visibility, hint contents,
capacity limits, allocator reuse, artwork release and unsupported adapters.

### Source allocation measurements on Pi 4

The deployed release client SHA-256 starts `d5c38bb98e33`; CLI starts
`5ecdf9c33cd4`. The source probe measures 96 consecutive moving frames per
block, four repeats per policy/path/fixture: 384 samples per cell. It samples
ticks 137–232, 1,817–1,912 and 1,517–1,612 respectively, after the common warmup.
These are longer, newly generated histories than the preceding 24-frame plan
replays; compare policies within this run, not their absolute times across runs.

| Source creation and cleanup CPU, ms | Reference | Pooled | Reduction | Peak retained bytes |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 1.240 | 0.688 | 44.5% | 213,280 |
| Later bots | 2.271 | 1.205 | 46.9% | 284,128 |
| Close-up and changing zoom | 2.695 | 1.472 | 45.4% | 403,520 |

| Completed geometry, ms | Reference wall | Pooled wall | Reference CPU | Pooled CPU |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 11.212 | 10.730 | 6.654 | 6.074 |
| Later bots | 12.746 | 11.633 | 8.230 | 7.087 |
| Close-up and changing zoom | 18.052 | 16.831 | 13.317 | 12.029 |

Completed wall time improves 4.3%, 8.7% and 6.8%; CPU improves 8.7%, 13.9%
and 9.7%. Wall p95 improves 12.043 → 11.424, 13.934 → 12.594 and
19.238 → 17.808 ms. Source primitive counts and both camera-height sequences
match across every timing block. Pool budget discards are zero. All 216
paired frame/pixel/draw-count checks pass on both the workstation and Pi;
equality is within each device, not across hardware. The kiosk remains idle
during the standalone probe: its geometry-frame and GL-draw totals do not
advance. Temperature checkpoints are 61.8/64.8°C.

### Source pool in the live application

The pooled producer completes the same natural match at tick 16,458. Its
settled result screenshot matches the preceding build byte for byte. Retained
source capacity peaks at 487,328 bytes (476 KiB), with zero budget discards and
no renderer errors. The first result loop still costs 35.611 ms CPU and has
51 shaping-run misses: this allocation change does not solve cold UI work.

The frozen comparison keeps scratch plan storage enabled for both source
policies. It alternates reference/pooled in ABBA/ABBA order for basic clocks,
then the reverse order for detailed clocks, within one process and frozen
match. Each policy has four 240-loop captures per detail level: 960 loops per
cell. Commands, settling intervals and screenshots are outside captures.

| CPU work, ms | Reference source | Pooled source |
| --- | ---: | ---: |
| Whole loop, basic clocks | 19.375 | 17.108 |
| Whole loop, detailed clocks | 20.932 | 19.081 |
| Source creation, detailed clocks | 2.683 | 1.964 |
| Source disposal or recycling, detailed clocks | 1.718 | 0.663 |
| Projection, detailed clocks | 2.917 | 3.230 |
| Direct scene draw, detailed clocks | 6.061 | 6.097 |
| Plan recycling, detailed clocks | 0.336 | 0.332 |

Basic whole-loop CPU falls 11.7%; every pooled basic block mean is below every
reference mean. Basic p95 falls 21.360 → 19.852 ms, maximum 23.970 → 21.854 ms.
Detailed source creation/cleanup together improve 40.3%, while projection is
slower in this run. Detailed whole-loop improvement is 8.8%, not the 11.7%
basic-clock result; the nested rows are attribution, not additive totals.
Individual drawing tails are not uniformly better. Absolute baselines differ
from the preceding build, so use these same-build comparisons for the effect.

All 16 comparison screenshots match exactly. Every captured loop has the same
87 GL draws, 12 buffer-upload calls, 69,760 buffer bytes, zero texture uploads
and zero shaping misses. Basic traces have no application/text clocks;
detailed traces have one call to each scene boundary per draw. RSS checkpoints
range from 83,924 to 83,984 KiB and settle at the latter value, rather than
growing with each policy switch. This short check is not a leak soak.
Temperature checkpoints range from 67.7 to 69.6°C.

Validation passes for 641 client, 28 CLI, 14 engine-common, four backend-startup
and three terrain-art tests: 690 total, with four existing diagnostics ignored.
Software-only compilation, formatting and diff checks pass; Clippy completes
with existing warnings. The terrain-edit test initially needed its first
simulation step before requesting a surface disruption; the corrected test
passes with actual removed-cell assertions.

Raw scripts, traces, images, analysis and build/test logs are under
`target/gpu-experiments/source-points.5Yu3SnEv/`; device artifacts remain in
`/tmp/spacewars-source-points.W4S2yv7Q/`. Only Picade 2 was updated, without
rebooting. The normal fast-update health check passed, original preferences
were restored exactly (settings SHA-256 starts `89a32bc36c24`), boot ID is
unchanged and restart count is zero. It is left on GPU Clock with the existing
geometry/stream/text experiments and scratch plans. The source policy is
`pooled`, but Clock does not opt in: its source pool reports unapplied and
zero storage, and application trace clocks are off. Production defaults and
other devices are unchanged.

### Preparing camera and object transforms once

`gpu-plan prepared` builds on bounded scratch storage. It computes the camera's
world bounds and their rounded width/height once per viewport, and sine/cosine
once per geometry hint. Every source point is still projected, every hint is
still compared against the authoritative world primitives, and malformed
geometry still takes the same rejection or ordinary-rendering path. No new
geometry results, source frames, or GPU resources are retained.

The camera calculation deliberately keeps the reference subtraction, division,
and viewport multiplication order. It does not substitute inverse multiplication
or a combined affine matrix: those can round differently at close zoom or large
world coordinates. Prepared values live only for the current projection, so
moving, resizing, and changing zoom cannot reuse an old camera transform.

```sh
# Compare in the same active GPU session; source allocation is independent.
spacewars-cli host gpu-plan scratch
spacewars-cli host gpu-plan prepared
```

The existing `--geometry-plan-profile` now compares four policies: reference,
reserved, scratch, and prepared. Four repeats balance order positions and
within-block predecessors. Each block cyclically replays the same 24 consecutive
production frames after one complete warmup cycle. `cpu-plan` includes plan
construction and disposal/recycling; `completed` also includes retained-culled
encoding, bucketed uploads, drawing, and `glFinish`. Neither includes source
generation, simulation, text/UI, scanout, or input latency. The separate
full-application comparison uses ordinary trace clocks for performance and
detailed clocks for attribution.

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-plan-profile --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 96 --presentation-repeats 4 \
  --geometry-output /tmp/spacewars-prepared-transforms
```

### Prepared transform measurements on Pi 4

The release comparison uses 96 timed frames per block, four balanced repeats,
and identical histories. Each cell contains 384 samples. This isolates prepared
transforms from scratch storage: both columns reuse the same bounded plan
allocations. Values are means in milliseconds, not capped FPS.

| Pi CPU or completed wall time | Early bots scratch → prepared | Late bots scratch → prepared | Changing zoom scratch → prepared |
| --- | ---: | ---: | ---: |
| Plan construction and recycling CPU | 0.857 → 0.752 | 1.450 → 1.284 | 1.612 → 1.431 |
| Completed geometry CPU | 5.406 → 5.261 | 8.864 → 8.679 | 10.211 → 9.990 |
| Completed geometry wall | 10.325 → 10.182 | 13.540 → 13.339 | 15.145 → 14.919 |
| Completed wall p95 | 10.759 → 10.652 | 14.935 → 14.670 | 16.399 → 16.086 |

Plan CPU falls 11.2–12.3%, but completed geometry wall time falls only 1.4–1.5%.
Every prepared block has a lower mean CPU cost than every scratch block for the
same fixture and timing boundary. This is a small repeatable saving, not removal
of the larger encoding/submission costs. The two precomputed transforms are
tested together; this comparison does not attribute their savings separately.
These histories and timing boundaries differ from the preceding source-pool
probe, so their absolute totals should not be compared directly.

Both policies retain exactly the same peak plan capacity: 583,224, 527,808, and
976,448 bytes respectively, with zero budget discards. Host and Pi each pass
252 exact pixel/draw-count comparisons against reference across 84 moving and
resized views, including abrupt 44/640 camera heights and scenario replacement.
The benchmark validates all 9,216 Pi timing rows for matched geometry and storage
bounds. Kiosk scene-frame and GL-draw totals stay unchanged during the offscreen
probe; temperature checkpoints are 62.3 and 66.2°C. The workstation comparison
uses a debug build and serves as correctness evidence, not a release speed claim.

### Prepared transforms in the full application

With pooled source points and prepared plans, the seed-0 Human/Legacy-v9 match
again ends at tick 16,458. Its stable result screenshot is byte-identical to
the preceding source-pool build. Plan storage peaks at 1,825,928 bytes and source
storage at 487,328 bytes, with zero discards. The first result loop costs
38.250 ms CPU, including 51 shaping-run misses; this single cold transition
still exceeds a 60 Hz frame budget and is not a paired optimization result.

On that frozen result, four 240-loop captures per policy use basic clocks, then
four per policy use detailed clocks in the opposite order. Source pooling,
geometry, upload, and text policies stay fixed throughout. Each table cell
contains 960 samples; detailed spans are nested attribution, not extra work to
add to the basic total.

| Pi CPU milliseconds | Scratch | Prepared |
| --- | ---: | ---: |
| Basic whole loop mean | 17.562 | 17.320 |
| Basic whole loop p95 | 20.369 | 20.083 |
| Basic timers mean | 6.536 | 6.330 |
| Basic draw mean | 10.845 | 10.810 |
| Detailed projection mean | 3.473 | 3.324 |
| Detailed direct drawing mean | 5.937 | 5.959 |

Basic whole-loop CPU improves 1.4% in this sample. Block means overlap slightly;
do not describe this as a large or universal frame-time improvement. Detailed
whole-loop means improve only 0.6% (19.382 → 19.257 ms), and detailed p95 is
slightly worse (21.413 → 21.476 ms). Direct drawing remains the larger CPU
stage, rather than being improved by preparing camera transforms. Absolute
baselines differ from preceding builds; only the same-build paired comparison
isolates this change.

All 16 comparison screenshots match exactly. Each of the 3,840 captured loops
has 87 GL draws, 12 buffer-upload calls, 69,760 buffer bytes, zero texture
uploads, and zero shaping misses. Basic traces have no optional application/text
clocks; detailed traces preserve the expected call counts and nesting. RSS
checkpoints fluctuate between 83,808 and 84,644 KiB; this short run is not a leak
soak. Temperatures range from 68.2 to 70.1°C.

Validation passes for 643 client, four backend-startup, 14 engine-common, and
28 CLI tests: 689 total, with four existing diagnostics ignored. New tests
compare exact point bits and errors across cancellation, subnormal, overflowing,
and non-finite inputs, and verify that altered points/styles still invalidate
hints. Existing scene tests now cover prepared storage through moving Clock,
Pizza, and Spacewars scenes, resizing, zoom changes, and failure cleanup.
Software-only compilation, formatting, and diff checks pass; Clippy reports
the existing warnings.

Scripts, traces, images, analysis, and build/test logs are under
`target/gpu-experiments/prepared-transforms.P5K8NKkV/`; device artifacts remain
under `/tmp/spacewars-prepared-transforms.gXjh3u1d/`. The installed client hash
starts `6dd2c618e096`, and the matching CLI starts `b57a1b3423f0`. Only Picade 2
was updated. Its boot ID and original settings checksum are unchanged, the
service is active with zero automatic restarts, and GPU/text errors are zero.
It is left on GPU Clock with prepared plans and the existing geometry/stream/text
experiments. Source pooling remains selected but is unapplied for Clock, with
zero retained source storage. Trace clocks are inactive. Production defaults
and other devices are unchanged.

## CPU mesh expansion experiment

Four-vertex chunked expansion is rejected: it preserves geometry but makes the
Pi's production renderer slower. The working renderer retains its scalar
iterator. The candidate, temporary live selector, and focused benchmark were
removed after measurement; their source and release binaries are archived with
the results for reproduction. No renderer default changes.

The initial CPU attribution run uses eight rotated repeats of 64 consecutive
production frames per fixture, after eight startup frames. With retained
culling, its changing-zoom replay costs 5.210 ms CPU without optional clocks.
Detailed clocks attribute 2.308 ms to guarded stroke tessellation, 1.000 ms to
scalable vertex expansion, 0.669 ms to ordinary geometry, and 0.450 ms to validity
scanning. These instrumented spans are attribution, not components to add to
the uninstrumented total. Stroke construction remains the larger opportunity.

### Kernel controls and compiler settings

The experiment compares scalar iterator append, a portable four-vertex loop,
copy-then-transform, and an explicit two-vertex NEON kernel. Each preserves
output order and checks identical finite vertex bits before timing. The
production candidate uses only the portable loop: no unsafe code, new GPU
format, extra retained memory, or relaxed stroke/culling checks.

The first standalone build uses `-C opt-level=3 -C target-cpu=cortex-a72`.
For a 40,003-vertex frame, portable chunking appears faster: 21.043 → 7.656 ns
per vertex. Tiny repeatedly reused arrays give a different ranking. Splitting
the frame into 64-vertex ranges still shows a smaller apparent gain:
20.811 → 17.179 ns per vertex. Those kernel gains do not predict renderer gains.

Rebuilding the same kernel with the application's `-C opt-level=3 -C lto=fat
-C codegen-units=1` and default AArch64 CPU target reverses the large-buffer
ranking: scalar 7.798 versus chunked 19.245 ns per vertex. Explicit NEON costs
9.732 ns there. Both builds use Rust 1.94.0. CPU target and LTO settings changed
together, so this does not isolate either compiler option's effect. It does
establish that the original microbenchmark configuration was not representative
of the shipped build. Use matched compiler settings as well as realistic
working sets before drawing conclusions from future kernel tests.

### Production replay comparison on Pi 4

The release application compares scalar retained culling with the candidate on
three prerecorded, moving Spacewars histories at 1024×768. Each history has
24 consecutive ticks, replayed cyclically, with one full cycle of warmup before
each block. Four balanced repeats measure 64 frames per control. Prepared plans
and bucketed uploads stay fixed. Simulation, source generation, projection,
text/UI, and scanout are excluded. Completed drawing includes `glFinish`.

| Pi milliseconds per frame | Early bots | Later bots | Changing zoom |
| --- | ---: | ---: | ---: |
| Scalar CPU encoding | 1.392 | 3.999 | 4.999 |
| Chunked CPU encoding | 1.652 | 4.201 | 5.142 |
| Scalar completed drawing wall | 9.478 | 11.803 | 13.392 |
| Chunked completed drawing wall | 9.727 | 11.837 | 13.584 |
| Scalar completed drawing CPU | 4.279 | 7.030 | 8.340 |
| Chunked completed drawing CPU | 4.586 | 7.115 | 8.531 |

CPU encoding regresses 18.7%, 5.1%, and 2.9%, respectively. Every chunked
encoding block is slower than every scalar block for the same fixture.
Completed wall time also fails to improve: +2.6%, +0.3%, and +1.4%. Detailed
early-replay clocks place the main regression in exact expansion
(0.580 → 0.784 ms); changing-zoom scalable expansion regresses
0.963 → 1.057 ms. A full-application performance comparison was not pursued
after these consistent regressions.

Cyclic replay can still cause cache admissions, especially in the later-bot
fixture. All 4,608 timing rows match vertices, indices, upload bytes, and
admissions at the same full replay position across controls. Comparing only
the source-frame index modulo 24 would incorrectly assume identical cache
history. Absolute times should not be compared with the preceding consecutive
64-frame attribution run or with earlier builds.

On both workstation and Pi, 84 view states pass exact CPU vertex-bit, index,
batch, and cache-decision checks, plus exact GPU pixel/draw/upload comparisons.
These cover moving frames, native/odd/native viewport changes, abrupt 44/640
camera heights, and scenario replacement. The candidate passed 691 automated
tests, including tail/prefix/arithmetic edge cases and culling/fallback checks.
After removing the candidate and its two added tests, all 689 existing tests
pass again, with four diagnostics ignored. Software-only compilation,
formatting, and diff checks pass. The experimental build's Clippy run reports
the existing warnings, with none in the new expansion code.
The workstation GPU run is a debug correctness check, not a release speed claim.
Pi temperature checkpoints during the paired replay are 65.7 and 66.7°C.

Artifacts are under `target/gpu-experiments/mesh-expansion.8qNlnN9j/`:
`device/baseline-encoding`, `device/expansion`, the three kernel runs,
`moving-analysis.json`, scripts, build/test logs, `rejected-source.tar.gz`, and
`rejected-binaries.tar.gz`. The archived application supports
`--geometry-expansion-profile` and `host gpu-scene retained-chunked`; the current
working source deliberately does not. Its normal encode profiler remains
available for the next investigation. Device artifacts remain under
`/tmp/spacewars-mesh-expansion.rqq7hPc7/`.

Picade 2 keeps the experimental binary (client `6b5245b0b28c`, CLI
`86678b1c1dac`), with the **scalar** `retained-culled` control selected, prepared
plans, bucketed uploads, and CPU-shaped text. It is on Clock with source pooling
selected but unapplied. Original preferences and boot ID are unchanged, the
service is active with zero automatic restarts, and GPU/text errors are zero.
Other devices are unchanged. The guarded stroke follow-up below separates path
preparation from tessellation/output costs without weakening the zoom safety
guard or adding a per-frame dictionary for unique outlines.

## Guarded stroke attribution

Guarded stroke cost is mostly triangulation and geometry output, not preparation
of the input points. A new diagnostic breakdown leaves the production path and
its zoom guards unchanged. It measures preparation, the existing Lyon call,
and final budget checks without adding per-vertex clocks. After tessellation,
an optional visibility scan counts strokes whose generated vertices all lie
beyond one clip edge. This scan does **not** remove geometry or establish a safe
bound for rejecting a stroke before tessellation.

### Production replay measurements on Pi 4

The release application repeats the three moving Spacewars histories at
1024×768, with eight rotated blocks of 64 consecutive measured frames after
eight startup frames. Source creation, projection, simulation, GPU work, and
presentation are outside the encoding timer. The retained culling policy uses
the scalar expansion path throughout.

| Changing zoom measurement | Milliseconds per frame |
| --- | ---: |
| Normal encoding CPU without optional clocks | 5.209 |
| Encoding CPU with detailed clocks and visibility inspection | 6.025 |
| Guarded stroke parent span | 2.692 |
| Input preparation within that span | 0.174 |
| Lyon tessellation and mesh output within that span | 2.177 |
| Final checks and counters within that span | 0.058 |
| Separate post-tessellation visibility inspection | 0.078 |
| Parent time belonging to wholly offscreen strokes | 1.094 |

Instrumentation adds 15.7% to the zoom encoding total in this run. Nested spans
and the offscreen subset must not be added to the uninstrumented total, nor
treated as exact projected savings. The parent also includes timing/counter
overhead not assigned to a child span. Parent and child spans use elapsed
clocks; the encoding CPU totals use the thread CPU counter.

Each measured zoom frame averages 948.53 guarded strokes, 3,920 input points,
13,492 output vertices, and 33,338 indices. None is empty. Of those strokes,
390.95—41.2%—produce geometry wholly outside the clip. The corresponding
parent time is 40.6% of guarded stroke time. This is useful evidence for
investigating conservative early rejection, not permission to bypass the
guard with source bounds expanded by an assumed stroke radius.

The early-bot replay has no guarded strokes; the later-bot replay averages ten,
of which three are offscreen. Normal culled encoding averages 1.604 and 1.704 ms
there. All 96 profiled/unprofiled block pairs preserve output counts, upload
bytes, admissions, cache entries, and retained capacity. Temperature checkpoints
are 62.3 and 67.7°C.

### Isolated stroke output controls

A separate kernel compares Lyon's stock output writer, a writer that appends
three indices together and premultiplies color once per draw, and Lyon's
`NoOutput` sink. It uses the same pinned Lyon build, Rust 1.94.0, fat LTO,
one codegen unit, and default AArch64 CPU target as the application. Each
synthetic fixture contains 960 polygons with varied transforms and colors;
twelve rotated blocks measure 32 frames after four warmup frames.

| Synthetic stroke shape | Stock µs per draw | Bulk writer µs per draw | No output µs per draw |
| --- | ---: | ---: | ---: |
| Short edges and wide stroke | 2.627 | 2.483 | 2.075 |
| Concave | 2.763 | 2.618 | 2.210 |
| Sharp turn | 1.828 | 1.699 | 1.429 |
| Many turns | 14.220 | 13.508 | 11.095 |

The bulk writer preserves exact vertex bits and indices in all four fixtures
and reduces kernel time 5.0–7.1%. Even eliminating all output saves only
20.0–22.0% here. `NoOutput` also skips position/color materialization and changes
working sets/compiler opportunities; the difference is not a precise memory
store cost. These synthetic kernel results alone did not establish a renderer
improvement; the same-build moving comparison below measures that separately.
Avoiding invisible
tessellation still offers a larger potential saving, subject to conservative
bounds and edge/zoom regression checks.

### Validation and artifacts

All 691 client/common/CLI/backend-startup tests pass, with four existing manual
diagnostics ignored. Two new tests check exact output bits, prefix accounting,
empty paths, disabled counters, all clip edges, and thick strokes that overlap
an edge despite an offscreen outline. The encoding probe now also compares
culled plain/profiled meshes, bringing its startup/stable/zoom checks to 7,560,
passing on both workstation and Pi.
Software-only compilation, formatting, and diff checks pass; Clippy reports the
existing warnings. Workstation timings are debug correctness diagnostics.

Raw results, scripts, the isolated kernel, and `analysis.json` are under
`target/gpu-experiments/stroke-attribution.VylSCOhD/`; device artifacts are under
`/tmp/spacewars-stroke-attribution.oinykeD2/`. Only Picade 2 is used. Both timed
probe runs verify unchanged application binaries, boot ID, and restored original
preferences. No rendering policy or quality default changes.

The final installed client starts `739f39432b3e`, with CLI `b57a1b3423f0`.
Picade 2 is left on GPU Clock with retained culling, bucketed uploads, prepared
plans, and CPU-shaped text. The service is active with zero automatic restarts
and no GPU/text errors. Its original settings checksum and boot ID are unchanged.

## Stroke output writer in moving scenes

The bulk writer produces a small, repeatable saving in the stroke-heavy zoom
replay on Picade 2. Ordinary gameplay is effectively unchanged. Keep it as a
benchmark control; it is not selected by normal rendering and does not yet
justify a new live policy or a default change.

Only uncached stroke output changes: premultiply color once per draw and append
each triangle's three indices together. Lyon still generates the same stroke
positions and triangle order. Fill tessellation, cached template construction,
zoom safety guards, clipping, validation, and upload policy remain unchanged.
The writer preserves the reserved invalid-index sentinel and restores existing
buffer prefixes when geometry is aborted.

### Matched release comparison on Pi 4

`--benchmark-presentation --presentation-geometry --geometry-stroke-profile`
compares both writers in one release binary. At 1024×768, each fixture replays
24 consecutive production ticks cyclically, with 24 warmup frames followed by
eight blocks of 128 measured frames per control. CPU-only plain/profiled blocks
use a four-position counterbalanced order. Completed drawing alternates writer
order and includes GL submission and `glFinish`, without the optional per-piece
clocks. Both use retained culling, prepared plans, and bucketed streams.

| Moving fixture | Stock encoding CPU ms | Bulk encoding CPU ms | Stock completed wall ms | Bulk completed wall ms |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 1.400 | 1.397 | 9.527 | 9.510 |
| Later bots | 4.294 | 4.275 | 12.116 | 12.107 |
| Close-up zoom | 5.047 | 4.923 | 13.461 | 13.349 |

Zoom encoding CPU improves 2.47%, or 0.125 ms per frame. All eight block pairs
improve, by 2.06–3.18%. Completed wall time improves 0.83%, or 0.112 ms; all
eight pairs improve there too, by 0.34–1.31%. Completed drawing CPU falls
8.363 → 8.248 ms (1.38%). Zoom encoding p95 falls 5.140 → 5.004 ms; completed
wall p95 moves only 14.492 → 14.460 ms.

Early/later completed means differ by less than 0.2%, with mixed block
directions. Do not claim a general gameplay speedup from those results.
The detailed zoom runs place 2.110 → 2.023 ms in the nested Lyon/output span;
those instrumented elapsed clocks are attribution, not an additive component
of the uninstrumented CPU total. Simulation, source construction, projection,
text/UI, and display scanout are outside this comparison. It does not establish
a whole-application FPS improvement. Absolute times are not directly comparable
with the preceding 64-consecutive-frame attribution experiment.

### Validation and disposition

All 18,432 measurement rows form 9,216 stock/bulk pairs with identical vertex
counts, index counts, upload bytes, and admissions at the same full replay
position. Pairing by the source frame modulo 24 would lose cache-history
information. On both workstation and Pi, all 84 view states preserve exact
vertex bits, indices, batches, cache decisions, GPU pixels, draw counts, and
upload bytes. They include moving cameras, native/odd/native viewport changes,
abrupt 44/640 camera heights, and scenario replacement. The three exported Pi
images were also visually checked; text is intentionally omitted by this
geometry-only probe. Temperature checkpoints are 62.3 and 65.7°C.

All 695 client/common/CLI/backend-startup tests pass, with four existing manual
diagnostics ignored. New writer tests cover shape/style variations, repeated
geometry and prefixes, invalid colors, buffer budgets, index limits, and abort
rollback. The budget test also passes with fill disabled so that the stroke
writer itself reaches both limits. Software-only compilation, formatting, and
diff checks pass. Clippy reports the existing warnings, with none in the new
writer or probe. Workstation GPU runs are debug correctness checks.

The control and its reproducible probe stay available for future comparisons.
The small saving is useful evidence; the following early-rejection experiment
investigates the larger opportunity in invisible guarded strokes. Preserve the
stroke safety guard and test any proposed bound against the full tessellated
output before using it to discard work. A live comparison is required before
promoting the writer beyond the benchmark.

Results, scripts, source/binary archives, images, and build/test logs are under
`target/gpu-experiments/stroke-writer.nL8SNKmE/`; `analysis.json` contains block
means and paired differences. Device artifacts remain under
`/tmp/spacewars-stroke-writer.RTdFKVV8/`. The installed client starts
`ba2705f9a46f`, and the CLI remains `b57a1b3423f0`. Picade 2 is left on GPU Clock
using the stock writer, retained culling, bucketed streams, prepared plans,
CPU-shaped text, and source pooling selected but unapplied for Clock.
Original preferences and boot ID are unchanged. The service is active with zero
automatic restarts and no GPU/text errors. Other devices are unchanged.

## Early rejection of guarded strokes

Conservatively skipping invisible guarded strokes is a larger win than changing
their output writer. In the close-up Spacewars replay on Picade 2, encoding CPU
falls 17.7% and completed drawing wall time falls 7.9%. All eight paired blocks
improve. Early/later gameplay is effectively unchanged. This first comparison
is benchmark-only; the [live follow-up](#live-guarded-stroke-control) below adds
an opt-in control. Normal rendering still defaults to tessellating these strokes.

### Restricted bound and independent checks

The new bound applies only after the existing zoom guard has required ordinary
stroke tessellation. It does not relax that guard or change fills, cached mesh
culling, cache admissions, retained storage, or painter order. Both controls
use the stock output writer, isolating rejection from the preceding experiment.

An outline expanded by half the stroke width is not the bound used here. The
pinned Lyon 1.0.20 fixed-width implementation also constructs inner joins and
folds sharp joins that extend beyond both adjacent edges. The experimental
envelope pads the outline by `4 × (L1 diameter + stroke width) + 2` pixels.
It rejects only beyond an additional one-pixel clip margin.
It accepts closed straight-edge polygons with 3–256 points, widths of
0.001–256 pixels, finite coordinates within ±16,384 pixels, and valid colors.
Anything outside that domain, or without a valid clip, keeps the original path.
The bound allocates no memory and retains no history.

This is a deliberately loose, implementation-aware bound, not a universal
stroke theorem. Lyon, stroke-option, or domain changes require review. More
than 13,000 deterministic shape cases check actual Lyon vertices, including
degenerate/repeated edges, narrow turns, self-intersections, point merging,
rotation, small scales, and large translations. A separate checked control
tessellates every proposed rejection and verifies its actual output lies
outside the clip before discarding it. Tests also cover thick edge-overlapping
strokes, invalid/extreme inputs, existing buffer prefixes, missing clips, and
reappearance when the clip moves.

### Matched release comparison on Pi 4

Add `--geometry-stroke-rejection` to the existing stroke-profile probe:

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-stroke-rejection \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 128 --presentation-repeats 8
```

Each fixture replays 24 consecutive production ticks cyclically. Each block
starts with 24 warmup frames; plain/profiled CPU blocks are counterbalanced and
completed-drawing blocks alternate control order. Retained culling, prepared
plans, bucketed streams, and the stock writer are identical between controls.
The completed path includes submission and `glFinish`, not display scanout.

| Moving fixture | Stock encoding CPU ms | Reject encoding CPU ms | Stock completed wall ms | Reject completed wall ms |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 1.427 | 1.424 | 9.440 | 9.451 |
| Later bots | 4.355 | 4.353 | 12.087 | 12.077 |
| Close-up zoom | 5.110 | 4.207 | 13.459 | 12.396 |

Zoom saves 0.903 ms encoding CPU (17.67%, with block improvements of
17.17–18.06%) and 1.063 ms completed wall time (7.90%, with block improvements
of 7.06–8.70%). Completed drawing CPU falls 8.434 → 7.357 ms (12.77%).
Encoding CPU p95 falls 5.203 → 4.294 ms; completed wall p95 falls
14.404 → 13.176 ms. These use the plain paths, without per-piece timing clocks.
The profiled paths are retained for attribution, not used to claim the saving.

The zoom replay tests 916.44 guarded strokes per frame and rejects 372.30
(40.62%). Mean vertices fall 59,380.52 → 54,085.34, and upload payload falls
1,985,138 → 1,805,706 bytes (9.04%). No tested bounds are unknown in these
fixtures. Early gameplay has no eligible strokes; the warmed later replay
tests only 0.078 and rejects 0.023 strokes per frame. Its essentially unchanged
timing is therefore expected, not evidence of broad gameplay acceleration.
Simulation, source construction, projection, text/UI, and scanout are outside
this comparison. This does not establish a whole-application FPS improvement.

### Validation and next step

All 18,432 rows form 9,216 stock/reject pairs at the same full replay position.
Admissions match, geometry/upload counts never increase, and counters/counts
match across plain, profiled, and completed paths. On both workstation and Pi,
all 84 moving/resize/zoom/replacement states preserve exact GPU pixels. The
checked CPU control agrees exactly with the fast control, and the reference
comparison verifies that every omitted triangle is offscreen while retained
triangle bits and painter order are unchanged. Native/odd/native viewports and
44/640 camera heights exercise clip changes. The three exported Pi images were
visually inspected; text is intentionally absent from this geometry probe.
The previous stock/bulk writer probe also passes its 84-state regression check.

All 699 client/common/CLI/backend-startup tests pass, with four existing manual
diagnostics ignored. Software-only compilation, formatting, and diff checks
pass; Clippy reports existing warnings. Workstation GPU runs are debug
correctness checks, not release timing claims. Pi temperature checkpoints are
62.8 and 65.7°C.

This result motivated the live opt-in comparison below, including full matches,
changing zoom, and session transitions. That follow-up preserves the original
path, exposes tested/unknown/rejected counters, and checks paused captures.
Neither slice changes the default.

Results, analyzer, scripts, source/binary archives, screenshots, and build/test
logs are under `target/gpu-experiments/stroke-rejection.1l2qWHzF/`; paired results
are in `analysis.json`. Device artifacts remain under
`/tmp/spacewars-stroke-rejection.5Y1p9ksO/`. The installed client starts
`5c07235bce84`, and the CLI remains `b57a1b3423f0`. Picade 2 is left on GPU Clock
with retained culling, bucketed streams, prepared plans, CPU-shaped text, and
source pooling selected but unapplied for Clock. Early stroke rejection is off.
Original preferences and boot ID are unchanged; the service is active with
zero automatic restarts and no GPU/text errors. Other devices are unchanged.

## Live guarded stroke control

`host gpu-strokes` makes early stroke rejection independently selectable in a
running GPU scene. The default is `off`. Non-off modes require
`gpu-scene retained-culled`; other renderers reject the request instead of
silently ignoring it. This selects only the bound described above, not the
bulk output writer.

```sh
spacewars-cli host gpu-scene retained-culled
spacewars-cli host gpu-stream bucketed
spacewars-cli host gpu-plan prepared
spacewars-cli host gpu-source pooled
spacewars-cli host gpu-strokes conservative
# Correctness diagnostic: tessellate and verify every proposed rejection.
spacewars-cli host gpu-strokes checked
# Restore ordinary guarded stroke tessellation without cooling the cache.
spacewars-cli host gpu-strokes off
```

Changing only stroke mode preserves the pending plan, retained geometry cache,
source pool, upload policy, and zoom selection. It clears the measurement window
so samples do not mix modes. Repeating the same request is idempotent. The
requested mode changes immediately, but `gpu_scene_stroke_applied` changes only
after a successful drawing callback. Renderer failure retains the last image
until a raster replacement is available; it does not report the new request
as successfully applied. Incompatible geometry modes, session end, and graphics
teardown reset rejection to off. Nothing is saved in user settings.

Status reports `gpu_scene_stroke_policy`, `gpu_scene_stroke_applied`, and
`gpu_scene_stroke_bound_tests_avg`, `gpu_scene_stroke_bound_unknown_avg`, and
`gpu_scene_stroke_bound_rejections_avg`. The averages cover the published window
of 120 geometry frames, not simulation ticks. Unknown bounds use ordinary
tessellation. Zero means no work qualified during that window; `unavailable`
means there is no complete measurement window. Checked mode intentionally
retains tessellation work and is a correctness tool, not a performance mode.

### Live changing zoom measurements

The Picade 2 release comparison uses the saved seed-0 human/rule-bot Spacewars
match with asteroids disabled, native 1024×768 output, and the usual 60 Hz
target. Both modes use retained culling, prepared plans, pooled source points,
bucketed uploads, CPU-shaped text, and the stock stroke writer. The performance
overlay is hidden in both. Four off/conservative pairs balance first/second
order. Each block clears geometry resources, restarts the match, warms for one
480-tick camera sweep, then measures three sweeps, approximately 24 seconds.

| Live metric | Off | Conservative | Change |
| --- | ---: | ---: | ---: |
| Geometry encoding wall ms | 5.061 | 4.341 | −14.2% |
| Drawing CPU ms | 9.668 | 9.089 | −6.0% |
| Whole-loop CPU ms | 14.939 | 14.345 | −4.0% |
| Process CPU, percent of one core | 85.16 | 83.39 | −2.1% relative |
| Rendered frames/s | 55.69 | 56.80 | +2.0% |
| Geometry upload bytes/frame | 1,483,688 | 1,304,651 | −12.1% |

All four paired blocks improve encoding time, drawing/loop CPU, process CPU,
and FPS. Drawing CPU improvements range from 5.25–6.62%; FPS improvements range
from 1.26–2.67%. Simulation remains approximately 60 updates/s. The live saving
is smaller than the isolated completed-drawing result because the app still
generates/projects scenes, simulates the match, and draws text/UI. It does not
yet maintain 60 rendered FPS throughout this sweep.

The conservative mode tests a mean 871.75 strokes and rejects 369.88 per
geometry frame, about 42.4%, with zero unknown bounds in the sampled windows.
These are natural-paced runs: simulation intervals are aligned, but different
rendering rates produce different frame/cache histories. Do not interpret the
live windows as identical per-frame geometry, or add nested wall and CPU spans.
Per-block process CPU includes all process threads; renderer CPU spans measure
the UI thread. Sampled temperatures span 65.2–69.6°C, with no asserted power
alarm. Renderer cache/scratch/source allocations remain within their caps.

Three frozen gameplay states preserve exact screenshot bytes across off,
conservative, and checked modes, including text, minimaps, and the pause menu.
Those stable cameras warm into the exact cache and report zero guarded-stroke
tests by capture time. They validate live switching/composition, not active
rejection by themselves; the moving probe's checked geometry and pixel tests
remain the independent rejection comparison.

### Live correctness and lifetime checks

The additional moving `checked` run exercises actual early rejections: all 30
sampled windows have nonzero rejected counts (76.78–898.64 per geometry frame),
with zero unknown bounds and no geometry or text errors. It spans match ticks
182–3,762 while sweeping both cameras between heights 44 and 640. Unlike the
paused captures, this checks the emitted triangles for every proposed rejection
during live changing zoom. The sampled 120-frame windows can overlap; these
figures are rates, not a count of unique checked strokes.

Both natural-camera full matches, one in checked mode and one in conservative
mode, finish at tick 16,458 with Player 2 winning. Result-screen presentation,
rematch, incompatible geometry-mode selection, and return to launcher pass.
The cache peaks at 13,874,312 bytes and 128 entries, plan scratch at 1,827,128
bytes, and source storage at 487,328 bytes, within their respective limits.
Observed stream capacity peaks at 6,346,864 bytes. Launcher exit releases scene
targets/streams and resets requested/applied stroke state to off/inactive.
Incompatible modes reject non-off requests instead of appearing to apply them.

All 704 client/common/CLI/backend-startup tests pass, with four existing manual
diagnostics ignored. Coverage includes parsing, incompatible requests, warm-cache
preservation, idempotence, requested versus applied state, measurement reset,
session end, failure, and graphics teardown. The native 84-state rejection
pixel/geometry probe and existing layout/retirement probe pass. Software-only
compilation, formatting, and diff checks pass; Clippy reports existing warnings.
Workstation GPU runs remain debug correctness checks, not performance evidence.

Keep rejection opt-in. This is a measured live zoom improvement, not a universal
gameplay gain or evidence for enabling every experimental policy together. The
original path and the checked diagnostic remain available; the bulk output writer
is still benchmark-only. Further promotion needs broader scene/Clock-effect and
device coverage, including the still-unvalidated Pi 5 GPU path.

Results, analyzer, scripts, source/binary archives, screenshots, and build/test
logs are under `target/gpu-experiments/live-strokes.Y3Ql7xla/`. `analysis.json`
contains the paired zoom measurements and full-match resource peaks; `device/`
and `checked-device/` retain the raw live statuses. Device artifacts remain under
`/tmp/spacewars-live-strokes.1vGUrOZD/`. The installed client starts
`0207b1a3526f`, and the CLI starts `f22dcf1fba84`. Picade 2 is left on GPU Clock
with retained culling, bucketed streams, prepared plans, CPU-shaped text, source
pooling selected but unapplied for Clock, and stroke rejection off. Original
preferences and boot ID are unchanged. The service is active with zero automatic
restarts and no GPU/text errors. Other devices are unchanged.

## Rounded borders and fresh KMS screenshots

Two independent correctness fixes pass on `sw-picade-2`, using the existing
Pi 4 V3D 4.2.14.0 / Mesa 25.1.6 image at 1024×768. No OS or boot changes are
needed. Rendering policies remain experimental; software is still the default.

### Rounded borders

A private 640×480 EGL fixture reproduces the missing corners without the game
renderer: four radii, three widths, direct FemtoVG with/without stencil strokes,
and Slint rectangles. The old Pi build fails eight of these 36 cases. Every
one-pixel stencil border loses all four corner arcs; the non-stencil control
does not. The workstation passes, so this is not merely a tessellation or
missing-stencil-attachment problem.

This matches Mesa's V3D early-depth/stencil regression, whose correction is
listed in the [Mesa 25.2.3 release notes](https://docs.mesa3d.org/relnotes/25.2.3.html).
The vendored FemtoVG workaround forces late fragment tests by writing unchanged
fragment depth through `GL_EXT_frag_depth`. It is restricted to V3D renderer
strings with Mesa versions from 24.2.7 through releases below 25.2.3. Other
drivers and newer Mesa keep the original shader. Stencil overlap suppression
is preserved; disabling it would make translucent self-crossings blend twice.
Updating Mesa is the longer-term replacement for this compatibility path.

The expanded fixture passes all 46 cases on Pi and workstation: thin/thick
borders, fractional coordinates, clipping, translucent borders and crossing
strokes. The crossing control produces intensity 192 without stencil versus
128 with stencil, matching the expected double/single blend. On Pi, all twelve
non-stencil reference images remain byte-identical; stencil-image differences
are confined to border coverage. The original 36 workstation images are
unchanged by the gated workaround. These are pixel correctness checks, not
workstation performance evidence.

```sh
engine-client --benchmark-presentation --presentation-gpu-ui \
  --gpu-ui-output /tmp/gpu-ui-new-directory
```

This opt-in probe requires the GPU build, creates a new output directory, never
owns scanout or changes settings, and fails if its pixel assertions fail.

A counterbalanced old/new/new/old offscreen text check uses the same 1024×768
fixture and four repetitions per process, excluding each process's first
repetition. Each policy/build has six cold-canvas and 90 warm samples:

| Text policy and stage | Draw CPU before → after | Completed wall before → after |
| --- | --- | --- |
| CPU-shaped, cold | 5.439 → 5.352 ms | 7.384 → 7.309 ms |
| CPU-shaped, warm | 1.298 → 1.321 ms | 3.278 → 3.311 ms |
| Vector, cold | 92.167 → 92.019 ms | 94.767 → 94.685 ms |
| Vector, warm | 1.436 → 1.425 ms | 3.431 → 3.416 ms |

These small differences show no large regression in this text fixture; they
are not a general throughput claim. Completed wall time includes GPU completion,
but is not an isolated GPU timer. The direct triangle renderer's shaders are
unchanged.

### Screenshot freshness

The previous IPC path requested a redraw, waited 50 ms, then read the current
back buffer after EGL had already swapped it. An idle menu could therefore
return a previous screen. The old-binary regression reproduces this immediately:
the first App Settings screenshot contains the launcher, while the second
contains App Settings.

LinuxKMS/FemtoVG now accepts one pending readback callback and runs it after
Slint drawing and the application rendering notifier, immediately before swap.
It performs no readback or pixel retention on ordinary frames. Requests have
a five-second deadline; concurrent requests receive a busy error rather than
replacing the pending request. Context destruction reports an error, and old
timeout tokens cannot cancel newer requests. PNG encoding and writing run on
the event loop after presentation, outside the rendering callback's GL borrows.

`tools/check-gpu-screenshots.sh` exercises the real kiosk buffers. Run it on the
device with the launcher open and auto-start temporarily Off; restore that
preference afterward. The script changes no saved settings and restores the
previous session-only glyph policy. It checks 20 immediate menu transitions
across vector and CPU-shaped text, comparing the first capture to two subsequent
captures. The final Pi build passes all comparisons, invalid-path recovery and
concurrent requests: one capture succeeds and three report busy. The same
transition check fails on the old binary. The freshness guarantee is specific
to LinuxKMS/FemtoVG; desktop GPU backends retain the prior capture path.

Client/common/CLI/backend-startup tests pass, including the new probe parser
test (705 in total, four existing manual diagnostics ignored). The KMS backend's
28 tests include pre-swap ordering with two/three buffers, busy/cancellation,
expiration and destroyed-context cases. The driver-version test, software-only
compilation, formatting, shell checks and diff checks pass. Clippy reports
existing warnings, with none in the new capture/probe/workaround code.

The installed build also passes a software-renderer capture/fallback check,
a paused split-screen Spacewars capture, and the direct-geometry heavy-rain
Clock smoke check at 60 FPS / 60 updates per second. The rain capture includes
both the live scene and its Slint overlay. These are smoke checks, not a new
full-match or broad Clock-event performance survey.

Artifacts, old/new binaries, fixture PNGs, real-buffer scripts and build/test
logs are under `target/gpu-experiments/ui-correctness.F6vrnPyE/` locally and
`/tmp/spacewars-ui-correctness.B4xuKbqm/` on the Pi. The final client SHA starts
`bd023edaafae`; the CLI remains `f22dcf1fba84`.
Picade 2 is left running normal demo Clock in GPU mode, with retained culling,
bucketed streams, prepared plans, CPU-shaped text and stroke rejection off.
Source pooling is selected but does not apply to Clock. The original saved
preferences match byte-for-byte, the boot ID is unchanged, and the service has
zero automatic restarts and no GPU/text errors. Other devices are unchanged.

## Pi 5 HyperPixel deployment and rotation blocker

This records the initial October 6 investigation. The rotation failures are
resolved in the October 8 follow-up below; the cropped direct-rendering samples
from this initial run remain invalid.

The October 6 follow-up installs the current experimental GPU image on
`spacewars.local`, a Pi 5 / HyperPixel with an 800×480 logical window on a
480×800 panel, `DPI-1`, and `SLINT_KMS_ROTATION=90`. Its software-only rootfs
needed the normal A/B update to install Mesa and the renderer-switch helper;
an app-only update would not have been compatible. The update changes root
slot b to a, leaving the previous slot intact. Boot identity and the saved
display/hostname configuration are unchanged. The installed client and CLI
match the Pi 4 validation binaries (`bd023edaafae` and `f22dcf1fba84`).

GPU startup succeeds on the separate RP1 DPI / V3D device combination:
`Broadcom / V3D 7.1.10.2 / OpenGL ES 3.1 Mesa 25.1.6`, with eight stencil bits.
This is hardware GL, not a software Mesa fallback. Software startup, explicit
GPU selection, and returning to software all pass.

The 46-case offscreen border/alpha/clipping fixture passes, as do all 20
immediate real-buffer menu transitions, write-error recovery, and bounded
concurrent screenshots (one success, three busy responses). These menu checks
run **before** enabling the direct scene rendering callback. Plain raster-scene
GPU presentation also captures a complete idle Clock, heavy rain, and a
split-screen Spacewars scene with its HUD/minimaps and pause menu.

Two rotation problems prevent declaring the direct renderer ready here:

1. Even without the scene callback, GPU screenshots are in physical, rotated
   480×800 coordinates; software screenshots are upright 800×480. The screenshot
   API needs a consistent logical orientation.
2. Enabling `gpu-scene retained-culled` changes subsequent screenshots to
   800×480 **without undoing the rendered rotation**, cropping Clock and menu
   contents and leaving out-of-bounds readback white. Disabling `gpu-scene`
   does not cure it: the installed rendering notifier persists until restart.
   A fresh GPU process without that notifier restores the complete captures.
   Inspection of pinned Slint 1.13.1's
   `internal_render_with_post_callback` identifies a physical/logical mismatch:
   it first sizes the canvas to the physical surface, then resets it to the
   logical window dimensions in the `BeforeRendering` notifier branch, before
   drawing the rotated UI. This is not a terrain, water, or retained-cache issue.

Next correctness work should keep physical canvas dimensions through the
notifier and normalize screenshot orientation. Add a non-square GPU fixture
covering 0/90/180/270 degrees, both with and without a rendering notifier,
including scene→menu and enable→disable transitions. The existing software
snapshot and inverse-touch-coordinate tests pass (one client integration test
and all 28 KMS backend tests), but do not cover this GPU combination. Physical
touch alignment still needs a user check.

Same-build smoke measurements below use the original RGB upload path, not the
aligned RGBA optimization measured earlier on Pi 4. They use deterministic idle and heavy-rain
Clock fixtures at native 1×, no performance overlay, and a fresh process for
each renderer. Each row samples process CPU over about 12.3 seconds after
warmup, with six two-second rolling diagnostic snapshots. Heavy rain starts
sampling at raining phase tick 300. CPU 100% means one core; drawing CPU is not
GPU execution time. These are short sequential checks, not counterbalanced
throughput benchmarks or proof of a small regression.

| Clock fixture / output | Process CPU | Scene prepare | Drawing CPU | FPS / UPS |
| --- | ---: | ---: | ---: | ---: |
| Idle / software | 27.2% | 2.084 ms | 2.145 ms | 60.0 / 60.0 |
| Idle / raster + GPU | 28.9% | 2.196 ms | 2.430 ms | 60.1 / 60.1 |
| Heavy rain / software | 34.0% | 2.823 ms | 2.048 ms | 60.0 / 60.0 |
| Heavy rain / raster + GPU | 34.9% | 2.849 ms | 2.404 ms | 60.1 / 60.1 |

Raster→GPU presentation alone shows no CPU win in these checks. GPU drawing
wall time includes refresh synchronization and must not be read as GPU work.
**Discard the direct-geometry performance samples**: their cropped output is
not an equivalent workload, regardless of low CPU numbers or 60 FPS.

Artifacts are under `target/gpu-experiments/pi5-validation.v0dCAptK/` locally
and `/data/spacewars/gpu-validation.eviXhMIP/` on the Pi, with immediate-menu
captures also in `/tmp/spacewars-freshness.mXO3wqri/`. The device is returned to
normal Clock with software rendering and the exact original settings, including
volume, mute state, auto-start Off and overlay Off. No rendering default or
production source is changed by this device investigation. The GPU runtime
remains installed for the rotation follow-up; other devices are untouched.

## Pi 5 rotation fix and matched Clock comparison

The October 8 follow-up vendors the pinned `i-slint-renderer-femtovg` 1.13.1
crate, with its original source/manifests/licenses and a small isolated patch.
The notifier branch now keeps the canvas at the **physical surface size**;
layout and the application's scene texture remain in logical coordinates.
Screenshots undo recognized cardinal output rotations only when readback is
requested. Normal drawing gains no framebuffer copy or readback, and unrotated
snapshots reuse their allocation. Unknown transforms retain the old physical
behavior. KMS still schedules the readback before swap; these changes do not
extend that freshness guarantee to other backends.

A new non-square GPU fixture samples the physical framebuffer independently
from snapshots at 0/90/180/270 degrees. It covers no notifier, a no-op notifier,
and repeated scene/menu transitions with the notifier left installed. The old
renderer fails 15 of these 20 cases; the patch passes all 20 on workstation and
Pi 5. All 46 existing border/clipping/alpha cases also pass. Their workstation
PNGs are byte-for-byte unchanged by the rotation patch. These are correctness
checks, not offscreen throughput measurements.

The fast update installs client `e9cb1e2614d3` and the unchanged CLI
`f22dcf1fba84` on `spacewars.local`, with no reboot or rootfs changes. Real KMS
captures are now upright 800×480. The screenshot harness passes 20 immediate
menu transitions before direct rendering and another 20 after enabling and
disabling it, while the notifier remains installed. Both runs cover vector and
CPU-shaped text, invalid-path recovery and concurrent requests (one success,
three busy responses). Complete idle Clock and heavy-rain captures have no
missing edges or rotated/clipped content. To repeat the dimension check:

```sh
SPACEWARS_EXPECT_SCREENSHOT_SIZE=800x480 bash tools/check-gpu-screenshots.sh
```

Run it on the kiosk with GPU output, the launcher open and auto-start Off. It
does not change saved settings and restores its temporary text policy.

Unit/integration validation passes 750 tests: 704 client/common/CLI/renderer
tests, 41 standalone KMS backend tests, four backend-startup cases and the
software rotated-snapshot case. Five existing manual diagnostics are ignored.
Software-only compilation, formatting, shell checks and diff checks pass. The
66-case GPU probe is opt-in; ordinary CI only adds the three pure rotation tests.

### Matched native-resolution measurements

The comparison runs software → aligned RGBA → triangles → triangles → aligned
RGBA → software, restarting the app for each block. Each block runs both
deterministic 08:08 Clock fixtures: idle and heavy rain. The native viewport is
800×480 at 1×, the overlay is Off, and both GPU paths select CPU-shaped text.
The RGBA path uses ordinary Slint texture lifetime, not persistent textures;
triangles uses the reference plan/shared stream, without retained geometry or
the later allocation controls. Saved settings remain byte-identical throughout
the benchmark.

Each fixture warms for five seconds (rain waits for raining phase tick 300),
then samples process CPU over approximately 12.3 seconds, with six rolling
diagnostic snapshots two seconds apart. Screenshots happen after each interval.
Process CPU is the change in `/proc/PID/stat` user+system ticks divided by wall
time and the device's `AT_CLKTCK`; 100% means one core. Values below combine the
two blocks per path/fixture. The governor is unchanged; endpoint temperatures
are 45.8–49.1°C and CPU frequencies 1.6–2.2 GHz. These are native-paced CPU
headroom measurements, not maximum rendering throughput or isolated GPU timers.

| Clock fixture / output | Process CPU | Scene prepare wall | Drawing CPU | FPS / UPS |
| --- | ---: | ---: | ---: | ---: |
| Idle / software | 27.2% | 2.073 ms | 2.162 ms | 60.1 / 60.1 |
| Idle / aligned RGBA + GPU | 22.0% | 2.661 ms | 0.811 ms | 60.1 / 60.1 |
| Idle / direct triangles | 7.2% | 0.127 ms | 0.925 ms | 60.1 / 60.1 |
| Heavy rain / software | 34.0% | 2.844 ms | 2.070 ms | 60.1 / 60.1 |
| Heavy rain / aligned RGBA + GPU | 28.1% | 3.266 ms | 0.822 ms | 60.1 / 60.1 |
| Heavy rain / direct triangles | 17.4% | 0.375 ms | 2.027 ms | 60.1 / 60.1 |

Both directions agree: idle direct-rendering CPU is 7.160% and 7.154%; rain is
17.441% and 17.398%. Relative to software, aligned RGBA saves about **19% idle /
17% rain**, and direct geometry saves **74% idle / 49% rain**. RGBA still pays CPU
rasterization/conversion, but its upload call averages about 0.50 ms CPU and
reduces output drawing cost. Direct geometry eliminates rasterization and the
full-frame image upload; its encoding averages 0.606 ms idle / 1.591 ms rain,
with submission at 0.098 / 0.133 ms. Those stages are inside drawing, not extra
costs to add to it. Drawing wall time includes synchronization with refresh:
the direct path's roughly 16 ms wall draw is **not** 16 ms of CPU or GPU work.

The earlier apparent Pi 4/Pi 5 discrepancy compared different paths: the large
Pi 4 win was direct geometry, while the valid initial Pi 5 measurements only
covered original RGB upload. With the rotation defect fixed and paths matched,
the Pi 5 also gains substantial CPU headroom. Different hardware and viewport
sizes still preclude treating the two devices' absolute CPU percentages as a
controlled processor comparison. These Clock results do not establish heavy
Spacewars/Pizza throughput on Pi 5.

A separate paused split-screen Spacewars smoke check includes the HUD, both
minimaps and pause overlay. Baseline triangles and retained-culled geometry
with bucketed streams/prepared plans/source pooling produce byte-identical
PNGs, and returning to the launcher remains correct. This is not a full-match
test. Physical touch alignment still needs a user check.

Artifacts, raw counters/status snapshots, scripts, build/test logs and PNGs are
under `target/gpu-experiments/pi5-rotation.NAwPIHrT/` locally and
`/data/spacewars/gpu-rotation.YgzHlbWO/` on the Pi. Menu captures are in
`/tmp/spacewars-freshness.PogFsgTh/` and `/tmp/spacewars-freshness.M6IMwQCy/`.
The device is left in normal demo Clock, GPU triangles and CPU-shaped text,
not a benchmark fixture. Original preferences, including volume and auto-start,
are restored byte-for-byte after the temporary Spacewars selection/mute.
The boot ID is unchanged, with zero automatic service restarts and no GPU scene
errors. The renderer override is session-only and clears at reboot; software
remains the application default. Other devices are untouched.

## Pi 5 Spacewars and Pizza comparison

The October 8 complex-scene follow-up uses the same installed release client
`e9cb1e2614d3`, CLI `f22dcf1fba84`, Pi 5 HyperPixel and native 800×480 viewport
as the corrected Clock comparison. The driver identifies Broadcom V3D 7.1.10.2,
OpenGL ES 3.1, Mesa 25.1.6. There is no redeployment or application code change
in this follow-up. The ordinary Clock process is suspended during offscreen
tests and stopped through its Quit action during live tests, so it does not
compete with the measured process.

### Live application CPU

Each block starts a fresh process with private settings and a private control
socket. Spacewars uses seed 0, human P1, rule-bot P2, no asteroids and a ten-minute
match. This is the landed/approaching-planet fixture, not a complete two-bot
autoplay match. The measurement covers approximately ticks 1400–2132. Pizza uses
seed 0, 1000 prefilled balls, dense workload, Rapier contacts and fast Barnes–Hut
gravity, approximately updates 304–1035. Physics continues normally in both.

For each scenario, the order is software → RGBA → triangles → optimized →
optimized → triangles → RGBA → software. Each measured interval is about twelve
seconds, with six status samples at approximately 120-update intervals. Process
CPU uses `/proc/PID/stat` and `AT_CLKTCK`, as in the Clock comparison. GPU paths
all use CPU-shaped text; overlays and audio are disabled. Screenshots are taken
after timing. Frequencies are not pinned: endpoint readings span 1.6–2.4 GHz
and 51.25–55.1°C. These short counterbalanced blocks measure CPU headroom at
normal pacing, not a sustained maximum-throughput or fixed-frequency result.

| Live fixture | Software | Aligned RGBA + GPU | Direct triangles | Specialized geometry |
| --- | ---: | ---: | ---: | ---: |
| Spacewars landing/approach | 57.2% | **34.6%** | 81.9% | 57.0% retained/culled |
| Pizza, 1000 active balls | 71.1% | 62.7% | 67.3% | **48.3%** instanced circles |

Percentages are application CPU, with **100% equal to one core**. Each cell
combines both blocks. Spacewars' specialized path selects `retained-culled`,
bucketed stream storage, prepared plans and pooled source buffers; extra stroke
rejection remains Off. Pizza's specialized path selects the original `circles`
mode, not a later experimental vertex-layout variant. All paths report about
60 FPS and 60 updates/s. Counter-derived rates range approximately 59–61 because
the status publication and process-counter boundaries are not frame-aligned;
the display refresh rate is not evidence of equal rendering cost.

Spacewars' RGBA path cuts process CPU about **40% versus software** in this
fixture. It spends 2.77 ms in host preparation, including rasterization and
publication, and 1.47 ms of CPU drawing/presenting that image and the UI. Direct
triangles instead spend 11.17 ms in drawing CPU, including 8.88 ms of geometry
encoding. The retained path reduces these to 7.33 and 5.01 ms respectively,
with mesh uploads falling from about 3.12 to 2.54 MB per frame. It recovers most
of the direct-path regression but does not beat RGBA. Nested encoding/submission
times are part of drawing, not additional buckets to add to it.

Pizza's instanced path cuts process CPU about **32% versus software**. Its
drawing CPU falls from direct triangles' 4.22 ms to 0.71 ms, encoding from
3.29 ms to 0.028 ms and geometry uploads from 1.69 MB to 48 kB per frame.
The host simulation/control stage still averages 6.7–7.1 ms across the paths;
rendering gains do not eliminate the physical simulation's cost.

### Uncapped identical-scene capacity

The capacity probe alternates mode/fixture order over four 120-frame blocks,
with sixteen untimed warmup frames, fresh presenters, 4× MSAA and three frames
allowed in flight. Pizza geometry is frozen after initial simulation; the
interleaved synthetic fixture animates. Physics, UI, text and scanout are not
timed. Completed wall time includes draining GPU fences; it is neither the
submission call duration nor an isolated GPU timer. This driver lacks
`EXT_disjoint_timer_query`.

| Fixture | Triangles completed time | Circles completed time |
| --- | ---: | ---: |
| Pizza 1000 | 3.10 ms | 1.36 ms |
| Pizza 5000 | 12.50 ms | 2.45 ms |
| Pizza 10000 | 23.25 ms | 4.85 ms |
| Mixed interleaved | **0.93 ms** | 2.51 ms |
| 32 transparent overlapping layers | 4.16 ms | 4.16 ms |

Values are medians of the four block averages. The 10000-ball result is a
**render-only capacity result**, not 10000-ball live-game FPS. Triangle CPU is
23.21 ms there, including 20.41 ms of tessellation; circle CPU is 1.47 ms. The
mixed scene regresses roughly 2.7× with circles and uses 384 draws instead of
one. This run does not re-isolate program/vertex-state switching from draw
count; the earlier submission controls cover those causes. The transparent
overlap case uses only 0.15–0.17 ms of thread CPU but 4.16 ms completed wall time,
showing a separate completion/pixel-work limit rather than heavy CPU geometry.

The moving Spacewars probe separately includes source generation, projection,
encoding, upload, `glFinish` and temporary cleanup. Physics, text, UI and scanout
remain outside its timing. Each fresh scenario has eight initial frames reported
separately, then 128 measured frames; mode order alternates over four repeats.
It uses the reference plan/shared stream, not the live test's combined allocation
controls.

| Moving replay | Triangles completed time | Retained/culled completed time |
| --- | ---: | ---: |
| Early bots, seed 42 from tick 120 | 8.15 ms | 3.57 ms |
| Later bots, seed 42 from tick 1800 | 12.96 ms | 4.71 ms |
| Close-up zoom, seed 0 from tick 1500 | 12.73 ms | 8.90 ms |

Reuse helps all three, but changing-zoom encoding remains 4.69 ms versus
0.59/0.74 ms in the early/later replays. Retained first-frame wall medians are
10.41/18.66/21.12 ms respectively, so warm averages must not hide first-use costs.
The driver still exposes zero vertex-stage storage-buffer blocks; the
resident-buffer path is unsupported, not newly available because this is a Pi 5.

### Remaining Spacewars encoding cost

The existing `--geometry-encode-profile` probe then isolates CPU encoding on
the same 800×480 replays, with four rotated repeats and 128 warm frames per
control. Simulation, source generation, projection and GPU work are excluded.
Its 7560 mesh-equivalence checks pass. Uninstrumented retained/culled encoding
is 0.49/0.66/4.43 ms for early/later/zoom respectively. Detailed clocks increase
zoom CPU to 6.34 ms, about **43% overhead**; use them for attribution, not as an
estimate of uninstrumented frame cost.

Within the profiled zoom block, guarded outlines account for 4.52 ms, versus
0.43 ms expanding reusable exact/scalable meshes and 0.04 ms resolving cached
geometry. Stroke tessellation is 3.54 ms nested inside the outline bucket.
About 3690 guarded strokes are rebuilt per frame; post-tessellation visibility
inspection finds 1123 fully outside their pass, with no guarded fill fallbacks.
This identifies a reason to test the existing conservative early-stroke
rejection control on Pi 5 before attempting another cache/mesh rewrite. It
does not establish that all those rejected-after-tessellation strokes can be
safely rejected by a cheaper bound, or that their profiled 1.38 ms can be
subtracted from the uninstrumented total. Keep the live RGBA path as the
whole-application comparison when measuring that control.

### Correctness and next investigation

The capacity probe passes its independent supersampled circle comparisons,
repeat-checksum checks and 32 animated interleaved comparisons. These use
explicit edge-error tolerances; the two circle representations are not
pixel-identical. Moving Spacewars validation passes 2808 frame comparisons
across 13361 reused groups, with maximum RGB MAE 0.00059635 on a 0–255 scale.
The separate 5148 outline checks and six exact uniform-table boundary images
also pass. Unsupported storage-buffer checks remain skipped.

Real screenshots include full split-screen views, both minimaps, HUD text and
the dense Pizza population. Three repeated captures per paused mode/text-policy
combination are byte-identical, and 48 further active Spacewars captures preserve
the changing match timer with vector and CPU-shaped text. This is not full-match
or arbitrary-font coverage, nor a physical touch-alignment test.

Preserve all comparison paths. Clock's large direct-geometry win does not
establish a universal GPU-scene default, and Pizza's extreme circle case does
not establish a general instancing policy. The next Spacewars target is the
remaining geometry encoding during changing zoom, measured against **both**
the cached direct path and the faster live RGBA reference. Do not lower visible
detail or change physics to make direct rendering win.

Artifacts, raw CSV/counters/status samples, scripts and PNGs are under
`target/gpu-experiments/pi5-scenes.5brgQNiK/` locally and
`/data/spacewars/gpu-scenes.kIH413dE/` on the device. The kiosk is restored to
normal demo Clock with session-only GPU triangles and CPU-shaped text. Saved
preferences and installed binaries are byte-identical to their starting values,
the boot ID is unchanged and automatic service restarts remain zero. No other
device, production default, physics setting or GitHub state is changed.

## Pi 5 guarded stroke comparison

The October 8 follow-up tests the existing conservative rejection control on
`spacewars.local`, without changing its release binary, driver, rootfs or saved
preferences. Native logical output remains 800×480 at 1× on the rotated Pi 5
HyperPixel. Early rejection improves the direct renderer, but the three-way
live comparison still favors aligned RGBA for the normal-camera Spacewars
fixture. No renderer or rejection default changes.

### Identical moving geometry

The existing stroke probe uses 24 consecutive production ticks cyclically,
with 24 warmup frames and eight repeats of 128 measured frames per control.
CPU plain/profiled blocks are counterbalanced; completed-rendering blocks
alternate control order. Both modes use retained culling, prepared plans,
bucketed streams and the stock stroke writer. CPU timing excludes source
generation/projection; completed rendering additionally includes upload,
drawing and `glFinish`, but not physics, text, UI or scanout. These are narrower
timing boundaries and a different replay history from the earlier 128-tick
moving-scene test; compare paired controls within this probe.

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-stroke-rejection \
  --benchmark-width 800 --benchmark-height 480 \
  --presentation-frames 128 --presentation-repeats 8
```

| Fixture | Off encoding CPU | Reject encoding CPU | Off completed wall | Reject completed wall |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 0.405 ms | 0.405 ms | 2.082 ms | 2.082 ms |
| Later bots | 1.421 ms | 1.421 ms | 3.201 ms | 3.194 ms |
| Close-up zoom | 4.153 ms | 3.172 ms | 6.562 ms | 5.370 ms |

Uninstrumented zoom encoding improves **23.6%** and completed wall time
**18.2%**; all eight paired blocks improve. Encoding gains span 23.38–23.77%,
and completed-wall gains 17.70–18.98%. Completed CPU falls 5.293 → 4.139 ms.
Per-frame encoding wall p95 falls 4.183 → 3.216 ms and completed wall p95
6.684 → 5.430 ms. Detailed-clock samples are not used to claim these gains.

The zoom replay rejects 1112.93 of 3686.88 tested outlines per frame, **30.2%**,
with zero unknown bounds. Uploaded geometry falls 2,651,902 → 2,118,576 bytes
per frame, **20.1%**, and vertices 79,302 → 63,658. Early gameplay has no
eligible outlines; later gameplay tests only 0.133 per frame. Their effectively
unchanged performance is expected, not a contradiction of the zoom result.

All 84 moving/resize/zoom/replacement states preserve exact GPU pixels. The
independent checked tessellation verifies that every omitted triangle is
offscreen and retained triangle bits/order remain unchanged. The 18,432 CSV
rows form 9216 stock/reject pairs: geometry/upload counts never increase and
cache admissions match. Counts also agree across plain, profiled and completed
paths at all 6144 corresponding fixture/mode/repeat/frame positions. The three
exported geometry images were inspected; this probe intentionally omits text.

### Live normal camera comparison

Fresh private processes/settings repeat the preceding seed-0 human/rule-bot
fixture with no asteroids, native resolution, muted audio and overlay Off.
The order is RGBA → rejection Off → conservative → conservative → Off → RGBA.
Each block measures approximately match ticks 1400–2132 for twelve seconds,
with six rolling status samples. Both direct controls use identical retained
culling, bucketed streams, prepared plans, pooled source points and CPU-shaped
text. Only the rejection policy differs. The RGBA path also uses CPU-shaped
text, with ordinary Slint texture lifetime.

| Path | Process CPU, one core = 100% | Whole-loop CPU | Drawing CPU |
| --- | ---: | ---: | ---: |
| Aligned RGBA + GPU | **33.7%** | 5.607 ms | 1.447 ms |
| Cached geometry, rejection Off | 57.0% | 9.486 ms | 7.313 ms |
| Cached geometry, conservative rejection | 51.0% | 8.490 ms | 6.318 ms |

Both repeat directions agree. Rejection saves about **10.5% of process CPU**
relative to the same direct path with rejection Off, not 10.5 percentage
points. Geometry encoding wall time falls 5.009 → 4.136 ms, and uploads
2.551 → 2.070 MB per frame. The sampled geometry windows reject about 1028
outlines per frame. The direct path still uses substantially more CPU than
RGBA in this fixture; none of these results supports a universal direct-GPU
default.

### Live forced zoom and correctness

A separate Off → conservative → conservative → Off comparison warms each fresh
match for one 480-tick sweep, then measures three sweeps, approximately ticks
487–1949 over 24 seconds. Both cameras sweep in opposite phases between heights
44 and 640. All other geometry/text/allocation choices match the normal-camera
direct controls. The existing sweep only supports GPU scenes: **there is no
matched RGBA forced-zoom result here**. Its normal-camera CPU figure must not
be presented as the cost of rendering this sweep.

| Zoom metric | Off | Conservative | Change |
| --- | ---: | ---: | ---: |
| Process CPU, percent of one core | 47.7% | 42.6% | −10.8% relative |
| Whole-loop CPU | 7.926 ms | 7.073 ms | −10.8% |
| Drawing CPU | 5.807 ms | 4.909 ms | −15.5% |
| Geometry encoding wall | 3.887 ms | 3.027 ms | −22.1% |
| Geometry upload per frame | 2.049 MB | 1.623 MB | −20.8% |

Both repeat directions improve. The conservative windows average 2339 tested
and 858 rejected outlines per frame, with zero unknown bounds. Normal-camera
and sweep runs remain near 60 submitted FPS and 60 updates/s. Process CPU uses
user+system ticks divided by elapsed time and `AT_CLKTCK`; renderer spans measure
the UI thread. Status/counter boundaries are not frame-aligned, so short
counter-derived FPS values of roughly 59–61 are not refresh-rate changes.
The governor remains unchanged; endpoint readings span 1.9–2.2 GHz and
50.15–53.45°C. Detailed per-piece clocks are disabled in these live timings.

Two additional checked sweeps each cover about ticks 243–3900. All 60 sampled
windows contain actual rejected outlines, with no unknown bounds, geometry
errors or glyph errors. Checked mode fully tessellates proposed rejections
and independently verifies that their triangles lie outside the pass. These
rolling windows can overlap; they are not counts of unique checked strokes.
Repeated frozen captures are byte-identical across Off, conservative and
checked, including both views, minimaps, HUD and the pause menu. Stable paused
cameras warm into the exact cache, so moving checked runs and the independent
84-state probe supply the evidence for active rejection.

Incompatible triangle-mode requests are rejected, switching back to raster
releases all GPU scene targets and resets stroke policy to Off, and returning
to launcher/quit succeeds. Sampled cache payload peaks at 12,865,164 bytes and
128 entries; plan scratch at 2,459,624 bytes and pooled source at 502,560 bytes,
within the existing caps. Observed stream capacity peaks at 10,520,688 bytes;
this is requested buffer storage, not measured driver/VRAM consumption.

The first harness run hit a redundant Pause request during cleanup, after all
timings and image checks had completed. State-aware cleanup fixes the harness;
the second checked run and complete restoration finish successfully. No
application patch or new binary was needed. Script syntax, ShellCheck, analyzer
assertions and repository diff checks pass. This does not replace full-match
Pi 5 coverage or physical touch testing.

Keep rejection selectable and conservative. It is now a measured improvement
on both tested Pi generations, but not a reason to relax the outline safety
guard or increase cache size. Remaining visible-outline work is still material.
The next experiment below extends the presentation-only camera fixture to
raster/software and compares the same sweep across paths.

Raw samples, scripts, analysis and captures are under
`target/gpu-experiments/pi5-strokes.lJECYoO2/` locally and
`/data/spacewars/pi5-strokes.J1U1rT8b/` on the Pi. Client/CLI remain
`e9cb1e2614d3` / `f22dcf1fba84`. The device returns to normal demo Clock with
session-only GPU triangles and CPU-shaped text. Preferences and binaries are
byte-identical to their starting values, the boot ID is unchanged and automatic
service restarts remain zero. No other device is touched.

## Pi 5 matched zoom comparison

The same camera sweep now runs across software, raster-to-GPU and direct
geometry. In this Spacewars fixture, **aligned RGBA remains the cheapest path
during zoom**, using 40.1% less process CPU than software. Conservative outline
rejection improves direct geometry, but does not close its gap to RGBA.

### Shared camera control

`host camera-zoom off|sweep` replaces the GPU-owned camera state with a small
renderer-independent presentation control. The existing `gpu-zoom` command
and status fields remain aliases. It retains the 44–640-unit, 480-tick,
opposite-phase formula, applied before source culling and minimap construction.
Physics, camera smoothing and saved settings are unchanged. It resets at
scenario entry and launcher return, not when GPU resources are retired or
the renderer falls back to raster. Same-session restart repeats the sweep.

Software-only unit tests now exercise repeated/paused rendering, camera bounds,
phase/overflow handling, raster/vector/reference camera agreement, source
culling versus unculled pixel output, minimap footprints and unchanged
simulation after restoring the normal camera. Separate lifecycle tests cover
idempotent requests, GPU retirement and clearing the session. CLI and IPC
parsers accept both names and reject implicit or extra arguments.

### Method and results

The October 8 run uses `spacewars.local`, the Pi 5 HyperPixel at logical
800×480 / physical 480×800, native 1× scale, with the same release binary in
every block. Each fresh private process uses seed 0, human P1, legacy rule-bot
P2, no asteroids, muted audio, overlay Off and a ten-minute match limit.
Software presents RGB through LinuxKMS software. RGBA presents aligned raster
images through FemtoVG with ordinary Slint texture lifetime. Both direct
controls use retained culling, bucketed streams, prepared plans and pooled
source points. All GPU paths use CPU-shaped text.

The order is software → RGBA → rejection Off → conservative → conservative →
Off → RGBA → software. Each block warms for one eight-second sweep, then
measures about three sweeps: published match ticks 486/487–1948/1949, over
24.30–24.46 seconds. Twelve rolling samples per block validate the actual
renderer and policies, native dimensions, sweep source tick, zero errors and
bounded caches. Direct-path projected heights also match the shared formula.
Status publication and process counters are not frame-aligned; this is a
matched camera trajectory and simulation interval, not an exact frame-by-frame
GPU replay. The governor is unchanged; endpoint readings span 1.9–2.4 GHz and
47.95–53.45°C.

| Path | Process CPU, one core = 100% | Whole-loop CPU | Drawing CPU |
| --- | ---: | ---: | ---: |
| Software | **53.8%** | 8.958 ms | 5.701 ms |
| Aligned RGBA + GPU | **32.2%** | 5.347 ms | 1.427 ms |
| Cached geometry, rejection Off | **48.2%** | 8.013 ms | 5.904 ms |
| Cached geometry, conservative rejection | **42.2%** | 7.016 ms | 4.853 ms |

Both repeat orders agree: process CPU ranges are 53.79–53.87%, 32.16–32.30%,
48.05–48.25% and 42.15–42.27%, respectively. All paths submit about 60 FPS and
advance about 60 updates/s. Process CPU uses user+system ticks divided by
elapsed time and `AT_CLKTCK`; renderer spans measure the UI thread. These are
CPU costs, not isolated GPU elapsed times or uncapped throughput.

Rejection saves **12.3% of direct-path process CPU**. Geometry encoding wall
time falls 3.978 → 3.067 ms and geometry uploads 2.067 → 1.671 MB per frame,
reductions of 22.9% and 19.2%. Conservative windows average 2390 tested and 842
rejected outlines per frame, with zero unknown bounds.

The remaining gap is in drawing: conservative direct geometry saves 1.757 ms
of host/timer CPU versus RGBA, but spends 3.426 ms more drawing CPU, for a net
whole-loop deficit of **1.670 ms per frame**. RGBA pays about 0.478 ms of host
image-publication wall time and 0.505 ms of texture-upload CPU, yet remains
cheaper overall. Avoiding the image upload alone therefore does not justify
direct geometry in this scene. Its remaining 3.067 ms encoding span is the
next useful target, with the prior outline profile supplying the leading
explanation; this experiment does not separately time each encoding substage.

### Validation and handoff

All eight blocks pass pause → normal camera → legacy-alias sweep restoration.
Repeated and restored sweep screenshots are byte-identical within each path;
normal-camera images differ, confirming that the override actually changed
the view. Paused match ticks remain fixed. Direct-to-RGBA transitions retain
the sweep and release every GPU scene target. Launcher return clears it,
the launcher and Clock reject enabling it, and disabling it remains safe.
Active software/RGBA/direct and paused transition images were visually checked
at full logical size. Cross-renderer antialiasing is not pixel-identical and
is not claimed as such.

Desktop tests pass **627 with one ignored**, GPU-feature tests **658 with four
ignored**, and CLI tests **29**. The Yocto release build, fast-update health
check, script syntax/ShellCheck, analyzer assertions and diff checks pass.
The initial host GPU-test invocation lacked `libseat.pc`; using the existing
local development-package staging directory resolved that build prerequisite.
No system package or device OS change was required.

Raw samples, runners, images, hashes and analysis are under
`target/gpu-experiments/pi5-matched-zoom.JJ7fvU07/` locally and
`/data/spacewars/pi5-matched-zoom.QDG535j5/` on the Pi. Deployed client/CLI hashes
are `658cada8a722` / `0f8929332b30`. The device returns to normal Clock with
session-only triangles and CPU-shaped text, the camera probe Off, no renderer
errors and zero automatic service restarts. Saved preferences and boot ID are
unchanged. No other device is touched and no renderer default changes.

Keep the RGBA reference and selectable direct controls. Investigate visible
outline encoding next; increasing cache budgets or selecting a universal
direct-rendering default is not supported by this result. This is short
Spacewars zoom coverage, not a new Clock/Pizza comparison or full-match soak.

## Pi 5 small outline attribution

The remaining guarded-stroke hot spot is tiny terrain detail. In the close-up
replay, **every rebuilt outline is at most two logical pixels across**, with a
0.75-pixel stroke. Shapes at most one pixel across consume **88.0%** of the
nested stroke-tessellation time after conservative rejection. Larger ship and
planet outlines are not the source of this particular fallback cost.

This is the existing 24-frame close-up replay at 800×480 on Pi 5, not a new
measurement of the whole application or of the full forced camera sweep.
The renderer still uses its original geometry, styles, tolerance, cache budgets
and safety guards. No rendering default changes.

### Diagnostic scope

With `--geometry-output`, the stroke-profile probe now writes
`stroke-census.csv` alongside its existing captures and manifest. For each
fixture and control, a separate 96-frame replay follows the 24-frame warmup.
Fixed-size buckets group guarded outlines by input point count, projected
bounding-box extent and generated-output bounds. Counts include points,
vertices, indices, edges shorter than the stroke width and edges shorter than
Lyon's merge distance. The latter is a geometric descriptor, not a claim that
Lyon actually merged that edge.

The report labels a stroke `outside` only when all its generated vertices are
beyond one expanded clip edge. `clip-candidate` means it cannot be discarded by
that test; it does **not** prove that any triangle contributes a visible pixel.
Empty and unclipped output have separate buckets. The probe verifies that its
totals account for every guarded stroke, point, vertex and index. Its scratch
storage is bounded to 168 buckets, with no retained polygon copies.

This diagnostic pass is outside all plain/profiled/completed timing blocks.
Nested per-stroke clocks include clock overhead and describe where the work
goes; do not add them to, or subtract them from, the uninstrumented CPU totals.
Live rendering never enables the census, and the const-disabled path does not
collect it even if a diagnostic collector is present.

### Results

After rejection, the close-up replay still rebuilds **2575.4 outlines/frame**,
producing **36,403 stroke vertices and 91,574 stroke indices/frame**. Almost
94% of their source edges are shorter than the 0.75-pixel stroke width; only
2.45% are below the merge distance. The conservative scale guard is therefore
handling many cases where outlines are thick relative to the source polygon,
not simply a large number of coincident points.

| Guarded outlines after rejection | Outlines/frame | Nested tessellation ms/frame |
| --- | ---: | ---: |
| Clip candidates, four points, at most one pixel across | 1792.5 | 1.679 |
| Other clip candidates | 681.4 | 0.654 |
| Wholly outside the expanded clip | 101.5 | 0.096 |
| Total | 2575.4 | 2.429 |

The first row alone accounts for **69.1%** of remaining nested stroke time.
Its average bounding-box extent is 0.792 pixels. Wholly offscreen output is
only **3.93%** of that time after rejection, versus 1.146 ms/frame before
rejection. Further tightening the offscreen bound cannot remove the dominant
remaining work in this fixture. Early gameplay rebuilds no guarded outlines
after warmup; later gameplay averages only 0.125/frame with rejection enabled.

The same-build counterbalanced timing control reproduces the earlier rejection
benefit; it is not a new optimization:

| Moving fixture | Stock encoding CPU ms | Rejection encoding CPU ms | Stock completed wall ms | Rejection completed wall ms |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 0.407 | 0.406 | 2.075 | 2.062 |
| Later bots | 1.445 | 1.446 | 3.201 | 3.206 |
| Close-up zoom | 4.245 | 3.243 | 6.634 | 5.449 |

Close-up encoding improves 23.6% and completed drawing 17.9%. All 18,432 rows
pass paired count/admission checks, including equality between the plain,
profiled and completed paths of each control. As before, completed wall time
includes GL submission and `glFinish`, not display scanout or isolated GPU
elapsed time. These measurements exclude simulation, projection, text and UI.

### Validation and next experiment

All 84 validation states preserve the checked geometry and exact GPU pixels.
The three exported pictures also match the previous Pi build byte-for-byte
and pass visual inspection. New tests cover bucket accounting, closed edges,
empty/invalid/unsupported shapes, clip classification, nested clocks and the
const-disabled path. Desktop client tests pass **629 with one ignored**;
GPU-feature tests pass **660 with four ignored**. Formatting, script syntax,
ShellCheck, analysis assertions, the release build and fast-update health
checks pass.

The next useful experiment is an explicitly selectable representation for
small geometry, retaining the current exact renderer as the reference. Test
whether subpixel terrain detail can be rendered more cheaply without visible
zoom transitions, lost material detail, terrain-edit lag or new seams. Start
with a bounded render-only control and measure image differences as well as
CPU and completed rendering time. This result does not justify silently
dropping small polygons, increasing cache budgets, weakening the scale guard,
or adding a rectangle-specific shortcut. It also does not establish which
small-geometry representation will win.

Artifacts are under `target/gpu-experiments/pi5-stroke-census.mEBMw7Md/` locally
and `/data/spacewars/pi5-stroke-census.fCmGwiBl/` on the Pi. The client hash starts
`0dbeac3eae85`; the CLI remains `0f8929332b30`. Temperature checkpoints are
48.0 and 53.5°C. The device is left on normal Clock with session-only triangles
and CPU-shaped text, the camera probe Off, about 60 FPS/UPS, no renderer errors
and zero automatic service restarts. Saved preferences and boot ID remain
unchanged. No other device is touched.

## Pi 5 tiny outline approximation

Replacing rounded joins with bevels on tiny opaque polygons saves **12.5% of
encoding CPU and 8.0% of completed drawing time** in the close-up replay. This
is a useful diagnostic control, not a new rendering default. Production images
change very little in this test, but isolated tiny shapes expose a measurable
loss of edge coverage. Keep the exact renderer as the reference.

### Control and measurement

The opt-in `--geometry-small-strokes` control requires the stroke-profile
probe. It only changes uncached `Polygon` strokes when the source has 3–64
finite points, its projected bounding box is at most one logical pixel across,
the stroke is 0.001–1 pixel wide, and its opaque color matches the fill.
Circles, lines, distinct outlines, translucent shapes and larger polygons stay
unchanged. Lyon still handles point merging, folding, caps and tessellation;
only the selected join style changes. No shape is discarded, and there is no
rectangle-specific shortcut.

Both controls use conservative offscreen rejection, the stock stroke writer,
prepared plans and bucketed streams. Cached templates remain round; admission,
budgets and scale guards do not change. The control is off by default and has
no live host command or saved setting.

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-small-strokes \
  --benchmark-width 800 --benchmark-height 480 \
  --presentation-frames 128 --presentation-repeats 8 \
  --geometry-output captures
```

The Pi 5 run uses V3D 7.1.10.2 / Mesa 25.1.6 at 800×480. Each timing block
warms the existing 24-frame moving history, then replays 128 frames. Eight
paired repeats alternate reference/candidate order. Source generation,
projection, simulation, text, UI, readback and scanout are outside timing;
there are no per-piece clocks in these samples. Completed wall time includes
submission and `glFinish`, **not isolated GPU elapsed time**. The normal
application is suspended during the offscreen run and resumed afterward.

| Moving fixture | Reference encoding CPU ms | Bevel encoding CPU ms | Reference completed wall ms | Bevel completed wall ms |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 0.405 | 0.405 | 2.091 | 2.100 |
| Later bots | 1.417 | 1.419 | 3.192 | 3.184 |
| Close-up zoom | 3.183 | 2.785 | 5.420 | 4.987 |

The close-up saving is consistent: paired encoding gains range from
0.394–0.403 ms, and completed-wall gains from 0.399–0.458 ms. It approximates
2,274 outlines/frame, but reduces output by only 151 vertices, 452 indices
and 5.4 KB/frame—about 0.26% of upload bytes. The main measured improvement
is CPU construction, not a substantial reduction in geometry or bandwidth.
The early/later fixtures gain nothing meaningful: after warmup they select
zero and 0.023 outlines/frame, respectively. All 12,288 timing rows preserve
paired cache admissions and CPU/GPU geometry-count agreement.

### Image quality and history checks

The probe records 443 quality comparisons on both the workstation and Pi:
moving fixtures, native/odd/native resizing, extreme zoom, a 121-step smooth
out-and-back zoom, cold-to-warm stationary frames, synthetic tiny shapes, and
production terrain reveal/edit/replacement histories. It checks opaque output,
cache budgets, matching reuse decisions and agreement between CPU and GPU
encoding counts. Differences must stay inside conservative envelopes around
eligible shapes. This locality check is deliberately generous; it is not a
visual-quality threshold.

On the Pi, production comparisons change at most **four pixels per frame**;
the smooth zoom changes at most three. The largest production channel error
is 33/255. Both 80-frame terrain histories are pixel-identical to the reference
despite exercising the approximation. The stationary cold frame selects 2,321
outlines; subsequent frames use cached geometry, with no pixel difference
between controls or between the first and last frames. Returning to the initial
zoom is exact for each control too. On the workstation, cold-to-warm images
change one pixel equally in both controls; the approximation adds no change
in that check.

The synthetic fixture rotates and scales 1,200 separate triangles, quads,
concave polygons and sharp slivers. It exposes up to **525 changed pixels**,
with a maximum channel error of 48/255 across the sequence. Its maximum
screen-wide RGB mean absolute error is only 0.0282/255, but most of the screen
is empty: that small average must not be mistaken for imperceptibility.
Signed frame-to-frame approximation error is recorded separately from scene
movement. Reference/candidate captures and amplified differences were visually
checked for production zoom, synthetic coverage and terrain edits. The three
standard reference captures also match the previous Pi binary byte-for-byte.

### Disposition and next test

Keep bevels as a benchmark-only control. They demonstrate that tiny round-join
construction is worth investigating, but do not establish a general,
history-independent level-of-detail policy: cached geometry is still round,
and a hard one-pixel selection boundary remains. These replay savings also do
not establish that direct drawing now beats aligned RGBA in the separate
whole-application camera-sweep test.

Next test an **exact-output** shortcut for round joins that require no arc
subdivision. The pinned Lyon implementation computes angles and subdivision
counts even for those joins. A conservative early-out may avoid that work
without bevels' coverage changes; treat this as a hypothesis until vertex,
pixel, history and paired-timing checks pass. Preserve the full reference path
for folded joins, numerical boundaries and unsupported cases. Avoid expanding
this into a general geometry simplifier or relaxing existing cache guards.

Desktop client tests pass **632 with one ignored**; GPU-feature tests pass
**664 with four ignored**. New coverage includes selection bounds, transparency,
style overrides, default-Off behavior, CLI opt-in/conflicts, image locality,
dimensions/alpha and signed temporal-error accounting. Formatting, diff checks,
script syntax/ShellCheck, analyzer assertions, the Yocto release build and
fast-update health checks pass.

Artifacts are under `target/gpu-experiments/pi5-small-strokes.R2TrNgd5/` locally
and `/data/spacewars/pi5-small-strokes.tJLBycaD/` on the Pi. The deployed client
hash starts `bfc822780445`; the CLI remains `0f8929332b30`. Temperature checkpoints
are 47.4 and 52.9°C. The device is running normal Clock with session-only
triangles and CPU-shaped text, about 60 FPS/UPS, no renderer errors and zero
automatic service restarts. Saved preferences and boot ID are unchanged. No
other device is touched.

## Pi 5 exact round join shortcut

Skipping rounded-corner work that cannot emit an arc improves close-up replay
encoding **3.361 → 3.003 ms (10.6%)**, with completed drawing
**5.657 → 5.292 ms (6.4%)**. Unlike the bevel control, this experiment preserves
the exact output: all 439 cross-mode image comparisons are pixel-identical,
including moving zoom and terrain edits. This is still a benchmark-only
experiment, not a new live-rendering default or a whole-application result.
An additional cross-build check finds a **5.1% encoding regression in the
disabled/reference path**. Remove that baseline overhead before promoting the
shortcut; its same-build saving overstates the net benefit over stock Lyon.

### Control and scope

The pinned `lyon_tessellation` 1.0.20 source is now vendored with its original
licenses and a narrow patch to round joins. See
[patch provenance and bounds](../vendor/lyon_tessellation/SPACEWARS_PATCH.md).
`RoundJoinMode::Reference` retains the original calculation and is the default.
`Exact` skips angle/subdivision/arc calculations only inside a conservative
zero-subdivision domain; `Checked` runs the original math and asserts that each
proposed shortcut really has zero subdivisions. The required vertex-side state
update is retained even when no geometry is emitted.

The predicate accepts fixed-width, non-folded joins with finite, bounded
coordinates and radii, sufficiently coarse tolerance, and bounded normal
lengths. Dot/cross-product tests select a minor angle below 98°, while the
reference's flattening step is at least 120°. Near-degenerate and ambiguous
cases fall back, as do variable-width strokes and folded joins. This is not a
rectangle shortcut or a generic shape simplifier; it changes no cap/join style,
width, tolerance, color, point merging, cache admission, outline guard or
budget. Cached template builders continue using the reference.

Select `--geometry-round-joins` with the existing geometry stroke-profile
probe. It is mutually exclusive with the bevel and rejection comparison
flags. Both measured controls use prepared plans, bucketed streams, retained
culled geometry and conservative stroke rejection. Per-tessellator shortcut
and fallback counters are included in enabled-mode timing; reference counters
remain zero. No global switch, saved preference or normal-game diagnostic
collector is added.

### Paired timing

On the Pi 5 HyperPixel/V3D 7.1.10.2, eight counterbalanced block pairs each use
128 timed frames after the existing 24-frame warmup, at 800×480. All 12,288
rows pass geometry, upload, admission and CPU/completed-counter agreement
checks. Source generation, projection, simulation, text, UI, readback and
scanout remain outside these measurements. Completed wall time includes GL
submission and `glFinish`; it is not an isolated GPU timer.

| Moving fixture | Reference encoding CPU ms | Exact encoding CPU ms | Reference completed wall ms | Exact completed wall ms |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 0.411 | 0.408 | 2.059 | 2.050 |
| Later bots | 1.470 | 1.467 | 3.243 | 3.232 |
| Close-up zoom | 3.361 | 3.003 | 5.657 | 5.292 |

All eight close-up pairs improve: encoding saves 0.354–0.359 ms and completed
drawing saves 0.354–0.383 ms. Ordinary cached views gain less than 0.5% in
completed drawing, with mixed signs among repeated blocks; do not treat that
as a meaningful gameplay improvement. The close-up case skips an average
9308.4 empty round-join calculations/frame, with 1184.3 fallbacks. Its
63,658 vertices, 147,695 indices and 2,118,576 upload bytes/frame are unchanged.
This saves construction work, not geometry bandwidth or scene detail.

### Reference-path overhead check

The preceding build's close-up reference had measured 3.183 ms encoding and
5.420 ms completed drawing. Rather than attributing the difference to noise,
an additional four process pairs compare the previous and current binaries
directly, reversing order every pair. Both run the common bevel-comparison
harness with 128 frames × two repeats, and analysis selects only the reference
rows. All 24,576 rows preserve geometry, uploads and admission/selection
counts across builds; all 24 exported reference pictures match this run's
three reference PNGs byte-for-byte.

| Moving fixture | Previous encoding CPU ms | Current reference CPU ms | Previous completed wall ms | Current reference wall ms |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 0.403 | 0.412 | 2.066 | 2.070 |
| Later bots | 1.426 | 1.467 | 3.193 | 3.255 |
| Close-up zoom | 3.204 | 3.369 | 5.456 | 5.660 |

All four pairs have higher reference encoding CPU in the current build:
**2.2%, 2.8% and 5.1%** for early/later/close-up respectively. Close-up completed
wall time regresses **3.7%**, also in all four pairs. This establishes a baseline
cost in the new build, not its precise cause: the new per-join mode/counter
plumbing and compiler layout are candidates, not separately measured causes.
Do not report the 10.6% within-build reduction as a net gain over stock Lyon.
Nor should results from different runs be subtracted to claim a precise net
gain. The next change should isolate the reference implementation from this
overhead, then repeat reference/exact/checked validation and paired timing.
Only after that should a session-only switch be tried in the matched
whole-application camera sweep against aligned RGBA.

The previous executable is recovered from the local Yocto release output
`engine_client-0814c4a81be47e54`, not rebuilt from a guessed source revision.
Its Cargo fingerprint retains the same Pi GPU features, profile and rustflags
as the current build. Stripping a copy for transfer preserves all 28 allocated
ELF sections, including code, constants and data, byte-for-byte. The resulting
copy hashes `8a7a8ee75a30…`; because it was stripped locally, this is not the
earlier Yocto-packaged file hash. It is run from the experiment directory,
without replacing the installed application. Pair order, full binary hashes,
temperatures, samples and the asserting analyzer are retained with the artifacts.

### Exact-output checks

Independent reference/exact/checked CPU builders compare every vertex float
bit, index, batch and cache decision for every validation pass. CPU and GPU
builders must also agree on geometry and shortcut/fallback counts. Tests cover
production movement, cold/warm caches, 121 smooth out/back zoom frames, odd and
resized viewports, rotating/scaling tiny shapes, terrain reveals, edits and
replacement. The Pi validation exercises 1,579,755 shortcuts; all 439
cross-mode rows and four self-history rows have zero changed pixels. The three
production reference PNGs are also byte-identical to the preceding deployed
build, and the candidate captures pass visual inspection.

The workstation also passes all 439 cross-mode comparisons. Its cold/warm
self-history comparison retains the existing one-pixel difference in both
reference and exact paths; that is not a new cross-mode mismatch. Keep those
two types of comparison separate instead of relaxing exact-output checks.

The vendored tessellator passes **189 tests**, including its upstream suite
and new angle/quantization sweeps, difficult polygons, curves, caps, side/source
metadata, attributes, and variable-width fallback. Client tests pass **665
with four ignored** with GPU features and **633 with one ignored** on desktop.
The **30 CI report tests** pass; the report now recognizes the existing GPU
phases and the new tessellator phase. Formatting, diff checks, shell/analysis
validation, the Yocto release build and fast-deploy health checks pass. The
optional no-std check stops in the unmodified lyon_geom/euclid feature graph;
only the application's standard-library configuration is validated here.

Artifacts are under `target/gpu-experiments/pi5-exact-joins.wTasnNyf/` locally
and `/data/spacewars/pi5-exact-joins.1z4jYRLV/` on the Pi. The deployed client
hash starts `0ff90d4108d6`; the CLI remains `0f8929332b30`. Main-run temperature
checkpoints are 46.3 and 52.9°C. Clock resumes with the existing session-only
triangles/CPU-shaped-text settings, near 60 FPS/UPS, no renderer errors and
zero automatic service restarts. Preferences and boot ID are unchanged. No
Picade is touched; the exact shortcut is not enabled in normal Clock.

## Pi 5 compile-time round-join policies

Compile-time specialization removes most of the disabled/reference overhead.
In the final three-binary comparison, reference close-up encoding is **1.3%**
above the original executable, versus the previous runtime-switch design's
5.1% penalty. The enabled shortcut saves **8.6% encoding CPU and 6.4% completed
wall time against the original**, not just against its own slower reference
mode. Ordinary views remain slightly slower in completed drawing; this is not
a universal improvement or a reason to change live defaults yet.

### Implementation and controls

The reference, exact and checked paths are separate monomorphizations of the
same vendored stroke implementation. `StrokeTessellator`/`StrokeBuilder` retain
their reference API; the reference policy and its counter are zero-sized. It
has no per-join runtime selector or counter pointer. The client selects its
reference, exact or checked tessellator once per uncached stroke, while cached
template builders remain reference-only. Enabled counters remain cumulative
and saturating, and are still included in measured cost. No predicate,
geometry, cache policy, budget, visual style or saved setting changes.

This is not a duplicated fork of the stroke algorithm: sealed policy types
specialize the shared implementation. Tests check reference storage size,
reference/exact/checked geometry and metadata, and mode switching without
losing cumulative counts or allowing reference to increment them.

### First cross-build comparison

Four process pairs reverse old/new order on each pair. Each process runs 128
timed frames × two counterbalanced repeats per mode and path, using the same
800×480 fixtures, 24-frame warmup and retained-culled/bucketed/prepared settings.
The old binary's existing bevel harness supplies its reference rows; the new
binary's exact harness supplies both reference and enabled rows. All 24,576
sample rows pass the geometry/upload/admission checks. Timing excludes
simulation, source/projection, text, UI, readback and scanout; completed wall
time includes submission and `glFinish`, not an isolated GPU timer.

| Moving fixture | Original encoding CPU ms | New reference CPU ms | Exact CPU ms | Original completed wall ms | New reference wall ms | Exact wall ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Early bots | 0.404 | 0.404 | 0.401 | 2.052 | 2.079 | 2.078 |
| Later bots | 1.431 | 1.437 | 1.433 | 3.181 | 3.250 | 3.247 |
| Close-up zoom | 3.243 | 3.246 | 2.913 | 5.440 | 5.517 | 5.157 |

Close-up exact beats the original in all four pairs for both metrics. The new
reference's encoding differences have mixed signs in repeated pairs; the
prior consistent 5.1% close-up encoding penalty is no longer present. However,
its completed-wall means are 1.3%, 2.2% and 1.4% higher respectively, and the
ordinary enabled views do not recover that difference. Keep this distinction:
recovering encoding cost does not establish equal complete-draw cost.

Each new-binary process passes all 439 exact cross-mode image checks and four
exact self-history checks on the Pi, exercising 1,579,755 shortcuts during
validation. The 24 exported production reference images from both binaries
match the previous experiment byte-for-byte. Geometry, upload bytes and cache
decisions also match the original; close-up still skips 9308.4 empty joins and
takes 1184.3 fallbacks per frame without reducing emitted geometry.

Artifacts: `target/gpu-experiments/pi5-static-joins.hWZxIb3Z/` locally and
`/data/spacewars/pi5-static-joins.FOxLmdGU/` on the Pi. The static-policy binary
hash is `146742788973…`; the original control remains `8a7a8ee75a30…` as described
above. The runner's teardown returned nonzero because its unconditional Resume
attempt found Clock already on the gameplay screen. All timing/validation
processes completed, binary/preferences/boot checks passed, and the subsequent
status check confirmed Clock running at 60 Hz with no renderer errors or
service restarts. Treat that teardown issue separately from the measurements.

### Inlining follow-up

The ARM executable shows a separate `mesh::stroke_shape` call in the first
specialized build, whereas the original wrapper was inlined. A second build
adds an explicit inline hint to that small dispatch wrapper. Its standalone
symbol disappears, while the shared tessellator implementation and output stay
unchanged. This tests the call boundary, not a new join predicate.

Four three-binary groups compare original, specialized, and specialized plus
inline. Every pair of binaries runs in both orders equally often. The same
128-frame/two-repeat settings produce 36,864 rows. All geometry, upload,
admission and enabled-counter checks pass; all eight specialized processes
pass their 439 cross-mode and four self-history image comparisons. All 36
exported production reference images match the prior control exactly.

| Moving fixture | Original encoding CPU ms | Inline reference CPU ms | Inline exact CPU ms | Original completed wall ms | Inline reference wall ms | Inline exact wall ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Early bots | 0.404 | 0.404 | 0.402 | 2.052 | 2.078 | 2.077 |
| Later bots | 1.437 | 1.432 | 1.430 | 3.220 | 3.257 | 3.261 |
| Close-up zoom | 3.204 | 3.245 | 2.929 | 5.481 | 5.515 | 5.127 |

Close-up exact beats the original in every group. Against the non-inlined
specialized binary in this same run, however, inline changes close-up exact
encoding **2.915 → 2.929 ms** and completed drawing **5.160 → 5.127 ms**: small,
mixed effects, not a separate established optimization. Keep the hint to
preserve the old inline call boundary, but attribute the substantial improvement
to specialization plus the exact shortcut, not to removing that call alone.

Reference encoding differences from the original are −0.1%, −0.3% and +1.3%;
reference completed-wall differences are +1.3%, +1.2% and +0.6%. Ordinary
enabled views are about 1.2–1.3% slower than the original in completed drawing.
Thus the first run's near-equal encoding means should not be reported as proof
of zero overhead. The stock `ondemand` governor remains in use; before/after
checkpoints record 1.7/2.1/2.4 GHz and 48.5–55.7°C, not a locked-frequency test.

Artifacts: `target/gpu-experiments/pi5-inline-joins.1OAPZFwY/` locally and
`/data/spacewars/pi5-inline-joins.MEe1G4A1/` remotely. The deployed inline build
hash starts `f481725d1387`; the CLI stays `0f8929332b30`. The runner now checks
whether Resume is needed, and completes successfully. Clock resumes near
60 FPS/UPS with triangles and CPU-shaped text, no renderer errors and zero
service restarts. Preferences and boot ID are unchanged; no Picade is touched.
The exact shortcut still has no live default or saved setting.

The [live comparison below](#pi-5-live-round-join-comparison) follows this with
a session-only reference/exact/checked control and matched whole-application
Spacewars camera sweep against aligned RGBA. It includes ordinary views and
original-binary baselines, since the replay's small ordinary regression and
residual reference cost remain relevant. Do not subtract these offscreen
replay savings from an older live measurement to claim a total gain.

### Test coverage

The specialized tessellator passes 190 unit tests and seven doc tests. The
client passes 666 GPU-feature tests (four ignored) and 634 desktop tests (one
ignored); 31 CI-report tests pass. The workstation preserves all 439 exact
cross-mode checks and the prior symmetric one-pixel cold/warm self-history
difference, not a cross-mode difference.

The GPU CI command previously named `engine-client` but selected only library
targets; that client has only a binary target. It now selects `--lib --bins`,
with a regression test guarding the selector. The exact workflow command
passes locally: 666 client, 20 femtovg and three renderer tests. This repairs
coverage in the existing phase rather than adding a second copy of the suite.

## Pi 5 live round-join comparison

The exact shortcut now has a small measured **whole-application** benefit in
the tested Spacewars match, not just an offscreen encoding win. Against the
original executable, process CPU falls **35.0% → 34.4%** with ordinary cameras
and **42.4% → 41.4%** during the matched zoom sweep. Aligned RGBA remains cheaper
at **30.1%** and **32.0%** respectively. Keep the shortcut session-only and
reference by default; this does not make direct geometry the universal winner.

### Session-only control

The existing exact round-join experiment can now run in live `retained-culled`
scenes. These controls change neither saved settings nor simulation state:

```sh
spacewars-cli host gpu-scene retained-culled
spacewars-cli host gpu-stream bucketed
spacewars-cli host gpu-plan prepared
spacewars-cli host gpu-source pooled
spacewars-cli host gpu-strokes conservative
spacewars-cli host gpu-joins reference
spacewars-cli host gpu-joins exact
spacewars-cli host gpu-joins checked
```

Reference remains the default. Exact skips only the previously validated
zero-subdivision calculations; checked additionally runs the original math
and asserts each shortcut. No geometry, tolerance, cache budget, admission or
outline-safety rule changes. The non-reference controls reject unsupported
geometry modes. All join controls require an active GPU scene, not the launcher
or native video. Leaving the supported mode, ending the session, falling back
or tearing down the renderer resets the request to reference.

`gpu_scene_join_policy` reports the request; `gpu_scene_join_applied` changes
only after successful rendering and is `inactive` outside the supported mode.
The existing 120-frame diagnostic window now includes
`gpu_scene_join_shortcuts_avg` and `gpu_scene_join_fallbacks_avg`. Enabled modes
read cumulative tessellator counters around each complete scene draw, including
all camera and minimap passes. Reference does not read those counters. There
are no additional per-shape clocks. A changed policy resets its measurement
window, but preserves the pending plan and warm cache; repeating the same
request is idempotent.

### Live measurement method

The Pi 5 HyperPixel comparison uses the same seeded Spacewars match, 800×480
logical output at native 1×, muted audio and CPU-shaped UI text throughout.
Player one is human, player two is the rule bot, and asteroids are disabled.
Each fresh process warms to at least 480 simulation ticks, then samples at
120-tick targets through tick 1920 (about 24 seconds). Published status and
polling quantize the actual endpoints; CPU rates use the recorded elapsed time,
not the requested duration. Four paths run in forward and reverse order under
both ordinary cameras and the existing renderer-independent zoom sweep:

- aligned RGBA raster-to-GPU presentation;
- original pre-tessellator-patch executable, with cached direct geometry;
- new executable with reference joins;
- new executable with exact joins.

All direct paths use retained-culled geometry, bucketed streams, prepared plans,
pooled source storage and conservative stroke rejection. The original binary
is the same preserved `8a7a8ee75a30…` control used in the offscreen comparisons,
not a reconstruction from a guessed revision. The temporary config and private
control socket keep saved preferences untouched. The stock governor is not
changed.

Process CPU comes from `/proc/PID/stat` over the measured uptime interval;
100% means one full core, including the application's worker threads. Published
rendering windows add attribution, but are not subtracted from older offscreen
results. All paths are still display-paced; FPS checks detect failures rather
than establish headroom. These are whole-application CPU measurements, not
isolated GPU execution times or uncapped-capacity results. Per-stage windows
are not synchronized exactly across processes, so their averages are secondary
attribution, not proof of a small per-stage saving. The CPU-encoding work's live
`gpu_scene_encode_avg_ms` uses elapsed wall time, unlike process CPU or the KMS
loop's thread-CPU timer.

### Results

All 16 runs finish, with two reverse-order samples per path and camera case.
Recorded intervals are 24.26–24.53 seconds, covering approximately ticks
486–487 through 1949–1950. All paths remain near 60 submitted FPS and UPS.
The table reports process CPU; 100% is one core, not the whole Pi.

| Path | Ordinary CPU % | Zoom CPU % | Ordinary CPU ms/frame | Zoom CPU ms/frame |
| --- | ---: | ---: | ---: | ---: |
| Aligned RGBA | 30.09 | 31.96 | 5.027 | 5.318 |
| Original direct geometry | 34.99 | 42.38 | 5.806 | 7.073 |
| New direct, reference joins | 35.46 | 42.78 | 5.919 | 7.158 |
| New direct, exact joins | 34.38 | 41.44 | 5.738 | 6.902 |

Exact beats the original in both repeat orders. Ordinary-view CPU samples are
34.33–34.42%, versus 34.95–35.04% for the original. Zoom samples are
41.42–41.46%, versus 42.35–42.40%. The net process-CPU reductions are **1.8%**
and **2.2%**; normalizing by submitted frames gives **1.2%** and **2.4%**.
These are modest short-fixture results, not a sustained full-match guarantee.

The new reference still costs about 1–2% more CPU/frame than the original.
Its same-build exact saving therefore overstates the net gain; keep the
original-binary baseline in future tests. Live encoding-window means also
vary with sampling phase and must not replace these whole-process intervals.
The enabled windows average roughly 3674 shortcuts/frame for ordinary views
and 5514 for zoom, with about 573 and 906 fallbacks. Those are observations of
this fixture, not thresholds or a new detail policy.

The KMS loop's independently measured CPU averages agree with the modest gain:
original/exact are 5.823/5.703 ms in ordinary views and 7.044/6.888 ms during
zoom. Aligned RGBA's zoom loop is 5.307 ms, leaving about **1.58 ms/frame** to
close. Exact direct geometry still uses about **30% more CPU/frame than RGBA**
in this sweep. Investigate remaining geometry construction/submission using
the matched controls; do not infer that saved join arithmetic reduces upload
bandwidth or GPU work. No tolerances, geometry or visibility rules changed.

The 192 sampled windows pass the renderer, zoom-trajectory, error and resource
checks. Direct rendering stays within the existing 128-entry/16 MiB cache,
8 MiB scratch and 2 MiB source-pool limits, with no stream fallbacks or unknown
stroke bounds. Temperature checkpoints are 49.6–54.0°C; recorded frequencies
are 1.8–2.3 GHz under `ondemand`, not a locked-frequency experiment.

### Image and lifecycle checks

Four paused world states each switch reference → exact → checked → reference.
All 12 comparisons against each state's first reference PNG are byte-identical,
and match ticks stay frozen. Four additional moving checked-mode runs exercise
real shortcuts without an assertion or renderer failure. Unsupported geometry
and launcher requests reject the control; raster replacement releases the GPU
targets, and exiting the session resets the policy and applied diagnostics.

The Pi's subsequent offscreen probe passes all **439 cross-mode and four
self-history comparisons with zero changed pixels**, checking vertex bits,
indices, batches and cache decisions as well. It exercises 1,579,755 shortcuts
during validation. Normal and zoom gameplay captures pass visual inspection.
These image checks are separate from the timed live intervals.

Artifacts are under `target/gpu-experiments/pi5-live-joins.zBFp5aEV/` locally
and `/data/spacewars/pi5-live-joins.5eaP2snF/attempt-2/` on the Pi. An initial
runner attempt stopped at an invalid `host resume` command after its first
image checks; it restored Clock, and its timings are excluded from the table.
The corrected runner uses the existing `pause.resume` UI action and completes
the entire matrix and cleanup successfully. Its scripts, asserting analyzer,
raw samples, images and hashes are retained.

The deployed client hash starts `11934456e005`, and the CLI `3dfae6c0bacc`.
Clock resumes with session-only triangles and CPU-shaped text near 60 FPS/UPS,
reference joins, no renderer errors and zero automatic service restarts.
Preferences and boot ID are unchanged. No Picade is touched.

### Regression coverage

The client passes **671 GPU-feature tests** (four ignored) and **637 desktop
tests** (one ignored); the CLI passes **30 tests**. New tests cover explicit
parsing, policy mapping, saturating counter deltas, mode restrictions,
idempotence, pending-plan preservation, reported applied state, and cleanup
through session exit, fallback and context teardown. The tessellator still
passes its **190 unit tests and seven doc tests**; all **31 CI-report tests**
pass. Formatting and diff checks pass.

The workstation probe passes all **439 exact cross-mode comparisons**. Its
existing one-pixel cold/warm self-history difference remains symmetric in
reference and exact paths; it is not a new cross-mode mismatch. Live paused
captures include the pause UI, so the unoccluded offscreen comparisons remain
important alongside those transition checks.

## Pi 5 combined exact stroke controls

Combining the bulk output writer with exact joins saves a little isolated
encoding time, but **does not establish a whole-application improvement** on
Pi 5. Against the preceding exact-join executable, live process CPU is
34.46% → 34.48% for ordinary views and 41.66% → 41.69% during zoom. Keep the
independent controls for comparison, with stock/reference still the defaults.

### Independent writer and join selection

The existing bulk stroke writer can now run alongside the exact round-join
shortcut in live `retained-culled` scenes. The two controls are independent:

```sh
spacewars-cli host gpu-scene retained-culled
spacewars-cli host gpu-stream bucketed
spacewars-cli host gpu-plan prepared
spacewars-cli host gpu-source pooled
spacewars-cli host gpu-strokes conservative
spacewars-cli host gpu-joins exact
spacewars-cli host gpu-stroke-writer bulk
```

Use `gpu-stroke-writer stock` and `gpu-joins reference` for the original paths
in the same executable. Both remain the defaults, with no saved setting.
Bulk still changes only uncached stroke output: premultiply color once per
stroke and append three triangle indices together. Tessellation positions,
triangle order, fill output, cached templates, admission rules, bounds and
budgets remain unchanged. This is not a rectangle-specific or lossy detail path.

`gpu_scene_stroke_writer_policy` reports the requested writer, while
`gpu_scene_stroke_writer_applied` reports the last successful supported draw
or `inactive`. Changing the writer preserves the pending plan and warm cache,
but resets its measurement window; an identical request is idempotent.
Unsupported geometry rejects bulk, and inactive scenes reject both controls.
Leaving the supported mode, ending the session, fallback and context teardown
restore stock. A failed draw retains the old applied value until replacement.

### Factorial probe and live method

The offscreen join probe accepts `--geometry-stroke-bulk` only with
`--geometry-round-joins`. It applies bulk to both reference and exact controls;
omitting it keeps both stock. Four counterbalanced groups compare the previous
release executable with the new stock and bulk runs. Each process measures
128 frames per block, with two reverse-order reference/exact repeats after a
24-frame warmup of the same 24 production frames. Match measurements by full
replay position, not merely frame modulo 24. The previous executable is the
live-join build `11934456e005…`, not the older pre-Lyon-patch control.

The untimed oracle compares all six stock/bulk × reference/exact/checked CPU
combinations: every vertex float bit, triangle index, batch, cache decision
and enabled join counter must agree. It does not collect per-shape clocks in
the timed or live paths. GPU quality checks cover moving scenes, camera zoom,
odd/native viewport changes, cache history and terrain edits. Captures also
compare stock and bulk across processes and against the previous binary.

The live comparison keeps the preceding seeded Spacewars setup and the stock
governor: native logical 800×480 at 1×, muted audio, CPU-shaped UI text, human
player one, rule-bot player two, no asteroids and unchanged physics. Four paths
run in forward/reverse order for ordinary cameras and the shared zoom sweep:
aligned RGBA, previous-build exact joins, new stock/exact and new bulk/exact.
Each fresh process warms to tick 480, then samples through tick 1920. Process
CPU uses actual `/proc/PID/stat` and uptime intervals; 100% is one core.
Published 120-frame diagnostics are secondary attribution, not synchronized
per-stage subtraction. A stable 60 FPS alone is not the performance result.

### Offscreen results

The four groups contain 36,864 measurements. Every paired full replay position
has identical vertices, indices, upload bytes, admissions and enabled join
counts across writers and executables. All 5,316 quality checks have zero
changed pixels on the Pi; 720 exported reference/candidate image pairs are
identical, as are the corresponding reference images across executables.

| Close-up control | Encoding CPU ms | Completed wall ms |
| --- | ---: | ---: |
| Previous build, stock/reference | 3.254 | 5.499 |
| Previous build, stock/exact | 2.925 | 5.169 |
| New build, stock/reference | 3.254 | 5.505 |
| New build, bulk/reference | 3.204 | 5.515 |
| New build, stock/exact | 2.953 | 5.151 |
| New build, bulk/exact | 2.856 | 5.143 |

With exact joins, bulk reduces encoding CPU **3.3%** within the new build and
**2.4%** against the previous exact build; all four groups improve. Completed
drawing is effectively flat: **0.16%** lower wall time within the new build,
with mixed group directions. One bulk group is slower and remains included.
The preceding-build comparison is only 0.52% lower in completed wall time.
Do not treat the isolated encoding saving as an established throughput gain.

Ordinary views do not show a useful general benefit. With exact joins, new
stock/bulk early encoding is 0.401/0.397 ms and completed drawing 2.041/2.061 ms;
later encoding is 1.434/1.439 ms and completed drawing 3.248/3.252 ms. These
measurements exclude simulation, scene generation, projection, text/UI and
scanout. Completed wall includes GL submission and `glFinish`, not isolated
GPU execution time.

### Live results and next target

All 16 runs finish, covering 192 published measurement windows. Actual measured
intervals are 24.19–24.51 seconds, around ticks 486–487 through 1949–1950.
All paths remain near 60 FPS and UPS. The table reports whole-process CPU;
100% is one full core, not the entire Pi.

| Path | Ordinary CPU % | Zoom CPU % | Ordinary CPU ms/frame | Zoom CPU ms/frame |
| --- | ---: | ---: | ---: | ---: |
| Aligned RGBA, new build | 29.16 | 30.93 | 4.857 | 5.167 |
| Previous build, stock/exact | 34.46 | 41.66 | 5.714 | 6.943 |
| New build, stock/exact | 34.69 | 41.80 | 5.765 | 6.957 |
| New build, bulk/exact | 34.48 | 41.69 | 5.762 | 6.916 |

The same-build bulk saving is only 0.21 percentage points normally and 0.11
during zoom, with overlapping repeat ranges. Against the previous executable,
process CPU is effectively unchanged. Frame normalization moves these very
small differences in both directions; do not claim a net live win from the
isolated encoding result. This experiment adds no convincing evidence of
useful application headroom from tuning the output writer further.

The current zoom KMS-loop CPU means are 6.935 ms for the previous exact build,
6.939 ms for new stock/exact, 6.914 ms for bulk/exact and 5.133 ms for aligned
RGBA. The combination leaves about **1.78 ms/frame** of KMS-loop CPU above
RGBA in this run. These are CPU measurements, not GPU execution times. Do not
subtract this run from the earlier 1.58 ms gap as if those separate sampling
intervals were a controlled writer delta. All geometry and upload counts in
the matched offscreen experiment remain identical.

Keep RGBA and exact direct rendering selectable. The remaining small-outline
tessellation/construction cost is a better next investigation than additional
index-writer tuning. A larger change should still preserve the exact reference,
camera/terrain-history tests and image checks; this result does not justify
dropping small geometry, weakening guards or adding rectangle-specific paths.

Four paused live states each switch through all six writer/join combinations.
All **20 nontrivial screenshot comparisons** are byte-identical with frozen
match ticks. Four further moving bulk/checked runs complete at least 480 additional
ticks without assertion or renderer errors. Unsupported geometry and launcher
requests reject bulk; raster replacement releases GPU targets, and leaving
the session resets requested/applied diagnostics. Ordinary and zoom captures
also pass visual inspection.

All sampled renderer, zoom-trajectory and resource checks pass. Direct paths
stay within 128 cache entries/16 MiB, 8 MiB scratch and 2 MiB source storage,
with zero stream fallbacks and unknown stroke bounds. Live temperature
checkpoints are 50.15–54.0°C and frequencies 1.8–2.4 GHz under `ondemand`;
offscreen checkpoints are 45.75–56.2°C and 1.6–2.4 GHz. These are short runs
under the stock governor, not locked-frequency tests or full-match soaks.

### Regression coverage and artifacts

The client passes **676 GPU-feature tests** (four ignored) and **640 desktop
tests** (one ignored); the CLI passes **31 tests**. New tests cover strict
writer parsing, explicit probe opt-in, independent policies, idempotence,
pending-plan preservation, last-applied reporting and cleanup boundaries.
The tessellator passes **190 unit tests and seven doc tests**; all **31
CI-report tests** pass. Clippy completes with existing warnings outside the
new writer/control code. Formatting, diff, shell syntax and ShellCheck pass.

The workstation stock and bulk probes each pass all **439 exact cross-mode
checks**, including the six-path CPU oracle. All 180 exported PNG files match
across writers. The existing one-pixel cold/warm self-history difference occurs
symmetrically in reference and exact on AMD; it is not a writer mismatch. The
Pi's self-history comparisons are exact as well as its cross-mode comparisons.

Artifacts and asserting analyzers are under
`target/gpu-experiments/pi5-combined-strokes.GBiMSmhE/` locally and
`/data/spacewars/pi5-combined-strokes.1qfdNCN9/` on the Pi. The deployed release
client hash starts `a10035ffd979` and the CLI `c834ddde736a`. No renderer default,
saved preference, resolution, physics setting or other device is changed.
Clock is restored near 60 FPS/UPS with session-only triangles and CPU-shaped
text, reference joins, stock writer and camera sweep off. Saved preferences
and boot ID are unchanged; the kiosk has no renderer errors and zero automatic
service restarts. Both release executables remain in the device artifact
directory for future matched comparisons.

## Pi 5 concrete stroke output

Concrete stroke output reduces close-up encoding CPU **10.9%** and completed
drawing wall time **6.2%** against dynamic bulk output with exact joins in the
same Pi 5 release. All four comparison groups improve with identical geometry.
This is an isolated drawing result, not a whole-application CPU percentage.
The subsequent live comparison measures **3.1% less process CPU** than dynamic
bulk in both ordinary views and zoom, and **4.6% less than the preceding
executable**. Aligned RGBA remains cheaper on this Pi 5, so this stays an
opt-in geometry improvement, not a reason to change the rendering default.

The experiment retains the output writer's concrete type throughout polygon
and line tessellation. The previous bulk writer hoisted color work and
grouped index writes, but the stroker still called its output through a trait
object for every vertex and triangle. Concrete dispatch lets the compiler
specialize and inline that shared algorithm without changing its geometry.

### Selection and scope

`host gpu-stroke-writer bulk-static` selects concrete bulk output in an active
`retained-culled` session. `bulk` is the same writer with dynamic dispatch;
`stock` remains the default. All three independently combine with
`host gpu-joins reference|exact|checked`. Circle builders and retained templates
keep their existing path. There is no rectangle-specific path, tolerance
change, subpixel simplification, new cache key, saved setting or environment
switch. Existing pending-plan, successful-applied reporting, mode/session
reset, renderer-failure and resource-budget rules still apply.

The vendored stroker adds `tessellate_static`, a fixed-width path-event
entrypoint. Its output helpers and `StrokeBuilderImpl` become generic over
the output type; ordinary APIs still instantiate trait-object output. Both
paths share point merging, flattening, joins, caps, vertex state updates,
triangle emission and rollback code. The release code section grows from
21,296,628 to 21,329,872 bytes, about 33 KB or 0.16%; this is not a duplicate
stroke algorithm maintained in the client.

Use the existing probe with an additional explicit switch:

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-round-joins \
  --geometry-stroke-bulk --geometry-stroke-static \
  --benchmark-width 800 --benchmark-height 480 \
  --presentation-frames 128 --presentation-repeats 2 \
  --geometry-output /path/to/new-captures
```

Dropping only `--geometry-stroke-static` gives the dynamic bulk control;
dropping both writer switches gives stock output. Each process compares
reference and exact joins on the same 24-frame production histories with
24 warmup frames. Separate CPU-encoding and completed-GL samples exclude
simulation, source generation, projection, UI/text, readback and scanout.
Completed wall time includes submission and `glFinish`, not isolated GPU time.
The independent untimed CPU oracle checks all **nine** writer/join combinations
for vertex bits, indices, batches, cache decisions and enabled join counters.

### Regression coverage

The client passes **677 GPU-feature tests** (four ignored) and **641 desktop
tests** (one ignored). The CLI passes **31 tests**, Lyon **191 unit tests and
seven doc tests**, and the CI-report helpers **31 tests**. Concrete-output
tests compare all vertex fields and lifecycle calls for curves, open/closed
and empty subpaths, every cap/join style, and first/middle/last-vertex failures.
Existing prefixes survive aborts, and the same tessellators/builders can be
reused. The difficult-polygon corpus now checks static output for each join
policy, including folds, repeated points, concavity and self-intersections.

The desktop probe passes all 439 cross-mode image checks and its nine-path
CPU oracle. The previously recorded one-pixel AMD self-history difference
appears symmetrically in reference and exact, not between dispatch modes.
Clippy completes with the existing 25 client warnings; formatting, diff,
shell syntax and ShellCheck pass.

### Isolated Pi results

Sixteen processes form four reordered groups of the previous executable,
new stock, new dynamic bulk and new concrete bulk. The previous executable
uses stock output; every process compares reference/exact joins on identical
histories. The analyzer verifies all **49,152 samples**, **7,088 exact image
checks** including self-history, and **960 exported reference/exact image
pairs**. New-build reference images also match the previous executable.
Vertex/index/upload/admission counts and enabled join counters match at every
full replay position, not just modulo the history length.

Close-up results at logical 800×480, in milliseconds:

| Output path | Reference encoding CPU | Reference completed wall | Exact encoding CPU | Exact completed wall |
| --- | ---: | ---: | ---: | ---: |
| Previous executable, stock | 3.257 | 5.510 | 2.948 | 5.166 |
| New executable, stock | 3.196 | 5.480 | 2.857 | 5.100 |
| New executable, dynamic bulk | 3.139 | 5.432 | 2.814 | 5.091 |
| New executable, concrete bulk | 2.815 | 5.019 | 2.509 | 4.774 |

Without the join shortcut, concrete versus dynamic bulk also improves
encoding by 10.3% and completed drawing by 7.6%. The static/exact encoding
group means range from 2.499–2.532 ms versus dynamic/exact 2.809–2.825 ms;
completed drawing ranges from 4.711–4.821 ms versus 5.042–5.152 ms. Keeping
the same-build dynamic control separates output specialization from the
smaller build-to-build shifts visible in stock mode. The original arithmetic
is shared, but these results include the compiler's inlining and code-layout
effects; they do not isolate the cost of each indirect call.

Ordinary views show much smaller changes. With exact joins, early-view
encoding is 0.397 → 0.388 ms and completed drawing 2.066 → 2.072 ms; later
encoding is 1.432 → 1.410 ms and completed drawing 3.245 → 3.197 ms. The
first previous-executable early-view group is slower than its subsequent
groups and remains included; the main claimed gain uses the same-build
dynamic/concrete comparison. No sample or group is discarded.

Offscreen checkpoints range from 46.85–56.75°C and 1.6–2.4 GHz with the stock
`ondemand` governor. These are warm short rendering runs, not locked-frequency
tests. Probe screenshots of ordinary and close-up scenes also pass visual
inspection; text/UI is intentionally excluded from this probe.

### Live results

Sixteen fresh processes compare aligned RGBA, the preceding executable's
bulk/exact path, new dynamic bulk/exact and new concrete bulk/exact. Each
camera mode runs forward then reversed order. Every process uses an isolated
configuration, Spacewars seed 0, no asteroids, human P1, legacy rule-bot P2,
800×480 at 1×, CPU-shaped text, and no performance overlay. Direct paths use
`retained-culled`, bucketed streams, prepared transforms, pooled source points,
conservative stroke rejection and exact joins. The preceding executable is
**bulk/exact here**, unlike its stock-output isolated control above.

Measurements warm to tick 480, then collect twelve published 120-frame windows
through tick 1920. Actual intervals span 24.17–24.53 seconds and roughly ticks
486–1950. Process CPU comes from `/proc` CPU ticks divided by actual elapsed
time; 100% represents one core. All paths remain near 60 FPS/UPS, so frame rate
alone does not reveal the improvement.

| Path | Ordinary CPU % | Zoom CPU % | Ordinary CPU ms/frame | Zoom CPU ms/frame |
| --- | ---: | ---: | ---: | ---: |
| Aligned RGBA, new build | 30.06 | 31.57 | 5.027 | 5.256 |
| Previous build, bulk/exact | 34.57 | 41.81 | 5.717 | 6.988 |
| New build, dynamic bulk/exact | 34.05 | 41.14 | 5.649 | 6.878 |
| New build, concrete bulk/exact | 32.99 | 39.86 | 5.485 | 6.633 |

Concrete output saves 1.06 CPU percentage points in ordinary views and 1.28
in zoom against the same-build dynamic writer: **3.12% and 3.10%** relative
reductions. CPU per submitted frame improves 2.90% and 3.57%. Both repeats
improve; ordinary CPU ranges are 32.81–33.17% versus 34.03–34.08%, and zoom
39.78–39.94% versus 41.05–41.22%. Against the previous bulk executable, total
process CPU falls 4.55% and 4.65%. Keep those build-to-build deltas distinct
from the same-build dispatch comparison.

Zoom KMS-loop CPU is 6.968 ms for the previous executable, 6.848 ms for new
dynamic bulk, 6.602 ms for concrete bulk and 5.247 ms for RGBA. Concrete output
therefore still leaves about **1.36 ms/frame** more loop CPU than RGBA in this
run. This is useful additional headroom, not a universal direct-GPU advantage.
Do not subtract isolated savings from older live baselines or call completed
GL wall time GPU-only execution time.

Four paused states switch through all nine writer/join combinations, giving
**32 nontrivial byte-identical screenshot comparisons** with frozen ticks.
Four additional moving concrete/checked runs each cover at least 480 ticks
without assertion or rendering errors. Unsupported-mode and launcher requests
reject the new policy; raster replacement releases GPU targets, and session
exit resets requested/applied state. Ordinary and zoom captures pass visual
inspection with real UI/text. All 192 sampled windows satisfy the zoom
trajectory, applied-policy and resource guards: 128 cache entries/16 MiB,
8 MiB scratch, 2 MiB source storage, zero stream fallbacks and unknown bounds.
Live checkpoints are 50.7–53.45°C and 1.8–2.4 GHz under `ondemand`.

### Artifacts and next checks

Reproduction scripts, raw samples, images, asserting analyzers and both release
executables are under `target/gpu-experiments/pi5-static-strokes.YQyJ8wHK/`
locally and `/data/spacewars/pi5-static-strokes.Ve0OZq6e/` on the Pi. The
deployed client hash starts `b6b0cae36df6` and CLI `e7db08f7e9f9`.
The kiosk returns to unpaused Clock with session-only GPU triangles and
CPU-shaped text, stock output, reference joins and camera sweep off. Saved
settings and boot ID are unchanged, with no rendering errors or automatic
service restarts. Other devices are untouched.

Keep all three output controls for comparison. Repeat the concrete-output
path on Pi 4 and broader scenes before promoting it. It is a worthwhile exact
optimization, but remaining stroke construction and the whole-frame gap to
RGBA still need attention; small-shape removal or weaker quality guards are
not justified by this result.

## Pi 4 concrete stroke output

The same concrete-output specialization reproduces a smaller isolated gain on
Pi 4: close-up encoding CPU falls 4.2% and completed drawing wall time 1.6%
relative to dynamic bulk with exact joins. All four reordered groups improve
for that fixture. Ordinary-view completed drawing is effectively unchanged.

The target is `sw-picade-2.local`, V3D 4.2.14.0 / Mesa 25.1.6, at its native
1024×768 size. `sw-picade.local` is on a separate software-only application
build and is left untouched. The two controlled executables are the same
`a10035ff…78ff5` and `b6b0cae3…b0d` AArch64 releases used in the preceding Pi 5
experiment, not the unrelated application previously installed on the Pi 4.
The renderer source hashes still match that build. This comparison does not
include the subsequently added broader-fixture probe executable.

Sixteen fresh processes use four reordered groups of previous release, new
stock, new dynamic bulk and new concrete bulk. Each process measures both
reference and exact joins, with the previous release using its stock writer
for this isolated comparison. The replay histories, warmup, 128 timed frames
and two repeats match the Pi 5 protocol. Kiosk rendering is suspended during
the probe. The `ondemand` governor is unchanged; sampled temperatures are
66.7–70.6°C and frequencies 0.9–1.5 GHz.

| Fixture with exact joins | Bulk encoding CPU ms | Concrete encoding CPU ms | Bulk completed wall ms | Concrete completed wall ms |
| --- | ---: | ---: | ---: | ---: |
| Early bots | 1.430 | 1.395 | 9.403 | 9.385 |
| Later bots | 4.156 | 4.117 | 11.904 | 11.942 |
| Close-up zoom | 4.026 | 3.856 | 12.294 | 12.092 |

Early and later completed means change by −0.2% and +0.3%, respectively,
with mixed group directions. These are not established end-to-end gains.
For the close-up fixture, new stock/exact costs 3.993 ms encoding and 12.300 ms
completed drawing; the previous executable's stock/exact costs 4.042 and
12.325 ms. The concrete result is therefore not merely recovering a slower
same-build dynamic-bulk baseline. Keep cross-device comparisons qualified:
the Pi 4 and Pi 5 use different resolutions as well as different hardware.

All 49,152 timed samples preserve geometry counts, uploads, admissions and
join selection across matching controls. The nine CPU writer/join combinations
preserve vertex bits, indices, batches and cache decisions; all 7,088 image
checks and 960 exported reference/candidate pairs are exact, including
comparisons against the previous executable. Saved settings, binary hashes
and boot ID are unchanged through the isolated run, and the Clock resumes
with zero automatic service restarts.

Artifacts are in `target/gpu-experiments/pi4-static-strokes.UcYlv1z6/device/`,
with `compare-summary.json` and strict analysis/runner scripts in the parent.
The device copy is `/data/spacewars/pi4-static-strokes.BaTwMK9y/`.

### Pi 4 live comparison

The live matrix uses fresh processes with ordinary cameras and the same
tick-driven continuous zoom sweep as the Pi 5 trial. It compares aligned RGBA,
the previous executable's **bulk/exact** path, and the current executable's
dynamic/concrete bulk with exact joins. This differs deliberately from the
isolated previous-release **stock** control above. Direct modes share bucketed
streams, prepared plans, pooled source storage and conservative stroke rejection;
all modes use CPU-shaped text, a 1× viewport and a disabled performance overlay.
Each private session uses seed 0, human P1, rule-bot P2 and no asteroids.

| Camera | Dynamic bulk CPU ms/frame | Concrete bulk CPU ms/frame | Dynamic bulk process CPU | Concrete bulk process CPU |
| --- | ---: | ---: | ---: | ---: |
| Ordinary | 12.381 | 12.268 | 73.69% | 73.57% |
| Continuous zoom | 14.267 | 14.370 | 81.13% | 81.83% |

The ordinary CPU/frame mean improves 0.9%, but zoom worsens 0.7%; process CPU
is effectively flat ordinarily and slightly worse during zoom. The previous
bulk/exact release uses 12.220 / 14.307 ms per frame, respectively. These short
live runs do **not** reproduce the Pi 5's approximately 3.1% process-CPU win.
KMS-loop CPU averages move from 11.937 → 11.715 ms ordinarily and
13.869 → 13.797 ms during zoom; a small isolated or draw-loop improvement is
not evidence of a meaningful whole-application gain here.

The aligned RGBA comparison uses 14.192 / 14.945 ms of process CPU per rendered
frame for ordinary/zoom cameras, versus concrete bulk's 12.268 / 14.370 ms.
However, submitted zoom frames average 59.54 FPS for RGBA versus 56.95 for
concrete bulk; simulation remains near 60 updates/second in both. Lower CPU
cost does not establish better presentation pacing. The next comparison
should account for complete frame scheduling, submission and waiting rather
than subtracting isolated tessellation savings from the live frame budget.

The accepted matrix contains 16 processes and 192 status windows. Actual
measurement intervals are 24.11–24.54 seconds, with sampled temperatures
66.7–72.5°C and unchanged `ondemand` frequencies of 1.0–1.5 GHz. Each mode has
two fresh processes per camera condition; these are short controlled runs,
not a full-match endurance claim.

One original raster/RGBA zoom run fails the existing target-tick acceptance
window. Its last three status replies jump beyond the requested ticks; it
covers 68.69 seconds and 4,072 simulation ticks instead of the intended roughly
24 seconds and 1,440 ticks. The game advances about 4,030 rendered frames in
that interval, so the long measurement is not evidence of a corresponding
rendering stall. That run is retained in the analysis as `excluded`, with raw
CPU/frame totals and missed-tick details, and is not averaged into the matched
matrix. A fresh repeat of that reference uses a five-second CLI deadline and
checks tick overshoot during collection. It passes. The cause of the original
telemetry delay is unresolved; do not infer it from CPU usage alone.

All 32 paused non-self screenshot comparisons across the nine writer/join
combinations are byte-identical. Four moving concrete/checked runs each add
at least 480 simulation ticks without errors. Unsupported-mode rejection,
return-to-launcher resets, GPU resource retirement and cache/scratch limits
pass. The complete live data is under `live/`, with the separate control repeat
under `live-recheck/` on the device. Keep these rendering policies opt-in.

## Broader exact stroke probe

`--geometry-stroke-cases` reuses existing fixtures to compare the exact stroke
controls outside the moving Spacewars replay. It is benchmark-only, requires
`--geometry-round-joins`, and does not alter any live rendering default.

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-round-joins \
  --geometry-stroke-bulk --geometry-stroke-static \
  --geometry-stroke-cases clock-idle,clock-rain,pizza-300,pizza-5000,mixed-interleaved \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 128 --presentation-repeats 2 \
  --geometry-output /data/spacewars/new-stroke-captures
```

The output directory must not exist. Remove only `--geometry-stroke-static`
to measure dynamic bulk, and remove both writer flags for stock output. Each
invocation compares reference and exact joins with alternating order. Pause
other rendering work on the target before collecting isolated measurements.

| Fixture | Source | History |
| --- | --- | --- |
| Clock idle and rain | Seed 7 production benchmark at tick 600 | One frozen frame each |
| Pizza 300 and 5000 | Seed 7 production benchmark at tick 120 | One frozen frame each |
| Mixed interleaved | Existing prescribed animation of 576 shapes | All 32 poses, including camera/style changes |

Simulation, source generation and projection happen before measurement. The
probe shares the Spacewars stroke timing loop: at least 24 warmup frames,
separate CPU encoding and completed-GL measurements, no per-piece timers,
and no text/UI, readback or scanout inside the timed interval. Completed-GL
wall time includes CPU submission and completion waiting; it is not GPU-only
execution time. These frozen Clock/Pizza results do not measure live physics,
water simulation, presentation pacing or process CPU.

Outside timing, two traversals validate every pose against all nine
stock/dynamic-bulk/concrete-bulk and reference/exact/checked CPU combinations.
Vertex bits, indices, batches and cache decisions must agree; the selected
writer's reference/exact GPU pixels must also agree. The manifest records
source checksums, snapshot ticks, object counts, omitted text and history
lengths. The complete five-fixture run produces 72 image comparisons plus
10 saved reference/candidate pairs. Runs with different writers can compare
those exported images and every timed frame's geometry counts directly.

The workstation check passes all three writers: 216 exact image comparisons,
30 exact exported pairs, and identical counts, admissions and join selection
across 7,680 timed samples. The added history test checks both a frozen Clock
snapshot and the complete moving mixed-shape sequence. GPU-feature tests pass
679 cases (four ignored), desktop client tests pass 642 (one ignored), and
Clippy reports the same 25 existing client warnings. Renderer implementation
and live defaults are unchanged by this probe extension.

### Pi 5 broader scene results

Six fresh processes on `spacewars.local` use stock, bulk, concrete bulk,
concrete bulk, bulk, stock order at the HyperPixel's 800×480 logical size.
Each process measures 128 frames per control/path with two reordered repeats.
The installed kiosk is paused and suspended during the isolated probe, then
resumed. The normal `ondemand` governor is unchanged; sampled temperatures are
49.6–55.1°C and frequencies 1.5–2.4 GHz.

The table compares dynamic and concrete bulk with exact joins in the same
probe executable. All times are milliseconds per rendered frame.

| Fixture | Bulk encoding CPU | Concrete encoding CPU | Bulk completed wall | Concrete completed wall |
| --- | ---: | ---: | ---: | ---: |
| Clock idle | 0.379 | 0.373 | 1.686 | 1.677 |
| Clock rain | 1.078 | 1.075 | 2.574 | 2.564 |
| Pizza 300 | 0.888 | 0.885 | 2.139 | 2.133 |
| Pizza 5000 | 9.279 | 9.256 | 13.700 | 13.585 |
| Moving mixed shapes | 0.690 | 0.662 | 1.770 | 1.720 |

Clock and Pizza completed drawing changes by less than 1%; treat these as
effectively unchanged, not established wins. The mixed-shape animation saves
4.2% encoding CPU and 2.8% completed wall time, with both process repetitions
improving. Circles still use the same output dispatch; this result does not
justify claiming that their renderer became faster.

All 30,720 timed samples preserve geometry counts, admissions and join
selection across writers. All 432 image checks and 60 exported pairs are
exact, including comparisons against stock output. The source checksum and
history metadata agree across processes. A cleanup-script filename error made
the final settings check return failure after measurements; the original log
is retained, the script is corrected, and a separate check verifies the actual
saved settings, binaries, boot ID and healthy resumed Clock. No timing sample
was discarded or rerun to obtain a favorable result.

The probe executable is `02f347ea…64afb`; the installed application remains
`b6b0cae3…b0d`. Artifacts, manifests, scripts and the strict postcheck are under
`target/gpu-experiments/pi4-static-strokes.UcYlv1z6/`, with the Pi 5 data in
`pi5-broad/` and summary in `pi5-broad-summary.json`. The device copy is
`/data/spacewars/pi5-broad-strokes.StaDySWU/`.

### Pi 4 broader scene results

The same probe executable and six-process order run on `sw-picade-2.local`
at 1024×768. The kiosk is suspended for the isolated measurements and resumed
afterward. The unchanged `ondemand` governor samples 0.9–1.5 GHz and
temperatures 67.7–70.1°C. The following same-build comparison again uses
exact joins and milliseconds per rendered frame.

| Fixture | Bulk encoding CPU | Concrete encoding CPU | Bulk completed wall | Concrete completed wall |
| --- | ---: | ---: | ---: | ---: |
| Clock idle | 0.974 | 0.949 | 8.020 | 8.012 |
| Clock rain | 2.729 | 2.722 | 10.196 | 10.107 |
| Pizza 300 | 2.719 | 2.673 | 9.056 | 9.049 |
| Pizza 5000 | 26.954 | 26.772 | 41.968 | 41.718 |
| Moving mixed shapes | 2.099 | 2.002 | 7.930 | 7.851 |

Mixed-shape encoding improves 4.6% and completed drawing 1.0%, with both
process repetitions improving. Clock idle encoding improves 2.5%, but its
completed drawing remains flat. All Clock/Pizza completed changes are below
1%; treat them as effectively unchanged. The larger absolute times than on
Pi 5 also include a different viewport size, not merely different hardware.

All 30,720 timed samples preserve geometry counts, uploads, admissions and
join selection across writers; all 432 image checks and 60 exported pairs
are exact. Source/history metadata agrees, and saved settings, installed
binaries and boot ID remain unchanged throughout the probe. The runner exits
successfully with Clock resumed and zero automatic service restarts.

The Pi 4 data and lifecycle checks are in `followup/`, with
`broad-summary.json` in the artifact root above. After all measurements, the
normal fast-update installs `02f347ea…64afb` on this Pi 4. Its initial
filesystem `sync` takes several minutes before replacement begins; the old
application keeps running during that wait. The update then completes with
PID 27609, no reboot, zero automatic restarts, matching binary hashes and
unchanged saved preferences. A fresh screenshot verifies Clock running with
direct triangles and CPU-shaped text as session-only overrides. The default
stroke writer and join policy remain stock/reference. Deployment evidence is
in `final-deploy.log`, `final-device-updated.txt` and
`final-clock-updated.png`. The cause of the slow flush is not established;
do not assume it explains the earlier telemetry delay.

Across these follow-ups, concrete stroke output gives a modest live Pi 5
benefit, but no clear live Pi 4 benefit. Broader exact-output checks find no
regression, while most completed Clock/Pizza timings remain effectively
unchanged. Keep all controls opt-in. The next useful investigation is
remaining whole-frame work and the Pi 4 zoom pacing gap, preserving the RGBA
reference and separating CPU work from completion/scanout waits.

## Pi 4 frame pacing attribution

The direct-rendering zoom gap reproduces in frame-aligned traces. Average
CPU work is lower than aligned RGBA, but the direct path has many more frames
whose CPU work alone exceeds a 60 Hz frame budget. The short no-draw loop
iterations are not the main cost, and ordinary GPU swap/submission calls
remain small. This points toward reducing the expensive zoom frames rather
than changing the simulation rate or removing display synchronization.

The first matrix uses the installed `02f347ea…64afb` release on
`sw-picade-2.local`, native 1024×768, 1× rendering and the same seed-0 human-P1,
rule-bot-P2, no-asteroid fixture as the preceding live comparison. Direct mode
uses retained/culled geometry, bucketed streams, prepared plans, pooled source
storage, conservative stroke rejection, exact joins and concrete bulk output.
Both paths use CPU-shaped text and no performance overlay. Saved preferences
are untouched; each process gets a private configuration and control socket.

Twelve fresh processes compare RGBA/direct in reversed orders: four ordinary
camera runs, four zoom runs, and four zoom runs with detailed application/text
tracing. Four 240-loop captures per process start near simulation ticks 600,
960, 1320 and 1680. The target tick must be within 30 ticks and CLI requests
have five-second deadlines. An initial harness attempt checked telemetry
before startup had published it and aborted before any capture; correcting
that readiness check precedes the complete matrix. All 48 captures finish,
covering 11,520 loop iterations with unchanged binaries, preferences and boot
ID. The unchanged `ondemand` governor samples 1.1–1.5 GHz and 63.8–70.1°C.

The basic trace includes per-GL-call clocks but omits application/text detail.
These are diagnostic results, not uninstrumented throughput measurements.
Drawing rate below is completed KMS draws divided by captured loop wall time,
not the host's submitted-scene counter or verified physical scanout rate.
CPU and waiting times are normalized to actual draws, retaining idle-loop
cost. Each row contains 1,920 loops across two processes.

| Camera and path | Draw loops/s | Loop CPU ms/draw | Loop wall p95 ms | Loops over 20 ms |
| --- | ---: | ---: | ---: | ---: |
| Ordinary RGBA | 60.05 | 14.093 | 16.704 | 2 |
| Ordinary direct | 59.98 | 12.338 | 16.722 | 6 |
| Zoom RGBA | 60.06 | 14.504 | 16.706 | 2 |
| Zoom direct | 57.98 | 13.997 | 21.793 | 154 |

Of the direct zoom loops, 301 exceed 16.667 ms of CPU work. Of its 154 loops
over 20 ms wall time, 148 also exceed that CPU budget. Those slow loops average
20.316 ms CPU, including 13.481 ms building drawing items and the direct scene,
versus 7.880 ms for that stage across all zoom loops. Their page-flip wait
averages 2.488 ms, EGL swap 0.191 ms, and buffer-upload wall time 0.334 ms.
The intervals do not support treating every long frame as a GPU/driver wait.

The 38 no-draw direct zoom loops contribute only 0.033 ms per actual draw;
total event-dispatch wall time is 0.116 ms/draw. These wakeups are real but too
small to explain the overall gap. Average page-flip waiting is 3.141 ms/draw
for direct zoom and 2.039 ms/draw for RGBA. Such waiting can include ordinary
display pacing; reducing it without knowing presentation deadlines is not
automatically an optimization.

Detailed traces corroborate the expensive stage while also demonstrating
measurement overhead: direct zoom rises from 13.997 to 14.811 ms CPU/draw,
and RGBA from 14.504 to 15.447 ms. In detailed direct traces, frames over
20 ms wall time spend 11.614 ms CPU in the application's scene-drawing callback,
versus 6.045 ms across drawing frames. Projection rises 1.845 → 2.216 ms,
while buffer uploads and GL draw calls remain around 0.30 and 0.65–0.70 ms.
These stages nest; do not add them as independent costs. This localizes the
burst primarily inside scene encoding/drawing, but does not yet separate
individual tessellation, cache and vertex-output work on those exact frames.

Artifacts, strict analysis, private configurations and lifecycle checks are
under `target/gpu-experiments/pi4-frame-pacing.l5npeIsR/`, with the original
matrix in `data/` and `summary.json`. The device copy is
`/data/spacewars/pi4-frame-pacing.B1P7qB90/`. The startup-only failed attempt
is retained separately in `initial-attempt/`.

### Kernel page flip confirmation

Trace version 5 adds the kernel vblank sequence and timestamp from the existing
page-flip event, plus CPU-relative loop-start, submission and observation times.
It does not add a fence, readback, event wait or another queued frame. The
event-clock reads are active only during an explicit bounded capture; the
sample structures have a small fixed storage increase. Unit tests cover the
capture epoch, absent versus zero-valued events, cancellation, the 240-loop
limit and inactive hooks. Kernel and CPU timestamps have independent epochs;
only intervals within each clock domain are compared.

Four fresh processes repeat the basic zoom fixture in RGBA/direct/direct/RGBA
order using the same instrumented executable, `51261425…55712f`. All sixteen
240-loop captures complete, totaling 3,840 loops. Each mode has eight captures
across two processes. Presentation rates below use consecutive kernel events
within each capture, excluding gaps between captures. The unchanged governor
samples 1.2–1.5 GHz, with temperatures 66.7–69.1°C.

| Zoom path | CPU ms/draw | Kernel flip intervals | Flips/s | Intervals skipping a refresh | Longest flip interval ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Aligned RGBA | 14.541 | 1,901 | 60.004 | 0 | 16.688 |
| Direct geometry | 14.060 | 1,876 | 57.697 | 75 | 33.332 |

All 75 direct gaps span two refreshes. With one outstanding page flip, the
event consumed in a drawing loop belongs to the previous drawing loop's
submission. Of those 75 preceding frames, 74 exceed the panel period in CPU
work alone. They average 20.960 ms CPU but only 0.017 ms waiting for the previous
flip. The remaining frame uses 16.405 ms CPU and 17.747 ms wall time, also
longer than one refresh. This confirms that actual presentation gaps accompany
the expensive frames; extra event-loop iterations are not simply lowering a
misleading frame counter. It does not establish GPU execution time or
controller-to-display latency, and average CPU below 16.667 ms is still not
enough to meet every presentation deadline.

The next target is the worst-case scene encoding/drawing identified above:
associate cache, tessellation and vertex-output work with those exact slow
frames before choosing another optimization. Keep display synchronization,
queue depth, simulation rate and renderer defaults unchanged. Preserve both
the aligned RGBA control and untraced comparisons; these captures still carry
the basic trace's diagnostic overhead.

Two preceding attempts using USB-backed `/data` did not complete the full
matrix. One trace read exceeded its five-second deadline; another process
missed the first capture's target-tick window. Separate filesystem operations
also stalled while the game continued advancing normally. Those partial runs
are retained in `flips/` and `flip-repeat/`, not pooled into the table. Moving
the private executable, configuration and capture files to RAM-backed `/tmp`
allowed a complete repeat. This isolates a practical collection path, not
the root cause of the device's storage/CLI stalls.

The completed data is in `flip-ram/`, with strict validation in
`flip-ram-summary.json` and the runner/analyzer alongside the original matrix.
Its temporary device directory is `/tmp/pi4-flip-pacing.TXG3KTSy/`; the local
copy is retained independently. The runner verifies unchanged installed
binaries, saved preferences and boot ID, then restores the kiosk with zero
automatic restarts. No incomplete run is reported as a successful matrix.

Validation passes 44 standalone LinuxKMS tests and 679 GPU-feature client
tests, with one and four existing ignored tests respectively. Workspace and
edited-file formatting checks pass. A final hook-bookkeeping cleanup is
cross-built and deployed as `475c1a5b…fce585`; the benchmark table above remains
the `51261425…55712f` capture, not a speed comparison between these builds.
The installed build completes a fresh 240-loop Clock trace with valid flip
fields and no modeset/read errors. Reading again after cancellation preserves
the complete result byte-for-byte. Binary hashes match the bundle, saved
preferences and boot ID remain unchanged, and the kiosk has zero automatic
restarts. `final-clock-updated.png` verifies the rendered Clock with direct
triangles and CPU-shaped text as session-only overrides. Deployment and smoke
evidence are in `final-deploy.log`, `final-device-updated.txt` and
`final-clock-trace.txt`; renderer and pacing defaults are unchanged.

## Live geometry attribution during zoom

The frame-aligned geometry follow-up separates occasional cache construction
spikes from the more common guarded-stroke workload. Cache churn produces the
largest sampled frames, but most missed refreshes happen with no cache miss.
Reducing one will not eliminate the other.

The same Pi 4 zoom fixture runs in eight fresh processes, ordered RGBA/basic,
direct/basic, direct/work, direct/geometry, then reversed. Each process takes
four 240-loop captures near ticks 600, 960, 1320 and 1680. All 32 captures
complete: 7,680 loops with matching binary hashes, unchanged preferences/boot
ID, no renderer errors and no shader compilation inside capture windows.
RAM-backed configuration and capture files avoid the preceding collection
path through USB storage. The unchanged `ondemand` governor samples 1.2–1.5 GHz
and temperatures 64.8–69.1°C. Each row combines two processes and 1,920 loops.

| Zoom path and trace | Loop CPU ms/draw | Kernel flips/s | Intervals skipping a refresh |
| --- | ---: | ---: | ---: |
| RGBA, basic | 14.468 | 59.941 | 2 |
| Direct, basic | 14.010 | 57.799 | 72 |
| Direct, application/text detail | 14.700 | 57.739 | 74 |
| Direct, plus encoding detail | 15.151 | 56.803 | 106 |

These are diagnostic timings, not an optimization comparison. Encoding detail
adds about 0.45 ms CPU/draw beyond application/text detail in this matrix.
Different draw rates also sample slightly different camera positions within
each window; do not treat that difference as an exact timer-cost calibration.
The basic path copies existing geometry/cache counters and coarse encoding
times, without the extra per-shape clocks or diagnostic visibility scan.

In basic direct captures, all 72 presentation gaps follow frames above the
CPU budget. Only ten of those frames have cache misses; 62 have none.
Nevertheless, the largest frame spends 40.771 ms CPU, including 30.167 ms wall
time in geometry encoding and 21.427 ms in cache construction. It admits and
evicts 60 entries, all misses at different scales of already-resident source
geometry. Several other large frames spend 14–17 ms constructing entries.
Across all direct draws, construction averages only 0.222 ms, illustrating
why an average hides this problem.

The other slow frames contain substantially more guarded outlines and output
geometry. Basic direct loops above 20 ms wall time average 3,183 guarded
polygons and 70,954 vertices, versus 906 and 44,572 across all draws. The
detailed capture confirms where that work goes: actual fallback stroke calls
rise from 589 per draw to 2,531 in slow loops, and their wall time rises
1.355 → 5.478 ms. Tessellation/output inside that stage accounts for
1.082 → 4.350 ms. Scalable cached-vertex expansion rises 0.792 → 1.032 ms;
validation rises 0.326 → 0.548 ms. Ordinary unretained drawing stays around
0.74–0.77 ms. These stages nest and must not be summed indiscriminately.
The additional visibility scan itself costs 0.046 ms/draw, or 0.190 ms in
slow loops, and exists only in the detailed diagnostic path.

These measurements motivate the [settled admission experiment](#settled-scale-admission-experiment).
With the original policy, the
hybrid considers a scale seen in any earlier frame eligible for an exact-scale
entry; the probation record does not require consecutive/stable use. Returning
to a scale during a periodic zoom can therefore trigger promotion while the
camera is still moving. A consecutive-use admission control targets
exceptional rebuild spikes, not the separate work
needed to reduce repeated guarded-stroke tessellation. No admission, renderer,
queue-depth, pacing or simulation default changes in this follow-up.

The pilot's paused basic/detailed PNGs are byte-identical. A new unit test also
checks exact vertices, indices, painter order and cache decisions for profiled
versus ordinary clipped geometry using conservative rejection, exact joins
and concrete bulk output. Validation passes 680 GPU-feature client tests,
46 standalone backend tests and 31 CLI tests; existing ignored tests remain
four client and one backend. Formatting and diff checks pass.

Artifacts are under `target/gpu-experiments/pi4-geometry-trace.s9oi5pRL/`:
`smoke/` holds the pilot and frozen comparison, `data/` the complete matrix,
and `summary.json` the strict analysis. Device copies are
`/tmp/pi4-geometry-smoke.zoCIGYpj/` and
`/tmp/pi4-geometry-matrix.iIOI4Ups/`; local copies survive a reboot.
All measurements use client `6bce1871…325120` and CLI `8098e741…407612`.

That instrumentation build was installed on `sw-picade-2`; the kiosk remained active with
zero automatic restarts, the same boot ID and unchanged saved settings.
A final Clock smoke capture completes all 240 loops (232 draws and eight idle
loops), with no modesets, flip-read errors or shader compilation. Plain
triangles report coarse encoding measurements but correctly leave the
unsupported retained-detail fields empty, even when geometry detail is armed.
Repeated reads of the completed trace are identical. The fresh 1024×768 Clock
screenshot passes visual inspection with direct geometry and CPU-shaped text
selected for this session. `final-device.txt`, `final-clock-trace.txt`,
`final-clock-summary.json` and `final-clock.png` retain these checks alongside
the matrix. No other device was changed.

## Settled scale admission experiment

`host gpu-admission repeated|settled` adds a session-only control to
`retained-culled`. The default `repeated` policy is unchanged: a scale observed
in any earlier presentation can qualify for an exact-scale entry. `settled`
requires the same immutable geometry and exact scale in three consecutive
presentations. A gap restarts the count, and multiple placements or cameras
in one presentation count only once. This is a bounded reuse heuristic, not
proof that a camera has stopped moving. It does not quantize scale, simplify
geometry, change stroke widths or alter simulation.

Existing entries remain reusable under either policy. Switching policies
clears probation observations, not completed meshes; repeating the same
request is idempotent. Fresh-process comparisons are therefore necessary to
measure different admission histories. Probation stays bounded to 128 weak
identity records, and meshes retain the existing shared 128-entry/16 MiB
budget. Leaving the scene, GPU failure or renderer teardown resets the control;
unsupported modes cannot enable it. Requested and last-successfully-applied
policies have separate status fields.

```sh
spacewars-cli host gpu-scene retained-culled
spacewars-cli host gpu-admission settled
spacewars-cli host camera-zoom sweep-stop
```

The new `sweep-stop` fixture follows the existing opposite-phase 44–640-unit
camera sweep until simulation tick 1920, then holds both heights. Camera
centers keep following their normal subjects, and physics, saved settings and
scenario camera history remain untouched. This provides repeatable moving,
transition and settled windows without timing a manual zoom release.

Unit coverage includes cyclic zoom and crossing views, duplicate placements,
third-frame promotion, immediate reuse of completed entries, gaps, replaced
geometry, weak ownership, bounds, idempotent requests and lifecycle resets.
The cyclic fixture compares exact vertex/index buffers and painter order
against the existing policy even when their admission decisions diverge.
All 688 GPU-feature client tests and 32 CLI tests pass; four existing client
tests remain ignored. No renderer or admission default changes.

### Pi 4 comparison

Eight fresh processes run `repeated, settled, settled, repeated` twice on
`sw-picade-2`, with seed 0, a human P1, rule-bot P2, no asteroids, 1024×768
at 1×, and the preceding bucketed/prepared/pooled/conservative/exact/bulk-static
controls. Each process takes six basic 240-loop traces: moving near source
ticks 600, 960 and 1320, the stop transition near 1860, then settled near 2220
and 2580. All 48 captures complete, totaling 11,520 loops. No detailed encoding
clocks are enabled. The unchanged `ondemand` governor samples 1.1–1.5 GHz,
and temperature samples range from 64.8 to 70.1°C.

| Phase | Repeated CPU ms/draw | Settled CPU ms/draw | Repeated skipped refreshes | Settled skipped refreshes |
| --- | ---: | ---: | ---: | ---: |
| Moving zoom | 13.543 | 13.314 | 104 | 85 |
| Stop transition | 14.951 | 14.891 | 25 | 24 |
| Settled views | 14.696 | 14.668 | 42 | 42 |

Moving-zoom CPU improves **1.69%**, with all four adjacent block pairs improving
1.26–2.60%. Encoding wall time falls 4.178 → 4.044 ms/draw. The larger benefit
is eliminating repeated cache construction during these moving windows:
**2,246 misses/evictions become zero**, and maximum construction wall time
falls **21.590 → 0 ms**. The largest sampled moving CPU frame falls
**42.270 → 26.547 ms**; maximum encoding wall time falls 31.254 → 12.898 ms.
Kernel presentation rises 57.878 → 58.263 flips/s. These are sampled maxima,
not worst-case guarantees; different cadences also sample slightly different
camera positions. One pair's total CPU maximum is slightly higher despite
having no cache builds, because other frame work still spikes.

Stopping still incurs useful warmup. Each settled-policy run promotes 66
entries, with a largest construction burst of **16.274 ms** and a roughly
30.3 ms total CPU frame. The repeated control has a similar transition peak.
Steady-view CPU, encoding and refresh gaps are effectively unchanged; both
policies incur 80 later misses across the settled captures. The recurring
approximately 42 ms settled CPU spikes spend about 34 ms inside timer callbacks
and only 2.5 ms encoding geometry, so this cache rule cannot fix them.

All 85 remaining moving presentation gaps under `settled` follow frames above
the refresh budget in CPU work alone, with no cache miss. Repeated guarded
outline work therefore remains a separate target. Keep this policy opt-in
until broader moving-image and Pi 5 coverage support a default change; it is
a targeted spike reduction, not a general 60 Hz guarantee.

All eight warm-cache policy-switch screenshot pairs are byte-identical, and
launcher resets pass after every process. These images validate switching at
a frozen state, not every moving admission history. Captures contain no shader
compilation, modesets, flip-read errors or renderer errors. Saved preferences,
boot ID and installed binary hashes remain unchanged during the matrix.
The software-only `pi-kiosk` build check also passes; GPU clippy completes with
the existing warnings, and formatting/diff checks pass.

Artifacts are under `target/gpu-experiments/pi4-settled-admission.S1XtaI56/`.
`data/` and `summary.json` contain the complete comparison; `pilot/` is a
separate successful pilot. An earlier partial `smoke/` run is excluded: its
collector scheduled from the periodically published HUD tick and rejected a
late sample. The corrected collector uses `camera_zoom_source_tick`, updated
on every rendered source frame. Device artifacts are in
`/tmp/pi4-settled-matrix.MJk9N85d/`; the local copies survive a reboot.
The client is `9eb83236…793acf` and CLI `e578fc59…1c528f`.

The experimental build is installed on `sw-picade-2` without a reboot. Binary
hashes match the measured build, the service is active with zero automatic
restarts, and the boot ID and saved settings still match the pre-test state.
Clock is running with session-only direct triangles and CPU-shaped text; its
fresh screenshot passes visual inspection. The installed CLI correctly rejects
`settled` on the plain-triangle path. `final-device.txt`, `final-clock.png`,
`unsupported-mode.txt` and `deploy.log` retain these checks. No other device
was changed, and the admission default remains `repeated`.

## Admission image coverage and Pi 5 timing

Broader image validation supports the settled policy's rendering correctness,
and the Pi 5 repeats its reduction in moving cache-construction spikes. The
average CPU gain remains small. Keep the default `repeated` policy and use
`settled` as an explicit comparison control while targeting the recurring
guarded-outline cost separately.

### Moving image validation

The new validation-only probe compares independent `repeated` and `settled`
presenters against ordinary triangles on every presentation. Unlike the earlier
warm-cache screenshot switches, each policy starts with an empty cache and
can make different admission decisions throughout the history.

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-admission-check --benchmark-width 800 --benchmark-height 480 \
  --geometry-output /tmp/admission-images
```

The output directory must not already exist. This uses an explicit render-node
context, not scanout, and rejects software GL fallback. It prepares 96 source
frames for each of the three production Spacewars histories plus terrain reveal
and terrain edit/replacement. Each history is replayed twice with six extra
holds at two points, then resources are retired and recreated for six stopped
frames: **222 presentations per fixture, 1,110 overall**. Existing empty-frame,
nine-pass overflow and smaller-viewport transitions remain in the history.
Minimap compositing is included; text, UI, input latency and live producer
lifetimes are not. Prepared definitions remain alive for the replay, with the
existing 256 MiB accounted-input limit.

Every presentation compares both policies to ordinary triangles and to each
other, giving **3,330 image comparisons** per device. The existing tight gate
is unchanged: RGB MAE at most 0.05 and at most 0.1% of pixels with summed RGB
error above 30. Cache bounds, actual movement, differing admission decisions,
promotion, reuse, guarded fallback and GL errors are checked too. Timing
options are rejected; this probe is correctness evidence, not a benchmark.

The workstation RX 7800 XT at 1024×768 passes all comparisons, with 88
presentations taking different cache decisions and 1,108/1,110 policy pairs
byte-identical. The largest policy-to-policy RGB MAE is **0.000025007**;
the largest reference MAE is **0.000615777**.

The Pi 5 V3D 7.1.10.2 at 800×480 also passes all comparisons. It has 92
presentations with different decisions and 1,106/1,110 exact policy pairs.
Maximum pair MAE is **0.000051215**, with at most one pixel per image over
the summed-error threshold; maximum ordinary-reference MAE is **0.000604167**.

The Pi 4 V3D 4.2.14.0 at 1024×768 passes all comparisons too, with 88
presentations taking different decisions and 1,109/1,110 exact pairs. Maximum
pair MAE is **0.000075955**; maximum ordinary-reference MAE is **0.000773112**.
Both devices use Mesa 25.1.6. Their worst-pair captures and the workstation's
worst pair and edited terrain captures pass visual inspection. Across all
three GPUs, **9,990 image comparisons** pass the unchanged gate. These are
small edge differences, not a promise of pixel identity for all admission
histories or bit-identical simulations across architectures.

### Pi 5 live comparison

Eight fresh processes on `spacewars.local` repeat the preceding Pi 4 protocol:
`repeated, settled, settled, repeated` twice, six 240-loop basic captures per
process, three moving windows, one stop transition and two settled windows.
The complete **48 captures / 11,520 loops** pass the strict analyzer. Native
logical output is 800×480 at 1× on the rotated HyperPixel; this is not a
resolution-matched Pi 4 versus Pi 5 comparison. The governor remains `ondemand`,
with sampled clocks 1.6–2.4 GHz and temperatures 48.5–52.4°C.

| Phase | Repeated CPU ms/draw | Settled CPU ms/draw | Repeated skipped refreshes | Settled skipped refreshes |
| --- | ---: | ---: | ---: | ---: |
| Moving zoom | 6.567 | 6.509 | 0 | 0 |
| Stop transition | 6.708 | 6.664 | 0 | 0 |
| Settled views | 6.258 | 6.249 | 0 | 0 |

Moving CPU improves **0.89%** overall. Three adjacent block pairs improve
0.99–1.38%; one is 0.065% worse. Encoding wall time falls 2.620 → 2.571 ms/draw.
Moving cache misses fall **1,053 → 0**, maximum construction wall time
**7.475 → 0 ms**, and largest sampled moving CPU frame **15.371 → 13.295 ms**.
These are sampled maxima rather than worst-case guarantees. Both policies
present at approximately 60.057 flips/s, which hides their CPU difference.

The stop still needs cache construction, peaking at **9.035 ms** with settled
versus 10.146 ms with repeated. Both policies have 80 later misses in the
settled captures. Steady-view averages are essentially unchanged, and roughly
22 ms CPU outliers remain. This is a narrow cache-churn improvement, not a
solution to all frame-time spikes or a measured input-latency improvement.

All eight frozen warm-cache screenshot pairs remain byte-identical, launcher
resets pass, and the trace contains no shader compilation, modesets, flip-read
errors or renderer errors. Device preferences and boot ID are unchanged.
The timing matrix uses the same `9eb83236…793acf` client as the Pi 4 comparison;
the later `182f452d…812008` build adds the validation probe without changing
the renderer or admission policy. Keep those builds distinct when comparing
small performance differences.

The later build is deployed to the Pi 5 with CLI `e578fc59…1c528f`, without a
reboot. Clock is running normally with session-only direct triangles and
CPU-shaped text; the 800×480 scanout capture passes visual inspection. Saved
preferences are unchanged, the service has zero automatic restarts, and the
default admission control is still `repeated`. All **690 GPU-feature client
tests** pass, with four existing ignored tests. The software-only kiosk check,
GPU clippy with existing warnings, formatting and diff checks pass. The Pi 4's
installed binary is unchanged: its validation runs from RAM, and Clock resumes
in the same kiosk process with settings, binary hashes and boot ID intact.

Artifacts are under `target/gpu-experiments/admission-validation.NCIgruBo/`:
`native-quality.csv`, `native-quality-summary.json`, `pi5-quality/`,
`pi5-quality-summary.json`, `pi4-quality/`, `pi4-quality-summary.json`,
`pi5-matrix/` and `pi5-summary.json` retain the images, manifests, raw traces
and analysis. `run-traces.sh`, `run-quality.sh`,
the analyzers, source snapshots and `bundle-manifest.json` retain the protocol.
The Pi 5 copy is `/tmp/admission-validation.SBjwG9WM/`; local copies survive a
reboot. The Pi 4 copy is `/tmp/admission-quality.S4yfu7IW/`.
`deploy-pi5.log`, `final-pi5-*` and `final-pi4-status.txt` retain device checks.

## Direct polygon input experiment

The `bulk-polygon` stroke writer tests whether path-event dispatch contributes
meaningfully to the recurring guarded-outline cost. It sends polygon and line
endpoints directly into the same Lyon fixed-width builder. Geometry arithmetic,
merging, folding, caps, joins, emission order and per-endpoint error handling
are unchanged. This is neither geometry caching nor a rectangle fast path.
Circles, curves and retained templates keep their existing route; no cache or
memory budget changes. The default writer remains `stock`.

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-round-joins \
  --geometry-stroke-bulk --geometry-stroke-static --geometry-stroke-polygon \
  --benchmark-width 800 --benchmark-height 480 \
  --presentation-frames 128 --presentation-repeats 4 \
  --geometry-output /tmp/polygon-input-images
```

The last flag changes the pair to `bulk-static` versus `bulk-polygon`, both
using exact joins and conservative rejection. It does not also change join
policy. Omitting it preserves the existing reference-versus-exact benchmark.
The output directory must be new. Add `--geometry-stroke-cases
clock-idle,clock-rain,pizza-300,pizza-5000,mixed-interleaved` for the broader
fixtures. Live selection is `host gpu-stroke-writer bulk-polygon` within
`retained-culled`, with the existing session, failure and unsupported-mode
reset rules.

Validation compares all twelve writer/join combinations for vertex bits,
indices, batches and cache decisions, outside timed samples. The workstation
passes **443** moving/zoom/resize/edit/synthetic image comparisons plus **72**
broader-scene comparisons, all with identical pixels. The vendor's **193 tests**
pass in both debug and release, including direct-input cap/join, degenerate
polygon and output-error ordering checks. The client passes **692 tests** with
four existing ignored tests, and the CLI passes **32**. Software-only checking,
GPU clippy with existing warnings, formatting and diff checks pass.

The workstation's first close-up replay measures 0.5966 → 0.5853 ms encoding
CPU, a **1.9%** reduction. This is an uncapped prepared-geometry replay, not
whole-application CPU, FPS or input latency.

### Device comparison protocol

The Pi tests run the installed old executable, new executable twice, then old
executable again. Each new run compares the two input routes in alternating
order for four repeats of 128 frames, after warming the 24-frame source history.
Two further new-executable runs cover Clock idle/rain, Pizza 300/5000 and the
32-pose mixed-shape animation. CPU encoding and completed GL wall time are
separate scopes; simulation, scene generation, projection, text, UI and scanout
are outside both. CPU encoding has no per-piece timing clocks. The analyzer
rejects missing samples or differences in geometry counts, upload bytes, cache
admissions and join decisions.

On the Pi 5 at logical 800×480, close-up encoding CPU falls **2.5376 → 2.5241 ms**
against the same-build static input, **0.53%**, with all eight pairs improving
0.18–0.82%. Completed rendering is effectively unchanged: **4.7306 → 4.7287 ms**
(0.04%). Early/late ordinary-match encoding changes by less than 0.2%; the
broader fixtures change by at most 0.13% in either measured scope.

The old-executable control matters: its close-up encoding averages **2.4995 ms**,
so the new direct-input path is **0.98% slower** in this matrix, despite its
small same-build gain. The new static reference is 1.53% slower than the old
static reference. Completed close-up wall time versus the old executable is
only 0.13% better. These small differences do not establish a useful net win
or a whole-application benefit. Keep the control experimental, with no default
change. Pi 5 temperatures sampled 49.1–56.2°C; its unchanged `ondemand` governor
sampled 1.8–2.4 GHz before/after blocks, not a locked-frequency test.

All **1,030** new-build Pi 5 image comparisons pass with identical pixels,
including cold/warm caches, camera motion, zoom/resize, synthetic outlines and
terrain reveal/edit/replacement. The old-executable checks also pass. Clock
resumes in its existing kiosk process with unchanged settings, installed
binaries and boot ID, and zero automatic restarts. Probe execution and captures
use RAM-backed temporary storage; no app installation or reboot is needed.

The Pi 4 at 1024×768 confirms the lack of a useful saving. Close-up encoding
CPU changes **3.8601 → 3.8543 ms** (**0.15%**), with five of eight pairs slower;
completed rendering changes **12.1504 → 12.1749 ms** (**0.20% slower**).
Against its old executable, close-up encoding improves only 0.42% and completed
wall time 0.08%. Early/late match encoding is 0.46%/0.17% slower than the
same-build reference and 2.65%/3.33% slower than the old executable. This is
not evidence for adopting the new path. Across the broader fixtures, encoding
changes range from 0.76% better to 0.28% worse, with every completed-wall mean
slightly worse (0.05–0.24%). These are native-viewport tests, not a
resolution-matched Pi 4 versus Pi 5 hardware comparison.

All **1,030** new-build Pi 4 image comparisons also pass with identical pixels.
Together with the workstation and Pi 5 this gives **2,575** new-path image
comparisons, plus the separate old-executable validation runs. Pi 4 Clock
resumes in the same kiosk process with settings, installed binaries and boot ID
unchanged and zero automatic restarts. Neither device receives an app update.
The experiment removes input-loop overhead but leaves almost all the measured
stroke cost. Keep the existing controls/defaults; investigate geometric
calculations and vertex emission next, with the old executable retained as a
baseline rather than relying on same-build gains alone.

Artifacts are under `target/gpu-experiments/polygon-input.UJRTJdIj/`:
`run-device.sh`, `analyze.mjs`, `pi4/`, `pi5/`, `pi4-summary.json`,
`pi5-summary.json`, native captures/summaries, test/build logs and source
snapshots retain the protocol and evidence. Each device has 45,056 timed
samples, including the old executable's additional join-reference control;
the input comparison uses eight paired repeats per fixture/scope. Remote
copies are `/tmp/polygon-input.AJYGKYBH/` on Pi 4 and
`/tmp/polygon-input.boph5Itg/` on Pi 5. The tested client is
`5430596d…718602`; `bundle-manifest.json` records the full hashes.

## Adjacent edge calculation reuse

The `bulk-edges` experiment reuses an outgoing edge's length and normalized
direction as the next join's incoming edge. A single-entry cache lives inside
the fixed-width stroke builder; all four endpoint coordinate bits must match.
Merged closing points and flattened curves can miss the cache and use the
original calculation. Nothing is reused across strokes, frames or zoom levels.
The ordinary builder has a zero-sized reference policy, while
`bulk-edges-checked` independently recalculates every hit and asserts exact
length/direction bits. The enabled paths include hit/miss counter cost.

Both compared modes use path-event input, concrete bulk output, exact joins
and conservative rejection. Only edge reuse differs. Circles and retained
templates are unchanged; no rectangle fast path or quality approximation is
introduced. The writer default remains `stock`, with no saved-setting changes.

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-round-joins \
  --geometry-stroke-bulk --geometry-stroke-static --geometry-stroke-edges \
  --benchmark-width 800 --benchmark-height 480 \
  --presentation-frames 128 --presentation-repeats 4 \
  --geometry-output /tmp/edge-reuse-images
```

The output directory must be new. The edge flag conflicts with
`--geometry-stroke-polygon`; omitting both retains the earlier join-policy
comparison. Broader fixtures use `--geometry-stroke-cases
clock-idle,clock-rain,pizza-300,pizza-5000,mixed-interleaved`. Live selection,
within `retained-culled`, is `host gpu-stroke-writer bulk-edges`; the checked
variant is for validation rather than timing. Existing session/failure reset
rules apply.

The untimed oracle compares all **18** writer/join combinations for exact
vertex bits, indices, batches, cache decisions and policy counters. Regression
tests cover signed-zero keys, changed closing endpoints, degenerate polygons,
curves, caps, subpaths and failures at the first/middle/last vertex, including
buffer-prefix preservation and reuse after abort. **195 vendor tests** pass in
debug and release, **693 client tests** pass with four existing ignored tests,
and **32 CLI tests** pass. GPU clippy, software-only checking, formatting and
diff checks pass; clippy retains the existing warnings.

The workstation passes **443** replay/synthetic/terrain image comparisons and
**72** broader-scene comparisons, all pixel-identical. Its first validation
frame in the early-match fixture reuses 1,310 incoming edges and recomputes 18;
the arithmetic reduction is exercised, not an inactive switch. Timing from
that first workstation pass overlaps the cross-build and is excluded from the
performance decision.

### Comparison protocol and results

The device sequence is previous executable, new executable twice, previous
executable again, followed by two broader-fixture runs. The previous executable
is the preceding polygon-input build (`5430596d…718602`), not a different
installed kiosk revision. Each new block alternates the two policies for four
repeats of 128 frames. Spacewars uses a 24-frame warmup; the broader fixtures
warm at least 24 frames (32 for the mixed animation). CPU encoding and completed GL
wall time are separate, uncapped measurements; scene generation, simulation,
text, UI and scanout are excluded. Geometry counts, upload sizes, join counts
and cache admissions must agree. Validation and per-frame reuse diagnostics are
outside timed sections. Both kiosks are suspended only for the probes, using
RAM-backed temporary storage and retaining their installed binaries/settings.

The clean workstation old/new/new/old comparison, after the cross-build,
measures close-up encoding **0.5578 → 0.5639 ms**, **1.10% slower**, with all
eight pairs slower. Completed wall time is **0.9040 → 0.9119 ms**, **0.88%
slower**. Against the old executable, encoding is 0.48% slower and completed
wall time 0.25% faster. This does not establish a useful improvement.

On Pi 5 at logical 800×480, close-up encoding is **2.5407 → 2.5512 ms**,
**0.41% slower**, again with all eight pairs slower. Completed drawing is
**4.7715 → 4.7733 ms**, essentially unchanged (**0.04% slower**). The old
executable averages 2.5300 ms encoding and 4.7663 ms completed drawing, making
the candidate 0.84% and 0.15% slower, respectively. The new reference itself
has a 0.42% encoding penalty against that old executable.

This is not a lack of reuse: the Pi 5's first close-up validation frame reuses
**10,255** incoming edges and recomputes only **43**. Clock reuses 352 per
validation frame; Pizza correctly reuses zero because circles are unchanged.
Across the five broader Pi 5 fixtures, mean encoding changes stay within
0.11% and completed drawing within 0.17%, in either direction. Ordinary-match
completed drawing is 0.09–0.18% slower than the same-build reference. The
unchanged `ondemand` governor samples 1.7–2.4 GHz before/after blocks, with
49.1–55.1°C temperatures; this is not a locked-frequency comparison.

All **1,030** new-build Pi 5 image comparisons pass. Separately, **360** saved
PNG comparisons across 60 poses match the old executable byte-for-byte, as do
the corresponding workstation comparisons. Pi 5 Clock resumes in its existing
process with unchanged settings, binaries and boot ID, and zero restarts.

The Pi 4 at 1024×768 has a small same-build improvement: close-up encoding
**3.8792 → 3.8542 ms** (**0.64%**) and completed drawing **12.1643 → 12.1129 ms**
(**0.42%**), with all eight pairs improving in each scope. Against the old
executable's 3.8836 ms encoding and 12.1149 ms completed drawing, the gains are
**0.76%** and only **0.02%**. Ordinary early-match drawing is 0.41% slower and
late-match drawing 0.57% faster than the same-build reference; neither is more
than 0.17% faster than the old executable. The broader fixtures range from
0.21% worse to 0.62% better encoding, and 0.27% worse to 0.25% better completed
drawing. Pi 4 samples 62.8–67.7°C and 0.9–1.5 GHz with its unchanged `ondemand`
governor. These are native-viewport workloads, not resolution-matched hardware
comparisons.

All **1,030** new-build Pi 4 image comparisons and its **360** saved old/new
PNG comparisons pass. Together with the initial workstation checks and its
clean repeated comparison, there are **3,461** new-path image comparisons and
**1,080** saved old/new PNG comparisons. Pi 4 Clock also resumes in its original
process with unchanged settings, installed binaries and boot ID, and zero
restarts. Neither device receives an app installation.

Do not promote edge reuse: the implementation removes repeated arithmetic but
does not produce a useful net, cross-device rendering improvement. Keep it as
an explicit comparison control. The next investigation should separate the
remaining join-construction and vertex-emission costs before choosing another
optimization, retaining the old executable as an independent timing baseline.

Artifacts are in `target/gpu-experiments/edge-reuse.0EeazKSr/`: source snapshots,
build/test logs, `run-device.sh`, `run-native.sh`, `analyze.mjs`,
`check-images.mjs`, `pi4/`, `pi5/`, `native-controlled/`, native exploratory
captures, summaries and old-image reports. Each device has **45,056** timed
samples; the clean workstation sequence has **24,576**. Remote copies are
`/tmp/edge-reuse.bzaiq959/` on Pi 4 and `/tmp/edge-reuse.8B0mAGvL/` on Pi 5.
The tested client hash is `e217fc92…5ebd08`; the bundle manifest records the
full binary hashes and sizes.

## Stroke construction attribution

`--geometry-stroke-attribution` separates fixed-width join construction from
the adjoining output work. It is a diagnostic replay, not another live writer
or a proposed optimization. Existing writer choices and defaults are unchanged.

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-stroke-attribution \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 96 --presentation-repeats 4 \
  --geometry-output /tmp/stroke-attribution-captures
```

The probe warms the existing 24-frame moving Spacewars histories, then captures
the guarded polygon strokes actually tessellated after conservative rejection.
Their projected points, styles, vertex bits and rebased indices form an
isolated replay corpus. Capture is limited to 8,192 outlines and an 8 MiB
accounted payload per frame, with a 128 MiB history budget; unsupported shapes,
bad index ranges or exhausted budgets fail instead of silently omitting work.
All eight controls must reproduce every captured vertex bit and triangle index.
The existing curve/cap/subpath tests also compare every vertex field and the
output transaction, including first/middle/last vertex failures.

The controls are ordinary static tessellation, explicit no-op hooks, counters
without clocks, four individually timed regions, and all regions timed
together. The regions distinguish pure join-side geometry, base-vertex
construction plus writes, edge triangles, mixed join extras, and flattened
steps. Join extras include round-join math and output; base vertices also
compute normals. Setup, point merging, input decoding, caps and remaining
closing bookkeeping are outside these spans. The polygon corpus has no
flattened-curve steps; curve correctness remains covered by unit tests.

Per-region durations use wall clocks. Whole-replay wall and thread CPU time
are separate columns. Nested clock reads perturb both execution and code
layout; the empty-hook calibration is a diagnostic floor, not a reliable
subtraction formula. In particular, neither the measured stage sum nor the
unassigned remainder is a percentage breakdown of an uninstrumented frame.
Scene generation, projection, cache lookup/culling, GPU work, UI and simulation
are absent from the isolated throughput samples. The independent previous
executable remains the reference for ordinary CPU/completed-render timing.

### Device measurements

Both devices run the previous executable (`e217fc92…5ebd08`), new executable,
an attribution block, new executable, previous executable, then another
attribution block. Each ordinary block uses two repeats of 48 frames; each
attribution block uses four repeats of 96 frames, rotating/reversing the eight
controls. Each device supplies **23,040 timed samples**. Generation, capture,
exact-output checks and calibration are outside those samples. Ordinary CPU
encoding and completed GL drawing remain separate scopes.

These are different native-viewport workloads, not a Pi speed comparison:
Pi 4 at 1024×768 captures about **545** guarded outlines per close-up frame,
versus **2,575** on Pi 5 at logical 800×480. The corresponding vertex counts
are 7,740 and 36,403. Early-match capture is empty after warmup on both devices;
late-match work is intermittent. The workstation run checks capture/replay
correctness but overlaps compilation, so its timings are not performance
evidence here.

| Close-up replay measurement | Pi 4 | Pi 5 |
| --- | ---: | ---: |
| Plain isolated stroke CPU | 1.0100 ms | 1.7817 ms |
| Explicit no-op hooks CPU | 0.9967 ms | 1.7454 ms |
| Counter-only CPU | 1.0018 ms | 1.7721 ms |
| All regions timed CPU | 2.0466 ms | 4.7670 ms |
| Join geometry span, separately timed | 0.3486 ms | 0.6982 ms |
| Base vertex construction/output span | 0.2267 ms | 0.5215 ms |
| Edge triangle span | 0.1198 ms | 0.3144 ms |
| Mixed join extras span | 0.3737 ms | 0.6797 ms |
| Empty-hook measured floor per call | 53.7 ns | 35.9 ns |

Timing every region increases replay CPU by **103% / 168%**. Individually
timing one region increases it by 19–29% / 32–46%. The all-region spans closely
agree with the individually timed spans, but that does not remove their clock
bias. Geometry, base vertices and extras each execute about 2,114 / 10,202
times per frame; edge-triangle regions execute about 1,583 / 7,660 times.
The edge-triangle spans are particularly close to the clock floor. Counter-only
and explicit no-op variants being slightly faster than plain is further
evidence of measurement/code-generation sensitivity, not a useful optimization.

The qualitative result is consistent across devices: join geometry and mixed
round-join work are more promising targets than triangle writes. The result
does **not** establish precise production CPU shares or make the unassigned
remainder a measured setup cost. A useful next experiment is to count repeated
stroke-invariant calculations in those regions, then test amortizing them once
per outline where there is actual reuse. Preserve exact arithmetic/output and
the old-executable control; do not assume fewer calculations will be faster.

The independent ordinary-render comparison exposes a small reference penalty:
close-up encoding on Pi 4 changes **3.8557 → 3.9634 ms**, **2.79% slower**, while
completed drawing changes **12.2566 → 12.2359 ms**, effectively unchanged
(-0.17%). Pi 5 encoding changes **2.5496 → 2.5672 ms**, **0.69% slower**, and
completed drawing **4.7903 → 4.8009 ms**, +0.22%. Early/late encoding changes
are -0.74%/-1.93% on Pi 4 and +0.97%/-0.96% on Pi 5. These measurements do not
support a speedup or a claim that adding diagnostic code has zero effect on
the compiled reference. No installed app or renderer default changes.

All **1,772 new-build GPU image comparisons** pass across the two Pis, including
the existing moving/zoom/resize/terrain cases and eighteen-way mesh oracle.
Another **720 saved old/new PNG comparisons** are byte-identical. Captured
outlines match exact vertex bits and index order in all eight replay controls.
Client tests pass **695**, with four existing ignored tests; CLI tests pass
**32**; tessellator tests pass **195** in both debug and release. Formatting,
software-only checking and the Yocto release build pass; clippy completes with
warnings, including a collapsible diagnostic validation `if`.

Pi 4 samples 59.9–65.2°C and 0.9–1.5 GHz; Pi 5 samples 46.9–52.9°C and
1.5–2.4 GHz. Governors are unchanged (`ondemand`), not locked-frequency tests.
Both Clock sessions resume in their original processes with identical settings,
installed binaries and boot IDs, and zero service restarts. Probes and captures
run in RAM-backed temporary directories; neither unit is rebooted or flashed.

Artifacts are under `target/gpu-experiments/stroke-attribution.e1Htp8HB/`:
`run-device.sh`, `analyze.mjs`, `pi4/`, `pi5/`, both summary JSON files,
source snapshots, bundle manifest, native capture validation and build/test
logs. Remote copies are `/tmp/stroke-attribution.rRMbhNjU/` on Pi 4 and
`/tmp/stroke-attribution.8saIw0fr/` on Pi 5. The tested client is
`ab674604…f91224`; the manifest records full hashes.

## Prepared stroke guard experiment

Preparing fixed-width round-join bounds once per outline reduces isolated
close-up stroke CPU by **2.84% on Pi 4 and 4.51% on Pi 5**, with identical output.
The absolute replay savings are **29 / 80 microseconds**. These isolated
percentages do not describe total rendering or application CPU. The subsequent
[renderer integration](#prepared-guard-renderer-integration) tests whether
the saving survives complete geometry encoding and drawing.

The per-outline census counts repeat guard invocations and fallback arc-step
calculations outside all timing. Over each native viewport's 24-frame close-up
history, the measured reuse opportunities are:

| Measurement | Pi 4 at 1024×768 | Pi 5 at 800×480 |
| --- | ---: | ---: |
| Captured outlines | 13,079 | 61,809 |
| Round-join guard invocations | 50,747 | 244,856 |
| Invocations after the first in an outline | 37,980 | 183,851 |
| Fallback arc-step calculations | 15,203 | 24,882 |
| Arc-step calculations after the first in an outline | 4,332 | 6,017 |

About 75% of guard invocations repeat the same width/tolerance parameters;
only 28.5% / 24.2% of fallback arc-step calculations repeat. These are
opportunities, not counts of every executed arithmetic instruction: early
rejections can skip later checks. The candidate therefore prepares only the
fixed-width eligibility and squared-radius bounds. It leaves the original
angle/flattening calculation, fold/position tests, dot/cross products, cap
rules, tolerances and emitted geometry unchanged. There is no geometry cache,
allocation, cross-frame state or rectangular fast path.

The ordinary builder retains its original predicate through a zero-sized
policy. The new `tessellate_static_invariants` entrypoint has unchecked and
checked specializations; checked compares every decision against the original
predicate and asserts the width invariant. These entrypoints were initially
replay-only; the subsequent integration adds session-only renderer controls.

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-stroke-attribution \
  --geometry-stroke-invariants \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 192 --presentation-repeats 6 \
  --geometry-output /tmp/stroke-invariant-captures
```

Attribution manifest version 2 adds plain/prepared/checked comparisons and the
per-outline histograms. The command above times only those three controls,
without region clocks or empty-hook calibration. It rotates and reverses their
order over six repeats, warms each path before measuring, and buffers CSV rows
until the timed block ends. All ten attribution variants still validate every
captured vertex bit, rebased index and join counter outside timing. The original
eight diagnostic variants remain available without `--geometry-stroke-invariants`.

### Device results

Both devices run old executable, new executable, isolated comparison, new
executable, old executable, isolated comparison. Each ordinary-render block
contains two repeats of 48 frames per fixture/control/scope; each isolated block
contains six repeats of 192 frames per fixture/control. The analyzer validates
**25,344 timed rows per device**, exact geometry/count stability and complete
sample keys. Different viewport workloads are not a hardware-speed comparison.

| Close-up isolated stroke CPU | Pi 4 | Pi 5 |
| --- | ---: | ---: |
| Same-build plain reference | 1.0023 ms | 1.7733 ms |
| Prepared guard | 0.9738 ms | 1.6933 ms |
| Change | −2.84% | −4.51% |
| Checked guard | 1.2002 ms | 2.1590 ms |

The candidate improves all twelve paired repeats on each Pi: reductions range
from 2.49–3.17% on Pi 4 and 4.39–4.65% on Pi 5. Checked mode costs about 20–22%
more than plain and is validation-only. A workstation run without per-region
clocks at 800×480 changes 0.7628 → 0.7389 ms (−3.14%); the earlier exploratory stage-profile run
is not the device performance baseline. Early-match captures are empty after
warmup, and late-match captures are sparse on the Pis, so neither establishes
a useful isolated speedup for ordinary play.

The independent old-executable comparison uses **bulk-static + exact joins +
conservative culling**, not the stock renderer default, and does **not** enable
the prepared guard:

| Close-up ordinary rendering | Pi 4 old → new | Pi 5 old → new |
| --- | ---: | ---: |
| Encoding CPU | 3.8842 → 3.9184 ms (+0.88%) | 2.5737 → 2.5780 ms (+0.17%) |
| Completed GL wall time | 12.3198 → 12.2451 ms (−0.61%) | 4.8032 → 4.7862 ms (−0.35%) |

Early/late encoding changes are −0.02%/−0.54% on Pi 4 and +0.86%/+1.13% on
Pi 5. Completed drawing changes by less than 1% in every fixture. This exposes
small compiled-reference differences; it is not evidence that the prepared
guard improves complete frames. The subsequent renderer comparison keeps
the independent executable baseline and adds broader scenes before any
decision to adopt the guard or pursue more small per-join changes.

### Verification and device state

The prepared path passes exact captured-output and join-decision checks on
both Pis, including repeated width changes. The tessellator's full-field
polygon/curve/cap/subpath and first/middle/last output-failure tests include both
prepared variants, as do angle/radius/tolerance and numerical-boundary checks.
All **196 tessellator tests** pass in debug and release; **696 client tests**
pass with four existing ignored tests, plus **32 CLI tests**. Formatting,
software-only checking, clippy and the Yocto release build pass. Clippy retains
25 client / 37 client-test warnings; the prior attribution warning is fixed.

Separately, the unchanged ordinary-render paths pass **1,772 new-build GPU
image comparisons** and **720 saved old/new PNG byte comparisons** across the
two Pis. These checks cover reference/edge controls, not a prepared-guard GPU
integration. The workstation also passes 443 ordinary-render image checks.

Both devices resume Clock in their original processes, with unchanged installed
binary/settings hashes and boot IDs, and zero service restarts. All device
probes use RAM-backed temporary directories. Governors remain `ondemand`, not
locked-frequency benchmark settings. Block-boundary samples are 63.3–67.2°C
and 0.9–1.5 GHz on Pi 4, and 46.9–52.9°C and 1.6–2.4 GHz on Pi 5.

Artifacts: `target/gpu-experiments/stroke-constants.dlsrWOfu/`, including
`run-device.sh`, `analyze.mjs`, source snapshots, old/new executables,
`pi4-summary.json`, `pi5-summary.json`, full device samples/captures/integrity
checks and native build/test logs. Remote copies are
`/tmp/stroke-constants.4NaEvhlo/` on Pi 4 and
`/tmp/stroke-constants.tqLHdZx3/` on Pi 5. The previous executable is
`ab674604…f91224`; the tested build is `8ac6eb41…837cb8`, with the full hash in
the bundle manifest.

## Prepared guard renderer integration

The prepared fixed-width guard is now available through session-only
`bulk-prepared` and `bulk-prepared-checked` stroke writers. The renderer uses
them only for uncached polygon and line strokes. Circles, retained templates,
stroke tolerances, geometry-cache policy and the `stock` default are unchanged.
There is no persistent geometry cache or rectangular fast path in this change.
The close-up comparison saves **0.94% / 2.80% encoding CPU on Pi 4 / Pi 5**,
but completed drawing is effectively unchanged on Pi 4 and improves **1.39%**
on Pi 5. The [live application comparison](#prepared-guard-live-application-comparison)
below finds a small Pi 5 saving but no Pi 4 benefit. Keep this opt-in; the
result does not justify another per-join optimization or a default change.

For an active direct-geometry GPU session, compare these controls with
`spacewars-cli host gpu-stroke-writer bulk-static` and
`spacewars-cli host gpu-stroke-writer bulk-prepared`, keeping the join policy
fixed at `spacewars-cli host gpu-joins exact`. The checked writer additionally
verifies each prepared decision against the original predicate and is for
validation, not timing. Software and unsupported rendering modes retain the
existing explicit rejection and session-reset behavior.

The offscreen comparison uses:

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-round-joins --geometry-stroke-bulk \
  --geometry-stroke-static --geometry-stroke-prepared \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 96 --presentation-repeats 4 \
  --geometry-output /tmp/prepared-guard-captures
```

Add `--geometry-stroke-cases
clock-idle,clock-rain,pizza-300,pizza-5000,mixed-interleaved` for the broader
fixtures. Clock and Pizza use frozen production snapshots; the mixed fixture
uses a prescribed 32-pose animation. Spacewars replays three moving 24-frame
histories. Projection and scenario stepping happen before timing. CPU encoding
and completed GL drawing include all geometry passes and minimap compositing,
but exclude simulation, text, UI, readback and display scanout. These are
uncapped geometry measurements, not full-application CPU or FPS results.

The untimed oracle now compares eight stroke writers with reference, exact and
checked join policies: vertex bits, indices, batches, cache decisions and join
decisions must agree. The two measured controls also require exact GPU pixels.
Cold-to-warm and zoom-return self-history comparisons remain separate because
ordinary cached expansion can change antialiased edge pixels in both controls.

### Complete geometry measurements

The balanced sequence is old executable, new executable, new executable, old
executable, with Spacewars and broad scenes in every block. Each control/scope
uses four repeats of 96 measured frames after at least one complete warmup
history. Control order alternates within each block. The old executable's
`bulk-static` control is the independent baseline; its paired edge-reuse
control supplies additional output checks but is not the performance baseline.
All timed guard comparisons use `bulk-static` versus `bulk-prepared` with
exact joins, conservative culling, prepared plans and bucketed streams. This
does not compare against the default `stock` writer.

The close-up/zoom results use native logical viewports: 1024×768 on Pi 4 and
800×480 on Pi 5. They contain different visible and uncached geometry, so the
absolute times are not a hardware-speed comparison.

| Scope | Pi 4 reference → prepared | Pi 5 reference → prepared |
| --- | ---: | ---: |
| Encoding CPU | 3.9182 → 3.8812 ms (−0.94%) | 2.5827 → 2.5105 ms (−2.80%) |
| Completed GL wall time | 12.2197 → 12.2159 ms (−0.03%) | 4.7933 → 4.7265 ms (−1.39%) |

Encoding improves in all eight paired repeats on each device: 0.32–1.46% on
Pi 4 and 2.55–3.00% on Pi 5, saving about 37 / 72 microseconds per geometry
encode. Pi 5 completed drawing improves in all eight repeats by 1.02–1.57%,
saving about 67 microseconds. Pi 4 completed drawing varies from −0.87% to
+0.59% across repeats; its four-microsecond mean difference is not a useful
frame-time gain.

Against the independent previous executable, close-up encoding/completed
drawing change −1.38%/−0.21% on Pi 4 and −2.20%/−1.94% on Pi 5. The compiled
reference itself changes −0.44%/−0.18% on Pi 4 and +0.62%/−0.56% on Pi 5.
Within the new build, the other fixtures change as follows (negative is faster):

| Fixture | Pi 4 encoding CPU | Pi 4 completed wall | Pi 5 encoding CPU | Pi 5 completed wall |
| --- | ---: | ---: | ---: | ---: |
| Early match | −0.06% | −0.10% | −0.39% | −0.09% |
| Late match | −0.03% | −0.44% | −0.03% | −0.17% |
| Clock idle | −1.67% | +0.25% | −0.31% | −0.02% |
| Clock rain | −0.47% | −0.23% | −0.12% | −0.12% |
| Pizza 300 | +0.13% | +0.25% | +0.10% | −0.02% |
| Pizza 5000 | +0.01% | +0.03% | −0.04% | −0.02% |
| Mixed animated shapes | −0.18% | +0.04% | −1.18% | −0.40% |

Keep compiled-reference differences separate from the guard's effect. On Pi 5,
late-match encoding is 1.3836 → 1.4057 ms in the old/new reference (+1.60%),
while toggling the guard changes it only −0.03%. Clock idle/rain reference
encoding improves about 3.7% across builds, but toggling the guard changes
it only −0.31%/−0.12%. The mixed-shape fixture improves 1.18% in encoding and
0.40% in completed drawing within the new build. Pizza changes by less than
0.10%/0.02%; its circle path is unchanged. These results support a small
targeted saving, not a universal renderer improvement or default change. Pi 4
has a similar caution: late-match and mixed-shape reference encoding increase
0.92% and 1.19% across builds, while the guard's same-build changes are only
−0.03% and −0.18%. Pi 4 Clock idle encoding improves in all eight paired repeats
(0.9345 → 0.9189 ms), but that does not improve completed drawing.

### Verification and artifacts

The analyzer validates **49,152 timed rows per device**, complete sample keys,
geometry/upload/admission/join counts and broader-fixture source metadata.
Across both devices, **2,044 new-candidate image comparisons** pass exactly,
along with the 24-way CPU oracle and **840 saved PNG byte comparisons** against
the first old-executable run (560 old/new and 280 old/old). The 32 separate
device self-history comparisons also show zero differences. Moving cameras,
a smooth out-and-back zoom sweep, resized
views, synthetic outlines, terrain reveal and terrain replacement all pass.

The workstation passes another 511 candidate image comparisons. Its existing
one-pixel cold-to-warm cache difference appears in both old and new controls;
all 180 exported Spacewars PNGs, including difference images, match the saved
executable. No native performance claim is taken from this run, which overlapped
other build work. All **699 client tests** pass with four existing ignored
tests, plus **32 CLI tests** and **196 tessellator tests** in both debug and
release. Formatting, clippy, software-only checking and the Yocto release build
pass; clippy retains the existing 25 client / 37 client-test warnings.

Both device probes use RAM-backed directories and resume Clock in their original
processes, with unchanged settings, installed binary hashes and boot IDs, and
zero service restarts. The governors remain `ondemand`. Block-boundary samples
span 64.8–67.7°C / 0.9–1.5 GHz on Pi 4 and 46.9–54.6°C / 1.7–2.4 GHz on Pi 5;
these are not locked-frequency runs. The new live writer is unit-tested through
the request/reset/failure paths, but was not enabled in the installed apps.

Artifacts: `target/gpu-experiments/stroke-prepared-render.NIglGrG7/`, including
`run-device.sh`, `analyze.mjs`, `pi4-summary.json`, `pi5-summary.json`, full
device samples and captures, integrity checks, source snapshots, native
verification and build logs. Remote copies are `/tmp/stroke-prepared.mpJeSxBN/`
on Pi 4 and `/tmp/stroke-prepared.GUVL8dve/` on Pi 5. The previous client is
`8ac6eb41…837cb8`, the tested client is `60076223…511296`, and the bundle
manifest records the full client/CLI hashes.

## Prepared guard live application comparison

Preparing the round-join guard once per outline saves **1.26% of whole-process
CPU per rendered frame on Pi 5 with ordinary cameras**, and **0.40% during
continuous zoom**. Pi 4 shows no useful gain: ordinary cost changes −0.19%
with mixed repeat results, and zoom cost increases 0.26%. These live results
include simulation, scene preparation, text, UI and user-space driver-thread CPU; they
do not measure GPU execution time. Keep `bulk-prepared` opt-in and move the
investigation toward larger reductions in guarded-stroke work rather than
another small per-join change.

### Workload and measurement

Each device runs four fresh processes per control and camera condition:
the independent previous executable with `bulk-static`, the new executable
with `bulk-static`, and the new executable with `bulk-prepared`. All use exact
joins, retained/culled geometry, bucketed streams, prepared plans, pooled
source storage, conservative rejection and the existing `repeated` admission
policy. Control order rotates/reverses between rounds; camera order reverses
too. Both devices use 1× rendering, CPU-shaped text and no performance overlay.
Pi 4 renders at 1024×768; Pi 5 at logical 800×480 with the existing 90° output
rotation. These are native-viewport workloads, not a hardware-speed comparison.

The fixture is seed 0, idle human P1, rule-bot P2 (`material_mission_v9`), no
asteroids and a 600-second match limit. Measurement starts near tick 480 and
takes twelve status windows at 120-tick intervals through tick 1920. The
continuous camera sweep uses the existing opposing 44–640 world-unit views.
This is a controlled 24-second slice of a match, not full-match endurance or
coverage of every gameplay density. Accepted intervals span 24.13–24.49 seconds
on Pi 4 and 24.21–24.61 seconds on Pi 5.

Whole-process CPU comes from `/proc/PID/stat` user plus system ticks and elapsed
uptime, including all application threads. **CPU per rendered frame uses fresh
`kms_draws_total`**, checked against completed page flips. The host's
`frames_total`, `updates_total` and match diagnostics are cached between
publications, normally once per second; they are unsuitable as precise
denominators for a sub-percent live comparison. They remain in the raw data,
and match ticks select the workload, but are not the frame-count denominator
below. Neither KMS draw counts nor completion counts establish input latency.

### Results

Each arrow compares the two policies within the new executable. CPU percentages
use one core as 100%; CPU per draw is total process CPU divided by actual KMS
draws, not the nested geometry encoder timer.

| Device and cameras | Process CPU | CPU ms per draw | CPU per draw change | Completed flips per second |
| --- | ---: | ---: | ---: | ---: |
| Pi 4 ordinary | 72.48% → 72.33% | 12.1365 → 12.1133 | −0.19% | 59.72 → 59.71 |
| Pi 4 continuous zoom | 80.56% → 80.57% | 13.9943 → 14.0301 | +0.26% | 57.56 → 57.43 |
| Pi 5 ordinary | 33.50% → 33.08% | 5.5793 → 5.5088 | −1.26% | 60.05 → 60.05 |
| Pi 5 continuous zoom | 40.32% → 40.16% | 6.7135 → 6.6865 | −0.40% | 60.05 → 60.06 |

Pi 5 ordinary CPU/draw improves in all four same-build pairs, by 0.37–1.86%,
saving about **70 microseconds per draw** in the aggregate. Zoom improves in
three of four pairs (−1.02%, −0.11%, −0.51%, +0.03%), saving about 27 microseconds.
Pi 4 ordinary pairs split two better/two worse; zoom is slightly worse in all
four (+0.09–0.57%). The guard does not resolve Pi 4's zoom presentation gaps.
The unchanged `ondemand` governors sample 1.0–1.5 GHz / 64.8–69.6°C on Pi 4
and 1.8–2.4 GHz / 46.9–51.3°C on Pi 5, so these are not locked-frequency tests.

The independent executable still matters. Old-reference → new-reference →
candidate CPU/draw is **5.5531 → 5.5793 → 5.5088 ms** for Pi 5 ordinary play:
the candidate's net saving is 0.80%, smaller than its same-build saving.
Pi 5 zoom is **6.7137 → 6.7135 → 6.6865 ms**, a 0.40% net saving. Pi 4 ordinary
is **12.2203 → 12.1365 → 12.1133 ms**, with most of the net difference already
present in the compiled reference; zoom is **13.9881 → 13.9943 → 14.0301 ms**,
0.30% worse overall. Do not attribute compiled-reference changes to the guard.

### Validation and artifacts

The first Pi 4 old-build run starts measuring at tick **540**, outside the
predeclared `[480, 540)` start window. It is preserved as rejected data, not
averaged into the accepted matrix. A replacement starts at tick 500 and passes
every gate. Its controls are configured while paused, then it resumes before
warmup, keeping first-use setup outside that window. It runs after the matrix,
so the independent Pi 4 ordinary baseline is no longer fully counterbalanced.
The four same-build pairs are unaffected. The analyzer records both the
rejected measurements and their replacement explicitly.

All **576 accepted status windows** pass mode, viewport, camera trajectory,
renderer-error and memory-budget checks. **48 paused-image comparisons** are
byte-identical across reference/prepared/checked modes, including 32 cross-policy
comparisons and 16 same-policy repeats. Four moving checked runs add **969
simulation ticks** without an assertion or renderer error. Return-to-launcher
resets pass in every accepted process. Both kiosks return to Clock, with
unchanged installed binary hashes, saved preferences and boot IDs, and zero
automatic service restarts. Temporary KMS sessions require cleanly quitting
and restarting the installed kiosk; they do not preserve the original process.

This step changes no application code or defaults. The tested Rust files still
match the preceding release snapshot. Shell syntax, ShellCheck, analyzer
assertions and diff checks pass. The scripts and reports are under
`target/gpu-experiments/stroke-prepared-live.rcH1W9ws/`, including `pi4/`,
`pi5/`, `pi4-recheck/`, both summary JSON files, `report.txt`, screenshots,
the prior bundle manifest and source snapshot. Remote copies are
`/tmp/stroke-live.DIuO2nWB/`, `/tmp/stroke-live.jPKlN5Ju/` and
`/tmp/stroke-live-recheck.Tpv8oEvx/`. The old/new clients remain
`8ac6eb41…837cb8` / `60076223…511296`; full hashes are retained in the manifests.

## Folded stroke template feasibility

A second small-scale stroke template, with the existing conservative safety
margins, does **not cover the expensive close-up workload**. Keep this idea out
of the renderer for now. A test-only audit captures the outlines that actually
fall back after warmed `retained-culled` encoding and conservative rejection,
then evaluates a folded template's possible scale interval. No live renderer,
cache budget, setting or device installation changes.

The candidate assumes endpoints remain unmerged and every corner stays on the
same side of its folding threshold. It stays twice as far from merge/fold
transitions, retains the near-right-angle restriction, and excludes almost
reversing turns. Within a candidate interval, Lyon builds a template at its
geometric midpoint; the audit compares expanded vertices and indices against
ordinary tessellation at the captured scale, interval ends, nearby zooms,
rotations and another screen position. This is a feasibility model using
already-projected points, **not a proven reuse guard for immutable local
geometry**.

The workstation runs the existing early/late/close-up fixtures at both device
viewport sizes. Counts below are outline occurrences across each **24-frame
history**, not distinct objects or actual Pi measurements.

| Viewport and fixture | Guarded outlines | Folded template candidates |
| --- | ---: | ---: |
| 1024×768 late match | 700 | 5 |
| 1024×768 close-up zoom | 13,074 | 0 |
| 800×480 late match | 1,573 | 4 |
| 800×480 close-up zoom | 61,808 | 0 |

Both early-match histories contain zero guarded outlines after warmup. The nine
candidates pass 70 geometry comparisons; their scale intervals span only about
9–11%. In the 1024×768 close-up history, first-matching rejection reasons are
51.90% fold-transition margin, 44.57% merge margin and 3.52% near-right-angle
margin. At 800×480 those shares are 15.90%, 9.73% and 74.37%. These categories
are ordered exclusions, not independent causes or measured tessellation states.

### Why simply reusing the previous mesh is unsafe

A separate, deliberately unguarded negative control captures the stroke recipe
at its original screen coordinates, preserving its initial topology. It then
compares that recipe with fresh tessellation after synthetic transforms. The
source reconstruction passes for all **77,155 outlines**. There are **462,930
geometry comparisons**, including the source reconstructions, with 1,090
topology differences and 442 additional coordinate differences above 0.002 px.

In close-up views, a **1% zoom-out** changes topology for **1.44% / 0.31%** of
outlines at 1024×768 / 800×480; zoom-in changes **1.76% / 0.52%**. Captured cases
reproduce both endpoint merging and corner folding. Another case changes
connectivity after translation alone: f32 rounding changes a near-right-angle
decision. When every endpoint merges into an empty round cap, rotating its
cached mesh is also wrong: Lyon's cap tessellation stays screen-aligned.

Topology differences do not establish visible pixel differences, and 0.002 px
is a coordinate diagnostic, not an image-quality gate. Most outlines retain
their topology under the sampled small transforms, so this result does not
rule out more precise reuse. It rules out adopting a blanket folded template
or blindly reusing the last mesh. A more useful next experiment is to retain
stroke topology only across explicitly validated merge/fold states, with
special handling for collapsed caps and uncertain corners. Establish image
agreement and net CPU benefit before adding that policy to the live cache;
do not simply shrink the existing safety margins.

### Reproduction and validation

The audit is excluded from normal test runs and application binaries:

```sh
SPACEWARS_STROKE_REUSE_AUDIT=/tmp/stroke-reuse-audit.json \
  cargo test --locked --profile ci -p engine-client --no-default-features \
  --features pi-kiosk-gpu --bin engine-client audit_guarded_stroke_reuse_domains \
  -- --ignored --nocapture --test-threads=1
```

The optional output path must not exist. This needs the GPU-feature build
dependencies but creates no GPU context or window. Six normal unit tests cover
the band restrictions, a synthetic folded template, and captured merge, fold,
translation and collapsed-cap counterexamples. The full GPU-feature client
suite passes **705 tests**, with five ignored manual audits. A second complete
audit produces identical JSON. Formatting and diff checks pass; GPU clippy
completes with the existing 25 application / 37 test warnings and none in the
new audit. No device timing or performance gain is claimed for this step.

Artifacts are under `target/gpu-experiments/folded-stroke-audit.IJCL6Z8K/`.
`native-audit-v2.json` and its log contain the complete candidate and negative
controls; `report.mjs` checks accounting and prints `report.txt`.
`native-audit-final.json` is the identical repeat from the final source. The
initial `native-audit.json` contains only the candidate check. All renderer Rust files
in the preceding frozen source snapshot are unchanged except the attribution
module's test-only inclusion of this new audit.

## Dynamic stroke topology reuse

Recomputing corner decisions before reusing triangle indices preserves the
tested geometry, but **does not justify live renderer integration yet**.
The isolated close-up workload improves at 800×480 and regresses at 1024×768
on both Pis. Keep this benchmark-only: the cost of assessing a candidate and
then repeating geometry work in ordinary tessellation is material when most
strokes are refused. Increasing cache capacity would not address this problem.

The prototype accepts fixed-width, closed polygons of 3–64 points with round
joins. It recomputes the current projected f32 merge/fold decisions and vertex
positions. All merged paths, collapsed caps, unsupported styles and uncertain
or nonempty round arcs use ordinary tessellation. Connectivity keys contain
the point count and each corner's fold/single-vertex flags, not object identity
or screen coordinates. Different shapes can share indices; widths, rotations,
translations and zoom never reuse old positions. This is a general polygon
experiment, not a rectangle-specific rendering path.

Cold layouts use the original edge/join triangle functions and refuse
unexpected arc vertices. The no-arc certificate retains the existing numeric
and angular margins, checking the actual side even on folded corners. Position
construction preserves Lyon's divide-then-multiply arithmetic, vertex order,
closing duplicates and index order. The cache has at most 128 FIFO entries and
512 KiB of key/index payload; map/allocator overhead is not included in that
payload counter. No live renderer entrypoint, saved setting or default changes.

### Pi comparison

Both devices run both viewport sizes, separating the viewport's geometry mix
from hardware differences. Each viewport uses two candidate passes with four
counterbalanced plain/reuse pairs of 192 timed history replays per pass. The
24-frame histories are warmed before timing. An older executable runs before
and after the candidates, with the current build's existing attribution control
between them. Timing includes current-point validation, lookup, fresh vertex
writes, copied indices and all ordinary fallbacks. It excludes projection,
simulation, text/UI, GPU rendering and scanout.

| Device and logical viewport | Plain stroke CPU | Topology reuse CPU | Change |
| --- | ---: | ---: | ---: |
| Pi 4, 1024×768 | 1.0119 ms | 1.2034 ms | +18.93% |
| Pi 5, 1024×768 | 0.4049 ms | 0.4839 ms | +19.52% |
| Pi 4, 800×480 | 4.3599 ms | 4.0268 ms | −7.64% |
| Pi 5, 800×480 | 1.7752 ms | 1.6606 ms | −6.46% |

All eight paired comparisons agree on each direction. Against the independent
older executable, the corresponding changes are +19.15%, +20.83%, −7.91% and
−6.61%. The unchanged attribution reference moves by at most 0.46% across
builds in these close-up cases. These are stroke-construction savings or costs,
**not whole-frame performance changes**.

At 1024×768, only 1,896 of 13,079 captured close-up stroke occurrences qualify
(14.50%); 2,474 are refused for merging and 8,709 for arcs. At 800×480,
42,140 of 61,809 qualify (68.18%), with 5,345 merge and 14,324 arc refusals.
Each history needs just two layouts, totaling 610 bytes of counted payload.
Warm samples have no cache misses, evictions or budget refusals. The smaller
viewport's camera projection leaves substantially more guarded outlines to
construct; it is not simply a lower-resolution version of identical work.

Early-match histories contain no guarded strokes. The Pi late-match histories
contain only seven / twelve occurrences at the two sizes, none eligible for
reuse; their sub-microsecond changes are not useful evidence of a gameplay
benefit. ARM and workstation late-match captures differ, so validation and
timing use each platform's own captured history. The two Pis' per-frame geometry
counts and candidate decisions agree.

### Validation and reproduction

Each complete two-viewport Pi candidate pass checks 74,907 original outline
occurrences and 374,535 transformed cases against exact vertex bits and index
order, outside timing. Transforms include ±1% zoom, rotation, subpixel movement
and a larger translation. Both passes on both devices succeed, as do the
workstation's own histories. The four previously captured merge, fold,
translation and collapsed-cap counterexamples are retained as ordinary tests.
This is geometry validation, not a new GPU image or live-session comparison.

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-stroke-attribution \
  --geometry-stroke-topology --benchmark-width 800 --benchmark-height 480 \
  --presentation-frames 192 --presentation-repeats 4 \
  --geometry-output /tmp/stroke-topology-results
```

The output directory must not exist. Five new tessellator tests cover varied
polygons, fresh geometry on hits, unsupported inputs, FIFO eviction and bounds;
client tests cover colored/rebased output, mixed fallback, captured transitions
and flag constraints. All **708 client tests** pass (five manual audits ignored),
and **201 tessellator tests** pass in debug and release. GPU clippy completes
with the existing warnings; software-kiosk checking, the Pi release build,
formatting, script checks and analyzer assertions pass. A separate no-default-
features vendor check stops in the unchanged `lyon_geom` dependency because
`euclid::Trig` requires its `std` or `libm` feature; this is not a validated
no-std configuration.

All 119,808 timed batch rows pass count and budget checks. The RAM-only probes
resume the original Clock processes, with unchanged installed binary/settings
hashes and boot IDs, zero service restarts and no reported UI error. Sampled
temperatures are 69.14–70.60°C on Pi 4 and 47.4–52.9°C on Pi 5. CPU frequency
samples are retained rather than assuming a fixed clock.

Artifacts are under `target/gpu-experiments/stroke-topology.bQDiyZ5K/`, including
`run-device.sh`, `analyze.mjs`, `report.mjs`, `report.txt`, `pi4/`, `pi5/`, both
summary JSON files, build/test logs, source snapshots and old/new executables.
Remote copies are `/tmp/stroke-topology.HLEtghSd/` and
`/tmp/stroke-topology.knUDmxlk/`. The independent reference hash is
`60076223b418d39d23e36c3597b254d2d74892898b4c587dadf81b0e9b511296`;
the candidate hash is
`cf3c5f7484166cb022256c47ff70f43691ab5f91970f472c74edc16b992d8d1a`.

The next experiment should make refusals cheap or let fallback emission use
geometry already computed during classification, then repeat this matrix.
Do not integrate the current policy based only on its favorable 800×480 result,
relax the correctness guard, or grow a cache that already has a 100% warm hit
rate. Whole-renderer and live pacing checks remain necessary if the isolated
tradeoff improves.

## Single-pass stroke fallback

Reusing the corner geometry already computed during classification removes
the preceding prototype's 1024×768 regression and increases its 800×480 saving
on both Pis. This is still a **benchmark-only, isolated stroke result**. It
justifies a complete-renderer comparison, not a live default change.

The new `single-pass` control keeps the previous `plain` and `topology` controls.
When classification encounters an arc, it retains the completed corner prefix,
computes only the remaining corners, and uses the original Lyon base-vertex,
edge and round-join emission functions. Previously appended base vertices can
be emitted again to preserve the original interleaving; the expensive corner
geometry is not recomputed. Output preserves every vertex bit and index order,
including closing-edge duplicates. Arc output is fresh, not a cached layout.

This path only handles coarse arcs with tolerance at least half the stroke
radius. Finer arcs, merged paths and unsupported inputs retain ordinary
tessellation. The no-arc certificate is unchanged. Reusable position/index
scratch has limits of 642 points and 3,072 indices, about 17.0 KiB of combined
payload, with checked emission bounds. The original 128-layout / 512 KiB cache
limits remain; the captured histories still use only two layouts / 610 bytes.
No additional cache capacity or rectangle-specific path is introduced.

### Pi comparison

Each Pi again runs both viewport sizes. For each size, executable order is old,
new, new, old. Each new-build pass runs all three policies in all six possible
orders, with 192 timed replays per policy/order and the complete 24-frame
history warmed before each block. This gives twelve paired comparisons per
fixture, viewport and device. The independent old executable is the preceding
two-mode topology prototype, not a rebuilt approximation.

| Device and logical viewport | Plain stroke CPU | Previous topology CPU | Single-pass CPU | Single-pass vs plain |
| --- | ---: | ---: | ---: | ---: |
| Pi 4, 1024×768 | 1.0203 ms | 1.2236 ms | 0.9802 ms | −3.93% |
| Pi 5, 1024×768 | 0.4029 ms | 0.4832 ms | 0.3962 ms | −1.68% |
| Pi 4, 800×480 | 4.3967 ms | 4.0901 ms | 3.7068 ms | −15.69% |
| Pi 5, 800×480 | 1.7715 ms | 1.6672 ms | 1.5177 ms | −14.33% |

All twelve pairs improve in each of the four close-up cases, and p95 stroke
CPU decreases too. Versus the independent executable's plain control, the
corresponding improvements are 3.09%, 1.55%, 15.05% and 14.41%. Plain-control
build/run drift is +0.87%, +0.13%, +0.76% and −0.10%; retain that distinction
rather than attributing every old/new difference to the algorithm. Savings
against same-build plain are about 40 / 7 microseconds at 1024×768 and
690 / 254 microseconds at 800×480 on Pi 4 / Pi 5. These exclude projection,
simulation, text/UI, GPU drawing and scanout.

The new path handles all 8,709 / 14,324 previously arc-refused close-up
occurrences at the two sizes. Cached admissions stay at 1,896 / 42,140;
2,474 / 5,345 merged paths still use ordinary fallback. Warm samples have no
cache misses, evictions or budget refusals. Early-game histories contain no
guarded strokes, and late-game histories only seven / twelve occurrences;
their changes are too small to establish an ordinary-gameplay benefit.

### Validation and reproduction

Each complete two-viewport candidate pass checks both policies against exact
vertex bits and index order for 74,907 original occurrences and 374,535
transformed cases **per policy**. Both passes on both devices succeed, as do
the workstation's own histories. The two Pis agree on per-frame geometry,
admissions and fallback counts. All 138,240 timed batch rows pass accounting
and memory-budget assertions. These are geometry checks, not new GPU image
comparisons or live pacing measurements.

The command from the preceding section now emits three modes; use
`--presentation-repeats 6` to cover all six policy orders. Its manifest is
version 2, and the CSV adds a `prepared` column distinguishing fresh arc
emission from cached triangle layouts. The analyzer also reads the old
executable's version-1 output.

All **708 client tests** pass (five manual audits ignored). All **204 vendor
tessellator tests** pass in debug and release, including partial-prefix
resumption, interleaved arcs, mixed fallback/cache use, fine-arc refusal and
maximum-point scratch bounds. The deterministic 3,000-polygon check exercises
both APIs twice per shape against the original tessellator. GPU clippy
completes with existing warnings; software-kiosk checking, the Pi release
build, formatting and script checks pass.

The RAM-only probes resume the original Clock processes with unchanged
installed client, CLI and settings hashes, unchanged boot IDs and zero service
restarts. Sampled temperatures are 68.17–70.60°C on Pi 4 and 47.95–52.90°C on
Pi 5. Both use the ondemand governor; boundary frequency samples range from
1.0–1.5 GHz and 1.7–2.4 GHz respectively, not an asserted fixed clock.

Artifacts are under `target/gpu-experiments/stroke-single-pass.tMiKSN2m/`:
`run-device.sh`, `analyze.mjs`, `report.mjs`, `report.txt`, both device result
directories and summaries, native results, build/test logs and source archives.
Remote directories are `/tmp/stroke-single-pass.dP7gKlMR/` and
`/tmp/stroke-single-pass.CQJeqLy1/`. The independent old executable hash is
`cf3c5f7484166cb022256c47ff70f43691ab5f91970f472c74edc16b992d8d1a`;
the candidate is
`f433a5ae3af9fcceace139d4ae076426e18c03efa46f1cb5e160acd329282ca8`.

Next, introduce an opt-in complete-renderer control, retaining the current
writer and independent old executable. Check uncapped encode/completed-draw
cost and exact images across moving zoom, ordinary Spacewars and non-Spacewars
scenes before evaluating live application CPU and pacing. Do not extrapolate
these isolated percentages to complete frames or change renderer defaults.

## Single-pass complete geometry integration

The complete geometry comparison confirms a **10.38% encoding CPU / 5.86%
completed-draw improvement for Pi 5 close-up zoom**, but not a general win.
Pi 4 close-up encoding regresses **1.11%** and completed drawing is essentially
unchanged. Keep the control opt-in: the earlier isolated stroke saving does
not necessarily survive integration, and these are not full-application timings.

The single-pass experiment is now available as session-only `bulk-single-pass`
and `bulk-single-pass-checked` stroke writers. This extends the preceding
isolated replay into the complete geometry renderer; it does not change the
`stock` default or saved preferences. The new path handles uncached closed
polygons only. Circles, lines, retained templates, culling, stroke tolerances
and cache admission remain unchanged. Unsupported inputs, merged paths and
fine arcs use the existing `bulk-static` writer.

The adapter allocates its state lazily and discards it when leaving the
single-pass writer. It preserves the 128-layout / 512 KiB key/index payload
limit and the bounded position/index scratch from the isolated experiment.
Current screen positions are recomputed, not cached. Frame vertex/index limits
are checked before appending an accepted mesh. The checked writer additionally
compares each accepted stroke's vertex bits and index order with ordinary Lyon;
use it for validation, not performance measurements.

For an active `retained-culled` session, compare
`spacewars-cli host gpu-stroke-writer bulk-static` with
`spacewars-cli host gpu-stroke-writer bulk-single-pass`, keeping
`spacewars-cli host gpu-joins exact` fixed. Software and unsupported geometry
modes retain the existing rejection/session-reset behavior. This integration
does not itself establish a live application CPU or frame-pacing benefit.

The full geometry probe uses:

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-round-joins --geometry-stroke-bulk \
  --geometry-stroke-static --geometry-stroke-single-pass \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 96 --presentation-repeats 4 \
  --geometry-output /tmp/single-pass-renderer-captures
```

The output directory must not exist. Use 800×480 for the Pi 5's logical
viewport. Add `--geometry-stroke-cases
clock-idle,clock-rain,pizza-300,pizza-5000,mixed-interleaved` for broader coverage.
This flag is separate from the isolated `--geometry-stroke-topology` experiment
and conflicts with the other alternative stroke-writer experiments.

The probe alternates both writer orders, with complete histories warmed before
timing. CPU encoding and completed GL drawing include every geometry pass and
minimap composition; completed drawing waits for `gl.finish()`. Source scene
generation, projection, simulation, text/UI, readback and display scanout are
outside these timings. The CSV reports cached layouts, fresh prepared arcs,
refusal reasons and cache resources separately from ordinary join counters.
Reading those counters and checking their budgets happen outside the timing
interval; maintaining them remains part of the measured renderer cost.

### Comparison and validation protocol

Each device runs old, new, new, old executables, with the Spacewars and broader
suites in every block. The new executable compares `bulk-static` with
`bulk-single-pass`; the independent old executable runs its existing
`bulk-static` / `bulk-prepared` pair. Only the old `bulk-static` samples are
the cross-build performance baseline. Four alternating-order repeats per
candidate block give eight same-build pairs per fixture and timing scope.
Both controls use exact joins, conservative culling, prepared plans and
bucketed streams. Native logical viewports differ: 1024×768 on Pi 4 and
800×480 on Pi 5. Their percentages are not a same-workload hardware comparison.

The untimed oracle covers ten stroke writers with three join policies each.
Vertex bits, index order, batches and retained-cache decisions must match.
Because the new path bypasses ordinary join calls, its counters are checked
separately: CPU and GPU paths must agree, checked-mode calls must cover every
accepted stroke, and cache/scratch limits remain enforced. The two timed modes
also require exact same-state GPU pixels. Cold-to-warm and zoom-return history
comparisons are separate from that requirement.

Native GPU validation passes at both viewport sizes. The 800×480 workstation
run retains a one-pixel cold-to-warm difference, maximum channel error 24,
shared by both controls; same-state comparisons remain exact. The 1024×768
native history comparisons are exact as well. These native runs are correctness
checks, not throughput evidence, because other build work was running.

All **712 client tests** pass (five manual audits ignored), as do **32 CLI
tests** and **204 release-mode vendor tessellator tests**. New coverage includes
lazy allocation, cache reuse, ordinary fallback, checked output, state release
on writer exit, frame-budget refusal without partial appends, and CLI flag
requirements/conflicts. Existing style/prefix and control/reset tests cover
both new writers. GPU clippy and the software-kiosk check succeed with warnings;
the latter includes an unused GPU-only `single_pass` helper. The ARM release
build, formatting and script syntax checks pass.

### Device results

Changes below are candidate versus same-build `bulk-static` means; negative
means less time. Small mixed-sign results should not be read as established
improvements. In particular, there is no consistent ordinary-gameplay or Pizza
completed-draw gain attributable to this writer.

| Fixture | Pi 4 encoding CPU | Pi 4 completed draw | Pi 5 encoding CPU | Pi 5 completed draw |
| --- | ---: | ---: | ---: | ---: |
| Spacewars early | −0.32% | +0.47% | −1.03% | +0.11% |
| Spacewars late | −0.06% | −0.57% | −0.47% | +0.11% |
| Close-up zoom | +1.11% | +0.04% | −10.38% | −5.86% |
| Clock idle | −4.26% | −0.03% | +2.67% | +0.68% |
| Clock rain | −1.24% | −0.70% | +0.92% | +0.52% |
| Pizza 300 | −0.03% | −0.10% | +0.10% | approximately 0% |
| Pizza 5,000 | +0.02% | approximately 0% | −0.02% | −0.17% |
| Mixed interleaved | +0.78% | −0.13% | −1.92% | −0.83% |

Pi 5 close-up encoding falls from **2.5688 to 2.3021 ms**, and completed drawing
from **4.8641 to 4.5789 ms**. All eight pairs improve in both scopes. Their p95s
fall from 2.6074 to 2.3366 ms and 5.0358 to 4.7442 ms. Against the independent
old executable's plain writer, the savings are 10.84% and 5.36%, respectively;
plain-control drift is −0.51% CPU and +0.53% completed wall time.

Pi 4 close-up encoding rises from **3.9062 to 3.9496 ms**. All eight pairs
regress, by 0.36–1.82%, and p95 rises from 3.9895 to 4.0384 ms. Completed
drawing is **12.2596 versus 12.2648 ms**, with mixed-sign pairs. The independent
old control likewise shows a 1.01% encoding regression; its plain-control CPU
drift is only −0.10%. This is not the isolated experiment's small Pi 4 win.
Additional adapter/output work is a candidate explanation, not yet an
attributed cause.

The native-viewport workloads differ substantially. Close-up frames average
89 cached-layout / 419 prepared-arc / 103 merged-fallback strokes on Pi 4,
versus 1,766 / 653 / 223 on Pi 5. Both warm histories use two layouts totaling
610 bytes, with no misses, evictions or budget refusals. Ordinary early/late
histories exercise only about 66/74 accepted strokes per frame. Counts include
all uncached polygon strokes, not just the isolated guarded-outline census.

Clock supplies a useful counterexample: its 88 strokes per frame use cached
layouts at the sampled Pi 4 viewport, but all 88 require fine-arc fallback at
the sampled Pi 5 viewport. On Pi 5 this adds about **10 microseconds encoding /
11–13 microseconds completed drawing** in both Clock cases, with every pair
regressing. On Pi 4, Clock idle encoding saves about 40 microseconds without a
completed-draw win. Rain's mean improves slightly, but its completed p95
worsens from 10.3372 to 10.7708 ms. Pizza never enters the new path; the apparent
4.5–5.2% cross-build Pi 4 encoding improvement is shared plain-control drift,
not a stroke-algorithm benefit. The mixed fixture uses 192 fresh prepared arcs
per frame and no layouts, with a small Pi 5 gain but no Pi 4 completed win.

All **98,304 timed rows** pass geometry/count/budget assertions. The candidate
passes **2,044 exact same-state GPU image comparisons** across both devices;
all **840 saved cross-run PNG comparisons** are byte-identical. The 32 separate
device self-history checks are exact too. Representative zoom and rainy Clock
captures were visually inspected on both Pis. Spacewars text is intentionally
omitted by this geometry probe. The oracle's peak layout payload is 30 entries /
9,150 bytes on Pi 4 and 33 / 10,137 bytes on Pi 5, below the unchanged limits.

Both RAM-only probes resume the original Clock processes with unchanged
installed client, CLI and settings hashes, unchanged boot IDs, zero service
restarts and no reported UI error. Boundary temperature samples are
66.22–69.63°C on Pi 4 and 47.95–55.65°C on Pi 5. Both retain the ondemand
governor; sampled CPU frequencies span 0.9–1.5 GHz and 1.7–2.4 GHz. These are
not fixed-frequency runs.

Artifacts: `target/gpu-experiments/stroke-single-pass-render.XE7StSlW/`, including
the runner/analyzer/report scripts, both summaries, all samples/captures,
native validation, test/build logs, source snapshots and both executables.
Remote copies are `/tmp/stroke-single-pass-render.46gkCHUn/` on Pi 4 and
`/tmp/stroke-single-pass-render.tYLWuubo/` on Pi 5. The independent old executable
hash is `f433a5ae3af9fcceace139d4ae076426e18c03efa46f1cb5e160acd329282ca8`;
the tested candidate is
`18dbc9074a8a5519dd253f12681f5ae49d17eed88f4530bad94ecc273819c8ed`.
The runtime source matches `source-built.tar`; subsequent edits are documentation.

Next, separate resolution/workload effects from device effects by running the
complete comparison at both viewports on both Pis. Attribute the Pi 4 integration
regression and Pi 5 fine-arc fallback cost before adding another policy or
changing defaults. Keep the exact-output controls. Live application CPU and
pacing checks remain necessary if the integrated tradeoff becomes worthwhile;
do not extrapolate the Pi 5 geometry saving to full frames.

## Single-pass viewport crossover

The native-size contrast follows the **viewport/workload more than the CPU
generation**. At 800×480, complete close-up geometry improves on both devices;
at 1024×768 it is flat on Pi 5 and slightly worse in encoding on Pi 4. This is
not a Pi-5-only optimization, but it still does not justify a default change.
The isolated stroke saving also survives in the exact executable used for the
complete renderer. Its small 1024×768 benefit does not survive integration.

### Fixed-build protocol

No runtime source or executable changes were made for this follow-up. Both Pis
run the preceding `18dbc907...` release executable, comparing `bulk-static`
with `bulk-single-pass` using the same exact joins, culling, prepared plans and
bucketed streams. Pi 4 runs viewport sizes 800, 1024, 1024, 800; Pi 5 runs
1024, 800, 800, 1024. Each block runs the three Spacewars fixtures, then Clock
idle/rain and mixed interleaved. Pizza is omitted here because the preceding
experiment already established that it does not enter this path.

Each suite uses 96 frames and four alternating writer-order repeats, giving
eight pairs per fixture, viewport and timing scope. The full image/geometry
oracle, odd-size resize, zoom-return and terrain mutation checks remain enabled.
The preceding complete-renderer command reproduces each block: run it at both
`--benchmark-width 800 --benchmark-height 480` and
`--benchmark-width 1024 --benchmark-height 768`, with separate output directories;
add `--geometry-stroke-cases clock-idle,clock-rain,mixed-interleaved` for the
second suite.

These sizes change aspect ratio and projected geometry as well as pixel count.
They are not a pixel-count-only experiment or a proposal to lower display
resolution. At a given viewport, all **18,432 cross-device timing rows** have
identical vertex/index/upload counts and algorithm counters. Encoding measures
thread CPU; completed drawing measures wall time through `gl.finish()`, including
encoding, driver and GPU work. Neither measures simulation, source/projection,
text/UI, readback or scanout, nor pure GPU execution time.

### Complete geometry results

Changes are same-build candidate versus reference means; negative means less
time. Both 800×480 cases improve in all eight encoding and completed-draw pairs.

| Device and viewport | Close-up encoding CPU | Change | Completed geometry wall time | Change |
| --- | ---: | ---: | ---: | ---: |
| Pi 4, 800×480 | 7.0872 → 6.5073 ms | −8.18% | 13.9115 → 13.3507 ms | −4.03% |
| Pi 4, 1024×768 | 3.8973 → 3.9830 ms | +2.20% | 12.2391 → 12.2332 ms | −0.05% |
| Pi 5, 800×480 | 2.5665 → 2.3146 ms | −9.81% | 4.8290 → 4.5587 ms | −5.60% |
| Pi 5, 1024×768 | 1.3072 → 1.3086 ms | +0.11% | 4.0354 → 4.0464 ms | +0.27% |

At 1024×768, Pi 4 encoding regresses in all eight pairs, but its magnitude varies:
the first block is +0.07–0.83%, the second +3.82–4.07%. Its preceding native-size
run measured +1.11%. Do not interpret +2.20% as a stable, precisely attributed
adapter cost. Completed-draw pairs remain mixed; Pi 5's 1024×768 results are
likewise effectively flat.

Close-up frames average **1,766 cached / 653 prepared / 223 merged-fallback
strokes** at 800×480, versus **89 / 419 / 103** at 1024×768, on both CPUs.
Cached-layout use is about 67% versus 15% of those attempts. Both warm histories
still use just two layouts, totaling 610 bytes, without misses or evictions.
More cache capacity is not the remedy.

Clock's direction also follows the viewport on both CPUs. At 800×480 all 88
strokes per frame require fine-arc fallback; idle/rain encoding regresses
3.52%/1.35% on Pi 4 and 2.65%/0.94% on Pi 5, roughly **34–38 / 10 microseconds**
respectively. At 1024×768 all 88 use cached layouts, saving 5.15%/1.50% encoding
on Pi 4 and 4.04%/1.41% on Pi 5. The smaller CPU savings do not guarantee better
completed drawing: Pi 4 idle remains mixed, and rain's completed p95 worsens
10.2238 → 10.8205 ms despite a 0.44% mean improvement.

Some device sensitivity remains: mixed interleaved uses 192 fresh prepared arcs
and no layouts at either viewport. Encoding regresses 0.57–0.78% on Pi 4 but
improves 1.69–1.87% on Pi 5. Ordinary Spacewars still has no convincing
completed-draw gain. These are reasons to measure adapter and fallback work,
not to add a resolution/device-name heuristic.

### Isolated control from the same executable

To rule out simply losing the earlier algorithmic benefit in this build, each
device also runs the existing isolated guarded-stroke replay at both sizes.
It uses 192 frames and six repeats, covering all six orders of `plain`,
`topology` and `single-pass`. Pi 4 runs 800 then 1024; Pi 5 reverses that order.
Use the same executable and sizes with:

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-stroke-attribution --geometry-stroke-topology \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 192 --presentation-repeats 6 \
  --geometry-output /tmp/same-build-isolated-captures
```

| Device and viewport | Isolated stroke CPU, plain → single-pass | Change | Saving |
| --- | ---: | ---: | ---: |
| Pi 4, 800×480 | 4.4048 → 3.6688 ms | −16.71% | 736 µs |
| Pi 4, 1024×768 | 1.0175 → 0.9565 ms | −6.00% | 61 µs |
| Pi 5, 800×480 | 1.7664 → 1.5171 ms | −14.12% | 249 µs |
| Pi 5, 1024×768 | 0.4079 → 0.3888 ms | −4.67% | 19 µs |

All six pairs improve in each close-up case. The corpus, selection decisions,
geometry counts and cache resources match the preceding isolated executable
exactly. Early/late isolated histories contain zero or almost zero guarded
strokes; their microsecond/sub-microsecond results are not useful throughput
evidence.

This confirms that a same-build isolated saving can coexist with a complete
renderer regression. It does **not** identify an exact culprit or justify
subtracting these separate measurements into a per-call overhead estimate.
In particular, isolated replay already copies fresh positions into colored
vertices and rebases indices. That conversion is not new integration work.
The real adapter adds state access, budget/check branches and counters, while
the complete renderer interleaves more shapes, fills and cache expansion.
Call-site/code layout and scheduling are also possible contributors.

### Validation, artifacts and next step

All **73,728 complete-geometry timing rows** pass accounting/budget checks;
**4,056 same-state GPU image comparisons** and **528 saved repeat PNG checks**
are exact. Of 32 separate self-history checks, Pi 5's 1024×768 zoom return
changes one pixel, maximum channel error 23, identically in both controls and
both repeated blocks. All other device self-history checks are exact.
Cross-GPU saved images are not assumed bit-identical: 174 of 264 are identical;
the other 90 differ at at most nine pixels (0.00115%), maximum channel error 57,
with unchanged alpha. Each device's candidate/reference pair remains exact.
Representative close-up and rainy Clock captures at the crossed sizes were
visually inspected; the geometry probe intentionally excludes text.

The isolated replay adds **41,472 checked timing rows**, with all work counters
matching the earlier executable, and **149,814 original / 749,070 transformed
exact geometry cases per candidate policy** across both devices. Full-renderer
validation peaks at 33 layouts / 10,137 bytes, below the unchanged 128-layout /
512 KiB bounds. Runners pass Bash syntax/ShellCheck and analyzers pass their
syntax and result assertions. No runtime source changed or unit suite was rerun;
the preceding build/test results apply to this identical executable/source.

Both probes resume the original Clock processes. Installed client, CLI and
settings hashes, boot IDs and service PIDs are unchanged, with zero restarts and
no final UI error. Crossover boundary temperatures are 66.705–69.627°C on Pi 4
and 47.4–55.65°C on Pi 5; isolated runs reach 70.114°C and 51.8°C. Governors
remain ondemand, so these are not fixed-frequency measurements.

Artifacts: `target/gpu-experiments/stroke-crossover.jNpo4vFe/` contains runners,
analyzers, summaries/reports, all samples/captures, cross-GPU pixel analysis and
input hashes. Crossover remote directories are
`/tmp/stroke-crossover.OrHRA8z8/` (Pi 4) and
`/tmp/stroke-crossover.HXsYFzrg/` (Pi 5); same-build isolated controls are
`/tmp/stroke-same-build.YO99rkjL/` and `/tmp/stroke-same-build.jrMzVpgW/`.
The executable is unchanged:
`18dbc9074a8a5519dd253f12681f5ae49d17eed88f4530bad94ecc273819c8ed`.
Runtime files still match the preceding `source-built.tar`.

Next, replay the same captured strokes through the **actual renderer adapter**
as a separate control, retaining the direct isolated and ordinary paths. Split
accepted-layout, fresh-arc and refused workloads, including Clock's fine-arc
case, to locate overhead without per-vertex clocks. Compare exact output and
unprofiled throughput before changing the implementation. Keep defaults,
tolerances and cache budgets unchanged; full-app CPU/pacing checks still follow
only if the integrated tradeoff becomes worthwhile.

## Actual stroke adapter replay

The actual renderer adapter keeps most of the isolated single-pass saving;
its measured overhead is small compared with the difference between cached
layouts and fresh/refused work. That narrows the investigation but does not
fully explain the preceding complete-renderer result at 1024×768. No production
adapter, tessellator, cache limit, tolerance or default changes in this step.

### Controls and scope

`--geometry-stroke-adapter` adds a benchmark-only control using the real
`stroke_single_pass::State::append`, with the renderer's lazy boxed state,
budget/check branches and cumulative counters. The other controls are ordinary
static Lyon with bulk output (`plain`) and the unchanged direct single-pass
replay (`direct`). All three write colored vertices and rebased indices. The
adapter uses ordinary static Lyon when `append` declines a stroke.

Each Spacewars case uses the same 24-frame guarded-outline capture as the
existing isolated probe. Clock idle/rain each add one frozen production frame's
88 ordinary projected polygon outlines. Fills, circles, lines, source/point
projection, retained expansion, simulation, UI, GPU drawing and presentation
are outside this CPU timing. This is not another whole-frame or GPU result.

Outside timing, classify outlines into cached layouts, freshly prepared arcs,
unsupported, merged, fine-arc, nonfinite and budget refusals. Each nonempty group
retains original order and frame boundaries; also measure the complete original
sequence as `all`. Filtering changes interleaving, so group timings must not be
added together as full-frame attribution. Empty groups are skipped. The original
8,192-draw / 8 MiB frame and 128 MiB history capture limits still apply; only one
subset clone is held at a time.

Every group gets two cold/warm validation traversals through all three controls:
exact colored vertex bits and index order against the captured ordinary output,
matching selection/cache resources, plus the real adapter's checked-mode oracle
on every accepted stroke. Timing disables those oracles and has no per-stroke
clocks; cumulative counter reads are outside the timed frame replay. Warm every
control's complete history before measuring 192 frames. Six repeats cover all
six control orders.

Both Pis run old/new/new/old executable blocks at both viewports. Pi 4 visits
800×480 then 1024×768; Pi 5 reverses that order. The old executable runs the
unchanged isolated three-control probe, providing build/call-site drift controls.
The new executable runs the adapter/groups probe. Reproduce its blocks with:

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-stroke-attribution --geometry-stroke-topology \
  --geometry-stroke-adapter --benchmark-width 800 --benchmark-height 480 \
  --presentation-frames 192 --presentation-repeats 6 \
  --geometry-output /tmp/stroke-adapter-captures
```

Use a fresh output directory and also run 1024×768. The runner in the artifacts
records the complete block order and safely suspends/resumes the installed app.

### Results

Mean thread CPU time per replayed close-up frame (`all` group), in milliseconds:

| Device and viewport | Plain | Direct single-pass | Actual adapter | Adapter versus plain | Adapter minus direct |
| --- | ---: | ---: | ---: | ---: | ---: |
| Pi 4, 800×480 | 4.3655 | 3.7066 | 3.7327 | −14.50% | +26.1 µs |
| Pi 4, 1024×768 | 1.0098 | 0.9648 | 0.9743 | −3.51% | +9.5 µs |
| Pi 5, 800×480 | 1.7686 | 1.5174 | 1.5307 | −13.45% | +13.3 µs |
| Pi 5, 1024×768 | 0.4100 | 0.3962 | 0.3991 | −2.64% | +2.9 µs |

All twelve adapter-versus-plain pairs improve in each of these four cases.
Adapter versus direct means are +0.70–0.99%, but not every pair has that sign:
Pi 4 at 1024×768 has one direct-run outlier giving −5.40%; the other eleven
pairs are +0.83–2.67%. This is not a precise universal per-call overhead.

The separated close-up groups explain where the useful saving resides. These
are adapter-versus-ordinary CPU changes, **not additive frame contributions**:

| Device and viewport | Cached layouts | Fresh prepared arcs | Merged fallback |
| --- | ---: | ---: | ---: |
| Pi 4, 800×480 | −24.68% | −1.65% | +5.74% |
| Pi 4, 1024×768 | −25.16% | −1.57% | +5.57% |
| Pi 5, 800×480 | −21.62% | −0.26% | +5.82% |
| Pi 5, 1024×768 | −22.73% | −0.31% | +6.52% |

Fresh-arc pairs on Pi 5 have mixed signs; do not sell the sub-percent mean as a
reliable win. Pi 4's direct prepared-arc control at 1024×768 varies between
blocks, even reversing the adapter/direct comparison; its adapter/plain pairs
remain modestly favorable. This reinforces the distinction between measured
control costs and a proven cost of individual branches or counters.

The per-history selection totals are identical on both devices and across
old/new builds: 800×480 has 61,809 attempts, comprising 42,140 cached, 14,324
prepared and 5,345 merged. At 1024×768 those counts are 13,079 / 1,896 / 8,709 /
2,474. Each close-up cache uses two layouts / 610 bytes with no warm misses or
evictions. Early Spacewars has no captured guarded outlines; late has only 12
or seven across 24 frames. Those mostly empty cases are not throughput evidence.

Clock at 800×480 repeats the fine-arc cost on both CPUs: its 88 outlines take
about 15% more stroke CPU through the adapter, **26 µs on Pi 4 and 10 µs on
Pi 5**. Only about 1.1–1.4 µs / 0.3–0.4 µs separates the adapter from direct
replay. At 1024×768 all 88 use one 281-byte layout, saving roughly 25% of stroke
CPU, **34–35 µs / 13 µs**. Idle and rain agree; rain's fills/water are deliberately
excluded. Neither percentage describes total Clock rendering cost.

The old/new controls also show why cross-build subtraction is unsafe. Close-up
plain/direct means move −0.96%/+0.60% on Pi 4 at 800, −0.36%/+0.21% at 1024,
−0.05%/+0.08% on Pi 5 at 800, and +1.74%/+2.08% at 1024. Per-frame work counts
and selection decisions match. The actual adapter's same-build saving remains,
but code layout/call site, buffer/cache interleaving and scheduling in the
complete renderer are not reproduced by this isolated loop.

### Validation and follow-up

All **387,072 timing rows** pass the analyzer's accounting checks: 304,128 new
probe rows and 82,944 old controls. It verifies 55,296 comparable cross-build
rows and 76,032 cross-device rows, with repeated-block counts also identical.
The real adapter makes **1,076,096 checked accepted calls** across device
validation traversals and groups; this includes repeated checks, not that many
unique outlines. All ordinary/candidate per-stroke geometry comparisons pass.
Warm cache misses, evictions and budget refusals are zero. No new GPU screenshots
were needed for this CPU-only experiment; preceding image checks are not
presented as newly rerun image tests.

Five new tests cover command-line requirements/conflicts, group coverage/order,
empty frames, lazy state, transformed current geometry, deliberate oracle
corruption, and Clock's viewport-dependent classifications. Full GPU-client
tests pass **717 tests, five ignored**; native build, Clippy, formatting and the
ARM release build pass (existing warnings remain). Native smoke runs validate
both viewports, and the original native capture agrees with the new probe.
Runner Bash/ShellCheck and analyzer syntax/assertions pass.

Both original Clock processes resume without a UI error. Installed app/CLI and
settings hashes, boot IDs, service PIDs and zero restart counts are unchanged.
Boundary temperatures are 67.679–70.114°C on Pi 4 and 48.5–52.35°C on Pi 5.
Both governors remain ondemand; sampled frequencies vary, so these are not
fixed-frequency measurements.

Artifacts: `target/gpu-experiments/stroke-adapter.C6x5VRrI/` contains the runner,
analyzer/report, all samples/manifests, build/test logs and source snapshots.
Remote directories are `/tmp/stroke-adapter.pOGzlA6K/` (Pi 4) and
`/tmp/stroke-adapter.bZv5ilwZ/` (Pi 5). Executable SHA-256s:

- Old: `18dbc9074a8a5519dd253f12681f5ae49d17eed88f4530bad94ecc273819c8ed`.
- New: `b4fbbfa558b28cfb0683bdff43fa023cc06480355a53a3d3d88c57e9c24180dd`.

Next, test whether obviously ineligible strokes can reach ordinary tessellation
without paying for speculative preparation first, using current geometry/options
rather than a device or resolution rule. Keep cached-layout eligibility exact
and measure the cost imposed on successful attempts as well as refusals. Retain
fresh-arc and ordinary controls; these results do not justify dropping either
globally. A worthwhile candidate must then survive complete-renderer comparison
at both sizes, followed by live CPU/pacing checks. Adapter bookkeeping alone is
not an established explanation for the preceding integration regression.

## Early fine arc refusal

The opt-in single-pass stroke path can reject a fine-arc request before setting
up endpoint records or computing its first corner. The existing implementation
already requires `tolerance >= line_width * 0.25` for both arc-free certification
and freshly prepared arcs. A fixed-width round join always leaves its front
side without a single vertex; folding that side does not suppress its own arc
test. Below that bound, the original preparation therefore rejects its first
corner. Checking the same bound earlier changes no accepted geometry or quality
setting and requires no rectangle, viewport or device rule.

The check follows the existing style, finite-coordinate and endpoint-merge
validation, preserving refusal priority and counters. It applies only to the
single-pass experiment. The topology-only experiment retains the original
preparation path; a compile-time switch also keeps that path available as the
differential test oracle. Ordinary tessellation, cached-layout keys, freshly
prepared geometry, output budgets and defaults are unchanged.

Two new tests compare 18,000 deterministic geometry/style cases, including the
adjacent f32 values on both sides of the tolerance cutoff, winding changes,
self-intersections, merged endpoints and large translations. The suite requires
substantial accepted, merged and fine-arc coverage. Classifications match the
old preparation; successful/prefix geometry matches bit-for-bit. Fine-arc
refusals now prepare zero corners instead of one. Separate invalid-input and
warm-cache checks preserve error priority and layout reuse after a refusal.
All 206 release tessellator tests and 717 client tests pass (five client tests
remain ignored). Desktop probes at both sizes preserve the old capture metadata
and pass 136 exact GPU image comparisons across Clock and mixed animation.

### Device comparison protocol

Both Pis run the preceding actual-adapter release and the new release from
temporary directories without replacing the installed application. Isolated
stroke replay uses old/new/new/old blocks at each viewport, 96 frames and six
control-order repeats per block. Both builds use the actual-adapter probe,
so its three controls and filtered groups are identical. Pi 4 visits 800×480
first; Pi 5 visits 1024×768 first.

Complete rendering then compares ordinary bulk-static and opt-in single-pass
in each build, using 48 frames and four alternating writer-order repeats for
Spacewars early/late/close-up and Clock idle/rain/mixed animation. Build order
is old/new at 800×480 and new/old at 1024×768. Full geometry, pixels, odd-size
resize, zoom-return and terrain-edit checks remain enabled. CPU encoding and
wall time through completed GL drawing are separate scopes; source generation,
projection, simulation, text/UI, readback and scanout are excluded.

The preceding actual-adapter command reproduces isolated blocks with
`--presentation-frames 96`. For full blocks use the preceding complete-renderer
command with `--presentation-frames 48 --presentation-repeats 4`; add
`--geometry-stroke-cases clock-idle,clock-rain,mixed-interleaved` for the broad
suite. Run both executables at both sizes with fresh output directories. Keep
the ordinary control in each build: cross-build changes alone do not isolate
the cost of the new branch.

### Results and limits

The targeted fallback becomes cheaper on both CPUs. At 800×480, all 88 captured
Clock outlines still fall back for fine arcs. Mean stroke-replay thread CPU,
in microseconds; overhead is adapter minus plain within the same build:

| Device / Clock fixture | Old adapter | New adapter | Old overhead | New overhead |
| --- | ---: | ---: | ---: | ---: |
| Pi 4 / idle | 198.47 | 182.01 | 26.32 | 9.83 |
| Pi 4 / rain | 198.10 | 181.63 | 26.24 | 9.54 |
| Pi 5 / idle | 76.16 | 70.84 | 9.86 | 3.10 |
| Pi 5 / rain | 76.13 | 70.87 | 9.89 | 3.35 |

The old/new overhead ranges do not overlap across the twelve within-build
pairs per fixture. New overhead is 8.63–11.12 µs on Pi 4 and 2.72–3.83 µs on
Pi 5, versus 25.27–27.14 / 9.66–10.07 µs before. The actual adapter therefore
costs about 8.3% / 7.0% less than its previous build in these groups, but still
costs 5.5–5.7% / 4.6–5.0% more than plain tessellation. This removes much of
the speculative setup, not the entire fallback cost. Pi 5's plain control also
moves about 2%; do not equate the reduction in relative overhead with a pure
cross-build saving. Rain's fills and water are absent from this isolated scope.

At 1024×768, all 88 Clock outlines still use one cached layout. Their isolated
adapter/plain saving remains about 26.5–26.8% on Pi 4 and 24.1–24.3% on Pi 5.
The change does not make more strokes eligible or increase cache reuse.

Successful attempts do not show a universal improvement. Close-up adapter
means move −0.76% / +0.59% across builds on Pi 4 at 800 / 1024 and +0.83% /
−0.04% on Pi 5. In particular, the Pi 5 800 case gets 12.6 µs slower while
plain is unchanged. Cached-layout groups remain beneficial; prepared-arc
results are modest and still have mixed pair signs on Pi 5. Merged fallback
still costs roughly 5–7% more than plain. There are no fine-arc refusals in
these close-up histories, so their shifts are not savings from the new early
exit. They also show why isolated group timings are not additive attribution.

Complete geometry confirms the limited scope. Each entry below is the
single-pass writer's percentage change versus bulk-static in that build;
negative is faster. These are within-build comparisons, not old/new subtraction:

| Device and viewport / close-up | Old encode CPU | New encode CPU | Old completed wall | New completed wall |
| --- | ---: | ---: | ---: | ---: |
| Pi 4, 800×480 | −7.49% | −8.36% | −4.51% | −4.37% |
| Pi 4, 1024×768 | +1.42% | +1.16% | +0.59% | +0.01% |
| Pi 5, 800×480 | −9.66% | −10.28% | −5.80% | −5.99% |
| Pi 5, 1024×768 | −0.25% | −0.37% | +0.40% | +0.99% |

All four new-build close-up pairs improve in both scopes at 800×480. At
1024×768, Pi 4 encoding is still consistently slower; Pi 5's encoding and
completed pairs have mixed signs. This does not resolve the earlier
viewport-dependent integration result. The new 800 means are 7.086 → 6.494 ms
encoding / 13.824 → 13.220 ms completed on Pi 4 and 2.574 → 2.309 / 4.803 →
4.515 ms on Pi 5. These are rendering capacity measurements, not 60-FPS caps,
whole-app CPU, scanout latency or isolated GPU time.

For full Clock at 800×480, single-pass's encoding penalty shrinks from 5.82%
to 0.52% (idle) and 1.71% to 0.94% (rain) on Pi 4; Pi 5 goes from 2.50% to
0.72% and 1.48% to 0.20%. Completed-drawing means are within 0.75% of the
ordinary control in the new build, with mixed pair signs in several cases.
At 1024×768 Clock retains its encoding benefit, but completed drawing is not
universally faster. Mixed animation likewise remains worse on Pi 4: about
1.4–1.5% more encoding CPU, versus 1.8–2.1% less on Pi 5. Early/late Spacewars
means remain small and pair results often vary. Keep these non-wins in view;
this is not evidence for making single-pass the default everywhere.

### Validation and evidence

All **340,992 timing rows** pass accounting: 304,128 isolated and 36,864 full
geometry rows. Selection decisions, output counts, uploads and cache counters
match across builds and across CPUs at each viewport. The isolated traversals
make **2,152,192 checked accepted calls**, including repeats/groups rather than
that many unique strokes. Warm isolated misses/evictions and all budget
refusals remain zero. Full validation peaks at 33 layouts / 10,137 accounted
bytes, unchanged between builds.

All **4,056 same-state GPU image comparisons** pass exactly, as do **528 saved
old/new PNG comparisons**. The 32 additional self-history checks reproduce the
known Pi 5 1024 zoom-return difference: one pixel, maximum channel difference
23, shared identically by both writers in both builds. This is separate from
candidate/reference equality, not a new candidate error. Clock rain, close-up
and terrain replacement captures were also inspected. The desktop's 136 image
checks and 206 tessellator / 717 client tests above pass; native build, Clippy,
formatting and the ARM release build pass with existing warnings.

Both original Clock processes resume unpaused with no UI error. Installed
app/CLI and settings hashes, boot IDs, service PIDs and zero restart counts
are unchanged. Boundary temperatures range 67.679–70.114°C on Pi 4 and
49.05–55.65°C on Pi 5; both governors remain ondemand and sampled frequencies
vary. No installed binary, saved setting or rendering default was changed.

Artifacts: `target/gpu-experiments/stroke-early-refusal.2FX0Ox1e/` contains
both executables, source snapshots, build/test logs, native captures, the
device runner, raw samples/manifests/images and `analyze.mjs` / `report.mjs`
with combined `summary.json` / `report.txt`. Remote evidence remains in
`/tmp/stroke-early-refusal.XMvU9GMi/` (Pi 4) and
`/tmp/stroke-early-refusal.Fk1GoEFL/` (Pi 5). Executable SHA-256s:

- Old: `b4fbbfa558b28cfb0683bdff43fa023cc06480355a53a3d3d88c57e9c24180dd`.
- New: `03d1ff3ad3254fca4bb8fedc319259701e3f20adc6faf927a3c39c4cbf7e37ee`.

Keep the cheaper guaranteed refusal in the opt-in experiment, with its old
preparation oracle and ordinary fallback. Do not spend another iteration
chasing the last few Clock fallback microseconds without a larger full-scene
reason. Next, sample the complete encoder in the 1024×768 close-up case to
locate the remaining cost that isolated replay does not explain, retaining
800×480 as a positive control. Live CPU/pacing verification remains necessary
before promoting any single-pass policy; no device-name or resolution shortcut
is justified by these measurements.

## Complete encoder stroke attribution

Full-encoder profiling narrows the lost 1024×768 benefit to the guarded-stroke
work in context, rather than a large saving hidden by extra ordinary objects
or topology-cache churn. Instrumentation itself is costly, so the standard
uninstrumented full-render probe remains the performance control.

The `--geometry-stroke-encoder-profile` diagnostic compares the complete
encoder with and without nested clocks, using the existing bulk-static and
single-pass writers. It retains the same 24-frame early/late/close-up histories
as the full stroke probe. This separates the terrain outline work measured by
isolated replay from ordinary objects, retained geometry expansion, validation
and encoder traversal. The production unprofiled path keeps ordinary append
calls without the new per-draw clocks or counter snapshots.
Explicit detailed live captures also use this shared instrumentation; basic
captures and normal draws do not. Old and new detailed traces therefore have
different instrumentation costs even when their rendered output is identical.

Whole-encoder samples record both thread CPU and elapsed wall time. The nested
spans are **elapsed wall time**, not per-stage thread CPU. Eight top-level spans
are disjoint; guarded-stroke and ordinary-object preparation/fill/stroke/finish
are nested inside their parents. Offscreen stroke time is also a subset.
Residual time includes traversal, clock/counter bookkeeping and other unlabelled
work. Do not add nested spans again or interpret the residual as a new rendering
stage. Ordinary-object attempt counts are a subset of total adapter attempts.

Each run compares four modes in counterbalanced order: static plain, static
profiled, single-pass plain, single-pass profiled. Each mode warms the complete
history before sampling. Source generation, projection, simulation, GL, UI,
readback and scanout are outside the timed encoder scope. The late fixture
readmits retained meshes as its working set cycles; those admissions are
compared across modes rather than assumed to disappear after warmup.

Reproduce one block with a GPU-enabled build and a fresh output directory:

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-round-joins --geometry-stroke-bulk \
  --geometry-stroke-static --geometry-stroke-single-pass \
  --geometry-stroke-encoder-profile \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 96 --presentation-repeats 8 \
  --geometry-output encoder-1024
```

Repeat at 800×480 as the positive control. Per-pass checks require identical
mesh bits, indices, batches, cache decisions and same-writer adapter counts
with profiling on or off. Existing independent checked-writer and GPU image
comparisons cover each moving frame plus odd/native resize and zoom changes:
84 exact same-state images per block. Three new unit tests check optional
instrumentation/count accounting, CLI constraints and nested-span accounting
without wall-clock thresholds.

### Device protocol

Temporary release binaries run with the installed Clock process paused and
stopped, then resumed by the runner's exit trap. No installed binary or saved
setting is replaced. Pi 4 visits 800/1024/1024/800; Pi 5 visits
1024/800/800/1024. Each viewport therefore has sixteen writer/profile-order
repeats, with 96 timed frames per mode per repeat.

A separate control then runs the preceding and current executable through the
existing uninstrumented full-render probe at 1024×768, 48 frames and four
alternating writer-order repeats. Build order is old/new on Pi 4 and new/old on
Pi 5. CPU-only encoding and completed-GL wall time remain separate. This checks
build drift in addition to the within-build instrumentation cost. For comparison
between probe types, use the diagnostic's first 48 frames/four repeats, not its
longer window: late-game mesh-cache churn changes the average over successive
history cycles. Matched output, admission and topology counters must agree.

### Encoder results and attribution limits

The uninstrumented diagnostic controls reproduce the 1024×768 result: Pi 4
single-pass encoding is slower and Pi 5 is effectively tied. Close-up thread
CPU means in milliseconds; changes compare writers within the same executable:

| Device / viewport | Static plain | Single-pass plain | Change | Faster pairs |
| --- | ---: | ---: | ---: | ---: |
| Pi 4 / 800×480 | 7.089 | 6.910 | −2.52% | 12/16 |
| Pi 4 / 1024×768 | 3.863 | 3.952 | +2.31% | 0/16 |
| Pi 5 / 800×480 | 2.560 | 2.351 | −8.17% | 16/16 |
| Pi 5 / 1024×768 | 1.313 | 1.317 | +0.31% | 4/16 |

Pi 4's 1024 pairs range +1.35% to +2.86%; Pi 5's range −0.19% to +1.03%.
The 800 Pi 4 diagnostic is notably less favorable and less stable than the
earlier standard full-render probe, with pair changes −5.96% to +1.32% and
essentially unchanged p95 (7.196 → 7.205 ms). The separate uninstrumented
full-render check below confirms that these diagnostic percentages must not
replace the existing performance baseline.

Detailed clocks materially perturb the workload. At 1024×768 they add roughly
15–17% to close-up CPU on Pi 4 and 27% on Pi 5; at 800×480 they add 33–38% /
56–61%. Instrumented and uninstrumented deltas are not interchangeable. The
following nested **elapsed-wall** means locate work in the instrumented
encoder; they are not isolated CPU costs or an additive explanation of the
uninstrumented saving:

| Device / viewport | Guarded stroke, static → single-pass | Ordinary objects, static → single-pass |
| --- | ---: | ---: |
| Pi 4 / 800×480 | 4.458 → 3.776 ms | 0.623 → 0.639 ms |
| Pi 4 / 1024×768 | 1.017 → 1.019 ms | 0.679 → 0.696 ms |
| Pi 5 / 800×480 | 1.825 → 1.604 ms | 0.243 → 0.243 ms |
| Pi 5 / 1024×768 | 0.409 → 0.410 ms | 0.262 → 0.262 ms |

Within the instrumented runs, extra ordinary geometry is not hiding a large
1024 terrain-stroke saving: that saving is already absent inside the
guarded-stroke span. Ordinary work adds about 18 µs on Pi 4 and is flat on
Pi 5. Both sizes make
the same additional 66 ordinary polygon attempts per frame: ten cached layouts
and 56 fresh arcs. The topology cache remains at two entries / 610 accounted
bytes during timing, without misses, evictions or budget refusals. Close-up
retained-mesh admissions are also zero. This is not cache-capacity pressure.

The guarded work mix changes substantially. At 800×480, a close-up frame has
about 2,575 guarded outlines: 1,756 cached layouts, 597 fresh arcs and 223 merged
fallbacks. At 1024×768 there are about 545: 79 cached, 363 fresh and 103 merged.
Reusable layouts fall from 68% to 14% of guarded attempts. Scalable expansion
then emits about 38,761 vertices instead of 19,751, while guarded outlines emit
7,740 instead of 36,403. At 1024, scalable expansion alone takes about
0.93 / 0.31 ms instrumented wall time on Pi 4 / Pi 5, unchanged between
writers. The cache-friendly portion is much smaller; the previous isolated
replay still does not establish why its remaining small benefit disappears in
context. These timings do not prove a hardware cache or allocator mechanism.

Early/late gameplay remains important: the diagnostic's single-pass CPU cost
is 0.5–2.3% higher on Pi 4 and 0.2–1.2% lower on Pi 5. Late-game cache resolution
dominates both writers (about 2.5 / 0.9 ms at 1024); optimizing outline output
cannot remove that cost. The new diagnostic is not evidence for a universal
writer policy or for increasing the topology cache.

### Independent full rendering controls

The extra 800×480 old/new check uses the same 48-frame/four-repeat full-render
probe, again old/new on Pi 4 and new/old on Pi 5. It preserves the positive
control in every pair. Within-build single-pass changes versus bulk-static:

| Device / viewport | Old encode CPU | New encode CPU | Old completed wall | New completed wall |
| --- | ---: | ---: | ---: | ---: |
| Pi 4 / 800×480 | −8.15% | −6.57% | −4.48% | −3.10% |
| Pi 4 / 1024×768 | +0.97% | +1.80% | +0.61% | +0.38% |
| Pi 5 / 800×480 | −10.36% | −10.18% | −5.79% | −5.89% |
| Pi 5 / 1024×768 | −0.10% | +0.33% | −1.24% | +0.48% |

New-build 800 means are 7.042 → 6.579 ms encode / 13.751 → 13.324 ms
completed on Pi 4, and 2.586 → 2.323 / 4.807 → 4.524 ms on Pi 5. These are
uncapped geometry measurements, not full-app CPU, scanout or GPU-only time.
The 1024 measurements retain the lack of a useful general improvement.

The matched 800 diagnostic window costs 5.61% more than the standard probe for
Pi 4 single-pass, versus only 0.50% more for static. Its output, admission and
topology counters match exactly. The Pi 5 discrepancy is smaller: single-pass
+1.18%, static −0.99%. At 1024 the matched-window differences stay within
1.28% on Pi 4 and 0.41% on Pi 5 across all three fixtures. Interleaving detailed
and plain modes is therefore not an interchangeable timing environment,
especially for the large Pi 4 close-up. This experiment does not identify the
specific compiler/cache/allocation mechanism. Keep the independent full probe
as the speed comparison and report the profiling overhead with attribution.

There is also modest build drift: Pi 4 single-pass close-up encoding costs
1.36% / 1.59% more in the new binary at 800 / 1024, while static moves −0.34% /
+0.76%. Pi 5 changes +0.14% / +1.04% for single-pass. The normal path contains
no new nested clock calls, but this does not establish zero whole-binary
performance impact. The larger diagnostic structs/code and changed execution
context remain possible influences, not measured causes. Do not credit these
cross-build shifts to a stroke optimization; this step adds instrumentation,
not a new writer algorithm.

### Validation and next experiment

Across both rounds, **92,160 timing rows** pass accounting, including 9,216
matched-harness count comparisons and 20,736 cross-device count comparisons.
All **4,184 same-state GPU comparisons** and **480 saved old/new PNG comparisons**
pass exactly. The 32 separate self-history checks retain the known Pi 5
1024 zoom-return one-pixel / maximum-channel-23 difference, identical between
writers and builds. Native runs add 168 exact GPU comparisons. Close-up and
terrain-replacement captures were also inspected.

All 720 client tests pass (five ignored), with native build, Clippy, formatting
and ARM release build passing with existing warnings. No tessellator algorithm
changed in this step. Both original Clock sessions resume unpaused with no UI
error after each round. Installed app/CLI/settings hashes, boot IDs, service
PIDs and zero restart counts remain unchanged. Sampled boundary temperatures
span 68.653–71.088°C on Pi 4 and 47.4–54.55°C on Pi 5, with ondemand governors
and varying sampled frequencies. No installed binary, saved preference or
rendering default was changed.

Artifacts: `target/gpu-experiments/stroke-encoder.9OUnnWkL/` contains the old/new
executables, built/final source snapshots, build/test logs, native/device raw
samples, manifests and captures, runners, analyzers and text/JSON reports.
`summary.json` / `report.txt` cover the diagnostic and 1024 controls;
`followup-summary.json` / `followup-report.txt` cover the independent 800 check.
Remote evidence remains in `/tmp/stroke-encoder.fxEbivWb/` (Pi 4) and
`/tmp/stroke-encoder.rdqWcTgK/` (Pi 5). Executable SHA-256s:

- Old: `03d1ff3ad3254fca4bb8fedc319259701e3f20adc6faf927a3c39c4cbf7e37ee`.
- New: `fd1e7e127ffc7327e04b9b75274e8375173c8560f6644fe159766c801bc5214b`.

Next compare cached-layout, fresh-arc and refused work **inside the complete
encoder**, preserving ordinary output and draw order. Use switchable emission
controls and the established uninstrumented probe before adding more per-piece
timers. The current aggregate profile rules out neither code-layout nor CPU
cache interactions; its absence of warm misses or evictions gives no reason
to grow the topology cache for these histories. Keep 800 and 1024 controls,
new/old build checks, and eventual live CPU/pacing validation before any
default change.

## Selective stroke emission in the full renderer

The full-scene stroke probe can independently disable cached-layout output or
fresh-arc output, while preserving classification of the current polygon and
ordinary fallback. This tests whether either emitter earns back its preparation
cost in context, without moving outlines into an isolated replay or adding
per-piece clocks. It is a diagnostic control, not a new rendering default.

| Selection | Layout output | Fresh arc output | Other output |
| --- | --- | --- | --- |
| Ordinary `bulk-static` reference | Ordinary | Ordinary | Ordinary |
| `all` | Reused connectivity, fresh positions | Resume prepared corners | Ordinary on refusal |
| `layouts` | Reused connectivity, fresh positions | Ordinary after classification | Ordinary on refusal |
| `arcs` | Ordinary after classification | Resume prepared corners | Ordinary on refusal |
| `classify` | Ordinary after classification | Ordinary after classification | Ordinary on refusal |

Each selective run uses the existing matched `measure()` loop: ordinary
bulk-static versus the selected writer, with alternating order, new bounded
caches per block, a warmed 24-frame history, and separate CPU-encoding and
completed-GL measurements. Source generation, projection, simulation, text/UI,
readback and scanout remain outside timing. The classification-only path still
prepares the current corner geometry, including scratch positions needed to
certify a layout; it is not a cheap predictor or an offline acceptance oracle.

The switches act **before** layout lookup/compilation or fresh-arc emission.
They do not emit a mesh, discard it, and then run the ordinary writer. Skipped
layout/arc counters are separate from unsupported/merged/fine-arc/nonfinite/
budget refusals. Same-state checks verify that accepted plus deliberately
skipped counts match the all-enabled control. In these fresh-state runs, disabling
layout emission must leave its cache empty, with zero hits, misses and evictions.
Every mode retains the same draw order, vertex bits, indices, batches and retained-cache
decisions. No screen positions are cached between polygons or frames.

Reproduce a selective comparison with a GPU-enabled build and a fresh directory:

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-round-joins --geometry-stroke-bulk \
  --geometry-stroke-static --geometry-stroke-single-pass \
  --geometry-stroke-selection layouts \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 48 --presentation-repeats 4 \
  --geometry-output selective-layouts-1024
```

Use `all`, `arcs` or `classify` for the other controls and repeat at 800×480.
Omitting the selection keeps the existing all-enabled comparison. The option
requires the single-pass probe and cannot be combined with the nested encoder
profiler. The full probe manifest is version 8; its two new trailing CSV fields
count deliberately skipped layouts/arcs. Quality rows count selected **or
skipped** classifications so a no-emission control can still prove coverage.
The independent CPU oracle now covers thirteen writers under three round-join
policies, outside timing. The same selection is available in the broader
Clock/Pizza/mixed-shape probe.

The device protocol uses temporary release executables, not installed app
updates. The previous binary is an independent control for whole-build drift.
Pi 4 visits 800 then 1024; Pi 5 visits 1024 then 800. Within the first size,
the sequence is previous build, all, layouts, arcs, classify; the second size
reverses that order. Each selective run also contains its own alternating
ordinary reference. Boundary temperatures/frequencies are recorded without
changing governors. This is not a randomized or frequency-locked experiment;
small differences need repeatability before being treated as useful wins.

These controls measure **substitutions in a complete frame**, not additive
timings of internal stages. In particular, `classify` deliberately does ordinary
tessellation after preparation; the all-enabled path reuses that preparation on
accepted polygons. Its extra time cannot be labelled pure wasted production
classification time. Differences of each control's candidate-minus-reference
mean estimate the effect of enabling an emitter, but can still include CPU
cache, dispatch and code-layout interactions. They do not by themselves explain
the earlier isolated-replay versus complete-encoder discrepancy.

### Close up comparison

Mean encoding thread-CPU change against **each run's own paired bulk-static
reference**; negative is faster. All rows use the same moving Spacewars
close-up history, not Clock or a circle-only synthetic scene:

| Device | Viewport | Both emitters | Layouts only | Fresh arcs only | Classification then ordinary |
| --- | --- | ---: | ---: | ---: | ---: |
| Pi 4 | 800×480 | −5.09% | −0.40% | +31.15% | +34.85% |
| Pi 4 | 1024×768 | +1.64% | +8.86% | +6.08% | +10.84% |
| Pi 5 | 800×480 | −8.43% | −2.23% | +30.91% | +37.44% |
| Pi 5 | 1024×768 | +0.32% | +7.91% | +4.57% | +11.96% |

The all-enabled controls' absolute means are:

| Device | Viewport | Encoding CPU ms ordinary → all | Completed GL wall ms ordinary → all |
| --- | --- | ---: | ---: |
| Pi 4 | 800×480 | 7.039566 → 6.681038 | 13.817408 → 13.534752 (−2.05%) |
| Pi 4 | 1024×768 | 3.929205 → 3.993491 | 12.224900 → 12.277302 (+0.43%) |
| Pi 5 | 800×480 | 2.563497 → 2.347464 | 4.831962 → 4.562852 (−5.57%) |
| Pi 5 | 1024×768 | 1.315085 → 1.319327 | 4.039418 → 4.065874 (+0.65%) |

Both emitters contribute useful savings in context; neither selective mode
beats the combined path in these close-up histories. In the 800×480 close-up, removing
layout output is especially costly. At 1024×768 the combined path is still
slightly worse on Pi 4 and essentially tied on Pi 5, but disabling either
emitter makes encoding materially worse. Dropping fresh arcs is not the fix
for the resolution-dependent result.

Pi 5 illustrates the substitutions with relatively stable paired results.
At 1024×768, classification followed by ordinary tessellation adds 0.158 ms.
Enabling layout output recovers about 0.054–0.056 ms, depending on whether arcs
are enabled; enabling arc output recovers about 0.098–0.100 ms. At 800×480,
the corresponding recoveries are about 1.013–1.021 ms and 0.159–0.166 ms,
against a 0.964 ms classification-and-fallback increase. These are differences
of paired candidate-minus-reference means, **not measured internal stage
durations**. They are consistent with the earlier large change in layout-use
share, while leaving the precise integration-gap mechanism unresolved.

Every 800×480 all-enabled encoding and completed-render pair improves on both
devices. The Pi 4 encoding improvement varies from 0.24% to 7.52% between
pairs; its ordinary baseline also varies across separate controls. Preserve
those samples and limits instead of interpreting tiny differences as stable
wins. The production policy remains unchanged.

The warm all-enabled close-up still uses just two layouts / 610 bytes, with no
misses, evictions or budget refusals. Whole-frame polygon counts, including
ordinary objects, average 1,765.833 cached / 652.833 prepared / 222.708 merged
at 800×480 and 89 / 418.875 / 103.083 at 1024×768. These counts match between
devices and builds; selective controls move only the intended successes into
the skipped counters. Growing the cache would not address these workloads.

Early gameplay remains about 1.19–1.33% worse in encoding CPU with all-enabled
on Pi 4 and about 1% better on Pi 5. Late gameplay is roughly tied. The close-up
benefit is not a general renderer speedup. Relative to the previous binary,
all-enabled close-up CPU changes +0.77% / +1.08% on Pi 4 and +0.37% / +0.09%
on Pi 5, for 800 / 1024 respectively. Ordinary controls shift too. These small
whole-build changes are retained as evidence, not credited to an optimization:
this step adds diagnostic switches, not a faster production algorithm.

### Regression coverage

New tests cover emitter exclusion before cache/arc output, unchanged refusal
priority, exact colored geometry with existing buffer prefixes, counter
accounting, lazy/bounded state, reset on writer exit, and explicit CLI selection
constraints. The suites pass **722 client tests** (five existing ignored tests),
**207 tessellator tests**, and **seven tessellator doctests**. Native build,
Clippy, software-only compilation, formatting and the ARM release build pass
with existing warnings. The optional no-standard-library check still stops at
the previously documented unmodified `lyon_geom`/`euclid::Trig` feature issue;
it is not claimed as validated.

Desktop checks add **950 exact same-state GPU comparisons**: two full 439-state
histories plus 72 broader Clock idle/rain, Pizza 300/5000 and mixed-shape states.
Their timings are not device performance evidence. The separate desktop
800×480 cold/warm self-history comparison retains a one-pixel/channel-24
difference, identical in the reference and classification-only paths; every
same-state candidate/reference comparison remains exact.

On the Pis, **46,080 timing rows**, **23,040 matched work-counter checks** and
**23,040 cross-device count checks** pass. All **8,780 same-state GPU comparisons**
and **1,920 saved cross-control/build PNG comparisons** pass exactly. The 80
separate self-history rows retain the known Pi 5 1024 zoom-return one-pixel /
maximum-channel-23 difference, identical in every writer and both builds;
Pi 4's self-history rows are exact. Device close-up and zoom captures were
also visually inspected.

Both original Clock sessions resume unpaused and error-free. Installed
app/CLI/settings hashes, boot IDs, service PIDs and zero restart counts remain
unchanged. Boundary temperatures span 69.140–71.088°C on Pi 4 and 49.05–57.3°C
on Pi 5; both retain the ondemand governor. No installed app, saved preference,
renderer default or device service configuration changes.

Artifacts are under `target/gpu-experiments/stroke-selective.dBD8tnBq/`:
old/new binaries, source snapshots, build/test logs, native/device captures,
raw CSVs, runner, strict analyzer and `summary.json` / `report.txt`.
Remote evidence remains in `/tmp/stroke-selective.woXYKXLS/` (Pi 4) and
`/tmp/stroke-selective.j5FLLB6b/` (Pi 5). Executable SHA-256s:

- Previous: `fd1e7e127ffc7327e04b9b75274e8375173c8560f6644fe159766c801bc5214b`.
- Selective controls: `e3eee5129d00689cb0bb6430d4f37f55f9d8158386eaf74c96913301da39ac54`.

### Next experiment

Keep both emitters and compare one bounded reduction in shared preparation or
scratch-buffer traversal against the frozen all-enabled implementation. The
classification-only control makes that area worth investigating but does not
prove which individual operation is costly or dispensable. Preserve the exact
ordinary fallback and checked geometry, both viewport sizes, broader fixtures,
and independent old/new controls. Do not add cache capacity or choose an emitter
from viewport size alone. Live CPU and frame-pacing validation is still required
before promoting a policy.

## Same polygon prefix reuse

Retaining already-prepared base positions gives a **small within-build saving
on close-up Spacewars**, but does not establish a net improvement over the
previous executable. Keep this experiment opt-in and resolve the control-path
timing shift before promoting it or adding another optimization.

### Scope and controls

The original single-pass path prepares corners until it encounters an arc.
It then clears its position scratch buffer and emits all bases again, although
the earlier corners have already been certified to need no arc vertices.
`bulk-single-pass-prefix` preserves those positions and their vertex IDs. It
still emits edge/join triangles in the original order, resumes the first arc
corner and computes the remaining corners normally. The closing duplicate
vertices and floating-point divide/multiply order are unchanged.

This is reuse **within the current polygon**, not a screen-coordinate cache.
Every polygon starts with empty position scratch. Both layout and fresh-arc
emitters stay enabled, with the same 64-point input limit, 642-position /
3,072-index scratch limits, bounded layout cache and ordinary fallback.
Unexpected vertex emission inside a certified prefix fails closed before
anything reaches the caller's frame buffer. The original single-pass and
selective-emitter paths remain selectable.

The session-only writer also has a `bulk-single-pass-prefix-checked` variant
that compares each accepted mesh against ordinary Lyon. The full untimed
geometry oracle now compares 15 writers × reference/exact/checked policies,
including matching classification and cache decisions between the two
single-pass implementations. `topology_reused_prefix_vertices` records avoided
base-position regeneration; it is not a count of vertices removed from output.

Run the direct matched comparison with the existing offscreen probe:

```sh
engine-client --benchmark-presentation --presentation-geometry \
  --geometry-stroke-profile --geometry-round-joins --geometry-stroke-bulk \
  --geometry-stroke-static --geometry-stroke-single-pass \
  --geometry-stroke-selection prefix \
  --benchmark-width 800 --benchmark-height 480 \
  --presentation-frames 48 --presentation-repeats 4 \
  --geometry-output /tmp/fresh-stroke-prefix-captures
```

Unlike the other selections, `prefix` pairs **original single-pass versus
prefix reuse**, not bulk-static versus single-pass. Each Pi also runs `all`
in both the frozen previous executable and the new executable, preserving the
independent bulk-static controls. Manifest versions are 9 for the full probe
and 5 for broader fixtures. The standard timing loop remains unprofiled;
the CSV gains the prefix counter but no per-piece clocks.

Both devices run both sizes, three controls per size, with reversed control
order for the second size and opposite starting sizes across devices. Each
timing block gets fresh state, a warm history, 48 frames and four alternating
pairs. CPU encoding and completed GL rendering remain separate scopes;
simulation, source projection, text/UI, readback and scanout are excluded.

### Matched close up results

Means below compare the two paths **inside the new executable**. Negative
changes are improvements. These are uncapped timing measurements, not FPS.

| Device | Viewport | Original encoding CPU | Prefix encoding CPU | CPU change | Completed render change |
| --- | --- | ---: | ---: | ---: | ---: |
| Pi 4 | 800×480 | 6.767580 ms | 6.622765 ms | −2.14% | −0.85% |
| Pi 4 | 1024×768 | 4.036295 ms | 3.983363 ms | −1.31% | −0.40% |
| Pi 5 | 800×480 | 2.400037 ms | 2.340127 ms | −2.50% | −1.26% |
| Pi 5 | 1024×768 | 1.363775 ms | 1.348350 ms | −1.13% | −0.81% |

All four close-up pairs improve in each device/viewport combination, for both
timing scopes. Completed-render means are 13.941653→13.822945 ms and
12.276971→12.227897 ms on Pi 4, and 4.566085→4.508775 ms and
4.097968→4.064804 ms on Pi 5, respectively. Early/late gameplay encoding changes
range from −0.33% to +0.50%; there is no useful general gameplay CPU win here.

Both Pis report the same coverage: an average **3,048.46** reused positions
per close-up frame at 800×480, and **1,767.67** at 1024×768. Early/late histories
reuse only 84/90 positions per frame. Warm layout caches remain two entries /
610 bytes, without timed misses, evictions or budget refusals. The broader
native Clock idle/rain, Pizza 300/5,000 and mixed-shape histories reuse zero
prefix positions, so they are regression controls, not evidence of this
optimization accelerating those scenarios.

### Independent executable comparison

The new executable's **original** single-pass path shifts by +2.91% / +1.02%
encoding CPU on Pi 4 close-ups at 800/1024, and +2.41% / +2.91% on Pi 5. Bulk-static
controls also shift, between −0.82% and +1.26% across gameplay fixtures.
These are separate-run observations, not a diagnosis of the mechanism.

Comparing prefix encoding means to the frozen previous executable's original
single-pass means gives **+0.56% / +0.10%** on Pi 4 and **−0.07% / +1.80%**
on Pi 5. That does not support claiming a net production speedup. In particular,
do not add the within-build saving to the earlier selective-emitter results.

The experiment adds a prefix-count field to the returned mesh and shared
adapter statistics, plus another specialized emitter. Bookkeeping and code
layout are plausible contributors to the control shift, but these runs do not
separate them from each other or from device variation. The next experiment
should keep the same geometry change while isolating that diagnostic overhead
from the original hot path, with the frozen executable and both viewports
retained. Do this before another preparation optimization or live-policy
promotion; live CPU and pacing checks are still required afterward.

### Validation and device integrity

- 723 client tests pass, with five existing ignored tests; 209 vendored
  tessellator tests and seven doctests pass. New differential tests cover
  18,000 changing-geometry comparisons, first/later/closing arcs, folds, cold/warm
  caches, eviction, style/refusal priority and stale-prefix prevention.
  Frame-prefix/color/index preservation, mode switching, checked coverage and
  frame-budget failure are covered in the client adapter tests.
- Native build, Clippy, software-only check, ARM release build, formatting and
  whitespace checks pass with existing warnings. The native GPU checks include
  950 exact same-state comparisons across both sizes and the broader fixtures.
- All **27,648 Pi timing rows**, **5,268 exact same-state GPU comparisons**,
  **13,824 matched counter checks**, **13,824 cross-device count checks** and
  **960 saved cross-control/build PNG comparisons** pass. No geometry,
  classification, fallback, batching or cache-capacity differences appear.
- The 48 device self-history rows retain the known Pi 5 1024 zoom-return
  difference of one pixel / maximum channel error 23, identical across both
  paths and both builds. Pi 4 histories are exact. Native 800 cold/warm history
  retains its shared one-pixel / maximum 24 difference. These are not
  same-state candidate/reference failures. Native and both Pi captures were
  also visually inspected.
- Both original Clocks resume unpaused and error-free, with unchanged
  app/CLI/settings hashes, boot IDs, service PIDs and zero restart counts.
  Boundary temperatures span 69.627–71.088°C on Pi 4 and 49.05–56.2°C on Pi 5;
  both keep the ondemand governor. No installed binary, preference, service
  configuration or renderer default changes.

Artifacts are in `target/gpu-experiments/stroke-prefix.9zcqpeWP/`, including
frozen binaries/source, build/test logs, raw CSVs, captures, the safe device
runner, strict analyzer and `summary.json` / `report.txt`. Remote evidence
remains in `/tmp/stroke-prefix.byoO6m17/` (Pi 4) and
`/tmp/stroke-prefix.sB71WEv5/` (Pi 5). Executable SHA-256s:

- Previous: `e3eee5129d00689cb0bb6430d4f37f55f9d8158386eaf74c96913301da39ac54`.
- Prefix experiment: `7cd29a52e36d7bac1cdaa87da2ea8547e458f26c9cbd6513d5630e88a069ed80`.

## Checked only prefix accounting

Moving reuse accounting out of the timed writers preserves exact output but
does **not establish a dependable net prefix-reuse speedup**. Close-up encoding
CPU versus the pre-prefix executable ranges from 1.08% faster to 0.64% slower
across the two devices and viewport sizes. Keep the cleaner diagnostic design
and selectable experiment, without changing production defaults.

### Accounting and measurement scope

The prefix-reuse geometry is unchanged, but its detailed reuse counter now
belongs to the **checked writer only**. The ordinary mesh result again contains
only position/index slices and the output source. The uninstrumented original
and prefix entry points use a compile-time no-op observer; the counted entry
point returns its separate diagnostic to the checked adapter. Accumulation
occurs after frame-budget and exact-output validation, inside the existing
checked branch, rather than on every accepted polygon in every mode.

This removes the extra result payload and per-polygon accumulation introduced
by the first prefix experiment. It is not another geometry optimization:
`emit_prepared` and the remaining emission helpers are byte-for-byte unchanged
in the source. Classification, cache limits, ordinary fallback and renderer
defaults remain unchanged. The diagnostic field still exists in aggregate
statistics; code generation and layout can still differ between executables.

The full/broader manifests advance to versions **10 / 6** and explicitly mark
prefix accounting as checked-only. The existing timed CSV column
`topology_reused_prefix_vertices` is now zero for uninstrumented writers,
meaning **unmeasured**, not no reuse. Untimed `prefix_reuse_check` log records
give the per-state counts from the exact oracle. Cold validation traversals
and warmed cyclic timing can exercise different retained-template work;
compare coverage only when the classified work matches.

The paired device protocol retains three executables: the original
single-pass build, the first prefix build and this accounting cleanup. Each
device runs both 800×480 and 1024×768. Within each size, controls are original
bulk-static versus single-pass, frozen single-pass versus prefix, new
bulk-static versus single-pass, and new single-pass versus prefix. The second
size reverses this order; the two devices start at opposite sizes. Each run
uses the same 48-frame, four-pair timing loop with separate encoding CPU and
completed GL wall-time scopes. No nested profiler, governor change, installed
app update or persistent setting change is part of this experiment.

### Close up results

These are encoding thread-CPU means from separate executable runs, not isolated
instruction costs. The last column compares the cleaned prefix path with the
frozen **pre-prefix original single-pass** path. Negative changes are faster.

| Device | Viewport | First prefix build → cleaned prefix CPU | Cleanup change | Net versus pre-prefix original |
| --- | --- | ---: | ---: | ---: |
| Pi 4 | 800×480 | 6.593392 → 6.519315 ms | −1.12% | −1.08% |
| Pi 4 | 1024×768 | 3.990590 → 4.010741 ms | +0.50% | +0.64% |
| Pi 5 | 800×480 | 2.358322 → 2.318834 ms | −1.67% | −0.63% |
| Pi 5 | 1024×768 | 1.351419 → 1.330921 ms | −1.52% | +0.12% |

Inside the cleaned executable, prefix reuse still lowers close-up encoding CPU
by 1.96% / 0.42% on Pi 4 and 1.80% / 0.98% on Pi 5 at 800/1024, with all four
pairs improving in each combination. Completed-render changes are −1.08% /
+0.06% and −0.15% / −0.28%, respectively; the Pi 4 1024 and Pi 5 800 pairs have
mixed signs. Early/late gameplay CPU changes remain small, −0.24% to +0.50%,
without a useful general gain.

Independent controls still move: close-up bulk-static CPU shifts −1.07% to
+1.07% between builds, while the unchanged original single-pass path shifts
+0.39% to +1.56%. Removing the diagnostic payload/accumulation improves several
comparisons, but these runs do not establish that bookkeeping alone caused
the earlier shift. Code generation, layout and device variation remain possible
contributors. Do not accumulate these small percentages with earlier wins or
interpret completed-render differences as isolated GPU time.

Checked coverage matches the previous timed counts where classified work is
identical: about 3,048 / 1,768 reused positions per close-up frame at 800/1024,
and 84/90 in early/late gameplay. Both Pis agree. Warm layout caches remain two
entries / 610 bytes with no misses, evictions or budget refusals; there is no
case for increasing capacity. Broad native fixtures again reuse no prefix
positions.

The next useful step is a bounded **live whole-frame comparison** of the
existing bulk-static, original single-pass and prefix controls during moving
Spacewars and controlled zoom, including process/loop CPU and frame-pacing
tails. Keep the frozen executables and ordinary output available. The offscreen
results do not justify another default change or a further preparation
optimization before that validation.

### Validation and device integrity

Local validation passes 723 client tests, 209 tessellator tests and seven
doctests. The 18,000 changing-geometry cases now compare original, counted
prefix, uncounted prefix and ordinary output, including matching refusal and
layout-cache decisions. Checked accounting, mode switching and frame-budget
failure remain covered. Native validation passes 950 exact same-state GPU
checks, 390 saved-image comparisons against the previous executable and 2,192
reuse-count comparisons with matched classified work. An additional 800×480
Clock/Pizza/mixed sweep passes 72 exact same-state checks. Its differing viewport
is not compared against the previous 1024×768 broad captures. The shared native
800 cold/warm one-pixel history difference remains unchanged.

All **36,864 Pi timing rows**, **7,024 exact same-state GPU comparisons**,
**18,432 matched-counter checks**, **18,432 cross-device count comparisons**
and **1,440 saved cross-control/build images** pass. The 64 self-history rows
retain only the known Pi 5 1024 zoom-return one-pixel / maximum-23 difference,
identical across controls and builds; Pi 4 histories are exact. Representative
native and Pi captures were also visually checked.

Both original Clocks resume unpaused and error-free, with unchanged installed
app/CLI/settings hashes, boot IDs, service PIDs and zero restart counts.
Boundary temperatures span 68.653–70.114°C on Pi 4 and 50.15–57.3°C on Pi 5,
with the existing ondemand governors. Formatting, whitespace, Clippy,
software-only checks and ARM release build pass with existing warnings.

Artifacts are in `target/gpu-experiments/stroke-accounting.vKto7atG/`: the three
frozen release executables, built/final source snapshots, raw results, captures,
safe device runner, strict validators and `summary.json` / `report.txt`.
The final Rust source differs from the built snapshot only by a comment clarifying
cumulative versus per-frame accounting; `verify-source.mjs` checks that exact
difference. Device artifacts remain in `/tmp/stroke-accounting.CefEiVUO/`
(Pi 4) and `/tmp/stroke-accounting.1QU0h6nL/` (Pi 5). The cleaned ARM executable
SHA-256 is `bea79fc0158247bb572a09875d0e7d7dcb238aa0dd7a9df1c3a90388a7df6f1c`;
the two frozen controls retain the hashes from the preceding experiment.

## Live stroke writer comparison

The single-pass writers provide a **small live CPU saving on Pi 5, not a
general Pi 4 improvement**. Prefix reuse reduces whole-process CPU per draw by
about 2.5% versus bulk-static on the HyperPixel, but stays within 0.32% of that
reference on the Picade. It does not resolve the Picade's zoom-related long
frames. Keep the controls available and defaults unchanged; investigate those
long frames before extending the stroke preparation optimization.

### Live protocol and controls

Both devices run the same frozen accounting-cleanup ARM executable. The CLI
now also accepts the existing layout, arc, classification, prefix and checked
prefix controls, with parser coverage. This changes access to the experiments,
not their rendering code. The rebuilt client has the same executable hash.

Each device runs **18 fresh Spacewars sessions**: three writers, normal or
sweeping cameras, and three rotating orders so every writer occupies each
position. Seed 0, an idle human P1 and the moving `material_mission_v9` P2 keep
the fixture repeatable; asteroids, sound and the FPS overlay are disabled.
The Pi 4 uses its native 1024×768 viewport; the rotated Pi 5 HyperPixel uses
800×480. These are within-device comparisons, not a hardware comparison at
matched resolution or a full-match capacity test.

Every run uses `retained-culled`, bucketed streams, prepared plans, pooled
sources, conservative stroke rejection, exact joins and CPU-shaped text.
After 480 warmup ticks, `/proc/PID/stat` measures all application threads until
about tick 1920, normalized by fresh KMS draw-counter differences. These
untraced intervals total **435.78 seconds / 25,479 draws on Pi 4** and
**439.86 seconds / 26,413 draws on Pi 5**. Host diagnostics update about once
per second: starts span ticks 481–539 on Pi 4 and 486–487 on Pi 5, with ends
1937–1944 and 1948–1950. Three repeats and approximate tick boundaries do not
justify precise claims from sub-percent differences.

Twelve rolling status samples per run provide secondary stage means. Their
120-frame windows can overlap; they are not independent pooled frame counts.
Two separate 240-loop `render-trace start` captures follow the untraced interval,
near ticks 2040 and 2400. This basic trace includes per-GL-call clocks, so its
pacing/tail measurements remain separate from the process-CPU comparison.
Nested text, application and geometry detail are disabled. Checked writers
run only afterward, on paused frames. No measurement is isolated GPU execution
time, and percentages here must not be added to earlier offscreen/build results.

### Whole process CPU results

Values are CPU milliseconds per KMS draw, computed from summed CPU time and
draw counts across the three untraced runs.
Negative relative changes mean less CPU. All columns use **one executable**;
the original single-pass column is not the earlier frozen pre-prefix build.

| Device and camera | Bulk-static | Single-pass | Prefix | Prefix versus bulk-static | Prefix versus single-pass |
| --- | ---: | ---: | ---: | ---: | ---: |
| Pi 4 normal | 12.322 ms | 12.291 ms | 12.305 ms | −0.14% | +0.11% |
| Pi 4 zoom sweep | 14.067 ms | 14.118 ms | 14.112 ms | +0.32% | −0.04% |
| Pi 5 normal | 5.555 ms | 5.455 ms | 5.414 ms | −2.54% | −0.75% |
| Pi 5 zoom sweep | 6.705 ms | 6.601 ms | 6.535 ms | −2.55% | −1.00% |

All three paired Pi 5 comparisons improve against bulk-static: normal
−1.88/−2.27/−3.47%, sweep −2.76/−2.44/−2.44%. The extra prefix step over
single-pass is smaller: effectively tied in one normal pair, and 0.93–1.14%
less CPU in the three sweep pairs. Pi 4 prefix-versus-static pair signs are
mixed. Its rolling loop/draw means also fail to establish a consistent saving.

The untraced completed-flip rate remains **59.46–59.58/s normally and
57.37–57.46/s while zooming on Pi 4**, versus **60.04–60.06/s on Pi 5**.
Pi 5 therefore demonstrates a CPU saving without a meaningful FPS change.

### Remaining long frames

All three Pi 4 sweep controls retain a **33.32–33.33 ms p95 completed-flip
interval**, with occasional approximately 50 ms gaps. The separate traces
contain 101/1395 delayed intervals for bulk-static, 89/1412 for single-pass and
98/1415 for prefix. Those short slices do not establish a dependable pacing
gain. Pi 5 has no skipped vblank sequence in its sampled intervals; this is
not a guarantee for every frame or a longer, denser match.

The Pi 4 traces expose two different expensive paths. Host/timer CPU peaks
around **35–37 ms** across controls, including normal-camera runs. In other
sweep frames the draw itself is expensive: encoding wall-time reaches about
**21–31 ms**, inside a **27–36 ms draw-CPU** peak. Nested wall and CPU spans
must not be added, and the timer bucket alone does not identify simulation,
bot, scene-construction or other callback work. A next bounded investigation
should use the existing `start-work` trace to split those host stalls and
`start-geometry` to attribute the expensive encoding frames. Keep those
diagnostic clocks outside the comparison timings.

### Validation and device state

All **432 status windows, 72 complete traces / 17,280 loops, 48 exact paused
image comparisons and 36 launcher-exit/reset checks** pass. The image checks
cover normal and zoomed split screens on both devices across all three writers
and checked prefix output; representative captures were visually inspected.
Requested/applied policies agree, cache/scratch budgets hold, and there are no
renderer errors, stream fallbacks, trace-time shader compilations or DRM flip
read errors. The CLI's 32 tests, strict Clippy check, formatting, harness shell
checks and ARM release build pass.

Both installed Clocks resume unpaused, with their prior scene modes, unchanged
binary/settings hashes and unchanged boot IDs. Service processes are deliberately
restarted once around the private test sessions; restart counters are zero.
Existing ondemand governors are unchanged. Boundary temperatures span
67.192–71.575°C on Pi 4 and 47.4–52.35°C on Pi 5. No persistent settings or
installed executables are updated.

Artifacts are in `target/gpu-experiments/stroke-live.80nZyhj1/`: `protocol.json`,
frozen client/CLI, the CLI source/build manifest, runner, strict analyzer,
raw device captures, `pi4-summary.json`, `pi5-summary.json` and `report.txt`.
Remote copies remain in `/tmp/stroke-live.Rl71agSu/` (Pi 4) and
`/tmp/stroke-live.vwwaNrYp/` (Pi 5). The renderer executable retains SHA-256
`bea79fc0158247bb572a09875d0e7d7dcb238aa0dd7a9df1c3a90388a7df6f1c`;
the CLI is `54cdd67b517b837dccfb0093951dc2c97c19d9de22cb2bcba4e70ef418c7c4d6`.

## Remaining device checks

1. Upload format, conversion alignment and texture reuse are now measured on
   Pi 4; aligned RGBA and direct triangles are also compared on Pi 5. Preserve
   the aligned RGBA reference and software fallback; persistent
   texture reuse is not justified as a production default by these results.
2. Rounded borders and first-capture freshness now pass on Pi 4 and Pi 5, including
   minimal fixtures and repeated CLI-driven menu transitions. Test physical
   controller input as well, and repeat the UI checks on other GPU/driver
   combinations. Desktop GPU capture is outside the new KMS guarantee.
3. Triangle batching passes the measured Clock idle/rain performance and
   water-seam checks. Multiple viewports and grouped minimaps are implemented;
   continue full-match and remaining Clock-effect validation, including Slint
   text/UI and session transitions. Geometry reuse is now an opt-in live mode;
   the bounded hybrid preserves exact-cache steady-view performance and reduces
   changing-zoom cost. The refined outline guard cuts that zoom cost further
   without growing the cache. First-use spikes and about 939 genuinely guarded
   outlines per zoom frame remain. Paced admission and edit/visibility churn
   are now measured: smaller initial spikes do not outweigh longer warmup
   universally. Exact repeated-polygon construction reuse now cuts initial
   replay frame time about 6% and terrain-stress maxima 8–13% on Pi, without
   longer warmup. Warm zoom is about 21.0 ms with bulk validity scanning and
   contiguous expansion, both small additional wins. The sampled guarded
   outlines do not repeat within a view; a frame-local dictionary would not
   help. Guarded tessellation and vertex expansion/upload remain substantial;
   reducing loop overhead has not reduced the amount of geometry moved.
   GPU-resident references now reduce zoom uploads 29% and encoding time 11%,
   but complete Pi rendering regresses 21.175 → 25.053 ms. Separate per-pass
   data textures remove much of an early-replay submission stall, not the
   remaining regression. Full storage buffers now improve workstation replay
   rendering 8–12%, but the Pi driver exposes zero vertex-stage storage blocks.
   Its supported uniform-transform variant does not recover the regression:
   21.639 ms with CPU expansion versus 25.614 ms with uniform transforms.
   Keep CPU-expanded `retained-ranges` as the Pi control and investigate
   remaining geometry work or cheaper draw representations, not a presumed
   universal resident-buffer fast path.
   The new cached-visibility control removes 35% of zoom uploads, but its
   completed Pi zoom gain is only 3.2%, with worse p95 and cold-use means.
   Later gameplay improves 10.4%; early gameplay and edits are effectively
   unchanged, while terrain reveal regresses about 2%. The counterbalanced
   capacity follow-up recovers another 3.6–5.0% in the culled replays by
   changing buffer storage/upload policy, with exact same-policy pixels and
   geometry. An exact-size split-upload control now shows that the API split
   alone is insufficient: bucketed stores retain a 3.8–4.4% improvement while
   both exact-size controls stay within 0.9%. The queued-history follow-up
   preserves a 10–18% bucketed improvement in culled Pi replays at depth three,
   with terrain histories effectively unchanged. It uses more reserved memory
   and overlap increases observed completion age. The live policy comparison
   now confirms a 15.1% draw-CPU reduction and 7.2% higher sampled Spacewars FPS
   at normal pacing, with exact paused captures and successful session/pool
   transitions. Clock average CPU is unchanged. The ten-minute follow-up keeps
   renderer resources bounded across two rematches but exposes a first-game-over
   UI hitch and recurring simulation spikes. Controlled live zoom also improves
   FPS 45.86 → 51.22 and draw CPU 13.370 → 10.820 ms, with exact paused captures;
   it still misses 60 rendered FPS. The aligned first/repeat result-screen
   trace finds 2,653 versus 125 GL draw calls and 109.302 versus 16.267 ms draw
   CPU, strongly implicating cold glyph-atlas generation. The isolated text
   fixture now directly counts 3,560 atlas draws versus zero when cached;
   fresh-atlas population costs about 92 ms CPU on the Pi after process warmup.
   Row preparation doubles total CPU and still exceeds a frame budget. CPU
   masks with batched uploads now reduce that isolated fresh-atlas CPU cost
   from 91.934 to 3.275 ms, with small measured antialiasing differences and
   unchanged placement. The switchable live integration now reduces cold
   launcher, settings and result draws by roughly 4.5–6 times, with bounded
   residency and clean repeated-screen/full-match checks. Per-run font-face
   reuse now reduces isolated Slint CPU from 13.607 to 5.507 ms with identical
   pixels and uploads. Live cold result draws fall from 26.836 to 18.508 ms;
   the natural transition still costs 26.904 ms draw CPU, plus timer work.
   Coarse tracing now separates these costs: a natural result transition spends
   8.760 ms in shaping-run misses, absent from warmed atlas-reset tests, plus
   about 4.6 ms building/uploading masks. The fresh-context follow-up now cuts
   cold shaping CPU 6.796 → 3.606 ms by reusing at most eight borrowed font
   faces per shaping run. Its paired natural result draw improves
   28.895 → 26.068 ms, with identical pixels, while warmed-atlas costs remain
   essentially unchanged. Frame-aligned application spans now attribute
   14.741 of 21.195 ms steady result-loop CPU to scene generation, staging and
   the direct drawing callback, while scoreboard publication itself is cheap.
   Finer spans now identify about 4 ms of projection and 2.4 ms of temporary
   data destruction per frozen result frame. A same-process allocation control
   reduces projection 4.135 → 3.516 ms and basic whole-loop CPU
   20.427 → 19.744 ms, with identical pixels and GL work. Cleanup remains
   about 2.5 ms. Bounded scratch storage now halves plan preparation/disposal
   in matched moving replays, improving completed geometry wall time about
   10%, and cuts basic frozen whole-loop CPU 20.819 → 18.137 ms (12.9%).
   Pixels and GL work stay identical; the full-match storage peak is 1.74 MiB
   under an 8 MiB cap. The bounded source-point pool now removes 44–47% of
   source CPU in moving Spacewars fixtures and cuts same-build basic frozen
   whole-loop CPU 19.375 → 17.108 ms (11.7%), with identical pixels/GL work.
   Its live capacity peaks at 476 KiB under a 2 MiB cap. Keep `reference` as
   the default for both allocation controls. Prepared per-camera/object
   transforms cut another 11–12% of isolated plan CPU, but only 1.4–1.5% of
   completed geometry wall time and 1.4% of the sampled full-app loop CPU.
   Pixels and memory bounds remain unchanged. The four-vertex CPU expansion
   experiment is rejected: it increases Pi encoding CPU 3–19%, despite an
   initially favorable microbenchmark with different compiler settings.
   Direct drawing is still the larger CPU stage. Guarded zoom stroke
   preparation costs 0.174 ms versus 2.177 ms in tessellation/output in the
   detailed follow-up; 41% of those strokes generate wholly offscreen geometry.
   The bulk output writer's same-build moving comparison now reduces zoom
   encoding CPU 2.47% and completed drawing wall time 0.83%, with exact output
   and all eight block pairs improving. Ordinary gameplay is effectively
   unchanged. Pi 4 live writer validation now also passes, but concrete output
   does not reproduce the Pi 5 live gain; both devices' broader exact-output
   checks pass. Conservative early
   rejection now skips 40.6% of guarded zoom strokes without relaxing the guard,
   reducing encoding CPU 17.7%, uploads 9.0%, and completed drawing wall time
   7.9%. All eight paired blocks improve, with exact pixels and independent
   checks of discarded geometry. Ordinary gameplay is unchanged. The live
   opt-in comparison now improves zoom drawing CPU 6.0%, loop CPU 4.0%, and
   sampled FPS 55.69 → 56.80. Two full matches, moving checked zoom, exact
   paused captures, and session transitions pass with bounded resources. Keep
   rejection off by default until broader scene/device coverage supports a
   change; unknown bounds still use ordinary tessellation.
   Separately consider
   unchanged-world reuse for paused/results without
   claiming an active-game benefit. Retain genuinely new-text and warmed-atlas
   controls. Remaining
   UI item preparation is not layout alone. Keep
   per-frame geometry and simulation costs separate from uploads. Avoid
   unnecessary visibility checks when geometry is already on-screen.
   Keep these policies experimental and preserve the original comparison path.
4. Pi 5 HyperPixel: the separate RP1 DPI / V3D path starts successfully. The
   rotated canvas/notifier mismatch and screenshot orientation are now fixed;
   all 66 pixel cases and real scene/menu transitions pass. Verify all four
   touch corners physically and continue full-match/effect coverage. Captured
   output and inverse-coordinate unit tests do not establish physical touch
   alignment.
   The concrete-output stroke follow-up now saves about 3.1% live process CPU
   versus dynamic bulk, with exact images and unchanged resource limits.
   Preserve the RGBA comparison: direct zoom still costs about 1.36 ms/frame
   more KMS-loop CPU in that run. Broader Clock/Pizza/mixed-shape and Pi 4
   validation now pass without establishing a universal performance benefit.
   Pi 4 direct zoom has lower average CPU/frame but worse kernel page-flip
   pacing than RGBA. Frame-aligned traces associate nearly all skipped
   refreshes with frames whose CPU work exceeds the refresh budget. Target
   worst-case scene encoding before changing pacing or defaults; the frozen
   fixtures do not replace live effect/full-match checks.
5. Decide on a default or the next GPU adapter change from those results. Keep
   software available regardless; do not add unrelated CPU shape fast paths.
