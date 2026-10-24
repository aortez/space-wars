# Native flat gravity stepping contract (#327)

`PhysicsWorld::step_material_flat_gravity(dt, settling)` now exposes the qualified
two-plate gravity operation. It advances flight, material impacts, safe support
and explicit bounded settling in one transaction. The independent
[ballistic fixture](material-ballistic-gravity.md) supplies the physical evidence.
Public types live in `world::loading::response::gravity`; continuation state,
admission and physical phases remain private. No scenario selects this operation
by default, and this bounded scene does not complete #327.

The first successful positive call enrolls snapshot v6 history. Unenrolled worlds
keep their existing v1-v5 formats; pause and refusal cannot enroll. There is no
separate enrollment/reset or partial-step API. Existing public arrival operations
still refuse gravity, and ordinary stepping retains its `ExternalWork` guard.

## First admitted scene

Start with the complete two-plate scene, not the three-plate incoming experiment.
The latter has not qualified departure, later returns or full-stack settling.
Admission examines the whole world on every positive call:

- Exactly one dynamic plate and one fixed plate, with local half-extents
  `(0.5, 1)`, identity rotation, zero y position/velocity and zero angular velocity.
  The fixed center is `(1, 0)` and its velocity is zero. The dynamic center's x
  coordinate is measured relative to its touching position at x=0; the gap is
  `-x`. The physical contact plane is x=0.5. No translated or rotated frame is
  claimed by this first operation.
- Each plate is the certified filled rectangle from native flat admission:
  one/two/four uniform tangent slices, one/two identical copies per slice, at most
  eight colliders per body and sixteen total. Body roles come from body type and
  geometry, independently of handle, source, or insertion order.
- The dynamic body has refreshed mass 20, rotation locked, translations unlocked,
  sleeping disabled, finite COM and positive effective inverse mass on both
  axes. The fixed mass does not supply an energy source. A broader mass range
  needs additional independent qualification before expanding this boundary.
- Gravity is `(a, 0)` with finite `a > 0` and gravity scale 1. No other force,
  torque, damping, friction, joints, CCD/soft CCD, skin, sensor, disabled object,
  custom hook/filter, nondefault dominance or kinematic body is admitted.
- Exactly one distinct live registered region and one consistent source per
  body, with every collider bound to its actual owner. No extra registrations,
  orphan colliders or unbound solids. Material definitions and sections retain
  the response layer's validation; safe support evaluates both actual materials.
- Capture's touching tolerance is `1e-5`. The fixture's arrival-contact band is
  `1e-6`; the departure band is twice capture's tolerance. The configured impact
  speed threshold still applies. A touching approach below it must refuse if no
  accounted response applies, regardless of the body's kinetic energy.

The first successful positive call requires no pre-existing contact episode or
pending ball/flat departure witness. It can start in free flight, in a fresh
touching approach, or at stationary contact where it establishes Support itself.
It cannot adopt an old ordinary-contact episode without its missing history.
Later calls require the exact saved continuation. Admission and mass refresh run
inside the transaction, before any retained state is adopted.

Initially refuse a failed region, and refuse the entire call if an impact makes
either region fail, including an impact at the frame endpoint. Damage below
failure remains allowed. Physical failure and loss-of-support consequences have
not been qualified. Safe rest requires the actual load `20*a` to remain within
both regions' safe force at width 2 and their registered sections. Overload
refuses; static reaction and solver penetration cannot fund permanent damage.

## Explicit operation and policy

The public entry point and types live in `response::gravity`:

```rust
pub fn step_material_flat_gravity(
    &mut self,
    dt: f32,
    settling: GravitySettling,
) -> Result<GravityStep, GravityRejection>;

pub enum GravitySettling {
    Refuse,
    BoundedNumerical,
}
```

Both choices allow safe stationary support. `Refuse` rejects an approaching
return whose old episode has not proved departure. `BoundedNumerical` permits
one settlement only with the fixture's observed turn, complete excursion upper
bound inside the exit band, retained post-impact energy, unspent settlement and
safe support. Actual removed kinetic energy is reported as numerical settling;
it never charges a region or represents calibrated material damping. Neither
policy guesses ownership when the bounds straddle the departure threshold.

