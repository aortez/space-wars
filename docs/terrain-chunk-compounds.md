# Chunk compound terrain colliders

This implements the bounded experiment proposed by the
[rounded-terrain physics profile](rounder-planets-physics-profile.md).
The candidate-graph cost existed with block terrain. Rounded boundaries added
many more convex pieces, making that existing cost much larger.

## Representation

`TerrainAssembly` now defaults to one physical collider per nonempty material
chunk. The existing merged rectangles and convex patches become its children;
a chunk with one piece keeps an ordinary collider. `TerrainColliders::Separate`
retains the previous layout for comparisons. Planet fragments inherit the
chosen layout. Material cells, contour construction, mining sizes and cached
render geometry are unchanged.

Each compound carries the mass, center and inertia of its owned material cells.
Merged rectangles contribute their full area; boundary cells contribute once;
additional contour patches contribute no extra mass. The compound sits at its
material center. Convex child vertices are recentered locally to avoid resolving
small surface patches in planet-sized coordinates. Cuts still rebuild only
changed chunks, refresh material mass and preserve point velocities. Damage
without removal preserves collider handles.

A compound is not one chunk-wide damage event. Contact events retain the real
physical collider identities and add optional child identities. Aggregation and
CCD deduplication use the child pair, preserving separate impact locations and
impulses. Surface contacts already apply manifold child transforms. Diagnostic
coverage now counts physical colliders separately from cached geometry pieces.

## Prescribed motion during CCD

The first prototype reduced collision overhead but failed three long-running
mission tests. Traces showed moving planets suddenly reporting zero velocity
while nearby CCD impacts subdivided a step, throwing off surface support.
Recentering child vertices alone did not resolve those failures.

A minimal regression reproduces this with a plain cuboid, independently of
terrain or compounds. In the locked Rapier 0.34 implementation,
`PhysicsPipeline::interpolate_kinematic_velocities` runs inside each CCD substep.
It interpolates a position-based body's full target using that substep's duration.
The first substep can spend the full prescribed move, leaving zero velocity for
the remaining substeps. The regression catches this on its second tick with a
24-unit/second translating platform and an unrelated rebounding CCD ball.

The adapter now temporarily integrates moving position-based kinematics using
their full-step center-of-mass velocity and angular velocity. After the one
shared solve it restores position-based control and the exact prescribed target,
refreshing collider poses and broad-phase bounds. Restoring the exact endpoint
prevents accumulated floating-point drift from changing next-step velocity.
Dynamic actors are not repositioned. The CCD substep budget is unchanged.
A regression also covers simultaneous translation/rotation with an offset center
of mass, actual CCD subdivision and snapshot replay.

This is a physics correction in addition to the collider optimization. Gameplay
trajectories are therefore not expected to match the old executable bit for bit.

## Acceptance coverage

The final release workspace/all-targets run passes **1,293 tests**, with zero
failures and 41 existing ignored tests. This includes all 20 mission tests,
three-minute asteroid duels, capture/boarding/departure, recovery, weapons,
material/fragment audits and snapshot checks. Formatting and diff checks pass.
The release Clippy check of all changed packages/targets reports only existing
warnings in unchanged scenario code (seven library and seven test warnings).

New paired adapter tests cover block, contour and interpolated terrain before
and after cuts: mass, center of mass, impulse response, ray distances, capsule
clearance and durability-only handle stability. Ray normals are compared away
from shared vertices, where either adjacent face can legitimately win a BVH tie.
Solid rays starting inside material have no boundary normal.

Separate/compound contact fixtures cover two simultaneous child hits, both
sides being compounds, local impact coordinates, snapshot replay and CCD impacts
that disappear from the final contact graph. Invalid empty, nested, polyline or
nonfinite compounds are rejected atomically.

Existing mission deadlines and outcome requirements remain in place. The
jetpack/recovery fixture needed a more explicit setup after the motion correction:

