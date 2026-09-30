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

## Results

Implementation, runner and plan were frozen at `8832a5c`. The preserved release
binary is `target/current-neutral-costs/surface_mission_soak-8832a5c`, SHA-256
`5c9d6e4b5cd0a1d82e0bf24f8f63d14186655faa2fd14cde20c4a1ba3a55b2e4`.
The [complete evidence](data/current-neutral-costs-v1.json) retains all runs,
predictions, milestones, failures, timing joins, work audits and raw hashes.
Development artifacts are in `target/current-neutral-costs/development-v1`;
the frozen main study is in `target/current-neutral-costs/v1`.

All eight engineering runs passed. The four disabled-option replays retained
the merged binary's exact physical outcomes and mission telemetry. Evaluator,
evaluator-work, flag-publication, flag-work and destination-cover streams were
byte-identical. Enabling the candidate preserved those four physical outcomes
while producing the first two-option comparison 3.32–5.82 seconds earlier:

| Recorded world 0 condition | Arrival tick | Predecessor first pair | Candidate first pair |
| --- | ---: | ---: | ---: |
| P1, quiet | 613 | 615 | 266 |
| P2, quiet | 953 | 955 | 756 |
| P1, asteroids | 613 | 615 | 266 |
| P2, asteroids | 1,014 | 1,016 | 748 |

These first visits still had an unmeasured third option, so neither arm had a
complete shortlist or made a new switch in those engineering comparisons.

### Directed decisions and failures

All 32 directed runs passed their physics audits. The predecessor's six
switches completed capture, boarding and departure. The candidate made ten:
eight completed and two were abandoned. Thirteen of 16 paired physical outcomes
were unchanged; the three changes were owned-base P1 cases.

At bearings 0.8 and 1.2, the candidate switched from neutral planet 0 to enemy
planet 1 and completed both trips. Its first claim occurred 35.35 and 12.95
seconds later than the predecessor's neutral claim. Those are different
ownership gains, so first-claim delay alone does not measure their strategic
value. The 0.8 switch's conditional departure estimate was 39.43 seconds, while
the actual trip from that source took 76.50 seconds: a 37.07-second underestimate.
The 1.2 switch's departure error was +0.30 seconds.

At bearing -0.8, the candidate switched to the enemy at ticks 2,863 and 7,264.
Both attempts arrived but were abandoned before landing, at ticks 4,953 and
9,003, with the native reason `pausing travel for nearby opponent`. The
candidate completed no claim/boarding/departure within the declared 180-second
window; its predecessor completed one neutral sortie. These interruptions have
no invented completion error. Across directed trials, completed sorties fell
from 21 to 20; neither arm lost a ship or pilot.

### Fresh finished matches

All 32 new matches finished. Fourteen of 16 paired physical outcomes were
identical. Both changed pairs retained the same winner.

| Tested-seat measurement, 16 runs per arm | Predecessor | Candidate |
| --- | ---: | ---: |
| Wins / losses | 8 / 8 | 8 / 8 |
| Destination switches | 0 | 2 |
| Completed sorties / recoveries | 39 / 4 | 40 / 5 |
| Ships lost / pilot deaths | 11 / 4 | 12 / 4 |
| Recorded visits | 104 | 118 |
| Visits with first current cost before arrival | 11 | 34 |
| Visits with first current/alternative pair before arrival | 2 | 14 |
| Visits with first complete shortlist before arrival | 1 | 3 |
| Time beyond 20 s without observed progress | 1,223.87 s | 1,271.48 s |
| Median / maximum longest no-progress interval | 27.36 / 323.97 s | 30.53 / 323.97 s |

These are raw counts and durations, not independent success rates. Changed
trajectories and match lengths change the visit and observation denominators.
The candidate also had two first complete comparisons on visits with no observed
arrival; these remain separate from comparisons classified before arrival.
Two numeric options alone still do not satisfy a three-option selection gate.

The new world 0 quiet/P1 switch at tick 6,653 completed capture, boarding and
departure, with a -5.04-second departure-reference error. The new world 3
asteroid/P2 switch at tick 9,220 was abandoned before landing at tick 10,912 to
pursue a nearby opponent. That pair also went from one to two ship losses for
the tested seat. The record establishes the paired differences; it does not
isolate an individual collision or combat decision as their cause.

First supported current-trip predictions retained 32 completed, seven abandoned
and one unfinished predecessor attempts, versus 39 completed, 16 abandoned and
one unfinished candidate attempts. Completed median absolute departure error
was 4.64 versus 6.52 seconds; both maxima were 89.23 seconds. Earlier/newly
available predictions change the sample, so these are not errors on identical
forecasts. Acquisition, native site choice and interruption costs remain unknown.

### Work, validation and decision

The main study audited all 1,111,973 physical/dispatch ticks. Maximum combined
use was four graph operations and 195 physics queries in a tick, inside the
shared 4/384 allowance. All 64 report and associated raw-file hashes were checked.
This cap does not constrain total CPU time, snapshot preparation or sensors.

The largest per-run fresh-candidate p99 component times on this host were
3.820 ms for sensors, 0.019 ms for policy, 2.702 ms for physics, 0.0012 ms for
evaluator construction, 0.0010 ms for evaluator dispatch and 0.0045 ms for flag
dispatch. These percentiles cannot be added into a frame percentile. Drawing,
trace IO and observer work are outside the component figures; no Pi performance
claim follows from them.

Local validation passed 281 AI tests, 37 harness tests, four physical
destination tests and 571 Python tests. New tests cover per-seat/v13 opt-in,
current/alternative evidence separation, negative replacement, identity/age
changes, reset and local priority, plus completion-time joins, interrupted
visits and partial shortlists. Formatting and strict AI/harness/integration
Clippy with `--no-deps` passed.

**Decision: retain defaults and keep this candidate experimental; #142 remains
open.** Earlier evidence enabled actual enemy-versus-neutral choices, including
two completed directed choices and one completed fresh-match switch. It also
exposed interrupted trips, a lost directed completion and an additional ship
loss without a win advantage. This does not justify promotion.

The next decision-model gap is the mismatch between a conditional capture
estimate and the native controller's opportunity to finish it. In particular,
enemy approaches can yield to pursuit before landing, and the native site/return
task need not match the historical surveyed route. Preserve these failures as
regressions; investigate an explicit interruption/remaining-task eligibility
condition before another candidate. Do not fit a smaller switch margin to
these outcomes or turn unknown exposure into a zero cost.
