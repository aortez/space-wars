# Rounded terrain: physics phase profile

This follows the [generated-match comparison](rounder-planets-matches.md) and
the [mission routing fixes](mission-route-clearance.md) at `e625b2e`.
The dominant cost is maintaining and searching the collision candidate graph.
The next experiment should group each terrain chunk's existing convex pieces
into a compound collider, preserving the material and visible boundary.
This checkpoint adds profiling and fixes timing accounting; it makes no
collision representation or gameplay changes and claims no speedup.

Follow-up: [chunk compound implementation and measurements](terrain-chunk-compounds.md).
The results below remain the original profiling checkpoint.

## Measurement

The runner now accepts `--profile-physics true`. It records the existing shared
world and Rapier phase timers after each step, plus body and contact populations.
Once per simulated second, and at a finished round, it scans the final
narrow-phase contact graph to distinguish same-body candidate pairs from other
pairs. This optional scan and report bookkeeping are outside the timed step.
Sensor intersection pairs are not included in these candidate counts.

The timings have explicit scope:

- `world_*` and `sortie_outside_world` partition the measured scenario step.
  Lifecycle includes terrain preparation; workload includes post-Rapier event
  collection and other shared-step work. The residual covers the surrounding
  sortie's input, support, landing, claim and recovery work.
- `world_physics` measures the raw Rapier step. The `rapier_*` timers are nested
  within it. Broad phase, final broad-phase update and narrow phase belong to
  collision detection; adding parent and child timers double-counts work.
- Rapier 0.34 does not provide complete CCD timing. The report keeps it `null`.
  No zero-cost CCD conclusion should be drawn.
- The existing workload residual previously subtracted terrain preparation even
  when the sortie had performed it before the shared-step clock started. The
  accounting now includes that external preparation time before subtraction.
  This changes reported timing only.

Eight release runs use the smallest and largest initial rounded collider counts
from the previously selected eight-world survey:

| World | Seed | Initial block / round terrain colliders |
| --- | --- | ---: |
| Small | `10897516310764597894` | 247 / 3,027 |
| Large | `3121799525250095703` | 1,457 / 11,434 |

Each world runs both surfaces with asteroids off and Mixed asteroids at a
three-second mean interval. Both seats use `material_mission_v9`, normal 15/4
combat breaks and finished-match rules. Every run reaches its 60-second budget
(3,600 ticks). Blocks and round use the same current code; these are new timings,
not a comparison against the older bot in the original report.

Runs are serial on the shared desktop, with no simultaneous builds or other
benchmark jobs from this task. Small-world pairs run blocks first; large-world
pairs run round first. Unrelated desktop activity is uncontrolled. These are
headless CPU-operation timings, excluding sensors/policy, audit, output and
display work. They do not establish cabinet FPS. No Pi was deployed or measured.

An initial eight-run pass preceded the optional pair scan. The final pass below
repeats those cases with that scan enabled; every non-timing report field agrees.
Those repetitions check the diagnostic, rather than constituting a statistically
controlled repeated benchmark. Surface changes themselves produce different
trajectories, contacts and edits, so a block/round pair is not identical work.

## Results

| Case | Mean step, blocks / round (ms) | Step P95, blocks / round (ms) | Round collision detection (ms) | Round solver (ms) |
| --- | ---: | ---: | ---: | ---: |
| Small, asteroids off | 0.041 / 0.679 | 0.051 / 0.736 | 0.609 | 0.0009 |
| Small, Mixed / 3 s | 0.048 / 0.739 | 0.062 / 0.821 | 0.662 | 0.0012 |
| Large, asteroids off | 0.281 / 3.489 | 0.318 / 3.877 | 3.244 | 0.0011 |
| Large, Mixed / 3 s | 0.311 / 3.665 | 0.349 / 4.212 | 3.365 | 0.0100 |

Collision detection accounts for **89.6–93.0%** of the rounded scenario step.
The broad phase and final broad-phase update together account for **79.4–83.5%**.
The solver is **under 0.3%**. For the large world without asteroids, the 3.244 ms
collision-detection total comprises 2.589 ms broad phase, 0.326 ms final
broad-phase update, and 0.329 ms narrow phase.

The candidate graph explains why very few real contacts still cost time:

| Rounded case | Mean candidate pairs, every tick | Mean active contact pairs, every tick | Same-body share of sampled candidates |
| --- | ---: | ---: | ---: |
| Small, asteroids off | 15,550 | 0.15 | 79.92% |
| Small, Mixed / 3 s | 15,852 | 0.21 | 79.83% |
| Large, asteroids off | 70,534 | 0.43 | 83.73% |
| Large, Mixed / 3 s | 71,181 | 9.88 | 83.65% |

The same-body percentage divides summed same-body counts by summed total counts
over 60 one-second samples per case. It is not an every-tick measurement or a
percentage of CPU time. A candidate is an overlapping bounding-box possibility,
not evidence that solid material is colliding with itself.

