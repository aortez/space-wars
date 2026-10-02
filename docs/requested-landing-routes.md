# Requested landing corridors

The requested walking corridor now supplies the cover route and enables a
complete enemy capture in both directed control runs. The blocked cases are
unchanged, and all eight armed runs remain losses. Keep the feature experimental.

The [focused-route study](focused-landing-routes.md) delivers short walking
routes, but cover search waits for a different site whose route exceeds that
patch. This experiment extends early positive delivery to explicitly selected
sites and the actual touchdown hatch. It remains opt-in; no bot default changes.

`--requested-objective-routes true` requires focused routes, early delivery,
ground reuse and route dependencies. Reports use `live_joint_objective_v8`
or `live_jetpack_objective_v8`. Existing profiles keep their original behavior.

## Measurement contract

Keep the existing short patch when it applies. Otherwise, for a selected site
or actual hatch, test a single shorter surface arc toward the flag. Measure five
ordinary ground samples around the hatch, choose the nearest clear footing,
and require both the existing start distance and a supported boarding envelope.
Proceed along adjacent positions on the ordinary 512-sample contour, stopping
at the first flag footing within the existing standing-center range. Reject
arcs that could require more than 96 edges, including endpoint margins.

Each node uses the ordinary retained-floor ray, support-normal and spaceling
capsule checks, plus a separately charged hypothetical-hull query. Each edge
uses the native directed walking rise gates, nine capsule samples, nine hull
checks and three support rays, in **both** directions. Every physics operation
performs one actual query; node inspections and transitions consume graph work.
There is no general graph indexing/search or variable scan hidden in a query.

This certifies one walk and its return, not a shortest path. It neither forecasts
jumps/flights nor declares other routes unavailable. A blocked corridor stays
unknown. After success or failure, the original full survey continues against
its original snapshot and clock; its complete result remains unchanged.

Capture query footprints in bounded per-node/per-edge groups, including failed
start samples, and include the entrance probes. Publication uses the existing
current geometry, actual hatch and 120-tick source-age checks. Recheck the
maximum walking rise against current gravity. The corridor reissues its queries
against the retained snapshot and does not count them as cached reuse. The full
survey retains its existing cache behavior. Snapshot copying, fixed setup,
publication validation and ordinary immediate/on-foot sensors remain outside
dispatch counters; quotas are not total frame-time limits.

Selected-site changes do not cancel an already-running request. The next normal
request uses the current selected site; pending work keeps its original expiry.
Count corridor starts, completions and positive measurements separately from
full candidates and short focused patches, including retired requests.

## Frozen physical trial plan

Freeze code, tests, runner and this plan before collecting mission outcomes.
Use all 14 shared cases from `target/focused-routes/v2/summary.json`:

1. Replay all six directed missions with the new option disabled. Require their
   physical/mission report fields, seven controller/evidence streams, ordinary
   sensor rows, every allocation ledger row and non-timing planner counters to
   match. Keep all existing focused-route settings.
2. Enable requested corridors in all six 180-second directed missions: blocked
   bearing -0.8 with cover on/off and the +0.8 walkable control with cover on,
   each using walking and powered v13, seed 42 and seat 0.
3. Enable the option in all eight 600-second armed cases, regardless of directed
   results. Keep both generated worlds, both v13 seats versus v10, both route
   models, two active planners, weapons and no asteroids.

This is six retention replays and 14 new trials. Change only the new option,
binary and output path. Keep the shared 4 graph / 384 query allowance, lifetime,
cover-search deadline, probe count and all physical permissions. Preserve every
loss, unknown route and unfinished visit. Two runs may execute concurrently;
desktop timings are not a Pi performance claim.

Use the existing physical auditor for model identity, contiguous observations,
source age, live validation, combined quotas, launch charge, claims, original
ship boarding and departure. Record first action differences against the focused
baseline. Retain first deliveries, selected-site rows and cover-state transitions
so route delivery, cover selection and physical success can be distinguished.
Pin all original input hashes before and after the study. Do not promote defaults
or modify acceptance thresholds in response to these outcomes.

```sh
python3 tools/validate-requested-routes.py \
  --prior target/focused-routes/v2/summary.json \
  --binary target/requested-routes/surface_mission_soak-COMMIT \
  --out target/requested-routes/v1
```

