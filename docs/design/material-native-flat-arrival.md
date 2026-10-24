# Native flat-face arrivals (#327)

`PhysicsWorld::step_material_flat_arrivals(dt)` advances a complete scene of two
or three parallel solid plates, including an arriving plate loading a resting
interface. It promotes the [independent fixture](material-flat-arrival.md) into
an explicit opt-in operation with scene admission, bounded work and whole-frame
rollback. No scenario enables it by default; this does not complete #327.

## Complete scene admission

The operation checks all bodies, colliders, material registrations and bindings
on every positive call. It admits:

- Exactly two or three plates, each with local half-extents `(0.5, 1)`, identical
  fixed orientation, and one distinct live material region/source per body.
- One, two or four uniform slices along each plate's local y axis, with one or
  two identical copies of every slice. At most eight colliders per body and 24
  total. Slice counts and copy counts may differ between bodies.
- Dynamic bodies with rotation locked, translations unlocked, sleeping disabled,
  finite positive mass and positive effective inverse mass on both axes. Fixed
  and constant-velocity kinematic supports are allowed; at least one body must
  be dynamic. All bodies have zero angular velocity.
- Relative motion normal to the common faces, allowing only rounding-scale
  tangent residue, with finite positive face overlap for every pair. A whole
  scene may translate in either world axis. Spatial order is determined from
  actual positions and need not match source or body-ID order.

Admission refuses gravity, forces/torques, damping, joints, friction, sleeping,
CCD/soft CCD, contact skin, sensors, disabled bodies/colliders, custom hooks or
collision filters, nondefault dominance, and position-driven kinematics. It also
refuses missing/stale registrations, multiple sources on one body, orphan or
unbound colliders, and unsupported geometry. Capture's touching tolerance must
remain `1e-5`; its speed threshold and capacity limits still apply.

Mass properties are refreshed inside the staged world before solving. A finite
center-of-mass offset is allowed: with rotation locked, it does not change face
drift or the active impulse Jacobian. Even symmetric slices can acquire a small
offset from f32 mass summation. The tests cover an intentional offset too;
actual motion must still pass the drift checks.

## Certified exterior boundaries

Admission verifies the exact local interval of every slice, not merely a shared
bounding box. The distinct intervals must partition `[-1, 1]` into one, two or
four equal parts with uniform copy counts. Each cuboid has half-width 0.5, no
local x offset, and no local rotation. The dimensions are binary-exact. Gaps,
partial overlaps, nonuniform duplicates and displaced face planes are refused.

Only after this proof can temporary solver hooks suppress an internal tangent
face. The certificate records which collider top/bottom faces are strictly
inside the filled union. The callback transforms its normal into local axes and
removes only those certified internal faces. Real outer edges remain visible.
An arbitrary compound collider is never presumed solid between its pieces.

The same temporary hooks remove speculative points and preserve eligible
touching points. Admission refuses pre-existing custom hooks, and their original
flags are restored after each advance. Ordinary stepping and the ball operations
do not use this seam filter.

## Transaction, timing and persistence

For each pair, compute the f64 normal gap and relative approach from actual f32
positions/velocities. The earliest positive `gap / approach` bounds the next
advance. The complete transaction is limited to eight disposable previews and
eight actual advances. Each preview observes capture duration zero and computes
the current material response; its integrated positions are discarded.

The actual advance must reproduce the preview's response records and rounded
velocities. Ordinary solving must leave those velocities unchanged. Endpoints
must agree with constant drift within the existing per-coordinate bound
`8*EPS*(abs(start)+abs(velocity*dt)+1)`, with penetration no greater than `1e-5`
and face overlap greater than `1e-5`. Tangential edge entry, sliding and corners
are outside this operation. The fixture documents the arithmetic and checks.

Every failure preserves the original world, capacities, capture/response outputs
and any prior departure evidence. There is no automatic ordinary-physics
fallback. An exact frame-end arrival responds on the next positive call; a
zero-duration call clears transient outputs without advancing persistent state.

Public types live in `response::flat_arrival`: `FlatArrivalStep`,
`FlatArrivalRejection` and `FlatEpisodeEvent`. Successful output includes actual
durations, backend metrics for every preview/advance, total call time, and
capture transitions with frame-relative timestamps and substep indices. All
events identify either `FlatEventPhase::BeforeSolve` capture observations or
`FlatEventPhase::AfterDrift` proved departures. They are ordered by execution,
then source within a phase. A departure precedes a new impact observed at the
same timestamp in the next substep.

