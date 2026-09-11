# Software presentation performance lab

This lab separates software-renderer bookkeeping from pixel drawing. It builds
on the [Clock measurements](clock-performance-lab.md), but also uses Falling's
native-video image path. No graphics algorithm or repaint policy is changed by
the diagnostic hooks.

## Frozen-image comparison

```sh
cargo run --release --locked -p engine-client -- \
  --benchmark-presentation --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 60 --presentation-repeats 4 --presentation-detail
```

The same command works on a deployed Pi using `engine-client` directly. It does
not open a display connection, read/write settings, start input/audio workers,
or bind the application's control socket. Leave the kiosk at its idle launcher
while measuring on the Pi to avoid competing with an active scenario.

Four fixed fixtures are drawn into ordinary XRGB8888 RAM:

- Background only.
- A 1024×768 Clock image fixed at 08:08:00, events Off.
- The same Clock at 2048×1536, normally reduced to the output size.
- Falling's 256×240 frame after 180 emulated frames, cropped to 256×224 and
  enlarged using the native-video path's contain/pixelated settings.

Fixture generation, including NES emulation, is completed before timed draws.
The images remain frozen. One window contains only the image; the other is the
real `MainWindow` type with its default UI models and menus inactive. In the
baseline those menus remained in the tree; the conditional-panel optimization
below removes them while inactive.
It uses Falling's scenario label to suppress Clock's visible Controls button,
so **both windows must produce exactly the same output pixels**. This tests the
application shell, not a fully initialized live game's complete UI state.

Both windows use Slint's generic software texture path, not the LinuxKMS RGB
specialization. They force full repainting (`NewBuffer`), with no display pacing,
DRM mappings, or page-flip waits. These are draw timings, **not application FPS**.
Each block warms up with ten draws, then measures the requested frame count.
The first window alternates between paired repeats. Completed buffers must be
byte-for-byte equal; CSV rows include their checksums and dimensions.

Experiments:

```sh
# Quarter the number of output pixels; keep source images unchanged.
engine-client --benchmark-presentation --benchmark-width 512 --benchmark-height 384 \
  --presentation-frames 60 --presentation-repeats 4 --presentation-detail

# Publish a newly allocated but identical RGB image before each draw.
engine-client --benchmark-presentation --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 60 --presentation-repeats 4 --presentation-detail \
  --presentation-republish

# Omit --presentation-detail to check the result without operation timestamps.
engine-client --benchmark-presentation --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 60 --presentation-repeats 4
```

`--presentation-republish` excludes image allocation/copy/property assignment
from the draw timer; their cache effects and deferred work can still affect the
subsequent draw. It is not a measurement of total publication cost. With detail
disabled, detailed CSV columns are zero/unmeasured, not evidence of free work.
The viewport is bounded to 64–2048 pixels per axis, frames to 1–10000, and
repeats to 1–20. Default output size is the shared benchmark default, 1280×720.

## Live draw attribution

LinuxKMS profiler version 5 adds `kms_draw_detail` and `kms_core_*` timings and
call counts to `spacewars-cli status`. Detailed observation follows the existing
profiler by default; set `SPACEWARS_KMS_DRAW_DETAIL=0` before application startup
to disable these nested hooks, or `SPACEWARS_KMS_PROFILE=0` to disable all timing.
Other display backends do not install this observer. The core hooks themselves
are clock-free and optional; the backend owns CPU clocks and bounded storage.

| Stage suffix | Scope |
| --- | --- |
| `render` | Complete Slint render call, inside the existing `draw` stage |
| `prepare` | Buffer/window setup before dirty-region calculation |
| `dirty` | Dirty-region calculation, including item bounds/cache processing |
| `background` | Complete window-background fill |
| `items` | Traversal and rendering of visible items, including operations below |
| `image` | Image preparation and drawing, including texture work |
| `texture` | Target texture operation, either accelerated or fallback |
| `fallback` | Generic texture sampling/drawing; also includes internal glyph textures |
| `rectangle` | Rectangle operations, including borders and gradients |
| `text` | Text preparation and glyph drawing |
| `path` | Path calls, even if the renderer does not support painting them |

These stages are nested, not additive. For example, the existing RGB blit lies
inside `core_texture`, which can lie inside `core_image` or `core_text`, then
`core_items`. Use totals from the same completed-loop window, divided by actual
draw count when reporting per-frame costs. Repeated operation calls are summed
within each iteration. Recursive spans of the same kind are timed as their
outermost span, avoiding double-counting that kind.

