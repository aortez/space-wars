# First landing endpoint probability comparison

This offline prototype compares a phase-only forecast with a supported refinement
using recent native replans. It follows the
[interruption investigation](bot-landing-interruption-risk.md). The timing
profiles, bot controls and native physical permissions remain unchanged.

## Forecast contract

Forecast four mutually exclusive endpoints over the next ten seconds: native
prelanding replan, first physical landing, reaching the horizon without either,
or the attempt ending before them. A failed/abandoned attempt is its own
competing endpoint. A low replan probability therefore does not imply a safe or
successful trip. Match end, missing observations, changed captures, reset
counters and ambiguous ordering remain censored.

The diagnostic's earlier binary `interrupted` flag deliberately leaves
`attempt_ended` unresolved. This model uses its already-recorded categorical
endpoint as a fourth outcome; it does not relabel it as a landing or a clear
horizon. Administrative/ambiguous censoring is excluded from the point fit and
retained in support counts and sensitivity bounds. This is not a fitted
whole-trip success probability or expected retry delay.

Each scope and checkpoint has its own tables. Normal matches and controlled
setups are separate. First-choice +0 and +15 seconds are separately calibrated;
later checkpoints remain unsupported. The prototype is a comparison at these
fixed observations, not yet a forecast for every control update or remote
arrival.

The baseline indexes the current flight phase: approach, circling, alignment,
descent or settling. The refinement additionally indexes whether a native
prelanding replan occurred in the previous five complete seconds. Unknown
history retains the independently supported phase estimate. A sparse refinement
also retains that exact estimate, including its probabilities and evidence.
An unsupported phase stays unknown. Invalid current plan/contact evidence and
malformed history cannot be repaired by a refinement.

Physical landing stops exposure before controller acknowledgement. A grounded
replan remains separate; one or two supported feet without native `landed` status
remain at risk. Site acquisitions are not counted as replans. Forecast features
and prior event history use the existing causal diagnostic; future phase ends,
final successful tails and eventual trip outcomes are not predictor inputs.

## Frozen fit and independent comparison

The fit uses the prior 216 recordings on 39 worlds, now known development data.
The rule was declared before fitting, without score-based parameter selection:

- At least twelve classified attempts across four worlds per cell.
- At most ten percent censored observations per cell.
- Equal weight for each world's classified endpoint distribution.
- Add one uniform prior world before normalizing the four probabilities. This
  avoids certainty from cells with no observed examples of a particular outcome.

There is at most one observation per attempt in each checkpoint table. Different
worlds have equal weight within a cell; repeated native calls do not create
additional training examples. The model retains counts, per-world support,
censored outcomes and empirical censoring sensitivity bounds separately from its
regularized point prediction. Those bounds are not confidence intervals and do
not include uncertainty from sampling or the prior.

The resulting profile has 29 cells, fourteen supported: nine phase cells and
five recent-history refinements. It occupies 107,766 bytes with provenance and
per-world evidence. Prediction uses at most two probability-table lookups; this is an
offline operation count, not a Pi timing measurement. No search, fitted feature
weights or runtime scheduler was added.

The two forecasts, rules, code, tests, drivers, native executables and five timing
profiles were frozen before generating eleven new worlds, excluding all 78
previously used world seeds:

- 32 normal matches: eight worlds, v11 versus v10 with seats swapped, asteroid
  intervals zero and three seconds, at most 600 simulated seconds.
- 24 controlled trials: three worlds, both seats, walking bands [2,6), [8,20),
  [20,40), [40,60), direction −1, offset +0.6 and the established side-zero exit,
  at most 180 seconds including setup. Failed/unavailable setups remain counted.

Runtime is still `43764580869c1ad56e6b7e9c4b7a21f485496220`. Calibration and
validation observations belong to `material_mission_v11` under the declared
configurations; the model is not validated for arbitrary control policies. These executions
exercise existing bot policies; they are not a new-policy win-rate comparison.
Both risk estimates observe identical traces. The combined clearance/coarse
timing estimator is replayed unchanged to retain the source lifecycle and
ordinary timing evidence.

