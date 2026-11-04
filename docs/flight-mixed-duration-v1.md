# Mixed-execution training-duration experiment v1

**Status: completed, 2026-10-23 — extra training regressed mixed execution.** Experiment
`learned-flight-mixed-duration-v1-20261023`. This follows the
[routine evaluation integration](flight-evaluation-integration-v1.md) and tests
whether more training improves aligned-approach consistency under the candidate
mixed decoder. No architectural or reward change is combined with this experiment.
The original pre-run specification is retained separately in the archive.

## Hypothesis and matched comparison

Run fresh learner seeds **17/29/43** to **256 updates**, twice the original 128-update
budget. Keep the original initializations, training/development tasks, observations,
physics, reward, PPO settings and exploration. Evaluate the original and extended
budgets with `mean-steering-sampled-buttons-v1`: mean steering, sampled thrust/brake,
four repeats, seed base 5,310,000. Model decisions and physics remain 60 Hz; all controls
remain learned, with no native labels, fallback, rescue or handoff.

The original 128-update baseline is the verified 192-trial mixed replay: **56/64,
43/64 and 38/64 arrivals**. Reuse those retained results and traces only after the
fresh runs' update-0 and update-128 model, optimizer, collector, history and random
states match their original counterparts bit-for-bit. Provenance contracts differ
because this is a new budget/evaluation experiment; this comparison does not weaken
the strict checkpoint-resume contract. Any mismatch invalidates the matched-duration
claim and stops qualification pending investigation. No old checkpoint is resumed.

The only training setting changed is the declared update budget. Evaluation mode and
its wall guard change to support the candidate execution contract; isolation has
already been verified to leave training unchanged. No intermediate checkpoint search.
Qualification uses **update 256**, even if the best-reference pointer prefers update 0.

## Fixed distribution, model and objective

Use the existing `aligned-approach-v1` recipe: equally represented distances 4–6,
6–8, 8–10 and 10–12 units; stationary starts with zero spin; goals initially forward
of the ship, full-circle world bearings/headings, altitude 140–150, zero gravity and
planet spin. No added obstacles or terrain edits. Preserve the open-wing ship and
existing planet/boundary hazards. Admission remains geometric and provides no labels.

Reuse partitions 4210001 (64 training tasks, sixteen per stratum) and 4210002
(sixteen development tasks, four per stratum), with the existing capped admission
procedure. **4210003 and 4110003 remain unused regardless of this result.** Passing
this experiment supports a separate confirmation decision; it does not itself qualify
unseen tasks or permit a general-navigation claim. Do not add observed failures ad hoc.

Keep the 39,445-parameter network and 4,009-value observation. PPO uses four persistent
slots, 256-tick rollouts, four epochs, minibatch 256, Adam .0003, gamma .9995,
lambda .98, clip .2, value weight .5, entropy .01, gradient clip .5 and target KL .03.
Training retains independent Gaussian auxiliary actions (rho=0), including sampled
steering. This experiment does not align training exploration with noise-free steering.

Reward is unchanged: potential `-.5*tanh(distance/100)-.25*tanh(speed/20)-.25*tanh(abs_spin)`,
discounted potential difference, time −.001, success +10, failure −10; actual deadlines
score `-2-4*excess/(30+excess)`, where `excess=max(final_distance-3,0)`. Real terminals
do not bootstrap, rollout cuts do. Arrival remains distance ≤3, relative speed ≤1,
absolute spin ≤.2 rad/s for thirty consecutive complete ticks, with free final heading.
Deadline remains 1,800 ticks. No reward-total gate or reduced physical tolerance.

## Budget and stopping rules

- Three fresh runs, **262,144 training ticks each / 786,432 total**. Save every
  update plus initialization: **771 full checkpoints** and six initial/final exports.
- Initial and final evaluations each use sixteen tasks × four repeats per seed:
  **384 new episodes, at most 691,200 evaluation ticks**. Reused update-128 baseline:
  192 episodes / 177,917 ticks, not counted as newly executed physics.
- Each training process has a **2,400-second wall guard**, including initial/final
  evaluation. Three concurrent CPU processes, one PyTorch thread each. Record timings.
