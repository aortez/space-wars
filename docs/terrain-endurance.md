# Terrain endurance test bed

`spacewars_terrain_soak` runs the actual Spacewars simulation for **1–180 simulated
seconds per workload and seed**, at a fixed 60 Hz. It runs faster than real time
when possible and needs no display or controller. The output is an offline HTML
report with a time slider, snapshot playback, planet cameras, performance charts,
and an event timeline, plus JSON and CSV for comparisons.

## Running

The default matrix is four workloads × three seeds × three minutes: 12 runs,
129,600 requested physics ticks, and 36 minutes of requested simulated time. Each
run also has a default three-minute wall-clock budget. A slow run retains its
partial evidence and is marked **incomplete**, then the matrix continues. Raise
`--wall-seconds` when deliberately studying workloads slower than real time.

```sh
cargo run --locked --release -p scenario-spacewars \
  --example spacewars_terrain_soak -- \
  --output target/terrain-soak/baseline --label desktop-baseline
```

Open `target/terrain-soak/baseline/report.html` in a browser. The HTML works
offline and embeds its snapshots; it does not require a web server or fetch data
from the network. Keep the companion files beside it to use the raw-data links.

A quick smoke run and a focused three-minute run:

```sh
cargo run --locked --release -p scenario-spacewars \
  --example spacewars_terrain_soak -- --seconds 10 --seeds 42 \
  --output target/terrain-soak/smoke

cargo run --locked --release -p scenario-spacewars \
  --example spacewars_terrain_soak -- --seconds 180 --seeds 42,1337 \
  --cases fragments,multi-planet --output target/terrain-soak/stress
```

| Option | Default | Meaning |
| --- | --- | --- |
| `--seconds` | `180` | Whole simulated seconds per case/seed; accepts 1–180. |
| `--wall-seconds` | `180` | Wall-clock budget per case/seed, checked between ticks; accepts 1–3600. |
| `--seeds` | `42,1337,9001` | Seed list for terrain and intervention scheduling. |
| `--cases` | All four | Comma-separated workload names below. |
| `--output` | `target/terrain-soak/<timestamp>` | New directory; existing directories are rejected to preserve previous evidence. |
| `--label` | `local` | Machine/build label retained in the report. |

Use a release build for performance comparisons. The report labels debug builds
as unrepresentative. It also records the executable's SHA-256, architecture, OS,
command, and the current checkout revision/dirty flag when Git is available.
Checkout metadata is collected at run time; it does not prove which source built
an arbitrary copied binary. Give copied builds a meaningful label.

The older 20-second microbenchmark remains available as
`--example spacewars_terrain_benchmark`. Its name is unique across the workspace;
Terrain Lab retains its own `terrain_benchmark` example.

## Workloads

| Name | World | Scheduled destruction |
| --- | --- | --- |
| `cannon` | One radius-60 material planet | An aimed shell every two seconds, beginning at one second. |
| `excavation` | Same single planet | A seeded thin cut at five seconds and every ten seconds thereafter, plus the shell workload. |
| `fragments` | Same single planet | A grid cut at five seconds, with 12-cell spacing, plus the shell workload. |
| `multi-planet` | Normal radius-1500 world with its planets opted into material terrain | A seeded thin cut at five seconds and every fifteen seconds thereafter, plus the shell workload. |

Shells are real Spacewars projectile bodies spawned outside the selected material
field and aimed at an occupied cell. Surviving planets and detached pieces are
eligible targets. They use ordinary contact, damage, work-budget, and cleanup
paths. A shot can miss or hit an intervening body; injected-shell count and
successful terrain-hit count are deliberately separate measurements.

Scripted cuts use the public queued terrain-edit path. The seeded cuts are three
cells wide so diagonal cuts form a continuous gap. They cut the retained
planet component through a seeded occupied cell, then ordinary connectivity
processing decides which pieces detach. The grid cut deliberately creates a
large burst of contacts. Nothing in the runner caps, deletes, or resets fragments.

Player 1 also holds the cannon and Player 2 holds the laser. Players can die and
bases can fail normally. Scheduled shells continue independently so the workload
does not become idle after an early death. Match victory pauses are bypassed in
the runner and logged; each requested physics tick must still execute. These are
controlled engine workloads, not terrain-aware bot matches or realistic weapon
balance tests.

## Measurements and checks

