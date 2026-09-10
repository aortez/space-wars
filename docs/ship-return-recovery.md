# Returning to displaced or inaccessible ships

This follows [claim footing recovery](claim-footing-recovery.md). The shared
ground and recovery tasks are now `ground_navigation_v8` and `recover_ship_v7`.

## Recorded failures and behavior

Two exact desktop replays from `92c72fa` establish the failures. Seed 7/P1,
mirrored duel with 3-second Mixed asteroid arrivals, ends beside a nearly
motionless full ship whose hull rests on a terrain step. Its feet do not qualify
for landing, so there is no grounded hatch. Seed 42/P1, mirrored intercept with
no environmental arrivals, leaves the spaceling on planet 1 while the opponent
displaces its empty ship to planet 0. The old ground task attempts to reach that
foreign hatch as if it were on the spaceling's planet.

Ground navigation now rejects a hatch on a different retained planet once the
ship has landed on both feet and the spaceling has actual support. An active
jetpack crossing finishes first. A missing hatch gets fifteen seconds to settle,
measured from its disappearance rather than from the beginning of prior walking.
The existing ninety-second ground-task deadline remains in force.

These two failures have explicit telemetry types. They allow the containing
recovery task to consider replacing the assigned ship; malformed observations
and unrelated route failures still produce their existing bounded failures.
Replacement requires a balanced, supported spaceling moving at no more than one
unit per second relative to its retained planet. For a nearby missing hatch,
the full ship must also be within 24 units, in the same planet frame, moving at
less than one unit per second radially and tangentially, and spinning at less
than 0.2 radians per second relative to that planet. A landed ship on another
planet remains independently identifiable through its two-foot support.

The bot holds the existing human scuttle chord (primary, interact and brake).
The shared world consumes the inputs and performs the actual loss after three
seconds. Neutral input rearms the surviving spaceling, which then uses the
ordinary claim, eight-second rebuild, hatch access and boarding rules. Fresh
hatch access cancels the hold. Dirty queries, support loss or unstable motion
release it; normal jump/get-up inputs can restore balance. The attempt has a
fifteen-second deadline within the original recovery budget, with at most one
completed replacement per task. Both host policies preserve that task across
the deliberate loss instead of restarting its clock.

There are no new human recovery rules, physics steps, gravity solves or world
writes from the policy. A ship still flying far away, a distant same-planet
hatch without a measured route, and general cave navigation remain outside this
slice. Replacement can require recapturing hostile ground, so starting recovery
near the three-minute cutoff does not imply a completed return by that cutoff.

## Physical return trials

The new `surface_return_soak` isolates the return leg from the preceding flight.
Its fixture places a spaceling and a reachable, tipped or foreign-planet ship
only at construction. Ordinary physics must establish support and raise the
initial flag. The task then emits normal inputs through boarding, and the host
applies normal thrust to verify departure. No ownership, landing or recovery
state is forced during the run.

```sh
cargo build --locked --release -p spacewars-ai --example surface_return_soak
target/release/examples/surface_return_soak \
  --seed 42 --seat 0 --mirror true --bearing 0 --case other-planet \
  --seconds 180 --out /tmp/surface-return
```

The matrix covers three conditions, both seats, both mirrored layouts and two
surface orientations: 24 three-minute runs per platform. Every run must board
and depart, conserve retained plus removed material, retain finite physics and
stay below the existing speed bound. Reachable controls must never require a
replacement. Reports preserve claim, scuttle, boarding and departure ticks,
task events, per-second physical audits and renderer frames.

Decision tests cover support loss, cancellation, moving-ship exclusion, dirty
queries, hatch settling time, replay, the replacement limit and host deadline
preservation. A physical integration test exercises all three return conditions.
The final validation also retains the 52 mission and 36 impact trials per
platform, the release workspace suite and the frozen ordinary-game navigation
and strategy baselines. Physical audit success and mission completion are
reported separately.

Artifacts and reproduction scripts:

```text
/home/oldman/.codex/visualizations/2026/09/06/01a078c0-7d43-7490-9599-f9ce4705c9b8/ship-return-recovery-20260909/
```

Final validation and installed-image identity will be recorded after the source
checkpoint is built, tested and deployed.