There is no implicit policy default. The first successful positive call stores
the choice; changing it later refuses. This makes replay and the meaning of a
retained settling allowance explicit. A public `dt=0` remains a pause: clear
ephemeral outputs, return an empty result, and preserve policy, ledger and
episode state without enrolling or changing the world. Invalid time refuses
without clearing previous outputs.

Success consumes the requested duration with the documented split integration.
Every refusal preserves the entire original world and previous outputs. There
is no automatic ordinary-physics fallback. Callers must select this operation
again after restoring; persistence does not select a stepping method.

## Native transaction and physical phases

Stage through the native snapshot boundary, then perform complete admission,
continuation validation and mass refresh. Clear staged capture/response outputs
once for the public call. Temporary gravity changes, hooks and solver settings
must be restored before commit, including after the last instantaneous solve.
The stored integration `dt` is bookkeeping, not a frozen contract parameter;
all other integration settings remain the enrolled settings for this slice.

Flight uses rounded midpoint velocity for force-free Rapier drift and rounds
`v_start + a*h` once for the final velocity. Use the same stable ballistic arrival
root and compensated apex split as the fixture. Prove the entire interval stays
outside the fixed plate before suppressing contacts. Check actual drift against
the scale-aware bound before using it as either departure or containment
evidence. The existing material-response energy tolerance does not change.

An arrival, including one exactly at the requested endpoint, resolves before
success returns. After every flight, the scheduler also checks the observed
endpoint for an approaching contact inside the arrival band. Rounded drift can
reach contact even when the analytic root is just beyond the frame; that
approach must resolve or refuse within the current transaction. The same rule
applies to a contained return requiring settlement. The internal zero-duration solve runs capture and the checked
material response at the physical incoming velocity, without a gravity kick or
pose change. This uses a distinct native internal path:
`PhysicsWorld::step_controlled` currently returns before solving when `dt<=0`.
The new phase calls the raw backend with the normal loading hooks, zero gravity
and zero duration after complete admission. Public pause semantics and the general
`ExternalWork` guard remain unchanged. The phase restores gravity, hook flags and
the previous backend `dt` before returning, including a discarded solve.

Safe hold uses two half-kick/projection phases around stationary force-free
drift. Check reported reaction against weight, exact unchanged poses and final
zero velocity. Report the small permitted residual-velocity correction, and
the auxiliary kick/projection energies that cancel. These are separate from
physical gravity work, which is zero during hold. No positive-duration material
capture or absence timer runs during flight/hold. Certified departure retires
the root explicitly; held support retains its original owner.

Each auxiliary projection clears its contact warm-start impulses before solving.
A large previous cache can otherwise cancel the reaction to a tiny retained time
interval in f32, even when the velocity reaches zero. This rule is local to the
auxiliary projection hook; ordinary solving and its settings are unchanged.
Safe hold refuses an unrepresentable positive kick or a reaction outside the
existing momentum tolerance. It never drops that interval or substitutes a
computed impulse for the backend measurement.

Bound each call to eight positive intervals (flight or hold), eight material
impact solves, and seventeen projection solves (two per held interval plus at
most one settlement). Initial support capture is at most one further solve.
Check each counter before invoking the backend. Existing capture batch/record
limits still apply; End, settlement and support records consume the same
per-call record allowance. Flight diagnostics are bounded by the interval cap.
This supplies finite limits for zero-duration work as well as physical time.

Select positive f32 durations without overshooting the remaining requested
time, and retain every positive remainder, including the reproduced 12 ns tail.
Require their exact sum to equal the requested f32 duration; refuse if that
cannot be represented within the interval limit. Timestamp accounting must not
lose a nonzero duration when adding it to elapsed time. Internal impacts and
projections consume zero time. The native clock checks exact f64 addition of each
selected f32 interval and exact reconstruction from elapsed plus remaining time.
If the positive remainder rounds upward in f32, use its lower neighbor and
schedule the retained tail. If f64 cannot preserve a duration or remainder,
return `TimePrecision` atomically. This differs from the independent fixture's
nearest-f32 remainder; exact time accounting is enforced by the native scheduler,
not inferred from fixture results.

## Outputs and refusals