- All three original recovery starts remain and must capture, rebuild and depart
  within the existing three-minute budget. The original close P2 wreck now leaves
  a walkable route, so it no longer requires an unnecessary jetpack flight.
- An additional P2 start at a larger offset requires a real hull crossing.
  The other two starts still require crossings. The separate ordinary navigation
  test still requires flight both outward and back, including terrain edits.
- Seed 7 no longer happens to settle its pod sideways. A dedicated scenario
  fixture places a pod sideways initially, waits for actual grounded eligibility,
  then verifies that the shared thrust action lifts it and breaks ground contact
  on both block and rounded material. Existing bot tests verify the righting
  action/release decision. This replaces an accidental-bounce assumption with
  physical lift coverage; it does not demonstrate a bot righting that old seed.

## Measurement design

Two complementary comparisons avoid attributing different gameplay workloads to
collider layout alone.

The new `terrain_collider_benchmark` copies the actual generated material fields
for the same small and large seeds used by the previous profile. For each block
and rounded surface it applies identical prescribed translation, rotation and
39 cuts over 3,600 ticks to separate and compound layouts. Every edit's material
hash, cell count and cached shape count, plus final material hashes, must match.
Prescribed body endpoints are checked every tick. There are no actors, gravity,
bots, connectivity splitting or rendering in this fixture. Its whole-step timer
includes adapter work, backend metrics and contact collection. Synchronization
timing excludes material editing and geometry regeneration.

The generated-match comparison uses two seeds, two surfaces, asteroids off or
Mixed at a three-second mean interval, and three executables/configurations:

1. Archived `3b29507` baseline: separate colliders and original kinematic handling.
2. Current separate layout: the kinematic correction without compounds.
3. Current compound layout: both changes.

Each of the 24 runs uses two bots, ordinary match rules, profiling and draw-command
measurement, with a 60-second simulation budget. Small cases run baseline first;
large cases reverse the order. Runs are serial, with no builds or other benchmark
jobs from this task during measurement. Unrelated desktop activity is uncontrolled.
These are single-run desktop CPU timings, not cabinet FPS or statistical estimates.
The complete scenario step includes the kinematic adapter work; the nested raw
Rapier timer excludes it. Measured tick additionally includes sensors, policy and
draw-command generation, but excludes display/presentation and audit/output work.

## Results

All 24 generated-match runs reach 3,600 ticks and pass physical/material audits.
All eight isolated fixture runs pass identical-work and exact endpoint checks:
four surface/seed pairs, with 39 matching edits per pair. Rounded matches reduce
mean scenario-step time by **14–57× versus the archived baseline** in these cases.
Mean measured tick improves by about **3–7×** for rounded cases; this is still a
headless CPU-operation measurement.

| Case | Step mean, baseline / fixed separate / compound (ms) | Compound step P95 (ms) | Mean tick, baseline / compound (ms) | Compound tick P95 (ms) |
| --- | ---: | ---: | ---: | ---: |
| Small, blocks, off | 0.042 / 0.059 / 0.022 | 0.032 | 0.132 / 0.093 | 0.274 |
| Small, blocks, Mixed / 3 s | 0.049 / 0.067 / 0.027 | 0.038 | 0.150 / 0.101 | 0.293 |
| Small, round, off | 0.691 / 1.018 / 0.028 | 0.045 | 0.976 / 0.328 | 1.896 |
| Small, round, Mixed / 3 s | 0.742 / 1.081 / 0.032 | 0.044 | 0.987 / 0.233 | 0.234 |
| Large, blocks, off | 0.293 / 0.460 / 0.052 | 0.062 | 0.417 / 0.150 | 0.173 |
| Large, blocks, Mixed / 3 s | 0.324 / 0.492 / 0.066 | 0.082 | 0.436 / 0.170 | 0.202 |
| Large, round, off | 3.478 / 5.007 / 0.061 | 0.079 | 3.985 / 0.535 | 0.610 |
| Large, round, Mixed / 3 s | 3.681 / 5.905 / 0.265 | 0.511 | 4.185 / 0.747 | 1.022 |

