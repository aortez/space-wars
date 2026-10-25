# Gravity and sustained-support qualification (#327)

The fixture in `response/tests/flat_arrival/gravity.rs` measures the backend
force-integration and support behavior needed before extending
[native flat arrivals](material-native-flat-arrival.md) to gravity. It keeps
the same rectangular geometry: one dynamic mass-20 plate against one fixed
plate, locked rotation, no friction, and equivalent sliced/duplicate collider
representations. The normal and gravity point along +x; the frame can be
pictured as a floor rotated sideways.

This is diagnostic evidence, not a new stepping operation. The observer runs
actual Rapier steps, records the material callback and subsequent motion, and
leaves the material adapter's external-work refusal in place. An observed raw
step can continue through ordinary physics after a material refusal. The
public transactional flat operation still rejects gravity before mutation.
The follow-up backend correction below fixes impulse reports. It does not
expand the production API, change the material law or change snapshot formats.

## Gravity is applied after the material callback

`PhysicsPipeline` invokes `modify_solver_contacts_batch` before assembling the
islands and effective forces. `IslandSolver` divides the requested duration by
the solver-iteration count. For each internal step, `VelocitySolver` adds the
force increment, solves constraints, advances positions with biased solver
velocities, then performs the unbiased stabilization solve. Consequently:

- The material callback sees the incoming body velocity, before this step's
  gravitational acceleration.
- Free flight uses repeated semi-implicit steps. For constant acceleration `a`,
  initial velocity `v`, duration `h`, and `N` internal steps, ideal arithmetic
  gives the following endpoint, before f32 rounding:

```text
v_end = v + a*h
x_end = x + v*h + (N+1)/(2*N) * a*h*h
gravity_work - kinetic_change = m*a*a*h*h/(2*N)
```

The final expression is an integration error. There is no material contact or
material damping in this free-flight case. Across a fixed physical horizon the
error depends on both frame subdivision and the internal solver-step count.
It must not silently become irreversible material work or be hidden by a common
transport velocity increasing an energy tolerance.

The 128 free-flight configurations cover 1/2/4/8 internal steps,
30/60/120/240 Hz, two partitions, both copy counts and both insertion orders.
They compare the analytic discrete endpoint and work discrepancy, preserve
capacities, and replay complete snapshots and observations exactly. The work
error exceeds the existing impact tolerance in these cases; this does not
relax that tolerance or qualify force-driven material accounting.

## Arrival timing must use the same integration contract

For a body initially at rest a distance `d` from contact, a continuous ballistic
arrival `sqrt(2*d/a)` overshoots under the backend's discrete integration by
`d/N`. A root matching the discrete endpoint reaches the face, but its incoming
kinetic energy is only `N/(N+1)` of the gravitational work `m*a*d`.

The retained example uses `d=0.02`, `a=10`, mass 20 and four internal steps:
continuous arrival penetrates by approximately 0.005, far beyond the existing
`1e-5` material limit. The discrete arrival reaches the face with kinetic energy
approximately 3.2, versus 4 units of gravitational work. Eight partition/solver
configurations preserve this distinction. At the next touching callback the
material adapter explicitly refuses `ExternalWork`, leaving capacity unchanged.

Removing speculative contacts alone therefore does not qualify a gravity
arrival. The force-integration scheme, arrival root and work ledger must be
designed together before native admission expands.

## Corrected support impulse reporting

In this fixture, momentum balance independently determines the net support
impulse: `J = m * (v_before + a*h - v_after)`. At rest the average reaction is
the expected weight, 200 force units for mass 20 and acceleration 10.

The original backend's `ContactPair::total_impulse_magnitude()` reported that
impulse plus the incoming warm-start cache. The constraint update added the old
impulse to its accumulator before the first internal step solved; writeback
then included it in the new step's total. The excess remained when
`warmstart_coefficient` was zero, so disabling warm-start application did not
repair the report.

| Internal steps | Original reported resting impulse / momentum-balance impulse |
| --- | --- |
| 1 | 2 |
| 4 | 1.25 |
| 8 | 1.125 |

The opt-in correction uses the integer internal-step index to select which
impulses builders accumulate. Only completed substeps enter the report. The
initial cache still seeds the first solve, with the same warm-start coefficient;
it no longer enters the new total
before that solve. Normal and tangent totals are corrected in the rigid-body
and generic multibody paths. This avoids subtracting a cache from a rounded
total or identifying the first step through floating-point elapsed time.

Ninety-six support configurations now require the reported impulse to match
momentum balance, with both warm-start coefficients, all four frame rates, two
partitions and both copy counts. They retain one support episode and no impact
charge. Evaluating the pure sustained law with the safe load produces zero
permanent loss. This remains one known normal reaction, not a general method
for attributing arbitrary constraints to material regions.

