# Flat-face departure and reimpact qualification (#327)

The optional tracker in `response/tests/flat_arrival/departure.rs` qualifies
early departure for the [parallel-plate fixture](material-flat-arrival.md).
It lets genuinely separated faces start a fresh impact episode on return,
including returns within capture's ordinary separation grace. This tracker is
test-only; the [native flat operation](material-native-flat-arrival.md) now has a
separate implementation with validated snapshot persistence. They are compared
at every frame of the positive sweeps without sharing proof arithmetic.

## Evidence after a completed drift

The fixture retains its existing geometry, force, response and drift checks.
For an ordered pair of plates with common unit normal `n`, departure uses the
face-plane gap, not center distance:

```text
gap(p) = dot(p_j - p_i, n) - 1
predicted_k = start_k + velocity_k * dt
error_k[axis] = 8*EPS*(abs(start_k[axis]) + abs(velocity_k[axis]*dt) + 1)
uncertainty = sum_axis(abs(n[axis]) * (error_i[axis] + error_j[axis]))
lower = min(gap(actual_end), gap(predicted_end)) - uncertainty
exit = 2 * capture.touch_tolerance
```

Arithmetic uses f64 values promoted from the actual f32 positions and rounded
post-response velocities. The coordinate error boxes are the existing drift
allowance, projected onto the face normal. Both actual and predicted endpoints
must clear the exit boundary after that allowance. The fixture uses capture's
`1e-5` touching tolerance, so `exit` is `2e-5`.

The pair must have an existing episode, strictly outward normal relative
velocity, and a starting gap no greater than `exit + uncertainty`, unless it
continues a pending witness described below. Constant relative drift makes the
normal gap monotone throughout that actual advance. A successful proof removes
the episode and emits its `End` at the completed advance's timestamp. It does
not predict an `End` before observing the actual motion. Resting or closing
pairs cannot retire through this proof.

Simultaneous departures are sorted by source pair. Their notifications count
against capture's record limit. The tracker and world are staged together:
any later response, drift, capture or advance-limit refusal restores the entire
call, including prior outputs and pending evidence. Pause clears transient
outputs without advancing the world or tracker.

## Rounding and continuity

One advance can leave the actual endpoint outside `exit + uncertainty` while
the predicted endpoint still prevents certification. Rejecting the next start
solely because it lies outside that band would strand the original episode.
An eligible outward advance therefore retains pending evidence when it cannot
yet prove departure.

A pending witness contains the source pair, episode ID, both complete world
endpoints, and the common face normal. Continuation requires the same episode,
exact endpoint equality with the next start, and exact normal equality. It must
still pass all drift and outward-motion checks. Stale or discontinuous evidence
is discarded; it cannot manufacture a departure after an external pose edit.

The normal is necessary even with rotation locked during each advance. A host
can rotate every plate between calls without moving its center. In the retained
partial-overlap regression, changing the angle from `0.37` to `0.3701` moves the
face planes beyond the eligibility band. An endpoint-only witness would
incorrectly continue the old proof. The tracker refuses that continuation.

The rounded-crossing regression starts three plates at local normal positions
`-0.01`, `1`, `2.02`, with tangent offset `0.5`, angle `0.37`, common velocity
`(17, -11)`, and plate A approaching at relative speed 40. After the computed
first-arrival duration, it advances `2835.0_f32 * 1e-9`, then `0.006` seconds.
Keeping the pending witness accounts for all four impacts in both fixture and
native stepping. Dropping it causes the later return to fail `Unaccounted`;
restoring only the fixture's physics into the native path likewise cannot
recover the missing evidence. Nearby durations and sliced/duplicate
representations are also tested.

## Qualification and limits

Six regression tests establish:

- **640 dynamic-chain cases:** five slice layouts, one/two copies, both
  insertion orders, angles 0/0.37, common velocities `(0, 0)`/`(17, -11)`,
  tangent offsets 0/0.5, and 30/60/120/240 Hz. Every case runs to the same
  `1/30`-second horizon and accounts for four alternating A-B/B-C impacts.
- **96 fixed-support cases:** stationary horizontal support, two layouts,
  both copy counts/orders/offsets, and 30/60/120/240/2000/10000 Hz. Every case
  runs to 0.02 seconds and accounts for seven impacts. The seventh full-overlap
  impact occurs around 9 ms; comparing only one frame at different rates would
  omit it from some runs. Sixteen additional dynamic cases test 2000/10000 Hz.
- **40 rounded-crossing cases:** two layouts, both copy counts/orders and five
  durations around the threshold, including the witness-dependent sequence.
- Endpoint/orientation edits, reversal, invalid drift, resting-contact jitter,
  pause, invalid time, advance limits and simultaneous notification overflow.

Positive sweeps replay the fixture's combined world/tracker snapshot exactly,
including capture and response batches. Each impact has a distinct root and
each episode ends once, after its start and before the same pair's next impact.
Actual kinetic loss plus signed prescribed work closes against the response
ledger within the existing `1e-5` initial-energy tolerance. Fully dynamic cases
also preserve momentum in both axes. Matching cases compare material balances
and dissipated energy across subdivisions and collider representations.

Departure proof does not make every response numerically representable. A
horizontal three-plate scene with a translating prescribed support at common
velocity `(17, -11)` completes two impacts in 0.0015 seconds, then refuses a
0.03-second continuation with `Response(Backend)` at a smaller later impact.
The regression preserves the rounded-energy check and verifies full rollback,
including the pending tracker and prior successful outputs.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib flat_departure_
```

Native promotion now stores orientation-aware flat witnesses separately from
the ball tracker in snapshot v5, with structural restore validation, complete
scene admission and transactional invalidation across stepping modes. The
fixture still serializes its own trusted wrapper and implements its own proof;
the native tests compare witness bytes separately from the complete physical
and capture snapshot. Its endpoint-only ball witnesses are not reused for
flat geometry. General shapes, free rotation, tangential contact entry, other
loading classes, pulse calibration and #328 failure consequences remain outside
this qualification.