`inspect-landing-risk.py --features-only` writes and hashes the causal records
without invoking the endpoint labeller. Both forecasts are then persisted and
hashed before a separate pass generates outcome labels. The full diagnostic's
feature file must match the earlier export exactly. Validation rejects training
world/recording overlap and incompatible runtime, rules or source identities.

The primary comparison is normal +15-second forecast quality, using paired
multiclass Brier scores and the interruption component's Brier score. Lower is
better. The multiclass score is the sum of four squared probability errors,
without dividing by two; its range is [0,2]. First-choice +0 results are separate.
Log loss, fixed-bin reliability, interruption AUC, unknown coverage, per-world
score differences and controlled results are also retained. AUC is unavailable
when a cohort has only one interruption class.

Censored observations stay visible. Their possible multiclass scores range
over all four endpoints instead of assigning a fabricated label. These extreme
score bounds and coverage accompany the classified-only point scores. They do
not assume censoring is random or prove out-of-sample calibration.

## Results and decision

All **56 simulations pass the physical audits** and exit successfully, totaling
**13,248.97 simulated seconds**. All **180,421 planner quota rows** stay within
their graph/query allowances. The frozen inputs and timing profiles match;
neither risk model emits controls. All **310 Python tests pass**.

Normal runs retain 164 attempts: 59 completed, 101 abandoned and four ending
with the match. Seventy never acquire a landing choice. Controlled runs retain
twelve observed attempts: nine completed and three ship losses. Twelve of the
24 controlled trials fail to obtain their physical setup; three observed
attempts never acquire a choice. These remain separate denominators.

At normal +15 seconds, 76 observations have an eligible local plan. Both models
forecast 74; two settling observations lack phase support. Another five lack an
active plan and thirteen checkpoints are not observed. The table uses the
identical 74 classified forecasts: seventeen interruptions, 42 first landings
and fifteen clear horizons, with no censored outcomes in this paired cohort.

| Normal +15 s metric | Phase only | Phase + supported recency |
| --- | ---: | ---: |
| Numeric forecasts | 74 | 74 |
| Mean multiclass Brier ↓ | 0.38048 | 0.36695 |
| Interruption Brier ↓ | 0.12552 | 0.11889 |
| Mean log loss ↓ | 0.69866 | 0.68919 |
| Interruption AUC ↑ | 0.84107 | 0.86894 |

The refinement changes 69 normal +15-second distributions and retains the exact
phase forecast on five others. The improvement is modest: phase already
identifies much of the risk. Six of eight worlds improve in multiclass Brier
and two regress slightly; seven improve in interruption Brier and one regresses.
Equal-world mean differences (refined minus phase-only) are **−0.01404** and
**−0.00754** respectively. Equal-world log loss instead worsens slightly,
**+0.00170**, despite the better pooled log loss. A small world with a rare
long descent accounts for that distinction.

The Brier gain appears with asteroids disabled, 0.33841 → 0.32263 on 35 paired
forecasts, and at the three-second interval, 0.41823 → 0.40672 on 39. Pressured
log loss worsens slightly, 0.77357 → 0.77691. These are descriptive results on
eight independent validation worlds, not a significance or universal-calibration
claim.

Most of the gain comes from seven circling observations, all with recent
replans, all interrupted within the horizon. Their forecast rises from 0.5893
to 0.6875; multiclass Brier falls 0.28699 → 0.15451. These seven observations do
not establish certainty or justify tuning a probability to one. The other
67 forecasts predict mean interruption probability 0.1388 → 0.1350 against an
observed 10/67 = 0.1493. Sparse reliability bins remain explicit.

The other phases show much smaller changes: alignment's multiclass Brier
regresses 0.63350 → 0.63456 on eleven observations; approach improves
0.63859 → 0.63552 on 26; descent improves 0.08582 → 0.08563 on thirty. The recent
history does not reliably anticipate a first fresh invalidation.

At first choice the two normal models are identical: 94 numeric forecasts,
93 classified and one administratively censored. The classified endpoints are
27 interruptions, 61 clear horizons and five attempt endings. Mean multiclass
Brier is 0.43681 and interruption Brier 0.19157. Including the censored record
gives extreme all-forecast Brier bounds 0.43529–0.44707; it is not silently
counted as a successful approach.

