# Driven sustained compression (#327)

`PhysicsWorld::step_material_compression(dt, speed)` advances an ideal rigid
platen against one fixed compliant material layer. It measures actual backend
motion, integrates the layer's reaction, and charges the existing sustained
material law for measured permanent compression. The independent fixture is in
`crates/engine-rapier/tests/material_compression.rs`.

This is the first native sustained-deformation slice. It is displacement-driven;
it does not yet replace the gravity operation's refusal of overloaded supports.
Dynamic support deformation, transient-impact handoff, beam connections and
failure topology remain separate work. The existing impact path still permits
sufficiently energetic weaker materials to damage stronger ones. Here the
platen is an explicitly ideal prescribed boundary, with no finite material of
its own.

## Admitted scene and use

Enable loading capture and material response. Construct exactly two full-face
rectangles with half-extents `(0.5, 1)`: a `KinematicVelocity` platen and a fixed
support. Bind each to its own live loading source, and register a material only
on the support. Use an initially stationary platen exactly touching the support.
Call the compression method again for each interval, including after restore;
pass a new signed normal speed through this method, rather than editing the
body velocity between calls.

```rust
use engine_rapier::world::{PhysicsWorld, loading::response::compression::{
    CompressionRejection, CompressionStep,
}};

fn compress(world: &mut PhysicsWorld) -> Result<CompressionStep, CompressionRejection> {
    // Positive advances into the layer; negative unloads; zero holds its pose.
    world.step_material_compression(1.0 / 120.0, 0.05)
}
```

The normal is the common body's local +x axis. Exact quarter-turn poses set with
`set_quarter_turn_pose` are supported. The fixed center minus one normal unit
must be exactly f32 representable, with both components in `[-16, 16]`. Centers
share the tangent coordinate. Each full rectangle may use 1/2/4 uniform tangent
slices and 1/2 coincident copies; these always describe one physical patch.

There is no gravity, tangent/angular motion, external force/torque, damping,
joint, CCD, skin, sensor, friction, restitution or custom collision filtering.
Collision and solver groups/types use defaults. Additional per-body solver
iterations must be zero; there are no dynamic bodies to solve. Stationary
kinematic sleep is harmless: the next nonzero command wakes the platen.

The support collider describes the layer's **undeformed envelope**. Within this
opt-in operation, the platen can enter it as the layer compresses. Default
Rapier kinematic/fixed pairs cannot apply rigid contact impulses; no solver
penetration correction is counted as physical deformation. The load report's
reaction is the constitutive reaction carried by the prescribed boundary, not
a measured rigid-solver impulse or a force integrated onto a dynamic body.
Ordinary positive stepping invalidates this interpretation permanently.

## Physical model and work

Let `x` be actual indentation from initial touching and `p` the retained plastic
indentation. Negative `x` is an initial-frame gap; after permanent compression,
the free surface is at `p`. Area `A = 2 * thickness`. Define:

- `k = stiffness * A / depth`, the existing section contact stiffness;
- `Y = strength * A`, and yield deflection `ey = Y / k`;
- `eta = Y * CRUSH_TIME_CONSTANT / depth`.

The layer is unilateral elastic-perfectly-plastic, with viscous overstress only
during plastic flow. For one monotone measured interval:

```
p_next = max(p, x_next - ey)
dp = p_next - p
elastic = max(x_next - p_next, 0)
U_next = 0.5 * k * elastic²
v = (x_next - x) / dt
plastic work = Y * dp
viscous loss = eta * v * dp       (zero without plastic flow)
driver work = U_next - U + plastic work + viscous loss
```

Reaction is `k * elastic` during elastic compression/unloading, zero while
detached, and `Y + eta*v` during plastic flow. Its integral gives impulse.
For nonzero displacement this is `driver_work / displacement * dt`; a stationary
hold carries `reaction * dt` and consumes no work. The monotone interval's
average measured speed defines its piecewise-linear prescribed motion.

Viscosity makes the plastic flow rate under constant overload agree with the
existing sustained law's fallback: `v = depth*(F-Y)/(Y*tau)`. It also identifies
the separate overstress loss that the driver must supply. This host coupling is
a bounded game model, not a material calibration or general continuum solver.
Material damping remains an impact parameter; it is not applied to a static hold.

