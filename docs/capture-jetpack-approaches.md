# Existing jetpack forecasts for blocked approaches

The [topology study](capture-approach-topology.md) found that the proposed parked
ship disconnects all 37 sheltered walk/jump routes in the recorded failed
approach. The v11 landing model already forecasts a powered crossing of that
ship, while v13 uses the walk/jump model. This study tests the existing powered
model on the same observations before integrating it into a mission policy.

## Diagnostic contract

`--probe-cover-jetpack true` extends the fixed-clock cover probe and requires
`--probe-cover-ticks`. It measures `JetpackRoundTrip` in native batches of at
most eight sites, with the real proposed hull present. Paired single-site jobs
run the existing walk/jump and jetpack models against one immutable snapshot.
They record native routes/costs, graph/query work, finished-candidate counts and
the existing flight forecast's rejection reason. These repeated single-site
measurements attribute work and rejections; they are not a proposed live scan.

Each paired walking route must match the old native measurement, and each
paired powered route must match its separate native batch. Successful walking
routes must remain identical and must not start a flight forecast. Missing
equipment must preserve the walking result. If no powered forecast starts,
that remains distinct from a measured flight rejection. A null diagnostic is
unknown, not zero available routes.

This probe isolates prospective airborne sites. A landed observation is
explicitly unsupported so an actual-pose route cannot contaminate a site's
work or rejection counts. Both native models retain the ship, terrain, other
obstacles, equipment, physics, launch charge, reserve, arrival window and
moving-frame checks. There is no controller, forecast threshold or policy change.

Additional work runs on a cloned world after the main control decision, with
separate profiling. Results never enter controls or sensor demand. Its work
counts describe completed offline jobs and do not show that a request can be
delivered within live allowances or before its evidence expires. Snapshot
construction and other synchronous work remain outside dispatch counts.

## Frozen replay plan

Freeze the probe, tests, analysis and this plan before new measurements. Replay
the complete `target/capture-topology/v3` study, preserving every command except
the new binary, output path and diagnostic option:

| Source | Cover response | World ticks |
| --- | --- | --- |
| Directed failure, flag -0.8 | On | 3,270; 3,930; 8,580; 9,030 |
| Directed failure, flag -0.8 | Off | 9,270; 9,720 |
| Successful directed control, flag +0.8 | On | 3,510 |

These are three correlated, quiet-mode replays and seven existing observations,
all seat 0 and seed 42. Keep unavailable snapshots and every rejected forecast.
Require exact physical/mission/progress reports and controller/planner streams,
unchanged ordinary sensor counters and shared allocations, and exact original
cover/topology diagnostics apart from timings. Bind each new observation and
mission record to its trace row. Validate crossing endpoint/revision identity,
proposed hull position, both flights, fuel reserve, launch window and native
costs, independently of the number of successful routes.

Report added covered round trips, whether they occur in the existing shortlist,
each rejection category and paired work. Do not equate a route forecast with a
physical capture. If the model supplies usable sheltered routes, the next step
is controlled physical execution and a separately declared mission integration
experiment. If it rejects them, diagnose that rejection before changing model
thresholds or defaults. Preserve all raw hashes and any failed audit.

```sh
python3 tools/probe-capture-jetpack.py \
  --study target/capture-topology/v3 \
  --binary target/capture-jetpack/surface_mission_soak-COMMIT \
  --out target/capture-jetpack/v1
```
