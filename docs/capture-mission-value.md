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

## Results at `b6e74f3`

The [machine-readable results](data/capture-mission-value-v1.json) retain the
plan, commands, binary/report hashes, both players, unknown evidence, outcomes
and timings. All **43 matches finished**, representing 269.99 simulated
minutes: 38 ended by pilot death and five by the match deadline. All physical
audits passed. The largest recorded remote allocation was 126 queries in a
tick, within 384; candidate work continued to receive only the graph budget
left by the existing dispatcher.

| Fresh same-seat comparison against v10 | v12 | v13 |
| --- | ---: | ---: |
| Wins / losses | 8 / 8 | 8 / 8 |
| Accepted destination switches | 0 | 0 |
| Pairs with identical recorded physical outcomes | 16 / 16 | 16 / 16 |
| Completed sorties | 46 | 46 |
| Completed recoveries | 9 | 9 |
| Ships lost / pilot deaths | 13 / 7 | 13 / 7 |

The v10 same-seat controls were also 8–8. These results establish fallback
parity, **not a strength improvement**. The physical fixture separately
demonstrates an accepted v13 switch, earlier first capture, boarding and
departure in each seat; the ownership-value tradeoff is covered by evaluator
tests. A slower enemy capture selected for its value has not yet been observed
in these generated matches.

The known regression is distinct from that fresh sample. V12 still switches
at tick 9527 and loses. V13 makes no switch and exactly reproduces v10's
recorded physical outcome, winning at tick 21005. At source tick 9525 its
current enemy capture is 33.385 seconds / two ownership units. The alternative
is **unknown because a moving-body sweep requires an unmodelled detour**;
there is no complete value recommendation. This is a successful conservative
fallback, not evidence that the new transfer timing or value ranking caused
the win. Both v10 and v12 also reproduced the earlier study's physical
results and byte-identical evaluation logs.

Coverage is the present limit. Across the 16 experimental fresh seats, v13
published 89,708 reports, including 2,563 with multiple numeric destinations
and 2,580 complete value preferences, but **zero alternative value
preferences**. V12 had one alternative time preference and accepted none.
Missing surface evidence dominates; v13 also rejects unsupported static and
moving detours. V13 reports much more frequently because motion revokes the
pinned source. Report counts are not comparable independent opportunities.

On this desktop, mean evaluator construction per observation was 0.173 μs for
v12 matches and 0.229 μs for v13 matches; shared dispatch per tick was 0.047 μs
and 0.133 μs respectively. These are weighted means of instrumented headless
runs, excluding synchronous sensors, rendering and JSON output. They are not
Pi FPS measurements. More frequent evaluation remains bounded, but avoiding
unnecessary refresh while inactive is a possible follow-up.

Validation passed: 154 AI unit tests; the two physical integration tests
(now exercising v12 and v13 in both seats and exact no-result fallback); six
native mission tests with one existing long test ignored; settings persistence;
336 Python tests; workspace compilation for all targets; Rust formatting and
strict AI Clippy. Independent review found and resolved a missing moving-body
sweep check and aggregate model labels that still named v12. Follow-up review
found no outstanding issue in the code, model claims or comparison accounting.

## Next investigation

Keep v13 selectable while retaining v10/v12. Before tuning weights or claiming
strength, improve the evidence that makes alternatives comparable:

1. Use the recorded missing-surface and route reasons to identify where an
   affordable alternative survey could produce a full trip, including enemy
   flag return routes.
2. Measure a small set of actual transfer phases against the staged references.
   Extend support for a specific detour/frame transition only with an explicit
   bounded model and physical replay; unknown geometry should remain unknown.
3. Add a physical case where the slower enemy capture wins on ownership value,
   then predeclare new generated worlds. Retain the present seeds as regressions,
   not a new strength test set.

Raw reports and logs are under `target/capture-mission-value/finished/`.
`regression-verification.json` in its parent directory records the frozen
policy comparison and the source-tick decision window. The committed JSON
includes the critical tick and hashes. Re-run with a fresh output directory;
the runner refuses to overwrite an existing study.

## Device validation

Deployed runtime `a803dbc` to **sw-picade.local** using the app-only updater.
Remote client/CLI hashes matched the built bundle; the kiosk remained active
with zero restarts after installation. Saved P1 `planner-bot` (v10) and P2
`value-bot` (v13), then verified a fresh launcher-idle automatic match used
those policies and the two correct evaluator model IDs. The existing
30-second countdown, 15-minute matches, combat breaks, asteroid setting and
2× raster scale were retained.

The launcher commits scenario choices when **Play World** is used; backing
out of its settings screen alone does not save them. Validation included that
save path and a subsequent automatic launch. Screenshots confirmed the
**value v13** picker and both **Planner bot v10 / Value bot v13** HUD labels.
One live sample showed 34.5 FPS / 60.1 UPS at 1024×768 with 2× raster rendering;
this is a health observation, not a controlled comparison of bot performance.

Device evidence is retained beside the study: `deploy.log`,
`pi-saved-settings.toml`, `pi-autoplay.status`, `pi-health.txt`,
`pi-settings.png` and `pi-match.png`. The final independent data review checked
all raw report hashes, outcomes, regression interpretation, coverage and
weighted timing arithmetic and found no inaccurate claims.

CI's display tests exposed a test-only assumption that every controller could
be reached in three clicks. With five choices, wrapping back to Human can
require four. The test now follows a complete selector cycle and detects
repeated values instead of imposing that old limit. This changes no runtime
code or recorded simulation result.
