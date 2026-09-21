# Short and moderate walking-cost regimes

The explicit walking rule restores the coverage lost by strict affine
composition. In **16 new normal matches**, first-choice numeric forecasts rise
from **27 to 47**, and +15-second forecasts from **21 to 29**, without losing or
changing any previously numeric strict forecast. Restored short-walk estimates
use the earlier model's measured support.

In a separate **24-trial controlled set**, 11 completed first-choice whole-trip
comparisons reduce median error from **4.35 to 1.74 seconds** versus phase-aware
landing with the original ground costs. This retains the moderate-walk formula
tested previously; it is not a new fit. Landing uncertainty remains the largest
coverage and error limitation.

This offline diagnostic is selected with `--walk-model short-and-affine` in
`tools/compose-trip-estimates.py`. Bot controls, physics, mission selection,
production defaults and planning quotas are unchanged. The default `affine`
option preserves the [strict comparator](bot-composed-trip-estimate.md).

## Frozen selection rule

For each leg of a valid pure-walk anchor:

1. Below its frozen affine minimum, use the original empirical estimate with
   its own domain, sample availability and snapshot-validity checks.
2. Within the affine domain, including both endpoints, use the frozen affine fit.
3. Otherwise retain an unknown cost. Longer legacy support, if supplied in a
   different profile, cannot extend the affine upper bound.

The boundary comes from existing training data. No coefficients, domains, support
thresholds or landing cells change. Reference time is prospective route length
divided by five; each leg selects independently.

| Leg | Short reference, within legacy support | Affine reference | Affine distance |
| --- | --- | --- | --- |
| Outbound | [0, 1.520) s | [1.520, 10.893] s | 7.600–54.464 units |
| Return/board | [0, 1.403) s | [1.403, 10.386] s | 7.015–51.931 units |

These displayed bounds are rounded; code uses the exact profile values. A missing
affine fit cannot invent a boundary or enable a legacy fallback. A gap below its
minimum remains unknown if the original model lacks support there. Invalid/stale
evidence, unavailable queries and native capture limits keep their existing
guards. Nonwalking categories, including separately measured `no_flag`, are unchanged.

Each forecast records each leg's regime, model, reference, estimate and unknown
reasons. Revalidated evidence can change the current regime without rewriting
the first forecast or resetting elapsed time. The hard boundaries introduce
steps of approximately **+0.65 seconds** outbound and **+0.45 seconds** return.
There is no validation-tuned blend. Future ranking must not treat tiny differences
across these boundaries as precise physical advantages. Historical envelopes
remain descriptive, not deadlines, confidence intervals or success probabilities.

## Independent validation protocol

The rule, source/profile hashes, runtime binaries, seed domains, conditions and
comparisons are frozen before generating any new recording. All seven new worlds
are disjoint from the 43 prior worlds in this experiment series. Prefix-only
replay uses no refitting or outcome-dependent selection. The separate predeclared
generation protocol makes these recordings new validation data; replay alone
would not establish independence.

Normal matches use four worlds, both v11 seats against v10, asteroid intervals
zero and three seconds, and a 600-second maximum. Controlled trials use three
different worlds, both seats, and requested route bands **[2, 6)**, **[8, 20)**,
**[20, 40)** and **[40, 60)** units. They retain the fixed-side exit, direction `-1`,
offset `+0.6`, no combat/asteroids, and a 180-second limit including preparation.
They do not establish opposite-direction exit symmetry.

| Cohort | Recordings / worlds | Outcomes |
| --- | ---: | --- |
| Normal | 16 / 4 | 35 completed attempts, 31 abandoned, 4 match ended |
| Controlled | 24 / 3 | 11 completed, 1 abandoned, 2 ship losses, 3 frame changes, 7 unavailable setups |

There are **87 observed attempts**. Seven additional trials never start capture
and remain in the trial denominator. Twenty-two normal and two controlled
attempts have no observed landing choice. No replacement worlds are sampled.

Normal totals start at mission selection; controlled totals start at capture
entry, excluding preparation and remote transfer. Checkpoints are 0, 15, 30, 60
and 120 seconds after the first observed landing choice, including retries.
Each scope retains its existing outcome and departure boundaries.

## Coverage and errors

Normal coverage now matches the phase-only model at every checkpoint:

| Checkpoint | Strict affine numeric | Regime numeric | Newly covered completed / unfinished |
| --- | ---: | ---: | ---: |
| First choice | 27 | 47 | 13 / 7 |
| +15 s | 21 | 29 | 7 / 1 |
| +30 s | 1 | 3 | 1 / 1 |
| +60 s | 0 | 1 | 0 / 1 |
| +120 s | 0 | 0 | 0 / 0 |

Restored coverage does not turn failures into successful timing comparisons.
The +60-second forecast belongs to an abandoned attempt. Completed normal
forecasts have median error **3.98 seconds** at first choice (35 comparisons)
and **1.06 seconds** at +15 seconds (27), exactly matching phase-only. The
original frozen estimator's initial median is **3.09 seconds** on the same 35
completions; initial flight prediction still does not consistently improve it.

All numeric normal forecasts here are `no_flag` or short/short. These matches
do not validate ordinary-game moderate or mixed-regime sorties. Controlled
trials supply moderate walking; mixed-leg boundaries have unit coverage but
are not a new successful physical cohort in this experiment.

Controlled whole-trip errors, paired with phase-only on identical rows:

| Checkpoint | Paired completions | Phase-only median | Regime median |
| --- | ---: | ---: | ---: |
| First choice | 11 | 4.35 s | 1.74 s |
| +15 s | 4 | 3.82 s | 0.75 s |
| +30 s | 1 | 2.01 s | 2.01 s |

