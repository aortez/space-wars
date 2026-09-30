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

## Results

Implementation, tests, runner and plan froze at
`e5b458e6efa63022d46939a59a1517480074c828`. Profiled binary SHA-256:
`2bbf3ed5822652736b65557eb22e19a36ddc940db412290d61888717e71ffb39`.
All **16 runs** pass, totaling **88,118 physical ticks**. Full controls and
observations, native sensor counts, upstream evidence/work, original transfer
forecasts and native outcomes match the historical runs. The nearest-mode
fresh reports and work ledgers match too. No runtime, model, runner, case or
coefficient changes followed the new replay outcomes; no runs were retried or
dropped. All four ordinary cases remain untriggered in each mode.

Each controlled neighbor collection attempts bearings **33, 32, 34**, in that
order, at first/first+30/first+60. It freezes at first+61 with original sample
ages **61, 31, 1**. All 12 slots have actual measurements; **ten** provide usable
landing geometry and **two** report `no_landing`. The latter remain explicit
unknown sites in the conditional comparison.

| Original source | Neighbor source | Ready tick | Usable sites | Unassessed bearings | Physical queries | Fresh graph work |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| 3816 | 3914 | 3940 | 3 | 61 | 180 | 1616 |
| 3876 | 3974 | 3999 | 2 | 62 | 122 | 1542 |
| 3934 | 4028 | 4052 | 2 | 62 | 122 | 1505 |
| 3997 | 4097 | 4122 | 3 | 61 | 180 | 1545 |

In all four flights, both the center-only preference evaluated at the new
endpoint and the full retained-subset preference select **33 / +1**. This
matches both the actual full native choice and the actual best among the
source-admitted/projected retained sites. All ten material/age/geometry joins
pass, with maximum same-frame residual **0.000273 world units**. Every usable
site has two eligible solar directions; the native small-angle correction
makes each site's direction scores tie, so its preferred-direction ordering
still matters.

| Original source | Center score | Previous-neighbor score | Next-neighbor score |
| --- | ---: | ---: | ---: |
| 3816 | 1.619427 | 8.130936 | 11.370341 |
| 3876 | 1.106288 | Unknown | 10.856565 |
| 3934 | 1.487643 | Unknown | 11.237622 |
| 3997 | 3.551586 | 6.198925 | 13.301942 |

These scores are native approach units, **not seconds**. The extra measured
alternatives confirm the center's conditional preference in this corpus; they
do not improve or change it. The real native selection still visits many more
bearings. Four correlated flights with the same central winner do not establish
general selection accuracy or stronger live play.

Neighbor surveys cost **604 queries**, versus **720** in nearest mode, with
12 charged attempts in each. The old path remeasures the same center on later
ticks; the new path spends those attempts on different slots. Two early
negative results cost only two queries each, accounting for the 116-query
difference. The maximum observed combined physical charge is **126**, under
the unchanged cap of 384; the 192-query per-attempt reservation and 30-tick
refresh gate hold.

Fresh graph work totals **6208**, versus **6332** for nearest mode. This is
**not a demonstrated planning speedup**: neighbor sources start 60 ticks later
and therefore have different remaining forecasts. Screening, local-reference
and preference work is separately accounted per retained slot, including
refusals, under the same 64-operation shared cap. Source-to-publication delays
are 24–26 ticks; collecting all three samples adds one second before that
comparison starts. Observed acquisition remains one tick in these four native
flights, while predicted acquisition and whole-trip duration remain unknown.

The negative samples at **bearing 32 / ticks 3943 and 3997** are particularly
useful. Native selection later has a usable bearing-32 site in both flights.
Thus an earlier `no_landing` cannot be treated as proof of future native
unavailability. Both attempts spend exactly two queries. Reading
`vehicle_landing_site_with_query_observer` places the failure in the two
footing-ray/slope checks, before hull placement, cover or climb checks; the
current log does not identify which check failed. To investigate, replay the
same frozen commands and record the existing `LandingQuery` observer plus ray
hits/normals at those exact ticks, without changing the decision. Compare the
later same-ID native survey in its own material frame. Do not fill the old
negative slots from that later successful survey.

Local validation passes **417 Rust tests**, **520 Python analysis tests**,
formatting and diff checks. Clippy reports eight existing dependency warnings
and none in the AI/harness. Independent pre-freeze review checked collection
lifecycle, throttling, terminal refusals, mode enforcement, native tie order
and retrospective validity. Its findings added explicit charged-absence and
pending-slot handling, missed-deadline refusal checks, and a same-frame
center-only comparison with an explicit admitted-site denominator.

The [compact tracked projection](data/capture-arrival-neighbors-v1.json) retains
commands, hashes, slot findings, actual native assessments, preferences,
same-frame comparisons and accounting. Full proofs remain in
`target/capture-flag-survey/arrival-neighbors-v1/summary.json`, SHA-256
`5fa9867a57a477ef1c945914a4ebe6fccb1d8008ae987d459ca6c8443253aacd`.

The [independent post-run audit](data/capture-arrival-neighbors-audit-v1.json)
passes with no findings. It checks 232 raw files and 16 logs, 176,252 complete
control/observation rows and the same number of native sensor rows, 23,932 fresh
work-ledger rows and 968 active validations. It reconstructs 14 native material
joins (four nearest and ten neighbor sites), scores and solar comparisons;
score and solar reconstruction errors are zero. The compact projection,
quantitative notes and saved test logs pass too. One checker-only correction
handled the projection's explicit transformation of raw records; it changed no
runtime, experiment or result. The retained checker is
`target/capture-flag-survey/arrival-neighbors-post-audit.py`, SHA-256
`939900bd392a9df9d0fb11d8d60225260a37489f6d867192a438ca04363df793`.

The [native acquisition audit](capture-acquisition-waits.md) follows up how query
availability and the local selector affect the wait before selection. It
separates the wider study's enemy-flag waits exceeding twenty seconds from
no-flag scan scheduling. These quiet one-tick arrivals cannot supply a general
acquisition estimator, and this multisite result does not remove that missing
part of a whole-trip comparison.
