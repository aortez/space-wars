# Landing estimates by observed flight phase

`tools/estimate-landing-phases.py` adds a read-only landing model to the
[rolling trip diagnostic](bot-rolling-trip-estimate.md). It replaces the
whole-approach duration prior with evidence for the phase the ship is actually
in. First approaches and retries have separate calibration. Bot controls,
mission selection, physics and the Pi build are unchanged.

## Model and boundaries

The predictor observes five phases:

| Phase | Current observation |
| --- | --- |
| Circling | Tactical `seek_cover` |
| Approach | Tactical `approach` |
| Alignment | Tactical `surface`, landing pilot `approach` or `reposition` |
| Descent | Tactical `surface`, landing pilot `land`, no supported feet |
| Settling | Same landing goal, at least one foot supported on the target planet |

Contact is not a completed landing. The endpoint remains the native controller's
landing acknowledgement, including its settling and exit-readiness checks.
Losing contact begins another descent episode. Unknown goals, a different local
planet, unavailable ship, or on-foot observations do not invent a flight phase.

Contiguous observations establish each episode's entry and age. An isolated
first sample can establish entry only when the native goal clock starts on that
tick; physical contact has no equivalent clock. A gap invalidates the observed
entry until a subsequent phase transition is observed. Material/route evidence
refresh does not restart physical progress. Site/controller/capture-clock changes
do start another plan, with retry context even when its native retry count has
not changed. A nonzero native retry count also identifies retries at the first
observation.

Each calibration sample records the duration of an observed phase episode and
the time from its entry to native landing acknowledgement. At prediction time:

1. Use only the current phase and initial/retry context.
2. Retain historical episodes that were still **in that phase** at the current
   phase age. A long eventual landing time does not make an already-ended
   circling or approach phase a valid comparison.
3. Subtract current phase age from their landing-time intervals.
4. Take the median within each recorded attempt, then the median across attempts.
   Repeated contact episodes or retries cannot give one attempt extra weight.
5. Require at least two supporting attempts. Otherwise report an unknown cost;
   never substitute the initial-approach prior for a retry.

The reported envelope spans the retained historical intervals. It is neither a
confidence interval nor a promised deadline. Support includes distinct attempts,
episodes and world seeds. Two attempts are a minimum for producing a diagnostic
number, not sufficient evidence of broad calibration.

Calibration includes only episodes that reach landing without a subsequent
observed plan restart, missing observation, or unavailable phase. Interrupted,
unfinished and uncertain episodes remain in the profile's exclusions and in
the full replay records. A later ground failure does not erase an already
completed landing measurement. This is a conditional successful-duration model,
not a completion-probability model.

Flight calibration pools ground-route categories. Ground travel, claiming,
boarding and departure still use the previous frozen profile and its original
reference domains. Unsupported ground tails remain unknown. Landing-only errors
are reported separately so ground calibration cannot mask landing accuracy.
Distance, speed, radius, gravity and damage are not fitted features in this slice.

## Accounting and comparisons

The rolling tracker remains responsible for evidence invalidation, historical
route provenance and native capture/retry limits. The new model uses a small
landing-estimate hook; the default tracker retains its previous behavior.

Every replay keeps three estimates:

- The immutable estimate at the first observed landing choice.
- The previous rolling estimate, conditioned on total age of the current plan.
- The phase estimate, conditioned on current phase age and retry context.

All include time already spent since mission selection. None resets the native
150-second capture allowance. The strict native `elapsed > 150 * 60` condition,
separate ordinary/cover/solar retry budgets and unknown success probability remain
visible. The earlier 155-second failed approach still ends at the same native
deadline; its unfinished duration is not fitted as a successful landing.

Forecasts are written and hash-bound before outcome reports are opened. The
replay records also retain the observed phase history for later analysis; that
history is not an input to earlier forecasts. Fixed comparisons occur at 0, 15,
30, 60 and 120 seconds after the **original** observed choice. A retry cannot
move the measurement origin. Ended attempts, already-landed cases, missing
observations and unknown estimates stay in the denominator.

Three-model error comparisons use exactly the same completed attempts at each
checkpoint. Landing-only comparisons can also include a completed landing whose
ground trip subsequently failed. Separate summaries show each retry context,
each phase, and the phase model's own numeric coverage. Later checkpoints have
different survivor populations; these tables are not an accuracy-over-time curve.

## Calibration and validation

