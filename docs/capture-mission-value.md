# Capture mission value, v13

V13 (`material_mission_v13`, **Value bot v13**) builds on the v12 destination
selector. Both remain available; v9/v10 defaults and v11's separate jetpack
experiment are unchanged. Both launcher seats offer **value v13**, persisted as
`value-bot`; automatic matches retain the selection.

The [finished v12 trials](capture-destination-finished-matches.md) exposed two
gaps. Its only accepted switch abandoned an enemy flag for neutral ground,
using a 0.46-second transfer reference for a trip that actually took 5.37
seconds. The alternative also left the opponent's foothold intact. These are
measured immediate differences, not proof that either alone caused the later
defeat.

## Decision and limits

V13 uses the existing empirically measured local landing/claim/return phases.
It replaces nominal distance/speed travel with a bounded direct-flight
reference: settle current relative velocity, turn, climb to the transfer
controller's 70-unit altitude (including its inward-speed allowance), turn
toward the entry ring, accelerate/cruise/brake. Acceleration reserves the
observed gravity magnitude; turn, thrust and brake limits come from the ship.
The same 38-unit nominal cruise reference is retained. Velocity differences
between the launch and destination frames add a settling cost.

This staged reference is **not a flight simulation or calibrated prediction**.
It assumes a fixed gravity magnitude and staged control, ignores drift during
settling/turning, and does not reproduce the controller's concurrent actions,
orbital acceleration or changes of gravity frame. Planet/sun/boundary detours,
insufficient control authority and references longer than 30 seconds are
unknown. Linear moving-body sweeps through the direct corridor also make the
reference unknown. An already selected local approach retains its remaining
local timing reference; it is not charged for launching again. Ordinary live
flight, contact and landing checks still authorize every physical action.

With an owned planet, ranking uses completion seconds per ownership swing:
neutral capture adds one; enemy capture adds one and removes one from the
opponent. With no owned planet it ranks time to the first rebuild foothold.
This is a one-capture objective, not a search of future captures or a model of
survival, combat or rebuild denial. Enemy capture can justify a slower trip,
but must fit the remaining match clock. All shortlisted options still require
supported costs. At most one switch is accepted per natural trip, outside
committed descent/landing/return/recovery. Hysteresis remains at least five
equivalent seconds or 20% of the current cost, whichever is greater.

The report exposes transfer phases, ownership swing, priority units, seconds
per unit and the selected value objective. It retains time-only ranking for
diagnostic comparison. Switch telemetry records both actual time references
and equivalent time saved at the current ownership value. Per-seat model IDs
distinguish `capture_mission_value_v1` from v12's unchanged timing model.

Motion-dependent results compare against their original source: two position
units, two velocity units/second, 0.1 radians heading, 0.2 radians/second spin
and 0.5 gravity units. Frame, flight limits, bodies, boundary and sun are also
rechecked. Drift revokes a result and requests a refresh; it cannot renew the
old source. Existing terrain/ownership/route/cover/claim freshness gates remain.

One charged graph step handles one analytic candidate, bounded by eight body
checks per leg. There are at most three candidates plus one comparison step.
No world queries or motor rollout are added. The two seats share the existing
4 graph / 384 query allowance. Snapshot construction and synchronous local
sensors remain outside that allowance; this is not a total bot CPU budget.

## Validation plan

Tests exercise the ownership tradeoff, first-foothold priority, clock/unknown
fallback, motion invalidation, partial budgets, short/long transfer stages,
obstacles and moving-body crossings. Physical fixtures run both v12 and v13
through claim/board/depart in both seats and assert exact v10 controls without
a completed comparison.

`tools/compare-capture-value.py --out target/capture-mission-value/finished`
records the complete plan before starting. It runs the known regression with
v10/v12/v13, then 40 fresh finished matches: four SHA-256-derived seeds from
`native-capture-value-v1:{0..3}`, quiet/three-second asteroid pressure, and
v10/v10 controls plus v12 and v13 in each seat against v10. All use native
cadenced local sensors, the shared 4/384 allowance, and a ten-minute match
deadline. The four world seeds, reused controls and mirrored policy seats
are not independent samples. No timing constants or utility weights are fit
to these outcomes. Failures and unchanged matches are retained.

Results, device validation and final review are pending.
