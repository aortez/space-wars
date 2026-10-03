# Local powered-route evidence within the source lifetime

## Implementation and frozen comparison plan

Add `--powered-objective-routes true` on top of walking feedback/bounds. Live
profiles are `live_joint_objective_v12` and `live_jetpack_objective_v12`. The pass
acts only on jetpack planning for a currently selected site or the actual hatch.
Walking policies, disabled behavior and all bot defaults remain unchanged.

The selected pass tries a direct walking corridor first. It keeps all existing
floor, capsule and proposed-hull checks in both directions, committing fixed
node/edge bookkeeping at the final charged query. A failed walk then measures
57 footing samples around the proposed hull, covering the ordinary proposal's
angular window. Each operation performs exactly one ray, capsule or hull query.
The ordinary proposal criteria select two real measured nodes from this window.

Two walking segments must connect the hatch to the first crossing node and the
second crossing node to the flag. Each segment is checked in both directions;
flight endpoints require the exact sampled node and position, without a
nearest-node snap. The combined walking segments retain a 224-step cap including
their margins. Omitted arcs, jumps and alternative crossings remain unknown.
Only after both walking segments succeed does the flight forecast begin.

The opt-in forecast commits one integration at launch and after each successful
charged hull-clearance query. It retains both world and hull queries at every
predicted step, with unchanged sizes, fuel reserve, arrival checks and time
limit. Launch environments advance one charged tick at a time, retaining the
same f32 recurrence, and are reused across directions and successive epochs.
Both directions and all three moving-world launch epochs remain mandatory.

Walking failure notices wait until this powered pass finishes unsuccessfully.
No failed or unfinished pass publishes a negative route verdict. A positive
route includes both walking footprints, the local node measurements, flight
region and boarding checks, and passes the existing live geometry, gravity,
equipment, kinematic and age validation before publication. A full native
fallback remains after the local pass. In this mode, powered jobs (including the
full fallback) take a fresh snapshot when they would otherwise reuse older
geometry: geometry and flight environment must share their original source
epoch, never a relabeled clock.

The shared allowance stays at 4 graph / 384 physics queries, with a 120-tick
source lifetime. Cover search keeps eight probes and its original 600-tick
deadline. Snapshot construction and immediate/publication validation still have
costs outside operation quotas; dispatch counters do not establish Pi speedup.

Freeze implementation, tests, this plan and the runner before collecting match
outcomes. Compare to `target/walk-bounds/v1/summary.json`:

1. Six disabled directed replays require exact prior physical/mission fields,
   seven evidence/controller streams, ordinary sensor rows, allocation ledgers
   and non-timing planner telemetry.
2. Run all six enabled 180-second directed missions: seed 42, seat 0, both route
   models, blocked bearing -0.8 with cover on/off and +0.8 control with cover on.
3. Run all eight enabled 600-second armed matches: both fixed generated worlds,
   both v13 seats against v10, both route models, weapons, two planners and no
   asteroids. Require exact replay retention for all seven walking-policy runs.

Run at most two games concurrently. Change only the new flag, binary and output
directory. Preserve all losses, unavailable routes and unfinished visits. Reuse
the existing physical/publication auditor and walking-notice auditor unchanged;
add a strict check that each powered publication's flight and geometry share
the same measurement tick and original 120-tick launch window. Record route
delivery, search advancement, actual launches, claims and original-ship boarding
and departures separately. Verify prior hashes before/after, keep raw streams,
and archive first powered deliveries and first changed actions.

```sh
python3 tools/validate-powered-routes.py \
  --prior target/walk-bounds/v1/summary.json \
  --binary target/powered-routes/surface_mission_soak-COMMIT \
  --out target/powered-routes/v1
```