`GravityStep` returns a single execution-ordered timeline, the actual interval
durations, a read-only summary of the persistent numerical ledger, per-phase
backend metrics and total call wall time. Metrics are observational and are not
part of deterministic replay equality. Each timeline item has a frame-relative
time, a monotonic phase index and stable source/body identities. Payloads are:

| Record | Required evidence |
| --- | --- |
| Flight | Duration, actual gravity work, kinetic change, signed work/impulse/position defects, predicted endpoint and uncertainty |
| Episode | Capture transition or proved End, episode ID and canonical source pair; phase distinguishes contact observation from after-flight proof |
| Impact | Incoming/outgoing motion and link to the committed response batch and its existing work ledger |
| Settlement | Original episode, retained allowance, actual dissipation, raw projection impulse and signed correction impulse/loss |
| Support | Original episode, duration, force/reaction, physical work, auxiliary kick/projection energies and correction terms |

For an interval, emit its Flight record and then any proved End; a later impact
at the same timestamp follows that End. At contact, emit the capture transition
before its Impact or first Support record. Settlement precedes subsequent held
intervals and does not emit a new Impact. The timeline is authoritative for
ordering; current capture/response getters remain available and contain only
this public call's committed material observations. Ordinary contact events and
sensor outputs must not retain stale events from a previous call or expose
temporary support-projection events.

The exported types are `GravityStep`, `GravityTimedRecord`, `GravityRecord`,
`GravityFlight`, `GravityImpact`, `GravitySettlement`, `GravitySupport`,
`GravityNumericalSummary`, `GravityErrorAxis`, `GravitySettling` and
`GravityRejection`. `GravityStep.records` contains the timeline, `durations` the
positive intervals, `advances` one metric per raw backend call and `wall_time`
the complete transaction duration. A record's `at` is frame-relative seconds;
`phase` indexes `advances` (Support uses its first projection). Its body/source
arrays use dynamic/fixed role order; Episode payloads use canonical pair order.

`GravityImpact.response_batch` indexes this call's
`world.material_response_batches()`, and `response` owns the corresponding
successful result. The index refers to the current call only; its owned copy
remains usable after later calls replace the world's ephemeral outputs.
Numerical arrays use work/impulse/position order, with allowance `1e-5*scales[i]`.
Returned summaries are copies and cannot reset retained state. Pause returns the
retained summary even for invalidated history; it is `None` only if never enrolled.

The four `MAX_GRAVITY_*` constants expose the interval, impact, projection and
initial-support-capture caps. Callers cannot raise these limits. Report payloads
support exact equality/clone; the complete `GravityStep` deliberately has no
equality implementation because its metrics include nondeterministic timings.
The method's Rustdoc includes a compiled example linking an Impact to its batch.

Use typed refusals so a caller can distinguish `InvalidTimestep`, `TimePrecision`,
`ResponseDisabled`, `UnsupportedScene`, `StaleContinuation`, `PolicyChanged`,
`AmbiguousDeparture`, `SettlingDisabled`, `UnsafeSupport`, `FailedRegion`,
`Numerical`, `NumericalAllowance`, `UnaccountedApproach`, `AdvanceLimit`,
`SolveLimit`, `RecordLimit`, `ExistingEpisode`, `Snapshot`, and
`Response(ResponseRejection)`.
Numerical allowance exhaustion identifies work, impulse or position and the
attempted total/bound. These diagnostics describe a discarded transaction;
they are not partially committed motion or work. Failed proof is never success
with an empty response.

A subthreshold touching approach with no applied material response returns
`UnaccountedApproach` before comparing the subsequent ordinary solver velocity.
This identifies the missing accounting explicitly; the same whole-call rollback
still discards all provisional motion.

## Persistent continuation, separate from an episode

Keep a bounded `GravityContinuation` alongside the existing loading state. The
following are authoritative; derive collider handles and geometry certificates
from current mappings on each admission instead of serializing them as proof:

| State | Content and lifetime |
| --- | --- |
| Contract | Revision, settling choice, dynamic/fixed BodyIds and source roles, constant acceleration, body/geometry identity, material definitions/sections, capture configuration and all integration settings except the last `dt` |
| Initial reference | Initial x, velocity, mass and acceleration; fixed energy, momentum and distance scales computed from them |
| Numerical ledger | Signed accumulated flight work, impulse and position defects; survives every frame and episode transition |
| Current endpoint | Exact poses, velocities and mass properties of both bodies; compare before continuation |
| Optional episode | Canonical pair and ID, retained post-impact energy, complete excursion upper gap, observed-turn flag, held flag and spent settlement |