After a completed drift, the operation applies the independently qualified
[flat departure proof](material-flat-departure.md). It projects the existing
coordinate-error boxes onto the face normal and requires both actual and
predicted gap bounds to exceed twice capture's touching tolerance. Strictly
outward drift must start within the eligibility band or continue a pending
witness with the exact episode, full endpoints and face normal. An external
rotation cannot reuse endpoint-only evidence. A proved departure removes the
old episode so a genuine return can start a new impact. Its `End` notification
counts against the existing capture record limit.

The first successful positive call selects snapshot **version 5**. The base
physics, capture and response payloads are unchanged; the new tail contains an
optional ball tracker followed by the flat tracker. Flat entries contain a
canonical source pair, episode ID, two source-ordered endpoints and unit face
normal. Versions 1 through 4 retain their old formats and remain readable;
worlds that never enter flat stepping continue to write those formats.

Restore validates at most three entries, canonical distinct live sources with
bindings and distinct body owners, matching episode IDs, finite endpoints and
finite unit normals. Both tracker kinds cannot contain pending evidence at
once. Missing/trailing data is rejected. Stale poses or shapes may roundtrip:
complete scene admission re-proves the filled plate geometry and source
ownership before stepping, and exact continuity determines whether the old
witness can continue. Structurally valid evidence is not a geometry exemption.

A successful flat call clears ball evidence inside the transaction. A successful
three-ball call clears flat evidence; ordinary positive stepping and binding or
region retirement invalidate both. Refusal and pause preserve evidence. Empty
trackers remain in the snapshot, so a version-5 world remains version 5 across
mode switches. The snapshot does not automatically choose the next stepping
operation: replay calls the same operation with the same durations.

## Qualification and retained refusals

All 2,560 positive two/three-plate fixture combinations now compare native and
independent stepping, including exact snapshots, capture/response batches,
durations and replay. They cover partitions, duplicates, insertion order,
rotations, common velocities, partial overlap, dynamic/fixed/moving supports and
30/60/120/240 Hz. Existing work, damage, width and momentum checks remain intact.
The independent fixture now supplies its own departure tracker for these
comparisons; physical/capture snapshots and witness bytes are compared
separately, without sharing the native departure implementation.

The departure sweeps also compare native output and exact replay at every frame:
640 dynamic chains with four impacts, 96 stationary fixed-support cases with
seven impacts, 16 additional fine-subdivision dynamic cases, and 40 rounded
crossings. These cover rates through 10,000 Hz and preserve the existing work,
material balance and momentum checks.

Additional native tests cover 33 admission mutations with previous successful
outputs preserved, frame-end timing, pause, invalid time, capture overflow,
an advance limit after staged response, reflected source/spatial order, event
collection, center-of-mass offsets and transactional ball-witness cleanup.
Four departure boundary tests add snapshot corruption and mixed-mode rejection,
flat-to-ball invalidation, endpoint/normal edits, reversal, late rollback,
simultaneous notification limits, and reflected pending witnesses whose source
order differs from spatial order.

Conservative refusals remain deliberate:

- A touching approach below capture's impact-speed threshold returns
  `Unaccounted`. Lowering that threshold to zero accounts for the tested heavy,
  slow impact. Accelerating an existing support cannot invent a new impact root.
- A relative-speed-0.25 impact at common velocity +/-1,000,000 still returns
  `Response(Backend)` when f32 cannot represent the ledger's rebound. The work
  tolerance and strict approaching-contact check are unchanged.
- Teleporting the already resting three-plate fixture to its reflected layout
  can leave a cached B-C manifold reporting distance -2. The operation returns
  `Numerical` and rolls back instead of accepting correction work. A fresh
  reflected pair completes normally. Geometric admission alone does not make
  every retained backend contact valid.
- A rapid chain against a prescribed support moving at `(17, -11)` completes
  two impacts in 0.0015 seconds, then rejects a 0.03-second continuation with
  `Response(Backend)` at a smaller later response. Departure evidence does not
  waive the rounded-energy check; prior outputs and pending evidence roll back.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib flat_arrival
```

Arbitrary compound geometry, mixed curved/flat shapes, free rotation, other
loading classes, pulse calibration and #328 failure consequences remain
unfinished. This operation is still an opt-in bounded scene, not general
contact-set admission.

The separate [gravity/support fixture](material-gravity-loading.md) documents
the force-integration, impulse-reporting and deformation-work evidence behind
the qualified [native gravity operation](material-native-gravity.md). That
explicit two-plate API adds constant gravity, safe support and optional bounded
settling with persistent numerical accounting. It does not relax this
operation's gravity guard or inherit this operation's clear-on-mode-switch
departure semantics for the gravity ledger.