## Cover delivery and physical results

The frozen `2b61aee` implementation completes all 20 planned runs. All six
disabled replays retain their physical/mission fields, 42 exact evidence streams,
64,800 ordinary sensor rows, allocation ledgers and non-timing planner counters.
All prior input hashes remain unchanged.

In both directed control runs, cover search starts at tick 3,565. The request
at tick 3,664 measures site 0's 49-edge walk, about 35.72 units each way. At
3,691 the planner delivers this 27-tick-old route and the controller selects
the covered site. This is also the first action difference from the focused
baseline. Previously the search reached its 4,165 deadline without a measured
cover site and abandoned the enemy visit.

Both control runs now execute that visit and then capture the neutral planet:

| Physical enemy milestone | Walking v13 | Powered v13 |
| --- | ---: | ---: |
| Landed | 4,925 | 4,925 |
| Exited ship | 4,973 | 4,973 |
| Claimed enemy planet | 6,395 | 5,858 |
| Boarded original ship | 7,381 | 6,348 |
| Departed | 7,606 | 6,572 |

Each finishes two sorties instead of one. The actual touchdown return arrives
within the same source-age limit; no candidate route substitutes for that
check. Neither run launches a jetpack. Powered mode retains its existing v12
walking behavior, which accounts for controller differences without a flight.

Across all 14 new games, 39 corridor attempts start and 33 complete, all with
positive walking measurements. Six attempts do not complete.
The 33 requests delivering long routes have first-delivery ages of 27–98 ticks.
There are 37,536 total deliveries including ordinary short patches and repeated
validation; these are not independent physical successes. No publication is
older than 120 ticks. The full fallback still completes zero candidates and
starts zero flight forecasts.

## Armed outcomes and remaining limits

All eight v13 armed runs remain losses, with unchanged completed-sortie counts.
Seven retain the entire original action sequence. In world 0, P2 walking, nine
long corridors deliver at ages 29–33 ticks. The first changes controls at 24,955,
when the planner supplies site 33 for planet 2. That visit never lands or claims;
it ends for ship/surface recovery at 26,407. The match finishes at 26,591 instead
of 27,344, with the same four completed sorties. Route delivery alone does not
establish better combat performance.

The four blocked directed runs also retain their original action sequences and
single neutral capture. They start no new long corridors. In the cover-on
walking case, the requested site 0 at tick 3,931 is approximately **182.16 ground
samples** from the flag, outside the 96-edge bound. It stays unknown and reaches
the cover-search deadline at 4,511. The archive retains the complete selected
site observation, its angular calculation and the deadline/abandonment rows.

The remaining work is timely evidence for longer or powered routes, including
how cover search proceeds when its first requested corridor cannot be supplied.
Do not interpret a skipped or blocked corridor as a negative route measurement.
These runs leave the quota, lifetime, search deadline and defaults unchanged.

## Evidence and validation

The [result manifest](data/requested-landing-routes-v1.json) records every game,
comparison, physical visit, delivery clock and work total. Its
[compressed evidence](data/requested-landing-routes-v1.json.gz) contains 39 exact
documents: current/prior summaries, hashes for all 261 raw files, physical and
cover witnesses, delivery histories and validation logs.

All 448,870 pilot observations and 256,835 dispatch ticks pass the original
physical/evidence auditor. The combined charge stays at or below 4 graph / 384
queries each tick. Validation passes 986 Rust tests, 648 Python tests, strict
AI library/harness Clippy, formatting and profiled/ordinary release builds.
Scenario Clippy reports only the same seven pre-existing findings in unchanged
files. Unit checks cover actual query charging, both directions and sample-ID
wraparound, real stepped-terrain gaps, blocked hull/boarding, shared two-actor
delivery, actual touchdown, dependency/gravity changes, source age, warm
snapshots, zero allowance, cancellation and full-survey parity.

The preserved binary is
`target/requested-routes/surface_mission_soak-2b61aee`, SHA-256
`62530aa1493cc6b4fc8b63f50b41fa3f5623f813bd36fbba9eb23e9ec340c51e`.
The complete summary at `target/requested-routes/v1/summary.json` has SHA-256
`646adbcc49705c438128667b7f6d76dbdcd587aba7c9b1bbb77d6479ba38ae05`.