The fixed scales are `E0=max(0, m*v0*v0/2-m*a*x0)`,
`P0=sqrt(2*m*E0)` and `L0=max(abs(x0),1)`; the unit length is the plate's extent.
After every flight, each signed cumulative defect must remain within `1e-5`
times its corresponding scale. Zero energy/momentum scale grants zero error
allowance. Reject nonfinite values, exceeded bounds and a nonzero contribution
lost in f64 accumulation. Opposite measured defects may cancel. Impact, End,
hold, settlement, pause and restore cannot clear counters or enlarge scales.
The instantaneous-response checks authorize adopting a new velocity without
resetting this ledger. Capacity changes from that checked response do not count
as external material edits. This bounds flight defects, not arbitrary-horizon total
trajectory error or support/impact rounding; those retain separate checks.

Use three lifecycle states: never enrolled, active, and invalidated. Enrollment
commits only with a successful positive gravity step. A refused first call does
not enroll. An invalidated continuation retains its initial reference, policy
and numerical totals for diagnostics, but cannot resume or re-enroll in place.
It drops live episode claims so removed bodies/regions can still serialize.
There is no reset/rebase API in this first slice. A fresh world is a new
simulation; restoring a prior valid snapshot restores its actual prior ledger.

Successful external mutations affecting the admitted bodies, geometry, mass,
material identity, bindings, gravity, forces or relevant solver/capture settings
invalidate the continuation. Failed/no-op setters do not. Public impulses and
velocity/pose edits must invalidate even if a later edit restores the old values;
endpoint comparison alone cannot detect an intervening external intervention.
Internal verified kicks/projections do not call that public invalidation path.
Ordinary positive stepping also invalidates, even if the endpoint happens to
match. A successful ball/flat operation invalidates inside its transaction;
its refusal or pause preserves the gravity state. Read-only queries do neither.

This deliberately differs from clearing ordinary departure proofs. Clearing a
gravity ledger to `None` would allow the next call to manufacture a new allowance.
Every invalidation path must retain the invalidated marker, including after
source retirement, removal of every body, or later creation of new sources.
An unsupported runtime edit not caught by a setter must still fail complete
admission/exact continuity; a refused gravity step must not mutate the original
world merely to record that failure.

## Snapshot layout and validation

Native snapshot **version 6** stores the continuation enrolled by the first
successful positive gravity call. The base physics, v2 capture and v3 response
payload layouts are unchanged. The v6
tail is optional ball tracker, optional flat tracker, then the required gravity
state (active or invalidated). This is a distinct v6 tail, not bytes appended
after an implicitly present v5 flat tracker. Worlds that never enroll continue
to emit their existing v1-v5 format. Once enrolled, they retain v6 after End,
invalidation or switching stepping methods.

Continue reading v1-v5 with exactly their current layouts. They contain no
gravity history and restore as never enrolled; do not infer a continuation from
an existing episode. V6 requires loading and response state and exactly one
gravity payload; reject truncation, trailing data and unsupported revisions.

For active state, validate distinct live sources with actual bindings/body
owners, exactly two roles, at most one matching episode and no pending ball or
flat witness. Check finite current/reference values, positive mass/acceleration,
reference-derived scales, bounded defects, valid policy, and coherent episode
flags/settlement spending. Absent episode means no held or excursion claim.
Held means zero motion and no second settlement. Re-prove the complete scene and
endpoint continuity before stepping; serialized state is not a geometry waiver.

An invalidated state may coexist with live ball/flat evidence and need not refer
to live old bodies or regions. Validate its retained numeric history and reason
structurally, without reviving claims on deleted identities. It must restore as
invalidated. Native snapshots remain same-build continuation data, not an
authentication boundary against fabricated histories.

The implemented history requires initial reference values to be exact finite
f32 backend values represented as f64. Its scales must equal the reference-derived
values; the signed errors must fit those scales. Each plate has a fixed array of
eight optional collider identities with a sorted populated prefix matching its
slice/copy count. The saved endpoint contains pose, velocity and local mass/COM/
inertia, not raw handles. Structural restore validates source ownership,
material definitions, configuration and coherent episode/settlement state.
The implemented staged admission separately certifies complete geometry and
compares current backend settings and exact endpoints. Restore alone does not
authorize motion.

