# Verified departures and rapid reimpacts (#327)

The [three-body arrival fixture](material-multi-arrival.md) now has an optional
departure policy. It allows a new impact after verified physical separation,
without waiting for the ordinary contact-absence grace timer. This remains a
test-only experiment for three collinear, centered, rotation-locked balls.
Native admission, production snapshot schemas and default episode behavior are
unchanged. Fixture snapshots also preserve pending departure evidence.

## Departure evidence

The scheduler first accepts a material response and a constant-velocity drift
on its staged world. Its existing checks require the actual response to match
the preview, ordinary solving to leave the response velocities unchanged, and
final geometry to remain valid. The departure check then compares each final
position with the f64 prediction `start + velocity * duration`.

For this fixture the per-body position allowance is:

```text
error[i] = 8 * f32::EPSILON * (abs(start[i]) + abs(velocity[i] * duration) + 1)
```

Any actual/predicted discrepancy larger than that allowance refuses the frame.
Nonfinite inputs and nonpositive drift durations cannot supply evidence. This
is a conservative allowance used by the experiment, not a general error bound
for arbitrary Rapier geometry or constraints.

For each ordered pair, compute actual and predicted surface gaps and sum both
bodies' position allowances into `uncertainty`. The exit threshold is twice
capture's touching tolerance: `2e-5` in these fixtures. Retire an existing
episode only when all three conditions hold:

- The accepted drift has strictly separating relative velocity.
- Its starting gap is no greater than `exit + uncertainty`, or it continues
  pending evidence for the same source pair and episode from the exact previous
  accepted endpoints. Starting from an unrelated distant pose does not prove
  how that episode departed.
- `min(actual_end_gap, predicted_end_gap) - uncertainty > exit`.

The geometric margin separates contact entry from confirmed departure. The
number of substeps, elapsed grace time, material response completion and solver
contact disappearance do not themselves authorize early retirement. If evidence
is insufficient, the existing episode and normal grace behavior remain intact.
Large coordinates or common transport can make this conservative check unable
to establish departure; they do not relax its required clearance.

When an eligible separating drift has not yet proved the required clearance,
keep a pending witness containing the episode ID and actual pair endpoints.
The next drift can continue this witness even if rounding has already placed
its starting gap beyond the original eligibility band. Otherwise the actual
endpoint could cross that band before the predicted lower bound clears it,
leaving both steps unable to retire a genuine departure.

Witness continuation requires exact endpoint continuity, unchanged source and
episode identity, and another verified outward drift. A pose discontinuity or
nonseparating motion discards the witness; retirement or episode replacement
also removes stale evidence. Starting again within the contact uncertainty band
can establish fresh evidence. The clearance test itself is unchanged.

## Ownership, outputs and rollback

Successful retirement removes only the current source-pair episode. It does not
change region identity, remaining material capacity or the monotonic episode
counter. A subsequent positive arrival goes through ordinary capture and native
material response, obtaining a fresh episode and a new finite work budget from
the actual incoming motion. A new kick while still touching cannot reuse an old
episode or manufacture an impact root.

The fixture emits an `End` observation at the accepted drift endpoint in its
returned trace. This is separate from the pre-solve contact batches, which keep
their existing observation times. Each trace record includes the ended episode,
canonical source pair, endpoint time and clearance lower bound. Simultaneous
departures sort by source pair. All End records count toward capture's existing
per-frame record limit; the whole group must fit before any episode is removed.

The outer transaction still commits only a completely accepted frame. A late
advance limit, capture overflow or numerical refusal preserves original
episodes, pending witnesses, capacities, poses and previous outputs. The fixture
bundles a `DepartureTracker` with its physics snapshot and stages both together.
There is at most one witness per retained pair (at most three in these fixtures).
Restoring only the physics snapshot would lose eligibility at a rounded boundary;
clone/replay therefore round-trip the complete fixture snapshot. Pause preserves
the tracker. A future native implementation must persist this evidence or supply
an equivalent ownership rule. End observations remain transient and need an
explicit endpoint-event contract in the runtime API.

## Evidence

The previously refused equal-mass case now completes four impacts:

| Arrival time, approximately | Pair | Ownership |
| --- | --- | --- |
| 0.0002500 s | A-B | First impact root |
| 0.0011521 s | B-C | First impact root; A-B departure already recorded |
| 0.0017042 s | A-B | Fresh root after verified departure |
| 0.0018022 s | B-C | Fresh root after verified departure |

All four occur before the ordinary `1/60`-second grace period. Each root has
exactly one End observation. Actual kinetic-energy loss matches the summed
material ledgers, dynamic momentum is conserved within the existing test
tolerance, and capacities agree across collider duplication and insertion order.

Eight tests cover:

- 30/60/120/240/2000/10000 Hz, common velocities 0/17, one/two/four coincident
  colliders and both insertion orders. All variants produce the same four roots
  and patch sequence, with energy/damage checked at existing tolerances.
- One-microsecond subdivisions through departure, plus exact snapshot, trace,
  contact-output and response replay across frame boundaries.
- Repeated sub-tolerance position jitter at a resting interface, insufficient
  separating clearance, unexplained displacement, and subsequent drift from an
  already distant pose: none authorizes early retirement.
- Both subthreshold and larger velocity kicks into a retained touching episode
  refuse without creating a fresh root or changing material state.
- Simultaneous End ordering and atomic overflow when both End notifications
  cannot fit; late failure after earlier episode retirement; pause and replay
  after a successful departure.
- The reported subdivision `0.00025`, `5229.0_f32 * 1e-9`, `0.003` at common
  velocity 17, plus neighboring splits and collider/order variants. The exact
  sequence failed with `Unaccounted` before pending witnesses were added. It now
  completes four accounted impacts with one End per root, including restoration
  and rollback while the first departure is pending.
- Pending evidence is discarded when its endpoint continuity is broken by a
  pose edit or outward motion reverses before clearance has been proved.

The original timer-only refusal remains a regression. The new rule does not
make every rapid contact representable: motion confined to the uncertainty band
still lacks evidence, and any frame exceeding the bounded scheduler work is
still refused.

## Next integration boundary

This establishes evidence for the departure rule in the admitted fixtures.
Promoting it requires complete native scene admission, actual geometry/source
coverage for every departure, bounded scheduling, and a public output contract
for endpoint End observations alongside pre-solve Impact/Support events. The
fixture's preview-clone mechanism is not a promised runtime architecture.
General 2D/rotating/flat geometry, other loading classes, physical pulse
calibration and #328 failure consequences remain outside this experiment.

```sh
RUST_MIN_STACK=16777216 cargo test -p engine-rapier --lib verified_departure
```