Controlled forecasts are also identical: nine numeric at first choice and six
at +15 seconds. The other three eligible +15-second observations lack alignment
or descent support. The six forecasts yield five clear horizons and one landing,
with mean multiclass Brier 0.35071. There are no interruptions in this scored
controlled cohort, so it provides no discrimination test for the risk feature.

The audit verifies all **366 numeric distributions**, preserves all **183**
phase forecasts across both scopes/checkpoints, and checks **108 exact phase
fallbacks**. Six controlled refinements select a child cell whose probabilities
are identical to the parent; only the 69 normal forecasts change probabilities.
All feature exports match the later labelling pass byte for byte, and closed
prediction hashes remain unchanged.

Keep the refinement as an **offline candidate alongside the phase baseline**.
The independent comparison supports a small benefit, principally in circling,
while the log-loss regression, sparse cells and fixed-checkpoint scope prevent
a broad deployment or calibration claim. No parameter was changed after seeing
these validation outcomes.

The next missing component is **site acquisition**: time to obtain a usable
local landing plan, failure/censoring before that choice, and repeated
invalidations while no plan exists. Seventy normal validation attempts have no
choice, and the prior investigation retains high-count examples. This is a
larger coverage gap than further tuning the current recency split. Remote
transfer and extension beyond the two forecast checkpoints remain prerequisites
for time/risk-aware mission selection.

## Saved regressions and scope limits

`selected-cases.json` and `case-context.json` retain outcome-selected diagnostic
cases. They are not extra fitting examples or a reason to replace validation
worlds.

- `world0-asteroids0-seat0`, selection 6225, forecast tick 7795: recent circling
  replans raise the interruption forecast 0.5893 → 0.6875, followed by another
  native replan at tick 7876, 1.35 seconds later. This is representative of the
  main improvement.
- `world2-asteroids3-seat1`, selection 15754, forecast tick 16827: quiet recent
  history lowers the approach replan forecast 0.1778 → 0.1627, but a native
  replan follows at 16939, 1.87 seconds later. Physical landing eventually occurs
  at 18410. This is a fresh interruption the history feature fails to anticipate.
- `world4-asteroids3-seat1`, selection 23601, forecast tick 25297: an initially
  healthy descent has no recent replans and roughly 1.6 units of net site-distance
  closure in the preceding second. The refinement lowers the probability of a
  clear ten-second horizon from 0.02 to 0.01. That is the actual endpoint, adding
  **log(2) = 0.69315** to this forecast's log loss. The first native replan is at
  25903, **10.1 seconds** after the forecast, just beyond the fixed horizon.
  Its preceding observation has one supported foot and ten seconds since the
  last native progress update. Physical landing is much later, at 28816. The
  correct ten-second label does not imply a successful uninterrupted descent or
  accurate whole-tail timing.

The last case explains why better ranking and Brier scores are insufficient to
claim well-calibrated trip risk. Retain the exact horizon and the longer native
timeline when revisiting it; changing the horizon to catch this one event would
be tuning on validation evidence.

## Reproduction

The experiment's working evidence is under
`target/bot-landing-risk-probability/`. `calibration-design.json` precedes fitting;
`design.json` freezes the profile and new-world matrix. Each scope retains source
reports/traces, timing and event replays, a causal feature export,
`forecasts-closed.json`, forecast probabilities, labelled endpoints and score
breakdowns. Original calibration evidence remains in the preceding risk archive.

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/estimate-landing-risk.py forecast \
  --features target/bot-landing-risk-probability/normal/risk-features/features.json \
  --evidence target/bot-landing-risk-probability/normal/risk-features/provenance.json \
  --manifest target/bot-landing-risk-probability/normal/risk-inputs.json \
  --profile target/bot-landing-risk-probability/calibration/profile.json \
  --out target/bot-landing-risk-probability/normal/reproduced-predictions.json
```

The probability tool's `calibrate` and `evaluate` modes respectively bind the
known feature/outcome pair and score a previously closed prediction file. They
do not refit during validation. Tests cover distinct failure/censoring treatment,
equal-world weighting, sparse-cell fallbacks, malformed current evidence,
duplicate attempts, future-data isolation, overlap rejection, proper scores,
censored score bounds and feature-only export without outcome labelling.
