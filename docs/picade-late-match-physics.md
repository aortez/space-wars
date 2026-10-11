# Picade late-match physics investigation

Reusing compound-shape thickness within each CCD pass reduces the retained
Picade slow window from 46.256 to 24.287 ms of physics work, a 47.5% reduction
in one paired headless run. The 120 measured native-state hashes, starting and
ending solver snapshots, and all 153 retained state checks match the control.
The candidate is committed locally; the deployed client contains only the new
diagnostic fields. Rendered performance with the optimization is not measured.

This investigation is separate from #178, which is ready for review with its
candidate still opt-in. The headless measurements below use runtime source
unchanged from `ffee5ba`; `e82b2a4` adds the final publication notes. The initial
Rust addition was the read-only `picade_physics_probe` example. Live diagnostic
fields now expose the existing backend counters as described below.

The [CCD qualification receipt](data/picade-ccd-thickness-v1.json) records the
live capture, fixed inputs, candidate source, binary hashes and comparisons.

## Live settings and scope

The Picade is at 1× raster scale, 1024×768. Its saved automatic-match preset is
heavy asteroids every second, Round grains, and 15/4-second combat breaks.
The saved human seat becomes Legacy v9 for automatic play; the second seat
retains Planner v10. `Round` is the original grain mode with strength and
collapse Off, rather than Weak/Round. The recovery candidate is disabled.

Two live samples motivated the investigation:

| Seed | Tick | FPS / UPS | Native physics mean |
| --- | ---: | ---: | ---: |
| `3057324812542282494` | 12,971 | 5.9 / 29.4 | 19.057 ms |
| `1978168328740471875` | 10,736 | 3.6 / 17.8 | 43.893 ms |

The headless probe follows the launcher's initialization, seat order, four-Hz
observation cadence, destination planner and shared work budgets. It uses the
same seed and saved gameplay settings, recording every action and native timer.
It excludes rendering, camera updates and evidence I/O from physics timings.

**These are new retained stress cases, not exact replays of the old live
states.** The original input streams and solver snapshots were not captured.
The first recreation's P2 remains on its mission path near the live sample's
rebuild period; the second recreation is substantially cheaper than its live
sample. The diagnostic uses Rust 1.89 with static host glibc, while the shipping
client uses Yocto Rust 1.94. Compiler/library and host differences have not been
isolated. The exact 43.893 ms live sample remains unpartitioned; CCD dominance
below is measured on the newly retained headless cases.

## Measurements

The kiosk stayed paused while both probes ran sequentially on the Pi. These
are means within each window, not an optimization comparison. Timers are nested:
CCD is part of physics; first-impact search and clamping are parts of CCD.

| Case / window | Physics | CCD | CCD share | Contact narrow phase | Solver |
| --- | ---: | ---: | ---: | ---: | ---: |
| First seed, ticks 12,852–12,971 | 32.888 ms | 23.738 ms | 72.2% | 3.136 ms | 3.889 ms |
| First seed, ticks 9,061–9,180, worst sampled window | 46.237 ms | 35.879 ms | 77.6% | 3.706 ms | 4.165 ms |
| Second seed, ticks 10,617–10,736 | 12.904 ms | 9.729 ms | 75.4% | 1.180 ms | 1.104 ms |

In the worst window, first-impact search averages 18.472 ms and motion clamping
17.326 ms, with three CCD substeps. This does not establish which shape pairs
or tree traversals account for the expensive sweeps. Final contact-pair counts
are not CCD query counts, and do not prove terrain self-collision.

At tick 12,960 of the first case, the world has 253 physics bodies, 128 loose
grains and four fragments. Grain counts remain within the configured capacity.
Both runs pass all conservation, finite-state/cache and speed audits: 14,040
plus 12,000 ticks, with 234 plus 200 audits. This is not evidence of a body leak.
Device temperature samples are retained with the raw logs; every captured CPU
clock sample reads 1.5 GHz. No whole-game FPS improvement is claimed.

The first dynamic probe could not start because the host linker required
`GLIBC_2.43` from the Pi's `libm`. No simulation tick ran in that attempt. The
successful static binary, failed invocation and all source hashes are retained.
The kiosk resumed automatic play afterward, with PID 2429, zero restarts,
unchanged settings bytes and 1× scale. A later read confirms ticks advancing.

## Reproduction and next gate

The [compact receipt](data/picade-late-match-physics-v1.json) records timings,
populations, checksums, exact configuration and compiler limitations. Full input
and cost traces are retained in `spacewars-handoffs/picade-physics-20261011`.
Use a fresh output directory with the frozen static binary on the Picade:

```sh
./probe --seed 3057324812542282494 --ticks 14040 --out retained-seed-new
./probe --seed 1978168328740471875 --ticks 12000 --out current-seed-new
```

Build the diagnostic with locked dependencies, Rust 1.89, the release profile,
default features and target `aarch64-unknown-linux-gnu`; its target Rust flags
are `-C target-feature=+crt-static`. Strict example Clippy and formatting pass.
No bot defaults, solver limits or historical recovery fixtures change in this
branch.

## Live diagnostic fields