Eight foundation regressions cover policy/ledger/episode roundtrips, public pause,
End without ledger reset, and terminal invalidation after ordinary stepping or
external edits. Edit-and-restore, force clearing, mass refresh, collider
replacement, joints, kinematic targets, source retirement, deletion of all old
bodies and rebuilding cannot revive active state. Failed/no-op public edits and
queries preserve enrollment. Another arrival operation invalidates only its
staged world; early/late refusal preserves the original history and outputs.

Legacy tests independently spell the v2/v3 tail fields and v4/v5 tracker slots,
then require byte-exact roundtrip for versions 1 through 5. V6 checks cover both
optional legacy tracker slots, 27 malformed-history cases, unknown policy/state
tags, missing/truncated/trailing data and mixed pending claims. Invalidated
history may accompany an old-mode departure witness, but remains numerically
validated even after its original identities are gone.

## Native admission and impact evidence

`phase::Staged::prepare` clones through the snapshot boundary, certifies the
entire scene, refreshes mass in the clone, and either constructs initial history
or compares the exact saved continuation. The existing flat operation and this
phase share the filled-rectangle certificate and certified seam filter, with
separate gravity admission. The flat operation still explicitly requires zero
gravity. Gravity admission additionally requires the original two-plate frame,
mass 20 and the exact body roles, poses, motion and force restrictions above.

Continuation compares the actual plate identities, collider prefix, material
definitions, endpoints, mass/COM/inertia, solver settings and acceleration with
the saved state. `dt` is normalized out of that comparison. An invalidated
history, changed policy or old episode without its own gravity history refuses.
No state is adopted into the caller merely because preparation succeeds.

`Staged::impact` consumes the disposable transaction. It rechecks admission and
continuity immediately before solving, requires a touching approach with no
existing owner, and performs one zero-duration solve. Success requires one Impact
transition and one matching response root, unchanged poses and mass properties,
stationary fixed support, nonclosing outgoing motion equal to the callback's
rounded result, and kinetic loss matching the material ledger within the existing
`1e-5` allowance. The available budget must equal actual incoming kinetic energy;
prescribed support work remains zero. A newly failed region rejects the entire
phase after solving. Success anchors the episode and endpoint while preserving
all initial numerical scales and signed defects.

Six regressions qualify this stage:

- 32 unsupported scene mutations preserve the original snapshot and outputs;
  nine stale continuation/policy cases cover raw pose, velocity, gravity, COM
  and solver edits as well as public edit-and-restore. Bookkeeping `dt` changes
  preserve otherwise identical continuation.
- 144 independent impact comparisons vary two incoming speeds, three gravity
  magnitudes, 1/4/8 solver iterations, partitions, duplicates and insertion order.
  Rebound, work, capacity and capture/response records match the independent fixture.
  Removing only the native history envelope gives byte-identical complete
  physics/capture snapshots. Repeating the native solve from a snapshot is exact.
- Pause, noncanonical source/spatial order, temporary-setting restoration,
  unchanged positions, post-impact persistence and repeated preparation preserve
  the committed history. An intervening raw edit after preparation refuses.
- Capture limits, subthreshold or separated approaches, old ownership, initial
  failure and failure caused by the provisional impact preserve state and prior
  nonempty reports. Lowering the speed threshold to zero accounts for the slow
  touching case. Ordinary gravity stepping still reports `ExternalWork`.

These remain internal phases, with a test-only instantaneous wrapper for the
isolated comparison. The public operation calls the scheduler below directly;
there is no separate conversion layer or public partial-step/enrollment API.
The temporary dead-code expectation has been removed.

## Native flight and departure evidence

`phase::schedule` stages a whole call, schedules ballistic arrival/apex intervals,
and commits only after consuming exactly the requested duration. `phase::flight`
re-proves admission/continuity before each drift, uses compensated gravity kicks,
checks the actual endpoint and charges signed work/impulse/position defects to
the persisted initial scales. Analytic and measured endpoint bounds prove
departure; pending evidence survives rounded crossings until that proof holds.
End removes episode ownership while preserving the numerical history. No
positive-duration material capture or absence timer runs during flight.

