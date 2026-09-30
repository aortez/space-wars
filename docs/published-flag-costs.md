# Published flag costs: experimental v13 evidence

Issue #142 follows [the previous promotion study](capture-value-promotion.md).
That study retained the defaults: v13's three-planet owned-base trials never had
two numeric destination costs. This experiment changes evidence availability,
not priority weights, destination hysteresis or native control.

## Diagnosis and candidate

The previous `value-destination-p1-bearing0.8-v13` selected enemy planet 1 at
tick 1. Neutral planet 0 had measured remote landing/hatch/climb costs, but the
current enemy had no remote surface cost. The neutral survey requested only
alternatives; the flag survey also excluded the current target. At tick 553 the
controller interrupted travel for a nearby opponent. It later claimed the neutral
at 4089 and the enemy at 8003. The early missing comparison was not evidence that
different priority weights were needed.

`--admit-flag-costs none|0|1|both` enables `capture_value_published_flags_v1`
for selected v13 seats in the headless harness. The predecessor is
`capture_mission_value_v1`. Default is `none`; interactive policies do not enable
it. Each enabled seat is identified in `policy_configuration` and evaluator
reports. Evaluation and shared flag surveys must also be enabled.

The selected seat can request its current enemy destination as well as an enemy
alternative. Two sites and 17 contour samples per site retain the existing shared
planner limits. Evaluation and flag survey dispatch share the residual 4 graph /
384 query allowance after native planning. No extra synchronous world query is
introduced by admission.

Only published certificates can supply historical walking, landing, hatch and
climb costs. The existing flag shadow's admission checks are shared: request,
actor, generation, objective, material revision, radius, source/publication
epochs, query predicates, geometry, complete round trip, in-range endpoint,
calibration bounds and age. Existing local evidence and unrelated negative
findings retain precedence. Missing, superseded, negative and expired samples
revoke imported costs. Consumption also rechecks strict flag identity. The
controller retains its recovery, descent commitment, altitude and switch gates.

These are conditional references. Native arrival, finding a usable local site,
combat exposure, changed gravity and future material changes are not forecast.
The actual controller reacquires and validates its own landing site and route;
the remote certificate does not authorize landing or boarding.

## Development observations

Eight exploratory trials used seed 42, bearing 0.8, both mirrors and both
`destination` / `value-destination` fixtures, predecessor and candidate.
Raw results and the development binary are in `target/flag-cost-development/v1`
and `target/flag-cost-development/surface_mission_soak-v1`.

In the two first-foothold trials, the candidate switched earlier, completed both
sorties, and first claimed at ticks 1773 / 1837 versus 2339 / 2872: 9.43 / 17.25 s
earlier. The owned-base trials now produced 35 / 104 reports with two numeric
destinations, but kept the same physical outcomes. Their first comparisons gave
the enemy higher ownership value despite the neutral's shorter completion cost.
These are development observations, not held-out strength evidence.

## Progress measurement

`--measure-mission-progress true` adds a read-only observer. It counts elapsed
ticks without a new best distance to the active goal (2 world units in flight,
0.2 on foot), a claim-fraction improvement of 0.005, or an actual
arrival/landing/claim/boarding/departure/recovery milestone. Ground distances use
the rotating planet frame. Changed goals, sites, ground tasks and local ground
endpoints initialize distance baselines without resetting elapsed time.
Replanning to the same endpoint retains its previous best, so oscillation must
still beat that distance.

Reports include the longest interval, ticks beyond 20 seconds without observed
progress, eligible ticks and distance coverage. Combat, avoidance, patrol and
watch are excluded and end an interval. Missing distance observations still
permit claim or milestone progress. This is lack of observed objective progress,
not proof of immobility or failure: detours can legitimately lengthen it, and
relative goal motion can shorten it. It replaces the earlier phase-duration
proxy without claiming to measure combat effectiveness.

## Frozen validation plan

Freeze code and this plan before running `tools/validate-flag-costs.py`.
Use Rust 1.89.0, the release `surface_mission_soak` example with `sensor-profile`,
and preserve binary, source and raw-output hashes. No tuning after the first
held-out result; any follow-up changes require a new experiment identity.

* 32 directed trials: both fixtures, both mirrored seats, bearings 0, 0.8, 1.2
  and -0.8, predecessor/candidate pairs. Retain unsupported and interrupted trips.
* 32 fresh finished matches: four SHA-256-derived world seeds from
  `published-flag-costs-v1:{0..3}`, quiet / 3-second asteroids, swapped candidate
  seats, predecessor/candidate pairs against v10. All use a fixed report observer
  seat 0 and a 600-second round limit. Rotate execution order without changing
  matchups. Both arms enable the same flag survey infrastructure; only the
  candidate seat changes demand and admits costs. These are correlated worlds,
  not 16 independent strength samples.
