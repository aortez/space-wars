# Combining coarse and clearance descent estimates

The explicit `supported-clearance` option uses a supported clearance estimate
and otherwise retains an eligible coarse duration. This addresses the early
coverage loss found in the [strict-clearance comparison](bot-descent-clearance.md).
The strict option stays the default for that tool. Both models remain offline;
this does not change bot controls, physical permissions or Pi deployment.
The subsequent [interruption-risk investigation](bot-landing-interruption-risk.md)
finds useful recent-replan evidence in 216 known recordings, while retaining
healthy descents interrupted by their first fresh route invalidation. It defines
the next offline comparison without changing this timing baseline.

## Selection rule

The five profiles and all clearance features, residuals, support tolerances and
training samples are unchanged. `DescentRegimeTrip` runs the previous comparator
and strict estimator before choosing an output. It never feeds the selection
back into either estimator's clocks, anchors, deadlines or immutable first
forecast.

1. Outside an eligible descent forecast, keep the previous comparator.
2. Keep a supported strict-clearance forecast exactly.
3. If clearance lacks a calibration cell, measured-domain support or enough local
   examples, or assist is weak/still starting, retain the independently numeric
   coarse forecast when all other evidence and budget guards pass.
4. Otherwise remain unknown.

The entire coarse forecast is restored together: landing/remaining/total time,
empirical envelopes and its deadline comparison. The strict forecast, original
clearance rejection and selection reason remain available separately. There is
no timer, blending, delayed commitment or latch; changing support can change the
selected model on the next observation.

Fallback does not apply to missing or invalid rays, negative/no-hit clearance,
missing motion, a stale/ambiguous site, incompatible target/contact, unobserved
phase entry or invalid assist values. A valid native `flying` or weak `assisted`
state can retain a coarse duration; another native phase or assist outside
[0,1] cannot. Missing/expired coarse support cannot be reconstructed, and ground
or native capture-budget guards cannot be repaired by either timing model.

`--descent-model strict` preserves the previous strict model and profile format.
`--descent-model supported-clearance` selects the new rule and records it in
provenance. This is an estimator choice, not a new ship-control policy.

## Development and frozen comparison

A selection-only development check uses the prior forty recordings' hash-bound
strict forecasts. It restores **151 sampled updates: 117 normal and 34
controlled**.
All previously numeric strict results are retained. The restoration reasons are
calibration-domain/local-support misses or ordinary assist transients; there is
no refit or new-world accuracy claim from this development replay.

The rule, helper code, tests, drivers, five profiles and native binaries were
hashed before forty new simulations on seven new worlds, excluding all 71
previously used world seeds. Runtime remains
`43764580869c1ad56e6b7e9c4b7a21f485496220`.

- Sixteen normal v11-versus-v10 matches: four worlds, both seat arrangements,
  asteroid intervals zero and three seconds, at most 600 simulated seconds.
- Twenty-four controlled trials: three worlds, both seats, walking bands
  [2,6), [8,20), [20,40), [40,60), direction −1, fixed side-zero exit, offset
  +0.6, at most 180 seconds including physical setup. Unavailable setups stay
  in the trial denominator.

Three independent replays compare the previous coarse/approach-expiry model,
strict clearance and the composed rule. Forecasts are closed and hashed before
reading outcome evaluations. All five profiles require disjoint worlds and
recordings. Primary checkpoints are first-choice +0/+15 seconds and the first
observed descent entry. The +1/+3 descent samples use the first fixed one-second
grid point at least that far into the same first descent episode; ended and
unobserved episodes remain explicit. Additional +30/+60/+120 choice checkpoints
and future interruptions remain in the outputs.

Sampling still uses fixed one-second points plus phase/lifecycle/evidence
changes. Composing a formerly unknown result can remove an extra status-change
sample, so those event-only missing timestamps are counted separately. All
fixed checkpoints and descent-entry samples are compared directly; a missing
event-only sample is not treated as a numeric forecast or a coverage gain.

