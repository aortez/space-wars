# Experimental capture destination policy v12

`material_mission_v12` adds the first behavior driven by the
[capture evaluator](capture-mission-evaluation.md): replacing the current
destination with a sufficiently shorter supported capture trip. It reuses
v10's local flight, landing, joint walking/return, combat and recovery tasks.
It does not include the v11 jetpack-planning experiment or promote a new default.

Both player selectors offer **destination bot v12**, saved as `destination-bot`.
Automatic matches retain it, and the HUD names each seat's actual policy.
Legacy v9 and Planner v10 remain selectable and do not consume proposals.

## Decision boundary

The existing host still emits controls, attaches remote observations, then
spends the shared planning allowance. On a later control tick v12 may consume
a completed comparison, after checking its original dependencies against the
new observation. It cannot construct a proposal from a partial search.

A switch requires all of the following:

- The entire shortlist has supported costs and is not truncated. Unknown,
  expired, changed or unsupported options retain the v10 destination.
- The suggested destination beats the current remaining reference by at least
  **five seconds and twenty percent**, and fits the current match clock.
- The report matches the pilot, policy, selected visit/site, ship form/location,
  material revisions, ownership, flag position and claim duration. Report source
  age is at most two seconds; stored samples keep their original age.
- Recovery, solar avoidance and opportunistic combat keep their existing
  priority. Destruction, a dead pilot or a finished match invalidates a proposal.
- The ship has no supported feet and is still flying. Survey, circling and a
  high approach may switch; Surface, departure and return commitments cannot.
  Radial clearance must exceed `35 + falling_speed² / 50` world units.
- There has been no earlier destination switch in this trip, and the proposed
  planet is not deferred after a failed attempt. A natural new selection starts
  a new trip; reset clears the experiment's state.

The switch records the abandoned attempt and a new selection. The ordinary
transfer controller must fly there and obtain current landing/exit/boarding
evidence. Remote climb samples and predicted durations never authorize a
physical action. `destination_planning` telemetry records the switch count,
last source/decision tick, destination pair and both estimates.

These are conditional time estimates, not utility or survival estimates.
Opposing fire, detours, likely stalls, the strategic value of removing an enemy
flag, and the cost of the following mission are not scored. Consequently an
earlier foothold can be a worse longer-term itinerary.

## Making native evidence usable

Two issues appeared when a physical flagged trip finally offered an attractive
alternative:

1. Native objective routes arrive every thirty ticks. Between those surveys,
   absence was overwriting numeric costs with “unmeasured” before the bounded
   evaluation could be used. A valid sample now survives an ordinary cadence
   gap for less than thirty ticks, without renewing its source time. Explicit
   stale live work, a new negative route, edits or the next missing survey revoke
   it. Current landing/hatch/cover observations remain consumption gates.
2. Ground-cost validity used acceleration at the airborne ship, so descending
   invalidated an unchanged ground route. Tactical observations now publish the
   same acceleration at the flag that the existing ground-route planner uses.
   Dependency checks use that value with the existing 0.01 tolerance. Ship
   altitude cannot renew/invalidate a flag's route; a changed ground field can.

Published reports retain their own evidence and dependencies, separate from a
pending refresh. A newer request cannot refresh an older result's lifetime.
Timing medians and their supported walking domain are unchanged.

## Comparison and reproduction

The native client retains synchronous local sensors and the shared allowance of
**four evaluator units and 384 remote physical queries per tick** for both seats.
Each evaluator actor remains capped at two units. This is not a budget for all
bot sensors or wall time. Proposal validation is a bounded read of at most eight
planet/evidence records and performs no physics queries.

The runner can now use `--live-objective-seats none` to match native local sensing
while retaining the remote dispatcher. The new `--world destination` fixture
uses two rounded physical planets and an initial opposing flag on measured
retained ground. Only initialization installs that flag; the subsequent flights,
claims, losses and transfers all use ordinary actions and one shared step.

```sh
cargo build --release -p spacewars-ai --example surface_mission_soak
python3 tools/compare-capture-destinations.py
cargo test -p spacewars-ai --test capture_destination
```

The comparison includes four controlled v10/v12 pairs, both seats and mirrored
travel, plus eight generated-match pairs with fresh SHA-256-derived seeds,
v12 in each seat, and asteroids off/every three seconds. Each run lasts three
simulated minutes; none is discarded for a stall or lack of switching. These
are short physical comparisons, not finished-match win-rate evidence.

The controlled cases exercise both a supported change of destination and
unsupported mirrored approaches that retain v10. CI also checks exact control
fallback with zero evaluator fuel through a full flagged capture/return, unsafe
descent and touchdown gates, expiry and dependency changes, reset/replay,
controller persistence and the native host's unchanged v9/v10 round.

## Follow-up

Keep the experiment selectable until broader evidence warrants promotion.
The next useful extension is targeted coverage of missing destination costs,
followed by explicit comparison of first foothold, enemy-flag removal and the
remaining itinerary. Preserve the unsupported mirrored cases as regressions;
do not assign invented costs just to produce a choice. The frozen v10 baseline,
query caps and retained failed attempts make that comparison repeatable.

Independent subagent review was attempted twice but failed with an API
authentication error before reviewing files. Local review and tests do not
replace that outstanding independent review.
