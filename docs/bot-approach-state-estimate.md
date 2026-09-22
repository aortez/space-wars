# Approach-state estimate: first comparison

The first state-conditioned approach model is implemented and compared with the
frozen duration model on **40 new simulations across seven worlds**. It is useful
as an experiment, but **does not justify replacing the existing landing estimate**.

It restores two controlled +15-second forecasts that the duration model cannot
provide, with whole-trip absolute errors of **0.02 and 2.02 seconds**. Both are
from the same world and seat. In normal matches, it loses many initial forecasts
and worsens median error across six paired approach checkpoints at +15 seconds. The approach
component is fairly close on those six cases; the subsequent flight tail is the
larger error. Runtime behavior and strategic selection still use their existing
implementations.

## Candidate and support contract

`tools/estimate-approach-state.py` composes the existing walking regimes with a
new lookup only while the controller is in `approach`. Circling, alignment,
descent and settling retain the phase-duration estimator. Original forecasts,
elapsed time, retries, route validation and capture limits remain separate.

The [flight diagnostic](bot-flight-progress.md) supplies current geometry in the
planet's translating/rotating frame. The candidate distinguishes initial versus
retry approaches, and exact phase entry versus observations with a complete
one-second progress window. It requires observed phase entry; an observation
gap cannot manufacture age or a progress window.

Each query must lie inside every measured feature domain for its cell. A
historical state is locally eligible only when all differences satisfy:

| Feature | Maximum absolute difference |
| --- | ---: |
| Height above suggested ship origin | 12 units |
| Signed sideways error | 3 units |
| Normal velocity | 4 units/s |
| Sideways velocity | 4 units/s |
| Normal gravity | 4 units/s² |
| Sideways gravity | 3 units/s² |
| Distance closed in preceding second, for progress queries | 4 units |

These are explicit development choices, not bounds derived from validation.
Missing gravity is unsupported. Between exact entry and a complete progress
window, the candidate returns unknown. It does not use elapsed phase age as a
duration cutoff, divide distance by current speed, extrapolate beyond measured
domains, or fall back to the duration estimate for an unsupported approach.

Among eligible states, normalized squared feature distance selects one nearest
state per attempt, with source tick breaking ties. At least **three distinct
attempts from two worlds** are required. Approach-remaining time and the
post-approach flight tail are each median-aggregated within a world and then
across worlds. Their sum is the landing estimate. Repeated frames and repeated
attempts in one world cannot dominate merely through sample count. Component
min/max ranges are summed for a descriptive envelope; it is not a confidence
interval, deadline or physical permission.

Training accepts observed, completed approach episodes followed by an
uninterrupted native landing acknowledgement. Interrupted/censored episodes
remain exclusions. A later ground failure does not erase a completed flight.
The existing 56 recordings are explicitly development data: 16 original flight
training matches, 16 latest normal matches and 24 latest controlled trials,
covering eleven worlds. Their 704 retained states are:

| Cell | States | Attempts | Worlds |
| --- | ---: | ---: | ---: |
| Initial / entry | 57 | 57 | 11 |
| Initial / progress | 546 | 57 | 11 |
| Retry / entry | 13 | 13 | 6 |
| Retry / progress | 88 | 13 | 6 |

There are 55 excluded approach episodes. Per-cell capacity is 2,048 states,
with at most 64 states from one attempt in each cell. Height buckets restrict
the lookup to three adjacent buckets; no tree search or forward physics solve
is introduced. Sampled validation updates examine a median 77 states and a
maximum 368. These are offline operation counts, not Pi timing measurements or
a demonstrated runtime quota implementation.

## New-world protocol and correction

The rule, coefficients/tolerances, profile, source hashes, binaries, seeds,
conditions and comparisons were recorded before generation in `design.json`.
New seeds are disjoint from 50 prior worlds. Normal trials use four worlds,
both v11 seats against v10, asteroid intervals zero/three seconds, and a
600-second maximum. Controlled trials use three different worlds, both seats,
the existing fixed-side exit, offset +0.6, direction −1, four requested route
bands [2,6), [8,20), [20,40), [40,60), and a 180-second maximum including setup.
There are no replacement seeds or outcome-dependent stopping.

