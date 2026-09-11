# Clock performance lab

Use the existing engine-client headless runner for repeatable CPU measurements,
then use live host timings to find work outside that runner. Neither is a GPU
profiler. In particular, headless throughput is **not displayed FPS**.

## Fixed-workload benchmarks

```sh
cargo build --locked --release -p engine-client
./benchmark-clock.sh
```

The wrapper runs all seven cases at 1024×768, raster scales 1 and 2, three
independent processes per combination. Each run warms up for two simulated
seconds, resets the seeded scenario, then measures 15 simulated seconds.
Each CSV row represents exactly 60 fixed steps and 60 rendered/prepared frames,
run as quickly as possible, without sleeping. Total wall time depends on the
machine, viewport, renderer and workload. Longer runs can use `--seconds 60`.

Reports go into a new unique directory under `target/clock-benchmarks/`. It
contains raw per-run CSV, separate stderr logs, and a metadata file with UTC
collection time, system identity, binary SHA-256 and workload options. The
wrapper also captures temperature and CPU governor/frequency before and after
each run where Linux exposes them, and prints a small per-run timing summary.
No saved Clock settings or
launcher choices are changed. `--output DIR` selects another report parent;
failed runs retain their partial output and show the error log.

```sh
# A smaller comparison; rerun for independent samples.
./benchmark-clock.sh --cases idle,digit-slide --repeats 3 --seconds 15

# CPU conversion only for vector; this does NOT draw the Slint scene.
./benchmark-clock.sh --renderer vector --scales 1 --repeats 3

# Fix the recipe explicitly, without borrowing the user's saved text/settings.
./benchmark-clock.sh --cases marquee --recipe text-ribbon

# A single raw run, without the wrapper.
target/release/engine-client --scenario clock --benchmark-headless --seed 7 \
  --clock-benchmark-case meltdown --renderer raster --raster-scale 2 \
  --benchmark-width 1024 --benchmark-height 768 \
  --benchmark-warmup-seconds 2 --benchmark-seconds 30 \
  --benchmark-report /tmp/clock-meltdown.csv
```

Fixtures ignore wall time, physical input, and saved Clock settings. They show
24-hour **08:08**, refresh the seconds/colon once per simulated second, and use
the fixed seed. `idle` stays anchored. Each other case previews its named event,
including construction, active phases, cleanup, the normal two-second cooldown,
and one second of idle before repeating. Digit Slide intentionally rolls all
four digits as a dense preview; ordinary minute changes usually move fewer.
Marquee uses the specified recipe and the fixed default `SPACE WARS` message.
Its 15-second cycle is the longest; use at least 15 seconds to cover every phase.

Warm-up exercises the real renderer, retaining its buffers/caches; the scenario
is reconstructed with the same seed before measurement, so warm-up length does
not change which simulation ticks are measured. Physics/event creation is still
included in the first measured cycle. For steady-state comparisons, collect
multiple complete cycles and repeats, and report cold-start effects separately.

CSV **benchmark version 2** adds metadata/resource columns to the existing host
report; consume columns by name, not numeric position:

- `viewport_width/height` are logical pixels; `internal_width/height` are the
  actual raster allocation dimensions (rounded up). Scale 2 means **four times
  as many pixels**: 1024×768 becomes 2048×1536.
- `avg_step_ms` includes scripted actions and simulation. `avg_render_ms` is
  draw-list generation. `avg_present_ms` is CPU rasterization/image creation or
  vector scene conversion—not Slint drawing, GPU upload, composition or vsync.
- `avg_raster_clear_ms` measures frame background initialization. When a single
  frame starts with an opaque, unstroked rectangle covering every pixel, the
  renderer paints that color directly and skips the redundant primitive.
  `avg_raster_other_frames_ms` then covers the remaining Clock drawing. Compare
  their sum as well as the individual stages across this optimization; work
  was both accelerated and removed, not just moved between timing buckets.
- `avg_total_ms` and total percentiles cover the same whole-frame CPU interval,
  including bookkeeping. Output/file I/O is outside that interval.
- `max_scene_items` is the peak prepared scene item/primitive count in the row
  (renderer-dependent); `max_bodies/max_colliders` report Clock physics peaks.
- `event_active_frames` distinguishes active-effect rows from cooldown/idle
  rows. Whole-cycle averages deliberately include recovery/idle work; do not
  mistake them for active-phase-only costs.
- Percentiles describe each 60-frame row. The wrapper reports the worst row's
  p95, not a whole-run p95. Compare median **per-run means** across repeats;
  averaging p95 values does not produce a combined percentile.

Clock currently exposes these fixtures headlessly only. Its normal launcher,
pause menu, local-time source and Preview controls are unchanged; `host benchmark`
and the B shortcut remain unavailable for Clock. Pizza and Spacewars retain
both visual and headless benchmarks and gain the shared viewport/warm-up metadata.

## Live CPU timings

```sh
spacewars-cli status
ssh spacewars@picade.local spacewars-cli status
```

For ordinary frame-based scenarios, the host now records a bounded rolling
window of at most **120 unpaused callbacks**. `host_*_avg_ms`, `host_*_p95_ms`
and `host_*_max_ms` expose:

- `interval`: elapsed time between host callbacks, including event-loop waits;
- `step`: input/control handling and simulation for that callback;
- `scene`: draw-list generation, counts and pointer projection setup;
- `prepare`: CPU raster/vector preparation and setting Slint properties;
- `callback`: total measured host callback work, including diagnostics/UI work.

The existing `fps` counts host callbacks, not verified display scanouts.
`host_updates_per_callback` makes catch-up visible. A low callback rate with
small `callback` cost points to work/waits **outside** the measured callback;
it does not by itself identify GPU, Slint, vsync or timer scheduling as the cause.
Native/realtime NES has its own telemetry and is not covered by these CPU stages.

