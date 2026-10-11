# Temporary cooldown after a cover rejection

The [visit-ending correction](capture-visit-endings.md) identified two directed
failures that repeatedly select the same uncovered site. Both first choose it
while unexposed, then select it seven more times while exposed, exhausting eight
native cover retries. A successful control finds sheltered ground after one
retry. This experiment remembers actual cover rejections without treating the
first selection or every retry as a failure.

## Candidate boundary

Headless `--cover-retry-seats none|0|1|both` enables `cover_retry_cooldown_v1` on
selected v13 seats. Default is `none`; the launcher and standard bot defaults
remain unchanged. Per-seat configuration and capture telemetry identify it.

After the native `replan_for_cover` decision, retain the rejected site's ID,
material revision, rejection tick and a 30-second expiry. At most eight entries
are stored, matching the existing maximum cover-retry allowance. Repeated
failures replace the same site's entry; reset/new capture tasks clear the history.
Thirty seconds is a declared cooldown using the existing solar-retry scale,
not a fitted estimate of when an opponent will move.

During native site selection, temporarily exclude that same site/revision while
the ship remains exposed and its current measured cover would not allow the
native SeekCover transition. The filter permits an earlier retry when exposure
ends, when ground and approach cover are usable, or when grounded cover is usable
below the native 40-unit height threshold. A new revision or expiry also permits
reconsideration. The ranker still applies every required-site, prior rejection,
solar, material and objective-route check; release never authorizes a landing.

Other measured candidates retain their existing scores and order. If none
survive, use ordinary acquisition waiting within the original capture deadline.
This may move the ship out of the approach frame and must remain a counted
failure if it does. Cooldown does not reset the eight-cover-retry limit or the
150-second overall deadline. It does not suppress combat, change destination
weights/hysteresis, request extra sensors, or add a planning job.

`capture.cover_retry_cooldown` records bounded history and exclusion-call counts,
including the first/last exclusion tick. Acquisition checks separately report
`cover_cooldown` rejections. Disabled telemetry omits the new fields so recorded
predecessor streams can remain byte-identical. Exclusion-call counts are repeated
observations, not independent tactical decisions or a success probability.

## Frozen validation plan

Freeze source, plan, tests and runner before outcomes. Build the sensor-profile
release harness with Rust 1.89.0 and preserve its binary. Retain all failures and
raw hashes; do not tune duration or eligibility on this sample.

1. Six regression runs: predecessor/candidate pairs for directed bearing -0.8,
   the recorded world-3 asteroid/P2 failure, and the successful +0.8 control.
   Require the disabled option to reproduce the preserved `1985ca2` physical
   reports, mission/metric records and all controller/evaluator/planner streams.
   Require candidate recorded trace prefixes to match up to its first exclusion,
   after removing only the new option telemetry.
2. Thirty-two directed runs: destination/value-destination fixtures, seed 42,
   both seats, bearings 0, 0.8, 1.2 and -0.8, paired predecessor/candidate.
3. Thirty-two new finished matches: four SHA-256-derived
   `cover-retry-cooldown-v1:{0..3}` seeds, quiet/three-second asteroids, swapped
   tested seats against v10, rotated arm order, fixed observer seat 0, 600-second
   deadline. Four worlds reused across conditions remain correlated.

Both arms admit published flag costs and survey the current neutral destination.
Only the selected seat's cover memory differs. Keep complete/abandoned/unfinished
trips, corrected first terminal reasons, capture/board/departure, recoveries,
losses, no-progress, outcomes and original conditional prediction errors.
The timing model is not recalibrated for the new retry behavior. Audit the shared
4 graph / 384 query allowance, and require changed physical pairs to show a
recorded cooldown exclusion. Host component timings are separate from quotas
and do not establish Pi frame time.

```sh
CARGO_TARGET_DIR=/home/data/workspace/space-wars3/target cargo +1.89.0 build --offline --locked --release \
  -p spacewars-ai --example surface_mission_soak --features sensor-profile
python3 tools/validate-cover-retries.py \
  --binary target/release/examples/surface_mission_soak \
  --baseline-manifest target/visit-terminal-audit/validation-v2/summary.json \
  --out target/cover-retry-cooldown/v1
```

Reduced repeated-site selection alone does not justify promotion. Require useful
completion without unexplained survival/progress regressions; retain defaults
unless the evidence supports a separate promotion and target-device validation.

The first validation attempt retained in `target/cover-retry-cooldown/v1`
exposed a runner clock mismatch: directed fixtures start at world tick 1, but
the trace's outer tick is a zero-based loop ordinal. The first exclusion is
world tick 3,930 / trace tick 3,929. Prefix comparison now uses the observation's
world tick, like native cooldown telemetry. A regression covers the offset.
The candidate code, duration, cases and seed plan remain frozen at `058d761`;
this correction changes only the verification join.

## Validation results

