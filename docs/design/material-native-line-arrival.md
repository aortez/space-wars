# Native three-ball arrivals (#327)

`PhysicsWorld::step_material_line_arrivals(dt)` promotes the
[three-body scheduler](material-multi-arrival.md) and
[verified-departure rule](material-verified-departure.md) into an explicit,
experimental runtime operation. It handles sequential and simultaneous arrivals,
resting supports and rapid reimpacts within its admitted scene. No scenario
enables it automatically. The separate [two-ball API](material-native-arrival.md)
retains its existing contract.

The [oblique three-ball operation](material-oblique-arrival.md) now shares this
scheduler for two-dimensional translations. This line operation keeps its
horizontal admission and original numerical bounds; its public type names
remain available as aliases of the shared ball-arrival result types.

## Admission

Enable capture and material response and register/bind all materials as described
in [native response](material-native-response.md). The complete world must contain:

- Exactly three enabled bodies and three distinct registered material regions,
  with at least one dynamic body. All centers lie on the same horizontal line;
  vertical and angular velocities are zero. Each body's colliders share one
  source, centered on the body, with radius exactly 0.5 and one to four coincident
  representations. Every collider must be bound to its body's live region.
- Dynamic bodies with positive finite mass and inverse mass, rotation locked,
  translations unlocked and sleeping disabled. Mass properties are refreshed
  on the staged world; all centers of mass must be at the body origin. Fixed
  and constant-velocity kinematic supports are admitted; position kinematics are
  refused. Spatial order need not match source or insertion order.
- No gravity, user forces/torques, damping, joints, friction, sensors, contact
  skin, CCD/soft CCD, custom dominance, custom hooks or collision filtering.
  Collision/solver groups use the default ALL groups. Capture's touching
  tolerance is `1e-5`; existing capture and response limits remain in force.

Admission checks the complete registries, including distant objects. Initial
penetration beyond `1e-5`, invalid endpoint geometry or numerical ambiguity
refuses the operation. The restriction is not a general 2D or arbitrary-radius
arrival algorithm.

## Scheduling and transaction

The operation stages the complete frame in a snapshot clone. Each advance makes
a disposable preview clone and runs native capture/response for the remaining
duration. Preview capture observes zero elapsed time. The scheduler reads the
rounded velocities immediately after material response, before ordinary solving,
and recomputes all pair arrival times from those velocities and the current
positions using f64 gap/approach arithmetic. It then performs the actual advance
to the earliest arrival or the frame endpoint.

The actual response and immediate velocities must match the preview. Ordinary
solving must leave those velocities unchanged. Final geometry and displacement
must pass the departure proof's numerical checks. The preview's integrated
positions are discarded: they may have passed a later contact during the full
remainder and do not supply arrival or departure evidence.

Temporary hooks suppress separated speculative contacts and separating contacts
only within this admitted straight, force-free scene. Resting touching contacts
remain in the shared response. Nonseparating points within `1e-6` are processed
together in canonical order; distinct upcoming arrivals are not combined early.
An approaching touching pair still unresolved after preview is `Unaccounted`,
including subthreshold approaches with substantial kinetic energy. The checked
backend continues to reject rounded responses that cannot realize their energy
ledger.

There are at most **eight actual advances and eight previews per call**. Capture
resets once and accepted advances share its batch/record bounds. Endpoint End
events consume the same record budget. Any admission, response, numerical,
preview mismatch or limit failure discards the staged physics, material balances,
episodes, pending witnesses and outputs. The original world and its previous
outputs remain intact; no automatic ordinary-step fallback consumes time. The
returned `LineArrivalRejection` describes failure, since preserved diagnostics
still describe the previous call.

## Persistent departure evidence and snapshots

After each accepted drift the implementation applies the verified-departure
fixture's clearance rule unchanged. The actual displacement must agree with
`start + velocity * duration` within the per-body allowance:

```text
error[i] = 8 * f32::EPSILON * (abs(start_x[i]) + abs(velocity_x[i] * duration) + 1)
exit = 2 * capture.touch_tolerance = 2e-5
clearance = min(actual_end_gap, predicted_end_gap) - error[i] - error[j]
```