The item operation counts indicate rendering calls, not dirty-tree visits or
necessarily visible pixels. In particular, **zero text draw calls does not rule
out text measurement while computing item bounds in `core_dirty`**. The dirty
stage is not itself a measurement of font shaping or of one particular widget.

## Baseline Picade results, 2026-09-10

The release diagnostic build was fast-deployed to Picade's Pi 4, with its
1024×768 XRGB8888 display, direct buffer mode and the existing RGB specialization
enabled. No rendering algorithm or repaint-policy change was made for this
experiment. The application SHA-256 is
`63cd9037484f64abe53b05d0bec3777d9e49948d16a47c09e4d76240d4f584a8`.

### Live Clock and Falling

Each scenario warmed up for five seconds, then collected six status samples
three seconds apart. Values pool stage CPU totals from the completed-loop
windows and divide by the actual draw count. Clock used Off, 24-hour time,
raster 2× and 103 primitives; Falling was at its title screen, with sound muted
but emulation and audio workers still running.

| CPU time per draw | Clock, raster 2× | Falling title screen |
| --- | ---: | ---: |
| Complete Slint draw | 14.85 ms | 13.26 ms |
| Dirty-region calculation | **8.97 ms** | **6.90 ms** |
| Window background | 1.82 ms | 1.95 ms |
| Image preparation and drawing | 3.26 ms | 4.26 ms |
| Visible text drawing | 0.47 ms | 0 calls |

These rows are selected parts of the draw, not an exhaustive additive breakdown.
Clock drew one image, one rectangle and one text item (the visible Clock
Controls button). Falling drew one image, no rectangles and no text. Clock's
optimized blit accounted for 3.24 ms inside image drawing; Falling used the
generic texture sampler. Dirty-region work accounts for about 60% and 52% of
their respective draw CPU costs.

Clock still used about 99% of one core, with a 20.29 ms mean frame interval.
Falling maintained about 60 page-flip completions/s and 60 emulated frames/s;
its process used about 156% of one core, split chiefly between the UI thread
(86%) and NES runner (68%). These are diagnostic-build observations, not gains
from a new optimization. Temperature samples were 70–75°C; frequency samples
were 1.4–1.5 GHz. Sampling does not hold the governor fixed.

### Frozen pixels, bare window versus application shell

At 1024×768, four paired repeats of 60 measured draws per window produced the
following means. These are **wall times in ordinary RAM using the generic
sampler**, not the live KMS CPU timings above.

| Frozen fixture | Bare window | Full UI | Full UI dirty-region work |
| --- | ---: | ---: | ---: |
| Background only | 0.95 ms | 7.74 ms | 6.70 ms |
| Clock, 1024×768 source | 5.50 ms | 12.56 ms | 6.95 ms |
| Clock, 2048×1536 source | 5.81 ms | 12.88 ms | 6.90 ms |
| Falling, 256×240 source | 4.68 ms | 11.77 ms | 6.88 ms |

Every bare/full pair produced identical pixels. Neither window painted text or
rectangles. The full UI adds roughly 6.7–7.1 ms even for an empty background;
the main difference is dirty-region calculation, not additional visible drawing.

The controls help narrow the cause:

- Reducing output to 512×384 quarters the pixel count. Falling drops to
  1.07 ms bare / 8.17 ms full, but full-UI dirty work remains 6.91 ms. The large
  Clock source takes 1.65 / 8.71 ms, with 6.91 ms in dirty work.
- Republishing identical images leaves the large Clock at 5.97 / 13.09 ms and
  Falling at 4.78 / 11.98 ms. This does not explain the multi-millisecond UI gap;
  remember that publication itself is outside the timer.
- Disabling operation timestamps leaves the large Clock at 5.80 / 12.86 ms and
  Falling at 4.68 / 11.78 ms. The gap is not an artifact of those timestamps.
- The desktop reproduces approximately 1.0 ms of extra full-UI dirty work at
  both output sizes. At 1024×768, Falling takes 0.58 / 1.62 ms. Its release
  SHA-256 is
  `05b61e305e1693c224b825b283d633a415914ef99ba22dfd65e3aeb23f7ff448`.

These are small diagnostic runs, not controlled thermal trials or confidence
intervals. Bare/full order alternates within a run; the different experiments
run sequentially. The kiosk remained at the idle launcher during Pi probes.
Before/after snapshots ranged from 65–75°C and 0.6–1.5 GHz, and do not establish
the frequency during every draw. Treat small cross-run differences cautiously.