- Stop on the fixed update or wall budget, nonfinite/infrastructure error or failed
  provenance checks. No replacement seeds, additional updates, reward/task edits or
  checkpoint selection in response to results. Incomplete runs cannot pass gates.
- Preflight only adds gate/trace-validation tests; production source and runtime
  are unchanged from the integration's 52 passing tests. No additional training smoke
  or broad regression rerun is needed for these report-only additions.

## Frozen criteria

Every evaluated set must contain the same sixteen unique tasks in all four repeats,
four tasks per distance group, the declared decoder and correct episode seeds. Count
physical failures from outcomes or any collision/damage evidence. Verify every success
against the final thirty actual physics frames. Aggregate only the three declared seeds.

**Learning evidence per model:** ≥16/64 final arrivals, gain ≥16 versus initialization,
and ≥4/16 arrivals in at least three repeats. Require at least two models. This preserves
the earlier diagnostic standard and is separate from the stronger progression test.

**Scoped progression per model:** ≥48/64 final arrivals (75%), gain ≥32 versus
initialization, ≥8/16 arrivals in every distance group, ≥12/16 arrivals in at least
three repeats, and zero physical failures. Require at least two models. These are
new explicit mixed-repeat criteria; historical deterministic gates remain unchanged.
The sixteen starts are repeated trials, not 64 independent task samples.

**Duration benefit:** improvement of ≥8/64 arrivals over the matched update-128
baseline in at least two models, with no model losing more than 8/64. Report paired
retained, gained and lost successes even if this rule fails. This assesses the effect
of the extra training; it is not interchangeable with learning or progression.

## Analysis and retention

Report initial/128/256 outcomes, repeats, distance groups, collisions, action use,
goal entries, speed/spin compliance, dwell, optimizer diagnostics and training outcomes.
Training uses changing weights and noisy steering and remains separate from frozen
policy evaluation. Check final raw network outputs against independent Rust inference
on all 48 development starts. This does not claim native mixed-decoder integration.

Before training freeze this specification, configs, selected dirty/untracked source,
candidate descriptor, unchanged native bridge, runtime/dependency identities and copied
parent inputs. Retain admissions, every new checkpoint, initial/final exports, complete
and censored training traces, evaluation physics/probability records, logs, exact
commands and report evidence. Verify all eighteen prior sealed manifests and copied
inputs remain unchanged. Seal a new SHA-256 archive and compact receipt after analysis.
Local retention is not off-machine backup; no commit or clean checkout is implied.

If extra training helps consistently, review confirmation and a separately declared
difficulty expansion. If it fails or regresses, inspect whether approach or settling
changed before choosing another mechanism. Do not automatically extend the budget.

## Results

All three runs completed the declared **786,432 training ticks** and **426,044 new
evaluation ticks**, within their caps. Each worker took 971.4–1,062.8 seconds
(16.2–17.7 minutes concurrently); training with checkpoint writes took 694.7–702.4
seconds. All 771 checkpoints and six initial/final exports are retained. There was
no budget extension, replacement seed, task change or checkpoint search.

| Learner seed | Initial mixed arrivals | Update 128 baseline | Update 256 | Change |
|---|---:|---:|---:|---:|
| 17 | 0/64 | 56/64 | 38/64 | −18 |
| 29 | 0/64 | 43/64 | 40/64 | −3 |
| 43 | 0/64 | 38/64 | 6/64 | −32 |
| Total | 0/192 | 137/192 (71.4%) | 84/192 (43.8%) | −53 |

The frozen **duration-benefit and scoped-progression criteria both failed**. None of
the models met progression. The weaker learning criterion still passed in two models
(17/29), relative to initialization; this does not make the extra training beneficial.
Initial evaluation had 191 collisions and one deadline. Final evaluation had zero
physical failures and 108 deadlines. Reserved partitions remain untouched.

Across paired baseline/final trials, 82 successes were retained, 55 were lost, two
were gained, and 53 failed at both endpoints. All gains occurred in seed 29. These
are repeated development tasks, not independent evidence of generalization.

