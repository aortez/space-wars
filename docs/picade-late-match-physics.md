# Picade late-match physics investigation

The latest follow-up shares compound thickness across the complete physics
step, saving a further 27.6% and 20.4% of physics time in the same two retained
Pi cases. These are isolated headless comparisons against the per-pass cache,
not live FPS measurements. The shipping candidate is now installed on
`sw-picade.local` and passes a five-minute autoplay observation, including a
completed round and automatic restart. Heavy-scene slowdowns remain. See the
whole-step and live follow-ups at the end for method, limits and evidence.

Reusing compound-shape thickness within each CCD pass reduces physics work by
47.8% and 40.9% in two retained Picade stress cases. Two control/candidate pairs
per case, run in both orders, preserve every measured native state and all
1,144 retained observation checks. The optimized shipping client is deployed
at 1×. The worst retained window still averages 24.519 ms of physics, above the
16.7 ms budget for 60 updates per second.

This investigation is separate from #178, which is ready for review with its
candidate still opt-in. The initial headless investigation used runtime source
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

## Initial probe reproduction

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

This initial result covered one paired Pi run on one retained Round-heavy
case, with limited additional destruction fixtures. It motivated the repeated
comparison and shipping-client check below. Captured CPU samples read 1.5 GHz;
temperatures range from 71.6°C to 76.0°C during the initial pair, without
exhaustive throttle monitoring.

After that initial qualification, the Picade resumed automatic play at 1×,
PID 3198 with zero service restarts.
Saved settings remain byte-identical. #178 and bot defaults are unchanged.
Full evidence is in `spacewars-handoffs/picade-live-ccd-20261011` and
`spacewars-handoffs/ccd-fixed-input-20261011`; their source worktrees are excluded
from the portable evidence archives.

## Repeated comparisons on both retained matches

Eight fresh runs use the same verified static control and candidate binaries,
with counting disabled. Each case has two pairs with opposite control/candidate
order. The kiosk is paused for these measurements. Table values are the mean
of the two 120-tick window means for each variant:

| Case / window | Control physics | Candidate physics | Reduction | Control CCD | Candidate CCD |
| --- | ---: | ---: | ---: | ---: | ---: |
| First seed, 9,061–9,180 | 46.976 ms | 24.519 ms | 47.8% | 36.536 ms | 14.181 ms |
| Second seed, 7,861–7,980 | 17.095 ms | 10.103 ms | 40.9% | 12.897 ms | 5.945 ms |

The first case's control means range from 46.839 to 47.112 ms and candidate
means from 24.464 to 24.574 ms. The second case ranges from 16.961 to 17.230 ms
for control and 10.066 to 10.140 ms for candidate. CCD decreases by 61.2% and
53.9%, respectively. These are two repetitions per variant, not a broad
statistical estimate across maps or devices.

Each arm reconstructs the physical world from tick zero using frozen actions.
Across 68,640 replayed ticks, all 1,144 original per-second observation checks
match and all material audits pass. Each case's four runs have byte-identical
starting/ending native snapshots, starting/ending observations and audit logs.
All 120 per-tick native hashes, observation hashes and CCD substep counts in
each measured window match across every arm. Five-second temperature/frequency
samples range from 72.1°C to 79.4°C; all 169 captured CPU clocks read 1.5 GHz.

The [repeated qualification receipt](data/picade-ccd-qualification-v1.json)
records individual runs and shipping evidence. Full inputs, binaries, outputs,
scripts and build artifacts are retained in
`spacewars-handoffs/ccd-cache-qualification-20261011`. Reproduction instructions
require fresh output directories and separate Cargo target directories per
source variant. The performance PR is stacked on #178 so recovery-policy work
remains separately reviewable.

## Optimized shipping client at 1×

The clean `23b51e9af014` source was built with the Yocto Rust 1.94 toolchain,
without `ccd-diagnostics`, and installed with the compatible fast-update helper.
The client hash is recorded in the receipt. The saved heavy/Round preset and
Legacy/Planner defaults remain byte-identical, including 1× raster scale at
1024×768. The helper verified the new client/CLI pair and retained the previous
binaries for rollback.

