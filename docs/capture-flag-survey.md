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
fitted to the outcomes. The headless opt-in observes both seats regardless of
their controlling policy; native autoplay observes only v13 seats.

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

## Replay results at `a4bfee1`

All 11 matches again finished: seven by pilot death and four by the ten-minute
deadline, totaling 67.45 simulated minutes. All physical audits passed. All six
paired comparisons retained identical per-tick controller traces, evaluator
bytes and recorded physical outcomes. Each replay also matches its corresponding
initial study's trace and evaluator hashes. The largest combined allocation was
four graph operations and 127 physical queries in a tick, within 4/384.

| Survey result | Initial index | Sparse index |
| --- | ---: | ---: |
| Recorded regression: site checks started / completed | 4 / 3 | 5 / 5 |
| Recorded regression: validated walking trips | 1 | 2 |
| Recorded regression: cancelled jobs | 1 | 0 |
| Recorded regression: total graph work | 1,823 | 1,287 |
| Recorded regression: total physical queries | 989 | 1,475 |
| Four generated conditions: checks started / completed | 6 / 3 | 6 / 4 |
| Four generated conditions: validated walking trips | 0 | 0 |
| Four generated conditions: cancelled jobs | 3 | 2 |

The shorter index allows another regression trip to finish, and subsequently
another site to be checked. That raises its total physical query count even
though a fixed survey's queries are unchanged. The first shared positive goes
from 946 to 445 elapsed ticks. This is a reduction in work and simulated survey
latency, not a measured frame-rate or playing-strength improvement.

Both regression positives belong to the **v10-controlled first seat**, revisit
the same planet 2 / bearing 14, and have zero-length outbound and return legs:
the flag is already within reach of the boarding footing. These establish a
narrow class of adjacent flag/hatch geometry, not varied walking paths or new
v13-controller evidence. The first sample's cover checks are all false; the
second has no armed-opponent cover measurement. Neither establishes a safe
combat approach.

Coverage remains limited. Of the six generated-condition checks, two lack
landing/boarding/climb evidence, two complete walking routes but fail publication
geometry validation, and two are cancelled when the request changes. Two of the
four conditions start no survey at all. Earlier completion converts one
cancellation into a diagnosed geometry rejection; it does not establish a valid
enemy alternative in these worlds. No constants or utility weights were tuned.

Focused tests cover both seats, wraparound patch equivalence to the original
full survey, quota exhaustion, clone/cancellation/expiry, authoritative ownership
and a newly blocked high climb sample. A native three-minute paired test checks
v13 evaluation, controller telemetry and physics with the observer off/on.
Final local validation passed 476 scenario tests, 155 AI tests, the native paired
test and 339 Python analysis tests. Independent review covered geometry freshness,
shared work accounting and the sparse index equivalence. A separate raw-data
audit reconciled samples and cancelled-job work with allocations and telemetry,
and checked the compressed traces against their stored hashes.

Runtime `a4bfee1` was deployed to `sw-picade.local`, with both executable hashes
verified and the kiosk active without automatic restarts. The retained automatic
match uses P1 v10 / P2 v13. The first live observation completed two site checks:
one lacked landing/boarding/climb evidence and one found no walking round trip
in the patch. The diagnostic model reports `observational: true`.

## Next investigation

Keep this evidence observational. Before connecting enemy alternatives to v13's
value comparison, record which changed geometry invalidates these completed
routes and whether it affects the actual landing, hull, hatch, climb or walking
queries. The current whole-planet validation is deliberately conservative;
its rejection alone cannot distinguish a blocked route from irrelevant motion.
Any narrower validation must still detect new obstacles and changed long-body
rotations across all those query footprints. Retain these worlds as regressions
and use new predeclared worlds for any later behavior or strength comparison.

## Reproduction

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-flag-surveys.py \
  --baseline target/capture-flag-survey/baseline-v13 \
  --out target/capture-flag-survey/finished
```

The frozen v13 executable came from `b6e74f3`, using the same release build
command; preserve that executable before building this branch. Its SHA-256 and
both study binaries' hashes are in the comparison record. Use a new output
directory for each replay; the runner refuses to overwrite an earlier study.

Headless surveying is opt-in with `--survey-capture-flags true`; it requires
mission evaluation and the shared live planner. The native client runs it only
for v13 seats and exposes `mission_flag_survey` alongside existing diagnostics.
The unchanged match picker and HUD still identify the controlling v13 brain.