Each tick measures the whole scenario step and construction of all four normal
local-play draw lists. Each one-second window records P50/P95/P99/max and counts
ticks whose **step + draw-list construction exceeds 16.67 ms**. It also retains
terrain lifecycle and physics timings, peak contact pairs, and peak active bodies.
Run summaries aggregate individual tick measurements, not averages of percentiles.

The timer excludes workload scheduling, audits, motion-history capture, snapshot
conversion, report I/O, rasterization, and presentation. Audit, scheduling,
motion-history, and snapshot costs are reported separately. Timing overruns do
not fail correctness checks and are not proof of actual dropped display frames.
Use the kiosk for display/controller
validation; this tool measures simulation and draw-list production.

Every simulated second, including the final tick, the runner checks:

- Remaining occupied cells + intentionally removed cells = the initial total.
- Cached terrain hashes and chunk revisions match the material fields, and the
  rectangle cover accounts for the occupied-cell count.
- Terrain body and collider identities match the retained planets and fragments,
  including expected service sensors; missing and stale identities are errors.
- Detached pieces are nonempty and have finite positive mass.
- Base support agrees with footing material, and unsupported bases have no stale
  docking contact, held-ship marker, or rebuilding progress.
- Every rigid body has finite position, angle, linear velocity, and angular
  velocity; ship motion and health are finite too.

Tick advancement is checked every tick. Material hashes and sorted body-motion
hashes are recorded every second to compare repeated runs of the same build.
They are diagnostic fingerprints, not a save format or a cross-machine lockstep
guarantee. Identity checks do not compare every numerical collider vertex.

The runner also enables an opt-in rigid-body motion trace, disabled in ordinary
gameplay. Every tick it captures all rigid bodies before and after terrain edits,
before and after gravity, after Rapier, and after collision/gameplay processing.
Each frame records identity, position, angle, center of mass, velocity, spin, and
mass. A rolling 360-frame buffer holds up to one second at 60 Hz.

The first non-finite motion value, or finite speed large enough to traverse the
world's diameter in one 60 Hz physics tick, freezes that history and records the
body, tick, and stage in `report.json`. Non-finite motion fails the run; high speed
is a separate anomaly flag, not a gameplay speed limit. Peak speed and absolute
spin continue to be measured across all six stages for the rest of the run.
This catches short pre-collision spikes that the one-second audits could miss.
The JSON schema is version 2; `motion_anomaly` is null when no trigger occurred.

A correctness failure stops that case, preserves its samples and error, and lets
the remaining cases run. A panic in workload scheduling, stepping, or drawing is
also recorded with the attempted tick. The process exits nonzero if any case
fails or exhausts its wall-clock budget. Budget exhaustion preserves a final
partial-second sample and snapshot, and is reported separately from failed
correctness checks. The budget includes audit/export overhead and is checked
between ticks; it cannot interrupt a physics call that never returns.
The CSV flushes every simulated second; HTML and JSON update after each
case, so completed runs remain available during the matrix. A process kill or
out-of-memory abort can still interrupt the current case before its report is
written.

## First extended baseline (2026-09-07)

The first matrix used 180 simulated seconds and a 60-second wall budget per run:
four workloads × seeds 42, 1337, and 9001 on the desktop 9800X3D, plus the four
seed-42 workloads on a Pi 5 alongside its active kiosk. Seven of twelve desktop
runs and one of four Pi runs completed the full simulated duration. The other
eight exhausted their wall budget. Together they recorded 139,120 physics ticks,
or about 38.6 minutes of simulated time.

All recorded material-accounting, cache, identity, support, and finite-state
audits held. Thirteen runs raised the separate high-speed anomaly flag. This is
an engine behavior to investigate: finite state and conserved material do not
establish physically plausible motion after fragmentation.

| Pi workload, seed 42 | Simulated seconds reached | Step P95 / max (ms) | Peak fragments | Outcome |
| --- | ---: | ---: | ---: | --- |
| Cannon | 180.000 | 4.841 / 78.280 | 18 | Complete; speed flag |
| Excavation | 100.633 | 9.131 / 1851.628 | 29 | Wall budget; speed flag |
| Fragments | 99.750 | 14.654 / 216.033 | 92 | Wall budget; speed flag |
| Multi-planet | 87.067 | 23.116 / 36.109 | 36 | Wall budget |

Incomplete runs cover different time windows, so compare matching prefixes in
the CSV before treating their aggregate timings as equivalent workloads. The
report's charts and event log locate the onset of the slow or unusual behavior.

