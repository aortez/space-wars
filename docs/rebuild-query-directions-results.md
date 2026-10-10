# Contact-normal queries expose positive placements in the same coarse scenes

Changing only placement query direction reveals **six forecast-positive
placements at every retained coarse snapshot**. Radial queries find none. The
paired diagnostics hold terrain, footings, measured maps, routes, offsets and
all placement guards fixed. This isolates a query-frame limitation in the
previous negative search result. **Actual gameplay remains unchanged, and
executed recovery is still unqualified.**

The [frozen plan](rebuild-query-directions-plan.md) also pairs both directions
at the successful handoff scene. Its contact-normal query reproduces all 14
positive placements; the radial query instead admits four negative ones.
Unlike the [earlier radial experiment](rebuild-radial-placement-results.md),
these comparisons do not change the travel path before reaching the scene.

| Snapshot | Revision | Placement-tested footings per direction | Contact-normal accepted / positive | Radial accepted / positive |
| --- | ---: | ---: | ---: | ---: |
| Handoff 27114 | 22 | 97 | 14 / 14 | 4 / 0 |
| Coarse 25373, before veto | 21 | 91 | 6 / 6 | 5 / 0 |
| Coarse 25440, first search after veto | 21 | 91 | 6 / 6 | 5 / 0 |
| Coarse 25560, first search at new revision | 22 | 91 | 6 / 6 | 0 / 0 |
| Coarse 25710, last search before timeout | 22 | 91 | 6 / 6 | 4 / 0 |

These are snapshot-specific footing/offset pairs, not independent games or
distinct recovery successes. All **56 forecasts** finish the 120-step horizon:
38 positive and 18 negative, with no inconclusive or cap-unexamined cases.
Every negative fails to obtain two supported feet. There are **6,720 projected
physics steps**, **11,986 paired offset queries** and **24,028 total offset
evaluations**, including the repeated accepted-offset checks.

## The coarse alternatives

The same three measured footings produce two positive offsets each at all four
coarse snapshots. All six pass existing ground, alignment, hull, other-seat,
hatch-footing and hatch-route checks. Their precise outbound routes stay well
inside the existing 24-unit limit; none needs the staging extension.

| Bearing | Positive offsets | Precise outbound length | Query-frame angle difference | Hatch-route length |
| --- | --- | ---: | ---: | ---: |
| 343 | -11, -12 | 7.223 | 18.83° | 3.490 |
| 344 | -10.5, -11.5 | 7.650 | 18.13° | 3.063 |
| 345 | -10, -11 | 8.074 | 17.42° | 2.639 |

Each outbound route contains **two jumps and no jetpack flight**. Each hatch
route is walk-only. These are measured route proposals, not newly executed
traversals. At the first snapshot, conditional settling occurs 72–74 ticks
after hypothetical insertion. The later three snapshots also settle all six
placements despite the intervening terrain revision and changing world epoch.

All three footings are omitted by the native two-unit sparse spacing. They also
need offsets from the existing refined set, rather than the four coarse
offsets. None appears in the actual surveys at the three sampled post-veto
ticks. Thus these results do not establish that a direction toggle by itself
will make the playing search choose a positive placement.

There is a useful lead within the existing refinement rules: sparse bearing
341, near the positive cluster, reaches `no_hatch_route` at offset +14 under
contact-normal queries at 25373 and 25560. That is an existing promising-seed
rejection. At 25440 and 25710 the same offset is hull-obstructed instead.
Whether the bounded search discovers the cluster depends on its actual query
tick, seed ordering and retained search history; it has not been executed here.

The current native support point has no accepted contact-normal placement in
any coarse snapshot. Relocation is still required. The radial placements near
bearings 324–326 remain negative wherever they pass geometry; the separate
current support point is also negative at 25373 and 25440.

## What direction changes

Every contact-normal positive is rejected as `no_ground` by the corresponding
radial query, including all 14 handoff positives. Conversely, all 18 radial
accepted placements fail `no_ground` under contact-normal queries. There is
**no footing/offset pair accepted under both directions** in these snapshots.
The result concerns which poses the placement queries admit; it is not the
same admitted pose landing differently under two forecast models.

`no_ground` aggregates failed placement rays and their normal/alignment checks;
this study does not identify which sub-check rejected each pair. Contact-normal
means the native support up-vector at the current point and the measured node
normal at a hypothetical footing. Radial up runs from the planet center through
that point. Actual ship orientation still comes from the measured landing feet.

All comparisons reuse the unchanged two-body conditional model, with zero
launch delay and a two-second horizon. Whole selected-planet geometry remains
privileged. They do not predict travel time, arrival eligibility, the full
construction interval, future terrain, other actors, hazards, damage or boarding.
The earlier preview-versus-arrival contact-normal mismatch also remains a
separate execution concern. A stable hypothetical placement is not yet a
successful recovery policy.

## Preservation and validation

Both directions have identical maps, node order, measured normals, sparse
membership, foot positions, coarse/precise routes, staging legs and placement
eligibility. Each snapshot's native-direction report matches the preceding
[standing-search report](rebuild-standing-search-results.md) in every field
except explicit forecast wall-clock timing. Live physics, actor, native
direction flag and recovery state remain unchanged.

The two prefixes and four continuations retain their original controls and
outcomes. Every pre-existing gameplay raw file is byte-identical. Both selector
logs match outside their known timing fields. The archives preserve all
**42 handoff / 43 coarse** prior files other than those two timed logs, including
the historical standing reports and audits with their original timings. New
archives contain **46 handoff / 50 coarse** files.

Live handoff still builds at 27154, settles at 27228, boards at 27260 and completes
at **27261**, with health **60.666924**, one ship loss and three relocations.
Live coarse still blocks at **25739**, with health **100**, one loss, one
relocation and no build. Recorded continuations still end at 29421 with their
prior outcomes. No bot defaults or production placement policy change.

Runtime source **c88cf1f**, **1,026 input hashes**, five changed inputs, commands,
ticks, bounds and the copied Rust 1.89 release binary were frozen before the
batch. There are **zero simulation retries and zero audit resumes**. Passed:
**104 Rust tests** (41 native rebuild, 63 harness), **1,018 Python tests**, bot
Clippy, formatting, locked release build and default-feature library check.
Native Clippy retains the same 16 existing message/file diagnostic identities.
Unchanged engine and bot-controller suites were not rerun or counted as fresh
validation.

The next focused execution test is a contact-normal recovery continuation from
the retained coarse scene after the forecast veto, preserving the earlier
travel path. First test whether the existing bounded refinement search can
discover these sites, then require actual arrival, fresh placement validation,
construction, settling and boarding within the original task limits. Preserve
the successful handoff control and forecast veto. These results do not require
wider routes, relaxed placement guards or a global radial default.

The [manifest](data/rebuild-query-directions-v1.json) and
[portable review bundle](data/rebuild-query-directions-v1.json.gz) include all
paired reports and samples, prior references, actual search rows, timing logs,
validation, frozen source, exporter and archive receipts. Complete verified
archives remain under `target/rebuild-query-directions/v1/archives/`. Work is
local on `bot-rebuild-query-directions`.
