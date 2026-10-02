# Drain-wall collision cost on Picade

Paired measurements for the Clock drain-wall fix in [PR
#153](https://github.com/aortez/space-wars/pull/153), collected on 2026-10-02.

The fix adds a modest whole-frame CPU cost: Heavy Rain increases by **0.231 ms at 1× and
0.425 ms at 2×** (2.4% and 1.4%). Meltdown increases by 0.046 ms and 0.225 ms (0.7% and
1.0%). Idle differences stay below 0.06 ms. Rain simulation itself is about 20–22% more
expensive, so the added work is measurable, but raster preparation still dominates the
total at either scale. These results support keeping the collision fix without an
optimization detour.

## Workload and comparison

The target was `sw-picade.local`, a Raspberry Pi 4 Model B Rev 1.4 running aarch64 Linux
6.6.63-v8. Both binaries used Rust 1.94.0, release optimization with fat LTO and one
codegen unit, and were Yocto builds using the same layers, target configuration and
runtime compatibility fingerprint. The baseline was the exact parent of the fix:

| Variant | Source commit | `engine-client` SHA-256 |
| --- | --- | --- |
| Before | `46b04eadc3ca6a3451c9e7036e0e15148209cc20` | `e4ebe747c2dbde02ca4116996d3b4b42d35e5030253977f6be6fc72f88826849` |
| After | `cc65a75efa7acdfaba59ada3b43ae952825fce4d` | `596f0ac822befade6182e94aa042fac9f4ddc9885861de1e81e8b6859ff5278f` |

The after binary is the deployed build. Rebuilding it after collecting the parent
produced the identical SHA-256. The installed client was never replaced by the baseline.

After this collection, the branch incorporated the rendering optimizations from
PR #146 and a review fix preventing stream mixing across solids. The tables
retain the original binary comparison; they are not measurements of that later
combined build.

The existing `benchmark-clock.sh` drove the production simulation, scene construction
and CPU raster preparation at 1024×768, seed 7, fixed 08:08, with changing
seconds/colon. Each process warmed up for five simulated seconds, then reset the
scenario to the same seed before measurement. Every measured second contained exactly 60
fixed updates and 60 rendered/prepared frames, executed without display pacing.

| Fixture | Measured seconds/process | Coverage |
| --- | ---: | --- |
| Idle | 15 | Dry clock control |
| Heavy Rain | 45 | 20 s raining, 20 s draining, 2 s clearing, 2 s cooldown, 1 s idle |
| Meltdown | 46 | Four complete 11.5 s cycles, each including melt, drain, reform, cooldown and idle |

Both raster scales were tested: 1× is 1024×768; the cabinet's saved 2× setting renders
2048×1536 internally. Each case/scale had three paired independent-process repeats,
alternating build order before/after, after/before, before/after. The complete matrix
contains 36 runs and 76,320 measured frames. CSV metadata, dimensions, frame counts,
event counts and binary hashes were checked before aggregation.

The existing automatic match was paused and its kiosk process temporarily suspended with
`SIGSTOP` for the measured matrix. An exit trap sent `SIGCONT` afterward. This preserves
the match and saved settings while preventing background kiosk rasterization. A
preliminary attempt with only the game paused was excluded: the host still constructs
and rasterizes frames while paused. Its reports remain separate from the final dataset.

The final matrix ran from 14:43:42 to 15:09:07 UTC. The suspended kiosk's user/system
CPU counters were identical before and after the matrix, confirming that it did no
competing work. The governor remained `ondemand`. Run-endpoint temperatures ranged from
**70.6–74.0°C** and frequency snapshots from **0.9–1.5 GHz**. These snapshots do not
establish the frequency throughout a run or rule out transient throttling. Alternating
order and the stable idle control help interpret the comparison; very small differences
should still be treated cautiously.

## Results

Whole-cycle means include event creation, active water, drainage, cleanup and idle.
Tables report median per-run mean milliseconds per frame. Paired deltas are the median
of the three after-minus-before comparisons; their ranges show all three observed
differences, not confidence intervals.

| Fixture | Scale | Before (ms) | After (ms) | Paired increase | Range of paired increases |
| --- | ---: | ---: | ---: | ---: | ---: |
| Idle | 1× | 8.127 | 8.113 | -0.014 ms (-0.2%) | -0.016 to -0.006 ms |
| Idle | 2× | 27.230 | 27.283 | +0.036 ms (+0.1%) | +0.006 to +0.055 ms |
| Heavy Rain | 1× | 9.762 | 9.993 | +0.231 ms (+2.4%) | +0.228 to +0.238 ms |
| Heavy Rain | 2× | 29.680 | 30.096 | +0.425 ms (+1.4%) | +0.403 to +0.437 ms |
| Meltdown | 1× | 6.911 | 6.960 | +0.046 ms (+0.7%) | +0.033 to +0.063 ms |
| Meltdown | 2× | 22.190 | 22.423 | +0.225 ms (+1.0%) | +0.168 to +0.233 ms |

Breakdown of the same whole-cycle samples:

| Fixture | Scale | Simulation before → after | Scene construction before → after | Raster preparation before → after |
| --- | ---: | ---: | ---: | ---: |
| Heavy Rain | 1× | 0.477 → 0.574 ms | 0.270 → 0.300 ms | 9.010 → 9.111 ms |
| Heavy Rain | 2× | 0.472 → 0.575 ms | 0.271 → 0.302 ms | 28.929 → 29.215 ms |
| Meltdown | 1× | 0.117 → 0.140 ms | 0.182 → 0.203 ms | 6.606 → 6.609 ms |
| Meltdown | 2× | 0.120 → 0.143 ms | 0.186 → 0.208 ms | 21.877 → 22.066 ms |

The simulation interval includes Clock actions, water and any rigid-body work; it is not
an isolated water-function timer. At 2×, Rain's median paired simulation increase is
0.105 ms and scene construction adds 0.029 ms. Meltdown adds about 0.024 ms and 0.022 ms
respectively. The remaining measured difference is mostly in raster preparation of the
changed scene.

Rain's active phases show that cleanup/idle is not concealing a much larger drain-phase
cost. The table uses the reported one-second rows spanning raining (1–20) and drainage
(21–40):

| Rain phase | Scale | Simulation before → after | Scene construction before → after | Total paired increase |
| --- | ---: | ---: | ---: | ---: |
| Seconds 1–20 | 1× | 0.562 → 0.665 ms | 0.296 → 0.342 ms | +0.255 ms |
| Seconds 21–40 | 1× | 0.469 → 0.582 ms | 0.265 → 0.283 ms | +0.240 ms |
| Seconds 1–20 | 2× | 0.557 → 0.664 ms | 0.297 → 0.343 ms | +0.481 ms |
| Seconds 21–40 | 2× | 0.464 → 0.585 ms | 0.267 → 0.283 ms | +0.427 ms |

For Rain at 2×, each process's worst one-second-row p95 was 32.235–32.354 ms before and
32.974–33.151 ms after. These are ranges of per-process worst row percentiles, **not** a
combined whole-run p95. Peak scene-item counts were 1,750 → 1,752 for Rain and 1,400 →
1,400 for Meltdown. Peak body/collider counts were unchanged. These counters are not a
heap-allocation or memory-use profile.

## Interpretation and limits

The added work is bounded swept collision against the two finite floor panels and
clipping of rendered ribbons against those same panels. The comparison includes both
algorithmic cost and the changed water paths/drawing produced by the fix. It does not
isolate individual collision functions or allocator costs.

These are headless frame-work timings. They exclude Slint/KMS drawing, image upload,
page flips and vsync, and do not measure displayed FPS. Temperature/frequency samples
are run endpoints rather than continuous traces. Three repeats at one seed and one
cabinet aspect are a practical before/after check, not a worst-case bound for eight
arbitrary solids, portrait layouts or future splash particles. Timing results are
observations, not CI pass/fail thresholds.

Keep this fix and use the same paired workloads when adding drain splashes. A future
water optimization should start with attribution inside simulation and ribbon clipping;
the much larger existing raster cost is a separate rendering investigation. Neither
raster scale nor water budgets were changed for this work.

## Reproduce and inspect

[The retained dataset](data/water-drain-collision-20261002.tar.gz) contains all 36 raw
CSV reports, per-run environment snapshots and logs, binary/build metadata, exact run
order, the collector and analysis scripts, and the calculated tables. It excludes
binaries and private saved settings. Archive SHA-256:
`e10a79e3b1d38b8b9abd41ba8c372bea09ba25fb1d172a1d746a6c3e56c347e9`.

```sh
mkdir -p /tmp/drain-wall-results
tar -xzf docs/data/water-drain-collision-20261002.tar.gz -C /tmp/drain-wall-results
cd /tmp/drain-wall-results/water-drain-collision-20261002
sha256sum -c SHA256SUMS
python3 analyze.py
python3 tables.py
```

The Python scripts use only the standard library (Python 3.10+). Detailed build metadata
records Rust/Cargo versions, release flags, Yocto layer commits and the runtime
fingerprint. The complete local collection, including the excluded preliminary attempt,
is under `target/clock-benchmarks/drain-collision-20261002/`.

Build the two revisions with the same Pi release configuration and retain them as
separate executables. For one full Rain run:

```sh
bash benchmark-clock.sh --client /path/to/engine-client \
  --output /tmp/drain-wall-profile --renderer raster --scales 2 --cases rain \
  --seed 7 --repeats 1 --seconds 45 --warmup 5 --width 1024 --height 768
```

Repeat for both builds at scales 1 and 2, with Idle at 15 seconds and Meltdown at 46
seconds. Alternate build order across pairs. Pause the kiosk before temporarily
suspending it, verify that its service has no watchdog, and guarantee `SIGCONT` on exit;
the recorded runner does this for `spacewars-kiosk.service`. A game pause alone does not
isolate the rendering workload. Do not collect screenshots or run other device workloads
during measurements.

After collection, the original automatic Spacewars session resumed and progressed to its
next match. The service remained active with zero restarts. The installed/running binary
hashes still match the after build, and the saved settings file is byte-identical to the
pre-profile snapshot. This follow-up changes documentation and retained evidence only;
it does not change runtime code or require another deployment.