Independent backend regressions in `world/tests/contact_impulses.rs` also cover
72 sliding configurations: rigid bodies and free-root multibodies, 1/4/8
internal steps, warm-start coefficients 0/0.5/1, two frame rates, both insertion
orders and sliding directions. Each runs four steps with changing durations
and gravity. Normal and friction reports must match momentum change, and force
events must match the normal reaction. For the coincident multibody masses,
momentum uses the final generalized root velocity; Rapier refreshes ordinary
body velocities later. Separate regressions cover unloading and a force-event
threshold of 225 under a true load of 200 (previously reported as 250).

A before/after run of those 72 sliding configurations compared serialized body
state, multibody state and normal/tangent warm-start caches after every step.
All 288 observations matched byte for byte. The three backend regressions fail
against the original accumulation and pass with the correction.

The solver impulse updates and warm-start writeback are unchanged. However,
Rapier also uses a zero reported normal impulse to classify a later contact as
new. Applying the correction globally changed the navigation-v1 seed-0 trace
at tick 9723: the preceding step's stale positive report became zero, changing
restitution on a continuing contact. Two existing Clock regressions also failed.
These cases make corrected reports a physics behavior choice, not a passive
diagnostic fix.

`PhysicsHooks::report_applied_contact_impulses` therefore defaults to false.
The pipeline samples it once per step, applies the choice to every island and
CCD subdivision, and resets reused solvers on the next step. Native gravity
operations and the independent gravity/impulse fixtures opt in. Ordinary
stepping, passive loading capture and the earlier arrival operations retain
their existing reports, force events and restitution behavior. The setting
adds no serialized state or snapshot schema changes.

The opted-in reports change force-event thresholds and serialized contact
impulse values, including later new-contact classification. A regression
alternates default and corrected steps for rigid and multibody support with
1/4/8 internal steps and warm-start coefficients 0/0.5/1; it checks both the
reaction and event reports against momentum and the retained cache. General
gravity work accounting, native multibody admission and a global reporting
migration remain outside this correction.

## Solver position error is not plastic deformation

The ordinary solver integrates positions with biased velocities before its
final stabilization pass. In the resting mass-20 fixture with acceleration 10,
two copies of each unsplit plate drift into contact by about `2.34e-5` after
four 60 Hz steps, while equivalent `[4,2]` slices move less than `1e-10`.
Final velocities are near zero in both cases. This partition-sensitive motion
cannot be interpreted as measured plastic crushing.

At acceleration 1000, the reaction is approximately 20,000, above the wood
side's safe force of 6,000 for width 2 and unit thickness. In the highly
constrained sliced fixture the actual motion supplies effectively zero work.
The material law's constant-load fallback nevertheless requests about 933.33
units over one 60 Hz step. A measured plastic displacement of zero reports
overload but charges no permanent work. The fixture applies neither result to
capacity: the fallback's implied deformation has not been coupled to any
physical work source. Static reaction force alone is not a finite damage budget.

## Next boundary

With reaction reporting corrected, the separate
[ballistic fixture](material-ballistic-gravity.md) qualifies split gravity kicks,
backend drift and instantaneous impacts. Its optional departure evidence
qualifies accelerated returns with fresh episode ownership. Its opt-in support
experiment now holds safe loads with measured reactions and no permanent loss,
and accounts for small-hop settling as a separate bounded numerical sink.
Neither phase grants another impact budget. Uncertain excursion ownership,
overload and failed support still refuse; native gravity admission is unchanged.
The three-plate extension now qualifies a new arrival against existing support,
with one incoming budget transmitted through both contacts, separate weight
and impact reactions, and energy/momentum checks. It retains support identity
through outbound flight but refuses later returns, support failure and a full
stack settling transition. Scale-aware departure bounds now resolve the retained
small-hop case, and a persistent force/work/position discrepancy ledger permits
bounded tiny frame remainders. The compensated final gravity kick avoids the
systematic bias of two rounded velocity increments. Genuinely ambiguous
departure and exhausted numerical allowances still refuse. The implemented
[native gravity operation](material-native-gravity.md),
`step_material_flat_gravity(dt, settling)`, exposes the qualified two-plate
flight/impact/support/settling phases with explicit policy, persistent numerical
history and snapshot v6 enrollment. Its public timeline reports work, response
links, ownership, settlement and measured reaction; successful calls account for
every positive interval exactly. The independent comparisons now use this public
entry point, with additional downstream integration tests.

The three-plate experiment remains a later extension. An overload needs bounded
deformation supplied by gravitational potential or an accounted external driver.
Other arrival methods still refuse gravity, and ordinary material response keeps
its `ExternalWork` guard. Solver penetration cannot substitute for material
deformation to make an unsupported scene succeed.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib gravity_loading_
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib contact_impulses_
```
