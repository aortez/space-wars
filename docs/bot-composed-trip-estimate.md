# Composed flight and walking estimates

Combining the frozen walking model with the phase-aware landing estimator
improves **nine paired completed controlled trips** at the 15-second checkpoint:
median absolute whole-trip error falls from **3.41 to 1.16 seconds**. The strict
combination is **not a general replacement** for the earlier estimator. Many
ordinary captures have walks shorter than the affine model's measured domain;
their forecasts become unknown. Initial controlled forecasts also remain worse
than the original estimator on the common completed subset.

This checkpoint adds `tools/compose-trip-estimates.py`, an offline diagnostic.
It changes no bot controls, mission selection, physics, planning quota or default.
It reuses recorded outcomes; there are **no new simulations or fitted parameters**.

## Composition and accounting

The existing [phase estimator](bot-phase-landing-estimate.md) supplies remaining
landing time from observed flight phase, phase age and initial/retry context.
The [walking model](bot-walking-calibration.md) supplies outbound and
return/boarding times for a currently acquired pure-walk route. Exit, claim and
departure keep their original empirical estimates:

```text
total = elapsed since attempt start
      + remaining landing + exit + outbound + claim + return/board + departure
```

The walking model applies only within each leg's measured reference domain:
outbound **1.520–10.893 seconds**, return **1.403–10.386 seconds**, with references
equal to route length divided by five. Either unsupported leg leaves the whole
trip unknown. There is no extrapolation or automatic return to the old walking
model. `no_flag`, jump and powered categories retain the earlier model, including
its missing evidence. No existing flag is a separately measured category, not a
zero-length walk with zero cost.

The superclass still produces the unmodified phase-only comparator. Composition
reuses its current anchor without changing its original prediction, evidence,
phase history or clocks. An anchor is acquired only from a valid current
landing-choice snapshot. A material/objective/route change invalidates it; stale
or unavailable evidence cannot repair itself. A refreshed anchor can produce a
new estimate but cannot rewrite the first forecast. Historical timing does not
renew physical route validity.

Applying walking coefficients trained at original choices to subsequently
acquired route references is an explicit transfer assumption being examined by
this replay. Future execution routes, successful outcomes and actual durations
are never prediction inputs. Native capture deadlines and retry counters remain
separate from conditional completion time. Unknown landing or ground costs never
become zero. The summed historical envelopes are neither confidence intervals
nor guaranteed travel bounds.

## Recorded comparison

The experiment declares three complete cohorts before replay:

| Cohort | Recordings / worlds | Attempt outcomes |
| --- | ---: | --- |
| Earlier normal-match diagnostics | 20 / 5 | 54 completed, 64 abandoned, 5 match ended |
| More recent normal matches | 8 / 2 | 21 completed, 38 abandoned, 1 match ended |
| Walking validation trials | 48 / 6 | 18 completed, 10 frame changes, 6 ship losses, 14 unavailable setups |

These are **76 recordings and 217 observed attempts**; unavailable setups stay
in the trial denominator. All evaluation worlds/recordings are disjoint from
every input profile's training data. They have already been examined in earlier
experiments, so this is a diagnostic replay, not fresh validation of composition.

Normal totals start at native mission selection. Controlled totals start at the
capture controller, excluding physical preparation and remote transfer. Their
departure gate and frame-change censoring come from the existing controlled
evaluator. They are not pooled into one match-performance score.

Checkpoints remain 0, 15, 30, 60 and 120 seconds after the **first observed landing
choice**, including when a retry has started. Completed errors compare identical
rows within each pair. Unknown forecasts, attempts without choices, failed
attempts, already-landed checkpoints and ended attempts are separately retained.
All four comparators remain available: original frozen estimate, age-only landing,
phase-aware landing with the old tail, and phase-aware landing with affine walks.

### Controlled whole trips

| Checkpoint | Paired completed | Phase-only median error | Combined median error | Phase-only / combined numeric forecasts |
| --- | ---: | ---: | ---: | ---: |
| First choice | 13 | 6.72 s | 5.16 s | 24 / 25 |
| +15 s | 9 | 3.41 s | 1.16 s | 10 / 13 |
| +30 s | 1 | 5.97 s | 3.18 s | 4 / 4 |

At +15 seconds the paired mean also improves, **5.90 to 3.64 seconds**, but the
largest paired underestimate remains **12.83 seconds**. Across all 11 completed
combined forecasts at that checkpoint, median/mean errors are **1.16/3.18 seconds**.
The other two numeric forecasts belong to unfinished trips and do not become
successful comparisons. There are no numeric completed comparisons at +60/+120.

The walking improvement does not make every complete estimator better. On the
same 13 completed first-choice rows, the original estimator's median error is
**3.69 seconds**, versus **6.72** for phase-only and **5.16** for combined. Flight
calibration transferred from normal matches is not uniformly better at the start
of these controlled approaches. At +15 seconds, the nine-row original median is
**3.10 seconds**, versus the combined **1.16 seconds**.

### Ordinary-match coverage

At +15 seconds, numeric predictions drop from **42 to 24** in the earlier cohort
and **18 to 12** in the recent cohort. All 24 losses have at least one walk shorter
than its affine training minimum. No new numeric coverage is gained there.
The remaining paired medians, **0.65** and **0.22 seconds**, are unchanged: those
checkpoints use the existing `no_flag` model.

