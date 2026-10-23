# Opt-in native material response (#327)

`engine_rapier::world::loading::response` connects the canonical contact stream
to persistent material capacity and the checked paired-impulse operation. It is
an experimental adapter for a deliberately restricted class of discrete impacts.
No scenario enables it by default, and it does not complete #327.

The [shared-energy experiment](material-shared-energy.md) describes the
`shared-elastic-pulse-v1` allocation hypothesis and its calibration limits. This
adapter uses that model without changing the isolated material law or global
Rapier settings.

## Registration and ownership

Enable contact capture, then call `PhysicsWorld::enable_material_response()`.
For each logical region, register an existing incarnation token with
`register_loading_material(token, region, section)`. The input `Region` carries
the material parameters, represented volume and precise remaining capacity;
`Section` provides material thickness and deformation depth. Collider bindings
continue to use the existing `ContactSource` API.

Registration is immutable for that incarnation. Retirement removes its material
balance and collider bindings; stale tokens cannot register replacement state.
Duplicate registration cannot refill capacity. `loading_material_remaining`
returns the authoritative `f64` balance. Numeric material parameters are stored
and identified with `Material::identity`; presentation names are not serialized.

A new `Impact` episode starts response evaluation. Existing support episodes can
participate in the same load, so a resting interface can receive transmitted
work. Every positive-energy independent component must contain a new impact
start; an unrelated new collision cannot authorize charging ongoing motion in
another component. Continuing contact alone does not allocate another budget.

This preserves the capture layer's time-based separation grace and its known
limits for rapid departure/reimpact. Programmatically accelerating a body while
it stays in contact does not create a new impact episode; that work needs the
future external/sustained-load path.

## Admission and geometry

The first adapter admits complete retained solver-contact batches with:

- At most 64 bodies in the world, 32 physical source-pair patches and 256 solver
  points. Traversal also uses capture's visit bound.
- Ball-versus-ball or aligned cuboid face-versus-face solver contacts, no
  friction or tangent drive, and absolute separation at most `1e-5`. Ball width
  comes from the law's `patch_width_from_radii`; flat width comes from the union
  of actual contact intervals. Thickness and depth come from registered sections.
- Bound colliders and registered materials on both sides of every solver point.
  Unbound approaching contacts are reported even when capture cannot assign an
  episode to them. Approaching speculative contacts are also reported before
  a physical impact exists; see the arrival evidence below.
- No world gravity, joints, user forces/torques or damping. Sleeping bodies,
  CCD-enabled bodies, nondefault dominance, position-driven kinematics and
  rotating prescribed supports are refused. Constant translating supports are
  supported, with signed prescribed work in the response record.

These are conservative whole-world/whole-batch checks. An unrelated unsupported
body or contact can refuse an otherwise supported impact. This prevents silently
solving an incomplete coupled set while that set's closure is still being built.

Ball contacts sharing a source pair must have identical radius, point, normal
and body identity. Exact coincident duplicates form one physical patch.

Flat contacts require zero angular velocity on both bodies and zero effective
inverse inertia on dynamic bodies: rotation must remain locked throughout the
response, not merely start at rest. The common normal must align with a face
axis on both cuboids (within `1e-6`). Any fixed orientation is supported. For
each collider-pair manifold, project its actual solver points onto the common
tangent to obtain an interval. Merge those intervals in coordinate order;
overlap and duplicates add no width. This supports different partitions on
either side and partial face overlap. Collider partition size never sets depth.

All intervals for one source pair must be coplanar and connected within `1e-5`;
tiny tolerated gaps do not add area. Disconnected or noncoplanar footprints are
`AmbiguousPatch`; zero-width contact is unsupported. Mixed shape pairs, corners
against nonparallel faces, and flat contacts with rotational freedom are also
unsupported. Independent surfaces need distinct host patch identities.

Each patch receives one material evaluation and budget share. The flat patch's
representative point is its union centroid. Response is split equally over
solver points in canonical collider order. With rotation locked, each point has
the same active normal Jacobian, so this split preserves the modeled velocity
response. It does not model pressure distribution or support reaction moments;
freely rotating faces need a richer response representation.

## Transaction and response

At the pre-solve callback:

1. Capture canonical records and episode transitions for the substep.
2. Check admission and aggregate physical patches in source order.
3. Compute the coupled compression budget and pulse shares.
4. Copy participating region balances into staging, evaluate the material law
   in source order, and charge those staged balances. Later patches see earlier
   charges to a shared region. Resolve excess as blocking failure dissipation.
5. Prepare all output records and paired impulses. Derive a common rebound
   coefficient per coupled component from its recoverable energy.
6. Ask the backend to apply the complete response. Only after acceptance commit
   staged balances and publish the successful response batch.

The backend operation leaves ordinary constraints enabled and clears the
targeted restitution/warmstarts. The pre-applied impulse is not included in
Rapier's later force-event totals. `MaterialImpact::normal_impulse` records the
total physical patch impulse separately, alongside geometry, material identities,
episode, before/after balances and the full outcome ledger. `AppliedResponse`
also records law/allocation versions, incoming episode IDs, component budgets,
coefficients and signed prescribed work for the full response including rebound.

`Outcome::failed` is emitted only on the first capacity exhaustion. Failure
consequences remain host/#328 work: this adapter does not remove bodies, open
collision paths, create fragments or grant protection.

## Refusal and bounds

