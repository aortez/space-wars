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
