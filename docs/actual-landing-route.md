# Actual landing route investigation

The [covered-request experiment](covered-request-handoff.md) delivers a positive
early route in World 1 P1 powered, but its planet-0 visit lands at 7635, never
exits, and loses the ship at 8117. This investigation separates physical hatch
stability, actual-route planning, publication and the controller's exit guard.
No controller, physics, planner budget, validity tolerance or default changes
are part of this diagnostic.

## Existing evidence

The retained dense stream reports one `hatch_moved` invalidation at 7683. The
three following requests, sourced at 7683, 7804 and 7925, each reach their full
120-tick lifetime without publication. A fourth pending request ends when the
ship is lost. The ship remains physically landed and its transfer reports ready.
Thus repeated hatch motion alone does not explain the continued wait.

Native code requires a current positive actual-hatch round trip before exit.
`touchdown_changed` starts new work at landing; `hatch_moved` is stale evidence
and resets capture's selected site. Expiry restarts planning without supplying
a route. None of those events grants permission to use the earlier hypothetical
site's route from the actual exit.

## Frozen diagnostic plan

Freeze the probe, its tests, runner and this plan before inspecting detached
job outcomes. Use two complete retained matches from
`target/covered-handoff/v1/summary.json`, with their exact source arguments:

- World 1 P1 powered: inspect 7635, 7682, 7683, 7803, 7804, 7924, 7925,
  8045, 8046 and 8116. These bracket actual request creation, invalidation and
  expiry through the failed visit.
- World 0 P2 powered: inspect 18199, 18200, 18240, 18288 and 18289. This
  comparison physically lands and later exits, although its eventual return
  and departure still fail. It is not a successful complete-mission control.

Run each match once with diagnostics disabled and once with only the probe
enabled, at most two concurrently. Require exact physical/control/evaluator
streams, sensor work, non-timing live telemetry, allocation ledgers, initial
cover witnesses and handoff receipts against the retained match. Run existing
physical, route and flight auditors. Preserve full match horizons and outcomes.

`--probe-actual-landing-ticks` and `--probe-actual-landing-seat` inspect the live
request after controls consume their normal observation and before that tick's
dispatch. The probe copies the existing actual-touchdown job, including its
retained immutable physics snapshot, original clock and completed partial work.
It performs at most one million extra native steps, stopping at the first
positive actual route, complete failure or the explicit work limit. Record
pipeline transitions, graph/query counts, native rejection counters and any
route. No copied result enters the live observation or queue.

The detached world and its measurement clock stay fixed. Its result establishes
only a native answer against that old snapshot; it skips live publication
validation and does not prove an executable exit in the evolving match. Extra
diagnostic work is separate from ordinary sensor profiling and live quotas.
For cost comparisons, `max(ceil(graph/4), ceil(queries/384))` is only a lower
bound on additional dispatch ticks if the job had the whole shared allowance.
It excludes contention, invalidation, expiry and physical movement.
Because observation precedes dispatch, work performed on the last valid tick
cannot be published until the next, expired tick. Count remaining useful
dispatches through the tick before the publication deadline.

Independently bind probe observations to dense records, source poses to request
ticks, live work to the allocation ledger before each probe, and detached totals
to phase transitions. Reconstruct actual hatch/vehicle motion and the native
angular threshold from the retained pilot rows without relaxing validity.

```sh
python3 tools/probe-actual-landing.py \
  --prior target/covered-handoff/v1/summary.json \
  --binary target/actual-landing/surface_mission_soak-FROZEN_COMMIT \
  --out target/actual-landing/v1
```
