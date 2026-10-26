# Unloading, rebound and compliant recontact (#327)

`PhysicsWorld::step_material_dynamic_compression(dt)` now also advances a
resting preloaded layer through elastic release, separation, flight and return.
The same finite-mass body receives the released elastic energy. A separate
sub-yield loading fixture overshoots yield, retains permanent indentation, and
rebounds into elastic oscillation without charging the unloading motion.

This extends the [finite-mass support operation](material-dynamic-compression.md).
The rectangle, source, material, mass and cardinal-frame restrictions are
unchanged. There is still one ideal rigid platen and one fixed material layer.
No impact response is applied in addition to the compliant reaction.

## Enrollment and physical state

Let `x` be platen indentation from the undeformed surface, `p` permanent
indentation, `k` layer stiffness, `Y` yield force, `ey=Y/k`, `W=mass*gravity`
and `eta=Y*CRUSH_TIME_CONSTANT/depth`. Enrollment requires zero velocity and:

- touching `x0=0` with `0 < W <= Y`; or
- an elastic preload `0 < x0 <= ey` with `0 <= W <= Y`.

Exact safe equilibrium `k*x0 == W` and touching overload `Y < W <= 2Y` keep
the original revision-1/v15 contract and numerical behavior. Other admitted
starts use revision 2/v16. The choice is immutable and checked on restore.
Gravity is constant along local +x; zero gravity is allowed for preloaded
release. A preloaded start supplies explicit initial elastic energy
`U0=k*x0*x0/2`. Initial plastic indentation is zero. An initially worn material
region may have reduced capacity, but this does not imply an unmeasured initial
plastic geometry or elastic energy.

For `x0 > 2W/k`, elastic release reaches `x=0` with outward velocity. Positive
gravity turns the detached body around and returns it to the layer; zero gravity
leaves it in outward flight. Touching with `Y/2 < W < Y` instead develops plastic
set through inertial overshoot, then unloads about the displaced equilibrium.
This fixture does not separate after plastic flow. These are distinct qualified
lifecycles, not a claim that every loaded history can be released arbitrarily.

Changing gravity, applying an impulse, editing poses or switching steppers after
enrollment still invalidates continuation permanently. Starting in free flight,
importing an existing plastic preload, transferring a driven/impact/gravity
history, moving supports and general angular/oblique contacts remain unsupported.
A future impact/support handoff must carry attributable work and material state.

## Signed work and native motion

The unilateral layer has elastic compression `e=max(x-p,0)`, elastic energy
`U=k*e*e/2`, and no tensile reaction. For each signed displacement `dx`:

```
p_next = max(p, x_next-ey)
dp = p_next-p
loss = eta*(dx/h)*dp       (zero unless dp>0)
work = (U_next-U) + Y*dp + loss
mean_reaction = work/dx    (endpoint elastic force if dx=0)
```

The energy difference uses its factored form. On unloading `work` is negative,
so `work/dx` remains a nonnegative compressive reaction. Across separation the
remaining elastic energy returns to the body; an entirely detached interval has
zero reaction, work and damage. Recontact increases elastic storage before any
new yield displacement can consume capacity. Permanent set never decreases.

Revision 2 solves the signed midpoint momentum equation with a fixed 64-step
bisection over `[-depth,depth]`, checking the bracket before solving. This is an
extension of the discrete-work method, independent of the analytic test oracle:

```
dx = h*(v+v_next)/2
v_next = v + h*(gravity-mean_reaction/mass)
```

Rapier advances the dynamic body at the computed midpoint velocity with raw
gravity and rigid solver contacts disabled for the certified pair. Actual drift
is measured; the endpoint reaction and velocity are recomputed from that drift.
No pose teleport or energy projection is used. Rounded turning points may have
zero actual displacement: their midpoint/energy defects are accounted for rather
than being mistaken for monotone overload. All material charges still use actual
plastic displacement and must survive capacity rounding.

Every interval retains the original energy, momentum and position checks. The
cumulative identity is:

```
U0 + gravity_work = kinetic_energy + elastic_energy + damage_work
                 + charge_error + viscous_loss + energy_error
```

