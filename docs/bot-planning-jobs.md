# Resumable v10 graph jobs

Follow-up: [live landing-objective surveys](live-bot-surveys.md) connect this
scheduler to coherent physical measurements in opt-in test-runner profiles.
The checkpoint and synchronous/default behavior described below remain available.

This continues [#81](https://github.com/aortez/space-wars/issues/81) after the
selectable v9/v10 comparison in [#82](https://github.com/aortez/space-wars/pull/82).
It supplies the first actual resumable job and a shared scheduler. It preserves
the v10 sortie decision; it does not introduce another policy or change aim,
combat breaks, controls, sensors or launcher defaults.

**Live v10 still finishes searches synchronously.** Both policy descriptors keep
`planning_work_quota: null`. The queue and offline probe establish the mechanics
needed for a later live adapter. Terrain measurements still happen in one
observation, and there is no new cross-update collision cache or Pi frame-time
claim.

## Shared scheduling contract

`engine_core::planning` contains no Spacewars rules. A job advertises its next
operation as graph work or a physical query, advances one operation, and exposes
an output when complete. `PlanningQueue` retains one current job per arbitrary
actor identity, under one global allowance and configurable per-actor caps and
weights. More actors share that allowance; they do not multiply it.

Dispatch uses weighted round-robin in ascending actor order. An unfinished turn
carries across updates, including when the global allowance is smaller than a
weight. A per-actor cap or unavailable resource forfeits the rest of the turn;
unused work goes to the next runnable actor during the same update. Unused
global allowance expires. Changing a weight forfeits an unfinished turn, and
replacing a request with the same weight does not refill its turn.

Each report records the supplied allowance, charged graph/query work, per-actor
limits and charges, logical job age and pending/ready state. Queue capacity
bounds retained jobs, including completed workspaces. Cancellation, replacement
and reset release the old job and its snapshot. Capacity counts jobs, not bytes;
the adapter must also bound the size of its input graphs and retained snapshots.

Tokens are scoped to their queue. Replacement, cancellation and reset invalidate
old tokens. Polling with changed dependencies invalidates and drops pending or
ready work. Pending and stale are distinct from a completed negative answer:
the graph result can be ready with `NoStartFooting`, `NoDestinationFooting` or
`Disconnected`. The last means no complete measured walk/jump trip; it says
nothing about an unmeasured cave, excavation or jetpack route.

The host owns the complete dependency key and must poll/cancel affected jobs
when measurements or requests change. A terrain revision alone cannot certify
moving ships, debris, gravity, actor mobility or a proposed landing pose. The
queue does not inspect the world or silently infer freshness. Immediate safety
and controls remain outside the planning allowance and must revalidate physical
permissions before acting.

## The Spacewars job

`GroundRoundTripJob` holds one immutable `Arc<GroundMap>` for queued use. The
existing synchronous `GroundRoutes` wrapper borrows its map and drains the same
state machine, avoiding a map clone on every old call. No queued job reads live
physics or combines measurements from different updates.

Resumption covers node diagnostics, outgoing/incoming indexes, forward and
reverse Dijkstra searches, endpoint selection, and both route reconstructions.
One unit inspects one node/edge/index slot, pops one heap entry, handles one path
element, or performs a constant-size transition. Edge scans are separate from
node expansion; a high-degree vertex cannot hide its whole adjacency scan in a
single unit. Obsolete heap pops are charged too. Reports expose total operations,
indexed/scanned edges, pops, expanded nodes and peak frontier.

Directed edge order, cost arithmetic, node-ID ties, failure diagnostics and
route lengths retain v10 semantics: minimize both legs' edge lengths plus two
units per jump, excluding prospective jetpack edges. An independent, frozen v10
solver is compiled only for tests and checks complete results, not just cost.

This is deterministic operation accounting, not a millisecond deadline. Initial
fixed arrays and buffer reservations are outside dispatch; heap operations,
allocator/buffer growth and scheduler/report overhead still cost time. An
immutable map and resumable frontier provide reuse within one job, not a cache
certifying a plan across changing world states. The physical-query allowance is
tested with synthetic jobs; a real resumable survey is not implemented here.

## Reproduction and diagnostic scope

```sh
cargo +1.89.0 test --locked --release \
  -p engine-core -p scenario-spacewars -p spacewars-ai
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --example surface_flag_soak --features sensor-profile

target/release/examples/surface_mission_soak \
  --world generated --seed 9216675843324634618 --mode duel --match true \
  --seconds 180 --p1-policy material_mission_v9 --p2-policy material_mission_v10 \
  --probe-planning-budget true --out /tmp/planning-budget-probe
```

The optional probe copies at most six nonempty maps already present in normal
observations, spaced at least two simulated seconds apart per seat. It adds no
physical survey or control action. Copies happen outside sensor/policy timing
scopes, though their CPU-cache effects cannot be excluded. After the simulation,
one, two and six diagnostic identities run frozen graph jobs at global quotas
16, 64, 256 and 1024. Start/hatch use the first measured footing and target the
middle footing at range four: these are representative graph requests, not
claims that extra bots executed those sorties.

`planning-budget.json` records snapshot identities/sizes, setup and dispatch
timings, total work, per-job completion ages, answers and synchronous equality.
`planning-budget.csv` records every dispatch allocation. Empty observations
produce an explicit `no_measured_ground_maps` report. The usual match report and
policy identities remain unchanged. Offline completion time divided by 60 is a
hypothetical latency at one dispatch per simulation tick, not observed live bot
latency or an equal-budget strength comparison.

## Validation checkpoint, 2026-09-13

The reference is merged main `e2d07ba09e6e1d80c2141a6988e24c51c8604f9b`.
The instrumented mission runner hashes are:

- Reference: `a442ee3173efbc1f1cc133318dbcc836fa700ef86b650fd37483e93f19266f4c`.
- Candidate: `d4952d5f2cf59bf87566b08d67938688e594c14e4af6c45bb3bdfbccab2d5348`.

Local commands, binary copies, reports and allocation traces are retained under
`/home/oldman/.codex/visualizations/2026/09/13/bot-planning-jobs/`;
`validation.json` records exact commands and binary identities, and `validate.py`
checks every non-timing output against the reference.

- All **578** engine-core, scenario and AI tests pass. Scheduler cases cover
  zero/exhausted quotas, independent resource allowances, weights/caps,
  insertion-independent fairness across six actors, replacement, cancellation,
  reset, stale pending/ready results, capacity and snapshot release.
- Sparse directed graphs match the frozen v10 solver and exhaustive endpoint
  checks. Three simultaneous 512-node high-degree graph jobs also match it at
  global quotas **1, 7, 64 and 4096**. Per-step assertions prevent a node from
  hiding many edge inspections; cloning a partly completed frontier retains
  exactly the same result and work. Changed terrain/obstacle dependency tests
  revoke both pending and completed results.
- Two paired generated matches preserve **84,926** dense player records
  byte-for-byte, all **41** non-timing report fields, **12** non-timing CSV
  columns, and every observation's physical/graph counters. Seed
  `9216675843324634618` uses v9 in P1 and v10 in P2 with no asteroids; it finishes
  at tick 6463. Seed `7725194555774358125` swaps the policies and adds a mixed
  asteroid every three seconds; it runs the full **36,000 ticks** to match expiry.
  Both use a 600-second limit and pass physical audits. These are identity
  checks against the old executable, not evidence of a new strength advantage.
- Paired contested-flag fixtures use seed 42, offset -0.8, real surveyed landing,
  capture, boarding and departure: v9 in seat 0 and v10 in seat 1. Both complete,
  pass audits and match all **35** non-timing report fields and objective surveys.
  This does not resolve the previously documented crater incompletes.
- Clippy completes with existing warnings in unchanged files; formatting and
  whitespace checks pass.

The synchronous path shows no material regression in these ordered desktop
pairs. Total profiled joint-search time for the quiet replay is **2.987 → 3.010
ms across 100 calls**; the asteroid replay is **3.074 → 2.732 ms across 158
calls**. Mean objective observation time is **3.567 → 3.562 ms** and **3.279 →
3.253 ms**, respectively. Run order reverses between seeds. These small samples
do not establish a speedup, and are not Pi timings or rendering measurements.

The offline probe retains six maps of 505–507 nodes and 1008–1012 edges. Each
query consumes **8209–8243** graph operations, independent of quota. All 12
job-count/quota combinations finish with the same answers and no allowance
overrun. The slowest job in each configuration finishes at:

| Shared graph units per dispatch | 1 job: ticks | 2 jobs: ticks | 6 jobs: ticks | 6 jobs: seconds at 60 dispatches/s |
| ---: | ---: | ---: | ---: | ---: |
| 16 | 516 | 1031 | 3085 | 51.42 |
| 64 | 129 | 258 | 772 | 12.87 |
| 256 | 33 | 65 | 193 | 3.22 |
| 1024 | 9 | 17 | 49 | 0.82 |

Dispatch maxima are below **0.015 ms** in this warm desktop sweep; job setup is
below **0.0011 ms**, excluding snapshot copies. The low quotas deliberately
demonstrate delayed completion. They are not suggested live settings: the graph
work is cheap compared with physical surveys, and several seconds of pending
planning would often be unacceptable. Calibrate useful quotas and stale-job
rates in the live adapter instead of choosing a small number for its own sake.

## Next integration boundary

Before enabling a live work quota, separate cheap immediate observations from
expensive survey requests. Give the physical survey a coherent measurement and
invalidation contract; only then schedule graph jobs from those measurements.
The bot must keep executing its valid committed action while a request is
pending, and use the existing hold/lift/recovery behavior when it becomes unsafe.
An unfinished query must never masquerade as a failed route or grant permission
to land, jump, exit, board, claim or rebuild.

That adapter needs physical tests for destruction, detachment, moving hulls and
debris, death/reset and changed goals during a pending request. Record separate
safety/control costs and retained memory, calibrate quotas on the Pi, then make
the budgeted configuration explicit in the existing comparison harness. Keep
v9 and this synchronous v10 checkpoint available when comparing changed timing
and behavior. Broader mission strategy follows that integration.