Mission profile version 3 reports Rapier timings and populations in the same
bounded 120-tick window as `mission_world_physics_avg_ms`. It reads the metrics
already copied into each mission sample; it adds no simulation clocks, queries,
or world scans. Reading diagnostics remains observational.

`mission_rapier_ccd_{avg,p95,max}_ms` measures all CCD work. First-impact search
and motion clamping have their own `mission_rapier_ccd_first_impact_*` and
`mission_rapier_ccd_clamping_*` fields. These are nested timings, not additive
to the outer physics timer. `mission_rapier_ccd_samples` explicitly counts
complete CCD measurements. Missing backend timers are excluded from all three
CCD aggregates; no timing fields are emitted when that count is zero. Compare
CCD and physics means directly only when the sample counts match.

Existing broad/narrow-phase, island, solver and update timers are also exposed.
CCD substeps, active/sleeping bodies and contact/candidate populations report
means, maxima and latest values. Contact populations do not count CCD sweeps.
The host FPS/UPS window covers callbacks and can span more simulation ticks.

## Live confirmation and fixed-input diagnosis — October 11

The diagnostic client at `4d383cf` was built with the shipping Yocto toolchain
and deployed at 1×. Twenty samples cover two automatic matches with the saved
heavy/Round settings. All samples have 120 complete CCD measurements. The
slowest sampled physics window, seed `16293239525808771919` at tick 12,056,
averages 15.451 ms of physics and 11.616 ms of CCD (75.2%). Its first-impact
and clamping means are 5.831 and 5.761 ms. The host reports 7.5 FPS / 37.3 UPS,
over its different callback window. This is direct live confirmation of CCD's
large physics share, not a reproduction of the historical 43.893 ms sample.

The fixed-input runner reconstructs seed `3057324812542282494` through tick
9,060, writes the complete native solver snapshot, then examines ticks
9,061–9,180. It uses the original recorded controls, without rerunning bot
decisions. Both ARM and desktop replays match all 153 original per-second
observation hashes; the two hosts also match the 120 native-state hashes in
the measured window. Terrain conservation, finite-state and speed audits pass.

Query instrumentation changes the diagnosis: actual shape casts total only
about 1.16 ms per tick on the Pi, despite roughly 36 ms of CCD. The desktop
prefilter trace records 552,489 candidate-pair thickness checks across the
120 ticks, with 551,280 rejected before a shape cast. Thickness calculation
accounts for 1.421 of 2.539 ms of instrumented desktop CCD time. A compound's
thickness folds over all child shapes each time it is requested.

The candidate caches that exact property for compound colliders within one
first-impact or clamping call. Both calls immutably borrow collider geometry.
Each local cache is discarded on return, so subsequent shape edits, removals,
handle reuse and CCD substeps require no persistent invalidation. Primitive
shapes still use direct property calls. Threshold arithmetic, filtering,
candidate order, casts, substeps and collision response remain unchanged.

## Focused candidate qualification

The Picade comparison uses the same static ARM toolchain and fixed controls,
with query counting disabled in both diagnostic executables. Gameplay remains
paused during the pair. Means for ticks 9,061–9,180:

| Timer | Control | Cached thickness | Reduction |
| --- | ---: | ---: | ---: |
| Native physics | 46.256 ms | 24.287 ms | 47.5% |
| CCD | 35.947 ms | 14.064 ms | 60.9% |
| Whole native scenario step | 51.322 ms | 29.386 ms | 42.7% |

Starting/ending solver snapshots and observations, plus the entire audit log,
are byte-identical. Every measured tick has matching native-state and scenario
hashes and the same CCD substep count. The instrumented desktop comparison
also preserves all prefilter counts and reduces thickness time from 1.421 to
0.322 ms. Instrumented times include measurement overhead.

Two additional 30-second Weak quarry fixtures, Round and Angular, exercise
release, fracture and deposition with replay. Each shape's complete non-timing
output, observation hash and motion hash matches its control. The 157 engine,
576 Spacewars and 19 Scorched tests pass, including moving-ground and compound
sweep regressions. Nine client diagnostic tests and formatting pass. Existing
ignored long tests remain ignored. Strict Clippy is blocked by existing lints
in unchanged code; it reports no new warning locations in this change.

The initial candidate quarry build reused the control artifact from the shared
Cargo target. It was discarded. Vendor source timestamps were refreshed, the
candidate's Rapier compilation was verified in the log, and distinct executable
hashes were checked before the fixture comparisons.

This is one paired Pi timing result on one retained Round-heavy case, with
limited additional destruction fixtures. The candidate still exceeds the
16.7 ms physics budget in this window. It needs repeated timings, another
retained match, and a shipping-client check before deployment or a general
frame-rate claim. Captured CPU samples read 1.5 GHz; temperatures range from
71.6°C to 76.0°C during the pair, without exhaustive throttle monitoring.

The Picade resumed automatic play at 1×, PID 3198 with zero service restarts.
Saved settings remain byte-identical. #178 and bot defaults are unchanged.
Full evidence is in `spacewars-handoffs/picade-live-ccd-20261011` and
`spacewars-handoffs/ccd-fixed-input-20261011`; their source worktrees are excluded
from the portable evidence archives.