Sixty-one read-only samples span 644.3 seconds and four automatic matches.
Fifty-nine samples contain complete 120-tick mission/CCD windows; 57 of these
are from matches still in progress. The performance ranges below exclude
incomplete windows and game-over samples:

| Live metric | Minimum | Median | Maximum |
| --- | ---: | ---: | ---: |
| Host FPS | 4.4 | 16.5 | 58.9 |
| Host UPS | 22.2 | 59.0 | 60.7 |
| Physics window mean | 1.550 ms | 6.759 ms | 31.753 ms |
| CCD window mean | 0.706 ms | 3.805 ms | 16.102 ms |

These are different worlds from the earlier diagnostic deployment, not a
paired live benchmark. Host and mission windows also differ. The ranges do
not establish a live frame-rate improvement. The worst sampled physics window
occurs at tick 11,728 of seed `4933744737834333975`: CCD costs 16.102 ms,
the contact narrow phase 5.580 ms and the solver 6.284 ms. Its mean CCD substep
count is four. Caching removes repeated work but does not eliminate expensive
CCD, contact or solver processing in damaged worlds.

Both bots perform on-foot capture and reboarding, and automatic match turnover
continues. Planner enters escape-pod recovery in seed `8247063650597350487`,
progressing from stabilization to landing. The pilot dies before rebuilding;
the match ends with `PilotDeath` at the sampled tick 10,198. No complete
escape-pod-to-ship rebuild was observed in the sampled interval. This bounded
check exercises recovery attempts without establishing universal recoverability
or changing #178's opt-in recovery policy. Ten-second samples can miss brief
states; their ticks are observation times, not exact event timestamps.

The client remains active as PID 4904 with zero service restarts, and the
retained journal contains no error or panic entry for that process. CPU samples
read 1.5 GHz at 70.1–73.5°C. Early and final screenshots were inspected at
1024×768. The first sampling attempt happened before automatic gameplay started
and had no scenario fields; it is retained separately and excluded from these
results. The successful collector explicitly waits for gameplay.