### What to investigate next

The evidence points to UI-tree bookkeeping, including inactive controls.
In the vendored core, `PartialRenderer::compute_dirty_regions()` visits the
item tree and computes bounds for uncached items. That path does not prune
invisible subtrees. The uncached branch does not populate the rendering cache;
items omitted from actual drawing can therefore repeat this work. Text bounds
call `text_size()`. Many application menus use `visible` rather than conditional
construction. See [the dirty-region traversal](../vendor/i-slint-core/item_rendering.rs)
and [the application UI](../crates/engine-client/ui/main.slint).

This is a source-backed explanation for the measured bucket, **not a measurement
of which hidden widgets or text operations dominate it**. The probe intentionally
uses default models and Falling's label, so it does not explain every difference
between the 6.9 ms probe and 9.0 ms live Clock bucket.

This motivated an experiment to remove inactive menu subtrees from the hot path and
repeat the same pixel and timing comparisons. A narrower full-repaint fast path
that avoids unnecessary dirty analysis is another candidate, but must preserve
cache invalidation, visibility transitions, old-pixel damage, resizing, and
buffer-age behavior. Neither optimization was part of the diagnostic baseline.
There is no reason yet to change Clock's simulation or reduce its visual
quality to address this particular cost.

Raw CSV/JSON reports, collectors, summaries, build/deploy/test logs, binary
hashes and restored-device state are retained in the collecting checkout under
`target/clock-benchmarks/draw-detail-picade-20260910/`. After that collection,
Clock Demo, raster 2×, all six effects, 24-hour time, Clock wave,
`SPACE WARS`, and unmuted 5% volume were restored; the service did not crash or
reboot during collection. Pi 5 was not changed.

## Conditional-panel experiment, 2026-09-10

The application UI now constructs launcher, pause, game-over, touch-test,
notification and scenario-specific HUD panels only while they are needed.
Launcher pages and the pause help panel are conditional too. Their settings,
selection indices and callbacks remain on `MainWindow`; its keyboard focus
scope remains alive even when the launcher is absent. No vendor rendering,
repaint-policy, resolution, frame-rate target, or simulation changes were made
for this optimization.

The Pi release SHA-256 is
`8d9e714b108298e088991e71702a2dd287bddebe0d2813e0c44975f478bef98d`;
the desktop release SHA-256 is
`d97eccf57ee391e15e086064ed5319512876d657371d599165fbf8f86ab3e65a`.
Both use the same diagnostic hooks as the baseline above.

### Frozen-pixel comparison

The Pi ran the saved baseline and new binary in old/new/old/new order, with the
kiosk idle at its launcher. Each process ran four paired blocks of 60 measured
draws per window at 1024×768, after ten warm-up draws per block. The table pools
the two runs of each binary. Every paired image and every before/after fixture
checksum matches. These remain generic-sampler RAM **draw wall times**, not FPS.

| Frozen fixture, full UI | Before | Conditional panels |
| --- | ---: | ---: |
| Background only | 7.62 ms | 0.96 ms |
| Clock, 1024×768 source | 12.68 ms | 5.51 ms |
| Clock, 2048×1536 source | 13.04 ms | 5.91 ms |
| Falling, 256×240 source | 12.01 ms | 4.73 ms |

Full-UI dirty calculation fell from approximately 6.6–7.1 ms to 0.01–0.02 ms.
The new full-UI draw times closely match the bare window. The second baseline
run still exhibits the original cost, ruling out a one-time warm-up explanation
for its disappearance. Small pixel-drawing variations remain between runs.
On the desktop, the large Clock fixture falls from 1.75 to 0.69 ms and Falling
from 1.65 to 0.59 ms; full-UI dirty work falls from about 1.05 ms to 0.001 ms.

### Live Picade comparison

Clock was sampled immediately before and after the deployment using Off,
24-hour time, raster 2×, 103 primitives, direct 1024×768 output and the RGB
specialization. Both collections use six samples three seconds apart after
five seconds of warm-up. Falling uses the earlier version-5 title-screen
baseline above and a new collection with the same settings, muted sound, and
sampling procedure. Stage CPU times pool totals and divide by draw count.

