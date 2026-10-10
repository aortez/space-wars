# No forecast-positive alternative in the coarse path's measured patch

Inspecting all reachable measured footings does not find a stable alternative
for the retained coarse path under its current query rules. Its 14 geometrically
accepted placements across four snapshots all fail to earn two supported feet
within the two-second conditional forecast. The handoff snapshot has 14 positive
placements, including the already successful native pose. **Actual gameplay is
unchanged and full recovery remains unqualified.**

The two paths retain different query directions: contact-normal placement for
the handoff and radial placement for the coarse path. They also occur at
different ticks and positions. This study does not establish that query direction
causes their difference or that the coarse scene has no physically possible
landing.

## Complete measured coverage within the existing limits

The [frozen plan](rebuild-standing-search-plan.md) inspects the existing local
ground map without sparse thinning or the promising-seed filter. Each snapshot
includes all measured nodes within 24 units of the pilot foot, plus its current
native support point. Every reachable candidate receives all 26 existing ship
offsets. Original geometry, hull, alignment and hatch checks remain in force.

| Snapshot | Terrain revision | Map nodes | Placement-tested points, including current support | Accepted placement pairs | Conditional settling |
| --- | ---: | ---: | ---: | ---: | --- |
| Handoff 27114 | 22 | 100 | 97 | 14 | 14 positive |
| Coarse 25373, before veto | 21 | 100 | 91 | 5 | 5 negative |
| Coarse 25440, first search after veto | 21 | 100 | 91 | 5 | 5 negative |
| Coarse 25560, first survey at new revision | 22 | 100 | 91 | 0 | No geometric candidates |
| Coarse 25710, last survey before timeout | 22 | 99 | 91 | 4 | 4 negative |

These are snapshot-specific footing/offset pairs, not independent games or that
many distinct sites. No forecast hits the 64-job cap, stops outside model scope,
or remains inconclusive. All 28 jobs complete 120 physics steps: **3,360 total**.
Across five snapshots there are 499 map-node inspections, five actual support
points and **12,014 offset evaluations**, including repeated single-offset
checks before each forecast.

The probe retains both coarse endpoint reachability (0.8 units) and precise
endpoint reachability (0.01 units). Each coarse snapshot has 89 precisely
reachable nodes and one coarse-only node; the latter yields no accepted
placement. The remaining 9–10 nodes are outside the allowed measured routes.
The existing walk-only staging evaluator adds no reachable coarse node. At the
handoff, 83 nodes have precise routes, another 13 have valid staging proposals,
and four remain unreachable. All positive handoff node placements have direct
precise routes.

## What the extra coarse candidates offer

The ordinary post-veto search performs ten surveys. It rotates through sparse
bearings and only refines around seeds that reach a hatch-related rejection.
The full-patch diagnostic additionally finds these geometric proposals:

| Footing | Accepted offsets at 25373, 25440 and 25710 | Precise outbound route length | Forecast |
| --- | --- | ---: | --- |
| 326 | -8 | 0.000 | Never two supported feet |
| 325 | -8 | About 0.439 | Never two supported feet |
| 324, the original previewed footing | -8, -8.5 | About 0.886 | Never two supported feet |

All three lie within about 1.14 units of the pilot at these snapshots and are
omitted by the native sparse set's two-unit minimum distance. Zero route length
for 326 means the graph already attaches the foot to that node; it is not a
claim of exact physical arrival. The separate actual support point also admits
-8 at 25373 and 25440, with the same negative classification. At 25560 none of
the points passes all geometry checks; four node placements become admissible
again at 25710 without another terrain revision.

Thus expanded sampling would expose additional placements near the failed
site, but these measurements supply no positive replacement there. Returning
to the originally previewed bearing 324 also fails the conditional settling test
at all three snapshots where its placements pass geometry. That is evidence
against assuming that later precise repositioning alone fixes the retained
failure. It does not test holding at that point from the much earlier preview
or predict the world after a future walk and full construction interval.

## Positive and negative reference checks

The handoff's actual support point admits offsets -10 and -11, both positive.
Six measured footings each admit two positive offsets: bearings **343–345**
near the pilot and **387–389** farther along the local patch. Their outbound
route lengths are 0–0.852 and 21.807–22.651 units respectively; the farther
routes include two jetpack flights. These are measured route proposals, with no
new execution or proof of future arrival.

The two initial actual-support forecasts exactly reproduce the archived
[local-forecast references](rebuild-local-forecast-results.md): all 121 ship,
planet, landing and contact rows match at native precision, with zero position
difference.

| Reference | First contact | Two supported feet | Settled |
| --- | ---: | ---: | ---: |
| Handoff at 27114, offset -10 | 27173 | 27174 | 27188 |
| Coarse at 25373, offset -8 | 25422 | Never | Never |

Each new job assumes hypothetical construction at the snapshot's epoch, with
zero launch delay. It uses the existing two-body model, whole selected-planet
geometry and physical gravity sources. It does not predict walking time,
construction eligibility, the eight-second build interval, other actors,
hazards, damage or terrain evolution. A positive is conditional landing
potential; a negative covers only this two-second model and the tested pose.
The synchronous diagnostic work is not a proposed gameplay frame budget.

## Preservation, validation and next step

The fixed batch completes two prefixes and four continuations, with five
read-only snapshots. Every pre-existing non-timed raw replay file is
byte-identical to `rebuild-forecast-selection/v2`. Both selector logs match in
every field except their explicitly named wall-clock measurements. All **40
handoff / 38 coarse** prior archive files other than those two timed logs are
preserved byte for byte; reused audits retain their original timings. Both new
and old selector logs are included for comparison.

The live handoff still completes at **27261**, with health **60.666924**, one
ship loss and three relocations. Live coarse still blocks at **25739**, with
health **100**, one loss and one relocation. Recorded forks still reach 29421
with their original outcomes. No control, task deadline, gameplay placement
policy or default setting changes.

Runtime source **c1fdf5b**, **1,023 input hashes**, eight changed inputs and a
copied Rust 1.89 release binary were frozen before execution. There are **zero
simulation retries and zero audit resumes**. Passed: **103 Rust tests** (40
native rebuild and 63 harness), **1,010 Python tests**, bot Clippy, formatting,
locked release build and default-feature library check. Native Clippy retains
the same 16 existing diagnostic identities. Unchanged engine and bot-controller
suites were not rerun or counted as fresh validation.

The next focused comparison is **radial versus contact-normal queries on the
same retained coarse snapshots**, retaining every placement guard and measured
route. The earlier direction experiment changed the selected travel path before
reaching this scene; this one holds the scene fixed. It can separate candidates
hidden by the query frame from the current patch's remaining geometric and
settling limitations, before changing search extent or recovery policy.

That follow-up is now recorded in the
[same-scene query-direction results](rebuild-query-directions-results.md).
Contact-normal queries expose six conditional positive placements in every
coarse snapshot while radial queries retain none, with identical maps and
routes. The original radial-only result above remains valid within its query
frame; executed recovery still needs validation.

The [manifest](data/rebuild-standing-search-v1.json) and
[portable review bundle](data/rebuild-standing-search-v1.json.gz) contain full
maps, all placement rejections, every forecast, both reference forecasts, actual
search observations, preserved timing logs, validation, frozen source and the
exporter. Verified complete replay archives remain under
`target/rebuild-standing-search/v1/archives/`. Work is local on
`bot-rebuild-standing-search`.
