# Observational enemy-flag walking survey

The v13 study found no numeric enemy alternative in the fresh active reports.
Missing enemy-alternative surface evidence appeared 33,600 times across 78
visit/planet combinations; missing neutral-alternative surface evidence appeared
16,470 times across 66. Those repeated reports are not independent decisions.
The existing remote survey deliberately measures only one neutral, flagless
planet. Ownership value cannot compare an enemy alternative without a round trip.

This slice measures a narrow class of enemy alternatives. It **does not feed
the measurements into v13's evaluator or controls**. There is no new brain or
strength claim. V13 remains the visible device policy; its separate diagnostics
identify `remote_flag_walk_patch_v1` and `observational: true`.

## Evidence and work boundary

The AI requests the nearest enemy alternative in its existing three-planet
shortlist. Two material bearings immediately beside the flag are checked one
after another. Each site uses the existing atomic landing, boarding and three
climb-sample check. A coherent query snapshot is created in that same tick.

The existing contour survey is restricted to 17 adjacent samples centered on
the flag, with walking edges only. The existing proposed-hull overlay and joint
outbound/return search then look for one flag footing reachable from the exit
and connected to a measured boarding entrance. Missing footing, a disconnected
patch or a required jump remains unknown. This is neither a full-planet route
search nor proof that an omitted route is impossible.

Each actor keeps at most one pending snapshot and two historical samples. The
source expires after 30 seconds. Each completed positive result rechecks a
conservative whole-planet region extending 80 units above the nominal radius,
covering the original landing rays, hull, both entrances, climbing and walking.
Its publication tick does not renew its source tick. Subsequent delivery is
historical evidence, never current feasibility or permission to fly or capture.
Changed ownership/flag/material, landing, recovery and absent demand retire it.
Existing immutable jobs can continue through local sensor demand; a new atomic
site measurement waits for that demand to end.

Hosts dispatch local planning, neutral surveys and mission evaluation first.
The flag experiment uses only their remaining shared **4 graph / 384 query**
allowance: at most two graph operations total and one per actor per tick. Atomic
site checks reserve 192 queries and wait if that actor already used physical
work earlier in the tick. Incremental queries also draw from the remainder.
No world is stepped and no additional gravity solve is run by the observer.

Snapshot construction and publication geometry validation are bounded by the
existing 8,192-body/collider limits but remain synchronous, outside operation
fuel. Their time is separately recorded. The quota is not a whole-bot CPU or
frame-time guarantee. Queue tokens have a separate diagnostic namespace.

The fixed 512-entry index phase in the original round-trip search is significant
at this budget. The initial physical-geometry fixture required 943 graph
operations and 517 queries, completing in 942 ticks with full leftover fuel in
both seats. The final remote job indexes only present nodes: the same route
needs **442 graph operations and the same 517 queries**, completing in 441 ticks
(7.35 simulated seconds). Existing policies retain the original index work.
The sparse option is restricted to at most 33 nodes/66 walking edges; bounded
ID sorting and validation join the constructor's fixed preparation outside
charged indexing. Dispatch charges each present prefix entry separately.
Tests compare complete, one-way, disconnected and empty sparse graphs with
the original search, including IDs that wrap through zero.

## Predeclared validation

`tools/compare-flag-surveys.py` records its plan before any match starts and
refuses an existing output directory. It runs three versions of the recorded
regression (frozen v13 binary, new code with observer off, observer on), then
eight complete matches: two SHA-256-derived `native-flag-survey-v1:{0..1}` worlds,
quiet/three-second asteroid pressure, observer off/on with v13 in both seats.
All have ten-minute deadlines and native cadenced sensors. No parameters are
fitted to the outcomes.

Every pair must retain identical full per-tick controller traces, identical
evaluator bytes, mission telemetry and recorded physical outcomes. Traces are
hashed before compression. New allocation logs must account for every query
and graph operation and retain the earlier stages' use of the shared budget.
Completed positives, failures, cancellations and starvation are all retained.
The question is evidence coverage and cost, not wins against the baseline.

The initial study at `52fcaf3` finished all 11 matches (67.45 simulated minutes)
with healthy physics and exact parity in all six comparisons. The regression
produced one validated remote walking trip. The four fresh enabled conditions
started six site checks: three jobs were cancelled before completion, two sites
lacked landing/boarding/climb evidence, and one completed walking trip failed
publication geometry validation. No fresh positive was published.

That result motivated the sparse index optimization above. The same complete
plan is replayed after it as an **engineering regression**, not new independent
worlds or a strength trial. The initial logs remain under
`target/capture-flag-survey/finished/`; optimized replays use `optimized/`.
Both are retained in [the comparison record](data/capture-flag-survey-v1.json).

Focused tests cover both seats, wraparound patch equivalence to the original
full survey, quota exhaustion, clone/cancellation/expiry, authoritative ownership
and a newly blocked high climb sample. A native three-minute paired test checks
v13 evaluation, controller telemetry and physics with the observer off/on.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-flag-surveys.py \
  --baseline target/capture-flag-survey/baseline-v13 \
  --out target/capture-flag-survey/finished
```

Headless surveying is opt-in with `--survey-capture-flags true`; it requires
mission evaluation and the shared live planner. The native client runs it only
for v13 seats and exposes `mission_flag_survey` alongside existing diagnostics.
The unchanged match picker and HUD still identify the controlling v13 brain.
