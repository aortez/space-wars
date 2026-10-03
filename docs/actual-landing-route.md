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
  --out target/actual-landing/v2
```

The first disabled pass in `v1` verified exact replay parity, then stopped on the
runner's comparison of integer-keyed native seat counts with string-keyed saved
JSON counts. The serialized receipts and initial-cover records are identical.
Before any detached outcomes were inspected, the runner was corrected to compare
canonical JSON receipts, with a test that still rejects changed generations.
The failed summary/logs remain retained; `v2` repeats both disabled replays before
running either instrumented comparison. Probe code and frozen binary are unchanged.

## Findings

The stalled visit has no positive native actual-hatch round trip in any of the
five sampled request snapshots, even when the detached job completes. Its short
walking pass fails; the powered crossing fails `arrival_window` in the first
two source epochs and `fuel_reserve` in the remaining three. Completing the
full fallback still reports `disconnected` for the actual outbound route, with
no return route. The closest reachable point remains about 39.85 units from the
objective despite valid start footing. This is a result of the existing native
model and retained geometry, not proof that every physically possible route is
impossible.

The live controller correctly withholds exit without a positive actual return.
The missing behavior is a bounded response to an unsuccessful local attempt:
the planner falls back to work that cannot finish before expiry, and capture
continues waiting across replacement requests. Relaxing hatch validity or
letting an offline result enter controls would not supply a valid exit here.

## Exact replay and scope

Probe code and the original plan were frozen in `a7cca6f`; the JSON seat-key
comparison was corrected in `55ae233` before detached outcomes. The same frozen
profiled binary was used throughout, with SHA-256
`001ba08c415119c2613e50af440ee3f68a3f47ab31ba61fde320956d9d194af3`.

Both disabled replays and both instrumented complete matches match their sources
exactly: seven control/evaluator streams, physical outcomes, ordinary sensor work,
non-timing planner telemetry, allocation ledgers, initial-cover records and
handoff receipts. All fifteen requested snapshots were reached. Fourteen contain
actual jobs: ten completed negative answers from five failed source epochs and
four positive answers in the comparison. The final comparison snapshot is already
on foot, with no actual request. No detached continuation hits its work limit.
Repeated probes of one source are correlated observations, not independent trials.

World 1 P1 powered still loses at **20910** with two completed departures.
World 0 P2 powered still loses at **30536** with three departures. The latter's
claim without eventual boarding/departure remains unresolved; its exit alone is
the comparison used here. No gameplay, scheduling, quota or default was changed.

## Actual hatch stability

The failed visit has only one `hatch_moved` event, at **7683**. Reconstructing the
native f32 angular calculation gives a half-angle sine of **0.000102520**, just
above the unchanged **0.0001** threshold. Planet-local vehicle translation is
about **0.00168**, and exit translation about **0.000133**, both below the
0.002-unit translation threshold. Translation reconstruction uses double
precision, so values near a native boundary remain descriptive rather than a
replacement for the recorded validator decision.

| Generation | Snapshot/request tick | End of observed request | Native reason | Positive publications |
| ---: | ---: | ---: | --- | ---: |
| 29 | 7635 | 7683 | `hatch_moved` | 0 |
| 30 | 7683 | 7804 | `expired` | 0 |
| 31 | 7804 | 7925 | `expired` | 0 |
| 32 | 7925 | 8046 | `expired` | 0 |
| 33 | 8046 | 8116 | Ship is lost on the following tick. | 0 |

The ship stays landed with transfer ready throughout the inspected interval.
At 7635 capture reports `actual_route_unavailable`. The first stale invalidation
resets it to survey at 7683; from 7684 it again reports `actual_route_unavailable`.
Its recorded controls remain neutral while waiting. The three full-lifetime
expirations establish that hatch movement is not the sole cause.

## The failed local pass finishes early; the fallback cannot finish in time

The detached native work before entering the full ground survey is **60 graph
operations** and roughly **3,100–3,200 queries**. At the late probes, the live
job's existing measurement counters already record that powered rejection and
show it in the full-ground phase. These are copied facts from work the live
queue performed, not conclusions injected by the diagnostic.

[The stage-timing reconstruction](data/actual-landing-stage-timing-v2.json)
joins the deterministic native step sequence with the per-generation allocation
ledger. It identifies the first dispatch reaching that phase boundary and retains
the following real observation:

| Generation | Source tick | Dispatch finishing the local attempt | Next observation | Local rejection |
| ---: | ---: | ---: | ---: | --- |
| 29 | 7635 | 7655 | 7656 | `arrival_window` |
| 30 | 7683 | 7703 | 7704 | `arrival_window` |
| 31 | 7804 | 7825 | 7826 | `fuel_reserve` |
| 32 | 7925 | 7946 | 7947 | `fuel_reserve` |
| 33 | 8046 | 8067 | 8068 | `fuel_reserve` |

These are reconstructed boundaries, not newly logged native events. Every
following observation remains landed and transfer-ready, with a valid current
request but no publication, and capture still reports `actual_route_unavailable`.

From a fresh request, finishing even the full ground-survey stage requires
**1,137 total graph operations** including the local prefix: at least 285
dispatches with the entire four-operation allowance, before considering the
opponent. The usable lifetime supplies at most 120 dispatches before a following
observation would expire the result. The complete detached jobs cost:

| Source tick | Graph operations | Queries | Minimum dispatches for this complete answer | Native actual answer |
| ---: | ---: | ---: | ---: | --- |
| 7635 | 52,561 | 18,582 | 13,141 | disconnected; powered arrival-window rejection |
| 7683 | 52,592 | 18,676 | 13,148 | disconnected; powered arrival-window rejection |
| 7804 | 26,884 | 17,633 | 6,721 | disconnected; powered fuel-reserve rejection |
| 7925 | 26,881 | 17,621 | 6,721 | disconnected; powered fuel-reserve rejection |
| 8046 | 26,867 | 17,612 | 6,717 | disconnected; powered fuel-reserve rejection |

The first two jobs include the selected hypothetical site as well as the actual
pose; the last three contain only the actual pose. All stop in complete native
failure, not at the diagnostic cap. These dispatch lower bounds describe the
recorded computation, not a recommendation to increase quotas or retain old
geometry for thousands of ticks.

In [`objective_job.rs`](../scenarios/spacewars/src/surface_sortie/live_planning/objective_job.rs),
the completed short pass records its failure, then continues into full fallback.
Existing walking feedback is keyed to a `LandingSiteId`; the actual pose uses
`None`, so it supplies no corresponding site-feedback event to capture. The
controller's `actual_route_unusable` branch requires a published negative actual
answer. That answer never arrives within the current lifetime, leaving the
controller on its pending-evidence path. A failed local attempt does not itself
certify that the full route set is negative.

## A comparison that exits despite more hatch resets

World 0 P2 lands at **18199**. Its detached actual job finds a complete walking
round trip with **3 graph operations and 9,645 queries**, a lower bound of 26
dispatches from the fresh request. No flight forecast is needed.

That live visit has five hatch-motion invalidations, at **18204, 18208, 18219,
18242 and 18262**. After the last reset, generation 125 publishes a positive
actual route at **18288**; capture reports `physically_landed`, and the pilot is
on foot at **18289**. Its request can therefore deliver once the pose remains
stable long enough. Repeated survey resets do not by themselves prevent the
controller from exiting once a valid actual route arrives.

## Validation and retained evidence

All **1,040 Rust tests** and **697 Python tests** pass. The new Rust tests verify
bounded inspection, native positive crossing results, unchanged live queue
continuation and unchanged physics. Formatting, strict AI Clippy with `--no-deps`,
and profiled/ordinary release builds pass. Scenario Clippy retains the same seven
pre-existing findings. All four final replay audits pass, preserving the shared
maximum of **4 graph operations / 384 queries** and publication age at most 120.

[The manifest](data/actual-landing-route-v2.json) records complete outcomes,
phase work, live poses and source/measurement clocks.
[The compressed archive](data/actual-landing-route-v2.json.gz) retains 33 exact
documents, including both frozen plans, the corrected full summary, the original
harness failure, probe/physical/route witnesses, validation logs and hashes for
102 raw files. The separately linked stage-timing data embeds its reproduction
script and hashes its input probe, ledger and dense stream. Embedded documents,
raw files, retained source summaries and both frozen binaries were verified.

- Summary SHA-256:
  `6234d8899878088dd04a1275f07adab7be75357d3181cff41eb3327457dbc014`
- Archive SHA-256:
  `54881a8baf75ccf6b04186d43c8628ad2df55ddff4a2c3a5b2bec0451206e9c8`
- Stage-timing SHA-256:
  `eea28dbd3751bd3457cf9854271bcc6fa7407b96b0e0ce12944b3c4c6ddb4c43`

The next supported change is native feedback for a completed unsuccessful local
actual-hatch attempt, followed by a bounded relanding or departure decision.
Such feedback must identify the actor, request, source pose and current validity;
it must not become an exit permission or a claim that every unsearched route is
impossible. Preserve this exiting comparison and the complete retained match set
when evaluating that behavior. The evidence does not support relaxing hatch,
arrival-window, fuel-reserve or publication-age checks.
