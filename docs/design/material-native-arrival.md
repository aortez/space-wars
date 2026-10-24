# Native isolated-ball arrivals (#327)

`PhysicsWorld::step_material_ball_arrivals(dt)` is an explicit, experimental
step operation for a complete world containing two registered balls. It turns
the [timed-arrival experiment](material-timed-arrival.md) into a bounded native
path. It preserves incoming motion until physical contact, then invokes the
existing transactional material response. Ordinary `step` and ordinary CCD
keep their existing admission and fallback behavior. No scenario enables this
new operation automatically.

## Admission

Enable capture and material response and register/bind both materials as
described in [native response](material-native-response.md). Call this operation
instead of ordinary `step` for each frame, including replay. Admission examines
the complete body/collider registries, not merely the current contact graph:

- Exactly two enabled bodies and two material regions, with at least one dynamic
  body. Dynamic bodies must have positive finite mass, rotation locked,
  translations unlocked and sleeping disabled. Neither body may rotate. Fixed
  supports and constant-velocity kinematic supports are supported; position-driven
  kinematics are refused.
- Two to eight centered ball colliders. All colliders on a body must have the
  same radius and share one registered source. Every collider must be bound.
  Coincident duplicate representations share width and work.
- No other bodies, joints, gravity, user forces/torques, damping, ordinary CCD,
  soft CCD, sensors, skin, custom dominance, custom hooks or custom collision
  filtering. Collision/solver groups must use the engine's default ALL groups.
- Capture's touching tolerance must be `1e-5`. Existing capture/output limits
  remain enforced across the complete operation.

This is intentionally a narrow whole-world contract. It does not admit a pair
inside a larger scene, even when other objects currently appear distant. Initial
penetration beyond `1e-5`, numerically ambiguous grazing trajectories, unusable
arrival times and invalid final geometry refuse explicitly. A new approaching
kick into an already-spent touching episode is `ExistingApproach`; ordinary
separation grace must rearm that episode first.

Every positive approach that starts touching must produce an accepted material
response, even below capture's `min_approach_speed`. If that threshold classifies
the contact as support and no response applies, the operation returns
`NumericalArrival` and rolls back. A low speed is not proof of negligible energy:
a sufficiently massive body can carry substantial work. Resting and separating
touching pairs do not require a new impact response.

## Stepping and transaction

The operation uses at most two Rapier advances. Analytic relative ball motion
selects the first arrival time with f64 arithmetic and `libm`. The first advance
drifts for the lesser of that time and the frame duration. A temporary local
hook suppresses only separated speculative solver points, so they cannot brake
the incoming motion. Any unexpected touching/penetrating point rejects the
operation. Velocity must remain exactly unchanged during flight.

When frame time remains after arrival, the second advance runs normal native
capture and material response. The existing `1e-5` checked-contact tolerance
still applies. No body is snapped to contact and no global solver setting is
changed. A predicted hit with time remaining must produce an accepted material
response; missing episode ownership cannot silently consume the work.

The **entire operation runs in a snapshot clone**. Only success replaces the
original world. Admission, numerical, capture-limit or response failure returns
`Err(ArrivalRejection)` with original poses, velocities, capacity, episodes and
prior transient outputs intact. There is no automatic ordinary-physics fallback
or elapsed-time consumption on error. The caller decides whether to retry with
ordinary `step`; it must inspect the returned error, since preserved diagnostics
still describe the previous successful step.

This favors a reviewable transaction over speed. Cloning cost depends on the
existing backend snapshot storage, including historical arena capacity; the
two-body bound is not a fixed byte-allocation guarantee. Runtime profiling also
counts attempted staged solves even when the simulation state is rolled back.

## Time, outputs and snapshots

Capture/output state resets once per outer call. Both internal advances append
to the same bounded capture stream, with monotonic substep indices; durations
sum to the requested frame time. Free-flight absence advances episode grace,
and a later arrival can start a fresh impact after separation. Failure records
and impulses remain available in material response batches. Ordinary contact
events retain the final advance's standard view; the flight cannot contribute
physical impulse events in an accepted operation.

An arrival exactly at the frame endpoint responds at the next call's start,
before additional integration. Zero duration clears transient outputs without
spending work or aging episodes. Negative/nonfinite duration returns an error
without mutation.

`ArrivalStep` exposes `flight_seconds` and at most two `advances` containing
ordinary `PhysicsStepMetrics`. Timings are per backend advance and exclude
snapshot staging overhead; entity counts are not additive. Snapshot schemas
are unchanged. Callers must choose this step operation again after restoring;
there is no hidden persistent stepping mode.

## Evidence and remaining boundary

Native tests cover 30/60/120/240 Hz, low and high incoming energies, multiple
gaps, frame-end arrivals, moving supports, duplicate colliders, oblique dynamic
targets, misses, exact replay, preserved capture time, pause, separation/reimpact,
and atomic refusal including overflow after an already-staged flight.

The tests also exposed repeated f32 velocity rounding when a low-energy response
was split over duplicate contacts on a moving support. The checked backend
impulse operation now accumulates each body's response in f64 and rounds once
before validating and committing it. One/two/four representations produce the
same final velocity, and the existing energy tolerance remains unchanged.

The native response also validates the rounded velocities against its energy
budget before backend or material commit. A mass-20 impact at relative speed
0.25 on a support translating at 1,000,000 cannot represent the required rebound
accurately in f32. It now returns `Response(Backend)` with the original world
unchanged. Tests cover both transport directions and one/two/four duplicates,
plus massive subthreshold touching impacts and host-validation veto atomicity.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib native_arrival
```

This is a review checkpoint before expanding admission. Multiple physical
contacts and simultaneous arrivals need a complete-scene scheduler and shared
batch ownership; flat, offset and rotating geometry need their own arrival
proofs. Friction, joints, external/sustained/connection work and soft queries
remain unfinished. The pulse allocation model still needs physical calibration,
and #328 owns failure consequences. This does not complete #327.

The follow-up [three-body arrival experiment](material-multi-arrival.md) proves
bounded sequential, simultaneous and supported arrivals in collinear fixtures.
It retains a timer-only rapid-reimpact refusal; the optional
[verified-departure fixture](material-verified-departure.md) now demonstrates
fresh episode ownership after proven separation. Native integration still needs
complete admission and endpoint-event semantics. Runtime admission is unchanged.
