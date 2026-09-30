# Approach-aware neutral surveys

This follows [v15's landing handoff study](costed-landing-handoff.md). V15
connected a forecast bearing to native arrival, but all four completed neutral
references departed later than v14. Two enemy references lost cover during
approach and fell back; one fallback added 9.48 seconds to departure.

## Investigation

The remote neutral survey chooses two material bearings once per journey.
Subsequent measurements refresh those same bearings, even as the ship moves.
Every valid neutral site receives the same phase constants, so recency (then
bearing number) chooses the reference. It does not compare approach geometry.

In the P1 bearing-0.8 neutral fixture, the original survey offered bearings 1
and 48. The accepted source at tick 445 selected 48. By native arrival at 497,
v14 chose 40; v15 held 48. Both entered SeekCover at 498, but v14 started
Approach at 501 and v15 at 858. Departure was 1,998 versus 2,211. The extra
circling explains most of this slowdown. Other neutral cases show the same
phase pattern. These are observations from the archived corpus, not a fitted
landing-time model.

The enemy cases are a different problem. At initial acquisition the opponent
was behind ground, and the chosen sites already lacked complete approach and
departure cover. Later exposure invalidated both references. In the 1.2 case,
v15's fallback actually landed 30 ticks earlier than v14, then took much longer
to claim and board: departure moved from 5,091 to 5,660. Shorter angular flight
alone cannot account for enemy exposure, flag walking or boarding.

Source: `target/costed-landing-handoff/frozen-v1`, bound by the tracked summary
SHA-256 `03e2ca83139228fcb78fe998df4afa2f3c4fcc4e671a5c08d8080800e8813f3c`.
Use native acquisition/milestone ticks for joins; event snapshots can be logged
on the preceding world-step tick. Relevant records are `report.json`,
`destination-cover.jsonl`, `landing-handoffs.jsonl` and the complete compressed
`destination-behavior.jsonl.gz`. `sensors.jsonl` contains profiles, not full
observations.

## Candidate contract

V16 (`material_mission_v16`, **Approach survey bot v16**) changes only the
neutral survey's requested bearings and ranking. The one-planet shortlist and
two-site bound remain. When the bearings change, requests may refresh at most
once per 30 ticks within the same journey/material identity. Local landing
sensing still takes priority. Unchanged bearings keep the existing generation.

Rank valid historical measurements by absolute angular separation from the
currently observed ship position, then measurement recency and bearing number.
Reproject each measured vehicle point from its original planet pose into the
current observed material frame. Reject degenerate/nonfinite/overflowing
geometry and preserve the original measurement tick. This is an approach
proxy in radians, not predicted arrival time, route feasibility or future cover.
Existing valid evidence may survive while a replacement request is pending;
its age is never renewed by retargeting.

Keep v15's landing handoff, source expiry, native reacquisition/refusal,
flag-evidence admission, phase constants, ownership weights and switch margins.
Ranking adds no world queries; all remote work uses the existing shared
4-graph-operation / 384-query dispatcher. Refreshes may increase request churn
and total queries, so an unchanged per-tick cap is not an equal-CPU-work claim.
Synchronous native sensors, snapshot construction and trace I/O remain outside
that cap. V16 is headless; UI and device defaults are unchanged.

## Development observations

Six exploratory replays are retained under
`target/approach-aware-surveys/exploration-v1`, including source/diff/binary
hashes and complete commands. No constants were tuned. Three neutral departures
improved versus v15 by 52, 132 and 138 ticks; P1 1.2 slowed by 53 ticks.
All four neutral cases reached all-planets-owned earlier. Both enemy cases
retained their v15 visits and refusals. These known fixtures are development
evidence, not independent strength samples or sufficient grounds for promotion.

Review found a finite-vector squared-length overflow and a publication-order
test that observed the same tick twice. The candidate rejects nonfinite norms;
the test now advances the tick. Flag consumption and pending/ready handoff
invalidation tests exercise v16 alongside v15.

## Frozen plan

Freeze code, tests, this plan and `tools/compare-approach-surveys.py` before
execution. Keep every failure, refusal, incomplete trip and regression:

- 32 directed runs: both original-destination and owned-base fixtures, both
  seats, bearings 0.0/0.4/0.8/1.2, v15 and v16, 180 seconds per run.
- The known seed 186767996776005237 regression, v15 and v16 in seat two versus
  v10, combat enabled, no asteroids, normal ten-minute match deadline.
- Twelve finished-match smoke runs over two fresh SHA-256-derived worlds from
  `approach-survey-behavior-v1:{0..1}`: no asteroids / three-second asteroids,
  v15/v15 controls and v16 in each seat against v15, rotating execution order.
  These are two independent worlds with correlated variants, not twelve
  independent strength trials.

All 17 directed/regression v15 controls must reproduce archived exact evaluator
bytes, mission telemetry and recorded physical outcomes. Paired complete action,
range and ownership traces must match before the first destination switch;
pairs with no switches must retain their physical outcomes. Audit shared work,
published flag references, accepted switch sources, native handoffs and fallback
cost invalidation for both policies. Report phase milestones, wins/losses,
captures/departures, transfer progress, survey queries/churn and unknowns.

`--trace-neutral-approaches true` records the v16 evaluator's immutable input
before shared dispatch, including both measurements, the current journey and
all observed planet/ship poses and identities. The optional headless trace adds
no queries. Check the full report-derived actor/tick sequence, both requested
slots and the refresh bound for uninterrupted demand. Independently join every
v16 evaluation to that input, verify original measurement ages and neutral phase
constants, and check angular ranking where current measurements exist. Separately
count retained references while replacement measurements are absent. Track
positive admission separately from the raw measurement registry; a negative
replacement, missing query readiness, expiry or changed material/ownership
cannot authorize a later retained numeric reference. Independent double
arithmetic admits only winners within 0.00001 radians of the shortest approach;
Rust tests cover exact recency/bearing tie ordering. A
post-dispatch `destination-cover.jsonl` publication is not evidence that the
same-tick decision had that sample available.

```sh
cargo +1.89.0 build --locked --release -p spacewars-ai \
  --example surface_mission_soak --features sensor-profile
python3 tools/compare-approach-surveys.py \
  --predecessor target/costed-landing-handoff/frozen-v1 \
  --out target/approach-aware-surveys/frozen-v1
```

Use a clean checkout and a new output directory. Keep commands and all
source/binary/artifact hashes. Review the complete results before deciding
whether to retain, reject or promote; leave defaults unchanged meanwhile.

## Results at `3461cbd`

All **46 runs / 25 comparisons** completed on the first frozen execution:
**662,089 physical ticks / 183.91 simulated minutes**. Physics, per-tick work,
flag-publication, neutral-source and native-handoff audits passed. All **17**
directed/regression v15 controls reproduced the predecessor's exact evaluator
bytes, mission telemetry and recorded physical outcomes. Every paired behavior
prefix check passed; 21 of 25 comparisons retained physical outcomes.

The [complete summary](data/approach-aware-surveys-v1.json) is an unchanged
copy of the raw summary, SHA-256
`e13baeb8b4c36e37707f8741573df584194067a4fc237751dc4c2bf49e135120`.
It retains commands, source/binary/tool/artifact hashes, original forecasts,
visits, refusals, prediction errors, work, progress and unknowns. Full reports
and traces remain under `target/approach-aware-surveys/frozen-v1`, alongside the
matching `surface-mission-soak-3461cbd` binary. No parameter fitting or candidate
changes occurred between runs. Instrumented desktop timing, including unequal
opt-in trace I/O, is not a CPU or Pi performance comparison.

### Directed outcomes

V16 made the same six switches at the same ticks as v15. Four neutral references
completed through native touchdown, claim, boarding and departure. Three first
departures improved; one slowed. All four reached all-planets-owned earlier:

| Case | V16 bearing | First claim change | First departure change | All owned change |
| --- | ---: | ---: | ---: | ---: |
| Neutral P1, 0.8 | 44 | −0.87 s | −0.87 s | −0.98 s |
| Neutral P1, 1.2 | 45 | +0.88 s | +0.88 s | −0.50 s |
| Neutral P2, 0.8 | 44 | −2.22 s | −2.20 s | −2.77 s |
| Neutral P2, 1.2 | 46 | −2.30 s | −2.30 s | −3.22 s |
| Enemy P1, 0.8 | 57 | 0 | 0 | 0 |
| Enemy P1, 1.2 | 61 | 0 | 0 | 0 |

Changes are v16 minus v15; positive means later. The ten other directed pairs
retained their outcomes, including both original-destination bearing-zero cases
that still made no capture. The known regression also remained identical: the
candidate seat won, completed five sorties and made no switch.

The older v14 control remains informative: v16's two P2 neutral departures beat
v14 by 0.22 and 0.67 seconds, but its P1 departures remained **2.68 and 4.52
seconds slower**. Shorter separation at the source pose is not enough to select
the fastest eventual approach. The four neutral whole-trip forecast errors
ranged from −0.30 to +2.80 seconds (prediction minus actual); faster execution
does not establish a newly calibrated timing model. No constants changed.

Both enemy references retained their v15 cover refusals and exact fallback
visits. Their frozen forecasts still underestimated departure by 37.31 and
9.18 seconds. After refusal, each policy produced 194 unknown current-trip
reports and 410 fresh native replacement reports in the directed/regression
set. These are repeated observations, not independent attempts. They do not
count as completing the original planned landing.

The four neutral longest transfer intervals without a two-unit range gain
changed by −33, −23, −2 and −1 ticks; enemy intervals were unchanged. This
direct-range measure excludes landing, walking, recovery and combat, so it
does not explain the entire departure or second-trip difference.

### Fresh matches

All **12 fresh matches** finished. All eight candidate/control comparisons
retained identical physical outcomes, and a separate post-study comparison
verified the **complete** action/range/ownership traces, ignoring only policy
identity. Its [parity record](data/approach-aware-surveys-trace-parity-v1.json)
retains per-pair row counts. Candidate seats won four and lost four, matching
those seats in their controls. Captures, departures, recoveries, pilot losses
and transfer progress were unchanged; longest no-range-gain intervals reached
416 ticks (6.93 seconds). These remain two worlds with correlated variants.

There were two v16 candidate-seat switches, both to enemy flags in world 0,
and both reproduced v15's decision and execution:

- No asteroids: switch at 6,216, fresh acceptance at 6,345, referenced touchdown
  at 7,995, claim at 8,361, boarding at 8,363 and departure at 8,592. This is a
  completed planned-site trip in generated combat play.
- Three-second asteroids: switch at 6,265, acceptance at 6,375, identity-change
  refusal at 6,705, fallback touchdown at 8,115 and departure at 8,813. This is
  a completed fallback trip, not a completed reference. The retained reason is
  an identity change; these data do not isolate its damage source.

Those original whole-trip forecasts underestimated departure by 3.51 and 4.53
seconds. This adds active handoff/fallback coverage in fresh worlds, but neither
switch used the changed neutral-site ranking. There is **no demonstrated
improvement in general match decisions or strength**.

### Evidence and work

The independent neutral audit joined **78,069 evaluations** to **376,791 dense
input rows**. It verified 29,464 original-age neutral references: 15,412 ranked
against current measurements (12,850 with two usable sites), and 14,052 retained
while current replacement measurements were absent. There were 2,053 input
observations with compatible negative-only results; these cannot authorize a
positive reference or preserve an earlier one. Counts include repeated observations.

Review strengthened the audit before freezing: separate raw immutability from
admission, retain only possible winners within floating-point tolerance, revoke
negative/query/material/age transitions, require both request slots and dense
input history, and reject samples first visible on their own measurement tick.
Adversarial tests exercise these cases, including legitimate pending-generation
continuity and Rust float ties. Two development trace checks preserved the
exploratory run's exact physical outcomes and mission telemetry.

The largest combined allocation remained **4 graph operations / 160 queries**,
inside the 4/384 allowance. Remote survey work nevertheless increased:

| Directed + regression aggregate (17 runs per version) | V15 runs | V16 runs |
| --- | ---: | ---: |
| Destination-cover queries | 102,023 | 109,734 |
| Flag-survey queries | 31,456 | 30,973 |
| Both remote query queues combined | 133,479 | 140,707 |
| Submitted destination-cover requests | 54 | 320 |
| Evaluator graph operations | 91,429 | 90,320 |
| Cancelled evaluator jobs | 2,025 | 1,538 |
| Reports with multiple numeric destinations | 1,142 | 1,274 |

Destination-cover queries increased **7.56%**; those request submissions grew
almost sixfold. Including the separate flag-survey queue, combined remote
queries increased **5.42%**. Fresh candidate matches used **2.95–11.57%** more
destination-cover queries than their paired controls, with unchanged behavior.
Evaluator work and cancellations
decreased in the directed/regression aggregate; none of these operation counts
includes synchronous native queries, snapshot construction or serialization.
Totals cover both evaluated seats in each run, including the v10 opponent in
the regression. Repeated report counts are not additional independent decisions.

The dominant unknown remained unmeasured remote/local surface evidence:
18,288 candidate entries across v15 runs versus 18,204 across v16 runs in the
directed/regression set. Each includes 530 v10-opponent entries. Incomplete
round trips, unavailable routes and unmodelled transfer
detours also remain. The complete summary retains every unknown reason.

### Decision and next boundary

**Retain v16 as a headless experiment; leave UI/device defaults unchanged and
#142 open.** Updating the two survey bearings produces a modest, mixed benefit
in a controlled neutral fixture family, with additional remote work. It does
not resolve the larger enemy exposure/ground-route error or improve fresh
match results.

The next useful investigation is **source pose versus actual arrival motion**:
compare signed angular motion, the native circling direction and the
SeekCover → Approach transition in the P1 regressions and P2 improvements.
Use the frozen v14/v15/v16 controls above. In particular, replay
`destination-p1-bearing1.2-v16` and compare its source input, accepted site and
phase milestones against both predecessors before changing another policy.
This should establish when a historical preference should yield to the native
arrival choice, or whether a bounded arrival estimate can nominate a better
site. Keep the enemy cover/flag-walk problem separately visible.

Validation: 290 AI unit tests, six physical destination integration tests,
31 soak-harness tests and 581 Python tests passed. Formatting and strict Clippy
passed for the changed AI library/example/integration targets. Independent
runtime and audit review completed before the freeze.
Independent results review also verified the raw artifacts, all 18 recorded
switch/native-handoff joins, work ledgers and complete fresh trace comparisons.

### Integration with the published-flag-cost experiment

On 2026-09-30, reconciled main's #144 with the #145 → #147 → #149 stack.
The opt-in v13 admission model remains separate from v14–v16, including its
report fields, native evidence precedence and current-target demand. A test
checks that configuring the v13 option cannot override another policy's model
or evidence. Both paths retain original measurement epochs. Independent review
also checked once-per-tick observation, handoff invalidation and pre-dispatch
v16 input recording.

The integrated tree at `a0e0a93` reproduced **94 archived runs** exactly:
all 46 frozen v15/v16 cases above, 24 #144 cases, and 24 v13/v14 cases.
The latter two subsets contain both seats at bearings 0.8/1.2 in both directed
worlds, generated world 0 with and without asteroids, and the v13/v14 regression
pair. Physical outcomes, full mission telemetry, evaluator/work/survey streams,
and complete behavior traces where present matched for 1,388,219 simulation
ticks. Raw manifests and hashes are in `target/stack-integration/a0e0a93/parity.json`
(46 v15/v16 cases) and its `predecessors/parity.json` (48 v13/v14 cases),
both using the same final binary. An earlier preliminary run at `b27b831` is
not included in these 94 runs.
All six CI jobs passed on each integrated PR tree. The later squash-ancestry
updates changed no files; final merged main `af2be2e` has the same tree as
`a0e0a93` (`06f84ff827030a1eb6698aa273496f7922760936`).
294 AI unit tests, six physical destination tests, 37 harness tests and 584
Python tests passed, as did formatting and strict AI library/harness Clippy
with `--no-deps`. A mixed v13-opt-in/v16 smoke run reports both consuming seats
and their distinct models correctly.

### Arrival-motion follow-up: 12 dense native replays

Used the existing `--trace true --trace-start-tick … --trace-end-tick …`
options to record the first switched neutral visit in **all four changed
fixtures, for v14, v15 and v16**. No policy, sensor request or physics changes.
Each replay reproduced its archived physical outcomes, mission telemetry and
evaluator/work/cover streams. The analysis checks 19,447 consecutive controller
ticks, original trace hashes, phase ordering, and identical ship/planet motion
at the first site selection across the three versions of each fixture.

The [machine-readable analysis](data/arrival-motion-v1.json) retains source and
binary identity, geometry, original milestone ticks and phase durations.
Raw traces and the replay plan/commands are in
`target/arrival-motion/investigation-v1/summary.json`. Recompute with:

```sh
python3 tools/analyze-arrival-motion.py \
  --root target/arrival-motion/investigation-v1 \
  --out target/arrival-motion/investigation-v1/analysis.json
python3 -m unittest discover -s tools/tests -p test_arrival_motion.py
```

The first native site is selected from **identical arrival motion** in each
v14/v15/v16 group. V16's earlier source geometry nevertheless differs from
its geometry when native acquisition actually happens:

| V16 case | Source signed angle | Acquisition signed angle | Acquisition tangent speed | Circling seconds, v14 / v15 / v16 |
| --- | ---: | ---: | ---: | ---: |
| P1, 0.8 | 0.1501 | 0.3648 | -19.48 | 0.05 / 6.00 / 4.58 |
| P1, 1.2 | 0.0605 | 0.2448 | -20.20 | 0.05 / 4.77 / 3.72 |
| P2, 0.8 | -0.0174 | 0.1675 | -14.84 | 0.00 / 5.07 / 0.00 |
| P2, 1.2 | 0.0015 | 0.1692 | -15.12 | 0.00 / 3.80 / 0.00 |

Angles are radians, signed counterclockwise from ship radius to site radius.
Tangent speed is in the rotating planet frame. Source geometry reprojects the
historical measured vehicle position into the planet's original evaluation
pose; it does not import later physics.

The existing native `SeekCover → Approach` gate requires `abs(angle) < 0.2`,
`abs(tangent_speed) < 18`, and cover or lack of exposure. These cases are
unexposed, with no sun. Both P2 v16 choices enter Approach immediately; the P1
choices first circle, reversing their initial negative tangential motion to
roughly +17.5 before descending. This explains the phase boundary and the seat
asymmetry; it does not establish that changing the threshold would help.

**The remaining P1 1.2 regression is downstream of faster circling.** In ticks:

| P1 1.2 phase | V14 | V15 | V16 | V16 minus v15 |
| --- | ---: | ---: | ---: | ---: |
| First site → Approach | 3 | 286 | 223 | -63 |
| Approach → Surface control | 519 | 437 | 599 | +162 |
| Surface control → touchdown | 443 | 460 | 414 | -46 |
| Touchdown → completed departure | 410 | 410 | 410 | 0 |

Here `Surface` means entry to the final landing controller, not physical
contact. The net is **+53 ticks (0.883 seconds)**. V16 enters Approach with
radial velocity -9.33, versus +0.34 for v15, and subsequently reaches lateral
error 21.71 versus 9.79. It spends 205 versus seven Approach ticks outside
8 units of lateral error, where the native guide asks for a height of 30
instead of 12. Those observations identify a concrete descent mechanism to
inspect, not a proven isolated cause. P2's faster v16 runs also have substantial
lateral excursions, so that count alone must not become a ranking metric.

The next bounded behavior experiment should test **when the historical neutral
site preference yields to ordinary native arrival selection**, using the
existing angle/motion/cover gates and fresh measured geometry. Rejection must
invalidate the old handoff/cost basis through the existing path; native landing,
hatch, route and solar checks still own execution. Compare complete
claim/board/depart outcomes, preserve v14–v16 controls, keep the two-seat shared
work ledger, and retain failures. A source-nearest site and a shorter circling
phase are both insufficient proxies for completing a capture sooner. The enemy
cover/walking problem and any future arrival predictor remain separate.
