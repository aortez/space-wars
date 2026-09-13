# Reusing ground measurements in live bot planning

This continues the [live objective-survey adapter](live-bot-surveys.md) from
[#89](https://github.com/aortez/space-wars/pull/89), within step 3 of
[#81](https://github.com/aortez/space-wars/issues/81). The new opt-in
`live_joint_objective_v2` sensor profile reduces repeated physical queries when
gravity, our own landing/hatch pose, or the scheduled request refresh changes.
The same executables retain v1 and the synchronous v9/v10 policies for comparison.
Launcher and Picade autoplay settings remain unchanged.

## What survives a restart

The base survey now records two kinds of gravity-independent evidence:

- Each sampled footing's retained-floor, slope and capsule-clearance result,
  including rejected footings.
- Each directed adjacent walk's capsule and continuous-floor result, including
  blocked walks. Walking eligibility is still checked against the new gravity
  before consulting this result, preserving the original survey's rise test.

A restart can move these measurements from a pending or completed job into its
replacement. It does not copy the map or keep a second cancelled job alive.
The new request reconstructs its map in the original order, recalculates jump
eligibility, remeasures every needed jump arc, and rebuilds each proposed-hull
overlay and outward/return search. A partly completed footing or walk is queried
again; only complete individual measurements enter the cache.

Each reused footing costs one graph operation. Each reused walk is a constant
time lookup within its already charged candidate operation. Fresh measurements
still consume individual physical-query units. A saved query count records the
queries originally needed to establish that footing or walk; it is separate
from the queries actually dispatched. Tests reconcile these counts against a
fresh survey of exactly the same snapshot at the new gravity.

## Validity and bounded lifetime

The cache carries its original immutable query snapshot and planet frame.
Fresh jump queries also use this snapshot; a request never combines unvalidated
samples from different physics steps. Compatible ground still requires the
same actor, planet/material revision and objective, plus validation of collision
geometry against the live world. Both old and current collider positions are
checked, with the existing exclusions for this pilot and its ship.

A gravity or hatch change cannot bypass obstacle validation. Before salvaging
the ground, the adapter checks geometry again using the new gravity's required
region. Lower gravity can expand that region. Terrain edits, other moving
ships/debris, changed goals and loss invalidate the cache; no route or transfer
permission is inferred from a stale or incomplete request.

Revalidation always compares against the **original** snapshot/frame, preventing
small tolerated movements from accumulating through a chain of cache copies.
The original measurement tick also survives every restart. Evidence expires
after 120 physics ticks, even if the replacement request just started. The
result's validation tick records current checks; it does not renew its source
age. Requests retain their own start tick for the 30-tick refresh cadence and
completion-delay telemetry.

The two-request bound still holds. Each request adds at most 512 footing entries
and 1,024 directed walk entries to the existing sequential map/search workspace.
These entries own one reference to the request's existing snapshot. There is no
cross-actor or idle cache, and reset, removal and expiry release it. These are
structural bounds, not allocator-byte measurements. Snapshot limits and the
single global graph/query allowance are unchanged. Validation and snapshot
creation remain outside that allowance and inside sensor timing.

## Reproduction and diagnostics

```sh
cargo +1.89.0 test --locked --release \
  -p engine-core -p engine-rapier -p scenario-spacewars -p spacewars-ai
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_flag_soak --example surface_mission_soak --features sensor-profile

target/release/examples/surface_flag_soak \
  --policy material_mission_v10 --seed 42 --seat 0 --offset -0.8 \
  --mode capture --jetpacks true --survey-landing true --landing-threat false \
  --edit none --expect complete --live-objective-planning true \
  --reuse-objective-ground true --out /tmp/reused-objective-flag

python3 tools/compare-mission-policies.py \
  --binary target/release/examples/surface_mission_soak \
  --baseline material_mission_v10 --candidate material_mission_v10 \
  --live-objective-planning --reuse-objective-ground \
  --seconds 180 --seeds 9216675843324634618 --out /tmp/reused-versus-synchronous-v10
```

Both physical runners accept `--reuse-objective-ground true` alongside the
existing live-planning options. Omit it for retained v1. The comparison tool
applies both options to its candidate role only; this is a scoped live sensor
experiment, not an equal-budget comparison of whole bots.

The live report now identifies its configuration and records reused requests,
footings, walks and avoided queries; rejected reuse attempts; the oldest
published source; and the largest request-to-publication delay. It also totals
work in retired requests that never published. Some of that work may have been
salvaged, so those retirement totals must not be described as wholly wasted.
Allocation CSV rows now include request generation, distinguishing restarts for
the same actor. Their age is queue/request age, not cached measurement age.

## Validation checkpoint

The reference is main `87d907f`, the squash merge of #89. Reproduction scripts,
exact binaries, source patch, commands, allocation traces and reports are kept
under `/home/oldman/.codex/visualizations/2026/09/13/bot-survey-reuse/`.
The preceding v1 checkpoint remains in `live-bot-surveys/integrated/`.

All 699 relevant release tests pass. Eleven live-adapter tests also pass in debug
mode with CI's 16 MiB test-thread stack. The new tests cover interrupted/full
surveys at gravities 1, 10, 30 and 300; exact correspondence with fresh maps and
forecasts; query savings accounting; gravity and own-hatch restarts; simultaneous
gravity/obstacle changes; real excavation; original-age expiry after repeated
reuse; token revocation; cloned continuation; and snapshot release.

With reuse disabled, the two v1 capture fixtures and the ten-minute v1 duel
preserve all non-timing reports and allocations against the preceding binary.
The duel preserves all 72,000 dense player records. A synchronous v9/v10
generated match also preserves its 49,812 player records and non-timing report.
Formatting, whitespace, client all-targets checking and Clippy pass; Clippy
retains warnings in unchanged code.

All six v2 controlled captures (both seats, query allowances 512/1024/2048)
capture, board and depart with physical audits passing. Repeating the default
seat-0 case preserves the entire non-timing report and every allocation. At the
default 1,024 queries, total physical queries fall from 1,294,003 to 1,083,943
for seat 0, and from 1,302,551 to 1,119,827 for seat 1. These executions can
follow different paths and are workload comparisons, not matched-trajectory
timing measurements.

For seed `9216675843324634618`, two live v10 bots run a complete ten-minute
generated match without asteroids, sharing 16,384 graph operations and 1,024
physical queries per update:

| Measurement | Retained v1 | Ground reuse v2 |
| --- | ---: | ---: |
| Physical queries dispatched | 16,668,139 | 9,685,370 |
| Graph operations dispatched | 124,558,576 | 111,378,138 |
| Query snapshots built | 1,422 | 891 |
| Distinct forecasts published | 724 | 620 |
| Updates publishing a validated forecast, summed over actors | 3,129 | 8,965 |
| Median completed-job queue age | 27 ticks | 10 ticks |

Physical-query work falls **41.9%**, graph work **10.6%**. The v2 counters record
531 reused requests, avoiding 7,003,415 queries within those requests. Its
median queue age includes finished jobs that become stale before publication;
17 such jobs occur in each profile. The worst request-to-publication delay
remains 29 ticks. Older cached evidence reaches 105 ticks, and the original-age
cap causes 125 expiry invalidations. Thus fewer distinct forecasts are produced
even though usable answers are available on more updates. Counts of publication
and completion are different measures, and neither establishes bot strength.

Both matches reach the time limit with the same 1–2 ownership score and surviving
pilots. The v2 run has 2,296 dispatches doing work for both bots; every aggregate
and per-actor allocation satisfies the shared quota. A full repeat preserves
all 72,000 dense player records, non-timing reports and allocations. Peak retained
request count stays at two.

The asteroid reproduction (`7725194555774358125`, strikes every three seconds)
still invalidates all 18 requests before publication. It performs no reuse and
preserves the preceding run's 53,910 player records and allocations exactly;
pilot death ends it at tick 26,955. This slice does not solve planning amid moving
obstacles. A three-minute seat-swapped comparison also exercises v2 against
synchronous v10 through the existing harness; it is not a strength estimate.

Candidate mission binary SHA-256:
`f1f0d1ca2456ce709f4758e7ac1fe223026f000df58dd9650394e4764d768172`.
Reference mission binary SHA-256:
`5c1bf3c9d7e781f5aa7a9551299c0af46b13ac69af75529c4a305e2c7e4f0458`.
These are desktop headless measurements. The earlier Pi timing table applies to
v1; this slice has not been installed or timed on the cabinet.

## Next boundary

The follow-up [local route dependency profile](bot-route-dependencies.md) now
implements positive-path validation and records what remains unresolved in the
asteroid reproduction. The v2 checkpoint above retains its original semantics.

Keep this optional while investigating moving-obstacle invalidation. The next
useful split is persistent retained-material evidence versus local obstruction
checks for footings and crossing edges. An obstacle entering an edge's corridor
must still revoke that edge; checking only endpoints or terrain revision would
be unsafe. Use the saved asteroid reproduction to test whether finer dependencies
permit complete, currently valid plans. Original-age expiry and refresh timing
are also visible now, but should be tuned using completion data rather than
extending freshness blindly. Broader sensor budgeting and mission-level utility
remain subsequent work.
