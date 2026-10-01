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
