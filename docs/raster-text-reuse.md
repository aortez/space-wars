# Retain raster text items between frames

The 2026-09-12 Pi match profile measured about 52 ms per frame producing and
presenting the scene, including approximately 8 ms in Slint dirty-region work
and 8 ms drawing text. The host created a new `VecModel` for all raster text on
every frame. Even unchanged labels therefore lost their item/bounds caches.

The raster path now retains its model, updates changed rows, and adds/removes
rows as needed. An empty or different model is initialized normally. Text,
position, clipping, font and color still update each frame. The scene image,
resolution, simulation and vector presentation path are unchanged. This change
targets text bookkeeping; it does not remove the remaining glyph-drawing cost.

## Controlled comparison

The presentation probe's `--presentation-text` mode uses the real Spacewars
client adapter with two bots, seed 42, after 120 simulation ticks. It freezes
the game image at raster scale 2, then compares two full application windows:
the original model replacement and the production row-update function. Scene
generation/rasterization happen before measurement. Publication is timed
separately from Slint drawing, and every output buffer must match exactly.

The frozen fixture keeps all 16 labels unchanged. The changing fixture changes
one label's content, position and font, inserts/removes rows, and periodically
clears the model. Other labels stay unchanged. Order alternates between paired
repeats; each repeat warms up for ten frames. Output is XRGB8888 ordinary RAM
using Slint's generic texture path, with full repainting and no display pacing.
These are draw timings, not cabinet FPS or thermal-controlled estimates.

Desktop, 1024×768, three paired repeats of 40 measured frames:

| Fixture | Original draw | Retained draw | Original / retained dirty work |
| --- | ---: | ---: | ---: |
| Frozen labels | 2.418 ms | 1.254 ms | 0.975 / 0.007 ms |
| Changing labels | 2.201 ms | 1.355 ms | 0.855 / 0.150 ms |

All frames match byte-for-byte. Text drawing itself stays around 0.35 ms in the
frozen fixture; the gain comes chiefly from preserving bounds/item caches.
Model update cost grows from about 0.2 microseconds to 1.9 microseconds for
frozen labels, and 4.4 microseconds for changing labels, well below the saved
draw time. Font and platform differences preclude comparing desktop and Pi
pixel hashes; comparisons are within each platform.

The Pi 4 comparison uses the same sizes, frames and repeats, with the kiosk
process temporarily suspended after a normal UI pause. A trap resumes the
process, and the UI resumes after measurement. Temperature was 72.55 °C at both
ends. The governor was not fixed. Both runs passed every pixel comparison:

| Pi fixture | Original draw | Retained draw | Original / retained dirty work |
| --- | ---: | ---: | ---: |
| Frozen labels, detailed timers | 18.145 ms | 9.892 ms | 6.995 / 0.087 ms |
| Changing labels, detailed timers | 16.571 ms | 10.612 ms | 6.076 / 1.123 ms |
| Frozen labels, no detailed timers | 18.007 ms | 9.881 ms | unmeasured |
| Changing labels, no detailed timers | 16.522 ms | 10.625 ms | unmeasured |

The reduced dirty-region work explains most of the saving. Item creation and
other deferred work also contribute to the full draw difference; the timings
do not separately attribute those costs. Retained-model publication costs
approximately 0.010 ms for frozen rows and 0.053–0.056 ms for changing rows.
The probe's text drawing is cheaper than the earlier live match's longer HUD
labels; it is a controlled model-update comparison, not a complete reconstruction
of that match. Tests also pass for 13 projection/presentation cases and both
rendered UI regression tests, including the reused-framebuffer text lifecycle.

## Live cabinet validation

Fast-deployed to `sw-picade.local` on 2026-09-12 without rebooting or changing
saved settings. Both the installed and running client match SHA256
`96db7ca5a161edfbf078950939d596e9ad3fb952edc144d2b08c632b19e85e92`.
The automatic ordinary Spacewars match resumed, PID 2112, with zero crash
restarts. The preceding executable is preserved in the artifact directory and
by the fast-update helper.

Seven status samples spanning approximately 32 seconds per collection:

| Live measurement | Before | After |
| --- | ---: | ---: |
| Submitted FPS, counter differences | 17.32 | 21.37 |
| Simulation updates/s, counter differences | 61.19 | 61.12 |
| KMS presentation, mean rolling wall time | 25.82 ms | 18.23 ms |
| Nested dirty-region work | 8.35 ms | 1.67 ms |
| Nested text drawing | 7.99 ms | 8.19 ms |
| Host scene construction | 7.94 ms | 6.86 ms |
| Host preparation, principally rasterization | 20.64 ms | 19.29 ms |

Status counters update on the application's reporting cadence, so short-window
UPS estimates can slightly exceed the 60 Hz target. After-deployment rolling
FPS samples range from 18.5 to 24.0. These are different generated matches and
phases, not a matched total-FPS experiment. The paired identical-pixel probe
establishes the model-update benefit; the live data confirms reduced dirty work
in the real KMS path. Variable simulation and geometry costs still affect FPS.
Nested stages are not additive to their parent presentation timer.

The next target is the roughly 19 ms host preparation bucket. Use the existing
raster layer timings to separate player geometry, overviews, strokes and pixel
fills before selecting an algorithm. Actual text drawing remains around 8 ms
in the live HUD and is a separate later candidate; this change preserves layout
caches rather than caching rendered glyph output.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p engine-client
target/release/engine-client --benchmark-presentation --presentation-text \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 40 --presentation-repeats 3 --presentation-detail

cargo +1.89.0 test --locked --release -p engine-client \
  --bin engine-client ui_render_tests
cargo +1.89.0 test --locked --release -p engine-client \
  --bin engine-client render::tests
```

Omit `--presentation-detail` to check without nested timestamps. The probe does
not load settings or connect to the real display. The separate rendered
regression compares retained text on a reused framebuffer against replaced
models with full repainting, across text/style/position changes, clipping,
insertions/removals, empty/restored overlays and landscape/portrait resizing.

Artifacts, exact binaries, paired CSVs, live status samples and source patches
are retained outside Git at
`/home/oldman/.codex/visualizations/2026/09/12/rounder-planets/render-profile/`.
