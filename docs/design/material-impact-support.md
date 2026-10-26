# Falling-platen impact-to-support handoff (#327)

`PhysicsWorld::step_material_dynamic_compression(dt)` accepts a falling finite-mass
platen and carries its incoming motion through impact into continuing compliant
support. The same calculation accounts for elastic storage, material yielding,
viscous loss, unloading, separation and return to the permanently indented
surface. An `Impact` observation and subsequent `Support` observation share one
episode and one physical history. They do not authorize two damage allocations.

This revision extends the [dynamic support](material-dynamic-compression.md) and
[rebound](material-rebound.md) contracts. It retains their certified two-body
rectangle, material, mass `[5,80]`, quarter-turn and translated-frame restrictions.
Only the fixed support has a finite material; the platen is an ideal rigid body
with finite inertia. It is not a general transfer from an existing rigid-impact,
gravity or prescribed-motion controller.

## Enrollment and initial motion

Use the undeformed touching surface as `x=0`, with positive x directed into the
layer. Revision 3 admits:

- `-depth/4 <= x0 < 0`, initial normal velocity `0 <= v0 <= 64`, and either
  positive velocity or positive gravity; or
- `x0=0` and positive approaching velocity `0 < v0 <= 64`.

Constant normal weight satisfies `0 <= W <= 2Y`, where `Y` is yield force. Zero
gravity therefore supports incoming motion, but not a motionless detached body.
Starting already indented with nonzero velocity, initially receding motion,
imported plastic preload and histories owned by other loading modes refuse.
The existing resting starts still select revision 1/v15 or revision 2/v16.

Initial velocity comes from the actual refreshed-mass body's motion; it is not
an independent energy command. The history retains `K0 = mass*v0*v0/2` and
initial momentum `mass*v0`. Initial elastic and plastic indentation are zero.
Gravity work starts at the actual initial gap. In particular, dropping from rest
has `K0=0`; subsequent contact energy comes from measured gravitational work.

## One work calculation across the handoff

The unilateral elastic/plastic/viscous layer and signed discrete-work midpoint
solve are unchanged. Each internal interval advances the actual dynamic Rapier
body, measures its drift, recomputes its reaction and endpoint velocity, and
checks rounded motion. Raw gravity and rigid penetration correction remain
disabled during auxiliary drift. There is no pose teleport or second transient
impact-law call at arrival.

The cumulative balances now include the initial motion:

```
K0 + gravity_work = kinetic_energy + elastic_energy + damage_work
                  + charge_error + viscous_loss + energy_error

W*elapsed - reaction_impulse = mass*(velocity-v0) + impulse_error

indentation-x0 = integrated_velocity + position_error
```

During fully detached flight, layer force, impulse, work and capacity charges
are zero. Contact loading stores elastic energy and charges only measured new
plastic indentation. Unloading returns elastic energy without replenishing
capacity or erasing permanent set. A later collision at `x=p` uses that same
stored geometry and remaining material, not the original undeformed surface.

Fixed initial error scales remain `[Y*depth, sqrt(2*mass*Y*depth), depth]`; incoming
kinetic energy does not enlarge them. The `1e-5` cumulative absolute allowances,
relative small-charge check, exact elapsed sums, power-of-two subdivisions,
1,024-interval call limit and physical bounds are unchanged. Large or poorly
represented arrivals can refuse before capacity failure. The translated
plastic-recontact regression reaches the same fixed position allowance after
return; pausing or saving cannot renew it. Exhausted material and partial failure
continuation still refuse atomically.

## Episode and observation order

Enrollment in free flight allocates no episode and emits no contact event. On
the first actual drift reaching `x >= p`, allocate an owner and emit `Impact`.
The next continuing interval emits `Support` with the **same ID**, even if it
occurs in a later public call or after restore. Support means continued compliant
contact; it does not mean the body has stopped or that impact energy was fully
dissipated. There is no resting-state projection or artificial settling loss.