All 70 runs completed in `target/cover-retry-cooldown/v2`: six regression runs,
32 directed runs and 32 fresh match runs. The runtime and declared plan remain
at `058d761`; the corrected runner is `c8c1ac8`. The preserved executable
`target/cover-retry-cooldown/surface_mission_soak-058d761` has SHA-256
`8320cd4e3fea55915f2b1068b2138b989d1f74207076b6b6d554aa791e5ee17f`.
The [evidence archive](data/capture-cover-retries-v1.json) retains commands,
source/tool/raw hashes, per-seat visits and forecasts, progress, work, timings,
the directed native-choice audit and the rejected first validation attempt.
Every retained raw hash was checked after the run.

All three disabled replays reproduce the prior physical reports, mission and
metric records and all six trace/planner/evaluator streams exactly. Enabled
recorded trace prefixes match before the first exclusion: world tick 3,930 for
the directed failure, 2,475 for the recorded match and 3,510 for the successful
control. All physics and visit-ending audits pass, with no
unverified visits or milestone discrepancies. Across 1,348,445 audited dispatch
ticks, maximum combined work is four graph operations and 194 physics queries,
within the unchanged 4/384 allowance.

Totals below concern only the tested seat. Each arm has 16 directed fixtures and
16 fresh matches; the three regression pairs are separate.

| Measure | Directed predecessor | Directed cooldown | Fresh predecessor | Fresh cooldown |
| --- | ---: | ---: | ---: | ---: |
| Wins / losses / draws | — | — | 7 / 7 / 2 | 7 / 7 / 2 |
| Completed capture sorties | 20 | 20 | 36 | 37 |
| Completed recoveries | 0 | 0 | 6 | 3 |
| Ships lost | 0 | 0 | 15 | 12 |
| Pilot deaths | 0 | 0 | 3 | 3 |
| Completed / abandoned / unfinished visits | 20 / 52 / 6 | 20 / 50 / 7 | 36 / 74 / 4 | 37 / 67 / 3 |
| Median longest interval without progress, seconds | 5.45 | 7.63 | 34.03 | 75.04 |

The cooldown excludes sites in nine capture tasks across six directed cases and
six capture tasks across six fresh matches. These produce 335 and 30 exclusion
calls respectively, not that many independent choices. Four directed pairs and
two fresh pairs change recorded physical outcomes; each has a recorded exclusion.
All other pairs match the recorded physical fields.

The four changed directed cases are the -0.8 approaches, with both fixture types
and seats. None gains a completed sortie, and each has a longer worst interval
without progress. Three approach-frame exits replace retry-budget failures on
P2. Fewer abandoned visits do not establish improvement: longer attempts leave
fewer opportunities to start another visit before the fixed horizon.

For the P1 value-destination failure, native selection changes from repeatedly
choosing site 41 to cycling among sites 41, 42 and 43. Every chosen site's
grounded, approach and departure cover is false. Both versions exhaust eight
cover retries. The first abandonment moves from tick 4,953 to 8,013, 51 seconds
later, with no claim. Its next attempt is unfinished at the horizon. The +0.8
successful regression control still completes two sorties at the same ticks:
excluding a rejected site preserves its already preferred sheltered alternative.

The recorded asteroid/P2 regression remains a win, but completed sorties fall
from two to zero, recoveries from two to zero, and ship losses from two to one.
Its longest interval without progress rises from 30.67 to 268.47 seconds.
The first exclusion is at tick 2,475, before the original failed visit selected
at 9,220; that original attempt never occurs in the candidate trajectory. This
is not evidence that the cooldown repaired that specific failure.

Both changed fresh matches are world 3 with three-second asteroids. P1 changes
from a loss to a win, completes four sorties instead of three, and loses one
ship instead of two. P2 changes from a win to a loss, retains three sorties,
but completes no recoveries instead of two. Its longest interval without
progress rises from 24.27 to 257.68 seconds. The lower ship-loss total therefore
coexists with stalled recovery and is not sufficient evidence of better
survival or capture policy. The fresh worlds are distinct from the recorded
regression worlds, but reused conditions within these four worlds are correlated.

Validation also passes 288 AI unit tests, four physical destination integration
tests, 39 harness tests and 585 Python tests, plus Rust formatting and strict
Clippy with `--no-deps`. The four cover-retry Python tests were rerun after the
runner clock correction. No runtime changes followed the freeze. Host component
timings are archived separately from work quotas; this is not Pi frame-time
validation, and the conditional timing model was not recalibrated.

**Decision:** retain defaults and do not promote this candidate. Keep the opt-in
model for reproducing the experiment. An extra fresh-match sortie does not
outweigh the failure and progress regressions. A separate follow-up should
investigate whether any reachable alternative has usable cover under current
exposure, including an explicit outcome when none does. Rotating through
rejected sites alone does not supply that evidence. No duration or eligibility
tuning was performed on these results.

The follow-up [cover-alternative diagnostic](capture-cover-alternatives.md)
measures all observed sites without changing the recorded gameplay. It separates
disconnected sheltered routes, sheltered routes that lose on score, missing
shortlist evidence and solar exclusions, providing the boundary for the next
behavior experiment.
