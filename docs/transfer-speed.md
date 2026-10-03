# Relative closing-speed guidance during committed transfer

## Frozen question and policy

The [clear-entry experiment](transfer-approach.md) still enters a neighboring
planet's local frame too quickly: at the transition, 104.393 units of clearance
is below the unchanged climb rule's 111.288-unit requirement for the observed
45.436 inward speed. Test a bounded velocity constraint before that transition.
Preserve the native climb, frame selection, arrival and physical capture gates.

`relative_transfer_speed_v1` is opt-in through
`--transfer-speed-seats none|0|1|both`, default `none`, and requires clear-entry
approach/travel configuration. It runs only when the existing approach decision
is fresh and actually used for **transfer** during the committed post-escape
trip. Launch/climb, solar avoidance, recovery, unavailable controls/queries,
disabled flight, supported feet, changed vehicle/selection and capture handoff
retain their existing priority. The original 60-second transfer deadline and
20-second progress limit remain unchanged.

For each non-destination planet, use the current ship/body motion to compute
outward radial normal, surface clearance and inward relative speed. Derive a
maximum desired closing speed from the existing 70-unit climb floor, time to
turn through half a revolution at the observed turn-speed limit plus 0.6 seconds,
and a braking estimate. As with the existing boundary guidance, reserve half the
nominal brake acceleration and inward gravity; clamp this estimate to 5–25.

```text
t = pi / max(turn_speed, 0.1) + 0.6
a = clamp(brake_acceleration / 2 - max(inward_gravity, 0), 5, 25)
room = max(clearance - 70, 0)
maximum_closing_speed = sqrt((a*t)^2 + 2*a*room) - a*t
```

These are guidance estimates, not certificates of future physical clearance.
Braking operates in the current local frame; later body motion, gravity,
contacts and the flight motor remain authoritative. The destination is excluded
so its ordinary arrival controller can take over. Existing obstacle routing and
solar checks remain active.

Each planet defines a half-plane of admissible world velocities. Select the
closest velocity to the ordinary waypoint request satisfying all constraints,
with a correction magnitude no larger than the existing 55-unit transfer speed.
In two dimensions, examine the original request, each single-line projection,
and each nonparallel pair intersection: at most **1 + N + N(N−1)/2** candidates
for N other planets. Use feasibility tolerance 0.001 and skip pair determinants
of magnitude at most 0.0001. There are no added planner graph operations,
physics queries, simulation forecasts or clock renewals.

If no admissible bounded correction exists, retain the ordinary desired velocity
and request braking. Also request braking when actual inward speed exceeds any
current cap by more than 0.001. Pass this request through the existing flight
motor, including its native braking compensation, thrust, turn and wing controls.
Keep defensive weapon eligibility unchanged. This does not grant interaction,
landing or capture permission, or extend the earlier escape boundary controller.

## Comparison plan fixed before candidate outcomes

Baseline: `target/transfer-approach/v1/summary.json`, SHA-256
`074b66abbaa7e280cdcd9e7438cd5a5106d565f3ea6bf0bba762a0f4a114d9b3`.
Retain all **17 cases**, seeds, full match lengths, opponents, budgets, physics
and prior options. Replay all 17 with this option disabled and require exact
prior reports, seven streams, sensors and allocation ledgers. Run 17 comparisons
with only the evaluated seat enabled in the existing 15 enabled cases; retain
the two disabled controls. Keep the three health regressions separate from the
eight primary armed cases. Use at most two concurrent simulations. Freeze code,
tests and this plan before outcomes, then copy/hash the profiled release binary.

`tools/validate-transfer-speed.py` retains all earlier native route, physical
crossing, continuation, cover, receipt, abort, escape, travel and entry audits.
Save the exact consumed observation, waypoint avoidance, desired velocities,
constraint inputs/outputs and actual actions for each eligible speed decision.
Independently reconstruct the ordinary waypoint velocity, body-relative bounds,
bounded joint projection, braking requirement and counters. Verify the actual
brake and wing bytes, immutable travel identity/deadline and native capture
handoff. A first changed control must follow a recorded speed/brake intervention.
Cases without an intervention must remain exact after removing only the new
telemetry, including every earlier experiment's evidence.

Measure approach-frame transitions and speed-dependent climb margins, native
arrival, physical capture/landing/exit/claim/boarding/departure, damage, vehicle
and pilot loss, and complete match outcomes. Preserve the earlier no-escape
reference alongside the immediate baseline. Report all primary and health
cases, including failures. Do not retune the frozen policy from these outcomes.
The correlated development suite does not establish independent playing
strength or Raspberry Pi performance. Defaults and remote deployment stay
unchanged; work and commits remain local.
