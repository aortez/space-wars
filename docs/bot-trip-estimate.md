# First read-only capture-trip estimate

The [recorded phase analysis](bot-trip-calibration.md) now has a predictive
companion: `tools/estimate-trip-costs.py`. It freezes a diagnostic estimate when
the trace first shows a selected landing site, then compares it with the later
execution. This is an offline tool using the existing observation and telemetry
contracts. It adds no game-loop work and does not change bot decisions.

The first independent-world comparison has 29 completed numeric predictions:
median absolute error is 2.48 seconds, with a worst underestimate of 25.24
seconds. Landing replans dominate the large misses. Those numbers mostly cover
trips with very short ground travel; they do not establish a general model for
long walks or powered crossings.

## Prediction boundary and model

Each normal mission attempt gets a snapshot at its first recorded observation,
anchored to the past `selected` event. An unmeasured remote trip stays unknown.
Its first observed local landing choice gets a second, frozen snapshot while
the pilot is still aboard a full ship and before recorded touchdown. A missing
route at that point stays missing in that prediction, even if a later survey
would make it available. The snapshot tick can be later than the actual choice
when reading a sparse recording; the prediction's clock starts at the observed
tick and does not claim otherwise.

The total consists of elapsed time since mission selection plus estimates for
six remaining phases:

| Phase | Reference and empirical adjustment |
| --- | --- |
| Approach and landing still remaining | Historical duration from observed site choice to touchdown |
| Exit | Historical touchdown-to-exit duration |
| Outbound | Existing route score in seconds plus historical execution residual |
| Claim | Full uninterrupted claim stages plus historical execution residual |
| Return and boarding | Existing return score in seconds plus historical execution residual |
| Departure | Historical boarding-acknowledgement-to-departure duration |

Observed transfer and local approach time before the choice are also recorded
separately when the past arrival event is present. They are counted once in the
elapsed prefix. The tool does **not** yet predict a complete trip from a remote
mission-selection point: remote route and landing information are still absent.

For a planet without an existing flag, the old score has no ground term. The
new model uses empirical outbound and return durations for that separate
category, rather than interpreting the absent term as zero travel time.
Walking, jumping and powered crossings have separate calibration categories.
Missing phase samples or a route/claim reference outside its sampled domain
leave the complete total unknown; available components remain visible.

Calibration records the median midpoint of measured duration/residual intervals
and their outer min/max envelope. It accepts completed phase boundaries at most
one second wide, preserving the sparse-observation uncertainty from the existing
analyzer. A phase may contribute even if a later phase fails. Incomplete phases,
wide boundaries, incompatible evidence and phases following an observed site,
route, terrain revision or ship-form change remain recorded as exclusions.

Adding the phase medians gives the point reference. Adding their envelope ends
gives a **historical envelope**, not a confidence interval, guaranteed bound or
completion deadline. No probability of surviving or finishing is inferred.
Samples from the same world are correlated; sample counts and distinct source
seeds are retained for each phase.

## Evidence and uncertainty

Snapshots retain the selected site, material revision, claim/flag observation,
planet motion/radius, observed gravity and vehicle form. Route source and
validation ticks stay distinct: revalidation does not reset source age. The
prototype rejects missing, future, expired, unvalidated or incompatible route
evidence at the choice. These structural checks do not repeat native physical
validation or grant landing, traversal, claim or boarding permission.

The source observation's ship-centered nearby/unoccluded-opponent condition is
recorded independently. Missing queries or an absent target stay unknown.
Future ship exposure and pilot visibility are unmeasured. They are not converted
into a risk score or a claim that the route is safe.

Evaluation retains dependency changes after the frozen prediction. It reports
both the original estimate's error and whether dependencies subsequently
changed. Predictions are written before opening the evaluation outcome reports;
later observations and outcomes live in the evaluation output, not the frozen
prediction file. Prefix-invariance tests check that appending a better route or
other later observations cannot change an earlier estimate.

## Calibration and independent-world results

All inputs use runtime `4376458`, including both-leg continuous walking. The
profile uses the preceding outbound-walking matrix: 16 runs across four seeds,
restricted to their v11 seat. Its 85 attempts include 35 completed, 48 abandoned
and two cut off by match end. The dense quiet recording replaces its sparse
duplicate; it is not counted twice. Most usable phase cells are small:
15 no-flag examples, seven walking examples for landing/exit/outbound, and five
walking examples for claim/return/departure. No usable jumping or crossing
calibration is available.

The frozen profile was then evaluated against eight new runs: two new seeds,
both v11 seats against v10, with quiet and three-second asteroid conditions.
Seeds come from the first eight SHA-256 bytes, big endian, of
`whole-trip-estimate-holdout-v1:0` and `:1`:
`15377010062155497960` and `17392165983706107741`. These recordings are dense and
run to a match result or the existing 600-second limit.