Samples reset on scenario replacement, pause/resume and viewport/internal-size
changes. Publishing is at the existing roughly one-second cadence and uses
already completed callbacks; it does not instrument Slint's renderer. The window
can contain several Clock phases, so use fixed headless cases for per-event
comparisons. Time values are observations, never unit-test thresholds.

## Picade procedure

Use the Pi 4 `picade.local`, not another checkout's Pi 5 target. Deploy a matching
client with the usual updater, then return the kiosk to its launcher before
running headless tests so an active scenario does not compete for CPU/memory
bandwidth. Avoid screenshots during measured runs. Keep cooling, CPU governor,
power supply, display mode and background load comparable; collect multiple
runs and note throttling/temperature when investigating device results.

```sh
scp benchmark-clock.sh spacewars@picade.local:/tmp/benchmark-clock.sh
ssh spacewars@picade.local \
  'bash /tmp/benchmark-clock.sh --client /usr/bin/engine-client --output /tmp/clock-benchmarks'
# Copy the printed report directory back to the workstation for comparison.
```

After headless runs, launch normal Clock and sample `status` under identical
event/profile settings at scales 1 and 2. This measures the real UI separately
from the headless CPU ceiling. Restore the player's original settings afterward.
No performance optimization or quality-default change is implied by this harness.

## Initial Picade baseline (2026-09-10)

Pi 4, aarch64 Linux 6.6.63-v8, release Yocto binary, 1024×768 display. Client
SHA-256: `93a854eb7bcb23be56ae9e64bda001e8a4e97840f25f7dccfbd54f5f7acbc77c`.
The default matrix above collected 42 runs: seed 7, fixed 08:08, Clock Wave,
15 measured simulated seconds plus two warm-up seconds, three processes per
case/scale. The kiosk remained at its launcher during these headless runs.

These are **median per-run mean CPU milliseconds per frame**, including idle
and recovery phases—not displayed frame rates or active-phase percentiles:

| Fixture | Raster scale 1 | Raster scale 2 |
| --- | ---: | ---: |
| Idle | 3.575 | 11.969 |
| Falling | 3.950 | 12.853 |
| Color Cycle | 3.574 | 12.137 |
| Meltdown | 3.706 | 12.600 |
| Duck | 4.087 | 13.538 |
| Marquee (Clock Wave) | 3.693 | 12.228 |
| Digit Slide (all four digits) | 3.666 | 12.326 |

Raster preparation dominates this workload. For idle scale 2, about 4.66 ms
goes into clearing the pixel buffer and 7.25 ms into drawing Clock's frame;
scenario stepping takes about 0.004 ms. Falling's whole-cycle simulation cost
is about 0.23–0.25 ms/frame. These figures suggest examining repeated raster
work before simplifying Clock's simulation or effects.

The `ondemand` governor was unchanged. Per-run endpoint temperatures ranged
from 72.1°C to 79.4°C (initial metadata: 71.6°C); frequency snapshots ranged
from 0.9 to 1.5 GHz. These are endpoint observations, not continuous monitoring
or proof that throttling never occurred. Cases ran in the listed order with
scale 1 before scale 2, so thermal/order effects are not fully controlled.
Treat small differences between effects cautiously, and use cooler, reordered
repeats when evaluating small optimizations. This is an initial baseline, not
evidence of a regression against an earlier build.

Normal on-screen Clock was then sampled separately with profile **Off**, at
the current local time, with 103 scene primitives. Each scale had ten status
samples one second apart after the 120-callback window filled:

| Scale | Host callbacks/sec (range) | Mean callback interval | Mean callback CPU work | Mean CPU preparation |
| --- | ---: | ---: | ---: | ---: |
| 1 | 56.0–56.8 | 17.71 ms | 3.22 ms | 3.12 ms |
| 2 | 36.9–37.1 | 27.07 ms | 12.11 ms | 12.01 ms |

The means here summarize overlapping rolling windows, not independent runs.
Both maintained approximately 60 simulation updates/sec. Roughly 14–15 ms
between callbacks is outside the measured callback at either scale; the data
does not distinguish Slint/display work from timer or event-loop waits.
Reducing CPU raster work should help, but this alone does not establish the
cause of every missed display frame. Scale 2, Demo, all effect toggles and the
original 5% sound volume were restored after sampling.

Raw CSV, environment snapshots and live samples from this run are retained in
the collecting checkout's ignored
`target/clock-benchmarks/picade-20260910-TBMpiYaD/`. Future runs should keep their
own report directories and binary hashes rather than overwriting this baseline.

## Bulk RGB fills and background folding (2026-09-10)

The raster renderer now copies a 256-pixel repeating RGB block for large fills,
while keeping ordinary slice filling for short spans. This avoids the scalar
byte/halfword stores emitted for three-byte RGB slice fills in the release
desktop and ARM binaries. It retains the opaque RGB image format, reusable
buffers, existing alpha rules, rendering cadence and raster-scale defaults.
For a provably covering first rectangle, background initialization also replaces
the previously separate clear and background polygon paint. Other frames keep
the general clear-then-draw path; this is not Clock-specific rendering logic.

A fresh before/after comparison used the same Picade, 1024×768 logical viewport,
scales 1 and 2, all seven cases, three independent processes each, seed 7,
two-second warm-up and 15 measured simulated seconds. The kiosk stayed at its
launcher during both headless matrices. Both are optimized Yocto release builds:

- Before: `93a854eb7bcb23be56ae9e64bda001e8a4e97840f25f7dccfbd54f5f7acbc77c`
- After: `384050cdbca42776ee4c0d2407e1f4a71886dbe732669e6e3c241434b0ddff47`

Median per-run mean CPU milliseconds per frame at **scale 2**:

