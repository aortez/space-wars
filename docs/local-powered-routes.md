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

## Frozen results

Keep this opt-in. Powered evidence now reaches both site selection and the actual
hatch before expiry, and one armed run launches a jetpack flight. That flight is
interrupted by a later unavailable forecast and does not complete its flag
visit. The scheduling change also delivers walking evidence sooner in the
powered policy, changing several physical outcomes. Defaults remain unchanged.

Implementation and plan were frozen at `d2e3f69` before collecting outcomes.
All 20 runs pass the existing physical, publication, work-allocation and walking
notice audits. The six disabled directed replays retain exact prior streams;
all seven enabled walking-policy runs also retain exact prior streams. Eight of
the fourteen enabled runs keep identical action sequences. All six directed
runs retain their completed-sortie counts: one neutral sortie in each blocked
case and two sorties in each successful control.

The four powered-policy armed comparisons are below. Counts belong to the
evaluated seat and require physical claim, original-ship boarding and departure.

| World / seat | Completed sorties, before → after | Outcome, before → after | Powered requests published | Jetpack launches |
| --- | --- | --- | --- | --- |
| 0 / P1 | 4 → 5 | Loss → win at time limit | 0 | 0 |
| 0 / P2 | 5 → 3 | Loss → win after opponent death | 0 | 0 |
| 1 / P1 | 1 → 2 | Loss → loss | 7 | 1 |
| 1 / P2 | 3 → 3 | Loss → loss at time limit | 0 | 0 |

The four walking-policy armed comparisons remain losses, giving two wins out of
eight fixed armed comparisons versus zero before. The two wins involve no
powered forecasts or launches. Their first changed actions follow earlier
walking-route publication: an actual-hatch route at tick 9472 in world 0/P1,
and a proposed walking route delivered to the opponent at tick 8246 in world
0/P2, which shares the work allowance. These are scheduling observations in a
small correlated corpus, not evidence that powered flight caused the wins.
World 0/P2 also completes fewer sorties, and the match ends earlier.

## Powered delivery and the remaining flight failure

In world 1/P1, bearing 34 becomes usable during the original cover search.
The first positive arrives at tick 6414 from source tick 6337, age 77. The prior
run has already exhausted its walking candidates and is still waiting. Six
requests publish this proposed crossing; a seventh validates the actual hatch:

| Source tick | First publication | Age | Route |
| --- | --- | --- | --- |
| 6337 | 6414 | 77 | Proposed site 34 |
| 7305 | 7382 | 77 | Proposed site 34 |
| 7426 | 7503 | 77 | Proposed site 34 |
| 7547 | 7623 | 76 | Proposed site 34 |
| 7668 | 7744 | 76 | Proposed site 34 |
| 7789 | 7865 | 76 | Proposed site 34 |
| 8197 | 8272 | 75 | Actual hatch |

All seven retain matching geometry/flight source clocks and the original
120-tick launch window. Repeated validation produces 268 powered route entries;
these are seven requests for one visit, not 268 independent successes.

The pilot exits at tick 8273 and launches at 8282 with full charge, using the
ordinary current forecast measured at 8280. Subsequent ordinary surveys at
8310, 8340, 8370 and 8400 still provide forecasts. The pilot enters the crossing
phase at 8412. At 8430 a completed survey contains neither a vehicle forecast
nor its crossing. `GroundNavigationTask::follow_crossing` consequently
interrupts the maneuver and switches to settling; charge remains about 0.778.
No completed crossing is recorded. The ground task gives up at 8821 with
`no complete flag round trip in fresh surveys`, and the visit is abandoned at
8822 without a claim or completed sortie. Later recovery boards the original
ship at 8973. A different subsequent visit supplies the extra completed sortie.

The ordinary on-foot sensor does not report why that forecast became
unavailable. The next investigation is to preserve its rejection reason and
the relevant physics at tick 8430, then determine whether the interruption
reflects a real clearance/flight failure or an inappropriate new-launch check
during an already active flight. Existing validation must remain enforced.

Across the corpus, 218 local passes start and 207 finish: 183 direct walks,
seven powered routes and seventeen unsuccessful passes. Failures comprise
five missing crossing-to-flag walks, six walking spans outside the cap, and six
flight fuel-reserve failures. Of seventeen flight forecasts started, seven
approve, six reject and four remain unfinished in the recorded runs. No
unsuccessful or unfinished pass grants a route.

## Validation and retained evidence

The final source passes 1,013 Rust tests and 655 Python tests. Differential
forecast tests preserve the original query sequence, physics samples, results,
rejections and clocks across sixteen fixture combinations. With two jobs sharing
the allowance, complete local powered routes arrive at age 36 in both parked
fixtures and ages 113/108 in moving fixtures. Live tests also advance real
physics through publication and verify expiry, missing boarding support, exact
walk/flight endpoints and fresh snapshots after cached geometry.

Formatting, strict AI-crate Clippy with `--no-deps`, and profiled/ordinary release
builds pass. Strict scenario Clippy still reports seven existing findings in
unchanged code. The trial audits cover 477,498 pilot rows and 271,149 dispatch
ticks; maximum charged work remains 4 graph / 384 queries, with maximum
publication age 120. Total live work increases by 28,285 graph operations and
1,082,490 physics queries across the changed match trajectories and durations.
This is not a performance or Raspberry Pi speedup claim.

The [result manifest](data/local-powered-routes-v1.json) records every outcome,
comparison and failure. The [compressed evidence archive](data/local-powered-routes-v1.json.gz)
contains 48 exact documents: both frozen summaries, raw hashes, physical and
route witnesses, first changed actions, the flight interruption, and validation
logs. Its entries were rehashed against their original files. All 261 raw files
remain under `target/powered-routes/v1`; the prior corpus was verified before
and after the trials.

- Binary SHA-256: `8285b5248883cfc978a3ae5f2190fa1f70a222a30ec0ac8443570746e15d9759`
- Summary SHA-256: `6cb04c9e216013b6eb9761b4810c13b6501225e49bc15e0412134c8a89089e69`
- Evidence SHA-256: `304ff46572b52bd1ffe65e9bd2e7c7b60f9371cb21bf7874d9230ed8046710b1`
