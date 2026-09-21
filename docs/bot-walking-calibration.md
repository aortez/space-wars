# Independent walking-cost calibration

On **14 paired completed ground intervals** from independent validation worlds,
the new model reduces median absolute error from **2.83 to 0.87 seconds**.
An additional newly covered interval is underestimated by **15.76 seconds**;
the improvement in typical estimates does not resolve the slow tail.

This checkpoint tests a small conditional walking-time model after the
[controlled-ground experiment](bot-controlled-ground.md) found that moderate
walks were consistently underestimated. It adds an offline calibration and
comparison tool, `tools/calibrate-walking-costs.py`. It does not change bot
controls, physics, planning budgets, mission selection or production defaults.

## Declared experiment

The design is fixed before new outcomes are inspected: **eight training worlds**
and **six disjoint validation worlds**, with both seats. Each world supplies
three requested route bands, **[2, 20)**, **[20, 40)** and **[40, 60)** units,
in outbound direction `-1`. Additional `+1` controls reverse the initial angular
separation from `+0.6` to `-0.6` radians and request [20, 40). This makes
**64 training trials and 48 validation trials**, capped at 180 seconds each,
including physical preparation. All trials remain in the denominator.

Seeds are the first eight SHA-256 bytes, big endian, of
`walking-affine-training-v1:0` through `:7` and
`walking-affine-holdout-v1:0` through `:5`. Both sets are disjoint from the
previous calibration, natural-match and controlled diagnostic worlds. The
fixture executable is unchanged from `14cc4ba`, with SHA-256
`4b3325f28780d628a1a3b4c7711ff3819fadde756315e32d91230745b625d1ed`.
Controls and physics remain at revision
`43764580869c1ad56e6b7e9c4b7a21f485496220`.

The existing fixture physically prepares a defender flag, then lets the attacker
approach, land, counterclaim, board and depart. It uses generated moving planets,
ordinary controls and the existing live planning budget. A read-only setup pass
chooses a site whose prospective outbound and return routes both fit the band;
these setup surveys remain outside the execution quota. It does not search for
replacement worlds when setup, approach or ground execution fails.

The direction limitation has a concrete code explanation:
`material_access_with_ray` uses hatch side zero for exiting, while
`material_boarding_with_ray` considers both sides for boarding. Reversing initial
separation is a useful control but does not mirror the exit mechanism. Full
outbound direction symmetry would require different exit behavior, so it remains
outside this unchanged-controller experiment.

## Model and training eligibility

For each leg, the candidate predicts:

```text
phase seconds = fixed overhead + reference multiplier × original route reference
original route reference = first prelanding route length / 5
```

Outbound means exit to first claim progress. Return/boarding means ownership
acquired to boarding. The claim estimate remains the original frozen model's
value. Ground total sums those three phases; approach and departure are excluded.
Later actual route lengths cannot replace the original prediction features.

The two nonnegative coefficients are fitted by least squares, with every world
contributing total weight one within each phase. More successful seats or bands
from one world do not increase its total weight. The candidate needs at least
six eligible samples, three distinct worlds and a reference span of four seconds
per leg. Unsupported cells remain unknown. Each leg formula needs only a
multiplication and addition, with no world queries or rollouts.

Two comparisons are preserved: the original frozen additive-residual model and
a newly fitted offset-only model whose reference multiplier stays at one. The
affine model is the declared candidate; validation does not choose or refit its
coefficients. The intercept is an empirical overhead, not a direct measurement
of a specific survey or settling operation.

The frozen training fit is:

| Phase | Samples / worlds | Overhead | Reference multiplier | Supported reference |
| --- | ---: | ---: | ---: | ---: |
| Outbound | 29 / 7 | 0.537 s | 1.176 | 1.520–10.893 s |
| Return/board | 27 / 7 | 0.206 s | 1.197 | 1.403–10.386 s |

These are successful phase samples, not 56 independent worlds or full trips.
Four contributing trials finish both ground legs but leave the local frame during
departure; completed phases remain usable even though the controlled trip is
censored afterward. The training ledger retains 30 completed target trips,
18 unavailable setups, 12 frame changes and four preparations on the wrong planet.
The profile SHA-256, recorded before validation starts, is
`e2100bdddd5993c8eb159dc332dd980f42c4bd5a0e8a59e744c5dda2bcd8cce4`.