| Fixture | Before | After |
| --- | ---: | ---: |
| Idle | 12.382 | 5.374 |
| Falling | 13.066 | 6.231 |
| Color Cycle | 11.997 | 5.369 |
| Meltdown | 12.819 | 5.872 |
| Duck | 13.366 | 6.371 |
| Marquee (Clock Wave) | 12.221 | 5.383 |
| Digit Slide | 12.328 | 5.455 |

Idle's stage breakdown (medians calculated independently, so rounded rows do
not necessarily sum exactly to the whole-frame median):

| CPU stage | Before | After |
| --- | ---: | ---: |
| Clear / background initialization | 4.779 | 3.058 |
| Remaining Clock raster drawing | 7.535 | 2.251 |
| Draw-list generation | 0.046 | 0.043 |
| Scenario step | 0.004 | 0.004 |
| Image handle creation | 0.001 | 0.001 |

Thus the main gain is faster filling **and removal of an entire background
paint**, not an eightfold whole-renderer speedup. Idle scale 1 fell from 3.672 to
1.545 ms. The comparable desktop release idle scale-2 workload fell from 1.575
to 0.344 ms; rerunning the original desktop binary afterward measured 1.567 ms.
These remain CPU-only benchmarks, not displayed FPS.

Repeating the original Pi binary after the new matrix measured 12.145 ms for
idle scale 2 (three runs), versus the initial 12.382 ms. That small shift does
not explain the drop to 5.374 ms. These confirmation reports are in
`target/clock-benchmarks/fills-picade/before-check/`.

A separate Pi fill probe, cycling three 9 MiB buffers with changing RGB colors,
measured approximately 4.64 ms for slice fill and 2.88 ms for the chosen bulk
copy. Block sizes from 64 to 4096 pixels and fixed-size copy loops all clustered
around 2.86–2.90 ms; copying by repeatedly doubling the initialized prefix took
4.32 ms. This suggests buffer writes/memory traffic deserve attention beyond
loop sizing, but it is not a measurement of the Pi's peak memory bandwidth.
Short spans have different tradeoffs and still leave room for improvement.

The governor remained `ondemand`. Before endpoint temperatures were
77.4–80.3°C; after endpoints were 72.1–79.4°C, with frequency snapshots spanning
0.9–1.5 GHz in both matrices. These are not continuously controlled thermal or
clock measurements. Keep that caveat for small differences between fixtures.

Live Clock was also measured at scale 2, profile **Off**, 103 primitives, after
the 120-callback timing window filled (ten samples one second apart):

| Live metric | Before | After |
| --- | ---: | ---: |
| Host callbacks/sec | 37.55 | 45.51 |
| Mean callback interval | 26.61 ms | 22.01 ms |
| Mean measured callback work | 11.96 ms | 4.95 ms |
| Mean CPU preparation | 11.85 ms | 4.86 ms |
| Process/service CPU, fraction of one core | 99.2% | 99.1% |

The live samples use local wall time, not identical frozen clock digits, and
summarize overlapping rolling windows. CPU utilization comes from service CPU
nanoseconds divided by elapsed `/proc/uptime` over each sampling interval.
Host callback rate is not a scanout measurement. The approximately 17 ms now
outside the measured callback includes the Slint/display/event-loop path, but
is **not** a directly instrumented Slint rendering duration. The Pi uses
`linuxkms-software`; that remaining work needs direct profiling before choosing
the next presentation optimization. The renderer speedup has improved live
throughput, but has not yet reduced total CPU utilization at this setting.
Demo and the original event, message, scale and sound settings were restored.

Raw matrices are retained in the collecting checkout under
`target/clock-benchmarks/fills-picade/{before,after}/`, with matching desktop
reports under `fills-desktop-before/`, `fills-desktop-after/`, and
`fills-desktop-before-check/`. Each run directory includes the binary hash,
per-run environment observations and original CSV stage timings.
The same Picade directory retains `live-before.txt`, `live-after.txt`, and the
isolated fill probe source/results; screenshots were taken outside timed runs.

## Live LinuxKMS presentation profiling

The Pi software backend adds bounded `kms_*` fields to the existing status
response. Query it on a running kiosk, not in the headless benchmark:

```sh
ssh spacewars@picade.local 'spacewars-cli status'
```

These measurements cover work outside the host timer callback. They retain the
last **120 completed event-loop iterations**, which are not necessarily 120
draws or timer callbacks. The current in-flight iteration is excluded. Counters
are also available as lifetime totals. No per-frame output or filesystem writes
are performed; percentile sorting and text formatting happen only on a status
request. Set `SPACEWARS_KMS_PROFILE=0` before application startup to disable the
clocks and recording. Other backends do not publish these fields.

| Stage | Scope |
| --- | --- |
| `loop` | Whole event-loop iteration, including blocking waits |
| `timers` | Slint timer/animation updates, including the host's existing callback |
| `callbacks` | Queued event-loop callbacks, including IPC/status requests |
| `render` | Render-if-needed and presentation, inclusive of the stages below |
| `map` / `unmap` | Mapping/unmapping the DRM backbuffer |
| `draw` | Slint software rendering into the selected direct or RAM buffer |
| `draw_generic` / `draw_rgb` | Whole draw classified by selected texture implementation, **included in `draw`** |
| `background` | Solid window-background fill, **included in `draw`** |
| `rgb_blit` | Eligible opaque RGB texture drawing, **included in `draw`** |
| `copy` | Copying a completed RAM frame into the mapped display buffer, if enabled |
| `flip_wait` | Waiting for the previous DRM page-flip event |
| `submit` | Buffer rotation/bookkeeping and the DRM submission call |
| `dispatch` | Event dispatch, input handling and blocking poll wait |

