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
the rotating planet frame. New goals and replans initialize distance baselines
without resetting elapsed time. Oscillation must beat the previous best.

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
python3 tools/validate-flag-costs.py --binary target/release/examples/surface_mission_soak --out target/published-flag-costs/v1
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
not a new candidate or new held-out seeds. Results pending that replay.
