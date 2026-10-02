# Longer requested walking corridors

The [requested-route study](requested-landing-routes.md) resolves the directed
control's cover wait, but the blocked case requests a site about 182 samples
from its flag, beyond the 96-edge limit. This experiment extends the measured
walking corridor while preserving the shared 4 graph / 384 query allowance
and the 120-tick source lifetime. It remains opt-in.

`--extended-objective-routes true` requires requested corridors and their
existing prerequisites. Reports use `live_joint_objective_v9` or
`live_jetpack_objective_v9`. Default planning and prior opt-in modes retain
their original behavior.

## Measurement and accounting

Keep the original dispatch sequence for all corridors inside the 96-edge
bound. For a longer selected-site or actual-hatch corridor, permit up to 224
edges including the existing endpoint margins. The rounded endpoint separation
must therefore be at most 220 samples. Still test only the shorter surface arc;
missing support, obstructed capsules/hulls and failed boarding stay unknown.
The original full survey remains the fallback, with the same snapshot and age.

The longer pass commits an edge's result when its final return support query
succeeds. This removes the separate graph operation that previously only
copied the completed edge's length, query footprint and endpoint. It retains
the node inspection, both directions' nine terrain capsule checks, nine hull
checks and three support rays. Every advertised physics operation still makes
exactly one actual query. The result update appends at most three fixed query
areas; it performs no variable-length scan, extra query, graph search or child
job. This follows the existing ground survey's pattern
of committing an edge after its final physical query.

Two maximum corridors require at most 462 graph operations including their
start windows and parent handoffs, within 120 dispatches at four operations.
This is an operation bound, not a frame-time guarantee. Unit checks compare the
old and new edge handoffs on the same long hypotheses: identical paths, results,
query footprints and query counts, with one fewer graph operation per completed
edge. They also check near-limit two-actor delivery, both directions, blocked
hulls, unchanged short-route timing, actual return, full-survey parity, warm
snapshot age, zero allowance, expiry and cancellation.

Query geometry, walking rise, dependency validation, landing/boarding permissions
and the source clock are unchanged. The new pass adds no jumping or flight
forecast. Count extended starts/completions/successes as a subset of corridor
work, including retired jobs. Snapshot preparation, fixed setup, publication
validation and ordinary immediate/on-foot sensors remain outside dispatch.

## Frozen physical trial plan

Freeze implementation, tests, runner and this plan before collecting outcomes.
Use every shared case from `target/requested-routes/v1/summary.json`:

1. Replay the six directed missions with extension disabled. Require the prior
   physical/mission fields, seven controller/evidence streams per game, ordinary
   sensor rows, allocation ledgers and all non-timing planner counters to match.
2. Enable the extension in all six 180-second directed missions, keeping both
   route models, blocked bearing -0.8 with cover on/off, +0.8 control with cover
   on, seed 42 and seat 0.
3. Enable it in all eight 600-second armed matches, regardless of directed
   results. Retain both generated worlds, both v13 seats versus v10, both route
   models, weapons, two active planners and no asteroids.

This is six retention replays and 14 new runs. Change only the extension option,
binary and output path. Preserve every loss, unknown route and unfinished visit.
Do not change the allowance, lifetime, cover deadline, probe count, equipment
checks or defaults in response to outcomes. Use two concurrent runs at most;
desktop timings do not establish Pi performance.

Use the existing auditor for physics, source clocks, current validation,
per-tick combined quotas, launches, claims, original-ship boarding and departure.
Keep first deliveries, selected-site observations and cover-state transitions,
and record the first action difference from the prior corpus. Route delivery
and physical success remain separate. Verify all original hashes before and
after the experiment; keep the frozen inputs and all new raw streams.

```sh
python3 tools/validate-extended-routes.py \
  --prior target/requested-routes/v1/summary.json \
  --binary target/extended-routes/surface_mission_soak-COMMIT \
  --out target/extended-routes/v1
```