Each stage exposes wall and **UI-thread CPU** total, mean, p95 and maximum
milliseconds. Wall time includes descheduling and waits; CPU time does not.
Statistics are per iteration in which that stage ran, summing repeated calls
within an iteration. `_calls` counts calls; `_cpu_samples` counts active
iterations with valid CPU readings. An unavailable CPU clock produces no CPU
timing fields, rather than a misleading zero. A render-if-needed call can do no
drawing: compare it with `kms_draws_window`.

The stages are nested: do **not** add `loop` to its children, `render` to its
children, `draw` to its implementation/background/blit timings, or `timers` to
the host callback's existing timings. To account for the whole loop, use stage
**totals from one window**, not averages with different denominators. The
residual includes untimed setup
and instrumentation overhead.
The host's rolling callback window is separate, so its averages need not align
exactly with this window.

`kms_output_*`, `kms_pixel_format`, `kms_buffer_bytes` and `kms_buffer_age`
describe the output and last rendered buffer, not the higher-resolution host
raster image. `kms_dirty_pixels_window` sums Slint's reported dirty regions;
compare with output pixels × draw count to identify full-screen repainting.
`kms_flip_submissions_*` counts successful page-flip requests,
`kms_flip_completions_*` counts observed completion events, and modesets and
event-read errors have separate counters. Completions may lag submissions by a
pending frame. None measures GPU work, scanout duration, or physical display
latency. The Linux framebuffer fallback has no DRM map/flip stages.

Profiler version 2 additionally reports `kms_buffer_mode` (`direct` or `ram`),
`kms_shadow_bytes` (retained RAM pixel-storage capacity), and
`kms_copy_bytes_{window,total}`. `draw` excludes the subsequent copy; use both
when comparing paths. `background` measures only the solid Slint **window**
background, not background-shaped scene items or the host's earlier raster
clear. Non-solid brushes retain Slint's fallback and have no separate fill
sample. Subtract background from draw totals to inspect the remaining scene
rendering; that residual is not exclusively image conversion.

### Picade findings (2026-09-10)

The instrumented release was fast-deployed to `picade.local`, without rebooting:
SHA-256 `6f13d9299c4a0556c6d27603af888bfd7917deeddcf80ae3c040e2bdd3056205`.
The display is 1024×768, XRGB8888, with a 3 MiB mapped output buffer. A scale-2
host raster image is separately 2048×1536 RGB (9 MiB).

Clock used profile Off, 24-hour time and 103 primitives. Measurement order was
raster 2, raster 1, vector, raster 1 again, then raster 2 twice more. Each group
has ten status samples a second apart, except the final scale-2 group with five
samples three seconds apart. The rolling windows were full before sampling;
one-second-spaced windows overlap and are **not independent benchmark trials**.
The final group spaces windows apart. Local clock digits were not frozen.

Stage CPU totals are divided by draws in the same window, then averaged across
samples. Values are milliseconds per draw, except the explicitly marked wait
and utilization rows. Inclusive whole-loop time is not an additional cost:

| Measurement | Raster 2, first | Raster 2, final | Raster 1, repeat | Vector |
| --- | ---: | ---: | ---: | ---: |
| Timer/host CPU work | 5.0 | 5.2 | 1.5 | 0.8 |
| Slint software draw CPU | 16.5 | 14.4 | 14.3 | 11.1 |
| Map + unmap + submission CPU | 0.26 | 0.25 | 0.23 | 0.21 |
| Whole-loop CPU, inclusive | 21.8 | 19.9 | 16.2 | 12.3 |
| Page-flip wait, **wall** ms | 0.02 | 0.07 | 0.35 | 4.36 |
| Service CPU, % of one core | 99.3% | 99.0% | 96.9% | 73.7% |

The initial native-scale draw was also 14.3 ms. The intermediate scale-2 repeat
was 14.4 ms. Scale-2 drawing varied materially between launches, so do not treat
the initial 2.2 ms difference from native scale as a stable supersampling cost.
Temperature snapshots across these instrumented groups spanned 74.0–78.4°C.
Raster samples reported 1.5 GHz; vector samples reported 1.1–1.5 GHz under the
unchanged `ondemand` governor. These are snapshots, not continuous clocks or a
thermally controlled comparison. They do not establish why the first scale-2
group was slower.

Findings:

- The missing time is predominantly **CPU work inside Slint drawing**, not a
  hidden frame-rate sleep. Timer + render + dispatch totals account for almost
  all loop time. At scale 2, software drawing is about 72–76% of UI-thread CPU.
- Page-flip waits barely block at scale 2. Vector mode has spare time to wait
  for flips instead of spending the whole interval on CPU. There were no
  recorded DRM event-read errors.
- Every measured draw reported a full-screen dirty region. Buffer age was 3;
  the current backend treats ages other than 1 or 2 as `NewBuffer`. Both this
  buffer policy and publishing changed full-screen content matter when later
  introducing partial updates. These measurements do not isolate their effects.
- `/proc` samples in the later groups show roughly **768 minor faults/draw**,
  matching the 768 4 KiB pages in the output buffer, and no major faults. This
  is consistent with remapping it each frame. Those page faults happen on
  access inside `draw`, not necessarily inside the short `map` call. However,
  kernel time is only about 4–8% of process CPU in these groups: mapping/faults
  alone are not a sufficient explanation for the whole cost.
- The coarse `draw` bucket includes background painting, image sampling/format
  conversion, scene traversal and mapped-buffer writes. It does **not** yet
  identify which of those dominates. Vector rendering remains substantial, so
  neither scaling nor copying the raster image explains everything by itself.

These findings motivated the RAM-versus-mapped-output experiment below, with
separate background-fill timing. Isolating opaque image drawing can then inform
a measured bulk RGB-to-XRGB path or avoidance of redundant background painting,
without assuming GPU acceleration, lower resolution, or timer changes are
required. Keeping mappings alive is a separate smaller candidate; skipping
unchanged frames will also matter for an idle clock.