The fixed initial scales, `1e-5` absolute-error bounds, exact elapsed-time sums,
internal timestep bounds and solve/output limits remain unchanged. Revision 2
allows `|velocity| <= 64` and `|indentation| <= depth/4`, including negative
indentation during flight. No episode transition, pause or save renews a budget.
The mass-20 zero-gravity release at 30 Hz reaches its position-error allowance
during the fifth call. A mass-5 scene translated by `(0.125,-0.125)` reaches it
during the fourteenth 240 Hz call, after recontact. Both refuse atomically;
limits were not widened. A separate mass-10 dyadic-time flight reaches the
physical indentation bound first. Worn-layer tests cover late capacity failure.

## Episode ownership and observations

Enrollment emits one `Support` event. Ownership persists while `x >= p`.
A drift ending at `x < p` emits `End` and removes the owner. A later drift from
`x < p` to `x >= p` emits a new `Support` with a fresh ID. No `Impact` budget is
opened on compliant recontact. This operation already accounts for the incoming
body motion, elastic return, yielding and loss.

Transitions follow actual backend endpoints and the retained deformed surface,
not the rigid rectangle's contact manifold or a predicted endpoint. Event times
interpolate the crossing within that interval's monotone native drift. They are
discrete trajectory times, not an exact continuous time-of-impact claim. At exact
`x=p` the contacting owner remains until an actual outgoing drift demonstrates a
gap. The load's episode owns its contacting portion; fully detached intervals
have `None`. Intervals and events are returned in execution order in their
respective arrays. Thresholds for generic contact capture do not suppress these
work-accounted transitions.

A disposable snapshot stages every public call. Late event/identity exhaustion,
material failure, out-of-range flight and numerical refusal preserve original
motion, time, capacity, history and outputs. Recontact does not enroll a new
physical history or supply another initial elastic-energy allocation.

## Saved-state compatibility

Snapshot v16 appends the optional arrival trackers, a distinct rebound tag and
the required revision-2 dynamic continuation after the unchanged base/capture/
material payloads. Its active status stores the certified scene and an optional
episode owner, including detached flight. The history retains initial material,
preload energy, mass/gravity and every physical/numerical total. The new status
variant is appended after the unchanged v15 active and invalidated tags.

Restore verifies header/tag/revision agreement, enrollment, live scene and mass,
material capacity, signed balances, fixed error bounds and contact-phase owner.
Invalidated histories remain readable after all bodies and regions are removed.
Unknown revisions, relabeled tags, truncated/trailing bytes and corrupt physical
or phase witnesses refuse. No restore path automatically upgrades v15 histories.

Versions 1–15 keep their selection and layout. Frozen serializers independently
spell out the v15 material/history/summary/scene/status field order and verify
bytes and continuation for active elastic overload, plastic overload, exact safe
equilibrium and invalidation. Existing frozen v14 and older compatibility tests
remain in place.

## Independent evidence

The analytic oracle uses a shifted harmonic oscillator during elastic contact,
a ballistic parabola during flight, and time reversal on return. It checks total
energy throughout repeated cycles. A second analytic case follows sub-yield
inertial loading through yield, viscous deceleration, turning and elastic
oscillation with permanent set.

A separate actual finite-mass Rapier body is driven by measured forces from a
virtual unilateral layer, without a support collider or the native midpoint
solver. Coarse and four-times-finer force integration check velocity/contact
convergence and permanent indentation. Raw f32 position accumulation in long
constant-speed flight is checked separately; making that fixture's time step
smaller does not eliminate its position quantization. Native acceptance uses
its own stricter measured-defect budget, not the oracle's comparison tolerance.

Public tests cover release/flight/return and plastic unloading, 30/60/120/240 Hz
subdivision, duplicate/split geometry, reversed insertion, masses `[5,80]`,
quarter turns, translations, irregular steps around departure, replay at every
phase, pause/edit invalidation, capacity/physical/numerical rollback and terminal
saves. Private tests cover corrupted totals and owners, version relabeling,
late event/identity exhaustion and frozen v15 compatibility.