Retirement requires separating motion, `clearance > exit`, and a starting gap
within `exit + error[i] + error[j]` or continuation of pending evidence. An
eligible drift that lacks sufficient clearance retains its episode ID and exact
actual endpoints. Continuation requires the same canonical source pair and
episode, matching full x/y endpoints, and another separating drift. This keeps
the reported rounded crossing from losing evidence between subdivisions. Pose
discontinuities, reversed motion and replaced episodes discard the witness;
starting within the original band can establish fresh evidence. The allowance
is specific to this restricted geometry, not a general Rapier error bound.

The first successful positive call uses snapshot **version 4**, appending a
bounded tracker after the unchanged capture and response payloads. Versions
1–3 remain readable, and worlds that never use this operation keep their prior
snapshot version. The tracker has at most three witnesses. Restoration checks
canonical live/bound sources, matching live episode IDs and finite endpoints;
truncated or trailing payloads are rejected.

Positive ordinary stepping, new collider bindings, unbinding and region
retirement invalidate pending proofs. Pause preserves them. The empty tracker
can remain in version 4; it does not select a stepping mode. Replay must call
`step_material_line_arrivals` again explicitly. Retirement changes episode
ownership only, not material capacity or region identity.

After a world has used [native flat stepping](material-native-flat-arrival.md),
it retains snapshot version 5, which stores the trackers separately. Successful
ball stepping clears flat evidence, and successful flat stepping clears ball
evidence. Refusals preserve the original tracker; snapshot restoration never
automatically selects either operation.

## Time, events and cost

`LineArrivalStep::events` is the complete material episode stream for the call.
Each `LineEpisodeEvent` carries seconds from the call's start, an advance index,
and one of two phases:

| Phase | Observation | Ordering |
| --- | --- | --- |
| `BeforeSolve` | Copy of the accepted capture batch's events at advance start | Canonical source order |
| `AfterDrift` | Verified End events at the accepted endpoint | Canonical source order |

Execution order resolves equal timestamps: an advance's endpoint Ends precede
the next advance's pre-solve events. Existing `loading_batches()` retain their
pre-solve observations and actual durations; their events are the same
observations copied into this stream, not additional events to count. Endpoint
Ends are available in the returned stream. Material response batches retain
their impulses, failures and ledgers. Ordinary contact events retain only the
final advance's standard view.

An arrival exactly at the frame endpoint responds at the start of the next
positive call. Zero duration clears transient outputs without aging episodes or
spending work and returns empty advance/event arrays. Invalid duration refuses
without mutation. Actual f32 durations sum to the requested duration within f32
rounding tolerance; no extra sub-ULP advance consumes subtraction residue.

The result reports each actual duration, backend metrics for both `advances`
and `previews`, and total `wall_time` including admission, snapshot cloning and
validation. Body/contact counts are per advance, not additive. Snapshot cost
depends on historical arena storage, so three live bodies is not a fixed byte
or time guarantee. Global profiling can include attempted previews and advances
even when a transaction is refused. These metrics expose the cost of the
experimental clone-based scheduler; no performance target is established.

## Qualification and remaining work

Native tests compare response batches and material balances against the
independent sequential/simultaneous/supported fixtures across 30/60/120/240 Hz,
common velocities 0/17, one/two/four collider representations and both insertion
orders. They check exact snapshot/output replay, event phases/order, captured
durations and bounds. Rapid-return cases check work and momentum, reflected
source order, and the exact `0.00025`, `5229.0_f32 * 1e-9`, `0.003` regression
plus neighboring splits. Additional coverage includes pending-state restore
and corruption, chatter, pose edits/reversal, frame endpoints, fixed/kinematic
supports, low-speed and rounded-energy refusals, admission violations, and
atomic failure after partial work or simultaneous End overflow.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib native_line_
```

General geometry, rotating bodies, friction, joints, other loading classes,
scenario integration and #328 failure consequences remain outside this path.
Pulse allocation still needs physical calibration. Conservation and replay
tests do not establish that calibration or complete #327.
