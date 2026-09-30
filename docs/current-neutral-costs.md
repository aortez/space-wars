# Current-neutral estimates before arrival

Follow-up to [published flag costs](published-flag-costs.md) and issue #142.
PR #144 merged at `2e5a2c0`; existing bot defaults remain unchanged.

## Recorded gap

The [arrival audit](data/current-neutral-baseline-v1.json) joins every evaluation
to its exact seat, planet and selection tick in all 64 recorded cases. It uses
the evaluator's completion tick for availability, excludes observations after
departure/abandonment, and retains visits with no usable evidence or arrival.
An arrival milestone is not the native altitude/commitment gate.

In the 16 generated candidate runs there are 76 visits. Twenty get numeric
costs for both the current destination and an alternative, all after arrival;
every first pair has a neutral current destination. Only four visits get a
complete numeric shortlist, also after arrival. There are 10,528 repeated
pre-arrival reports with an unmeasured neutral current surface cost. These
are observations in four correlated worlds, not independent decisions.

`mission_evaluation/survey.rs` explicitly excludes the current destination
from neutral survey demand. The published-flag experiment can measure a current
enemy, but current neutral costs normally wait for native local observations.
This motivates testing current-neutral demand. It does not establish that every
missing comparison is caused by this exclusion, or that earlier evidence will
beat the existing switch margin.

```sh
python3 tools/analyze-evidence-arrival.py \
  --study target/published-flag-costs/v3 \
  --out target/current-neutral-baseline-reproduction.json
```

## Candidate boundary

`--survey-current-neutral none|0|1|both` enables
`capture_value_current_neutral_v1` for selected headless v13 seats. It requires
published flag-cost admission for those same seats, mission evaluation, shared
planning and neutral surveys. The predecessor is the merged
`capture_value_published_flags_v1`; both arms continue using flag certificates.
Every evaluator record and per-seat configuration identifies the candidate.

The candidate requests two material bearings on its neutral current destination
and two on one neutral alternative. This uses the dispatcher's existing limit
of four sites on two planets. With an owned current target, demand is identical
to the predecessor. Requests retain their generation through motion and yield
to local landing work. No local sensor is replaced; dispatch remains inside
the shared 4 graph / 384 query allowance. Total work may increase within that
per-tick cap, and flag jobs may receive less residual work.

Evidence is chosen independently per planet, preserving its source timestamp,
ownership/material identity, landing, hatch, climb and age checks. Native local
measurements retain precedence. A negative replacement or incompatible/expired
source cannot supply a cost. The existing conditional no-flag timing calibration,
ownership values, hysteresis, match-expiry and native safety/commitment gates
remain unchanged. Two numeric options do not imply a complete three-option
shortlist. Native acquisition, exposure and future geometry remain unmodelled.

## Frozen validation plan

Freeze implementation, runner and this plan before the experiment. Use Rust
1.89.0, the release `surface_mission_soak` example with `sensor-profile`, and a
preserved binary. Retain all commands, source/binary/input/output hashes and
failed attempts. Never tune on the fresh-match results.

1. Eight engineering runs from the previous study's world 0: both candidate
   seats, quiet / three-second asteroids, predecessor/current-neutral pairs.
   These are recorded worlds. Require the disabled option to reproduce the
   merged binary's physical outcomes, mission telemetry and evaluator bytes.
   Measure evidence availability for the enabled option and retain all outcomes.
2. Thirty-two directed trials: `destination` and `value-destination`, seed 42,
   both seats, bearings 0, 0.8, 1.2 and -0.8, predecessor/candidate pairs. These
   preserve the recorded directed domains; they are not fresh strength samples.
3. Thirty-two new finished matches from four SHA-256-derived
   `current-neutral-costs-v1:{0..3}` seeds, quiet / three-second asteroids,
   swapped tested seats against v10, rotated arm order, fixed observer seat 0
   and a 600-second deadline. Each pair differs only in current-neutral demand.
   Four worlds reused across conditions remain correlated.

Pin first supported predictions and every accepted switch to their actual
visits. Keep abandoned/unfinished attempts and unchosen alternatives explicit.
Compare first available current cost, current/alternative pair and complete
shortlist against observed arrival. Record capture/board/departure, recoveries,
losses, no-progress, outcomes and conditional timing errors. Audit all three
planning ledgers together and require paired behavior changes to have a
destination switch. Report host component timings separately from operation
quotas and target-device frame times.

```sh
CARGO_TARGET_DIR=/home/data/workspace/space-wars3/target cargo +1.89.0 build --offline --locked --release \
  -p spacewars-ai --example surface_mission_soak --features sensor-profile
python3 tools/validate-current-neutral-costs.py \
  --binary target/release/examples/surface_mission_soak \
  --out target/current-neutral-costs/v1
```

Earlier comparisons alone do not justify promotion. Retain defaults unless
supported decisions improve useful physical completion without unexplained
regressions; any device-default change also needs a Pi performance check.