All seven [CI checks on the runtime source](https://github.com/aortez/space-wars/actions/runs/38108025801)
pass, including the Linux workspace/UI checks, deterministic AI baselines,
experimental recovery, Windows and AArch64 builds. Subsequent report-only
commits do not change that tested runtime source. The performance change is
ready for review separately from #178; the remaining slow live windows and
incomplete recovery observation are limits, not solved problems.

## GPU-default follow-up: residual physics costs — October 11

This is a diagnosis on `ca9b229`, not another optimization or a demonstrated
GPU regression. The installed client, CLI and saved settings remain unchanged.
The additional counters are behind `ccd-diagnostics`; lifecycle clocks also
require an explicit replay measurement. Normal client builds do not collect
them. The fixed-input example now reports terrain subphases and the existing
engine material profiler alongside CCD search, bounds and cache counters.

### Live evidence

Twenty-five read-only samples cover 140 seconds of automatic match
`9286148569070392789`, ticks 24,124–30,084. Median sampled window means are
14.024 ms of physics per tick, including 9.861 ms of CCD, and 3.082 ms of
terrain/body lifecycle work. Rendering consumes 7.706 ms of CPU per display
iteration. The host processes a median 4.975 updates per callback, close to its
five-update catch-up limit. FPS ranges from 6.7 to 14.3; UPS from 33.7 to 56.3.
Host and mission rolling windows cover different ticks, so these are not an
additive decomposition of one frame. The renderer reports V3D and no error.

The live interface cannot export a complete replay of this session. Instead,
the existing two retained input streams are replayed with the new counters;
they remain distinct cases, not exact recreations of this new live window.

### CCD screening is still the expensive part

| Per-tick mean | First retained case | Second retained case |
| --- | ---: | ---: |
| Time-of-impact candidate checks | 4,604.075 | 1,516.283 |
| Rejected by relative motion/thickness | 4,594.000 | 1,498.408 |
| Actual top-level shape sweeps | 10.075 | 17.875 |

The final, more detailed first-case pass suspends the already-paused kiosk
process during replay. With the display workload absent, instrumented CCD
averages 18.249 ms per tick. Its nested thickness checks cost 8.748 ms:

- 1,922.925 compound cache requests include 160.608 misses per tick.
- Those misses visit 24,920.025 immediate child shapes and spend 6.735 ms
  calculating compound thickness. The per-pass cache works, but discards its
  results before the next first-impact/clamping pass, and before the next tick.
- Uncached primitive thickness costs 0.589 ms. The rest of the thickness span
  includes cache access, dispatch and instrumentation overhead.
- Existing-contact lookups cost 1.785 ms and candidate traversal 1.713 ms.
  Swept bounds cost 0.292 ms and BVH updates 0.168 ms.
- Actual nonlinear shape sweeps total 1.146 ms. The compound/compound sweeps
  account for 0.767 ms of that.

These timers nest; do not add them to the outer CCD timer. Detailed collection
has substantial overhead: in the earlier pair, with the paused kiosk still
drawing, physics averages 36.325 ms with collection and 31.592 ms without it.
Even collection-disabled runs retain the diagnostic build's conditional code.
Do not treat instrumented component timings as shipping costs or compare the
suspended final pass to that pair as an optimization result.

### Terrain spikes are predominantly settling/packing

In the isolated first case, lifecycle averages 2.889 ms. Checking and depositing
loose dirt accounts for 2.728 ms, including 0.805 ms building the support graph
and 1.370 ms evaluating packing plans. Added-surface assessment alone accounts
for 0.844 ms. Successful collider synchronization averages only 0.077 ms;
ordinary body reconciliation costs 0.109 ms.

The earlier two instrumented windows contain lifecycle peaks of 23.769 and
24.046 ms. Their worst ticks spend 17.108 and 20.958 ms, respectively, in packing
plans. This distinguishes the sporadic planning/clearance cost from continuously
rebuilding all terrain colliders. Packing, surface assessment, clearance and
commit timers are nested, not independent totals.

### Recommended next experiment

1. Extend geometric-property reuse. Start with sharing compound thickness
   across a complete physics step; consider geometry-lifetime caching with
   explicit reshape/removal invalidation. Translation and rotation alone do
   not change local shape thickness. Preserve exact arithmetic and query order.
2. Investigate a cheap, conservative rejection before the existing-contact
   lookup. Most candidates never reach a shape sweep; do not weaken CCD or
   change collision thresholds to skip this work.
3. Separately reduce packing's added-surface/clearance work and allocation.
   Preserve placement checks, material conservation and the simulation schedule.

The paused GPU client continued to consume roughly one CPU core, so future
isolated physics benchmarks should suspend rendering as well as simulation.
The bounded suspension used a 300-second remote timeout, a resume trap and a
disabled service watchdog check. The same PID resumed autoplay with no restart.
This is a separate idle-rendering follow-up, not a physics optimization.

### Verification and retained evidence

Across four ARM replays (35,520 ticks), all 592 original per-second observation
checks pass. All 480 measured native-state/observation hashes and CCD substep
counts match their retained references. Starting/ending solver snapshots,
observations and audit logs are byte-identical. Terrain conservation, finite
state and speed audits pass. The initial desktop diagnostic also matches all
120 measured native states and 153 observation checks.

The focused suites pass: 157 engine tests plus 583 scenario tests, including
three new measurement-lifecycle tests; one pre-existing engine test is ignored.
The normal, non-diagnostic configuration also compiles. Formatting and diff
checks pass. No gameplay fix, commit, application deployment or PR is part of
this follow-up.

The [measurement receipt](data/picade-residual-physics-v1.json) retains means,
counts, hashes and thermal samples. Full local evidence and replay scripts are
in `target/physics-cost-investigation.wycn2MHN/`. ARM diagnostics use Rust 1.89
with static glibc, not the shipping Yocto toolchain. The initial three replays
leave the paused display rendering; the final pass suspends it and also adds
cache-miss counters. These differences preclude direct speedup claims between
those passes. No setting, installed binary or renderer default changed.

## Whole-step thickness cache — October 11

The pipeline now owns one temporary thickness cache after applying user shape
changes, sharing it across first-impact searches, motion clamping and CCD
substeps. Collider poses change in the loop; local geometry does not. Each
step discards the cache, so edits, removals and handle reuse on the next step
cannot read stale values. Primitive shapes still use direct property queries.
No cache is serialized or retained in `CCDSolver`. The public solver methods
keep their signatures and safe per-call caches for standalone callers.

### Isolated Pi comparison

Both variants use the same Rust 1.89 release/static ARM build settings and
diagnostic feature, with detailed collection **disabled** for timing. The
GPU kiosk is paused and its process suspended during each replay, with a
300-second timeout and resume trap. Unlike the earlier GPU-default diagnosis,
no display workload competes with either variant.

Two runs per variant use control/candidate/candidate/control order for the
first retained case and the reverse order for the second. Means per tick:

| Retained case | Per-pass physics | Whole-step physics | Reduction | Per-pass CCD | Whole-step CCD |
| --- | ---: | ---: | ---: | ---: | ---: |
| First, ticks 9,061–9,180 | 23.402 ms | 16.949 ms | 27.6% | 13.682 ms | 7.257 ms |
| Second, ticks 7,861–7,980 | 9.600 ms | 7.640 ms | 20.4% | 5.657 ms | 3.718 ms |

Physics includes CCD; do not add those columns. Cache construction/destruction
now sits at the step boundary, so the outer physics wall timer is the principal
comparison, not just the narrower CCD timer. Solver, contact narrow-phase and
terrain-lifecycle means remain essentially unchanged in the paired runs.

The first pair's frozen candidate predates moving the optional profiling
attributes from the public wrappers to the internal implementation. The final
source is separately rebuilt and rechecked on that case: 17.010 ms physics,
7.330 ms CCD, with identical states. The second case and final counter runs
use that final-source binary; all binary hashes are retained.

Separate counter captures show what was eliminated in the first case:

- Compound computations fall from 160.608 to 27.067 per tick; immediate child
  visits from 24,920.025 to 4,218.175, roughly 83% less repeated traversal.
- Compound requests remain 1,922.925 per tick. Candidate checks remain
  4,604.075, of which 4,594 are rejected and 10.075 reach top-level shape casts.
- The same candidate ordering, thickness arithmetic, rejection thresholds,
  casts, CCD substep counts and collision response are retained.

Those work counts explain the improvement; collection-enabled timer values
are not used for the speedup calculation. Normal builds omit these diagnostic
clocks entirely. The diagnostic compiler/static libc also differs from the
installed Yocto client, so a live-client deployment check remains necessary.

### Regression checks and remaining work

The new work-count test fails with per-pass caching (five compound calculations
instead of one) and passes with the whole-step cache. It covers disabled CCD,
one/multiple substeps and reset between steps. Another regression exercises
in-place edits, compound/primitive transitions, removal with collider arena
slot reuse, continued collision response and byte-identical snapshot
continuation. Existing moving-ground, compound-sweep and contact tests remain.

The normal physics/scenario library suites pass 1,157 tests with five existing
ignored tests. The diagnostic engine/Spacewars suites pass 742 tests with one
existing ignore. Formatting and diff checks pass. Strict all-target Clippy is
blocked by pre-existing lints in unchanged spaceling and Spacewars code; this
change does not address those unrelated warnings.

Eleven qualified ARM replays cover 94,980 ticks. All 1,583 original observation
checks and 1,320 measured native-state/observation hashes and CCD substep counts
match the retained references. Starting/ending solver snapshots, observations
and audit logs are byte-identical; terrain conservation and finite-state/speed
checks pass. A final-source desktop replay separately matches 153 observations
and 120 measured states. The counter runs also retain identical shape-cast,
tree-node and leaf-query counts.

One additional second-case run completed but was excluded from timing because
the kiosk changed from pause to gameplay at completion. The safety guard
stopped the batch without overwriting that UI state. Its state checks still
match; the complete second-case comparison was repeated after the user
confirmed device availability. No service restart was required.

Accepted-run temperature samples span 69.6–76.0°C. CPU samples read 1.5 GHz,
apart from two 1.4 GHz readings in the first 0.3 seconds of their runs, before
the measured windows. These are samples, not continuous throttling telemetry.
The same installed kiosk PID 451 is active in unpaused autoplay with zero
restarts afterward. Client, CLI and settings hashes are unchanged.

The [whole-step receipt](data/picade-ccd-step-cache-v1.json) includes per-run
means, latency distributions, exact compiler settings, source/binary hashes,
work counts and verification results. Full evidence, frozen binaries, source
copies and comparison scripts are retained outside build caches at
`spacewars-handoffs/ccd-step-cache-20261011`; the working copy is
`target/ccd-step-cache.JmZG0nN5/`. Original action streams remain in
`spacewars-handoffs/ccd-cache-qualification-20261011/inputs`.

Even after the saving, the heavier window still needs about 17 ms for physics
alone, plus lifecycle and host/rendering work. This is not a claim of 60 FPS
or a fix for loose-dirt packing spikes. The live-client check below qualifies
deployment; further hot-path changes need separate, similarly measured tests.

## Shipping-client observation — October 11

Built the normal GPU kiosk recipe with Yocto Rust 1.94, release/fat LTO, and
deployed with `./update.sh --fast --target sw-picade.local --yes`. The extra
`ccd-diagnostics` feature is absent from Rapier, the adapter and the scenario.
All ten candidate source hashes still match the tested headless source.
Client SHA-256 is
`70d1e1086e8656de7dec3810b060945ea1f0c7056e2b36d49ab361cac11b916d`.
The CLI and saved settings hashes are unchanged; no image flash or reboot was
needed. Neither Clock device was changed.

Read-only sampling ran for 300.137 seconds at five-second intervals: 61 total
samples, 57 during active gameplay, two in the launcher and two on the result
screen. Seed `3706749222645332783` completed at tick 12,706; autoplay then
started seed `3972511307554219699`, reaching tick 2,912 before sampling ended.
PID 4381 stayed active with zero unexpected service restarts. GPU scene mode
stayed `retained-culled`, with no scene or text errors. The journal contains
no new failure, only the already-present gamepad default-mapping warning.
A screenshot after sampling shows both split-screen views, ships, terrain,
minimaps and HUD rendering normally with V3D direct geometry.

For the 15 active samples at tick 9,000 or later in the first round:

| Reported metric | Median | Sample range |
| --- | ---: | ---: |
| Physics, ms/update | 7.344 | 4.355–11.680 |
| CCD, ms/update (included in physics) | 3.515 | 1.998–5.336 |
| Terrain/lifecycle, ms/update | 4.258 | 2.851–5.873 |
| Drawing CPU, ms/rendered loop | 10.866 | 6.175–14.703 |
| Updates per host callback | 3.425 | 2.183–4.900 |
| FPS | 16.8 | 8.4–27.9 |
| UPS | 53.1 | 41.8–56.8 |

These summarize rolling-window means, not the distribution of all ticks. The
physics window spans 120 updates; rendering/host windows cover their own
iterations. They are not aligned independent totals and must not be added
together. Process CPU is approximately one saturated core in this portion.
All mature-window clock samples read 1.5 GHz, with temperatures 71.6–73.5°C;
these are samples, not continuous throttling telemetry.

This is **not** a before/after performance comparison: the pre-deploy match had
a different seed, tick and geometry. The matched headless replays remain the
evidence for the 20–28% physics saving. The live run confirms successful
deployment and ordinary gameplay progression, not exhaustive correctness or
60 FPS. Exact-state replay checks supply the stronger regression evidence.

The live run also exposes remaining work. At tick 6,400, physics averages
18.992 ms with four CCD substeps: CCD 6.440 ms, contact narrow phase 4.348 ms
and constraint solving 5.916 ms. Terrain/lifecycle is separately 4.788 ms.
Across all active samples, the largest reported lifecycle tick is 38.883 ms.
The earlier instrumented replays attribute similar spikes mainly to dirt
settling/packing, but this shipping sample does not subdivide that timer.
The next experiment should retain the fixed-input/state-equality checks while
reducing that packing work; contact/solver cost and rendering remain distinct
contributors, not effects this cache change claims to eliminate.

The [live receipt](data/picade-ccd-step-cache-live-v1.json) records the build,
scope and measurements. Raw statuses, process/thermal samples, logs, deployment
output, screenshot, client binary and collector/report scripts are retained at
`spacewars-handoffs/ccd-step-cache-live-20261011/`; working files are in
`target/ccd-live-deploy.VVpDoDmU/`. The candidate remains installed and running
autoplay for further observation. No commit or push was made in this step.