The [follow-up runaway investigation](terrain-runaway-investigation.md) traces
the speed spikes to point-source gravity near excavated planet centers, measures
the resulting CCD cost, and records controlled tests of a bounded interior field.

## Bounded planet gravity validation (2026-09-07)

The bounded field described in [Spacewars Terrain](spacewars-terrain.md) completed
four workloads × three seeds × two machines for 180 simulated seconds each:
**24 complete runs, 259,200 physics ticks, and 72 simulated minutes**. Every
recorded audit passed. No motion-anomaly trigger fired at any of the six observed
stages. The largest recorded speed was 1,599.51 world units/s, and the largest
absolute spin was 892.57 rad/s. Finite motion and conserved material do not prove
every remaining spin or contact is physically plausible.

Release builds used Rust 1.94.1 on a desktop 9800X3D and Pi 5. The Pi ran alongside
the active Surface Expedition kiosk build. These are simulation timings, not
full-client frame times, and the background kiosk differs from the first
baseline. Each P95 range below spans the three seeds; the maximum is the worst
individual step across those seeds.

| Workload | Desktop step P95 range (ms) | Pi step P95 range (ms) | Pi worst step (ms) |
| --- | ---: | ---: | ---: |
| Cannon | 0.25–0.40 | 0.74–0.91 | 2.23 |
| Excavation | 1.67–2.36 | 6.08–6.93 | 12.15 |
| Fragments | 2.50–2.90 | 6.16–7.19 | 10.17 |
| Multi-planet | 8.54–24.43 | 29.34–100.25 | 139.74 |

All single-planet steps plus four draw lists stayed below 16.67 ms on both
machines. Dense multi-planet physics remains too expensive. For example, the
slowest sampled Pi second for seed 1337 has a 107.35 ms physics P50 and 1,551
contact pairs, while lifecycle P50 is 0.011 ms. Profiling that remaining physics
cost is the next performance task; the unavailable aggregate CCD timer cannot
attribute it by itself.

The initial Pi matrix was deliberately stopped during multi-planet seed 1337 to
raise the wall budget for the two remaining seeds from 180 to 600 seconds. Its
ten completed runs and partial CSV are preserved. Fresh runs of seeds 1337 and
9001 completed their three simulated minutes in 483.17 and 429.90 wall seconds.
The table and 72-minute total include only the 24 completed cases.

The measured build initially enabled bounded gravity for every planet. That
also changed the ordinary AI replay baselines. The final rollout enables the
field when a planet opts into material terrain; ordinary planets retain their
existing field. All endurance planets have material terrain throughout these
workloads, so their gravity law is unchanged by this restriction. Final binaries
replayed the first seven seconds of all 12 cases on each machine: all 96 material
and body-motion samples per machine matched the measured runs exactly.

Final validation also passes all 838 workspace tests on Rust 1.89, the ten window
lifecycle checks (including both terrain renderers), and the unchanged six-episode
navigation and twelve-episode strategy baselines. Formatting passes; Clippy
reports existing warnings. The terrain screenshot check now waits for a matching
rendered frame, since gameplay UI state can precede its presentation.

Raw reports are archived in the local `terrain-endurance-20260907/bounded-gravity`
artifact directory: `desktop`, `pi-initial`, and `pi-long-wall`. Final replay
reports, hash comparisons, both source revisions, build hashes, and
`analyze.py` accompany them. The script verifies complete coverage, material
accounting, anomaly status, and replay hashes before writing `summary.json`.

## Combined Expedition acceptance runs (2026-09-07)

The `surface_material_soak` example validates the integrated `spacewars-terrain`
gameplay separately from the historical stress matrix above. Three optimized
runs on each of desktop and Pi completed 180 simulated seconds each (18 minutes
total), using
only player actions: natural landing, exit, claim, mine the flag footing,
neutralization, reclaim, deliberate empty-ship loss, rebuild, board and depart.
After departure the pilot brakes and the simulation continues, including a
return to the surface. Every second audits material accounting, finite motion
and body/collider ownership. The other ship stays simulated in two-player cases.

| Players / driven seat | Seed | Journey complete on both (sim s) | Desktop P95 / worst (ms) | Pi P95 / worst (ms) |
| --- | ---: | ---: | ---: | ---: |
| One / P1 | 42 | 21.05 | 0.052 / 0.224 | 0.125 / 0.472 |
| Two / P1 | 7 | 21.05 | 0.050 / 0.160 | 0.146 / 0.482 |
| Two / P2 | 42 | 21.22 | 0.054 / 0.170 | 0.158 / 0.537 |

