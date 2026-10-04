# Clock drain splash cost on Picade

Paired measurements for the varied Lively drain splashes in
[issue #102](https://github.com/aortez/space-wars/issues/102), collected on
October 2, 2026. The [water design](design/water.md#drain-splashes-102-october-2-2026)
records the visual experiment, selected treatment and simulation limits.

The full-event comparison shows **no material CPU frame-work regression**.
Median paired changes range from -0.5% to +0.3% across the water workloads.
At the cabinet's saved 2× scale, Heavy Rain changes by +0.025 ms/frame (+0.1%).
Idle variation is of comparable size, so the small negative differences are
not evidence of a speedup. Keep the selected treatment without an optimization
detour.

## Workload and comparison

Both variants use commit `e84a08202c2e8d9a36aacd91a1a0b8b25c14f3d5`.
The control changes only the three Clock water constructors to pass
`splash: None`; the retained patch specifies that change exactly. The enabled
executable is the manually accepted build already installed on
`sw-picade.local`. Neither the older drain-collision binary nor an older main
revision is used as the baseline.

| Variant | `engine-client` SHA-256 |
| --- | --- |
| Splashes off | `eef47af5af0c1ce6d5ed6b7589272794f892b33d5ba4331e234fe358bd85d0ee` |
| Varied Lively | `d0c4b37af6ab618aa65f886c1061cbb4446858902856c8581957104d760bc7c5` |

These are matching aarch64 Yocto release builds: Rust 1.94.0, optimization level
3, fat LTO, one codegen unit, identical pinned layers and runtime compatibility.
The source was restored after making the disabled build. A verification rebuild
with the original embedded Git-revision label reproduced both installed binary
hashes exactly; without that override, the Info screen's new commit label
changes the client hash. The retained metadata records both source commit and
embedded labels. The control executable is used only by the headless benchmark;
it does not replace the installed app.

The existing `benchmark-clock.sh` runs the production Clock simulation, scene
construction and CPU raster preparation at 1024×768, seed 7, Classic 24-hour
08:08, with advancing seconds/colon. Each process warms up for five simulated
seconds and then resets to the same seed for measurement. Each measured second
has exactly 60 fixed updates and 60 prepared frames, without display pacing.

| Fixture | Measured seconds/process | Coverage |
| --- | ---: | --- |
| Idle | 15 | Dry Clock control |
| Heavy Rain | 45 | Raining, drainage, clearing, cooldown and idle |
| Meltdown | 46 | Four complete cycles, including melt, drain, reform, cooldown and idle |

Both 1× (1024×768) and the cabinet's saved 2× (2048×1536 internally) are tested.
Three independent-process pairs per fixture/scale alternate off/on, on/off,
off/on. This is 36 runs and 76,320 measured frames. The analyzer checks binary
hashes, report metadata, dimensions, frame/update counts and matching active
event frame counts before comparing results.

The running Clock was paused and its kiosk process suspended with `SIGSTOP`
during measurement, with a `SIGCONT` exit trap. A normal gameplay pause alone
still runs the renderer and would compete with these measurements. Saved
settings and the installed binaries are preserved.

The matrix ran from 02:22:34 to 02:42:16 UTC on October 3 (October 2 locally).
The kiosk's CPU counters stayed unchanged while suspended. Endpoint temperatures
were 74.0–78.4°C, with the `ondemand` governor unchanged and frequency snapshots
from 0.9 to 1.5 GHz. These snapshots do not establish the frequency throughout
each run or exclude transient throttling.

## Results

Whole-cycle values are median per-run mean milliseconds per frame, including
event construction, active water, drainage and cleanup. Paired changes are
medians of the three on-minus-off comparisons; they need not equal the
difference between the two independently calculated medians. Ranges contain
the three observed paired differences, not confidence intervals.

| Fixture | Scale | Off (ms) | Lively (ms) | Paired change | Range of paired changes |
| --- | ---: | ---: | ---: | ---: | ---: |
| Idle | 1× | 6.371 | 6.336 | -0.034 ms (-0.5%) | -0.035 to -0.003 ms |
| Idle | 2× | 20.148 | 20.052 | -0.109 ms (-0.5%) | -0.132 to +0.059 ms |
| Heavy Rain | 1× | 7.961 | 7.943 | -0.020 ms (-0.2%) | -0.031 to -0.010 ms |
| Heavy Rain | 2× | 22.577 | 22.583 | +0.025 ms (+0.1%) | -0.091 to +0.185 ms |
| Meltdown | 1× | 5.653 | 5.669 | +0.015 ms (+0.3%) | -0.003 to +0.018 ms |
| Meltdown | 2× | 17.436 | 17.360 | -0.094 ms (-0.5%) | -0.175 to +0.005 ms |

| Fixture | Scale | Simulation off → Lively | Scene construction off → Lively | Raster preparation off → Lively |
| --- | ---: | ---: | ---: | ---: |
| Heavy Rain | 1× | 0.587 → 0.584 ms | 0.301 → 0.302 ms | 7.069 → 7.050 ms |
| Heavy Rain | 2× | 0.583 → 0.574 ms | 0.302 → 0.303 ms | 21.684 → 21.700 ms |
| Meltdown | 1× | 0.143 → 0.146 ms | 0.201 → 0.202 ms | 5.301 → 5.314 ms |
| Meltdown | 2× | 0.147 → 0.148 ms | 0.206 → 0.207 ms | 17.075 → 16.998 ms |

| Rain phase | Scale | Simulation off → Lively | Scene construction off → Lively | Total paired change |
| --- | ---: | ---: | ---: | ---: |
| Seconds 1–20 | 1× | 0.680 → 0.676 ms | 0.347 → 0.340 ms | -0.027 ms |
| Seconds 21–40 | 1× | 0.594 → 0.592 ms | 0.279 → 0.289 ms | -0.010 ms |
| Seconds 1–20 | 2× | 0.672 → 0.664 ms | 0.348 → 0.341 ms | +0.030 ms |
| Seconds 21–40 | 2× | 0.592 → 0.582 ms | 0.282 → 0.289 ms | +0.027 ms |

Meltdown's simulation cost rises consistently by only 0.0015–0.0022 ms/frame
in the median paired comparison. Rain's simulation changes are similarly small
and negative. The interval includes all Clock simulation work, not just water.
Most whole-frame work remains raster preparation.

Rain's peak scene-item count changes from 1,752 to 1,760; Meltdown's changes
from 1,400 to 1,402. Peak body/collider counts stay unchanged. Each Rain run has
2,519 active-event frames, and the four Meltdown cycles have 2,036, identically
in both variants. These counters are not a heap-allocation profile or splash
counter; the separate conservation, resource and live-capture tests cover those
properties.

At 2×, each Rain process's worst one-second-row p95 is 25.079–25.343 ms off
versus 25.113–25.443 ms with Lively. Meltdown's corresponding ranges are
22.718–22.827 ms and 22.695–22.782 ms. These are ranges of the largest row
percentiles, not combined whole-run p95 values.

## Interpretation and limits

These measurements include the cost of generating spray and the changed water
paths and drawing that follow. They are not an isolated RNG/function benchmark.
The splash-disabled build retains the engine's new state fields and optional
code paths; this measures the cost of enabling the selected Clock preset.

The runner excludes Slint/KMS drawing, image upload, page flips and vsync, so
the results are CPU frame-work timings, not displayed FPS. Environmental
readings are run endpoints, not continuous frequency/throttling telemetry.
Three pairs at one seed and cabinet aspect are a practical comparison, not a
worst-case bound. Timing is observational rather than a CI pass/fail threshold.

## Reproduce and inspect

The [retained dataset](data/water-drain-splash-picade-20261002.tar.gz) contains
all raw CSV reports, per-run environment snapshots, exact order, binary/build
metadata, the splash-off patch, pinned Yocto configuration, collector and
analysis scripts. Binaries and private saved settings are excluded.
Archive SHA-256:
`613228fd61f2d52ceea588d9758ca327c7f979d7189e6ef7e7d8cc50004ac807`.

```sh
mkdir -p /tmp/drain-splash-results
tar -xzf docs/data/water-drain-splash-picade-20261002.tar.gz -C /tmp/drain-splash-results
cd /tmp/drain-splash-results/water-drain-splash-picade-20261002
sha256sum -c SHA256SUMS
python3 analyze.py
python3 tables.py
```

The scripts use only the Python 3.10+ standard library. The original local
collection, including binaries and build logs, is under
`target/drain-splash/pi-profile-20261002/`.

For an individual complete Rain run against either executable:

```sh
bash benchmark-clock.sh --client /path/to/engine-client \
  --output /tmp/drain-splash-profile --renderer raster --scales 2 --cases rain \
  --seed 7 --repeats 1 --seconds 45 --warmup 5 --width 1024 --height 768
```

Repeat for both builds at scales 1 and 2, with Idle at 15 seconds and Meltdown
at 46 seconds. Alternate build order. Remove competing kiosk work for the
matrix, verify the service has no watchdog before suspending it, and guarantee
resumption on exit. Do not collect screenshots or run other device workloads
during measurement. The retained `run-paired.sh` implements that procedure.

After collection, Clock was running unpaused in the original kiosk process,
with zero service restarts. Installed client/CLI hashes and the saved settings
were unchanged. This follow-up adds measurements and documentation; the
manually accepted deployment remains in place.