Immediately before deployment, the uninstrumented optimized build measured
5.06 ms of host callback work, a 21.86 ms callback interval and 99.1% service
CPU at scale 2. The initial instrumented group measured 4.94 ms, 21.98 ms and
99.3%. This is a sanity check that profiling did not grossly change throughput,
not a precise overhead measurement or a matched-digit performance comparison.
The idle launcher also correctly reported no draws in its window and almost
all elapsed time in blocking dispatch, with only about 0.09 ms CPU per loop.

Raw status/CPU observations, binary hashes and collection/summary scripts are
retained in `target/clock-benchmarks/kms-picade-20260910/` in the collecting
checkout. The original raster renderer, scale 2, Demo, all effect toggles,
message and 5% sound volume were restored, and a screenshot was checked outside
the timed runs. No further rendering optimization was made in this profiling
step.

## RAM-backed output experiment (2026-09-10)

The software backend can now render into a reusable, correctly aligned RAM
buffer and copy the completed frame into scanout memory. **Direct rendering
remains the default**: the measured RAM path did not recover its extra copy
cost on Picade. This is an opt-in diagnostic path, not a claimed optimization:

```sh
SPACEWARS_KMS_BUFFER=ram SLINT_BACKEND=linuxkms-software engine-client ...
SPACEWARS_KMS_BUFFER=direct SLINT_BACKEND=linuxkms-software engine-client ...
```

Set the variable before process startup; `status` reports the actual mode.
Changing the environment of an SSH command does not change an already-running
kiosk service. The default also leaves LinuxFB's existing RAM-buffer path alone.

The comparison intentionally keeps the same pixels, resolution, pixel formats,
scene rendering and page-flip scheduling. RAM mode forces `NewBuffer` rendering
and copies the **entire frame** every draw, regardless of the target buffer's
age. It does not claim that one shared RAM image is the previous contents of
each of the three display buffers. This avoids stale pixels without introducing
damage-history tracking or a partial-update optimization into the experiment.
Direct mode keeps its existing buffer-age policy. Both paths fully repainted
the screen in the measured gameplay windows.

The shadow allocation is reused for matching sizes/formats and replaced when
the pixel format changes. It adds 3 MiB on this 1024×768 XRGB8888 display; direct
mode has no shadow pixel allocation. Copying is timed separately from drawing,
while the solid window-background fill is timed within drawing. All other
scene operations in this RAM comparison use Slint's existing fallback; it does
not introduce custom texture sampling or alpha compositing.

Backend tests compare both paths with Slint's original slice renderer, not just
with each other: RGB and RGBA images, opacity, clipped/scaled images, rounded
rectangles, borders and text; XRGB8888, ARGB8888 and RGB565 output; all four
rotations; and growth/shrinkage of non-square windows. Guard pixels must remain
untouched. Separate tests check buffer reuse, whole-frame copying into rotating
targets, unsupported formats, the default/explicit mode choices, and byte/timing
accounting. The Slint dev dependency in the backend's standalone manifest is for
these headless UI fixtures; it does not add a new runtime renderer dependency.

### Measured result: keep direct rendering

Both paths were built as optimized Yocto releases and fast-deployed to the same
Picade Pi 4, without rebooting or changing the display configuration:

- RAM-default experimental binary SHA-256:
  `b7b483739dc0f204f6e31819ce0c61b228cb7bc7124b3332f2780fbb46f37d50`.
- Final direct-default binary SHA-256:
  `89ebcf78f7495e316fc8fed687140a61b1b5118e90895a5e95222958766672d6`.

Clock used Off, 24-hour time and 103 primitives. Each configuration had five
status samples three seconds apart after at least five seconds of warm-up.
Raw status was checked for actual buffer mode, renderer, raster scale, unpaused
Clock, process identity and scenario revision; filenames alone are not evidence
of the configuration. The collector now enforces these checks as well. Each
sample summarizes up to 120 completed loops, with
stage totals divided by draws in the same window. These are short live
observations with rolling windows, not independent fixed-workload trials.

| Rendering configuration | Direct draw CPU | RAM draw CPU | RAM copy CPU | RAM draw + copy CPU | Service CPU, direct / RAM |
| --- | ---: | ---: | ---: | ---: | ---: |
| Raster 2× | 16.85 ms | 15.48 ms | 2.18 ms | 17.66 ms | 99.1% / 99.3% |
| Raster 1× | 14.22 ms | 12.96 ms | 2.31 ms | 15.27 ms | 96.4% / 98.6% |
| Vector | 11.11 ms | 9.57 ms | 2.50 ms | 12.07 ms | 73.5% / 79.6% |

CPU timings are per draw; service utilization is a percentage of one core.
Direct mode has zero copy calls and zero retained shadow bytes. RAM mode copies
3 MiB per draw. Background filling is **already included** in each draw column:
it took 1.83–2.10 ms direct and 0.99–1.05 ms in RAM. Despite cheaper RAM drawing,
the extra copy exceeded the saving in all three observed configurations.
Observed flip-completion rates were approximately 44.7 / 43.1 per second at
raster 2×, 60.0 / 57.7 at raster 1×, and 60.0 / 60.0 in vector mode
(direct / RAM); these are not input-to-display latency measurements.

Clock digits were not frozen, and temperature/frequency were not controlled.
Across these groups temperature snapshots ranged from 72.5 to 78.9°C. Raster
samples reported 1.5 GHz; vector samples reported 1.2–1.5 GHz under the unchanged
governor. Earlier direct scale-2 runs varied from about 14.1 to 16.7 ms of draw
CPU between launches; the final direct result above uses the same new fill
instrumentation as RAM. Do not interpret small differences as universal or as
a tightly isolated estimate of memory-cache effects. The useful conclusion is
**no demonstrated net win from the extra RAM frame and copy**, so it is not
enabled by default.

