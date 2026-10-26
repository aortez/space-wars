# Finite-mass overloaded support (#327)

`PhysicsWorld::step_material_dynamic_compression(dt)` advances a finite-mass
rigid platen resting against one fixed compliant material layer. Gravity
supplies work; the layer's reaction changes the body's momentum. The operation
accounts for actual kinetic energy, elastic storage, irreversible capacity
charges and viscous loss. It builds on the
[driven-compression layer](material-sustained-compression.md), with a different
native motion solve and its own saved history.

The original v15 dynamic contract covers monotone compression from rest under constant
overload, including deceleration toward viscous terminal speed. It also admits
an exactly balanced elastic equilibrium. One Support episode persists through
elastic compression and plastic flow. The [v16 unloading and rebound extension](material-rebound.md) adds elastic
release, separation, free flight and compliant recontact for independently
enrolled preloaded/sub-yield scenes. Existing v15 histories keep the monotone
contract described below. Initial falling bodies, moving supports, multiple
finite-material bodies and handoff from another impact/gravity/driven history
still require separate qualification before sharing energy and episode ownership.
The existing rigid gravity operation continues to refuse overloaded support.

## Admitted scene and enrollment

Enable loading capture and material response. Use the driven operation's
full-face rectangles, stable sources and material section: half-extents
`(0.5, 1)`, 1/2/4 uniform tangent slices and 1/2 coincident copies. Both bodies
have live sources; only the fixed support has a finite material. The platen is
an ideal rigid material with a finite inertial mass. This does not change the
transient law's ability to damage stronger materials with energetic weaker ones.

The platen is dynamic, rotation locked and nonsleeping, with gravity scale 1
and refreshed mass in `[5, 80]`. Call `refresh_mass_properties` after creation;
admission checks its mass properties against a refreshed copy. Gravity is
constant along the common local +x axis, with acceleration in `(0, 1e6]`.
Exact quarter-turn poses, a shared tangent coordinate and exactly representable
world offsets in `[-16, 16]` follow the driven frame certificate.

There is no external force/torque, tangent/angular motion, damping, joint, CCD,
friction, restitution, sensor, skin or custom collision filtering. Additional
per-body solver iterations are zero. These are explicit restrictions of this
operation, not new restrictions on ordinary physics.

Let `W = mass * gravity`, `A = 2 * thickness`, `k = stiffness*A/depth`,
`Y = strength*A`, `ey = Y/k`, and `eta = Y*CRUSH_TIME_CONSTANT/depth`.
Enrollment requires zero velocity and either:

- exact touching (`x=0`) with `Y < W <= 2Y`; or
- exact safe equilibrium, `0 < x <= ey` and **`k*x == W`**.

The second case represents an already loaded elastic layer: its initial stored
energy is retained explicitly. Nearby rounded equilibria are not snapped into
balance. Under overload, starting from rest gives positive displacement; the
viscous reaction can subsequently decelerate the body without reversing it.

```rust
use engine_rapier::world::{PhysicsWorld, loading::response::{
    compression::CompressionRejection,
    dynamic_compression::DynamicCompressionStep,
}};

fn advance(world: &mut PhysicsWorld) -> Result<DynamicCompressionStep, CompressionRejection> {
    world.step_material_dynamic_compression(1.0 / 120.0)
}
```

## Finite-mass motion and material work

Permanent indentation `p` is retained. For a monotone measured segment,
`p_next = max(p, x_next-ey)` and elastic compression is `x_next-p_next`.
As in the driven layer:

```
material work = change in elastic energy + Y*dp + viscous loss
viscous loss = eta * (dx/h) * dp
```

The native kernel solves the interval's finite-mass momentum equation and
kinematic midpoint relation together:

```
v_next = v + h * (gravity - mean_reaction / mass)
dx = h * (v + v_next) / 2
mean_reaction = material_work / dx
```

For stationary equilibrium, reaction is `k*x` and work is zero. For positive
motion the scalar displacement equation is monotone, so a fixed 64-iteration
bisection gives its unique root. Elastic energy differences use a factored
expression to retain small work increments. This is a discrete-work integrator;
it does not call the continuous analytic reference used by the tests.

One backend interval uses an initial velocity kick, a force-free drift at the
computed midpoint velocity, and a final reaction/gravity kick. The body remains
dynamic. Its position is never teleported. Actual displacement is read after
Rapier integrates it; the final velocity is recomputed from that displacement's
constitutive reaction, then rounded to the backend representation. Gravity is
not also applied by the raw solve. Solver contacts are cleared only inside this
fully certified pair, preventing rigid penetration correction or a transient
contact response from supplying extra work. The fixed rectangle represents the
undeformed layer envelope.

Every rounded interval checks both energy and impulse, plus its midpoint
position relation:

```
energy defect  = gravity_work - delta_KE - material_work
impulse defect = W*h - reaction_impulse - mass*(v_next-v)
position defect = actual_dx - h*(v+v_next)/2
```

