# Bounded pools and spills

First delivery for Clock water (#55): reusable water accounting and motion,
Meltdown integration, deterministic fixtures, visual preview, and benchmarks.
The second slice adds opt-in one-way Rapier buoyancy. The third adds a deliberately
narrow displacement experiment: a prescribed box entering/leaving a closed tank.

## Model and boundaries

`engine-water` owns fixed-down, 2D unit-depth water. Volumes therefore have units
of world area; the Clock converts one melted square into its actual area.
It knows nothing about digits, event deadlines, rendering, or Rapier.

Each pool has regularly spaced columns with explicit bed elevations, water
amounts, and horizontal face velocities. Surface-level differences accelerate
flow; damping removes motion. Dry barriers prevent flow through higher beds.
Simultaneous donor limits keep transfers nonnegative and conservative. This is
a bounded height-column approximation, not a full shallow-water/Navier–Stokes
solver. Closed edges retain water; open edges have explicit spill lips.

Overflow becomes finite ballistic parcels. They accelerate downward, deposit
into the first pool surface crossed, or leave through the world lower boundary.
An upper pool can feed a separate lower pool. Parcels are not colliding particles
and do not simulate splashes, mixing, pressure, or breaking waves. On capacity
exhaustion, outflow is held in the pool; it is never silently deleted.
An optional vertical channel stops parcels' horizontal motion at its walls.
Normal Meltdown uses this for the central drain, matching the drawn banks;
the open collecting-pool fixture does not. The channel is not a general solid
collision system, and ballistic parcels still pass through one another.

The accounting contract is:

`injected = pooled + in-flight + drained + explicitly reclaimed`

Meltdown reclaims remaining water gradually during its 90-tick reform phase and
reports that separately from drainage. It must not claim that cleanup water
passed through the outlet. Unlike the old forced drain current, a flat basin
does not necessarily empty in the Clock's seven-second material window.

Pools reserve all column/face scratch storage at construction; stepping allocates
no additional buffers. Limits are eight pools, 512 columns total, and at most
512 parcels (128 in Clock). Each caller step accepts `(0, 1/30]` seconds and is
split into substeps no larger than 1/240 second. Speeds and donor withdrawals
are limited; these are stability/work bounds, not an accuracy guarantee at
arbitrary depths or scales. Geometry and source inputs must be finite and within
the API bounds; rejected source additions leave accounting unchanged.
Pool work is linear in column count. Parcel collection scans candidate columns
along each swept path in each pool; both pool and parcel counts are bounded.

`Pool::columns()` exposes the same bed, surface, liquid volume and separate body
occupancy used by simulation.
`WaterWorld::sample(point)` queries occupied water and its local horizontal
velocity. Closed boundaries retain unlimited height (not finite-height walls).
Parcel deposition transfers volume but does not impart impact momentum to a
pool. These are explicit limitations for future body/wave coupling.

## Integration and verification

- Render pool surfaces from the same geometry used for flow/deposition.
- Render falling parcels as short ribbons: increased falling speed lengthens
  them and reduces their cross-section for the same transported amount.
  Clip the visible ribbon against Clock's channel walls too; physics tracks its
  center and volume, not a finite-width collision shape.
- Keep the ordinary Meltdown lifecycle, time recovery, and controls.
- Provide an environment-only collecting-pool preview, not a launcher scenario.
- Test closed-pool equilibrium, unequal beds, spill flight/collection, capacity
  backpressure, invalid inputs, replay, cleanup, and volume conservation.
- Benchmark water stepping separately from full Meltdown stepping and draw-list
  construction. Hardware performance claims require device measurements.

### Running the fixtures

```sh
cargo test --locked -p engine-water -p scenario-clock
cargo run --locked --release -p engine-water --example water_benchmark
cargo run --locked --release -p scenario-clock --example meltdown_benchmark
SPACEWARS_CLOCK_ARTIFACTS=/tmp/clock-water-captures \
  cargo test --locked -p engine-client --bin engine-client meltdown_
SPACEWARS_CLOCK_WATER_LAB=1 cargo run --release -p engine-client -- --scenario clock
```

For the last command, choose **Clock Controls → Preview Event: Meltdown → Preview**.
The test renderer produces normal and collecting-pool PNGs at 800×480, 480×800,
1024×768 and 1280×720. The simulation fixture verifies an upper stepped bed,
observable in-flight water, collection without loss, pause, cleanup and resize.
Normal Meltdown also retains its real-client menu/preview/restart regression.
Tests assert accounting, bounded resources and outcomes, never wall-clock timing.

### Desktop measurements (2026-09-11, Rust 1.89 release)

The water-only workload warms up for 600 ticks, then measures 12,000 steps per
case, adding a small source each tick. Source insertion and statistics are outside
the timer. The cascade has two pools and continuously active spills.

| Total columns | Closed pool step p95 | Two-pool cascade step p95 |
| --- | ---: | ---: |
| 32 | 0.65 µs | 2.39 µs |
| 128 | 3.54 µs | 3.94 µs |
| 512 | 12.92 µs | 10.42 µs |

Peak cascade usage was 103 parcels with no capacity-limited ticks. Maximum
accounting error across the runs was 1.12e-11 world-area units. These fixtures
have different column widths/motion; rows are workload samples, not accuracy
or monotonic scaling guarantees. Scheduler outliers are reported by the tool.

The full Meltdown benchmark runs 24 dense `08:08` events at each of three sizes
(800×480, 480×800, 1280×720). Before this change, step p95 was 0.9 µs and draw-list
p95 4.4 µs. With pools/spills and gradual reform, step p95 was 4.4–4.5 µs and
draw-list p95 6.3–6.5 µs. Peak primitives grew from 416 to 527, with 85 spill parcels
maximum. Up to 29.28–51.71 of the original 88 cell-volumes were reclaimed during
reform rather than drained; this change is intentional and visible in telemetry.

These are local desktop microbenchmarks, not rasterization/presentation timings,
device FPS or Pi CPU measurements. No Pi deployment is part of this slice.

First-slice local checks passed: 13 water tests, 78 Clock tests (three existing extended
Duck sweeps ignored), 51 common/control/CLI tests, and the client unit suite
(295 tests, one ignored). Both Meltdown render fixtures and the real-client
Meltdown workflow were rerun after channel-wall/edge handling was finalized.
Strict scoped Clippy, formatting and a host `pi-kiosk` feature check also passed.
The feature check is not an ARM cross-build or device test.

## One-way buoyancy

`engine-water::immersion::WaterHull` clips a body hull against the occupied water
columns and integrates submerged area, first moments, polar moment, and local
flow. Boxes use exact polygon clipping. Circles use 32 sides with weights
normalized to the circle's true area; partial immersion and rotational drag
remain approximations. It scans only horizontally overlapping columns and
uses fixed stack buffers, with no per-step geometry allocations or Rapier types.
Immersion integrates hypothetical water up to the column surface; it does not
subtract a displacer or compute a union of overlapping solid hulls.

`engine-rapier::buoyancy::BuoyantBody` builds a dynamic box/circle collider and its
matching water hull together. It reads authoritative pose, mass, COM and inertia
from `PhysicsWorld`; callers must not replace the geometry behind the adapter.
This first binding is deliberately one centered shape, not arbitrary compound
colliders or a hidden global registry. Scenarios opt bodies in explicitly.

Lift is `fluid density × downward gravity magnitude × submerged area`, applied
at the submerged area's centroid. Its lever about the COM generates torque.
Distributed drag resists velocity relative to local water flow, including spin.
A mass/inertia-dependent bound limits drag's per-tick dissipative response for
light bodies and supported timesteps. This is not an unconditional stability
guarantee for arbitrarily small bodies and large gravity/timesteps.

The adapter adds **forces and torque**, so gravity and buoyancy act during the
same Rapier integration. A once-per-frame lift impulse initially produced a
small residual velocity at apparent rest; the settling regression caught it.
Callers use the ordinary force lifecycle:

```rust
water.step(dt)?;
physics.clear_forces();
// Add other external forces here too; buoyancy does not clear them.
for body in &floating_bodies {
    body.apply_forces(&mut physics, &water, buoyancy_config, dt).unwrap();
}
physics.step(dt as f32);
```

The return report contains submerged fraction, center of buoyancy, force and
torque. A dry body receives no water force. Water is borrowed immutably: bodies
do not move its surface, displace volume, obstruct flow or generate splashes.
Spill parcels do not push bodies. Solid beds/walls are ordinary Rapier colliders;
the water surface is **not** a collider. Sleeping is not optimized here: wet
bodies receiving forces may remain awake.

Supported setups use downward gravity, unit gravity scale, and non-overlapping
occupied pool regions. Obvious over-counting is rejected, but the adapter does
not solve a union of overlapping basins. Geometry changes, nonuniform/planetary
gravity, moving containers and feedback waves require further work.

### Preview and measurements

The existing `SPACEWARS_CLOCK_WATER_LAB=1` Meltdown preview now adds:

- an orange box at density 0.35 that floats and can ride off the upper ledge;
- a yellow ball at density 0.55 that floats in the collecting pool;
- a red box at density 1.8 that sinks to the solid bed.

Water density is 1.0. The preview uses exactly three dynamic bodies plus one
fixed support body, with ten colliders total. Support rectangles are shared by
rendering and physics. Pause freezes the simulation; event completion, replacement,
resize and restart release the lab. Normal Meltdown still has **zero bodies**.

```sh
cargo test --locked -p engine-water -p engine-rapier -p scenario-clock
cargo run --locked --release -p engine-rapier --example buoyancy_benchmark
cargo run --locked --release -p scenario-clock --example meltdown_benchmark -- --water-lab
```

Desktop Rust 1.89 release, 600 measured ticks after warm-up, mixed fully submerged
boxes/circles spaced apart in a uniform pool:

| Bodies | Coupling p95 | Mechanics p95 |
| --- | ---: | ---: |
| 3 | 4.03 µs | 1.65 µs |
| 100 | 26.94 µs | 8.13 µs |
| 1,000 | 239.35 µs | 62.05 µs |

These are active, non-contact bodies; water stepping/rendering are excluded.
Column width changes with tank width, so this is not a fixed-resolution scaling
study or a dense-collision benchmark. It does not establish Pi performance.

The complete Clock buoyancy preview (24 events at each of three sizes) measured
step p95 7.9–8.6 µs and draw-list p95 4.9 µs, peaking at 398 primitives. The normal
Meltdown recheck remained at 4.5–5.0 µs step p95 and zero physics bodies.

Tests cover area/centroid/moment accuracy, current integration, float density
ratios, sinking, tilt recovery, mass/inertia response, drag energy dissipation,
invalid inputs, dry-body force removal and preserving other force fields.
Settling is checked at 30/60/120 Hz. Clock tests replay the bodies, compare water
against an identical body-free run, and check preview/pause/cleanup across device
aspects. The renderer fixtures capture both paths at all four existing sizes.

Second-slice validation: 18 water tests, 59 mechanics tests, 79 Clock tests and
295 client tests passed. Both normal and buoyancy-lab real-client workflows pass
under Xvfb, including pause, preview, recovery, restart and launcher return. The
host `pi-kiosk` feature check passes; no device deployment was performed.
Clippy passes with only the pre-existing `collapsible_else_if` lint in
`engine-rapier/src/spaceling.rs` allowed; that unrelated source was not modified.

## Closed-tank displacement experiment

`WaterWorld::set_displacer(pool, Some(DisplacementBox { center, half_extents }))`
opts a pool into displacement. `None` removes the occupancy input, preserving
the liquid. The API accepts **one axis-aligned box per closed, flat basin**, at
most 75% of basin width. Box/tank intersections are clipped to the basin width
and bed. Rotation, multiple boxes, open edges and uneven beds are rejected or
not represented; invalid submissions leave the existing input unchanged.

For basin width `W`, bed `z`, liquid area `V`, and box area below a candidate level
`A(h)`, the reference level solves `W * (h - z) - A(h) = V`. The axis-aligned box
makes this a cheap piecewise-linear inversion. Its reference submerged area is
distributed over the overlapping columns. A column's pressure head is then
`bed + (liquid area + occupied area) / column width`. Existing conservative fluxes
spread the disturbance; moving or removing the box produces entry/exit ripples.
In equilibrium, a fully submerged box raises the level by its area divided by
the tank width. Removing it restores the original mean level.

**Occupied area is not water.** It is exposed separately as `Column::displaced`
and `WaterStats::displaced`, and never enters the liquid ledger. Sources,
deposition and reform cleanup refresh occupancy when liquid volume changes.
Empty/reclaimed tanks have no occupied-water height. Buffers are preallocated;
occupancy updates are linear in column count with no per-step allocations.
Without a displacer, the reference-level work is skipped.

This is a **hydrostatic-reference approximation**, not exact instantaneous
submersion against each rippling column. Occupancy changes pressure heads but
does not block fluxes: the box is not a watertight wall, piston seal or dam.
`sample(point)` excludes points inside the box, while hull immersion still
uses hypothetical column water. The model does not conserve coupled body/water
momentum or energy and does not simulate impact splashes or breaking waves.

### Matching visual fixtures

```sh
SPACEWARS_CLOCK_WATER_LAB=displacement cargo run --release -p engine-client -- --scenario clock
SPACEWARS_CLOCK_WATER_LAB=displacement-control cargo run --release -p engine-client -- --scenario clock
```

Select **Clock Controls → Preview Event: Meltdown → Preview**. Both modes show
the same closed tank, orange box, yellow floating ball and dashed initial-level
line. The box is **kinematically controlled**, not falling freely: it waits one
second, lowers over 1.5 seconds, holds for 1.5 seconds, then withdraws over 1.5
seconds. The actuator supplies its motion/work. The yellow ball responds through
one-way buoyancy; it is an observer and does not itself displace water. The
`displacement-control` mode disables only the orange box's occupancy feedback.
This separates the new effect from changes in geometry or other tuning.

The tank starts with 24 cell-volumes and uses 128 columns, zero spill parcels,
one kinematic box, one dynamic ball and one fixed support body (five colliders).
At full insertion, the box occupies 3.36 cell-equivalent areas, giving a 14%
mean-level rise. The last 1.5 seconds still reclaim water for Meltdown's normal
cleanup. Pause, resize, replacement, restart and completion retain the existing
lifecycle. Normal Meltdown stays body-free, and `SPACEWARS_CLOCK_WATER_LAB=1`
(or `cascade`) retains the original one-way collecting-pool fixture.

`clock state` reports `displaced_microunits` separately and labels it as body
space, not water. Older payloads default the new field to zero.

```sh
cargo test --locked -p engine-water -p engine-rapier -p scenario-clock
cargo run --locked --release -p engine-water --example displacement_benchmark
cargo run --locked --release -p scenario-clock --example meltdown_benchmark -- --displacement
cargo run --locked --release -p scenario-clock --example meltdown_benchmark -- --displacement-control
```

Regressions cover full/partial submersion, partial basin overlap, exact settled
levels, source/reclaim changes, invalid-input atomicity, zero-water cleanup and
repeated entry/exit plus horizontal motion at 30/60/120 Hz. They check bounded
ripples, nonnegative liquid, conservation, deterministic replay and reused
occupancy storage. Clock compares the same tank with feedback on/off, checks
observer response, mean levels, pause/replay and lifecycle cleanup at device
aspects. Render fixtures cover both tank modes through both rendering paths.

The water-only benchmark measures occupancy submission **plus** water stepping
for matched tanks, after 600 warm-up ticks, over 12,000 measured ticks. The
prescribed box moves in four-second entry/exit/sideways cycles. Motion generation,
diagnostics, Rapier and rendering are outside the timer. Clock's benchmark
includes the lab physics and reports draw-list construction separately.

### Third-slice desktop results (2026-09-11, Rust 1.89 release)

| Columns | Control submit + step p95 | Displacement submit + step p95 |
| --- | ---: | ---: |
| 32 | 0.65 µs | 0.83 µs |
| 128 | 3.72 µs | 3.74 µs |
| 512 | 10.80 µs | 10.77 µs |

These short timings include scheduler/frequency noise and different fluid motion;
near-equal values do not establish zero displacement cost. The largest absolute
water-accounting error was 5.46e-12 area units on 2,000 initial units.

The complete Clock tank preview measured **5.9 µs step p95**, versus **5.0–5.1 µs**
with displacement off, across the three benchmark sizes. Draw-list p95 was
4.5–4.7 µs; both modes peaked at 384 primitives and three bodies/five colliders.
Normal Meltdown remained at 4.6 µs step p95, 6.2–6.4 µs draw-list p95, and zero
bodies. These are desktop simulation/draw-list timings, not raster/presentation
cost, Pi CPU usage or device FPS.

Validation passed: 23 water tests, 59 Rapier tests, 81 Clock tests, 51
common/control/CLI tests, and 297 client tests. Three existing extended Duck
sweeps and one existing client test remain ignored. The real-client Meltdown
workflow passes under Xvfb in all four modes, including pause, preview, recovery,
restart and launcher return. Portrait/landscape captures were visually inspected.
Formatting, scoped Clippy (with the previously noted unrelated lint allowed),
and the host `pi-kiosk` feature check pass. No Pi deployment or ARM build was
performed for this slice.

## Later slices

Use the controlled-box/control pair to judge whether this approximation is
useful before adding feedback to freely moving bodies. Further candidates are
multiple/rotating hull occupancy and explicit obstacle-aware flow, each with
conservation and stability tests. Do not assume this first box model supports
arbitrary rigid bodies, sealed moving obstructions or splash physics.

Arbitrary enclosed cavities, inverted vessels, planetary gravity, and free
floating liquid require a richer representation. Keep body coupling separate
so those experiments need not change the Clock or rigid-body implementation.