The background accounts for only part of drawing: after subtracting it, RAM
still spent roughly 8.5–14.5 ms per draw on the rest of Slint's scene work. Both
paths still incurred about 768 minor faults per draw and no major faults, with
kernel time only about 5–8.5% of process CPU. Moving writes into the copy stage
does not eliminate the mapped-output cost, and neither faults nor the solid
background alone explains the remaining time. The next experiment should
isolate and optimize Slint's opaque RGB-image drawing path, comparing identical
pixels at native and scaled sizes before changing production behavior. Vector
cost also needs accounting; this result does not attribute all remaining time
to image conversion.

The six validated reports, collection/summary scripts, build/deployment logs,
test results and restored status are retained in the collecting checkout under
`target/clock-benchmarks/kms-ram-picade-20260910/`. The final direct-default build
is deployed. Raster 2×, Demo, all six event toggles, 24-hour time, Clock wave,
`SPACE WARS` and unmuted 5% volume were restored; no Pi 5 target was changed.

## Opaque RGB blit experiment (2026-09-10)

The next experiment specializes image drawing through Slint's existing
`TargetPixelBuffer::draw_texture` hook in the **LinuxKMS software backend**. It
does not change the scenario rasterizer, GPU backends, or the desktop Winit
backend. It accepts only opaque RGB8 textures without tinting, tiling or
rotation, with an exact 1:1 or 2:1 source-to-destination ratio on each axis.
Source dimensions are limited to 4096 pixels per axis, with pixel-aligned,
representable strides and coordinates, to preserve Slint's fixed-point limits.
Other operations return to Slint's original renderer without writing pixels.

For these integer ratios, Slint's fixed-point nearest-neighbor sampling reduces
to fixed-size RGB chunks. The specialized loop converts them directly to the
target pixel format, with no intermediate image or destination reads. It still
honors source crop/stride, destination placement, clipping and partial damage.
It does not skip background painting or change repaint/page-flip scheduling.
The RAM-output experiment remains separate and opt-in.

Set the texture implementation before application startup:

```sh
SPACEWARS_KMS_TEXTURE=generic SLINT_BACKEND=linuxkms-software engine-client ...
SPACEWARS_KMS_TEXTURE=rgb SLINT_BACKEND=linuxkms-software engine-client ...
SPACEWARS_KMS_TEXTURE=compare SLINT_BACKEND=linuxkms-software engine-client ...
```

`rgb` is the default, based on the paired Picade comparison below. `generic`
provides a rollback/comparison without changing the other presentation work.

Profiler version 4 reports the selected `kms_texture_mode`, handled
`kms_rgb_blits_{window,total}`, written `kms_rgb_pixels_{window,total}`, and
`kms_rgb_blit_*` timings. Blit time is **included in `draw`**, not additional to
it. Rejected/fallback textures are not counted or timed as RGB blits. A handled
texture with an empty clipped region can count as a blit with zero pixels.
The mode alone does not prove a scene uses the fast path; check these counters.

`compare` alternates generic and RGB-optimized **frames**, drawing each frame
only once. It is a diagnostic mode, not a shipping default. This lets both paths
run in the same process, with interleaved temperatures, frequency and scene
content. Because the output ring has three buffers, each path visits all three.
`kms_draw_generic_*` and `kms_draw_rgb_*` separately time the whole Slint draw;
each is included in `draw`. Compare their per-active-iteration CPU means, or
divide their CPU totals by their own valid sample counts, **not** by the total
number of frames from both paths. In steady-state Clock there is one draw per
active iteration. Clock digits still change, so this is not a frozen-scene
microbenchmark; use Off and validate that no effect is active throughout.

### Fixed-content benchmark and pixel tests

A headless backend test compares generic and specialized drawing into the same
1024×768 XRGB RAM buffer. Fixtures are a background-only window, a native-size
RGB image, and a 2048×1536 image reduced by exactly two. It measures complete
Slint draws (including the background), not the engine's rasterizer or DRM
presentation. Each mode warms up for ten frames, then measures 100 frames, with
four repeats and alternating mode order. Image content is fixed, profiling is
inactive, and there are no pass/fail timing thresholds:

```sh
CARGO_PROFILE_RELEASE_LTO=fat CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
  cargo test --release --locked \
  --manifest-path vendor/i-slint-backend-linuxkms/Cargo.toml \
  --features renderer-software --lib benchmark_opaque_rgb_presentation \
  -- --ignored --nocapture
```

The first optimized desktop run measured roughly 0.025 ms for background-only
drawing in either mode, 0.645 → 0.299 ms for the native image, and
0.654 → 0.313 ms for the 2:1 image. These are RAM microbenchmark results, not
claims about desktop application speed or Pi display performance.

Correctness tests compare against Slint's original slice renderer. In addition
to the earlier full-window matrix, 640 comparisons cover independent X/Y
integer scales, source cropping with padded rows, negative destination offsets,
clipping, output row padding/guards, both target pixel representations, all
rotations, alpha images, opacity, tinting, tiling, fractional scales and sources
beyond the fixed-point fast-path limit. They assert that eligible images
actually use the specialized path and rejected images leave the buffer
untouched before fallback. A separate eight-frame,
two-renderer test compares partial redraws with overlapping and disjoint damage
against the original renderer. Mode parsing and profiling counters have tests.

### Paired Picade comparison

The comparison release has SHA-256
`65d7c019fe4e256231027bc0b5d49b116ff0eb07cc30c21b37c6fb0209aa210a`.
Clock used Off, 24-hour time, 103 primitives and direct 1024×768 XRGB8888 output.
Each configuration warmed up for five seconds, then collected six status samples
three seconds apart. The same process alternated generic/RGB frames throughout;
neither mode drew a second copy of the frame. The means below pool each mode's
CPU totals and divide by that mode's valid CPU sample counts.