* Preserve the first supported current-destination forecast for every visit,
  including abandonments and unfinished trips. Pin every accepted switch to its
  original forecast and exact destination visit. Record cumulative phase and
  whole-trip errors; earlier milestones are explicitly already observed.
  Unchosen alternatives have no observed result.
* Record completions, recoveries, ship losses, pilot deaths, observed no-progress,
  ownership/outcome and switch counts. Audit every dispatch across all three
  ledgers and link admitted evidence to raw publication certificates. Report
  sensor/construction/dispatch/policy/physics timing scope separately.

Promotion requires supported useful decisions through physical completion,
freshness/refusal coverage and no unexplained paired behavior or safety
regression. Zero switches only demonstrate fallback. A small match advantage
alone cannot justify a broad win-rate claim. If evidence is inconclusive or
regresses, retain current defaults and keep #142 open with the concrete next gap.

```sh
CARGO_TARGET_DIR=/home/data/workspace/space-wars3/target cargo +1.89.0 build --release --locked -p spacewars-ai --example surface_mission_soak --features sensor-profile
python3 tools/validate-flag-costs.py --binary target/release/examples/surface_mission_soak --out target/published-flag-costs/v2
```

## Results

The first run (`target/published-flag-costs/v1`) completed all 32 directed trials
and the first generated predecessor match, then its audit rejected that match.
The audit incorrectly equated the planning queue's dispatch ordinal with the
world tick: directed fixtures begin at world tick 1, generated worlds at 0.
The corrected audit checks dispatch ordinals and joins costs by the separately
recorded world tick. The regression test exercises both epochs.

All v1 artifacts remain intact. Validation replays the same plan and identical
frozen binary in `target/published-flag-costs/v2`; this is an audit correction,
not a new candidate or new held-out seeds. All 33 overlapping physical results
and evaluation streams matched exactly on replay.

### Progress observer correction replay

Local review found that the ground-distance baseline identified only the planet
and destination kind. Replacing a route endpoint with a nearer one could count
as progress while the actor stood still. The corrected observer also identifies
the mission goal, ground task and planet-local endpoint; changing that identity
sets a baseline without resetting the no-progress interval. Replanning to the
same endpoint within the same task retains its best distance.

The correction, regression tests and replay plan were frozen at `55462b6`
before replaying the identical 64-case plan in `target/published-flag-costs/v3`.
All 64 recorded physical outcomes, missions, predictions, decisions and work
allocations matched v2. Evaluator results, evaluator work, flag publications,
flag work and destination-cover streams were byte-identical in every case.
The archive now contains corrected progress, replay timings and hashes, plus
the previous progress counts and replay parity records. V2 and its binary remain
intact. Across both seats, 35 false progress ticks were removed in 23 cases;
distance coverage and the reported longest intervals and time beyond 20 seconds
did not change. This corrects
measurement of the existing study; it introduces no new held-out seeds,
candidate behavior, thresholds or promotion claim.

### Completed validation

The [machine-readable evidence](data/published-flag-costs-v1.json) records the
plan, commands, per-seat configuration, original predictions, actual milestones,
failures, progress, timing and raw-file hashes. Candidate behavior was frozen at
`26dd4c5`; the corrected runner at `bc4d483`; the corrected progress observer at
`55462b6`. The replay binary SHA-256 is
`f2170dea202e825af07ef04728706a8cf1c8996ae6ccce7ce318f64080a38ef0`.
Original source, binary and summary hashes are retained in `observer_correction`.

All 64 cases passed their physical and work audits: 1,029,840 simulated ticks.
The combined dispatcher maximum was 4 graph operations and 194 physics queries
per tick, within the shared 4/384 allowance. This is a dispatch quota result,
not a total CPU or frame-time guarantee.

The candidate made six directed switches; all six reached claim, boarding and
departure. The predecessor made two switches. First milestone improvements
compare each player's first actual milestone, including cases where the policies
captured different planets first:

| Seat / flag bearing | First claim earlier | First boarding earlier | First departure earlier |
| --- | ---: | ---: | ---: |
| P1 / 0.8 | 9.43 s | 9.43 s | 9.45 s |
| P1 / 1.2 | 13.35 s | 17.45 s | 17.47 s |
| P1 / -0.8 | 37.92 s | 37.92 s | 37.92 s |
| P2 / 0.8 | 17.25 s | 17.25 s | 17.45 s |
| P2 / 1.2 | 14.72 s | 18.80 s | 18.82 s |
| P2 / -0.8 | 34.00 s | 34.00 s | 34.00 s |

The other ten directed pairs preserved physical outcomes. Both arms completed
21 sorties overall, with no ship or pilot losses. Bearing 0 remained unsupported
in the first-foothold fixture; it was not dropped. Its longest measured interval
without progress was 125 seconds in both arms.