The native operation calls `sustained` with **measured** `dp` and applies its
returned charge to the target's f64 capacity. That law's reported recoverable
energy describes current storage; it is never credited as new supplied work.
The retained identity is:

```
net driver work = current elastic energy + actual capacity charge
                + charge rounding error + viscous loss
```

Unloading returns elastic work to the driver. Plastic indentation and spent
capacity persist. Reloading to an earlier peak adds no plastic damage. Holds do
no work and preserve the episode, including at the yield deflection. A genuine
detachment ends the episode; touching/reloading creates a new Support owner,
while the same physical and numerical history remains. This prescribed operation
never creates an Impact budget.

## Records, bounds and atomic refusal

A successful positive call returns one `CompressionLoad`, chronological episode
transitions, and the retained `CompressionSummary`. The load contains the law
and material identities, geometric-role sources, episode, normal, section,
measured total/plastic displacement, end reaction, integrated impulse, elastic
storage change, driver work, actual damage, viscous loss and charge error.
Event source pairs are canonical. The explicit load report is the consumer
interface; this operation does not populate ordinary impact-response batches.

| Bound | Limit |
| --- | --- |
| Positive duration | `dt <= 1` second |
| Commanded speed | `abs(speed) <= 0.25` |
| Layer thickness / depth | `[0.125, 8]` / `[0.125, 1]` |
| Indentation or withdrawal from initial frame | `abs(x) <= depth/4` |
| Yield deflection | strictly between zero and `depth/4` |
| Retained duration / calls | 3,600 seconds / 1,000,000 |
| Retained scalar accounting magnitude | at most `1e12` |
| Absolute accumulated motion rounding | at most `1e-5 * depth` |
| Absolute accumulated charge rounding | at most `1e-5 * Y * depth` |

Motion is read from actual endpoints. Numerical allowances are fixed from the
initial material/section and sum absolute errors; saving, holding or replacing
an episode does not renew them. A nonzero command that rounds to no motion is
refused. Every positive plastic charge must be retained and agree with `Y*dp`
within `1e-5` relative error. Rounded forces that erase an overload, or capacities
that cannot represent its charge, refuse even below the cumulative allowance.

The entire operation runs on a disposable snapshot. Unsupported geometry,
limits, stale witnesses, failed/exhausted regions and numerical failures leave
the original world, capacity, history and prior outputs unchanged. Capacity
exhaustion refuses the whole interval; there is no partial failure or remaining
time silently handed to ordinary physics. A host must handle refusal before
continuing. The existing capture record/episode limits also apply, with one
record for the load plus at most two transitions.

Pause (`dt=0`) clears ephemeral outputs and returns the retained summary without
applying the command, enrolling or renewing history. Commands must still be
finite and in range. Public edits or other positive stepping retain the first
invalidation cause and permanently disable continuation, even if the edit is
undone. Gravity and compression histories cannot be combined or re-enrolled.

## Saved-state compatibility and evidence

Snapshot v14 retains the unchanged base, capture and material payloads, followed
by optional arrival trackers, a compression tag, and the required revision-1
compression continuation. The tail contains the initial material, physical and
numerical totals, and either an active scene/episode witness or the first
invalidation cause. Invalidated histories remain readable after both bodies and
regions have been removed. Restore validates physical balances, bounds, current
material capacity, live geometry/configuration, motion and episode ownership.

Versions 1–13 keep their existing layouts and selection. Independent frozen
serializers now also exercise the v12/v13 pair/stack tails; the earlier version
and lifecycle regression tests remain in place. Unknown, truncated, cross-labeled
and corrupt compression saves refuse. No material law version changes.

The independent fixture advances an actual backend platen and integrates a
force-displacement curve using midpoint quadrature. It does not call the native
closed-form work kernel. Tests compare work, stored energy, permanent indentation
and viscous loss through loading, a hold, unloading, detachment and reloading.
Public replay checks compare reports and complete snapshot bytes. Other cases
cover 30/60/120/240 Hz, all quarter turns, translated frames, split/duplicate
geometry, reversed insertion/source order, section scaling, constant-overload
rate equivalence, safe support, tiny unrepresentable charges, atomic refusal,
external edits, terminal saves and corrupted physical/numerical witnesses.