One bookkeeping correction was made **after generation, before prediction
replay or outcome inspection**. A synthetic test showed that the standalone
diagnostic could reset phase context when capture restarted within the same
mission, incorrectly allowing initial evidence for a retry. The composing
estimator now uses its existing authoritative phase clock and retains the reset
progress window. `clock-correction.json` records both source hashes and timing;
the original source is preserved. The rule/profile are unchanged. This is not
an exact pre-generation source freeze of the final implementation, and the
correction is not concealed as a model improvement.

Normal totals start at mission selection; controlled totals start at capture
entry. Fixed checkpoints remain 0, 15, 30, 60 and 120 seconds after the first
observed landing choice. Outcome reports are opened only after forecasts are
written and hashed. Evaluation rejects training-world overlap with all four
input profiles and checks trace/report/source bindings.

| Cohort | Recordings | Observed attempts / results |
| --- | ---: | --- |
| Normal | 16 | 109 attempts: 47 completed, 60 abandoned, 2 match ended |
| Controlled | 24 | 20 attempts; trial outcomes: 2 completed, 6 ship lost, 9 frame changed, 4 wrong-planet setups, 3 unavailable setups |

Controlled attempt endings are 2 completed, 6 ship lost and 12 frame changed.
Trial setup classification and observed attempt ending are separate denominators;
some wrong-planet trials contain capture telemetry before the frame check ends
the attempt. Thirty-eight normal and nine controlled attempts have no observed
landing choice. All failed and unavailable trials remain in the evidence.

## Coverage and paired timing

Numeric whole-trip forecasts, duration comparator → state candidate:

| Cohort / checkpoint | Duration | State | Gained / lost |
| --- | ---: | ---: | ---: |
| Normal / first choice | 66 | 41 | 0 / 25 |
| Normal / +15 s | 34 | 30 | 0 / 4 |
| Normal / +30 s | 4 | 4 | 0 / 0 |
| Normal / +60 s | 1 | 0 | 0 / 1 |
| Controlled / first choice | 10 | 8 | 0 / 2 |
| Controlled / +15 s | 0 | 2 | 2 / 0 |
| Controlled / +30 s | 1 | 1 | 0 / 0 |

There are no numeric +120-second forecasts. Of the 25 lost initial normal
forecasts, **18 eventually complete** and seven are abandoned. Eighteen losses
lack enough locally similar attempts/worlds; seven exceed a measured feature
domain. Coverage loss cannot be reported as better failure detection.

Unchanged phases dominate aggregate error medians: on the 28 shared completed
normal +15-second checkpoints, both medians are 0.50 seconds. Looking specifically
at the changed approach phase reveals the regression:

| Normal approach checkpoint | Paired completed | Duration median whole-trip error | State median whole-trip error |
| --- | ---: | ---: | ---: |
| First choice | 4 | 1.47 s | 1.61 s |
| +15 s | 6 | 0.84 s | 1.91 s |

For those six +15-second cases, the candidate's median absolute error is
**0.30 seconds for remaining approach**, but **1.86 seconds for the later flight
tail**. Landing-component median error worsens from 1.25 to 1.75 seconds. The
first-choice approach mean improves from 3.29 to 2.81 seconds on four completions,
but neither this tiny mean comparison nor the unchanged aggregate medians offset
the large coverage loss.

Only two controlled trials complete, both in world one, seat one. At +15 seconds
their approach ages are 15.0 and 11.6 seconds and the duration model has no surviving
episode. The state model gains both, supported by 28 attempts/nine training
worlds and 15 attempts/six training worlds respectively. Whole-trip errors are
−0.024 and −2.022 seconds; approach-component errors are −0.008 and +0.263 seconds.
This is a useful support-recovery example, not broad validation of high or
receding arrivals. No paired timing improvement exists at that checkpoint
because the comparator is unknown.

## Cases to revisit

`selected-cases.json` preserves these checkpoint states, component errors and
outcome timelines:

- **Recovered short controlled trip:** `controlled/world1-band2-6-dir-1-seat1`,
  capture start 795, checkpoint **1722**, landing 2286. Height 18.64, closing
  speed 4.22, one-second closure 2.58. Whole-trip error −0.024 seconds. A plan
  restart at 2283 still occurs shortly before landing; this result does not
  establish interruption prediction.