| Seed | Successes by repeat (out of 16 each) | Successes by increasing distance group (out of 16 each) |
|---|---|---|
| 17 | 9, 10, 9, 10 | 15, 12, 8, 3 |
| 29 | 10, 10, 10, 10 | 16, 12, 4, 8 |
| 43 | 2, 1, 2, 1 | 6, 0, 0, 0 |

### The regression is primarily in approach

Goal entries fell from **143 to 86**, while successful arrivals fell from 137 to 84.
Of the 108 final failures, **106 never entered the radius**; only two entered without
finishing dwell. Once inside, spin compliance remained 97.3%/100%/100% by seed.
This does not resemble the earlier failure to hold arrival under noisy steering.

The learned control changes differ between seeds:

| Seed | Mean predicted thrust probability, 128 → 256 | Executed brake fraction, 128 → 256 | Goal entries, 128 → 256 |
|---|---:|---:|---:|
| 17 | 3.82% → 4.24% | 91.85% → 65.36% | 56 → 40 |
| 29 | 5.03% → 4.12% | 75.57% → 94.04% | 48 → 40 |
| 43 | 2.37% → 0.27% | 38.59% → 59.99% | 39 → 6 |

Seed 43 nearly stopped thrusting; its executed thrust fell from 2.34% to 0.26% of
ticks. Seed 17 instead used slightly more thrust and substantially less braking;
near-goal speed compliance fell from 80.9% to 63.3%. Seed 29 braked more and entered
less often, although its mean final distance decreased. Distance alone is therefore
still an inadequate competence measure. These are trajectory averages under different
visited states, not isolated causal tests of any one action channel.

Sampled training recorded **24 arrivals**, versus two within the matched first half:
7/7/10 by seed, with 33 collisions, 403 deadlines and twelve censored attempts across
the full run. Thus 22 additional training successes coexisted with worse final mixed
evaluation. Training uses changing weights, different task starts and steering noise;
those counts are not a frozen-policy comparison. Learned steering standard deviation
decreased modestly (roughly .57–.62 to .54–.59), but training still sampled steering.

### Validation and retention

All six new report tests passed. Production sources are unchanged from the preceding
52-test validation; that evidence was retained rather than relabeled as a new run.
Initial and update-128 **complete training states matched their originals bit-for-bit**
in all three seeds, including optimizer, collector, history and random state. The
provenance contracts correctly differ. The additional training is the only changed
learning setting in this matched comparison.

All **48 final Rust/Python raw-output parity fixtures** passed, with maximum absolute
error 1.91×10⁻⁶. Every final success passed the independent thirty-frame physical
tolerance check; physics actions matched their policy records. No run was incomplete
or had a nonfinite/infrastructure error. The KL safeguard stopped optimization early
within one update for seed 17 and two for seed 29; these were ordinary configured
safeguards, not experiment termination. Maximum update-mean pre-clipping gradient
norms were 2.29/2.58/1.77. Terminal rewards and censored endpoints were also checked.
Compilation and whitespace checks passed.

The sealed archive retains 1,528 selected source files, 405 copied parent inputs,
all new checkpoints and physics/probability evidence, logs, commands, frozen criteria
and analysis. Eighteen earlier manifests and referenced parent files remain unchanged.
See [compact results](data/flight-mixed-duration-v1-results.json) and
[archive receipt](data/flight-mixed-duration-v1-archive.json). Local retention is not
off-machine backup.

### Interpretation and next decision

More training under this recipe did not fix consistency; it regressed all three
final policies. This is evidence against simply increasing the budget again. It does
not identify a unique reward, optimizer, generalization or architecture failure.

The next focused diagnostic should compare the saved update-128 and update-256
policies under **fully sampled execution**, using the same tasks and paired streams,
alongside their already retained mixed results. The update-128 fully sampled baseline
already exists (20/192 arrivals). This would test whether additional learning improved
the behavior used during collection while harming the mixed execution behavior, or
whether frozen performance deteriorated under both modes. Freeze that evaluation-only
comparison separately; do not infer its answer from changing-weight training successes.
No further training, harder tasks, model promotion or reserved confirmation use followed.