The runtime is still `43764580869c1ad56e6b7e9c4b7a21f485496220`, using the
previously frozen `candidate-mission-soak` executable. The ground profile is
unchanged, SHA-256
`23c9a12bec8eec532f750a1b9b67295cd4dd96380435155b4439503bbc881f06`.

The same four training worlds were replayed in both seats with quiet and
three-second asteroid conditions: 16 recordings, one per original condition.
Dense recordings replace the old sparse recordings; they are not extra samples.
Every previously recorded observation/control row matches. Reports differ only
in measured wall-time fields and the dense-trace setting. Successful calibration
coverage is:

| Context / phase | Episodes | Distinct attempts | Worlds |
| --- | ---: | ---: | ---: |
| Initial / circling | 10 | 10 | 4 |
| Initial / approach | 25 | 25 | 4 |
| Initial / alignment | 31 | 31 | 4 |
| Initial / descent | 34 | 30 | 4 |
| Initial / settling | 35 | 31 | 4 |
| Retry / each phase | 2 | 2 | 2 |

The 20 recordings used during earlier estimator work are diagnostic worlds now,
not new validation. The source and calibration profile were frozen before eight
new matches: two independently seeded worlds, both seats, quiet and asteroid
conditions, up to ten simulated minutes per match. Neither those worlds nor the
diagnostic worlds are added to calibration here.

Median absolute total-trip errors, in seconds, on three-model completed pairs:

| Set / checkpoint | Paired trips | Original | Previous rolling | Phase |
| --- | ---: | ---: | ---: | ---: |
| Known / 0 s | 51 | 2.42 | 2.42 | 1.83 |
| Known / 15 s | 38 | 1.81 | 1.95 | 0.79 |
| Known / 30 s | 6 | 20.68 | 10.15 | 5.26 |
| Fresh / 0 s | 20 | 8.22 | 8.22 | 2.57 |
| Fresh / 15 s | 18 | 8.52 | 5.41 | 0.28 |
| Fresh / 30 s | 2 | 9.58 | 1.17 | 1.14 |

There are no completed numeric pairs at 60 or 120 seconds. At the fresh
15-second checkpoint, all 28 observed choices are accounted for: 18 numeric,
five unknown, three ended and two already landed. The unknowns are one missing
ground tail, one unavailable plan reference and three phases with insufficient
surviving support. All 60 fresh attempts remain recorded: 21 completed,
38 abandoned and one stopped by match end. The known set has 123 attempts:
54 completed, 64 abandoned and five stopped by match end.

The improvement also appears in landing-only errors. At 15 seconds, the fresh
set has 19 paired landed cases: median error is 7.95 seconds for the original,
4.85 for the previous rolling model and 0.43 for the phase model. The known set
has 40 such pairs: 1.89, 1.89 and 0.89 seconds respectively.

Initial approaches and retries must still be distinguished:

| Set / context at 15 s | Paired completed trips | Previous rolling median | Phase median |
| --- | ---: | ---: | ---: |
| Known / initial | 33 | 1.91 | 0.75 |
| Known / retry | 5 | 4.53 | 5.85 |
| Fresh / initial | 12 | 6.71 | 0.36 |
| Fresh / retry | 6 | 4.72 | 0.09 |

The near-touchdown retry identified in the previous report
(`confirm0-asteroids3-seat1`, selection 6865) improves from a 12.82-second
overestimate to 5.85 seconds, but remains worse than its original frozen
estimate's 1.97-second error. The known retry cohort also regresses overall.
The six fresh completed retry pairs are encouraging, not evidence that this
two-attempt retry calibration is broadly reliable. Known retry coverage at
15 seconds is seven numeric and 15 unknown; fresh retry coverage is six numeric
and four unknown.

The fresh median hides a substantial outlier. In `fresh0-asteroids3-seat1`,
selection 10900, the 15-second checkpoint is tick 12057. It predicts a
44.17-second trip; actual departure occurs after 74.07 seconds. The ship is in
retry approach with four native live invalidations already recorded. Nine more
live invalidations occur before landing at tick 14744. The model follows each
new plan and keeps the original clock, but does not forecast those future
interruptions. Its 29.90-second underestimate is worse than the previous rolling
model's 22.93-second underestimate. At this checkpoint the fresh mean absolute
error is 2.29 seconds, compared with 6.56 for previous rolling and 8.38 for the
original. `fresh-outlier.json` preserves the attempt, future interruptions and
relevant prediction rows; do not remove it from aggregate accuracy results.

