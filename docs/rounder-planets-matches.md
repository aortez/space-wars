# Rounded terrain in generated matches

Follow-up: [landing forecasts](landing-forecast.md) identifies and fixes the
mirrored seed-42 self-obstruction loop, and records the remaining routing and
duel regressions. The measurements below retain the original `fff99b3` result.

The ordinary `spacewars` match and both generated arena presets now construct
all three planets with `TerrainSurface::Interpolated`. The initializer attaches
the existing circle samples before building collision geometry. Planet sizes,
orbits, material cells, weapons, bot policies and match rules are unchanged.
The fixed travel/recovery labs and Classic retain their historical surfaces.
The three single-planet comparison presets remain available.

The matched runs pass their physical/material checks, but the extra collision
pieces have a substantial CPU cost and two existing AI deadline tests regress.
This is an experimental adoption on the branch, not a merge-ready checkpoint or
a claim of equivalent performance. No cabinet deployment is part of this experiment.

## Experiment

Baseline metrics were collected before changing the generated-world initializer.
Commit `5c488f7` adds read-only terrain counters, optional per-tick draw timing,
and the isolated edit benchmark to the block-based match. Release binaries were
archived before the surface change. The initial replay check compares a ten-second
seed-42 duel with draw measurement on and off: every non-timing report field is
identical.

Both match matrices use the eight preselected seeds from the
[fresh-world survey](fresh-world-survey.md), each with asteroids Off and Mixed
at a three-second mean interval. Both seats use the normal mission bot and 15/4
combat breaks. Each run ends at pilot death or 180 simulated seconds; no result
or extra simulation time is injected. Matches are run serially in release mode
on the same desktop with Rust 1.94.1. No builds or other benchmark jobs run
alongside the timed processes.

Timing records each bot observation and policy call, the scenario physics/gameplay
step, and construction/destruction of two player draw lists and two minimaps.
The combined tick measurement sums those operations at each tick. Audit scans,
trace/report output, bookkeeping, rasterization and presentation are excluded.
These values are CPU workload measurements, not displayed FPS. Aggregate means
are weighted by measured operation counts; a median of case P95 values is not
a pooled P95. Different survival times and bot routes change the sampled workload.

An additional benchmark copies the actual three generated material fields for
each seed. Each receives the same 60 edits: forty circular cuts (radii 0/1/3/6)
near the outside, then twenty thin interior capsule cuts. Each field repeats
three times. This isolates edit and geometry-refresh work from AI trajectories.
It does not execute physics, connectivity splitting, fragment motion or drawing.
The comparison tool checks material hashes, quantities and changed-cell counts
after every cut, in addition to each runner's cache/material coverage checks.

Terrain diagnostics record raw cell/sample bytes, rectangles, polygons, polygon
vertices and total colliders. Memory figures exclude cache and allocator overhead;
peak dynamic counts are sampled once per simulated second and at the final tick.

## Results — 2026-09-11

| Match measure, 16 runs per surface | Blocks | Round |
| --- | ---: | ---: |
| Physical/material audit passes | 16 | 16 |
| Simulated seconds observed | 2,629.30 | 2,623.77 |
| Captures | 50 | 45 |
| Completed capture/board/depart trips | 47 | 43 |
| Runs with weapon contact | 13 | 14 |
| Finished rounds within the cap | 6 | 5 |
| Ships lost / ships rebuilt | 9 / 0 | 8 / 0 |
| Peak sampled terrain colliders | 1,538 | 12,556 |
| Peak sampled detached terrain fragments | 2 | 2 |

All audits preserve occupied plus removed material and report no stale terrain
geometry/body identities or nonfinite motion. Neither matrix hits the runner's
500-unit/s speed ceiling. These matches only reach two detached terrain fragments
at sampled instants; they do not establish heavily fragmented-world performance.

The timed desktop workload increases materially:

| Weighted mean work | Blocks | Round | Ratio |
| --- | ---: | ---: | ---: |
| Scenario step | 0.166 ms | 2.258 ms | 13.6× |
| Bot observation, per call | 0.084 ms | 0.188 ms | 2.2× |
| Bot policy, per call | 0.0021 ms | 0.0036 ms | 1.7× |
| Two player draw lists + two minimaps | 0.047 ms | 0.328 ms | 6.9× |
| Combined measured tick | 0.386 ms | 2.970 ms | 7.7× |

The per-case combined P95 ranges from **0.286–1.065 ms** before and
**2.572–7.331 ms** after. There are zero measured ticks above 16.67 ms in the
baseline and **68 of 157,426 (0.043%)** after. These are elapsed CPU-operation
measurements on a shared desktop; they exclude the display and can include OS
preemption. Unrelated work on the desktop was not controlled. Treat the ratios
as investigation results, rather than a portable hardware or FPS guarantee.

