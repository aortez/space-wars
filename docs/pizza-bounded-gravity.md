# Pizza: bounded gravity inside overlapping balls

This continues the cascade investigation in PR #505 under #59, #354 and #359.
Normal Pizza gravity now treats each ball as a uniform sphere, so a ball whose
center lies inside another ball feels a bounded pull instead of a nearly
singular point-mass kick. Desktop evidence shows that this removes the largest
CCD bursts without a consistent change in cascades. The change alters
trajectories; it is not a fingerprint-preserving optimization. Paired production
captures on the Pi 4 reproduce the desktop trajectories and show the CCD stall
gone there too. They do not show better frame pacing for seed 7: its bounded
cascade is heavier, and contact-bound updates still stall for about 100 ms.

## Cause of the largest cascade stall

The slowest update in all 15 retained ordinary gravity-On runs of
[the reference comparison](pizza-reference-comparison.md#worst-update-gravity-between-overlapping-fragments)
is tick 285: 96.6–102.0 ms in the Pi headless runs, 99.3–108.4 ms in the
production GPU runs and 14.0–14.3 ms on the desktop, mostly CCD. At the start of
that update two overlapping fragments have centers 2.6×10⁻⁶ apart. Pizza applied
point-mass gravity (mass r², G 0.0005 per update, softening 10⁻⁶; allan.pizza
uses the same G and softening), which kicked them by about 607 and 385 units/s
in one update. No other ball moved faster than 0.67 units/s. CCD then swept both
bodies across most of the playfield: 14,356 ball/ball shape casts against 216
one update earlier.

Fragments are created overlapping, so a center inside another ball is routine:
965 of the 1,200 seed-7 updates contain such a pair.

## Change

- `engine-gravity` already had a `UniformSphere` source shape: inverse-square
  outside its radius and a linear inward pull inside. Shaped sources were always
  summed directly, which is quadratic. Hierarchical spherical sources now join
  the Barnes–Hut tree. A node is replaced by its point aggregate only for targets
  outside every sphere it contains; the solver records each subtree's largest
  radius and compares it with the target's distance from the node bounds.
- Point-only trees skip all sphere bookkeeping and compile without the check.
  Their results are bit-identical, and their speed is unchanged within noise
  (below).
- Normal Pizza play gives every ball a sphere source of its own radius. The
  Classic and Rapier benchmarks keep point-mass gravity, so their legacy
  workload is unchanged.

The force on a ball changes only while its center lies inside another ball.
Each such pull is then at most G·m/r² = G = 0.0005 units/s per update, because
Pizza mass is r². The shape uses only the source's radius, so two overlapping
balls of different sizes receive unequal opposing pulls. Both are bounded by G,
so the momentum error is small; walls and explosions already change total
momentum.

## Desktop evidence

Rust 1.89.0 release builds (fat LTO, one codegen unit) of the baseline
`7839926` and the change run on a Ryzen 7 9800X3D. Each ordinary run lasts
1,200 updates at 1024×768 with a 500-ball budget; gravity-On runs trigger the
directional event at tick 120. Seeds 1–20 run with both laws. Counts come from
`ccd-diagnostics` builds; uninstrumented builds must reproduce their ordered
state fingerprints after every update, and all 50 such comparisons match.

### CCD work, gravity On, 20 seeds

Per-run values, shown as mean / median / worst seed:

| Per run | Point mass | Bounded |
| --- | ---: | ---: |
| Most shape casts in one update | 7,452 / 5,264 / 39,667 | 715 / 583 / 2,279 |
| Total shape casts | 27,665 / 22,527 / 62,768 | 9,275 / 9,473 / 17,327 |
| Most resweep candidates in one update | 13,296 / 10,271 / 46,185 | 1,071 / 865 / 4,166 |
| Updates above 1,000 shape casts | 3.1 / 2.5 / 7 | 0.2 / 0 / 2 |
| TOI prefilter calls (millions) | 2.25 / 2.30 / 2.71 | 2.19 / 2.32 / 2.72 |

The cheap relative-motion prefilter still sees about as many candidates; the
expensive survivors and resweeps are what disappear.

### Cascade behavior, 20 seeds

Means over seeds:

| | Point, field On | Bounded, field On | Point, field Off | Bounded, field Off |
| --- | ---: | ---: | ---: | ---: |
| Explosions | 43,106 | 42,048 | 3,061 | 3,265 |
| Fragments created | 43,513 | 42,452 | 3,223 | 3,415 |
| Updates with an explosion | 930.5 | 928.1 | 285.9 | 283.0 |
| Mean live balls | 339.0 | 339.8 | 87.8 | 85.1 |
| Updates at the 500 budget | 482.4 | 513.2 | 5.5 | 7.7 |
| Mean contacts | 585.2 | 579.4 | 100.0 | 99.5 |
| Balls beyond the physical walls | 0 | 0 | 0 | 0 |

Per-seed explosion ratios (bounded/point) have a geometric mean of 0.963 with
the field On (0.677–1.216; 10 seeds higher, 10 lower) and 1.012 with it Off
(0.150–8.861; 9 higher, 11 lower). Small field-Off cascades are chaotic under
both laws. Field-On explosions by fragment generation total 378 / 4,001 /
26,319 / 831,425 with point mass and 372 / 3,934 / 25,993 / 810,664 bounded.
No consistent shift appears in cascade size, depth or population.

### Timing

Uninstrumented runs alternate order and are pinned to one CPU. Other builds
shared the host, so these timings are indicative only. Each seed evolves
differently, so the cross-seed mean is the expected cost; ordinary values are
milliseconds per update.

| Case | Point mass | Bounded |
| --- | ---: | ---: |
| Gravity On, 20 seeds: mean update | 0.916 | 0.919 |
| Gravity On, 20 seeds: gravity phase | 0.166 | 0.187 |
| Gravity On, 20 seeds: p99, median of seeds | 3.39 | 3.14 |
| Gravity On, 20 seeds: worst update, mean of seeds | 10.88 | 8.42 |
| Gravity On, 20 seeds: worst update, worst seed | 28.37 | 17.26 |
| Seed 7, five repeats: mean / p99 / worst | 0.938 / 3.29 / 14.80 | 1.106 / 3.57 / 8.71 |

The per-seed ratio of mean update time has a geometric mean of 0.993
(0.633–1.258). Seed 7's higher mean comes from its different cascade, not from
the law. Across seeds, gravity takes 0.021 ms more per update and physics
0.017 ms less, 0.013 ms of it in CCD.

The 240-update mixed fixtures keep nearly the same cascades, so they isolate the
gravity cost. Mean update (gravity phase) in milliseconds over five repeats:

| Starting balls | Point mass | Bounded |
| --- | ---: | ---: |
| 500 | 0.345 (0.181) | 0.375 (0.205) |
| 1,000 | 0.878 (0.433) | 0.926 (0.483) |
| 2,500 | 2.766 (1.158) | 2.873 (1.278) |
| 5,000 | 6.702 (2.114) | 6.910 (2.366) |

The spheres cost about 10–13% more gravity time. Leaf interactions evaluate the
bounded interior distance, and the guard checks each candidate node and opens
slightly more of them.

### Other gravity users

Spacewars and the labs use only point sources. The repository gravity benchmark
(`crates/engine-gravity/examples/gravity_benchmark.rs`, eight point-only cases)
differs by −1.2% to +1.4% from the baseline with identical exact-source,
approximation and error counts. Two runs of the identical baseline binary differ
by −1.7% to +1.3%. The deterministic `engine-agent` navigation-v1 and strategy-v1
baselines match.

## Production Pi 4 check

Both clients use the production Yocto Rust 1.94 release/fat-LTO `pi-kiosk-gpu`
configuration. The point-mass client is the `9170d99` build from
[the reference comparison](pizza-reference-comparison.md#production-gpu-check-after-the-optimization);
`7839926` changed only documentation. The bounded client, SHA-256
`d9a7cc85abd63d651504507b96a087d49a65c1f213a30695894795bf79370ba6`, was built
from `7839926` plus this change. A second build from the frozen tree recompiled
it byte for byte. The control CLI is unchanged.

The Pi 4/Picade runs the unchanged `tools/pizza-live-capture.sh` at 1024×768,
60 Hz: seed 7, a 500-ball budget and 1,200 ordinary updates. Captures alternate
the two clients, field On twice and then Off, 30 s apart. The installed image
and settings are untouched, and the kiosk is restored after each capture. All
six runs reproduce the desktop seed-7 fingerprints of the same law at every
update, so the point-mass runs repeat the trajectory of the retained production
runs.

The device ran warmer than in the earlier batch: pre-export samples span
68.7–75.0°C against 65.2–68.7°C. Firmware throttling flags stay zero in every
sample, and the point-mass mean updates are within 1% of the earlier batch
(11.24–11.26 ms against 11.12–11.13 ms).

Selection follows the reference comparison: only CPU intervals and complete KMS
windows that end before JSON export count, and flip quantiles pool
within-window intervals.

| Case | UI core mean | Longest ≥95% | Captured flip intervals | Flip p50 / p95 / p99 / max |
| --- | ---: | ---: | ---: | ---: |
| Point mass, Off | 31.9% | 0 s | 955 | 16.7 / 16.7 / 16.7 / 200.0 ms |
| Bounded, Off | 27.4% | 0 s | 956 | 16.7 / 16.7 / 16.7 / 83.3 ms |
| Point mass, On, repeat 1 | 81.4% | 10.26 s | 463 | 16.7 / 83.3 / 116.7 / 250.0 ms |
| Point mass, On, repeat 2 | 81.6% | 10.36 s | 464 | 16.7 / 83.3 / 116.7 / 266.6 ms |
| Bounded, On, repeat 1 | 85.3% | 14.04 s | 477 | 16.7 / 100.0 / 166.7 / 183.3 ms |
| Bounded, On, repeat 2 | 85.6% | 15.09 s | 474 | 16.7 / 100.0 / 150.0 / 216.6 ms |

Update work, with times in milliseconds; ranges span both repeats:

| Case | Mean update | Update p95 | Slowest update | Most CCD in one update | Mean contacts | Mean live balls |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Point mass, On | 11.24–11.26 | 20.7–21.3 | 105.5–108.5 (tick 285) | 73.9–80.1 | 613.2 | 361.4 |
| Bounded, On | 12.98–13.02 | 25.0–25.2 | 102.4–104.3 (tick 253) | 11.5 | 718.3 | 381.9 |
| Point mass, Off | 2.20 | 6.8 | 99.1 (tick 731) | 10.3 | 102.2 | 88.3 |
| Bounded, Off | 1.51 | 3.0 | 58.2 (tick 730) | 4.0 | 71.5 | 70.3 |

The CCD stall is gone on the device as well, but seed 7's bounded trajectory is
heavier. It carries 17% more contacts and 6% more live balls, so physics takes
17% longer (8.75 to 10.23 ms) and the mean update 15–16% longer. Gravity itself
takes 2.01–2.04 ms against 1.85–1.87 ms. The desktop runs of seed 7 show the
same increase (0.94 to 1.11 ms), while the 20-seed desktop mean is unchanged, so
the increase comes from this seed's cascade rather than being the expected cost.
The capture script fixes seed 7, so the device measures only this trajectory.

Bounded seed 7's slowest update, tick 253, is contact-bound: 7,990 contacts,
31–33 ms of narrow phase and 34 ms of solver, but only 7.6–7.7 ms of CCD. The
contact-bound updates here cost the Pi about 12 times their desktop time (tick
253 takes 8.27 ms on the desktop), and the CCD-bound tick 285 about 7.6 times
(13.97 ms). Seed 7's slowest update therefore falls by 41% on the desktop but by
only about 3% on the Pi; desktop worst-update gains overstate the device gain.

With the field On, the bounded runs keep the UI core busier (85% against 81%)
and saturated for longer (14–15 s against 10.3–10.4 s). Their flip p95 and p99
are worse, although their longest captured flip interval is shorter. These
intervals mostly reflect catch-up: during the event, the status reports up to
3.1 updates per rendered frame with point mass and 3.5–3.6 with bounded gravity,
all on the UI thread. With the
field Off, the bounded cascade happens to be smaller (at most 357 live balls
against 500), so that run is lighter; Off cascades are chaotic under both laws.

## Reproduction

Copy each binary after building it: normal and diagnostic builds share one
output path.

```sh
git worktree add ../space-wars-gravity-baseline 7839926
(cd ../space-wars-gravity-baseline &&
  cargo +1.89.0 build --locked --release -p scenario-pizza --example capacity_lab &&
  cp target/release/examples/capacity_lab /tmp/pizza-point &&
  cargo +1.89.0 build --locked --release -p scenario-pizza --example capacity_lab \
    --features ccd-diagnostics &&
  cp target/release/examples/capacity_lab /tmp/pizza-point-diag)
cargo +1.89.0 build --locked --release -p scenario-pizza --example capacity_lab
cp target/release/examples/capacity_lab /tmp/pizza-bounded
cargo +1.89.0 build --locked --release -p scenario-pizza --example capacity_lab \
  --features ccd-diagnostics
cp target/release/examples/capacity_lab /tmp/pizza-bounded-diag
bash tools/pizza-gravity-runs.sh /tmp/pizza-point /tmp/pizza-bounded \
  /tmp/pizza-point-diag /tmp/pizza-bounded-diag /tmp/pizza-gravity
python3 tools/pizza-gravity-summary.py extract /tmp/pizza-gravity /tmp/pizza-gravity-extracts
python3 tools/pizza-gravity-summary.py summarize /tmp/pizza-gravity-extracts
```

The runner pins timing runs to CPU 6 when `taskset` exists (`PIZZA_CPU`
overrides it). The summary rejects any instrumented/uninstrumented or repeated
run whose fingerprints diverge, and any device run that does not replay the
desktop run of its law.

For the device runs, copy `tools/pizza-live-capture.sh` to the Pi, and each
client with `spacewars-cli` to `/tmp/spacewars-pizza-gravity-point` or
`/tmp/spacewars-pizza-gravity-bounded`. As the administrator, run
`sudo bash pizza-live-capture.sh ROOT RUN 500 500 1200 on|off ordinary 30` for
`on-r1`, `on-r2` and `off-r1`, alternating clients. Copy each completed
`ROOT/RUN` back as `point-RUN` or `bounded-RUN` in one directory, then pass that
directory to `summarize` with `--pi`.

The [archive](data/pizza-gravity-20261018-raw.tar.gz) retains per-update CSV
extracts and run metadata for all 170 reports, the complete seed-7 reports, the
point-only benchmark output and the six Pi captures without their temporary
config directories. Running `summarize` on its `extracts` directory with `--pi`
on its `pi` directory reproduces the
[summary](data/pizza-gravity-20261018-summary.json) byte for byte; without
`--pi` it reproduces the desktop part alone. The
[manifest](data/pizza-gravity-20261018-manifest.json) records binary, archive
and source-diff checksums and the device build provenance. The baseline
capacity binaries are byte-identical to the retained reference-comparison
builds.

## Verification

- Engine gravity: 18 tests pass. Two new tree tests fail without the guard: a
  target inside a heavy sphere must not see that sphere's point aggregate, and
  an overlapping Pizza-like cluster must match the exact solver. The routing
  test now expects hierarchical spheres in the tree.
- Pizza: 41 tests pass. The new test fails if normal play uses point gravity and
  confirms that benchmarks keep it.
- Other gravity users: Spacewars (619 passed, 1 ignored), its two example suites
  and three lab packages pass in the CI profile.
- Client: 11 Pizza tests pass; explicit captures and display tests are left to
  their usual runs.
- Tools: all 1,055 `tools/tests` tests pass, including seven new summary tests.
  `tools/pizza-profile-summary.py` now exposes its trace parsing and gives
  byte-identical output on all nine retained Pi captures. Scoped strict Clippy
  and formatting pass for both changed crates.

## Remaining qualification

This removes the CCD stall, not the Pi 4 frame-budget problem. With the field
On, both laws keep the UI core above 80% busy, flip p95 is 83–100 ms, and
contact-bound updates still take about 100 ms on the device. Frame scheduling
and narrow-phase and solver cost are the next targets; the
[simulation worker](pizza-simulation-worker.md) addresses frame scheduling. The device comparison
covers seed 7 only; a multi-seed device comparison needs a seed argument in the
capture script. Pi 5 remains offline. The capacity fixture screenshots and
[functional counts](pizza-capacity.md#local-functional-evidence) now record
bounded gravity: the 800×480 mixed 5,000 fixture has 3,471 bodies after 60
updates instead of 3,434.
