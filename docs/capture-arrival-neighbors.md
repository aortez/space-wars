# Three measured arrival candidates

## Frozen experiment plan

The [conditional preference](capture-arrival-preference.md) previously retained
only one site in physical replays. This experiment asks whether three measured
sites near the predicted arrival give a useful choice between alternatives.
It remains an observational harness experiment: playing controls, native
observations, mission rankings, cost coefficients and acquisition estimates do
not change. It does not select among all 64 native bearings.

`--arrival-survey-sites nearest|neighbors3` defaults to `nearest`, preserving
the previous request, report and work behavior. `neighbors3` requires the
arrival survey. Its fixed request contains the predicted central bearing,
previous bearing and next bearing, wrapping at 64. The fourth slot is unused.
The first issued request remains fixed for that source. Each real native query
checks just one slot. Its result is wrapped with the original three request IDs;
the other slots in that event are pending, with no invented measurement.

Each actor still reserves 192 residual physical queries, performs at most one
atomic check per tick, and waits at least 30 ticks between charged attempts.
The shared physical cap remains 384, after existing playing and evaluation work.
A charged negative, incomplete or absent result spends its slot. Zero fuel
does not. There are no retries or refreshes of previously attempted slots.

The first charged attempt fixes a 60-tick collection window. Queries stop after
that window even if a slot is missing. The fresh comparison freezes before
intent on the first real observation at or after first+61, after ingesting the
previous tick's sample once. With no delay, measurement ages are 61, 31 and 1
ticks. The original sample epochs and material frames are retained separately;
they are never rewritten to the source or arrival time. Early retirement or
query pressure leaves an explicit partial collection. All-absent results do
not create retained geometry. Negative findings remain negative findings.

A gap in observations, actor/vehicle/material identity change, replacement
request or mistimed event latches a refusal. Restoring the old context cannot
revive the collection. The host then stops querying and the fresh comparison
cannot attach or publish that geometry. Missing the deadline observation is a
refused collection recorded at the next real observation, never a repaired
source. The existing typed snapshot/episode admission remains in force.

The fresh queue uses only the original shared 64 graph operations per tick
after playing and original comparison work. Each retained candidate costs at
most two solar steps, one local-reference step and one preference step, in
addition to existing forecast/ranking work. Missing/refused candidates and
unassessed bearings remain explicit. A collected slot is not necessarily an
admitted material site, eligible direction or valid retrospective comparison.

Freeze implementation, tests, this plan and the runner before all 16 replays:
the eight enabled conditions from `arrival-preference-v1`, each with `nearest`
and `neighbors3`. These are four correlated ordinary sources and four controlled
capture continuations, original ticks 3816/3876/3934/3997, seed
3491156488288037499, seat zero, destination two, quiet world, point-v1 forecast.
Use all runs with no outcome-based selection, tuning, silent retry or omission.
The runner verifies all historical enabled file hashes before execution.

Both modes must match historical complete controls/observations, native sensor
counts, upstream evidence/work, original transfer forecasts and native capture
outcomes. Native sensor profiles wrap native observation only; observer queries
are separately charged and audited in the arrival survey ledger. `nearest`
must additionally match all previous fresh reports and ledgers. For neighbors,
audit every original request, actual measurement, refresh deferral, physical
charge, collection deadline, memory merge, graph allocation and publication.
The fixed quiet replay corpus has continuous unchanged identities; separate
mutation and Rust lifecycle tests cover explicit refusals and missing evidence.

The neighbors source is 60 ticks later than the nearest source. Differences
between those forecasts cannot be attributed solely to extra sites. Therefore
also derive a center-only preference from the **same frozen neighbor endpoint**
and compare it with the full retained-subset preference. Reconstruct scores,
direction eligibility, original epochs and native tie ordering independently.
Native best-in-retained comparisons preserve actual native assessment order,
not remote request order. Require original material/age/geometry and native
domain joins before classifying a retrospective relation; keep raw relations
and unknowns separately. Refused sites do not disappear from the source report.

Report measured/admitted/eligible counts, unknowns, subset choice, center-only
choice, actual native choice, actual best among source-admitted/projected
retained sites, query/graph costs
and readiness times. Global selection accuracy, acquisition duration, full-trip
cost, live strength and hardware timing remain unestablished. Acquire an
independent code and evidence review, update PR #122 and check exact-head CI.
No device deployment is needed for this diagnostic change.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-arrival-neighbors.py \
  --out target/capture-flag-survey/arrival-neighbors-repeat
```

Use a clean checkout, a new output directory and the archived raw baseline at
`target/capture-flag-survey/arrival-preference-v1`.