Each run removed four cells, neutralized the planet once and reclaimed it,
rebuilt one ship and boarded it without duplicating the pilot. All 1,080 sampled
audits passed; initial material equals remaining plus removed cells. Sampled
peak body speed was 41.75–51.51 world units/s. These light gameplay journeys
created no detached fragments; they do not replace the heavy fragmentation
matrix or establish Pi rendering performance. Detachment and simultaneous
claimants are covered by separate integration tests.

Combined validation passes 967 workspace/all-target tests on Rust 1.89, with
17 opt-in UI cases excluded from that command and run separately. All 17 real
window workflows passed during integration, including both renderers and
two-player terrain settings/lifecycle. The six-episode navigation and
twelve-episode strategy baselines match. Formatting passes and Clippy completes
with existing warnings. The ARM64 release journey runner compiles successfully.

An initial two-player attempt stalled after rebuilding. Investigation found
that a single hatch ray could miss usable ground beside a material-cell edge.
The hatch now checks one cell on either side while retaining the same bounded
downward reach. Regression tests verify both a narrow cut with surviving nearby
footing and a wide, deep excavation that must block exit. The scripted pilot
also uses ordinary jump input when walking catches on a ledge. No actor poses,
ownership values or character physics are forced by the journey runner.

The timing wraps the entire `SurfaceSortieScenario::step`, including material
commit preparation; report serialization and rendering are excluded. Desktop
uses Rust 1.89 release mode. The ARM64 runner uses Rust 1.94.1. Pi runs kept the
existing 60 Hz kiosk running alongside the headless runner.
Earlier Pi stress results above apply to the historical workload and binary.

The combined image was built from `21bff9f`, using the accepted Pi image's
Yocto layer revisions, and deployed to `spacewars.local` on slot A (`/dev/sda2`).
The installed engine-client SHA-256 matches the read-only extraction from the
archived image:
`6fb1249ebc083646df8d9c93b21212cc71fbfc2c581abf01182e9411917090dc`.
The live `spacewars-terrain` scene reports 60.1 FPS/UPS at raster scale 2.00,
with no service restarts. Its inspected 800×480 screenshot shows both ships
landed on material ground without a pad; the existing two-player setting was
preserved. This is a startup/idle rendering check, not a fragmented-world GPU
benchmark or a substitute for controller playtesting.

Reports and validation records are in the local
`surface-terrain-integration-20260908` artifact directory. `desktop` and `pi`
contain the final three runs per platform; earlier diagnostic attempts are
retained separately and are
not counted as completed acceptance runs. The example saves observations with
each per-second audit so failed journeys remain inspectable.

## Exploring the historical report

Choose a run from the comparison table. Scrub time or click a chart to inspect a
specific second. The world view uses the latest snapshot at or before that time;
its timestamp is shown explicitly. Snapshots are saved at zero, every five
seconds, and the final second. Playback advances ten simulated seconds per wall
second through these snapshots; it is not a full-frame replay.

The camera selector switches between the whole world and a close view of each
planet at its recorded position. Charts show when contacts and fragment counts
rise, how that corresponds to simulation cost, and whether destruction continues
late in a run. The event log records shell targets, cuts, base losses, ship losses,
and victory-pause bypasses. `samples.csv` holds the per-second measurements;
`report.json` additionally contains full summaries, events, and snapshots.

Useful comparisons include early vs late timing, multiple seeds of the same
workload, and the same workload on desktop and Pi. Same-build repeatability is
tested. Different architectures can produce different collision cascades, so
compare conservation and workload evolution as well as timing.

## Regression checks

CI's existing `cargo test --workspace --all-targets` includes the fast harness
unit tests. The full endurance matrix is an explicit run, separate from CI.

```sh
cargo test --locked --release -p scenario-spacewars \
  --lib --example spacewars_terrain_soak
cargo test --locked -p engine-rapier
```

Focused tests deliberately remove/add terrain physics bodies to prove that the
audit detects broken ownership, check valid splitting and empty-planet cleanup,
and repeat a seeded run across an actual scripted cut. They also inject stale
material caches and non-finite state, and verify that an exhausted wall budget
retains partial evidence. Report tests cover draw order and escaping, and CLI
tests enforce the supported duration range.