The material law receives measured plastic displacement. Actual f64 capacity
loss must agree with `Y*dp` within `1e-5` relative error and must retain every
positive charge. Lost small charges or rounded-away overloads refuse. Charge
rounding is reported separately from physical viscous loss and motion defects.
The cumulative identity includes initial elastic storage:

```
gravity work + initial elastic energy
  = kinetic energy + current elastic energy + actual damage
  + charge rounding error + viscous loss + energy defect
```

Static equilibrium transmits `W*h` reaction impulse without motion, work or
damage. The body has no prescribed velocity command; its speed depends on its
actual inertial mass and the material reaction.

## Time, numerical bounds and ownership

Positive public duration is at most `1/30` second. Divide it by powers of two
until the internal duration is at most all of:

- `1/4096` second;
- `1/(32*sqrt(k/mass))`, the elastic timescale bound;
- `mass/(32*eta)`, the viscous timescale bound.

At most 1,024 intervals are permitted. Binary subdivision makes their exact
f64 sum equal the requested f32 time; an unrepresentable subdivision or elapsed
sum refuses. There is no omitted tail. The tested 30/60/120/240 Hz schedules
use identical internal intervals and produce identical retained physics totals.
Irregular subdivisions are compared against the independent continuous solution.

The physical bounds inherited from the layer remain: thickness `[0.125,8]`,
depth `[0.125,1]`, yield deflection below `depth/4`, and maximum indentation
`depth/4`. This operation also limits velocity to `[0,64]`, retained duration to
3,600 seconds, solves to 1,000,000, and scalar accounting magnitudes to `1e12`.

Energy, impulse and position scales are fixed at enrollment:
`[Y*depth, sqrt(2*mass*Y*depth), depth]`. Sum **absolute** interval defects and
refuse above `1e-5` of each scale. Absolute charge errors have the same energy
allowance. Signed errors remain available for checking the cumulative balances.
These limits never grow with time, translation, calls or saves. They can refuse
a physically valid scene before material failure: the full-capacity WOOD
reference reaches its position allowance during the fourth 30 Hz call.
A separate worn-layer test exercises capacity exhaustion first. No tolerance
is widened to make a longer run succeed.

Enrollment emits one canonical Support event. Every interval carries its owner,
role-ordered sources, material/law identity, geometry, actual motion, kinetic
change, gravity work, reaction impulse, elastic storage, damage, loss and errors.
`interval.load.driver_work` is the mechanical work transferred **from the finite
platen into the layer**, not a second external energy source. The outer report
contains execution-ordered intervals and a copy of the retained summary.
Ordinary contact/impact batches are not the consumer interface for this path.

All work occurs in a disposable snapshot. Unsupported enrollment, exhausted
regions, output/solve limits, stale state and numerical refusals preserve the
original world, time, capacity, history and prior outputs. A refusal late in a
call also rolls back earlier provisional intervals. Failure topology and partial
failure continuation are left to later work. Pause clears ephemeral outputs
without enrolling, moving or resetting history.

## Saved-state compatibility

Snapshot v15 appends optional arrival trackers, a dynamic-compression tag and a
required revision-1 continuation after the unchanged base/capture/material
payloads. It saves initial material and energy, mass, gravity, physical totals,
fixed error scales, and either the active scene/episode witness or the first
invalidation cause. Restore checks all balances and limits against live geometry,
configuration, refreshed mass, gravity, motion, material capacity and owner.

Public edits and other positive stepping permanently invalidate continuation,
even when an edit is undone. The retained history remains readable after all
bodies and regions have been removed. Mode switching cannot re-enroll a new
compression/gravity budget. Unknown, malformed, relabeled, truncated or trailing
payloads refuse.

Versions 1–14 retain their selection and layouts. The rectangle certificate is
shared without adding serialized fields to the v14 scene. Independent frozen
serializers spell out the old driven history, scene, status tags and field order
for active elastic/plastic/detached and invalidated lifecycles, with replay checks.
Existing v1–v13 gravity and arrival compatibility tests remain unchanged.

## Independent evidence

The continuous reference solves Newton's equation as an elastic oscillator up
to yield, followed by exponential approach to the viscous terminal speed. It
analytically integrates viscous loss and checks the complete physical balance.
A separate small-step fixture applies measured reaction forces to an actual
Rapier body of finite mass against a virtual compliant surface. It has no rigid
support collider and does not use the native midpoint root solver.

Public tests compare the native method against those references across masses,
near-yield through 2Y overloads, time subdivisions, quarter turns, translated
frames, partitions, duplicates and insertion orders. They check exact safe rest,
work/impulse accounting, replayed report and snapshot bytes, numerical exhaustion,
late capacity-failure rollback, edit invalidation and terminal saves. Private
tests corrupt every accounting category and exercise live witness, version,
mode, tiny-charge and solve/output-limit refusals.