The eight owned-base candidate trials now had 388 reports with multiple numeric
destinations, versus zero for the predecessor. They still made no switches and
preserved physical outcomes. This demonstrates usable comparisons, not improved
owned-base decisions.

### Fresh matches and prediction limits

All 32 generated matches finished. Every one of the 16 paired comparisons had
identical recorded physical outcomes:

| Candidate-seat measurement, 16 runs per arm | Predecessor | Candidate |
| --- | ---: | ---: |
| Wins / losses | 8 / 8 | 8 / 8 |
| Destination switches | 0 | 0 |
| Completed sorties / recoveries | 37 / 4 | 37 / 4 |
| Ships lost / pilot deaths | 11 / 6 | 11 / 6 |
| Time beyond 20 s without observed progress | 962.87 s | 962.87 s |
| Median / maximum longest no-progress interval | 19.83 / 413.42 s | 19.83 / 413.42 s |

Distance was observed on 171,448 of 247,959 eligible candidate-seat ticks
(69.14%); other eligible ticks still allowed claim/milestone progress. Recovery,
missing geometry and legitimate detours limit interpretation of the long tail.
Eight development replays also preserved all recorded physical outcomes with
the progress observer enabled.

Held-out candidate evaluations admitted published flag costs in 5,972 records.
They produced 2,586 reports with multiple numeric destinations, versus 2,571 for
the predecessor. The only 15 reports preferring an alternative came from
`world1-asteroids3-p1`: equivalent savings of 2.38–3.69 s stayed below the
unchanged 5.30–5.66 s switch margin. No switch was warranted by that gate.
There were still 24,111 current-candidate records with unmeasured local/remote
surface costs. Among the candidate seats' 92 completed flag samples, 25 were
published, 34 lacked measured landing/boarding/climb, 28 lacked a walking round
trip, and five failed changed-geometry checks. Repeated records are not
independent decisions.

For the six accepted directed switches, original departure-reference error
(prediction minus actual) ranged from -5.58 to +4.22 s; median absolute error was
2.32 s. Each original source forecast and all five actual milestones remain in
the evidence. That narrow success does not calibrate the broader model:

* First supported current predictions in directed candidate trials: 17 completed,
  18 abandoned, one unfinished. Completed median absolute departure error was
  6.67 s; maximum 34.92 s.
* First supported current predictions in held-out candidate runs: 37 completed,
  ten abandoned, one unfinished. Completed median absolute departure error was
  3.93 s; maximum 91.72 s.

These are source-time conditional references; some first numeric forecasts occur
mid-trip. Already observed milestones are marked explicitly. Failures remain in
the denominator, and an unchosen destination has no observed counterfactual.
The new model expands forecast coverage, so its error set is not the same sample
as the predecessor's. Arrival/acquisition and exposure remain unmodelled.

### Timing, checks and decision

On this host during the observer correction replay, the largest per-run held-out
candidate p99 times were 3.680 ms for sensors, 0.030 ms for policy, 1.544 ms for
physics steps, 0.0013 ms for evaluator construction, 0.0011 ms for evaluator
dispatch and 0.0053 ms for flag dispatch.
Flag dispatch includes snapshot/publication validation, whose work is outside
operation quotas. These component percentiles cannot be added into a frame
percentile. Drawing was disabled; total measured-frame timing is absent. Trace
IO and the progress observer are outside these component measurements. This
does not establish target-device performance.

Original validation passed 276 AI unit tests, 35 harness tests, four physical
destination tests, and 564 Python tests. The observer correction passed all 37
harness tests and the three flag-cost audit tests, plus formatting and strict
Clippy for the harness with `--no-deps`. Its two new regressions exercise the
public observer: a stationary endpoint change preserves the stall interval;
same-endpoint replanning retains the best distance, while a new ground task
establishes a fresh baseline. Actual movement still counts afterward.
Existing shared tests cover near-expiry refusal,
recovery priority, descent/return commitment and unknown/stale evidence; new
candidate tests cover opt-in demand, source pinning, strict flag identity,
negative replacement, withdrawal, missing publication and expiry. Original strict
Clippy passed for the AI library and harness with `--no-deps`. Dependency-inclusive
Clippy stops at the existing `engine-rapier/src/spaceling.rs:395`
`collapsible_else_if` warning; that file is unchanged.

**Decision: retain existing defaults; keep this candidate opt-in and #142 open.**
There is now a supported improvement in directed first-foothold choices, but no
demonstrated match-strength improvement. The owned-base comparisons and broad
timing tail still do not justify promotion. The next evidence gap is usable
surface costs before native approach commitment, with explicit acquisition and
exposure uncertainty. Preserve the switch margin while investigating that gap;
this sample is not a reason to lower it.
