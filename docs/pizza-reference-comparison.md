# Pizza: common-fixture comparison and lazy CCD contact lookup

This continues PR #505 / #354. It does not qualify the 5,000-ball ceiling or
resolve the earlier production GPU failures in #359. The saved default is 500.

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

## Verification and remaining qualification

All 203 engine-physics tests (including CCD diagnostics) and 40 Pizza tests pass.
The new shared-motion regression fails against the old CCD path because it
performs an unnecessary contact lookup, and passes with the optimization. It
checks both one and four CCD substeps; the existing suite covers fast impacts,
moving/compound ground, sensors, hooks, shape edits and snapshot continuation.

This optimization preserves the recorded physics; it does not alter the
capacity, field force or fragmentation distribution to make the test easier.
Production rendered Pi 4 qualification must be repeated on the final candidate.
Pi 5 remains offline. PR #505 stays draft and #354/#359 remain open.
