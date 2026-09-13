# Live text cost and profiler overhead

The performance checkpoint is commit `7f6f9f3`. This follow-up investigates the
roughly 7.8 ms live text bucket after terrain scene culling. It adds a narrower
breakdown and removes a measured source of profiler overhead. Text content,
fonts, layout, colors and rasterization are unchanged.

## The clock experiment

The old LinuxKMS observer read both monotonic and thread-CPU clocks at the start
and end of every nested renderer operation. Each visible glyph normally creates
a texture span and a fallback span. The thread-CPU clock reads are
materially more expensive here than monotonic timestamps. The ordinary RAM probe only
read monotonic clocks, so these two measurements had different overhead.

`--presentation-text --presentation-cpu-clocks --presentation-detail` reproduces
the former CPU-clock policy in the headless probe. Both variants use the same
call nesting, executable, and frozen seed-42 match/HUD. Both also include the new
font/glyph-run spans: this measures clock-policy overhead in that workload, not
a bit-for-bit rerun of the old profiler binary. Each run compares replaced/retained models with exact pixel equality.
The changing fixture changes one label and exercises insertion/removal and an
empty overlay. It is synthetic UI variation, not a full live match replay.

The Pi sequence was wall-only → CPU clocks → wall-only, with three paired
repeats and 40 measured draws per window after ten warmup draws. Output was
1024×768 in ordinary XRGB8888 RAM. Simulation and geometry rasterization precede
the timers. Temperature was 67.7–70.1°C; 14 of 15 frequency samples were 1.5 GHz,
with one at 1.4 GHz. The installed kiosk was temporarily suspended under a resume
trap while the separate diagnostic executable ran; settings were unchanged.

| Retained text fixture | Wall-only full draw (two controls) | With CPU clocks | Added draw cost |
| --- | ---: | ---: | ---: |
| Frozen | 9.86 / 9.92 ms | 11.63 ms | 1.74 ms |
| Changing | 10.57 / 10.68 ms | 12.14 ms | 1.51 ms |

The frozen text bucket alone rises from 2.42 ms to 4.05 ms. All checksums agree
across clock modes and repetitions, and every replaced/retained comparison
matches pixel-for-pixel. This explains part of the live/frozen gap; it does not
establish that all of the remaining gap comes from display memory.

Frozen retained-text breakdown, wall-only:

| Work | Per draw |
| --- | ---: |
| All text | 2.42 ms |
| Font matching | 0.09 ms |
| Glyph lookup, positioning and painting | 0.97 ms |
| Nested glyph texture operations | 0.80 ms |
| Nested generic glyph fallback | 0.74 ms |
| Remaining text work, including layout | 1.36 ms |

There are 16 text lines and 336 glyph texture operations per frozen draw. The
last row is an arithmetic residual, not a direct shaping-only measurement.
Nested rows must not be added to their parents. Glyph bitmap caching already
exists in Slint; these timings do not justify adding another glyph cache or
changing fonts.

## Implementation

LinuxKMS profiler version 6 uses monotonic clocks for `kms_core_*` observer
spans. It retains thread-CPU clocks for the coarser `Scope` stages, including
loop, timers, render and draw. Core-stage `cpu_samples` are zero and their CPU
timing values are absent; they are unmeasured, not zero cost. The existing
`SPACEWARS_KMS_DRAW_DETAIL=0` and `SPACEWARS_KMS_PROFILE=0` switches remain.

Two clock-free Slint hooks distinguish font matching during drawing and a
laid-out line's glyph work. The observer also attributes existing texture and
fallback spans to text when nested inside a text call, reusing their elapsed
time without adding clock reads per glyph. New status suffixes are
`core_text_font`, `core_text_glyph_run`, `core_text_texture` and
`core_text_fallback`. Bounds/layout work inside `core_dirty` stays separate.

The RAM probe exports the same subdivision, operation counts and an explicit
`cpu_clocks` column. Its optional CPU-clock mode remains a reproduction of the
old observer, not the new default. The bare/full probe's column names were
extended to match the added diagnostic kinds.

## Live results and remaining work

Fast-deployed to `sw-picade.local` without rebooting. Installed and running client
SHA-256 match `992c4bf1f1e9d61b9ebaa739688a7bdcc83186e7aafa699b7e73dc3be5beb192`.
The kiosk is active with zero crash restarts. Settings are byte-for-byte unchanged;
automatic ordinary two-bot Spacewars resumes at native 1×, 1024×768 output.