## Results and decision

The forty simulations pass physical audits and finish without errors, totaling
**6,915.62 simulated seconds**. All **115,873 planner quota rows** stay within
their graph and query allowances. Normal runs retain 68 attempts: 28 completed,
36 abandoned and four ending with the match; 26 never acquire a landing choice.
Controlled trials retain eleven completed trips, one frame departure, three
ship losses and nine unavailable setups. Three observed controlled attempts
never acquire a landing choice.

**No coarse numeric estimates are lost at any declared checkpoint.** At first
descent entry the combined model restores all eighteen normal and three
controlled forecasts withheld by strict clearance. Three normal estimates gain
coverage over coarse at the +1/+3 descent observations; these are the unchanged
strict model's gains. All numeric strict forecasts at these checkpoints remain
exactly unchanged.

The table reports whole-trip numeric coverage and paired exact remaining
landing-time error. A landing component can be available while the whole trip
has an unsupported ground cost, so paired landings can outnumber numeric trips.

| Scope / observation | Numeric trips: coarse / strict / combined | Paired landings | Median absolute landing error: coarse → combined |
| --- | ---: | ---: | ---: |
| Normal, descent entry | 30 / 12 / 30 | 30 | 0.84 → 0.58 s |
| Normal, descent +1 grid | 26 / 29 / 29 | 26 | 0.70 → 0.19 s |
| Normal, descent +3 grid | 26 / 27 / 29 | 26 | 0.70 → 0.11 s |
| Controlled, descent entry | 10 / 7 / 10 | 11 | 0.92 → 0.72 s |
| Controlled, descent +1 grid | 10 / 10 / 10 | 11 | 0.89 → 0.52 s |
| Controlled, descent +3 grid | 10 / 9 / 10 | 11 | 0.88 → 0.34 s |

At first-choice +15 seconds, normal numeric coverage stays 29. On 28 paired
landings, median absolute landing-time error improves **0.78 → 0.13 seconds**;
on 27 paired completed trips, whole-trip error improves **0.57 → 0.14 seconds**.
Controlled coverage stays ten and its ten paired whole-trip comparisons retain
the same **0.92-second median**. Initial first-choice forecasts are unchanged.

These are descriptive attempt statistics across four normal and three controlled
worlds, not confidence intervals. At descent +1, each normal world's paired
landing median improves, while one controlled world regresses **0.31 → 0.42
seconds**. Keeping the supported strict model also keeps its individual misses;
composition does not guarantee improvement in every world or transient.

The audit verifies 2,140 timestamps shared with the independent strict replay,
including **1,087 numeric strict forecasts preserved exactly**. At all 35
restored sampled updates (31 normal, four controlled), every coarse forecast,
envelope and deadline field matches exactly. The coarse comparator matches at
2,114 common timestamps; all 1,790 unaffected sampled updates retain its output.
Selections, original/first forecasts, phase clocks and ground/budget evidence
also match. Eighty-one extra strict event samples are absent from the combined
output; fixed-checkpoint comparisons do not infer values at these timestamps.

Seven sampled normal updates outside the declared checkpoints still lose a
coarse numeric estimate: six have unavailable/invalid foot rays and one lacks
the current selected-site evidence. These remain unknown and are retained in
`hard-guards.json`. They were not converted into calibration misses to increase
coverage. No clearance profile, guard or frozen estimator/helper source changed
after the freeze.

This closes the descent-selection experiment. Use the explicit combined variant
as the offline timing baseline for the next investigation, while preserving both
older options for comparison. Further tuning of ordinary descent is lower
priority than measuring interruption risk. This is not a claim of improved
match strategy and does not promote a runtime bot policy.

## Retained cases and the next investigation

Eleven saved cases include improvements, regressions, restored forecasts and
large interruption misses. Raw traces and native counter/contact diagnostics
allow the same states to be revisited.