- **Recovered moderate controlled trip:** `controlled/world1-band8-20-dir-1-seat1`,
  checkpoint **1722**, landing 2674. Height 51.28, closure 12.10 units in one
  second. The approach component overestimates by 0.263 seconds while the later
  flight tail underestimates by 1.99 seconds; there is no later phase break.
- **Normal tail regression:** `normal/world0-asteroids0-seat1`, selection 3953,
  checkpoint **5204**, landing 6021. Whole-trip error worsens −1.33 → −3.40
  seconds. Of the −3.43-second landing error, −0.72 is approach and −2.72 is
  later flight. Support includes eleven attempts/seven worlds, so increasing
  sample count alone does not address this miss.
- **Unchanged interruption miss:** `normal/world0-asteroids3-seat0`, selection
  8701, checkpoint **10184**, landing 12801. The current phase is descent, and
  both models retain the same −41.07-second whole-trip error. Six later phase
  breaks occur at 10766, 10800, 10910, 10950, 12125 and 12150. This is outside
  the new approach-only model and keeps interruption risk on the roadmap.

## Decision and next step

Retain the candidate as an explicit offline comparison; do not promote it as a
replacement landing estimator. The next bounded experiment should test a
declared selection rule that **keeps supported duration forecasts and consults
state evidence when duration support expires**. Unsupported state evidence
would still remain unknown. This would be a new composed model, not a silent
fallback added to the strict candidate after seeing its results.

The [duration-first expiry experiment](bot-approach-expiry.md) now implements
that separate rule. Forty further simulations preserve every original numeric
forecast and add six controlled +15-second estimates across three new worlds,
with 0.98-second median whole-trip error. It remains offline; large interrupted
normal approaches are still underestimated.

Separately investigate the later flight tail using saved handoff, alignment and
contact states; the present state features do not include ship heading/spin or
forecast a future handoff pose. Do not widen tolerances or tune the tail on this
validation set and then call the same outcomes independent validation. Controlled
setup failures also limit useful coverage; retain them while improving fixture
diagnostics if the next declared matrix again cannot exercise the intended
arrival states. Remote transfer and interruption/completion risk still precede
strategic mission selection using these costs.

## Verification and reproduction

All **212 Python tests pass**, including 21 new tests. They cover independent
attempt/world support, local versus global domains, signed state support,
nearest-state selection, component accounting, observed clocks, expired
duration support, retries/capture restarts, stale ground, native limits,
terminal observations and censored training outcomes.

All 40 engine processes exit successfully. Physical audits and **157,389 quota
rows** pass across **7,894.8 simulated seconds**. Across **4,023 common sampled
updates**, the duration comparator, originals, ground costs, phase histories,
elapsed time and native limits are unchanged. All **3,469 non-approach updates**
retain exact forecasts/status/envelopes. The estimated-cost comparison against
the remaining time limit may change; the native deadline does not change.

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/estimate-approach-state.py calibrate \
  --manifest target/bot-approach-state/training-inputs.json \
  --out target/bot-approach-state/reproduced-calibration
python3 tools/estimate-approach-state.py evaluate \
  --manifest target/bot-approach-state/normal/inputs.json \
  --source-evaluation target/bot-approach-state/normal/baseline/evaluation.json \
  --ground-profile target/bot-trip-estimate/calibration/profile.json \
  --phase-profile target/bot-phase-landing-estimate/calibration/profile.json \
  --walking-profile target/bot-walking-calibration/fit/profile.json \
  --state-profile target/bot-approach-state/calibration/profile.json \
  --out target/bot-approach-state/reproduced-normal
```

Evidence is under `target/bot-approach-state/`, archived under
`/home/oldman/.codex/visualizations/2026/09/21/bot-approach-state/`. It includes
the declared generation protocol, frozen profile/source hashes, correction,
new reports/traces/quotas, predictions, comparisons, development checks and
source patch. The archive manifest identifies the earlier diagnostic, profile
and runtime dependencies. Adjacent metadata binds the verified archive to the
local commit. Sensor logs are omitted; the necessary dense traces are included.