| Clock presentation | Generic draw CPU | RGB-enabled draw CPU | Saving |
| --- | ---: | ---: | ---: |
| Raster 2×, 2048×1536 source | 16.53 ms | 14.85 ms | 1.68 ms / 10.2% |
| Raster 1×, 1024×768 source | 14.93 ms | 11.39 ms | 3.54 ms / 23.7% |
| Vector, no eligible RGB blits | 11.09 ms | 11.29 ms | No demonstrated gain |

Each RGB-enabled raster frame handled one full-screen blit; generic frames
handled none. Vector is a negative control: it does not use the fast path and
its small difference is not evidence of an improvement. These are **whole Slint
draw** savings, not equivalent percentages of total application CPU or latency.
The alternating workload completed about 47 flips/s at raster 2× and 60 flips/s
at raster 1×/vector; these are mixed-mode throughput figures, not pure RGB-mode
performance.

Earlier separate-process measurements varied enough to obscure the scale-2
gain (generic draw 14.42 ms versus RGB 14.83 ms). That motivated the same-process
comparison rather than selecting a favorable launch. Temperature snapshots
ranged from approximately 72–76°C, and frequency snapshots from 1.3–1.5 GHz;
the governor was not changed. Interleaving reduces launch/cache/thermal bias but
does not freeze clock digits, control frequency, or turn rolling status windows
into independent trials. Retain the raw reports when interpreting small changes.

The specialized blit itself takes approximately 3.23 ms at scale 2 and 1.83 ms
at scale 1. Independent RGB-only runs still leave about 8–10 ms of Slint drawing
after subtracting both the timed window background and blit. This residual
includes other scene rendering/setup; it is **not** measured image conversion
or a demonstrated DRM wait. The optimization helps, but does not explain all
of the remaining display cost.

The final RGB-default release has SHA-256
`a6c94e135ff2a6e82ce4041ba504df8d166db62ce737c8eebb01b48f2ed8f2c6`.
A five-sample, three-second-spaced follow-up at raster 2× measured 14.78 ms draw
CPU, 3.21 ms in the RGB blit, 1.82 ms in the window background, and 4.85 ms in the
host callback. It still used about 99% of one core, with roughly 20.2 ms between
frames (about 50 FPS). It handled one optimized blit per draw with no shadow
buffer or copy. This verifies the deployed default, not another paired trial.

### Falling as a comparison workload

Falling was measured in the same comparison process, with the same display and
six samples three seconds apart after warm-up. This is the **title screen**, not
an active-gameplay benchmark. Sound was muted to avoid playing music during the
measurement; the emulated APU and audio output thread remained active.

Falling uses a separate NES runner thread and the native-video path, rather than
Clock's scene generation and rasterization. Its 256×240 indexed framebuffer is
expanded into a reused RGB8 buffer (180 KiB), cropped to 256×224, then displayed
by Slint using aspect-preserving nearest-neighbor upscaling. Clock's scale-2
source is 9 MiB. The launcher's raster/vector and raster-scale fields do **not**
select Falling's native-video rendering path.

| Falling title-screen measurement | Result |
| --- | ---: |
| Displayed page-flip completions | 60.0/s |
| Reported emulated frames | 60.0/s |
| Whole process CPU, one core = 100% | 154.6% |
| UI/main thread CPU | 84.4% |
| NES runner thread CPU | 68.7% |
| Audio output thread CPU | 1.2% |
| Slint draw CPU per draw | 13.05 ms |
| Window background CPU, included in draw | 1.94 ms |
| Queued callbacks CPU per draw, including video conversion/presentation | 0.66 ms |
| Timer CPU per draw | 0.067 ms |
| Page-flip wait wall time per draw | 2.48 ms |

Process CPU uses systemd CPU-accounting deltas; thread CPU uses `/proc` task
user/kernel tick deltas over the same collection interval. Stage values pool
the backend windows and normalize by actual draw count, not event-loop count:
native-video wakeups also produce loops without a draw. Callback time includes
all queued work and is not an isolated measurement of palette expansion. The
Clock-specific host callback fields are absent for this path, **not zero**.

There were zero optimized RGB blits: fractional upscaling is deliberately left
to Slint's original sampler. Generic/RGB-enabled draws measured 13.06/13.04 ms,
consistent with both using the same fallback. Temperature snapshots were
72–75°C and frequency snapshots 1.3–1.5 GHz. A screenshot taken outside the timed
run verifies the title-screen workload, not physical scanout correctness.

Falling therefore demonstrates that Clock's high-resolution rasterizer is not
necessary for substantial UI CPU usage. Its emulation is additional parallel
work, while software image scaling and the rest of Slint's scene drawing still
consume most of the UI-thread budget. Further attribution inside the remaining
draw work is needed before choosing the next optimization; this comparison does
not establish that all of that time is scaling.

Raw reports, collectors, pooled timing summaries, binary manifests, test/build
logs and the Falling screenshot are retained in the collecting checkout under
`target/clock-benchmarks/rgb-blit-picade-20260910/`. The RGB-default release was
fast-deployed to Picade without a reboot. Clock Demo, raster 2×, all six effects,
24-hour time, Clock wave, `SPACE WARS`, and unmuted 5% volume were restored.
The Pi 5 target was not changed. Validation passed 930 workspace tests (20
display-dependent tests skipped), 18 backend tests, and the explicitly run
release presentation benchmark. The backend tests include the pixel-equivalence
matrix and partial-damage checks described above.

### Follow-up: attributing the remaining draw cost

The [software presentation lab](presentation-performance-lab.md) adds nested
draw attribution and identical frozen-image comparisons against a bare window.
Its diagnostic Picade build measured **8.97 ms in dirty-region calculation**
inside Clock's 14.85 ms draw, and 6.90 ms inside Falling's 13.26 ms draw. This
accounts for much of the previously unattributed time above.