Ten status samples span 48.15 seconds of one match. Counter differences give
**36.51 FPS / 58.11 updates/s**. Rolling FPS ranges from 9.1 to 56.0. This is a
different match phase from the previous 38.08 FPS capture; it is not a controlled
end-to-end FPS comparison or evidence of a rendering regression. The clock-mode
experiment above establishes the profiling overhead with unchanged inputs.

| Live stage | Mean |
| --- | ---: |
| Simulation/control per displayed frame | 7.55 ms |
| Scene construction | 1.16 ms |
| Host preparation | 6.07 ms |
| KMS/Slint presentation | 14.49 ms |
| Nested text | 6.41 ms |
| Nested font matching | 0.10 ms |
| Nested glyph lookup/placement/painting | 4.70 ms |
| Nested glyph texture operations | 4.48 ms |
| Nested glyph fallback | 4.39 ms |
| Text residual, including layout | 1.61 ms |

There are about 386 glyph texture operations per displayed frame. The live
fallback is about 11.4 microseconds per glyph, versus about 2.2 microseconds in
the frozen RAM fixture. Glyphs and text sizes differ, so this normalization is
indicative, not an exact memory benchmark. It does establish that font matching
is small here, and that most remaining text time lies in glyph texture drawing.

**Leading next experiment:** replay the same frozen glyph workload into ordinary
RAM and a private mapped DRM buffer, without presenting it. If the gap follows
destination memory, compare transparent/opaque blend shortcuts or compositing
small HUD regions in RAM. Include transfer cost, clipping, resize, rotation and
pixel-equivalence checks. The current XRGB target converts/reads the destination
for every blend, including fully transparent/opaque glyph pixels; this is a
candidate to measure, not a proven saving yet. Avoid changing fonts or adding
another glyph cache based on this evidence.

The 9.1 FPS window reports simulation/control averaging 18.02 ms, p95 76.93 ms
and max 120.15 ms, while scene construction is 1.05 ms and KMS presentation is
14.40 ms. The host-step bucket combines policy and physics; this capture does
not identify the individual function. Preserve `live-after-03.json` and reopen
sensor/physics attribution using the
[on-foot survey reproduction](on-foot-survey-fix.md). The older investigation
already tracks ground-map spikes, but this new stall is not attributed to that
specific call without a trace. Rendering improvements alone cannot ensure a
steady frame rate while these stalls remain.

## Validation

Five headless presentation integration tests pass, including timing hierarchy,
clock-mode pixel equality, consistent CSV fields and untouched malformed
settings. Both rendered UI regressions pass. Twenty standalone LinuxKMS tests
pass, including nested text/image attribution, absence of nested CPU clocks,
and pixel comparisons across formats, rotations, alpha and resize; its explicit
timing benchmark remains ignored. Desktop and Pi clock comparisons preserve
pixels. Formatting/whitespace checks and both Pi builds succeed.

## Reproduction

```sh
cargo +1.89.0 test --locked -p engine-client --test presentation_probe
cargo +1.89.0 test --locked --manifest-path vendor/i-slint-backend-linuxkms/Cargo.toml \
  --features renderer-software --lib
cargo +1.89.0 build --locked --release -p engine-client
SLINT_BACKEND=nonexistent-backend target/release/engine-client \
  --benchmark-presentation --presentation-text --presentation-detail \
  --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 40 --presentation-repeats 3 > /tmp/text-wall.csv
SLINT_BACKEND=nonexistent-backend target/release/engine-client \
  --benchmark-presentation --presentation-text --presentation-detail \
  --presentation-cpu-clocks --benchmark-width 1024 --benchmark-height 768 \
  --presentation-frames 40 --presentation-repeats 3 > /tmp/text-cpu.csv
```

Backend tests need the ordinary libinput development linker name. On this host,
the runtime library existed but that symlink did not. A temporary directory
supplied `libinput.so` → `/usr/lib/x86_64-linux-gnu/libinput.so.10` and `libseat.so`
→ `/usr/lib/x86_64-linux-gnu/libseat.so.1`; `LIBRARY_PATH` pointed there for the
standalone tests. No system package or library was changed.

Artifacts, exact binaries, raw CSV/status captures, scripts and source patches:
`/home/oldman/.codex/visualizations/2026/09/12/rounder-planets/text-drawing/`
