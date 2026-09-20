# Rolling landing estimates and retry clocks

`tools/roll-trip-estimates.py` extends the [frozen trip estimator](bot-trip-estimate.md)
with a causal, read-only tracker for the approach to touchdown. It expires old
cost references when their dependencies change, records renewed estimates and
preserves the original prediction and elapsed mission time. It also reports the
native capture deadline and separate retry limits. Bot decisions are unchanged;
this remains an offline diagnostic.

The accounting is now useful for observing failed commitments. The first
duration model remains coarse: updated estimates help some long retries but can
overestimate a restarted approach already near touchdown. It is not ready to
rank missions or claim improved strategic strength.

## Evidence refresh is separate from approach progress

The tracker distinguishes two kinds of change:

* A site, ground route, terrain revision, objective or vehicle change expires
  the old reference. Missing observations also require new evidence. An unknown
  route never inherits the previous route's numeric cost.
* An observed controller/target restart also starts a new approach clock.
  Material or ground-route evidence can refresh while the ship continues the
  same descent. That refresh preserves elapsed approach time.

Native replan counters expose a retry even when the new site ID equals the old
one. The diagnostic separately records evidence generations, observed plan
transitions and the controller's actual retry counts; they are not equivalent
counts. Clearing a site and then selecting a replacement can produce two
observed plan transitions for one native retry.

The original choice snapshot is immutable. New snapshots require current
evidence under the existing frozen estimator's checks. An unchanged historical
ground-time reference can then age during the same approach; its original source
tick and growing age remain visible. This is a timing assumption, not renewed
physical route validity or permission. Native stale evidence clears the
reference immediately. Unavailable local queries suspend the estimate, and a
gap requires reacquisition without pretending that the gap restarted the flight.

Flag identity is compared in the planet's material frame, using the native
position/range tolerances. Planet rotation does not invent flag motion, and
small successive flag movements cannot continually reset the comparison origin.
After landing, ship loss, leaving the ship, controller failure or mission
reselection, the pre-landing tracker stops. A later telemetry reset cannot
reopen that attempt.

## Conditional timing and independent budgets

The numerical experiment reuses the **unchanged** original calibration profile.
For the selected route category, it retains successful historical landing
durations whose upper boundary has not yet elapsed at the current approach age.
It subtracts elapsed time from the remaining intervals, then reports their
median midpoint and outer envelope. If every sample has elapsed, remaining
landing time becomes unknown rather than zero. Sparse measurement intervals
remain intervals; their midpoints are not treated as exact observations.

The unchanged outbound, claim, return/boarding, exit and departure components
form the remaining surface cost. Total time is time already spent since mission
selection plus remaining landing and surface costs. Elapsed time before the
current plan is retained separately. A new plan cannot erase it.

These are successful-duration references conditioned on still waiting. They do
not estimate completion probability, survival probability or a guaranteed upper
bound. A retry currently reuses the initial-choice landing prior; the dataset
does not establish that its starting pose resembles an initial approach. That
assumption is responsible for some remaining overestimates. Circling distance,
controller phase, foot clearances, descent/lateral speed and visible progress
clocks are recorded for investigating the next model, not fitted into this one.
The tactical approach controller's private progress clock stays explicitly
unavailable.

The capture controller's clock is separate: its first possible time-limit check
is `started_tick + 150 * 60 + 1`, because native code uses a strict `>` test.
Availability guards can defer that check. The diagnostic preserves this source
clock across replanning and reports any observed native clock replacement.
Ordinary, cover and solar retry limits are also separate; live evidence
cancellations do not consume the ordinary retry count. These are the limits in
runtime `4376458`, not general match-time or failure-probability estimates.

The tracker reports whether a numeric remaining trip exceeds the remaining
capture time. An unknown remaining trip leaves that comparison unknown. Actual
failure remains an observed outcome, not a duration inferred from a countdown.

## Replay results and the correction they exposed

The original profile remains byte-identical, SHA-256
`23c9a12bec8eec532f750a1b9b67295cd4dd96380435155b4439503bbc881f06`.
All original predictions in the eight previous evaluation runs also remain
identical, including missing predictions and the failed attempts.

Eight new runs on two new worlds first exposed an accounting error in the
prototype: it restarted the landing clock on any evidence invalidation. On ten
completed paired cases at the 15-second checkpoint, median error worsened from
2.00 seconds for the frozen estimate to 7.84 seconds for that prototype. Traces
showed terrain revisions while the ship kept descending with **zero** native
retries. The final tracker separates reference refresh from approach restart;
tests preserve that distinction. The rejected prototype and its output remain
in the evidence archive.

After that correction, four runs on another previously unseen world provide a
small confirmation set. All runs use the same frozen v11/v10 executable, both
seats and quiet/three-second asteroid conditions, to a match result or the
existing 600-second limit. This is evaluation of predictions, not a comparison
of changed game-playing policies.

| Set | Runs / seeds | Attempts | Completed / abandoned / match end |
| --- | ---: | ---: | ---: |
| Previously recorded evaluation worlds | 8 / 2 | 54 | 30 / 23 / 1 |
| New worlds used to discover the clock bug | 8 / 2 | 44 | 12 / 29 / 3 |
| New confirmation world after the correction | 4 / 1 | 25 | 12 / 12 / 1 |

Comparisons use fixed horizons after the **original** observed landing choice,
not the last convenient forecast before touchdown. The following errors are
paired: both versions must have numeric predictions for the same still-active,
not-yet-landed, subsequently completed attempt.