The full UI also adds about 7 ms when drawing identical frozen pixels in RAM,
including a background-only fixture. Quartering output pixel count does not
remove that cost, nor does disabling the detailed timers. The next target is
UI-tree/bounds bookkeeping, particularly inactive menus. No repaint-policy
optimization has been made in this follow-up; see the lab for raw-report
locations, controls, limitations and the next experiment.

The subsequent conditional-panel experiment removes inactive menus and HUD
subtrees from the application's item tree, while retaining settings and focus
state in the root window. Live Clock dirty-region CPU falls to about 0.06 ms,
whole draw CPU to 7.06 ms, and process CPU from 99% to 77% of one core while
reaching 60 FPS instead of about 49. Falling also benefits. Rendering resolution
and visual output are unchanged; 34 before/after menu PNGs and frozen-image
checksums match. See the [conditional-panel results](presentation-performance-lab.md#conditional-panel-experiment-2026-09-10)
for repeat runs, controls, thermal caveats and lifecycle coverage.

## Final performance-batch checkpoint (2026-09-10)

The complete matrix was rerun after all three optimizations: bulk RGB fills and
background folding, the guarded LinuxKMS RGB blit, and conditional UI panels.
The deployed Picade release is
`8d9e714b108298e088991e71702a2dd287bddebe0d2813e0c44975f478bef98d`.
The desktop release is
`d97eccf57ee391e15e086064ed5319512876d657371d599165fbf8f86ab3e65a`.

Each machine completed 42 independent runs and 37,800 measured frames: all
seven cases, scales 1 and 2, three repeats, seed 7, 1024×768 viewport, Clock
Wave, fixed 08:08, two simulated warm-up seconds and 15 measured seconds. The
Pi kiosk stayed at its idle launcher; local build/test workloads started only
after the desktop measurements finished. Every CSV contained the expected 900
measured frames and matching fixture/dimension metadata. Event-active frame
counts and maximum body counts agreed across repeats and both machines.

Median per-run mean milliseconds per frame on Picade, compared with the
initial baseline above:

| Fixture | Initial 1× | Final 1× | Initial 2× | Final 2× |
| --- | ---: | ---: | ---: | ---: |
| Idle | 3.575 | 1.509 | 11.969 | 5.383 |
| Falling | 3.950 | 1.954 | 12.853 | 6.199 |
| Color Cycle | 3.574 | 1.558 | 12.137 | 5.401 |
| Meltdown | 3.706 | 1.683 | 12.600 | 5.850 |
| Duck | 4.087 | 1.877 | 13.538 | 6.433 |
| Marquee (Clock Wave) | 3.693 | 1.513 | 12.228 | 5.452 |
| Digit Slide | 3.666 | 1.565 | 12.326 | 5.479 |

This confirms the earlier raster gains across the complete effect matrix:
about **2.0–2.4× faster** than the initial baseline. Desktop final means range
from 0.121–0.200 ms at 1× and 0.319–0.516 ms at 2×. These headless measurements
cover stepping, scene construction and CPU raster preparation, including idle
and recovery phases. They **exclude Slint display drawing and page flips**;
the later presentation optimizations are validated by the separate frozen-image
and live measurements above, not by these CSVs. This is not evidence that every
active effect sustains 60 displayed FPS.

Picade endpoint temperature readings were 74.5–77.4°C, with `ondemand` frequency
snapshots of 0.9–1.5 GHz. This checkpoint did not rerun the initial binary or
control frequency continuously; use the paired and baseline-recheck trials in
the earlier sections for attribution, not small differences between this table
and those trials.

Raw CSVs, environment snapshots, binary metadata, the summary script and test
logs are retained in this checkout under
`target/clock-benchmarks/final-performance-picade-20260910/`. Picade was restored
to unpaused Clock Demo, raster 2×, all six effects, 24-hour time, Clock Wave,
`SPACE WARS`, and its original unmuted 5% volume. No new deployment, service
restart, reboot, or Pi 5 change was needed for this checkpoint.

Final validation passed 936 workspace/all-target tests, all 20 display-dependent
UI functional tests under Xvfb, and 19 vendored LinuxKMS tests. The standalone
backend's explicit timing benchmark remains opt-in. Rust 1.89 formatting,
whitespace checks, and the strict NES/Falling CI Clippy check passed. Client
Clippy completed with existing warnings; it is not a warning-free check.

## Regression coverage

```sh
cargo test --locked -p engine-client --test benchmark_headless
cargo test --locked -p engine-client every_fixture_repeats
cargo test --locked -p engine-client host::profiling
```

Tests check deterministic action/frame replay, complete-cycle cleanup, zero
physics bodies/colliders for Digit Slide, CSV alignment and dimensions, untouched saved
settings, warm-up reset equivalence, bounded live samples and size-change reset.
The CLI tests remove both display environment variables and assert no timing
thresholds. Existing display-dependent Clock workflows cover the normal UI.

Raster correctness tests (`cargo test --locked -p engine-client raster::tests`)
compare bulk RGB fills against ordinary slice filling, including short spans,
partial blocks and unaligned starts. Background folding is checked against the
original clear-then-draw path, including partial/transparent backgrounds,
strokes, winding, layer ordering, buffer reuse and resize. All six Clock effects
are sampled through their active and recovered phases at landscape, portrait,
and fractional-aspect viewports and must produce identical RGB pixels.

The vendored backend's library tests cover bounded/completed windows, CPU-clock
unavailability, counter semantics, repeated-stage aggregation, scope cleanup on
early return, and disabled profiles. There are no machine-speed thresholds:

```sh
cargo test --locked --manifest-path vendor/i-slint-backend-linuxkms/Cargo.toml \
  --features renderer-software --lib
```

This is also run by Linux CI and requires the Linux input/display development
libraries listed in that job. The actual `pi-kiosk` build additionally needs
libseat; the Yocto application recipe provides all target dependencies.