All eight fresh matches finish with clean physics audits and 86,698 planning
quota rows within budget. They cover 4,087.82 simulated seconds in total.
The 16 training replays also pass their audits and 75,791 quota checks, with
88,776 original observation/control rows exactly matched. The 20 known runs
retain their clean audits and 179,497 quota checks. All original predictions
match the prior records exactly; 133 numeric previous-rolling checkpoints
match within `1e-12` seconds (maximum floating-point grouping difference
`1.42e-14` seconds).

The tool suite passes 99 tests, including phase/contact transitions, future
clocks, gaps, material refresh, retry isolation, per-attempt weighting, censored
episodes, retained unknown tails, the unchanged capture deadline, prefix-only
forecasting, disjoint-world checks and the original rolling contracts.

Final review tightened one missing-phase guard: returning from an unavailable
phase cannot establish entry merely because observations are consecutive again.
`verify-entry-guard.py` verifies that this correction leaves forecast bytes and
phase histories identical across all 44 training/diagnostic/fresh recordings.
The original source freeze and the final source hash are both retained.

## Reproduction and evidence

```sh
python3 -m unittest discover -s tools/tests -p 'test_*.py'
python3 tools/estimate-landing-phases.py calibrate \
  --manifest target/bot-phase-landing-estimate/training-inputs.json \
  --ground-profile target/bot-trip-estimate/calibration/profile.json \
  --out target/bot-phase-landing-estimate/reproduced-calibration
python3 tools/estimate-landing-phases.py evaluate \
  --manifest target/bot-phase-landing-estimate/fresh-inputs.json \
  --ground-profile target/bot-trip-estimate/calibration/profile.json \
  --phase-profile target/bot-phase-landing-estimate/calibration/profile.json \
  --out target/bot-phase-landing-estimate/reproduced-fresh
```

The manifest format is shared with the frozen estimator. Evaluation rejects
training world seeds and identical report/trace recordings from **either** input
profile. Calibration rejects duplicate recordings of the same seat. Profiles
bind the runtime revision and ground-profile hash. The tool uses only Python's
standard library.

Evidence lives under `target/bot-phase-landing-estimate/`, archived under
`/home/oldman/.codex/visualizations/2026/09/20/bot-phase-landing-estimate/`.
The archive contains exact commands, dense training replacements, fresh reports
and traces, profiles, predictions, checks, tests and the source patch. Adjacent
metadata binds it to the final commit. Restore the earlier
`bot-rolling-trip-estimate`, `bot-trip-estimate` and `bot-outbound-walking`
archives alongside it for the diagnostic recordings, ground profile, original
training rows and frozen executable; their hashes are documented in the
[rolling report](bot-rolling-trip-estimate.md#tests-evidence-and-reproduction).

`run-training.py` verifies dense replay equivalence. `prepare-evaluation.py`
declares the comparison worlds; `run-fresh.py` records the frozen sources before
starting them. New seeds are the first eight SHA-256 bytes, big endian, of
`phase-landing-estimate-holdout-v1:0` and `:1`:
`16912063594754708648` and `7001078675557248408`.
`analyze.py` checks audits, planning quotas, source/profile hashes and unchanged
baseline predictions. `calibration-support.json`, `examples.json`,
`fresh-errors-at-15s.json`, `fresh-outlier.json`, `checks.json` and `summary.json`
preserve the findings.

## Remaining work

Keep this as the routine-flight diagnostic and move on to independent longer
ground trips. Carry unknown retry costs and the interruption case forward.
Phase recognition improves the description of progress; it does not measure
how far a retry still has to fly. The two successful retry examples are a narrow
basis for a timing model. Future retry work should record current site-relative
height/side error and velocity, and distinguish ordinary approach from contact
or objective-readiness stalls. Keep failure/unknown coverage and budget accounting
when evaluating that extension.

Interruption risk needs separate treatment before selection: the fresh outlier
already has four live invalidations at its checkpoint. An investigation can
start with that observed history and its recorded nine later invalidations.
Use a new world set to evaluate any resulting rule; this outlier is now known
diagnostic evidence, not a held-out test for the next change.

Complete-trip mission selection still needs independent nontrivial ground trips,
remote-transfer timing, and explicit treatment of unsupported powered routes
and pilot exposure. These diagnostics do not promote the experimental bot or
change strategic decisions.