| Fixture | Step mean, separate / compound (ms) | Ratio | Sync mean, separate / compound (ms) |
| --- | ---: | ---: | ---: |
| Small, Blocks | 0.072 / 0.004 | 16.1× | 0.015 / 0.008 |
| Small, Interpolated | 1.848 / 0.004 | 412.0× | 0.415 / 0.231 |
| Large, Blocks | 0.515 / 0.028 | 18.6× | 0.034 / 0.011 |
| Large, Interpolated | 7.244 / 0.028 | 260.2× | 0.623 / 0.238 |

The isolated fixture's much larger ratios measure moving terrain maintenance
without actor contacts or gameplay. They are not expected match speedups. Its
separate and compound configurations both include the kinematic correction.
Synchronization also improves in all four fixture pairs, despite building an
internal compound BVH when chunks change.

Initial terrain collider counts are 247 → 20 for small blocks, 3,027 → 20 for
small round, 1,457 → 162 for large blocks, and **11,434 → 162** for large round.
Mean large-round candidate pairs fall from about **70,534 → 699** without
asteroids and **71,181 → 716** under pressure. These are physical collider-pair
counts; an active compound pair can contain multiple child contacts.

The correction alone makes separate-collider steps more expensive: it restores
prescribed endpoints and refreshes many individual collider bounds. Keeping the
old executable in the comparison shows that compounds still produce a substantial
net improvement over the pre-change build, not just over that extra work.

Not every timing percentile improves. Small round without asteroids changes
measured-tick P95 from **0.998 to 1.896 ms**, despite the much cheaper scenario
step. Its sensor mean rises from 0.073 to 0.086 ms per observation and policy mean
from 0.00035 to 0.00302 ms. The compound run reaches a capture and departure near
the end, while the baseline is still approaching/retrying; this is consistent
with a changed survey/planning workload, not proof of an isolated query regression.
Its median tick falls from 0.881 to 0.186 ms and P99 from 3.393 to 2.106 ms.
Preserve this case when profiling sensor work in the future rather than claiming
uniform improvement across all percentiles.

The final compound pressure cases remove 153–284 material cells. Only the two
large cases end with a detached fragment, one each. This short measurement is not
a heavily fragmented-world endurance run. No compound scenario step or measured
tick exceeds 16.67 ms in this pass; the baseline has five such samples, all in
large rounded cases. Shared-desktop scheduling can contribute to outliers.

The change is enabled by default locally. The next practical check is cabinet
playtesting of moving-ground landing, mining, hull crossings and fragment impacts.
Future optimization should re-profile the remaining sensor/draw work and the
large-round pressure case; the original broad-phase population is no longer the
same dominant workload. Rapier's CCD timer remains incomplete, so the pressure
case's residual raw-solve time cannot be attributed precisely from these counters.

## Reproduction

```sh
cargo test --locked --release --workspace --all-targets --no-fail-fast
cargo build --locked --release -p scenario-spacewars --example terrain_collider_benchmark
cargo build --locked --release -p spacewars-ai --example surface_mission_soak

target/release/examples/terrain_collider_benchmark \
  --seed 3121799525250095703 --seconds 60 --out /tmp/terrain-compound-large

target/release/examples/surface_mission_soak \
  --world generated --seed 3121799525250095703 --seat 0 \
  --mode duel --match true --seconds 60 --asteroid-interval 3 \
  --surface round --terrain-colliders compound \
  --profile-physics true --measure-draw true --out /tmp/match-compound-large
```

Use fresh output directories. Repeat the fixture with seed
`10897516310764597894`; use `--terrain-colliders separate` for the new executable's
comparison path. The archived baseline does not accept that switch.

Artifacts are kept outside the repository at
`/home/oldman/.codex/visualizations/2026/09/11/rounder-planets/chunk-compounds/`.
They include the original executable, failed prototype traces, final executables,
source patch, command lines and JSON reports. No Pi was deployed for this work.
