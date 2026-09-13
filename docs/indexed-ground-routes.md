# Indexed ground-route queries

This is the first implementation step from
[budgeted bot planning](design/budgeted-bot-planning.md), following the
[measured ground-route stalls](ground-route-profile.md). It preserves the
`material_mission_v9` decisions while changing graph lookup. Joint sortie
selection, retained material-policy comparisons and a shared planning budget
remain subsequent steps in that design.

## Implementation

`GroundMap::routes()` returns a `GroundRoutes` query object borrowing one
immutable measured map. It builds direct node lookup and a compact outgoing-edge
index once. Each node's outgoing entries retain their original edge order;
node selection, cost arithmetic, equal-cost ties, partial routes and diagnostics
retain the previous rules. Existing one-shot map methods delegate to the same
query implementation.

Landing-objective evaluation shares the index between its outbound and return
searches. Ground navigation also shares the augmented jetpack graph's index
between its complete-route attempt and partial-route fallback. A different
candidate hull or an edited/augmented map gets its own index. Nothing is reused
across changing physical surveys, and the serialized observation is unchanged.

The lookup removes the full edge-vector scan and linear node lookup at each
settled node. The 512-slot next-node minimum scan remains. With `V` as the bounded
node-slot capacity, the search work changes from roughly `O(V² + VE)` to
`O(V² + E)`, plus `O(V + E)` index construction. This does not reduce
ground-connection physics queries.

Opt-in profiling adds `ground_route_index` and
`ground_route_index_scanned_edges`, which counts both construction passes over
the edge vector. `ground_route_scanned_edges` now counts only examined outgoing
entries. Include index construction when comparing total routing cost; its
scope is separate from `ground_route`. Both are children of
`landing_objective_routes` for the paired landing searches.

## Validation contract

A test-only copy of the former lookup at `1e8ce10` compares complete returned
paths and every diagnostic field. Cases cover unordered node storage,
equal-cost parallel edges, one-way access, an added jetpack edge, empty maps,
missing endpoints, sparse directed surveys and partial routes. The survey
matrix compares 192 queries across four graphs. The reference is absent from
production builds.

The existing scenario and focused AI suites cover actual material surveys,
excavation, moving obstacles, ground/jetpack navigation, objective landing and
three-minute generated asteroid duels. Pi replay uses both recorded slow seeds,
matching all non-timing report/CSV fields and every observation's physical-query
counts, visited-node counts and stage call counts. Only the edge-lookup counts
and the new index stage are excluded from that last comparison.

The material-policy identity is retained because this step preserves route and
decision semantics. The next behavior-changing candidate must remain selectable
alongside its predecessor, using the lightweight comparison contract in the
design. This optimization is not evidence of smarter mission choices.

## Paired Pi results

Both binaries use Rust 1.89.0, the same compatible AArch64 linker and opt-in
sensor profiling. The retained instrumented baseline at `1e8ce10` runs first,
then the indexed implementation, on `sw-picade.local` on 2026-09-13 UTC.
Each runs both seeds with a 600-second limit, generated worlds, two mission
bots, four-Hz landing surveys, normal exhibition settings and no random
asteroids. The second seed ends at pilot death after 302.8 simulated seconds
in both builds. This is four physical runs, about 30 simulated minutes total.

| Measurement | Seed `9216675843324634618`, before → after | Seed `7725194555774358125`, before → after |
| --- | ---: | ---: |
| Routing at the known slow observation, including index construction and round-trip wrapper | 14.63 → 6.55 ms | 15.55 → 6.79 ms |
| Complete known slow observation | 52.71 → 44.35 ms | 52.99 → 42.64 ms |
| Worst complete sensor call anywhere in the run | 52.72 → 44.35 ms | 52.99 → 43.30 ms |
| Mean sensor call | 0.653 → 0.642 ms | 0.378 → 0.365 ms |
| Sensor p99 | 23.00 → 22.60 ms | 20.74 → 20.68 ms |
| Sensor calls exceeding 16.67 ms | 1,521 → 1,521 | 424 → 424 |

The matched observations are runner row 21,511/P1 and row 8,626/P2. Indexed
seed two's maximum moves to row 13,291/P1. The complete sensor timing includes
a small outer-wrapper overhead beyond the nested `mission_observation` scope.

Routing work at the matched calls falls by **55–56%**, including the new index
cost. Examined search edges fall from 4,648,896/5,055,952 to 9,213/9,980;
constructing the eight indexes additionally scans 16,128/16,192 entries.
Visited nodes remain exactly 4,626/5,012. Accumulated routing including its
wrapper falls from 722.73 to 422.51 ms and from 298.85 to 150.87 ms.

All **54,168 paired update rows** match in every non-timing CSV column and all
non-timing report fields match. Every one of **108,336 paired sensor
observations** has matching stage call counts and query/visited-node counters,
excluding only the changed edge scanning and new index work. Both builds pass
all physical/material audits. This includes sampled/final pilot and planet
state, mission milestones and the finished-round results.

The total sensor means improve by only 1.6%/3.4%; broad tail latency and the
number of calls above the frame budget barely change. Ground connections still
consume 27.86/7.80 seconds across the indexed runs. The improvement addresses
the extra route-search contribution to the largest pauses; reducing and
budgeting physical survey work remains necessary. These headless timings do
not establish rendered FPS or improved bot decisions.

All 128 frequency samples read 1.5 GHz. Temperatures span 70.6–72.5 °C in the
baseline and 71.1–73.5 °C in the indexed run. This is one ordered pair per seed,
not a distribution of repeated timing trials; do not attribute every small
change in unrelated stages to route lookup.

The local validation passes **423 tests**: 366 scenario tests and 57 focused AI
tests across ground navigation, jetpack, objective landing and surface missions.
The default-feature build check, Rust formatting and whitespace checks pass.

The kiosk is paused/suspended for each headless pair and resumed afterward.
Downloaded artifacts are checksum-verified, saved settings match byte-for-byte,
and final UI state is active unpaused Spacewars with zero kiosk restarts and
the same process ID. Its installed game binary has not been replaced.

## Reproduction and retained evidence

Use the command and Pi-compatible linker in
[ground-route profiling](ground-route-profile.md#reproduction) with either seed.
The old and new executables use identical runner arguments. The test-only
reference comparison can be run with:

```sh
cargo +1.89.0 test --locked --release -p scenario-spacewars \
  --features sensor-profile ground_navigation
```

Raw reports, CSVs, per-observation profiles, thermal samples, before/after
settings, source patch, analysis and collection scripts, test/build logs,
verified executables and final kiosk state are retained in:

`/home/oldman/.codex/visualizations/2026/09/13/indexed-ground-routes/`

- Baseline executable SHA-256:
  `a4851b3c21f390391a66b86a50f48a4d6e09d89226130b7becde3259f429ef18`
- Indexed executable SHA-256:
  `65b20c79fdf074bae69a02390bb36c79a3636e9743c4adf7a982f4c26d2aa2d9`