At first choice, the earlier cohort loses 31 forecasts and the recent cohort
loses seven. Two recent normal choices fit both walking domains, but only one
completes. Its error worsens from **0.12 to 1.30 seconds**. It is
`fresh1-asteroids0-seat0`, selection **7902**, choice **8004**; references are
2.617/2.093 seconds. After a retry, its return reference falls below the domain
and the combined forecast becomes unknown. One completed case cannot establish
the fit's transfer to ordinary matches.

Smaller errors on the remaining numeric subset are not evidence that dropping
short walks improves prediction. The old forecasts and their errors remain in
the output. Coverage must be addressed explicitly before using these costs to
rank missions.

## Preserved investigation cases

`diagnostic-cases.json` retains complete attempts, milestone bounds, checkpoint
updates and landing/tail error decomposition for these cases:

- **Controlled approach miss:** `validation1-band20-40-dir-1-seat0`, capture
  start 2580, choice 2608. At tick **3508** (+15 s), the combined underestimate
  is **12.83 seconds**: **9.77** from landing and **3.07** from the post-landing
  tail. This is initial approach, with observed phase age 8.5 seconds. By tick
  **4408** (+30 s), descent reduces the landing miss to **0.12 seconds**. Landing
  acknowledgement is 4643, boarding 6080, departure 6314. This is a useful case
  for site-relative height, lateral error, velocity and phase-transition evidence;
  a longer walking allowance alone cannot explain the miss.
- **Terrain/retry tail:** `discovery-heldout0-asteroids3-seat0`, selection 5756,
  choice 6405. At tick **7305**, the unchanged no-flag prediction underestimates
  the complete trip by **25.38 seconds**, with **24.74** from landing. It is still
  an initial alignment forecast; later restarts remain in the trace.
- **Earlier 29.90-second miss:** `fresh0-asteroids3-seat1`, selection 10900,
  checkpoint **12057**. The phase-only error is unchanged. The combined total is
  unknown because its walking references are unsupported; that does not resolve
  the landing failure. The landing-only underestimate is still **29.92 seconds**.
- **Slow successful ground interval:** `validation0-band40-60-dir-1-seat0` still
  has the prior **15.76-second** ground-cost underestimate. It changes frame before
  normal departure, so it correctly has no completed whole-trip error here. Keep
  the walking investigation's reducer windows **3440–3518** and **3979–4065**;
  censoring the whole trip must not erase this completed slow return.

The ordinary walking regression above is also included. The raw recordings and
prior walking execution diagnostics remain in their existing archives.

## Verification and reproduction

All **165 Python tests pass**, including composition, unsupported legs, unchanged
nonwalking categories, evidence refresh, retries, elapsed time, native deadlines,
immutable first forecasts, controlled-frame termination, outcome schemas, profile
compatibility and training/evaluation separation. Every original forecast and
all normal-match phase-only checkpoint forecasts and phase histories are unchanged.
The recordings represent 14,638.25 simulated seconds; all existing physical audits
and **332,855 execution quota rows** pass verification. Replay does not establish
a new runtime performance measurement.

An initial controlled evaluation failed because the adapter expected
`recorded_attempt` instead of its normalized field `actual_trip`. Correcting only
the outcome adapter and adding a real controlled CLI test leaves **every forecast
and sampled update byte-identical** across all 48 trials. The initial failed run,
source freeze, correction and successful rerun are retained. No profile was refit.

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/compose-trip-estimates.py \
  --manifest target/bot-walking-calibration/validation/inputs.json \
  --source-evaluation target/bot-walking-calibration/validation/baseline/evaluation.json \
  --ground-profile target/bot-trip-estimate/calibration/profile.json \
  --phase-profile target/bot-phase-landing-estimate/calibration/profile.json \
  --walking-profile target/bot-walking-calibration/fit/profile.json \
  --out target/bot-composed-trip-estimate/reproduced-controlled
```

For normal matches, use the phase experiment's `known-inputs.json` with
`known-evaluation/evaluation.json`, or `fresh-inputs.json` with
`fresh-evaluation/evaluation.json`. The tool binds outcomes to manifest, profile,
runtime, report and trace hashes. It writes and hashes all forecasts before
opening the outcome evaluation. It verifies the old comparator instead of
silently regenerating a different baseline.

Evidence lives under `target/bot-composed-trip-estimate/`, archived at
`/home/oldman/.codex/visualizations/2026/09/21/bot-composed-trip-estimate/`.
`design.json` freezes cohorts, source/input hashes and commands;
`adapter-correction.json`, `checks.json`, `summary.json`, `coverage-losses.json`,
`completed-errors.json` and `diagnostic-cases.json` retain the findings.
`controlled-validation-corrected/evaluation.json` is the authoritative controlled
result. `analyze.py` and `inspect-cases.py` reproduce the checks and case breakdown.
The lightweight archive includes the frozen profiles and outcome evaluations,
while `dependencies.json` identifies the existing raw-recording archives by hash.
Its adjacent manifest and commit binding verify the contents and final source.

## Next step

The subsequent [walking-regime validation](bot-walking-regimes.md) makes the
short/moderate distinction explicit and tests it on seven new worlds. It restores
short-walk coverage without losing strict numeric forecasts; 11 controlled
completions improve median initial error from 4.35 to 1.74 seconds versus phase-only.
The new cases retain initial-flight misses and unsupported longer approaches.

Investigating flight state/progress and interruption risk is next, alongside
unmeasured remote transfer. This composition supplies the cost-accounting
interface for mission planning, but conditional duration alone is not yet
sufficient to choose which mission is likely to finish.
