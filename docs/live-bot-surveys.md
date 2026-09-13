# Live, budgeted landing-objective surveys

This continues [#81](https://github.com/aortez/space-wars/issues/81) after the
[resumable graph scheduler](bot-planning-jobs.md). It connects real physical
measurements to that queue while the bot continues emitting ordinary controls.
The existing v9 and synchronous v10 profiles remain available. The new
`live_joint_objective_v1` sensor profile is opt-in in the physical test runners;
the launcher and Picade autoplay continue using their existing profiles. The
follow-up [ground-reuse experiment](bot-ground-reuse.md) adds an independently
selectable `live_joint_objective_v2` profile in the same runners.

## The first request

The request answers: for an existing enemy flag, which shortlisted landing
poses admit a measured outward **and return** ground trip with the proposed
parked ship present? Its work now resumes across simulation updates:

1. Sample up to 512 outer-contour footings and their directed walk/jump edges.
2. Apply each proposed hull's clearance tests to that shared base map.
3. Run the existing joint outward/return search for each of up to eight sites,
   plus the actual landed pose when a hatch is available.

The bot keeps current landing, collision, cover, hatch and action-permission
checks. An unfinished request has no route answer. Invalidated evidence resets
the approach and uses existing hold/lift behavior; it does not spend a failed
landing attempt. The overall capture timeout still bounds an unsuccessful
mission. A completed negative result concerns measured outer-contour walking
and jumping; it does not rule out mining, caves or unmeasured jetpack access.

This is a **landing-objective allowance**, not a total bot or frame deadline.
Landing-site selection/validation, immediate observations, on-foot and recovery
surveys, controls, the shared physics/gravity step and rendering remain outside
it. Both policy descriptors therefore retain `planning_work_quota: null`; the
runner separately records the live sensor configuration and its scoped budget.

## Coherence, freshness and lifetime

`QuerySnapshot` owns the completed world's body/collider poses and spatial
index. Immutable collision shapes are shared; solver/contact state is not
copied. Resumed queries read only this snapshot. A source frame carries the
planet-local map, and the result retains its **measurement tick**. Publication
adds a separate validation tick instead of relabelling old data as new.

Before publishing or retaining a request, the adapter checks the actor/form,
objective/ownership/material revision, flag region, gravity, actual ship/hatch
pose, and collision dependencies in the survey's region. It examines both old
and current geometry, including obstacles entering or leaving that region.
Rigid planet motion is allowed; relative obstacle motion, collider replacement,
removal, sensor/filter changes and excavation revoke the evidence. PhysicsWorld
replaces generational collider handles when shapes change; this check relies
on its existing prohibition on in-place shape mutation.

An enclosing arena wall must be tested against its actual geometry: its AABB
contains the entire arena and would otherwise invalidate every rotating planet.
Pose/flag roundoff tolerances are 0.002 world units, below the survey's 0.02-unit
capsule margin. This tolerance and a valid forecast never grant a transfer or
collision permission. Gravity changes over 0.01 invalidate the jump model.

Requests expire after 120 physics ticks. A ready result is reused until its
source is 30 ticks old, provided it passes the live checks. A slower successful
request is published once before refresh, rather than being discarded because
it missed that refresh period. At 60 updates/s these ages are two seconds and
half a second. In the retained v1 profile, cancellation or refresh rebuilds the
complete measurement. The opt-in v2 profile retains compatible footing and walk
measurements with their original snapshot, frame and age; it remeasures jumps
and proposed-hull clearance on each request.

The host observes all active actors, advances the shared queue once, then runs
one ordinary physics step. Repeating `advance` for the same tick cannot double
the allowance. Missing actors, loss, leaving the ship, changed goals and episode
reset release work. A host must call `reset` on every new episode, including one
that happens to start at the same tick. Cloned jobs/worlds retain replayable
continuations without depending on allocation addresses.

## Work and storage

The default experiment supplies **16,384 graph operations and 1,024 physical
queries per update**, shared between eligible actors. Those are trial values,
not a calibrated whole-engine recommendation. One physical unit is a ground
ray, a capsule query, or a capsule test against the small proposed ship assembly.
Candidate inspections, map filtering and search operations are charged
separately. There are no whole-map clones or per-edge linear node lookups hidden
in a graph unit. Query internals, heap operations and buffer growth still have
variable costs.

Snapshot construction and dependency validation are measured separately from
dispatch, but remain synchronous. Validation conservatively scans relevant
geometry across the world collider sets; its own cost needs attention if the
world becomes much larger. Scheduling distributes work; it does not necessarily
reduce total work, especially when requests are repeatedly invalidated.

The runners retain at most two requests, with at most two owned query snapshots;
requests starting on the same tick share one snapshot. Each map has at most
512 nodes and 6,144 directed edges. Candidate maps are processed sequentially,
so the job retains its base and at most one candidate map/search workspace.
New snapshots are refused above 8,192 live bodies or colliders. Reports include
peak request/body/collider counts and deferrals. These are structural bounds,
**not a byte-level allocator cap**: Rapier arena capacity, shared shape storage
and allocator overhead are not measured by those counts. Reset/replacement
tests assert that the last snapshot reference is released.

## Reproduction

```sh
cargo +1.89.0 test --locked --release \
  -p engine-core -p engine-rapier -p scenario-spacewars -p spacewars-ai
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_flag_soak --example surface_mission_soak --features sensor-profile

target/release/examples/surface_flag_soak \
  --policy material_mission_v10 --seed 42 --seat 0 --offset -0.8 \
  --mode capture --jetpacks true --survey-landing true --landing-threat false \
  --edit none --expect complete --live-objective-planning true \
  --out /tmp/live-objective-flag

python3 tools/compare-mission-policies.py \
  --binary target/release/examples/surface_mission_soak \
  --baseline material_mission_v10 --candidate material_mission_v10 \
  --live-objective-planning --seconds 600 --seeds 9216675843324634618 \
  --out /tmp/live-versus-synchronous-v10
```

The comparison flag applies the live sensor to the **candidate role only** and
swaps that role between seats. It can also compare against `material_mission_v9`.
For both live bots, use the mission runner directly with both policies set to
v10 and `--live-objective-planning true`. The optional `--live-objective-seats`
accepts `both` (default), `0` or `1`. Both runners accept
`--objective-graph-budget` and `--objective-query-budget`.

`live-planning.csv` records every retained job's age, state and allocation under
the one global allowance. Idle dispatches have no job rows. The JSON report
records all/active dispatch timing, snapshot/validation timing, completion age,
cancellations and structural storage counts. Snapshot setup and validation are
inside normal sensor timing; dispatch is separate and is added to
`measured_tick` when draw-list construction is measured. CSV/JSON writing is
excluded from those timings. These are headless measurements, not rendered FPS.
Instrumented synchronous sensor counters exclude dispatched live work; its
graph/query counts are in the separate live CSV and telemetry.

## Validation checkpoint, 2026-09-13

The reference is merged main `db09b3a12e2f1f6920376f060dc199b22517ab5a`
([#85](https://github.com/aortez/space-wars/pull/85)). Exact binaries, commands,
reports and allocation traces are retained under
`/home/oldman/.codex/visualizations/2026/09/13/live-bot-surveys/final/`.
`validation.json`, `live-validation.json`, `positive-replay.json` and the
comparison manifest distinguish reference, candidate and Pi runs. Mission
runner SHA-256 identities are:

- Reference: `d4952d5f2cf59bf87566b08d67938688e594c14e4af6c45bb3bdfbccab2d5348`.
- Candidate: `0f4966057c4dd5da285925a2462b9ac02ef385649aee18afa2414488a26a0f4b`.
- Pi flag runner: `d725d9a0a2d3b7ba1102f1abe210badfd5d018a4d20a3eb3dda5bfc8d284bde5`.

**Correctness and compatibility:**

- All 671 relevant engine-core, engine-rapier, scenario and AI tests pass.
  New cases compare complete incremental maps, candidate forecasts and route
  diagnostics against the synchronous implementations at several quotas,
  including one-query dispatches. Real edits, entering/leaving hulls,
  collider replacement, hatch movement, changed goals, loss, reset and cloned
  continuation exercise freshness and ownership of retained snapshots.
- Both jobs complete while a two-seat sensor fixture advances real physics,
  under one shared allowance. That test injects target observations only;
  the flag fixtures below establish ownership through actual controls.
- With live planning disabled, two paired generated matches retain **84,926
  dense player records** byte-for-byte against the reference, including all
  non-timing report fields, CSV columns and sensor counters. Paired v9/v10
  contested-flag fixtures also preserve complete non-timing reports.
- Six live controlled captures cover both seats at query quotas 512, 1024
  and 2048. All capture, board, depart and pass physical audits. Maximum ready
  ages are 55, 36 and 24 ticks respectively. Repeating the default seat-0 run
  preserves its complete non-timing report and all 1,834 allocation rows,
  including 53 completed forecasts.
- Formatting, whitespace, the client all-targets build check and Clippy pass;
  broader Clippy output retains warnings in unchanged code. The changed
  mission-runner target is warning-free.

**Live matches and comparisons:**

- Generated seed `9216675843324634618`, both live v10 bots, no asteroids:
  the full 36,000-tick match completes with physical audits passing. There
  are 932 published completions and 7,973 dispatches with work for both bots.
  Every recorded aggregate/per-actor allocation obeys the one shared quota.
  There are 706 gravity and three obstacle invalidations. Invalidation counts
  include already-published results, so they are not disjoint from completions.
- Generated seed `7725194555774358125`, both live v10 bots, mixed asteroids
  every three seconds: pilot death ends the match at tick 18,618. All 41
  requests are invalidated by obstacle movement before publication. Its
  repeated run preserves dense player records, non-timing reports and every
  allocation. This validates cancellation under pressure, not successful
  planning through that pressure.
- A three-minute, seat-swapped comparison of synchronous v10 against live v10
  uses the first seed and identical initial worlds. One seat configuration
  publishes 250 forecasts and reaches the observation cutoff; the other ends
  at tick 6,463 before the live role requests an enemy-flag survey. These
  runs exercise the comparison plumbing; they do not establish a strength
  improvement or an equal-budget contest.

Across these recorded live runs, peak snapshots contain 45 bodies and 159
colliders, with at most two retained requests. Byte-level retained memory and
large-world Pi snapshot/validation cost remain unmeasured.

**Pi calibration:**

`sw-picade` is a Pi 4B, running at the sampled 1.5 GHz with the `ondemand`
governor; temperature samples were 74.0–75.5 C. Autoplay was paused for these
four sequential seed-42, seat-0 flag runs and resumed afterward. All four
complete capture and departure with audits passing. Graph allowance stays
16,384 while query allowance varies:

| Sensor profile / query allowance | Active dispatch mean | p95 | max | Oldest completed forecast |
| --- | ---: | ---: | ---: | ---: |
| Live / 512 | 0.63 ms | 1.20 ms | 2.44 ms | 55 ticks |
| Live / 1024 | 1.16 ms | 1.60 ms | 2.81 ms | 36 ticks |
| Live / 2048 | 2.15 ms | 3.07 ms | 3.15 ms | 24 ticks |

At 1024 queries, snapshot construction peaks at 0.030 ms and validation at
0.236 ms in this small fixture. The synchronous comparison's largest sensor
call is 37.92 ms; the live runs still reach 23.70–26.03 ms in other synchronous
sensors. Paths and capture times differ, including between desktop and Pi,
so these are workload observations rather than matched-trajectory speedups.
Dispatch maxima also do not isolate the largest indivisible query. The quota
controls this planning slice; it does not certify a 16.67 ms total tick.

## Integration with the subsequent ship changes

The branch was then rebased onto `183c2979c426f2a07d15b5316a341a95d9e95b30`,
including [#86](https://github.com/aortez/space-wars/pull/86) and
[#88](https://github.com/aortez/space-wars/pull/88). Those changes include missile
launch positions, so the preceding battle outcomes and Pi measurements remain
labelled with their original base. A fresh reference executable was built from
the new main in a separate checkout/target directory. The integration artifacts
are in the sibling `live-bot-surveys/integrated/` directory:

- All **694** relevant release tests pass, as do the eight live-adapter tests
  in debug mode with CI's 16 MiB test-thread stack. Client all-targets checking,
  formatting and Clippy complete without new warnings.
- The disabled profile retains **121,812** dense player records exactly against
  this new reference across the same two generated match configurations. Both
  paired contested-flag fixtures retain their non-timing reports too.
- All six live controlled captures pass again. The updated quiet two-live-bot
  match runs the full ten minutes, publishes 724 completed forecasts, and has
  6,517 dispatches doing work for both actors. All allocation and physical
  audits pass. There are 724 gravity, 49 obstacle and four objective
  invalidations, including invalidations of already-published results.
- The updated asteroid match ends at tick 26,955; all 18 requests are invalidated
  before publication. Its repeated run checks deterministic controls, reports
  and allocations. The same limitation under moving obstacles remains.
- Peak observed snapshot sizes rise to 69 bodies and 178 colliders. The default
  two-request capacity is never exceeded.

Integrated mission runner hashes:

- Reference: `e4a062b6cba758270c4d0006354a1b7982d67f700de1a2c6eb432aa3be94e3ed`.
- Candidate: `5c1bf3c9d7e781f5aa7a9551299c0af46b13ac69af75529c4a305e2c7e4f0458`.

## Next boundary

The [ground-reuse follow-up](bot-ground-reuse.md) addresses gravity and our own
hull changes. Moving-obstacle dependencies remain conservative: narrowing them
must account for every affected footing and crossing edge. Do not loosen
freshness checks merely to make a completion counter look better. Reduce wasted
requests before promoting the live profile to cabinet defaults or extending it
to more sensors. Mission-level utility and strategic lookahead remain the
following phase.