| Live measurement | Before | Conditional panels |
| --- | ---: | ---: |
| Clock draw CPU per frame | 14.82 ms | 7.06 ms |
| Clock dirty-region CPU per frame | 8.87 ms | 0.06 ms |
| Clock displayed FPS | about 49 | 60 |
| Clock process CPU, one core = 100% | 99% | 77% |
| Falling draw CPU per frame | 13.26 ms | 6.76 ms |
| Falling dirty-region CPU per frame | 6.90 ms | 0.04 ms |
| Falling page-flip completions/s | about 60 | about 60 |
| Falling process CPU, one core = 100% | 156% | 122% |

The change removes the dominant UI-tree cost without changing the visible
scene. Clock now reaches its 60 Hz target and spends time waiting for page
flips. This is not a claim that Clock rendering is free: its host callback still
takes about 5.25 ms, and the background and optimized image blit take about
2.09 ms and 3.99 ms respectively. Falling still has a separate NES runner;
its UI thread falls from about 86% to 47% of a core, while emulation remains
roughly 68–74% across these samples.

These are not fixed-frequency trials. Clock's before samples were 75–77°C at
1.5 GHz; after samples were 71–73°C at 1.2–1.5 GHz. Different frame pacing,
memory/cache behavior and governor decisions can affect the other buckets.
The controlled, frozen-pixel comparisons support the large bookkeeping gain;
the live numbers describe the resulting application behavior on this device.

### Lifecycle verification and artifacts

A new headless test opens and removes 17 UI states at landscape and portrait
sizes, preserving the old framebuffer across transitions and resize. Each
partial repaint must match a complete repaint, and root settings and selection
state must survive panel removal. Its 34 PNGs were also byte-identical before
and after this change on the same desktop. To retain them locally:

```sh
SPACEWARS_MENU_TEST_ARTIFACTS=/tmp/spacewars-menu-images \
  cargo test --locked -p engine-client --bin engine-client ui_render_tests
```

A separate real Slint input test repeatedly removes a pressed launcher button,
renders the changed tree, releases the pointer, and reopens the launcher. It
checks that the removed button is not activated and that touch and keyboard
shortcuts still work after reconstruction. Existing keyboard/touch tests pass.
Picade's public UI API was also exercised through pause/help/Clock/sound,
launcher settings/help/touch-test, and Clock → Falling → Clock. A follow-up
after these menu cycles still measured about 7.08 ms Clock draw CPU at 60 FPS.

Initial validation passed 934 workspace tests (20 display-dependent tests
skipped), 19 backend tests (one explicit benchmark skipped), formatting and
whitespace checks. The new tests run headlessly. The final checkpoint below
also ran the display-dependent tests under an isolated Xvfb server.

Reports, collectors, summaries, menu PNG pairs, build/deploy/test logs and device
state are retained under
`target/clock-benchmarks/menu-pruning-picade-20260910/`. The optimized release
remains deployed to Picade; Clock Demo and the original display/effect/audio
settings were restored. No reboot, OS update, or Pi 5 deployment was performed.

## Validation

```sh
cargo test --locked --workspace --all-targets
xvfb-run -a -s "-screen 0 1280x1024x24" \
  cargo test --locked -p engine-client --test ui_control_functional -- \
  --ignored --test-threads=1
cargo test --locked -p engine-client --test presentation_probe
cargo test --locked --manifest-path vendor/i-slint-backend-linuxkms/Cargo.toml \
  --features renderer-software --lib
```

Black-box tests run with no display and an invalid backend name, preserve an
intentionally malformed settings file, assert that no extra files/socket are
created, compare both windows and publication modes, check CSV columns/call
counts, and reject invalid limits. They have no machine-speed thresholds.
Backend tests verify balanced nested accounting and distinguish generic
fallbacks from the RGB fast path without changing pixels. The standalone backend
tests now use the same locally patched core as the application so the observer
API and existing core fixes are exercised together.

The final performance batch passed **936 workspace/all-target tests**, all
**20 display-dependent UI functional tests**, and **19 backend tests**, using
Rust 1.89. The backend's explicit timing benchmark remains opt-in and was not
part of this final correctness run. Xvfb was extracted into a temporary directory
and used without a system install or changes to the workstation's active display.
Formatting and whitespace checks passed. Client Clippy completes with existing
warnings; the CI NES/Falling `-D warnings` check passed. Release builds on desktop
and Pi, and the Picade fast deployment, were validated in the preceding experiment.

The [final Clock matrix](clock-performance-lab.md#final-performance-batch-checkpoint-2026-09-10)
records all seven effects on both machines after the combined changes. Final
matrix and test logs are retained under
`target/clock-benchmarks/final-performance-picade-20260910/`.
