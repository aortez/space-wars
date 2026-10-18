# Pizza: common-fixture comparison and lazy CCD contact lookup

This continues PR #505 / #354; the measured optimization is #509 under #59.
It does not qualify the 5,000-ball ceiling or
resolve the earlier production GPU failures in #359. The saved default is 500.
The [planned performance work](#planned-performance-work) records the next
experiments and the decisions still needed before changing physics behavior.

## What the desktop screenshot establishes

The supplied allan.pizza screenshot shows 3,429 balls, 2,083 particles and a
displayed 101 FPS in Chrome on the user's desktop. That is a useful reference,
not a Pi measurement or a balls-per-second figure. The reference uses real
collisions and fragmentation. Its spatial circle solver, flat Barnes–Hut gravity
and instanced WebGL2 renderer are relevant architectural comparisons.

The following CPU experiment uses the available Ryzen 7 9800X3D host and Chrome
151.0.7922.173. It does not reproduce the screenshot's exact scene or measure
rendered FPS. The reference checkout is unmodified at
[`9fcab3e`](https://github.com/aortez/aortez.github.io/tree/9fcab3e85f52de1fb62a001b72b41d4c3509f1b0).
Rust is 1.89.0, release/fat LTO, one codegen unit. Both run sequentially on the
same host, with a 1024×768 playfield, seed-7 mixed fixture, matching live-ball
budget and 240 fixed updates of approximately 0.015 physical seconds each.

The Rust lab exports every initial ball's position, velocity, radius, health and
color. The browser importer rounds those values back to f32 before supplying
them to JavaScript, avoiding differences from decimal serialization. Both start
with healthy moving balls; neither forces deaths or makes balls invincible.
Spawning and events are off. Chrome runs one complete warm-up on a fresh world,
then three fresh measured repeats. Its normal `World.advance` is called directly
without drawing or the application animation loop. Rust has three fresh runs.
No CPU affinity, governor or host background workload is controlled.

### Common starting state, different evolved work

Mean update time across three runs, **before** the lazy-lookup optimization:

| Starting balls / budget | Rust | Chrome reference | Mean live balls, Rust / JS | Final live balls, Rust / JS | Deaths, Rust / JS |
| --- | ---: | ---: | ---: | ---: | ---: |
| 500 | 0.351 ms | 0.583 ms | 462 / 470 | 438 / 438 | 109 / 144 |
| 1,000 | 0.900 ms | 1.286 ms | 907 / 827 | 826 / 686 | 267 / 461 |
| 2,500 | 2.936 ms | 3.174 ms | 1,991 / 1,710 | 1,825 / 1,404 | 1,415 / 1,731 |
| 5,000 | 7.419 ms | 6.724 ms | 3,345 / 2,940 | 2,813 / 2,489 | 4,517 / 4,480 |

The reference also retains particles: 1,108 remain in its 5,000-start case.
Rust currently discards fragments below its ball threshold. The algorithms are
not interchangeable: direct circle separation versus an iterative rigid-body
solver/CCD, different damage, fragment size factors (JS 1.4, Rust 1.5), RNG,
numeric precision and ordering all affect subsequent work. These numbers show
that desktop simulation times are in a comparable range; they do **not** show
an equivalent-work speedup, sustained 5,000-body throughput, or a reason to
replace canonical physics. Repeated runs within each engine reproduce their
recorded work; cross-engine states are intentionally not asserted equal.

## Measured redundant work

Existing Rapier timers identify CCD motion clamping as a substantial cost:
about 1.99 ms/update in the initial desktop 5,000-start run. Opt-in per-query
diagnostics show an average 10,543.23 candidate TOI checks/update, of which
10,542.50 fail the existing relative-travel/thickness rejection. Nevertheless,
the old path traverses the contact graph for all 10,543.23 pairs first.

The original rejection is:

```text
relative_travel < shape_thickness_sum + max(existing_contact_distance, 0)
```

The added term is nonnegative. If travel is already below shape thickness, the
contact distance cannot rescue the pair. The private CCD helper now receives a
lazy distance lookup, rejects those pairs first, and performs the original
addition/comparison for survivors. First-impact searches, motion clamping and
resweeps use the same rule. Query order, accepted casts, solver settings, damage,
fragment admission and public API remain unchanged. There is no new cache.

With the optimization, actual contact lookups average **0.73 instead of
10,543.23** per update in the 5,000-start fixture, and **34.04 instead of
1,941.35** in the ordinary 500-budget gravity event. TOI call/rejection and shape
query counts remain unchanged. Instrumented runs reproduce the uninstrumented
state fingerprints. Per-query timers substantially perturb timing and are not
used for speedup claims; after this change contact-lookup time nests inside TOI
time, so those inclusive timers must not be added together.

## Paired optimization results

Separate baseline/optimized executables use the same compiler and settings.
Three repeats alternate baseline/optimized order (the second repeat reverses
it). Each mixed case runs 240 updates; the ordinary gravity event runs 1,200,
including recovery. Every update matches ordered state fingerprints,
population/birth/death counters, size/generation bands, event state and
contact/candidate counts. All sampled physical-wall escape counts are zero.

Desktop means:

| Case | Before | After | Whole-update reduction |
| --- | ---: | ---: | ---: |
| Mixed 500 | 0.359 ms | 0.352 ms | 2.2% |
| Mixed 1,000 | 0.916 ms | 0.886 ms | 3.2% |
| Mixed 2,500 | 2.981 ms | 2.794 ms | 6.3% |
| Mixed 5,000 | 8.522 ms | 7.475 ms | 12.3% |
| Ordinary 500, gravity On | 1.034 ms | 0.947 ms | 8.4% |

Host timing varies: the 5,000-case per-run means range 7.616–9.603 ms before and
6.832–7.837 ms after. Its CCD mean changes from 2.168 to 1.362 ms. The original
common-fixture batch and this paired batch are distinct; do not mix their
timings to calculate a gain. Small gains need device corroboration.

Pi 4 headless means (Rust 1.89 release, same fixtures, three interleaved pairs):

| Case | Before | After | Whole-update reduction | CCD before / after |
| --- | ---: | ---: | ---: | ---: |
| Mixed 500 | 3.603 ms | 3.563 ms | 1.1% | 0.500 / 0.463 ms |
| Mixed 1,000 | 10.202 ms | 9.865 ms | 3.3% | 1.614 / 1.292 ms |
| Mixed 2,500 | 37.390 ms | 35.509 ms | 5.0% | 6.352 / 4.516 ms |
| Mixed 5,000 | 105.277 ms | 97.338 ms | 7.5% | 19.548 / 11.549 ms |
| Ordinary 500, gravity On | 11.251 ms | 10.266 ms | 8.8% | 2.417 / 1.454 ms |

There are **6,480 matched before/after updates on each platform**, 12,960 total.
The Pi's installed kiosk was stopped during headless measurements and restored
afterward; installed binaries/settings were untouched. The Pi 4 Model B Rev 1.4
used its existing `ondemand` governor. Before/after run samples span
65.7–67.7°C with firmware throttling flags zero. These boundary samples do not
measure whole-run CPU utilization or exclude every intervening frequency change.
At 500 the timing ranges overlap; the small difference is not a strong claim.
The 5,000-case per-run means are 104.939–105.465 ms before and
97.132–97.479 ms after. All sampled escapes remain zero.

The earlier #505 capture and this batch have different timing distributions
even with matching state fingerprints. Only the interleaved pairs above support
the claimed gain. Do not subtract these headless values from older GPU results
or chain their percentages with the earlier indexed-damage experiment.

## Production GPU check after the optimization

Runtime source `9170d99451ecba6e8ace8e6c2a1517058f0c7fdd` was built with the
production Yocto Rust 1.94 release/fat-LTO `pi-kiosk-gpu` configuration. The
client SHA-256 is
`2051353849ac2916b4cdccd553c9735dcd195bc6d76619d98d8c3666bf36a103`.
Tracked source was clean and frozen throughout the build; its dirty provenance
flag comes from the four pre-existing unrelated untracked files.

The actual Pi 4/Picade runs at 1024×768, 60 Hz with the Femtovg KMS path. These
three sessions each capture 1,200 updates. Every recorded state/work field
matches the earlier `ff21581` production GPU trace, including both independent
gravity-On repeats. Screenshots are taken after timing. The installed kiosk is
restored after each temporary session; installed binaries/settings are unchanged.

Only CPU intervals and complete KMS windows that end before JSON export enter
this table. Quantiles pool within-window physical flip intervals without joining
gaps; coverage is bounded and not a complete session trace.

| Case | UI core mean | Longest ≥95% | Captured flip intervals | Flip p50 / p95 / p99 / max |
| --- | ---: | ---: | ---: | ---: |
| Ordinary Off | 31.5% | 0 s | 955 | 16.7 / 16.7 / 33.3 / 200.0 ms |
| Ordinary On, repeat 1 | 81.1% | 9.23 s | 467 | 16.7 / 66.7 / 100.0 / 216.7 ms |
| Ordinary On, repeat 2 | 81.3% | 9.36 s | 472 | 16.7 / 66.7 / 100.0 / 166.6 ms |

Mean normal update time is 2.18 ms Off and 11.12–11.13 ms On. Gravity-On update
p95 is 20.5–20.6 ms, with individual updates up to 99–108 ms. Pre-export
temperatures reach 68.7°C and firmware throttling flags remain zero. CPU means
use 100% = one core and weight by interval duration; small individual estimates
above 100% reflect separate proc reads, not extra core capacity.

The previous GPU batch reported 82–83% mean UI-core usage and longer stalls.
These are separate captures, so the reliable speedup claim remains the paired
headless experiment. The new GPU capture establishes that **the ordinary
gravity event still fails the frame-budget/headroom goal**. It does not qualify
5,000 or remove the need for further collision-processing work and Pi 5 testing.

[Updated device screenshot](screenshots/pizza-capacity/1024x768-pi4-gpu-after-gravity-ccd.png)
shows the scene after the first On capture; its exact tick/population is not
synchronized with a timing sample.

## Reproduction

Run the reference comparison against a clean checkout with its Playwright
dependency installed. The harness serves only JavaScript modules over loopback,
blocks other browser requests and does not modify that checkout:

```sh
cargo +1.89.0 build --locked --release -p scenario-pizza --example capacity_lab
target/release/examples/capacity_lab 5000 5000 240 7 1024 768 off mixed > /tmp/pizza-rust.json
node tools/pizza-reference-compare.mjs ../aortez.github.io /tmp/pizza-rust.json > /tmp/pizza-chrome.json
```

`PIZZA_CHROME` optionally selects the Chrome executable (default
`/usr/bin/google-chrome`). The script reports the reference commit, browser
version, host, fixture checksum and all per-step timings/work counts. It rejects
ordinary/event fixtures and query-instrumented Rust reports. The browser runs
are CPU-only and make no graphics-acceleration or FPS claim.

To inspect CCD work separately:

```sh
cargo +1.89.0 run --locked --release -p scenario-pizza --example capacity_lab \
  --features ccd-diagnostics -- 5000 5000 240 7 1024 768 off mixed > /tmp/pizza-ccd.json
```

The feature is off by default. Each report labels its scope; the regular
profiling samples now expose existing CCD, update and island timers as well.

Retained [raw reports](data/pizza-reference-20261018-raw.tar.gz),
[summary](data/pizza-reference-20261018-summary.json), and
[manifest/checksums](data/pizza-reference-20261018-manifest.json) identify both
headless executables and the production GPU pair. No binaries, credentials or
user settings are included. The summary's paired groups are reproducible with:

```sh
python3 tools/pizza-reference-summary.py /path/to/extracted/desktop --pi /path/to/extracted/pi
```

The archive also retains the exact temporary Pi headless runner and per-run
boundary temperature/frequency/throttling readings. Production runs use
`tools/pizza-live-capture.sh`; `tools/pizza-profile-summary.py` supplies the
conservative pre-export selection described in the earlier investigation.

## Verification and remaining qualification

All 203 engine-physics tests (including CCD diagnostics) and 40 Pizza tests pass.
The new shared-motion regression fails against the old CCD path because it
performs an unnecessary contact lookup, and passes with the optimization. It
checks both one and four CCD substeps; the existing suite covers fast impacts,
moving/compound ground, sensors, hooks, shape edits and snapshot continuation.
Scoped Pizza Clippy, formatting, JavaScript syntax and the three report-unit
tests pass. Strict engine-wide Clippy is blocked by existing warnings in plant,
spaceling and tank-route code; this change does not edit those unrelated areas.

This optimization preserves the recorded physics; it does not alter the
capacity, field force or fragmentation distribution to make the test easier.
Production rendered Pi 4 qualification was repeated on `9170d99` and still
exposes the stalls above. Pi 5 remains offline. PR #505 stays draft and
#354/#359 remain open.

## Planned performance work

Planning update: 2026-10-18. First make ordinary 500-budget cascades smooth with
CPU headroom on the Pi, then pursue larger populations. Keep 500 as the saved
default and 5,000 as an opt-in aspiration. Preserve the detailed chain reactions
in which large balls fragment and their descendants collide and explode.

The current Pi 4 production gravity event averages 11.12–11.13 ms per update,
with p95 around 20.6 ms and individual updates near 100 ms. The separate headless
experiment starting at 5,000 averages 97.34 ms while the population falls; it
does not establish sustained 5,000-ball throughput. Both CPU headroom and frame
stalls need improvement before PR #505 is ready.

### Experiment priorities

1. **Reduce collision bookkeeping.** Start with the worst recorded cascade
   updates. In the production gravity event, narrow-phase contact processing
   averages about 3.1 ms/update. The difference between total physics time and
   the raw engine step is about 1.1 ms; finer measurements must establish how
   much comes from event collection, metadata access and other adapter work.
   Look for repeated contact scans, duplicate event construction, unnecessary
   lookups and reusable scratch storage. Preserve contact ordering, transient
   CCD hits, resting contacts, damage and fragmentation. Use matching replay
   fingerprints to verify changes that should preserve results.

2. **Optimize the gravity tree.** Gravity averages about 1.9 ms/update in the
   production event and 24.6 ms in the separate large-population headless case.
   The engine already uses Barnes–Hut and reuses flat tree/work buffers. Measure
   construction and force traversal separately, then investigate memory layout,
   repeated calculations and traversal overhead. Changing approximation
   tolerance, precision or gravity update frequency would change trajectories;
   treat those as separate behavior experiments.

3. **Benchmark physics build options.** The pinned Rapier 0.34 build enables
   `enhanced-determinism` and excludes explicit SIMD. That version rejects
   enabling both together. Measure a separate SIMD build on the target hardware
   before considering a production change. Adoption depends on deciding whether
   Pizza needs identical results across platforms and understanding the impact
   on other scenarios sharing the dependency. Keep compiler versions and build
   settings matched within each comparison.

4. **Improve frame scheduling.** The host permits up to five fixed updates in
   one callback, then clears accumulated time if it reaches that limit. Join
   update timings to actual presentation intervals to determine how catch-up
   contributes to stalls. Evaluate scheduling changes and, if justified, a
   simulation worker with interpolated rendering. Track input latency, simulation
   progress and total process CPU use: smoother presentation alone does not
   establish cheaper physics. Any change to overload behavior or simulation
   timing needs an explicit design decision.

5. **Prototype a specialized Pizza physics backend.** If the preceding gains
   leave a large gap, compare a compact circle-and-wall engine inspired by
   allan.pizza with the canonical Rapier path. Rapier already specializes circle
   contact geometry; the hypothesis is that a smaller surrounding collision and
   response pipeline could save work. Retain gravity, damage and fragmentation
   rules for the experiment, and compare containment, collision stability,
   cascade behavior and CPU cost. Different solver trajectories are expected,
   so this requires behavioral validation rather than identical fingerprints.
   Keep it experimental until its benefits and integration costs are measured.

Start with collision bookkeeping, then gravity. Use the scheduling and build
experiments to assess their respective benefits before committing to a new
backend. Renderer and allocation rewrites have lower priority unless finer
profiling identifies a substantial cost there. No speedup is assumed for these
unmeasured experiments.

### Worst update: gravity between overlapping fragments

The slowest update in all 15 retained ordinary gravity-On runs is tick 285:
96.6–102.0 ms in the Pi headless runs, 99.3–108.4 ms in the production GPU runs
and 14.0–14.3 ms on the desktop. CCD takes 72–73% of it on the Pi and 82–83% on
the desktop; the deferred lookup leaves it essentially unchanged. The
instrumented run counts 14,356 ball/ball shape casts and 20,869 resweep
candidates in that update, against 216 and 2,490 one update earlier.

The cause is gravity rather than collision bookkeeping. At the start of the
update two overlapping fragments have centers 2.6×10⁻⁶ apart. Pizza's
point-mass law (mass r², G 0.0005, softening 10⁻⁶; allan.pizza uses the same G
and softening) gives them single-update velocity changes of about 607 and
385 units/s, while no other ball moves faster than 0.67. CCD sweeps both across
most of the playfield, both explode, and their fragments inherit up to
3 units/s. In this 1,200-update fixture, 33 updates have a largest kick above
1 unit/s, and all 8 updates with the most CCD shape casts coincide with or
immediately follow one. A ball center lies inside another ball in 965 of the
1,200 updates (66 pairs on average). Collision bookkeeping remains the larger
mean cost; this explains the largest single stall.

Normal play now bounds gravity inside overlapping balls; see
[bounded gravity](pizza-bounded-gravity.md). Over 20 desktop seeds of the
ordinary gravity-On fixture, the per-run maximum of shape casts in one update
averages 715 instead of 7,452 (worst seed 2,279 instead of 39,667). Cascades
show no consistent shift, and the mean update cost is unchanged. The change
alters trajectories. Paired production Pi 4 captures of seed 7 confirm that the
CCD stall is gone (at most 11.5 ms of CCD in one update instead of 74–80 ms).
That seed's bounded cascade is heavier, however, and its new slowest update, a
contact-bound one, still takes 102–104 ms.

### Validation and open decisions

- Retain repeatable ordinary On/Off scenes and mixed fixtures starting at 500,
  1,000, 2,500 and 5,000 balls. Report actual population over time, contacts,
  births, deaths and fragment generations so shrinking workloads cannot masquerade
  as higher throughput. Include sustained cascades and recovery.
- Compare interleaved baseline and candidate runs with the same build settings.
  For behavior-preserving changes, require matching state/work fingerprints.
  For alternative solvers or numerical settings, define behavioral checks before
  judging their performance.
- Validate with production rendering active on the Pi 4 at 1024×768/60 Hz.
  Track update and displayed-frame p50/p95/p99/max, duration-weighted process CPU,
  time near one fully occupied core, temperature and throttling. Measure any
  worker threads as part of total CPU consumption. Repeat on Pi 5 when available.
- Agree on numerical frame-stall and sustained-CPU acceptance limits before
  declaring qualification. The 16.7 ms display interval is a total frame budget;
  physics needs room for rendering and input. Exact headroom limits remain open.
- Keep changes to reproducibility, solver behavior, approximation quality or
  overload timing explicit. Do not reduce fragment populations or weaken the
  cascade workload to pass the performance check.
- Bounded gravity inside overlapping balls is implemented with spherical
  sources in the Barnes–Hut tree, with retained multi-seed desktop evidence and
  paired seed-7 production Pi 4 captures. It removes the CCD stall but does not
  qualify the Pi 4 frame budget; contact-bound updates remain the largest
  stalls there.

These experiments continue the performance investigation under #59/#354 and
event qualification in #359. They are planning notes; individual follow-up
tickets have not yet been created for the five proposals.