A drift ending at `x < p` emits `End`. Recontact opens a fresh episode and repeats
Impact/Support while retaining the single physical history. Contact thresholds
for generic capture do not suppress small approaching arrivals on this path.
Fully detached interval loads have no owner; an interval crossing the surface
is attributed to its contacting owner.

Arrival/departure times interpolate crossings within the measured, monotone
native drift. They describe the discrete trajectory, not exact continuous TOI.
A Support observation occurs at the start of the next internal interval. Events
and intervals are execution ordered within their respective report arrays; their
`at` fields place them within the public call. Save/pause preserves a pending
Support observation. Rejected calls preserve prior outputs and never publish
provisional events.

The snapshot transaction also covers late event/output/identity exhaustion.
A free-flight call needs only its interval records; an arrival and its same-owner
Support observation must each fit before the transaction can commit.

## Saved-state compatibility

Snapshot v17 uses a distinct falling-support tag. It writes the unchanged
base/capture/material payloads and optional arrival trackers, followed by a
revision-3 dynamic continuation and a required incoming-motion extension. That
extension stores the initial f32 normal velocity. Initial position and material
remain in the common history. The common history serializer skips the new motion
field, so v15/v16 bytes do not acquire an extra field or optional tag.

A new active status variant is appended after all existing status tags. It stores
the certified scene, optional owner and whether Support must be observed on the
next interval. Restore validates the incoming motion, kinetic/momentum balances,
contact phase, live geometry/configuration/mass/material and fixed error budgets.
Detached history cannot claim an owner or pending Support. Invalidated history
retains incoming energy/momentum after all bodies and regions have been removed.

Header/tag/revision disagreement, absent or corrupt incoming motion, unknown
variants, truncated or trailing payloads refuse. Rewriting a v17 header/tag and
dropping its extension cannot turn it into a v15/v16 history. Old histories never
upgrade automatically. Frozen v16 serializers independently spell history,
scene and status fields for elastic contact, detached flight, return, plastic
unloading and both invalidation causes; bytes and continued replay are checked.
The existing frozen v15/v14 and older compatibility tests remain in place.

## Independent evidence and scope

The continuous reference computes ballistic arrival and incoming speed, an
elastic oscillator through contact, viscous plastic flow when yield is reached,
and either sustained compression or turning/unloading. After a plastic turn,
sub-yield gravity may permit separation and return about the displaced surface.
Its full energy identity is checked at each query. It does not use the native
midpoint root solver.

A separate actual finite-mass Rapier body receives forces from a virtual layer,
without a rigid support collider. Coarse and four-times-finer integrations check
arrival/return counts, motion, permanent set and viscous work. Their f32 position
quantization is bounded separately; smaller steps do not eliminate it. Native
acceptance still uses its own measured-defect limits. The independent cases
include elastic return, yielding followed by rebound/recontact, permanent-set
support oscillation, overloaded continuing compression and zero-gravity impact.
An undamped elastic support oscillates; this does not claim automatic settlement.

Native tests cover those lifecycles, 30/60/120/240 Hz with identical physical
histories and event sequences, split/duplicate geometry, reversed insertion,
masses `[5,80]`, quarter turns, translated scenes, split arrival boundaries,
slow touching approach, and exact saved replay before/at/after the handoff.
Hostile-save, mode/edit invalidation, late material/event/identity failure and
cumulative numerical exhaustion tests verify atomic refusal.

Remaining work includes transferring histories between older loading modes,
changing applied forces while retaining attributable work, multiple finite
material bodies, moving/oblique/angular contact geometry, connection loads and
#328 failure consequences. This completes the first bounded falling-platen
handoff, not all of #327.

## Application demonstration

The [Material Lab](material-lab.md) hosts four bounded falling-platen cases with
slow playback, pause, single step and a persistent save slot. It uses this native
path without changing its physical or numerical restrictions.