| Set / time after original choice | Paired completed cases | Frozen median absolute error | Rolling median absolute error |
| --- | ---: | ---: | ---: |
| Previous worlds / 15 s | 26 | 2.42 s | 2.42 s |
| Previous worlds / 30 s | 5 | 22.07 s | 9.61 s |
| Discovery worlds / 15 s | 10 | 2.00 s | 2.97 s |
| Discovery worlds / 30 s | 2 | 23.33 s | 11.12 s |
| Confirmation world / 15 s | 9 | 1.97 s | 1.91 s |
| Confirmation world / 30 s | 1 | 16.07 s | 11.22 s |

These are different, small survivor cohorts at each horizon. They do not form
an accuracy-improvement curve or a general success-rate claim. The JSON also
counts unknown, already-landed and ended attempts at every horizon. For example,
of 17 confirmation choices at 30 seconds, nine attempts have ended, four have
landed, three are unknown and just one has a numeric pre-landing estimate.

The mixed result matters. `confirm0-asteroids3-seat1`, selection tick 6865, has
three native replans; at 15 seconds its frozen error is -1.97 seconds while the rolling
error is +12.82 seconds. It is already in the approach phase, so restarting an
initial-choice time prior is too generous. This requires a model of actual
approach progress, not a claim that refreshing estimates uniformly helps.

The earlier long failure now has an explicit clock trail. Seed
`15377010062155497960`, quiet, P1, selection tick 1202:

| Time after original choice | Total-time estimate | Capture time remaining |
| --- | ---: | ---: |
| 0 s | 30.00 s | 150.02 s |
| 15 s | 41.73 s | 135.02 s |
| 30 s | Unknown surface cost | 120.02 s |
| 60 s | Awaiting current plan evidence | 90.02 s |
| 120 s | Awaiting current plan evidence | 30.02 s |

The controller fails at tick 10517, exactly the recorded first time-limit tick;
the mission acknowledges abandonment at 10518, 155.27 seconds after selection.
Its original 30-second prediction remains visible. Successful phase samples
never turn this into a completed trip or grant another 150 seconds after retry.

All twelve new runs have clean physics/material audits and all 106,049 new
planning rows obey their shared graph/query quotas. Including the eight reused
runs verifies 179,497 rows. No additional world queries, controller work or
planner-budget charges are introduced by an offline replay.

## Tests, evidence and reproduction

73 Python tests pass, including 24 new contracts covering invalidation and
reacquisition, material-frame flag identity, cumulative movement, unchanged
descent clocks, data gaps, censored timing tails, native retry/deadline semantics,
fixed comparison horizons and prediction/outcome separation. Existing CI
discovers the tests. Runtime code and the calibration profile are unchanged.

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/roll-trip-estimates.py \
  --manifest target/bot-rolling-trip-estimate/known-inputs.json \
  --profile target/bot-trip-estimate/calibration/profile.json \
  --out target/bot-rolling-trip-estimate/reproduced-known
python3 tools/roll-trip-estimates.py \
  --manifest target/bot-rolling-trip-estimate/confirmation-inputs.json \
  --profile target/bot-trip-estimate/calibration/profile.json \
  --out target/bot-rolling-trip-estimate/reproduced-confirmation
```

Each replay streams observations through one active tracker per selected seat.
It writes one-second forecasts plus lifecycle/dependency/phase changes before
opening outcome reports. `predictions.json` binds each update file by hash;
`evaluation.json` retains outcomes and fixed-horizon comparisons. Inputs use the
frozen estimator's manifest format and independent-seed checks.

Evidence is under `target/bot-rolling-trip-estimate/`, archived under
`/home/oldman/.codex/visualizations/2026/09/19/bot-rolling-trip-estimate/`.
Restore `bot-trip-estimate` and `bot-outbound-walking` alongside it for the
previous traces, profile and executable. Their archive hashes are respectively
`95da5b15f324900eb8a7e0c370270dd62ae503b51296011619e38e304f055071`
and `e55d62730beb12cbbdd9d752c93ad504b705967dbbed59e93b351a5d93e0e328`.

`run-heldout.py` reproduces the discovery cases; `run-confirmation.py` reproduces
the confirmation cases. Seeds are the first eight SHA-256 bytes, big endian, of
`rolling-landing-estimate-holdout-v1:0`, `:1` and
`rolling-landing-estimate-confirmation-v1:0`, yielding
`8929351061747616468`, `4680780824147604876` and `9335038017350302155`.
`analyze.py` verifies the audits, quotas, unchanged original predictions/profile
and source frozen before confirmation. Its `long-failure.json`,
`phase-errors.json` and `summary.json` retain the specific cases above.

Final outputs are `known-corrected/`, `fresh-corrected/` and
`confirmation-evaluation/`. Earlier prototype outputs and source copies remain
separate. The archive includes exact commands, traces, reports, tests and source
patch, with an adjacent binding to the final commit.

## Next step

Build and test a landing-duration model conditioned on actual progress: circling,
approach, final descent and supported settling, with initial choices and retries
distinguished. Keep both original and rolling predictions, failed attempts and
unknown costs when comparing it. The present clock/invalidation framework is
the reusable part; the restarted-approach duration prior still needs work.

Remote transfer, nontrivial ground travel, powered crossings and pilot exposure
still require separate evidence before complete-trip costs can drive mission
selection.
