# Bounded escape after an actual-hatch abort

## Frozen question and policy

The [actual-hatch recovery experiment](actual-route-recovery.md) ended the
retained stalled landing, but the mission immediately started an incoming-fire
pursuit and lost its damaged ship. Test whether a bounded physical departure
gains separation while preserving defensive fire and higher-priority safety.

The `actual_capture_escape_v1` profile is opt-in through
`--capture-escape-seats none|0|1|both`, default `none`. It requires ValuePlanner
and actual-route recovery. Arm once when the mission consumes a native
actual-hatch failure on the tick immediately after the neutral abort. The
current actor, landed pose and flag objective must still match. The ship must
be available, aboard, armed and ready, with a visible, unoccluded live opponent
ship inside 350 units. Delayed, stale, moved or unthreatened aborts do not arm it.

Use the existing boundary-aware escape direction predictor and flight routing.
Keep the axis fixed; use radial lift while supported, then ground clearance,
obstacle routing and boundary braking. The original abort tick fixes the
deadline at **720 ticks / 12 seconds**. Hits, solar avoidance, unavailable
controls and target changes cannot restart that clock. Recovery takes priority.
No escape action presses interaction or authorizes hatch exit, jetpack, landing,
claim, boarding or a crossing. No planner budget or publication gate changes.

During escape, take only the existing combat controller's weapon actions;
discard its pursuit flight controls. This retains readiness, visibility,
occlusion, range, aim, ground-clearance and combat-break checks. Weapons remain
subject to those checks; permitting them does not guarantee a shot or hit.

Finish early only after 60 ticks of observed separation: no supported feet,
source-planet radial clearance above 70, boundary stopping clearance above 20,
and the original opponent either terrain-occluded or at least 350 units away
with nonnegative opening speed. Missing or replaced targets do not prove cover.
Early separation permits destination choice; new pursuit remains deferred only
until the original deadline. Otherwise timeout returns to ordinary mission
choice. These thresholds reuse the existing disengagement policy rather than
being fitted to the retained match. No game or bot default changes.

## Comparison plan, fixed before candidate outcomes

Baseline: `target/actual-recovery/v1/summary.json`, SHA-256
`e1810fbe8fbd22d9d488381bee62ed8f5e62ba38dc93988d5fa78065b378e186`.
Keep all **17** existing cases, commands, seeds, opponents, physics and budgets.
First replay all with the new option disabled and require exact report, seven
stream, sensor and allocation parity. Then run all 17 full comparisons: enable
only the evaluated seat in the 15 previously enabled cases; retain the two
cover-off controls. The three health-enabled regressions remain separate from
the eight primary armed matches. Use at most two simulations concurrently.

Freeze implementation, tests and this plan before inspecting candidate results.
Use a hashed copy of the profiled release binary. Do not tune the policy from
these outcomes. `tools/validate-capture-escape.py` retains the earlier native
route, physical transfer, flight, initial-cover, handoff and actual-abort audits.
Every arm, active tick and completion has the exact consumed observation, flight
and weapon actions, source receipt, clock and counters. Audit immutable clocks,
source binding, pursuit deferral, current separation and defensive weapon gates.
The first changed control must follow a recorded arm. Cases without an arm must
remain exact after removing only the new option's telemetry, preserving all
earlier failure/abort evidence.

Measure the actual liftoff, source clearance, opponent range/occlusion, damage,
loss or sustained separation, deadline/end reason, later captures/departures and
final result. Report all eight primary armed matches and the health cases,
including regressions. Suppressed pursuit is not evidence of successful escape.
These correlated development cases do not establish independent playing
strength or Raspberry Pi performance. Work remains local.

## Audit correction

The initial Python audit swapped the interaction and brake byte positions and
rejected a valid braking packet at tick 8040. Native `SurfaceSortieAction`
encodes turn, thrust, interaction, brake and seat in that order. The correction
uses named decoded controls and adds a regression with the actual native packet,
checking that braking passes and interaction fails. The bot implementation and
frozen binary from `24929ed` are unchanged. Re-audit all 34 saved matches, hashing
the gameplay inputs before and after; preserve the failed summary and original
checker alongside the corrected audit rather than rerunning or tuning physics.