- **Useful early restoration:** normal `world0-asteroids0-seat0`, selection
  **6689**, descent entry **8376**. Assist strength is 0.44, so strict clearance
  stays unknown. The combined estimate retains the coarse forecast, only 0.27
  seconds short of actual landing at **8771**.
- **A restored estimate can still be poor:** normal `world1-asteroids3-seat0`,
  selection **4596**, entry **5834**. Weak assist retains 6.32 seconds to landing;
  the ship lands after 1.60 seconds at **5930**, so the error is +4.72 seconds.
  The later trip ends with the match. Fallback preserves coverage and provenance,
  not a guarantee that the coarse duration is accurate.
- **Supported descent interrupted later:** normal `world0-asteroids3-seat0`,
  selection **2813**, tick **5713**. The retry state is nearly upright with
  ordinary descent speed and strong assist. Clearance predicts 5.23 seconds;
  landing takes 13.18 seconds. The **7.95-second underestimate** follows a native
  airborne replan at **5974** and site acquisition at **6000**. The coarse model
  is already unknown here. This miss survives unchanged from strict clearance.
- **Larger miss outside descent:** normal `world0-asteroids0-seat1`, selection
  **8649**, first-choice +15 at **10071**. Its retry-approach landing component is
  **17.80 seconds short**, followed by six native replans and five site
  acquisitions. The whole-trip forecast is already unknown because of ground
  cost evidence; it is not a completed numeric whole-trip comparison.
- **Do not treat late ground bookkeeping as airborne risk:** controlled
  `world0-band20-40-dir-1-seat1`, selection **345**, forecasts from **1485** have
  landing error −0.33 coarse versus −0.64 clearance. The native replan at
  **1820** occurs after physical landing; controller acknowledgement is **1823**.

Across active attempts the diagnostic retains 751 normal native replans (one
already physically landed), four controlled replans (three already landed), and
89 normal/nine controlled site acquisitions without another native increment,
including initial acquisitions. Subtype counters overlap. These are event
counts, not interruption probabilities or counterfactual avoidable delay.

Next, investigate whether **recent native terrain/route invalidations, stalled
progress and contact state** separate clean continuations from later airborne
interruptions. Define outcomes using actual native increments, keep site
acquisition and already-grounded replans separate, and include failed/censored
attempts before fitting risk. Use current/past features only. Remote-transfer
costs remain another prerequisite for strategic mission selection.

## Verification and reproduction

All 269 Python tests pass, including twelve new tests for exact coarse/strict
selection, ordinary versus malformed assist, missing rays/motion/site evidence,
expired coarse support, ground/deadline guards, current support changes,
immutable first forecasts, retries, aliasing and terminal scope.

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/estimate-descent-clearance.py evaluate \
  --descent-model supported-clearance \
  --manifest target/bot-descent-regimes/normal/inputs.json \
  --source-evaluation target/bot-descent-regimes/normal/baseline/evaluation.json \
  --ground-profile target/bot-trip-estimate/calibration/profile.json \
  --phase-profile target/bot-phase-landing-estimate/calibration/profile.json \
  --walking-profile target/bot-walking-calibration/fit/profile.json \
  --state-profile target/bot-approach-state/calibration/profile.json \
  --descent-profile target/bot-descent-clearance/calibration/profile.json \
  --out target/bot-descent-regimes/reproduced
```

`target/bot-descent-regimes/` retains `run-matrix.py`, `evaluate.py`, `analyze.py`,
`develop.py`, manifests, raw recordings and comparison outputs. The known-world
development audit is separate from fresh validation. `diagnose.py`, `cases.py`
and `inspect-guards.py` retain native events, selected examples and hard-guard
losses. Evidence is archived under
`/home/oldman/.codex/visualizations/2026/09/22/bot-descent-regimes/`, including new
recordings, all five profiles, scripts, tests and source patch. Earlier
calibration/raw sources and native binaries remain hash-bound archive
dependencies.