The ten affine/affine initial comparisons improve **4.21 to 1.73 seconds**; their
points are identical to strict composition. One restored short/short completion
retains the phase-only **9.34-second** underestimate. The regime model's full
initial mean is therefore **3.26 seconds**, versus **2.65 seconds** for strict
composition's narrower ten-row set. Unpaired means would hide this coverage
difference. Phase-only's paired mean across all 11 is **4.77 seconds**.

At +15 seconds only four controlled forecasts remain numeric: nine choices lack
surviving flight-phase calibration, one has an unsupported ground leg, and one
attempt has ended. Those four are completed, with **1.30-second mean** error and
a **3.46-second** largest underestimate. There are no completed numeric comparisons
at +60/+120 seconds.

## Investigation cases

`diagnostic-cases.json` retains attempts, checkpoint updates, budgets and
landing/tail error decomposition. `unknowns-at-15s.json` retains every unknown
checkpoint, including successful trips whose forecasts ran out of support.

- **Normal flight miss:** `normal/world3-asteroids0-seat0`, selection 1, choice
  **607**. Initial total error is **−22.93 seconds**: **−22.83** landing, **−0.10**
  post-landing tail. At **1507**, retry/circling has only two historical attempts
  supporting its estimate; total error remains **−7.84 seconds**. Inspect initial
  site-relative position/velocity and restarts over **607–1507**, then the approach
  at **2407**. Zero asteroid interval does not imply zero combat.
- **Restored short-trip miss:** `controlled/world1-band2-6-dir-1-seat1`, capture
  start 375, choice **418**. Its **−9.34-second** initial error combines landing
  **−9.47** with tail **+0.12**. At **1318**, initial approach age is 11.35 seconds
  and no historical episode survives that age. The estimate becomes unknown,
  then resumes in alignment at **2218**. The short-walk cost is not the main issue.
- **Error cancellation:** `controlled/world0-band8-20-dir-1-seat1`, choice **416**.
  Initial landing overestimates by **5.95 seconds** while the tail underestimates
  by **0.75**. Improving the moderate-walk tail worsens initial total error versus
  phase-only, **3.91 to 5.20 seconds**. At **1316**, flight progress reduces total
  error to **0.39 seconds**. Do not tune ground costs to cancel flight error.
- **Unsupported failed return:** `controlled/world0-band40-60-dir-1-seat1`, choice
  **416**, return reference **10.495 seconds**, beyond the frozen 10.386 maximum.
  Landing 1627 and claiming 2733 succeed, but return progress fails at **3630**
  without boarding. Neither model gives a full numeric forecast. A requested
  40–60-unit band is not permission to extrapolate the fit.

Controlled support gaps mostly involve initial approach ages around 10.4–15
seconds. Widening duration support using these validation outcomes would be a
new model change; these are now known diagnostic cases for that experiment.

## Verification and reproduction

All **178 Python tests pass**. New tests cover boundaries, per-leg selection,
missing support, invalid/stale evidence, no long-range fallback, immutable first
forecasts, revalidation, retry clocks, CLI selection and unstarted setup identity.
Across **2,357 common sampled updates**, all **893 strict numeric forecasts** retain
their exact points/envelopes and **427 numeric short/short forecasts** match
phase-only exactly. Original forecasts, flight histories, native clocks, outcomes
and invalidations are unchanged.

All 40 simulations exit successfully. Their 6,940.53 simulated seconds pass
physical audits and **107,670 execution quota checks**. Parallel desktop timings
are not a Pi performance benchmark.

Four bands for controlled world zero, seat zero, produce identical preparation
traces with no capture attempts. The initial evaluation guard rejects them as
duplicate recordings. Its correction distinguishes unstarted trials by setup
configuration; started captures and normal missions keep strict duplicate checks.
These are four requested conditions on one world/seat, not four independent
trajectories. Every controlled forecast/update file for both models is
byte-identical after correction. Predictor ASTs and the prediction loop match the
pre-generation freeze. The only other reporting change clarifies that replay
alone does not prove independent validation.

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/compose-trip-estimates.py \
  --manifest target/bot-walking-regimes/normal/inputs.json \
  --source-evaluation target/bot-walking-regimes/normal/baseline/evaluation.json \
  --ground-profile target/bot-trip-estimate/calibration/profile.json \
  --phase-profile target/bot-phase-landing-estimate/calibration/profile.json \
  --walking-profile target/bot-walking-calibration/fit/profile.json \
  --walk-model short-and-affine \
  --out target/bot-walking-regimes/reproduced-normal
```

Evidence is under `target/bot-walking-regimes/`, archived at
`/home/oldman/.codex/visualizations/2026/09/21/bot-walking-regimes/`.
`design.json`, `started.json` and `run-matrix.py` preserve the generation protocol;
`evaluate.py`, `adapter-correction.json`, `analyze.py`, `checks.json`,
`checkpoint-rows.json` and `inspect-cases.py` preserve comparisons and verification.
Authoritative controlled outputs are `strict-corrected/` and `regimes-corrected/`;
initial failures remain alongside them. Reports, traces, quota files, commands,
frozen profiles and the source patch are archived. Adjacent metadata binds the
verified archive to the commit; earlier runtime/fixture archives are dependencies.

## Next step

Keep this as the supported read-only ground-cost interface. Further walking-fit
tuning is not the next useful step. Investigate the saved initial/retry flight
misses and missing support using site-relative height, lateral error, velocity
and progress. Distinguish ordinary approach time from interruptions or stalled
control, retaining unknown tails. Any resulting estimator needs a new declared
comparison and new validation worlds. Remote transfer and completion/interruption
risk still precede strategic mission selection using these conditional costs.