Every accepted call returns an ordered Flight/Episode/Impact timeline with
frame-relative time, phase index, stable body/source identities, positive
durations and cumulative numerical summary. Identity arrays use dynamic/fixed
role order; Episode payloads retain canonical source order. A backend metric is
measured for each drift or instantaneous impact, and total wall time includes the
transaction. Timing is excluded from replay equality. Calls bound positive
intervals and impact solves separately to eight, with existing capture/record
limits checked before committing any work.

Eight tests add these checks:

- 18 free-flight trajectories through an apex and 72 first landings compare
  analytic motion/arrival energy and the independent fixture over
  gravity 1/10/100, solver iterations 1/4/8, partitions, copies and insertion order.
- 240 three-bounce trajectories compare the independent fixture at single-call,
  30/60/120/240 Hz schedules. Impact times, rebound, capacity and total work agree
  within the existing physical tolerances; episode roots and ordered transitions
  agree. Native snapshots and deterministic reports replay exactly after every
  call, even when retaining a remainder changes its internal interval list.
- Exact frame-end impacts resolve before return. The reproduced tiny tail stays
  positive and charges its measured rounding defects. A `1 - 2^-25` remainder
  consumes its lower f32 neighbor and then the tail, instead of overshooting.
  An exponent-extreme `2^-80` contribution that f64 cannot retain refuses.
- The rounded departure sequence (`0.000002`, f32 bits `0x383b2000`) retires its
  root only with proof and grants a fresh root on return. An apex whose uncertainty
  straddles the exit band retains ownership and refuses an unaccounted return.
- Late interval, impact, record and capture limits roll back provisional damage,
  End events and motion while preserving previous nonempty reports. Invalid time
  refuses without clearing outputs; public pause preserves history.
- Repeated `5e-8` steps exhaust the retained numerical allowance in fewer than
  500 calls. Snapshot restoration and pause cannot reset that allowance; every
  accepted prefix remains within its fixed work, impulse and position bounds.

The support and settling phases below complete the scheduler's two-plate policy.
The public operation uses this same implementation; the general `ExternalWork`
guard and other arrival operations' gravity restrictions remain intact.

## Native safe support and settling evidence

`phase::support` checks the real weight against both regions' safe force and
evaluates the sustained law with measured plastic displacement zero. Failed or
overloaded regions refuse. Initial rest captures exactly one zero-duration
Support transition with no material response, unchanged endpoints and capacity.
After an impact, rest retains that impact's owner. No later held interval
recaptures contact, grants a budget or advances the ordinary absence timer.

Two rounded half-kicks each run a zero-restitution projection around a stationary
force-free drift. Every projection checks unchanged poses, tangent/angular motion,
fixed support and mass properties, verifies the reported impulse against actual
incoming momentum, and reports any small signed velocity correction separately.
Hold requires exact unchanged endpoints, zero physical gravity work, cancelling
auxiliary kick/projection energies and the measured weight reaction. Native
history, including the initial scales and flight defects, stays unchanged.
Auxiliary contacts are hidden from ordinary event output, including when cloning
a held world would otherwise rebuild events from its final contact graph.

For an owned approaching return, `Refuse` returns `SettlingDisabled`.
`BoundedNumerical` requires an observed turn, a complete excursion upper bound
within the departure band, no earlier settlement and safe support. The actual
removed kinetic energy must fit the retained post-impact allowance. Its
Settlement record reports that loss and the measured projection/correction;
it never charges material capacity. The original episode becomes held and
cannot settle twice. Uncertain containment returns `AmbiguousDeparture`.

The independent fixture now distinguishes disabled support, strict rigid support
and optional settling. Its original flight, hold and projection algorithms remain
independent of the native implementation. Eight added tests qualify:

- Strict safe rest and exact zero rebound, while unresolved returns and overload
  refuse. Full material damping alone can still leave a rounded rebound; the
  exact-zero case uses speed 0.05 and impulse 1 with backend inverse mass 0.05.