Training requires a valid original pure-walk choice, an exactly observed
completed phase under current physics, no recorded dependency changes before
that phase ends, and dense execution. Actual routes must remain complete walks,
with no primary input or knocked-down/recovering posture. A completed outbound
phase can qualify even when the later return fails. Every rejected phase retains
its outcome and exclusion reasons. A `jump` goal without a jump command does not
by itself exclude a walk.

Profiles record samples, exclusions, source hashes and measured reference
domains. Predictions outside those domains remain unknown, without silently
falling back to another model. Fitted residual min/max ranges describe training
observations; they are not confidence intervals or guaranteed travel limits.
The model estimates time conditional on successful walking, not the probability
that walking will remain possible or finish.

## Measurement integrity

The controlled adapter retains first-choice forecasts, explicit capture-start
scope, target-planet checks and the match's departure threshold from the prior
experiment. A new training world reveals a preparation edge case: four trials
reach planet one before setup route selection. The adapter now records
`setup_wrong_planet` instead of aborting the batch. Any capture already in that
foreign frame ends at its start with no target-planet prediction. These trials
cannot become successful walking calibration samples.

That correction changes only outcome accounting. The initial evaluator failure,
initial source freeze and predictions are retained, along with the corrected
source freeze before fitting. The model algorithm, eligibility rules and matrix
remain as declared. All training predictions must match across the correction;
all 32 earlier controlled outcomes also retain their previous classification.

Validation cannot start until a profile and source hash freeze exists. Its runner
checks those hashes before launching, and the final audit checks them again.
The comparison tool rejects shared training/validation seeds, report hashes or
trace hashes. It writes forecasts using only original choice snapshots before
reading execution evidence for qualification. All completed forecast errors are
reported alongside the narrower unchanged-walking subset. Paired comparisons
use the same attempts for both models, so differences in numeric coverage do not
silently improve an error statistic.

## Independent validation

All **112 declared trials** finish with clean physics/material audits, after
**7,700.87 simulated seconds** in total. All **156,697 execution-planning rows**
respect their shared graph/query quotas. The read-only setup survey is excluded
from those execution quotas; these figures do not establish a frame-time budget.

| Controlled outcome | Training | Validation |
| --- | ---: | ---: |
| Completed on the intended planet | 30 | 18 |
| Setup unavailable or never reached | 18 | 14 |
| Left the target's local frame | 12 | 10 |
| Setup occurred on another planet | 4 | 0 |
| Ship lost during capture | 0 | 6 |
| **All trials** | **64** | **48** |

The reversed-separation `+1` controls still supply no valid setup route: training
has 15 unavailable routes and one wrong-planet setup; validation has 12 unavailable
routes. The model therefore retains the native exit-direction coverage limit.

Twenty validation trials complete both ground legs and claim; two of them change
frames during departure and remain incomplete full sorties. The original model
has 24 numeric ground forecasts and 15 completed comparisons; the candidate has
25 numeric forecasts and 17 completed comparisons. **Fourteen completed intervals
are numeric under both models.** The table uses exactly those shared samples for
ground/return and the 18 shared completed outbound samples.

| Phase | Paired samples | Original median error | Offset-only median error | Affine median error |
| --- | ---: | ---: | ---: | ---: |
| Outbound | 18 | 1.733 s | 0.420 s | **0.328 s** |
| Return/board | 14 | 1.345 s | 0.803 s | **0.542 s** |
| Ground total | 14 | 2.827 s | 1.230 s | **0.873 s** |

Paired ground mean absolute error falls from **3.68 to 1.71 seconds**. On the
narrower 11 paired intervals that satisfy the unchanged-walking eligibility
checks, median ground error falls from 2.45 to 0.78 seconds. The offset-only
comparison also improves on the old model, but the predeclared affine candidate
has smaller median and mean errors on all three paired phase comparisons.

All 17 numeric candidate ground comparisons have a median absolute error of
0.78 seconds and mean of 2.40 seconds. Those figures are not a paired comparison
with the old model. Three newly covered completed intervals and one lost completed
comparison change the denominator; coverage and unknown reasons remain explicit
in `summary.json`. Validation does not trigger another fit.