### Identical geometry work

All **4,320 edits per surface** leave identical material hashes, occupied counts
and changed-cell counts in each before/after pair. Differences are in the
reconstructed boundary and the work required to maintain it.

| Mean per field build or edit | Blocks | Round | Ratio |
| --- | ---: | ---: | ---: |
| Initial geometry build | 0.0815 ms | 0.4036 ms | 4.9× |
| Apply material edit | 0.0025 ms | 0.0152 ms | 6.1× |
| Refresh derived geometry | 0.0076 ms | 0.3164 ms | 41.8× |

The high refresh ratio has a small baseline denominator, but it is real work:
rounded boundaries clip convex patches and track neighboring shape samples.
Small edits can invalidate more than their changed material cells. Initial builds
also process large unchanged interiors, which still use merged rectangles. This
explains why the refresh ratio can be much larger than the whole-field build
ratio. These measurements include no Rapier synchronization or fragment splitting.

Initial collider counts increase **7.8–12.3×** across the same eight worlds,
from 247–1,457 to 3,027–11,434. More collision pieces and polygon draw primitives
make the larger physics and drawing costs plausible. The scenario-step increase
is larger than the collider-count increase; this experiment does not attribute
the remainder to a specific Rapier stage. Profiling collider updates, collision
queries and contacts is the next performance investigation, before changing the
surface algorithm again.

### Memory

The extra raw shape samples occupy **61–657 KiB per three-planet world** in this
seed set. That is only the stored scalar field, not the total cost of the mode.

A separate ten-second startup probe of world C, the largest raw field allocation
in the baseline, alternates archived block/round binaries for three runs each.
Peak process RSS has a median of **9.30 MiB before and 26.68 MiB after**.
No material is removed in these probes. The measurement includes physics, draw
temporaries, diagnostic allocations and allocator retention; it excludes a real
display. It demonstrates a cost beyond the scalar samples, but does not identify
each allocation category or predict kiosk RAM directly. Commands and all six
measurements are in `startup-memory/` beside the match evidence.

### Gameplay interpretation

Some recorded failures improve, while other journeys and fights take different
paths. World C with asteroids Off previously left P2 surveying for about 90 seconds
and completing no trips; P2 now completes three trips and is hunting by the final
window. P1's trip count changes from three to one in the same paired run. This is
evidence that the original stall does not reproduce in that rounded world, not
a general fix to landing or an isolated test of one route: flag placements,
contacts and both pilots' trajectories also change.

World B quiet still ends with P2 winning, but at 135.17 seconds instead of 88.35.
Some other cases finish only before or only after. Total observed simulation time
is almost unchanged across the matrices, yet captures/trips are slightly lower
afterward. There is no basis here for claiming an overall bot improvement. Keep
the exact paired traces when investigating a particular stall or combat outcome.
Neither matrix supplies a successful rebuild; the separate recovery contracts
remain necessary.

## Validation details

The generated-world contract checks the normal match's round default, identical
material and planet layout against explicit block mode, edits on all three planets,
and exact cloned simulation/observation agreement. An old corner-contact regression
uses explicit block mode because its recorded pose depends on that particular
block corner; its original landing assertions remain intact.

The explicit block override in the new runner reproduces every non-timing report
field of the archived world-B quiet baseline. The isolated edit harness also
checks seed 42 separately before the main matrix. Selected-example Clippy passes
with seven existing scenario-library warnings and no new example warnings.

The normal match's seat selection, both renderers, pause/restart and new-world
flows pass the desktop UI checks; vector and raster screenshots were inspected.
The first long result-screen check timed out after 180 wall seconds while the
debug application had advanced only 78 gameplay seconds, with the long debug
simulation suite also running. It is retained as a timing-limited attempt, not
counted as a successful result test. A separate headless release replay of the
same seed 7 reaches a real P2 win at tick 9,028 (150.47 simulated seconds).
The unchanged result/rematch UI test then **passes in release mode**, including
two complete physical rounds, rematch, a new world and return to the launcher.
Its result screenshot was inspected. The earlier timeout remains in the evidence
and is not counted as a pass.

The long generated mission target was rerun in release mode with its existing
assertions: **18 pass and two fail**. The partially completed debug mission suite
was stopped after the timed comparisons; successful earlier workspace targets
and the remaining targets are retained separately. The two failures are real
simulation-deadline regressions, independent of the earlier UI wall timeout:

- `generated_orbiting_ground_supports_real_claims_boarding_and_departure`:
  seed 3, P2, unreflected, completes two trips by 180 seconds. It arrives at its
  third destination at tick 8,838 and is still approaching at the cutoff.
- `pursuit_reaches_weapon_contact_after_physical_capture_preparation`:
  seed 42, P1, reflected, completes two trips and is settling after exiting at
  the third destination. Its first departure is late, at tick 6,490. It never
  enters pursuit within the unchanged 180-second preparation limit.

The tests are retained without weakening their deadlines or requirements.
Paired block/round runner traces are saved under `deadline-diagnostics/` to
separate slow travel, landing and on-foot delays before any AI tuning or rollout.

Both archived block controls pass and both rounded controls reproduce their
deadline failure with clean audits. The paired visits narrow the next work:

- **Seed 3:** the first two departures improve from 74.63/110.15 seconds to
  42.00/98.52 seconds. The third arrival shifts from 131.12 to 147.30 seconds,
  and the rounded run replans that approach before reaching the cutoff. Earlier
  trips change the orbital phase and subsequent flight route. This is not
  evidence of a spaceling stuck on the first two planets. Inspect world-level
  travel/avoidance and the final approach starting at tick 5,912.
- **Seed 42 reflected:** the first arrival barely changes (16.25 to 16.28 seconds),
  but its first departure moves from 45.27 to 108.17 seconds. The rounded run
  retries at 38.98, 61.17 and 84.95 seconds, selecting bearings 1, 0, 63 and
  finally 62 on planet 1. It lands at 101.28 seconds and claims/boards promptly.
  The delay is before landing, rather than in the pursuit controller. Samples
  at 32–38 seconds report zero accepted feet, no hull contacts and foot clearances
  of roughly 0.77–1.29 units. Capture denser contact/motor history around the first
  approach before attributing this to hovering or intermittent contact impulses;
  the one-second snapshots cannot distinguish them. Continue from the
  [landing investigation guide](landing-investigation-guide.md).

Across all completed workspace targets there are **1,279 passing tests, two
failures and 41 normally ignored tests**. This combines the completed debug
targets with the release mission target, without counting its interrupted debug
attempt twice. `test-summary.json` records every target and its source log.

The diagnostic commands preserve the original deadlines:

```sh
target/release/examples/surface_mission_soak --world generated --surface round \
  --seed 3 --seat 1 --mirror false --mode quiet --seconds 180 \
  --asteroid-interval 0 --require-route true --trace true --frames true \
  --out /tmp/round-seed3-deadline
target/release/examples/surface_mission_soak --world generated --surface round \
  --seed 42 --seat 0 --mirror true --mode pursuit --prepare-seconds 180 --seconds 90 \
  --asteroid-interval 0 --trace true --frames true --out /tmp/round-seed42-deadline
```

Use `--surface blocks` with different output directories for the controls. These
rounded commands currently exit nonzero after preserving their reports. For the
pursuit case, preparation fails at 180 seconds, so its additional 90-second
pursuit window never starts. Do not interpret this as a failed 90-second chase.

## Reproduction

Build both runners, then gather a serial baseline and rounded matrix with the
same current executable. Explicit surface selection is diagnostic only; the
normal launcher always uses the generated match's rounded default.

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak \
  -p scenario-spacewars --example surface_geometry_benchmark
python3 tools/run-match-world-survey.py \
  --binary target/release/examples/surface_mission_soak \
  --surface blocks --measure-draw --jobs 1 --out /tmp/match-blocks
python3 tools/run-match-world-survey.py \
  --binary target/release/examples/surface_mission_soak \
  --surface round --measure-draw --jobs 1 --out /tmp/match-round
python3 tools/compare-match-surfaces.py \
  --before /tmp/match-blocks --after /tmp/match-round --out /tmp/match-comparison
```

For an isolated edit case, use `surface_geometry_benchmark --surface blocks`
or `--surface round`, plus `--seed SEED --repeats 3 --out NEW_DIRECTORY`.
Store each seed under a parent directory and supply both parents as
`--before-geometry` and `--after-geometry` to the comparison tool. It checks all
material snapshots before calculating timing differences. Preserve the reports
and executable hashes when comparing another toolchain or machine.

Local evidence is retained under:

```text
/home/oldman/.codex/visualizations/2026/09/11/rounder-planets/multi-planet/
```

This includes archived executables, source manifests, build logs, commands,
one-second audits, bot decisions, damage events and frame JSON. Frame JSON is
simulation output, not an application screenshot. Pi display performance is
outside this desktop experiment.