All 16 profiling runs pass the physical/material audit. The final pass has no
removed material in the no-asteroid cases; pressure cases remove 153–306 cells.
Only one detached fragment is present at the end of either large pressure case.
This is not a fragmented-world endurance measurement. Five of 14,400 rounded
step samples exceed 16.67 ms, compared with zero of 14,400 block samples; isolated
elapsed-time outliers can include desktop scheduling.

## Mechanism and next experiment

`TerrainAssembly::synchronize` currently inserts every merged rectangle and every
owned convex patch as a separate Rapier collider on its terrain body's rigid body.
An intact large world has 9,984 polygon colliders plus 1,450 rectangles. Rotating
and orbiting planets continually move this collection through the broad phase.

In the locked Rapier 0.34 source, `geometry/broad_phase_bvh.rs::update` maintains
overlapping collider pairs without checking their rigid-body parents. Later,
`geometry/narrow_phase.rs::compute_contacts` traverses the candidate graph and
clears contacts when both colliders have the same parent. That matches the
measured same-body populations. Ordinary collision filters run after the broad
phase has produced its pairs, so adding such a filter would not remove the main
measured search/bookkeeping work.

The recommended bounded prototype is **one compound collider per nonempty
terrain chunk**, using the existing rectangles and convex patches as children.
This would reduce the world-level collider and pair count while retaining exact
boundary vertices. It remains a hypothesis: compound shapes add internal search
and contact work, and their measured benefit is not yet established.

Before applying that representation to matches:

1. Add compound shape support to the physics adapter, with a comparison path
   retaining today's separate colliders. Keep chunk rebuilds local and preserve
   unchanged handles on durability-only edits. Avoid nested compounds when
   building the child list.
2. Compute compound mass, center and inertia from owned material cells, including
   merged interiors. Zero-mass extra patches must stay zero mass. Preserve point
   velocities through cuts, moving centers of mass and fragment separation.
3. Verify ray hits, capsule/hatch clearance, tiny remnants, rotated local support
   coordinates, landing feet, resting contacts and transient CCD impacts. Check
   chunk seams and newly opened holes explicitly.
4. Review contact-event identity and damage aggregation. Today events are
   deduplicated per collider pair; many patches becoming one collider can change
   which impulses or contact locations gameplay sees. The local support iterator
   already understands manifold subshape transforms, but that alone is not proof
   that all contact consumers remain correct.
5. Update diagnostics that currently assume one collider ID per geometry piece.
   Keep separate geometry-piece and physical-collider counts, and continue
   auditing material coverage and stale body/chunk identities.
6. Compare old/new rounded representations in an identical-input physics fixture
   first, then this eight-case profile, then the existing three-minute mission,
   recovery, weapon and asteroid-pressure acceptance tests. Do not change their
   deadlines. Rendering geometry can remain as it is during this experiment.

An earlier same-body rejection inside Rapier is another possible investigation,
but would require backend changes and care around reparenting. Its own tests
retain a same-body candidate so it becomes a real contact after reparenting.
The chunk prototype stays within the engine's terrain adapter and also reduces
the per-collider transform/index updates visible in the final broad-phase timer.

## Validation and reproduction

- Release physics and scenario library suites: **414 passed, zero failed**,
  including a new diagnostic test separating same-body pairs from active
  cross-body contacts and proving the scan leaves the snapshot unchanged.
- All eight initial/final profiling report pairs match every non-timing field.
- For both selected rounded worlds with pressure, 60-second replays with profiling
  off and with the archived `e625b2e` runner match every non-timing field from the
  profiling-on run, including motion hashes, events, damage, visits and outcome.
  These four replay timings overlap validation work and are not benchmark data.
- Formatting and diff checks pass. The runner Clippy check has the seven existing
  scenario-library warnings and no new warnings. The full workspace suite was
  not repeated for this instrumentation-only change; its previous checkpoint is
  recorded in the routing notes.

```sh
cargo build --locked --release -p spacewars-ai --example surface_mission_soak
target/release/examples/surface_mission_soak \
  --world generated --seed 3121799525250095703 --seat 0 \
  --mode duel --match true --seconds 60 --asteroid-interval 3 \
  --surface round --profile-physics true --out /tmp/rounded-physics-new-run
```

Use a fresh output directory. Repeat for both seeds, surfaces `blocks` and `round`,
and asteroid intervals `0` and `3`, serially after the build completes. Phase
timings, populations and pair samples are under `physics_profile` in `report.json`.
Omitting the flag leaves that field null and avoids the optional scans/storage.

Local evidence is archived under:

```text
/home/oldman/.codex/visualizations/2026/09/11/rounder-planets/physics-profile/
```

It includes `baseline/` and `final/` reports, commands and logs; archived runners
and their hashes; source patches and the separately captured new profile helper;
`run-profiles.py`; `summary.json`; `profile-replay-checks.json`; and the four
`replay/` comparisons with `check-replay.py`. The `baseline` label denotes the
first instrumentation pass, not a gameplay or physics optimization baseline.