## Retained slow-return tail

`validation0-band40-60-dir-1-seat0` predicts **31.16 seconds** from exit through
boarding and actually takes **46.92 seconds**. Its original return reference is
10.154 seconds, just beyond the old return calibration's 10.118-second limit but
inside the new training range. The old model correctly has no numeric ground
total, so this **15.76-second underestimate** must not disappear behind the
paired-comparison headline.

The slow component is the return: **26.77 seconds** at ticks **2802–4408**,
versus a 12.36-second candidate estimate. The first actual return route is a
52.14-unit walk on a planet of radius 112.94. It remains balanced throughout
1,605 on-foot ticks, with no primary input, jumps, crossings or route invalidations.
There are 574 full-input, 974 partial-input and 57 neutral ticks, and only 657
ticks report planet support. The delay accumulates across waypoint movement;
the longest individual waypoint interval is 1.43 seconds. The frequent `jump`
goals occur without a commanded jump. This is a successful slow walk that passes
the declared eligibility checks, not the earlier knocked-down recovery failure.

The same world's shorter P1 return also underperforms: its 30.19-unit actual route
takes 12.57 seconds versus a 7.11-second estimate. The ground-total underestimate
is 6.33 seconds and is included in the paired table. Together these cases show
that one distance multiplier does not explain every contact/control condition.
They do not yet identify the physical cause.

`return-diagnostics.json` retains both cases, immutable forecasts, waypoint
intervals and two-second checkpoints with actor, planet, hatch, control and
posture observations. Begin a reduction of the longer case at **3440–3518** or
**3979–4065**, comparing support transitions, partial steering, relative surface
velocity and waypoint advancement. Keep generated motion and ship geometry.
The raw command and full trace are retained, including the later frame change.

## Next bounded step

Keep this frozen candidate as the improved conditional walking component and
combine it with the existing phase-aware landing diagnostic. Compare complete
sortie estimates with every unsupported phase and interruption still visible;
remote-transfer and completion-risk evidence remain prerequisites for informed
mission selection. The two slow returns and the earlier posture blockage have
reproduction notes for focused physical investigations. Neither needs to be
hidden in a fitted average or a larger timeout before that integration proceeds.

These fourteen new worlds are now known training/validation evidence. Any further
coefficient or feature tuning requires a newly declared independent validation.

## Tests and reproduction

**150 Python tests pass**, including affine/nonnegative fitting, world weighting,
minimum support, extrapolation refusal, preserved unknowns, censored phases,
changed dependencies, posture and later powered-route exclusions, common
comparison coverage, independent recordings and initial-placement identity.
The off-target setup correction has a regression test. Runtime Rust code is
unchanged; physical trials reuse the previously verified executable.

After restoring the archive, the core steps are:

```sh
python3 tools/evaluate-controlled-ground.py \
  --manifest target/bot-walking-calibration/training/inputs.json \
  --profile target/bot-trip-estimate/calibration/profile.json \
  --out /tmp/walking-training-baseline
python3 tools/calibrate-walking-costs.py calibrate \
  --evaluation /tmp/walking-training-baseline/evaluation.json \
  --manifest target/bot-walking-calibration/training/inputs.json \
  --out /tmp/walking-fit
python3 tools/calibrate-walking-costs.py evaluate \
  --evaluation target/bot-walking-calibration/validation/baseline/evaluation.json \
  --manifest target/bot-walking-calibration/validation/inputs.json \
  --profile /tmp/walking-fit/profile.json --out /tmp/walking-comparison
python3 -m unittest discover -s tools/tests -p 'test_*.py'
```

Evidence lives under `target/bot-walking-calibration/`, archived at
`/home/oldman/.codex/visualizations/2026/09/21/bot-walking-calibration/`.
The archive retains the declared design, all commands/reports/dense traces,
setup and execution failures, both source freezes, frozen profile, predictions,
comparison groups, audit/test results and final source patch. The frozen original
ground profile and prior-world inputs are included. Adjacent metadata verifies
every member and binds the source patch to its commit. `run-matrix.py` records and
executes the design; `analyze.py` verifies independence, quotas and preserved
forecasts. This evidence does not require replaying matches to inspect a failure.
