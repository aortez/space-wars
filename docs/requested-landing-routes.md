# Requested landing corridors

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
