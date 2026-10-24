# Oblique three-ball arrivals (#327)

`PhysicsWorld::step_material_three_ball_arrivals(dt)` extends the bounded
[three-ball line scheduler](material-native-line-arrival.md) to translations and
contact normals in two dimensions. A glancing hit can redirect a body into
another object; simultaneous impacts and verified rapid returns use the same
material accounting. This remains opt-in and no scenario enables it.

## Scope and compatibility

The complete scene still contains exactly three centered radius-0.5 balls, with
one to four coincident collider representations per body and one distinct live
material region per body. Dynamic bodies have rotation locked, translations
unlocked and sleeping disabled. Fixed and constant-velocity kinematic supports
are admitted. Gravity, external forces/torques, damping, friction, joints,
CCD/soft CCD, sensors, skin, custom filters and offset centers of mass remain
unsupported. Capture's touching tolerance is still `1e-5`.

The implementation shares its transaction, callback controls, work bounds,
metrics and event stream with the line operation. Public result types live in
`response::ball_arrival`: `BallArrivalStep`, `BallArrivalRejection`,
`BallEpisodeEvent` and `BallEventPhase`. The old `response::line_arrival` names
remain aliases, and `step_material_line_arrivals` retains its horizontal-only
admission, one-dimensional arrival arithmetic and original clearance bound.
The isolated two-ball API is unchanged.

Both three-ball operations use the existing snapshot-v4 witness representation:
episode ID and full x/y endpoints in canonical source order. There is no new
snapshot version or hidden stepping mode. Replay must use the same operation
and durations. Existing v1–v3 loading remains supported. Ordinary positive
steps and binding/retirement changes invalidate pending witnesses as before.

## Arrival calculation

For a pair, let `d = center_j - center_i`, `v = velocity_i - velocity_j`,
`a = dot(v, v)`, `b = dot(d, v)` and `cross = d.x*v.y - d.y*v.x`, all computed
in f64 from the actual f32 positions and rounded post-response velocities.
The summed radius is one. An approaching separated pair's first arrival is:

```text
gap = length(d) - 1
closest_distance = abs(cross) / sqrt(a)
time = gap * (length(d) + 1) / (b + sqrt(a - cross*cross))
```

The cross product avoids subtracting large squared distances to obtain the
closest approach. A closest distance within `1e-5` of tangency refuses with
`Numerical`; the cutoff is conservative, including a future near-grazing path
beyond the current frame. Clear misses and nonapproaching pairs do not schedule
an arrival. An approaching pair already within `1e-6` after preview refuses
with `Unaccounted`. Existing penetration and rounded-energy checks still apply.

All three trajectories are recomputed after every material response. Actual
response outputs must match preview, and ordinary solving must leave the
rounded response velocities unchanged. The preview's integrated positions are
discarded. The call allows at most eight previews and eight accepted advances;
every refusal rolls back the whole call, including previous outputs and pending
evidence. An exact frame-end arrival responds on the next positive call.

## Departure proof in two dimensions

For each coordinate, use the existing displacement allowance:

```text
error[i][axis] = 8 * f32::EPSILON *
    (abs(start[i][axis]) + abs(velocity[i][axis] * duration) + 1)
```

The actual endpoint must lie within that coordinate allowance of the f64
constant-velocity prediction. By the reverse triangle inequality, pair-distance
error is bounded by `length(error[i]) + length(error[j])`. Subtract that bound
from the smaller of the actual and predicted final surface gaps. Retirement
requires the resulting clearance to exceed `2e-5`.

Let `r` be the initial center separation and `w` the relative outgoing velocity.
Require `dot(r, w) > 0`. Since `dot(r + w*t, w)` equals
`dot(r, w) + length(w)^2*t`, the distance strictly increases throughout this
drift even while its direction changes. Tangential or inward motion does not
establish this proof. The starting gap must lie within the exit uncertainty
band, or continue an existing witness for the same episode from the exact
previous endpoints. Witness continuity checks both coordinates. This is a
conservative rule for the admitted constant-velocity geometry, not a general
Rapier error bound.

The rotated, translating rapid fixture exposes another rounded crossing:
after the first arrival, a `5562.0_f32 * 1e-9` step leaves departure pending.
Discarding the witness then stepping `0.003` seconds returns `Unaccounted`;
retaining it completes the other three impacts. This is covered in both the
independent fixture and the native API, including snapshot restore and rollback.

## Evidence and retained refusal

The independent fixture uses its own scheduler, two-dimensional motion and
departure calculation. Native qualification compares durations, episode events,
capture/response batches, body motion and material balances against it, as well
as exact same-build snapshot/output replay. Three layouts sweep 30/60/120/240 Hz,
five rotations, zero or two-axis common velocity, one/two/four colliders and both
insertion orders: 720 combinations, each continued for eight frames.

| Layout | Observed sequence in the unrotated, unboosted reference |
| --- | --- |
| Light projectile | A-B at about 0.7551 ms redirects B into C at 11.9433 ms |
| Simultaneous oblique impacts | A-B and B-C at 0.2500 ms; the outer balls subsequently meet at 24.6006 ms |
| Equal-mass rapid return | A-B, B-C, A-B, B-C at about 0.7551, 1.9067, 2.6485 and 2.7861 ms |

Actual kinetic loss closes against the summed ledgers at the existing `1e-5`
relative tolerance; dynamic momentum is checked in both axes. Additional tests
compare arrival times with independent distance bisection, exercise pending
endpoint edits and reversed motion, verify signed prescribed work, and test
grazing, numerical-energy, admission, capture-limit and late-advance refusals.

One equal-mass case with fixed C remains an explicit refusal. At rotation 0.4,
successive returns accelerate; after five impacts B approaches C again around
2.214 ms before C's retained pair episode has proved departure. The final gap
is about `7.2e-8`. Both fixture and native operation return `Unaccounted` and
preserve the original frame. A lighter projectile completes the fixed and
moving-support cases with accounted work. An admitted body type does not imply
that every rapid return can establish a new episode.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib multi_arrival::oblique
```

General shapes/radii, rotational freedom, friction/joints, sustained or other
externally driven work, soft queries, physical pulse calibration and #328 failure
consequences remain unfinished. This does not complete #327 or provide a general
production collision scheduler.