- 240 native stationary cases cover both policies, three consecutive frames,
  1/30/60/120/240 Hz, 1/4/8 solver iterations, partitions, copies and insertion
  order. Weight reaction, zero work, unchanged capacity and one retained owner
  agree with the independent fixture. Native replay is exact.
- 72 small-hop cases settle once under bounded policy, retaining their Impact
  owner and flight-error history. The finite loss matches the independent
  fixture, and later support adds no damage or settlement.
- 18 longer bounce-to-rest histories compare independent impact counts, ordered
  roots, capacity, final energy and one terminal settlement at 120/240 Hz. Genuine
  departures still grant new roots before the final contained excursion.
- The tiny held tail after a speed-0.01 hop at one solver iteration remains
  positive and reports its own nonzero reaction. Clearing projection warmstarts
  fixes the reproduced zero-impulse report without changing ordinary physics.
- Exact frame-end zero rebound resolves before return and retains its owner on
  the following held step. Pause, restore, policy mismatch, real held-state
  edit-and-restore, ambiguous return, failed support, overload and underflowed
  kicks preserve the specified history or refuse atomically.
- Projection limits fail before the requested solve, including after settlement
  or the first held projection/drift. Initial capture, interval and late record
  limits preserve the original world and previous nonempty reports.

Each backend drift, impact, initial support capture and projection has a metric;
the ordered timeline links Support to its first projection and Settlement to
its solve. Initial Support is emitted first at the same timestamp. Durations
include every held remainder, and support/settlement records consume the same
per-call allowance as End. The public operation returns these reports directly.

## Public API qualification and remaining work

All 660 native trajectory/support comparison configurations now call
`step_material_flat_gravity`, while their independent fixture implementations
retain separate physical phases and checks. They check each impact's response index against the committed
batch as well as the physical balances, ownership and replay described above.
The separate 144 instantaneous-impact comparisons still isolate the internal
zero-duration solve; public `dt=0` remains pause.

`tests/material_gravity.rs` is a downstream integration test using only public
types and methods. Six tests construct real registered scenes and cover:

- 24 landing/partition/copy/order/solver configurations with exact replay,
  capacity charges, actual final energy, canonical episode identities and
  response links; arrivals at the exact frame endpoint resolve before return.
- Both policies at rest and after exact zero rebound, one bounded settlement,
  tiny held tails, unchanged support capacity, policy mismatch and pause.
- Invalid time and edit-and-restore preserve nonempty prior reports; failed
  initial/impact regions, overload, subthreshold approach and unsupported gravity
  refuse without enrollment. A zero configured speed threshold accounts for
  the slow approach instead of silently dropping it.
- Exact representable remainders, unrepresentable time, default interval caps,
  late record/capture refusal and numerical exhaustion across repeated snapshots.
- Unenrolled pause, v3-to-v6 enrollment, terminal invalidation after ordinary
  stepping, source retirement/restoration with retained history, and unchanged
  flat-arrival and ordinary material-response gravity guards.
- Rounded frame endpoints whose analytic arrival is still in the future:
  the observed contact resolves at the current endpoint, replays exactly and
  retains its owner until departure. Capture thresholds and record limits
  instead refuse the complete call without enrolling or moving the world.

Checkpoint review reproduced the endpoint omission in both native scheduling
and the independent fixture. With mass 20, incoming speed 0.25, gravity 1 and
one solver iteration, gaps `0.0001`, `0.02` and `1` at respective durations
`0.0003996805`, `0.07015621` and `1.1861407` rounded to x=0 with positive
velocity, yet returned no response. Both schedulers now check the observed
approach before returning. Separate regressions fail against their old checks.
The fixture correction also accounts for contained returns in the ending frame;
its drift, impact, support, settlement and numerical tolerances are unchanged.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier
cargo test -p engine-rapier --test material_gravity
cargo doc -p engine-rapier --no-deps
```

This completes the bounded two-plate gravity API checkpoint and its local review.
Prepare this slice for PR review before expanding admission or coupling it to
a gameplay host.

Three-plate support histories remain useful independent evidence. Promote them
only after their later transitions and failure boundaries have their own native
contract. Overloaded deformation, changing gravity/drivers, broader masses,
translated/rotated frames, free rotation and general geometry remain later
work. #328 owns the material lifecycle/failure consequences; this bounded API
does not finish #327 or select gameplay calibration.