| Measurement | Result |
| --- | ---: |
| Recorded v11 attempts | 54 |
| Completed / abandoned / match end | 30 / 23 / 1 |
| Attempts without an observed pre-landing choice | 16 |
| Numeric local-choice estimates | 36 |
| Completed numeric comparisons | 29 |
| Median / mean absolute total error | 2.48 / 5.48 s |
| Worst underestimate / overestimate | 25.24 / 5.97 s |
| Completed durations inside historical envelopes | 23 of 29 |
| Completed comparisons with unchanged dependencies | 14 |
| Their median absolute error | 2.42 s |

Two powered-crossing choices remain unknown; one completes and one reaches
match end. Of the other completed choices, only two have a positive outbound
route reference, both below 0.17 seconds, and all have zero return-route length.
The very small ground/boarding errors therefore primarily exercise capturing
and boarding near the hatch. The earlier long-walk success is calibration data,
not an independent long-walk validation result.

The four completed underestimates exceeding ten seconds all occur under asteroid
pressure and have later terrain/site/route changes. Landing alone accounts for
14.72–24.93 seconds of their error. For example, seed 0, asteroid pressure, P1,
selection tick 15260 is estimated at 51.06 seconds and takes 76.30 seconds;
24.33 seconds of the 25.24-second underestimate comes from landing. Keep the
original prediction visible after invalidation rather than measuring only the
final successful approach.

A quiet attempt on seed 0, P1, selection tick 1202 is estimated at 30.00 seconds
but abandons at 155.27 seconds after exhausting the capture approach budget.
Its selected sites/routes change repeatedly. This is a censored failure, not a
155-second completed-trip sample. Fitting only successful phase durations cannot
predict the time spent in such a failed commitment.

All eight runs finish with clean physics/material audits. All 73,448 recorded
live-planning rows obey their shared graph/query quotas. These runs use the same
frozen executable as the prior checkpoint; no candidate controller is being
compared and these outcomes imply no win-rate improvement.

## Tests and reproduction

49 Python tests pass, including 26 new contracts for causal snapshots, timing
origins, evidence age, missing/expired evidence, no-flag costs, domain limits,
dependency changes, censored results, independent-world splits and CLI output.
The existing CI tools-test step discovers them. Runtime code is unchanged, so
the physical runs reuse the already verified executable instead of rebuilding
or repeating the unrelated Rust test suites.

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/estimate-trip-costs.py calibrate \
  --manifest target/bot-trip-estimate/training-inputs.json \
  --out target/bot-trip-estimate/reproduced-calibration
python3 tools/estimate-trip-costs.py evaluate \
  --manifest target/bot-trip-estimate/heldout-inputs.json \
  --profile target/bot-trip-estimate/reproduced-calibration/profile.json \
  --out target/bot-trip-estimate/reproduced-evaluation
```

A manifest uses the existing analyzer's version-1 run entries and adds an
explicit top-level `runtime_revision` and per-run `seats`. Each `source_commit`
must match that frozen revision; `physics_valid_from_tick` must be zero and the
scope must be normal missions. Only the v11 reference policy is accepted.
Evaluation rejects seeds or report/trace hashes found in the calibration set.

Evidence is retained in `target/bot-trip-estimate/`, with an archive under
`/home/oldman/.codex/visualizations/2026/09/19/bot-trip-estimate/`. Restore the
preceding `bot-outbound-walking` archive alongside it for the calibration inputs
and executable. That archive's SHA-256 is
`e55d62730beb12cbbdd9d752c93ad504b705967dbbed59e93b351a5d93e0e328`.
The executable SHA-256 is
`21d634428b86d3a4e5702af779b7b21fe8c4c842c4fe13f0ef12ee5e3e23a9de`.

`run-heldout.py` records exact commands and reproduces the eight fresh runs;
`summarize.py` verifies audits/quotas and records the larger errors. The final
tool output is under `final-evaluation/`. The archive also retains input hashes,
the frozen profile, source patch, test log and the initial comparison. A final
provenance/reporting check reproduces the profile byte-for-byte and leaves every
prediction unchanged. The profile SHA-256 is
`23c9a12bec8eec532f750a1b9b67295cd4dd96380435155b4439503bbc881f06`.

## Next step

The [rolling landing diagnostic](bot-rolling-trip-estimate.md) now expires
references on dependency changes, preserves elapsed retry time and exposes the
native capture deadline separately. It retains the original predictions and
compares updates at fixed horizons. The subsequent [phase-aware model](bot-phase-landing-estimate.md)
conditions updates on observed approach progress; interrupted approaches remain
a separate source of error.

The [long-ground validation](bot-long-ground-validation.md) finds that random
matches leave substantial walking accuracy unproven, with posture and timeout
failures among the few long choices. Controlled moderate-distance trips and
a remote-transfer estimate remain necessary before ranking missions. Keep those
missing costs,
pilot exposure and completion probability explicit. This completes the first
local complete-trip accounting slice; strategic selection remains a later step.
