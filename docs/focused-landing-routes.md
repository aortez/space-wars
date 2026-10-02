# Focused landing routes within the shared allowance

The [powered mission study](powered-mission-integration.md) found that a full
ground survey cannot finish within 120 ticks at four graph steps per dispatch.
This experiment measures a small walking corridor before the full survey.
It keeps the shared 4 graph / 384 physics query allowance, source lifetime,
current landing checks, physical hull and route dependency validation.

`--focused-objective-routes true` is opt-in and requires route dependencies
and early route delivery. The report uses `live_joint_objective_v7` or
`live_jetpack_objective_v7`. Native sensors and default planner configurations
keep their existing behavior.

## Measurement contract

Each request tries one candidate from the existing eight-site shortlist, or
the actual landed pose when present. A small arc joins its hatch and the flag,
with two ground-sample margins at each end. At most 17 of the ordinary 512
sample positions are measured. Longer approaches skip this extra pass. New
requests rotate through the shortlist for the same objective, so an unavailable
first candidate does not monopolize every retry. Reset/removal clears this
selection state; changing objectives starts at the first candidate.

The patch uses existing footing queries, directed adjacent walking checks,
hypothetical ship geometry, round-trip search and path dependency construction.
It uses the existing sparse index for small graphs. The hull overlay queries
each path sample directly, consuming the query allowance on every call. This
performs additional physics queries where the ordinary overlay would first
spend a graph step on a distance test; it does not batch queries or relabel
uncharged work. Tests count actual preview calls against charged queries.

Only positive patch routes enter early publication. Missing footing or a
disconnected patch leaves other walks, jumps and powered routes unknown. The
complete original survey runs afterward, against its original snapshot and
clock, and can replace the early answer. Its completed result must still match
the native full survey. Patch attempts, completions and successes are counted
separately from complete candidate/survey measurements. Source expiry and
retired-work accounting remain unchanged.

This first pass measures walking routes even when powered planning is enabled.
It does not simplify the existing flight predictor or claim that its forecast
can finish under this allowance. Synchronous observations, snapshot creation,
publication validation and bounded setup remain outside dispatch counters.
Operation quotas do not measure total frame time.

## Frozen replay plan

Freeze implementation, tests, runner and this plan before collecting mission
outcomes. Use the exact shared cases from `target/powered-mission/v3/summary.json`:

1. Replay all six directed missions with focused routes disabled. Require the
   original physical/mission/visit report fields, seven controller/evidence
   streams per run, ordinary sensor counters and shared allocations to match.
2. Enable focused routes in all six directed missions: blocked approach with
   cover on/off and the walkable control, each with walking/powered v13. Keep
   the 180-second horizon, seed 42, real mission state, cover/retry settings,
   flag-cost admission and current-neutral sensing.
3. Enable focused routes in all eight shared armed matches, regardless of the
   directed outcomes. Keep both generated worlds, both v13 seats versus v10,
   the same 600-second rules, weapons and two active planners. Compare against
   the immutable original shared runs. Preserve every loss and unfinished visit.

This is six retention replays and 14 new runs. All configuration except the
focused-route switch, executable and output location remains the same. Two
independent runs may execute concurrently; desktop timing is not a Pi benchmark.
Keep the earlier games and all raw hashes. Require per-tick conservation, the
combined allowance, original measurement age and current publication validation.
Audit positive partial routes, first delivery clocks, matching capture-site
telemetry, launches, claims, boarding and departure using the existing auditor.
Record the first action difference in each comparison. Route delivery,
controller selection, physical success and winning remain separate outcomes.

Do not increase quotas, extend lifetimes or change defaults based on these
results. If the focused pass delivers a route but the controller still cannot
capture, preserve that distinction and diagnose the observed handoff.

```sh
python3 tools/validate-focused-routes.py \
  --prior target/powered-mission/v3/summary.json \
  --binary target/focused-routes/surface_mission_soak-COMMIT \
  --out target/focused-routes/v1
```