Failure at capture, geometry, budget, material evaluation or backend application
leaves that batch's material balances and material response untouched. Ordinary
physics runs. The result is explicitly `ResponseBatch::result = Err(reason)`;
this is a physics-only fallback, not a successful zero-damage result. The host
must inspect `material_response_batches()` and `material_response_diagnostics()`
before treating its damage simulation as complete.

Refusal consumes the observed impact start: the adapter does not retry it using
the ordinary solver's residual velocity on the next frame. A new episode after
separation can be evaluated normally. Earlier accepted substep batches remain
committed if a later batch is refused.

When no physical `Impact` exists, a bounded scan still checks approaching solver
contacts. Bound contacts separated beyond capture's touching tolerance report
`SpeculativeArrival`; unbound contacts report `UnboundContact`. This is a
conservative warning of unmodeled solver work, even if a speculative point is
too far away to collide this substep. Separating/resting points do not trigger
it. No material evaluation, impulse or synthetic physical episode is created.
If a physical impact later starts, its ordinary admission checks still apply;
this does not recover work already spent during the speculative fallback.
Hosts must retain refusals across steps when assessing complete accounting.

At most 16 response batches are retained per outer step. If output is full,
response is refused before material or backend mutation and the diagnostic
reports `OutputLimit`. Earlier records remain intact. Each applied record is
bounded by the 32-patch budget; registry size is bounded by capture's region
limit. Pause clears transient outputs and diagnostics but spends no work and
does not advance episode timers.

## Persistence and evidence

Response-enabled worlds use physics snapshot version 3: the existing base and
capture state, followed by numeric material/section parameters, precise balances
and law/allocation version identifiers. Restore validates token liveness,
registry bounds, material/section ranges, finite balances and model versions.
Capture-only worlds retain version 2; capture-disabled worlds retain version 1.
Existing versions 1 and 2 remain readable.

Response output and diagnostics are transient. Same-build continuation restores
capacities and episodes exactly and does not replay an already spent impact.
A durable failure-consequence queue is not implemented; #328 must define the
host boundary between consuming these outputs and checkpointing consequences.

Native regressions cover:

- Actual energy loss and prescribed work, reversed insertion, moving supports,
  and one/two/four duplicate collider representations with the same mass.
- Flat faces with one/two/four partitions, mismatched partitions, duplicates,
  rotated orientation and partial overlap; equal material outcomes across
  equivalent representations, actual energy closure and exact snapshot replay.
- Flat-face refusal for rotational freedom, zero-width corner contact,
  disconnected footprints and noncoplanar intervals, with unchanged capacity
  and exact ordinary fallback motion.
- Speculative arrivals with/without CCD at 30/60/120/240 Hz, reversed insertion
  and moving supports; explicit fallback before any physical episode, unchanged
  capacity and exact ordinary motion. Unbound arrivals and separating/resting
  speculative contacts are distinguished.
- Real CCD-clamped arrival poses, geometric refusal even after disabling the
  CCD body guard, and exact snapshot continuation across the clamp.
- Transmission through a resting dynamic chain, stable shared-region charging,
  an existing support episode receiving a later impact, and failure exactly once.
- Unbound contacts, forces, gravity, friction, damping, unsupported geometry,
  ambiguous source patches and independent motion without a new impact owner.
- Capture/output limits and an injected backend refusal after material changes
  have been staged, checking unchanged capacities and ordinary fallback motion.
- Retirement, duplicate registration, pause, invalid snapshots and exact
  serialized continuation through separation and a new impact.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib world::loading::response
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib
```

## Arrival evidence and next boundary

Removing the CCD body guard does not yield a complete impact path. Native
regressions use a mass-20 ball, relative speed 400, radius 0.5 against radius 0.5,
zero friction/gravity/damping and one or four configured CCD substeps:

- At an initial gap of 0.001, the retained solver point is speculative. The
  solver spends over 99% of the incoming 1,600,000 energy units before capture
  reports a physical `Impact`. This also occurs with CCD disabled. Previously
  the native response produced no refusal at this boundary; it now reports
  `SpeculativeArrival`. The test preserves this negative evidence instead of
  calling the unchanged material balance a successful zero-damage result.
- Starting farther away preserves incoming velocity through the CCD clamp,
  but the next physical contact can be outside the response's `1e-5` touching
  tolerance. In the fixed-support 60 Hz probe, gap 0.01 produced penetration
  0.065; gap 0.1 produced about 0.000098. Rapier's clamp enforces minimum motion
  of 15% of body CCD thickness, and shape-cast tolerances also affect arrival.
  Disabling CCD after the clamp still leaves the geometry refusal, proving
  that the body flag is not the only obstacle.

CCD remains refused. Widening the penetration tolerance alone would admit
position-correction work that the ledger does not own. Arrival handling must
preserve incoming work before speculative solving, establish an arrival
point/time suitable for the checked response, and prove that subsequent solve
and correction cannot spend that work twice. Keep default physics unchanged
outside an explicitly admitted material path.

The [timed ball-arrival experiment](material-timed-arrival.md) now demonstrates
that sequence for an isolated pair: drift to physical contact without speculative
braking, then use the existing native material response. It preserves incoming
work and matches touching-reference damage across tested timesteps, but remains
fixture-only pending complete-scene admission and event-lifetime integration.

Next work is contact-set admission, especially CCD arrivals, then additional
geometry and loading classes. Freely rotating faces, mixed curved/flat contact,
friction/constraints and supported external work remain unsupported. The pulse
split still needs physical calibration. Sustained loads, connections, soft
queries and gameplay failure consequences remain unfinished acceptance work.
