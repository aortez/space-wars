# Paired first-exit hold experiment

**Status: completed bounded paired experiment, 2026-10-24.**
Experiment `learned-flight-early-hold-v1-20261024`, following the
[hold-update audit](flight-hold-update-audit-v1.md). That audit found only 0.58% of
selected rollout rows within all holding tolerances; across the parent run, at most
1.81% of rows could be in an episode's first thirty ticks. Test whether ending a
failed hold immediately improves learning by concentrating experience on holding.

## Fixed comparison

Two fresh arms, `long` and `early`, with learner seeds 17/29/43 and **32 updates each**.
Both use the same 64 training starts (seed 4310001) and sixteen development starts
(seed 4310002), four heading strata, at-goal COM, zero velocity/spin, gravity off,
no planet spin, obstacles or terrain edits. Physical starts and initial tensors must
match by seed. Confirmation seeds 4310003/4210003/4110003 remain unused.

The long arm preserves `hold-position-v1` and its 1,800-tick deadline. Only the early
arm's training tasks use `hold-first-exit-v1`, explicitly serialized as `hold_failure`.
It terminates at the first completed interval whose before or after frame violates
any arrival tolerance. `hold_broken` is a true terminal with the existing failure
penalty −10, zero terminal potential and no value bootstrap across reset. Physical
failures take precedence and remain distinct. Success still requires **30 complete
consecutive intervals**, position ≤3, speed ≤1 and absolute spin ≤native-f32 .2;
heading is free. The initial eligible frame alone earns no dwell or success.

Early tasks/traces use environment version 3; original tasks keep version 2 and omit
the new optional field. Policy input/action schemas remain continuous v2. Native
physics and inference are unchanged. Replay rejects mismatched task/trace versions.
The sampler changes only training termination; development always uses the original
long task. No native pilot, labels, rescue, handoff or warm start.

Both arms retain schema-v1 scaling, 39,447 parameters, 60 Hz physics/decisions,
independent continuous Gaussian exploration (rho=0, initial log standard deviation
−.5), potential reward, −.001 time cost, +10 success, −10 physical failure and graded
deadline reward. PPO: four slots ×256 ticks/update, four epochs, minibatch 256,
learning rate .0003, gamma .9995, lambda .98, clip .2, value .5, entropy .01,
gradient clip .5, target KL .03. Compared with the parent, both share a reduced
32-update cap to bound reset/retention costs. This is not a paired comparison with
its final 128-update policies. Long-arm update 32 tensors/Adam/RNG must reproduce the
parent update 32 as an additional no-regression check.

## Frozen gates and measurements

Evaluate native mean actions at initialization and update 32, sixteen original long
hold tasks per endpoint: **192 episodes**, at most **345,600 action ticks**. No
sampled evaluation or intermediate checkpoint search. Report all physical arrivals,
immediate holds (success exactly at tick30), physical failures, first exits, dwell,
controls and heading coverage. Later returns count as arrivals but not immediate holds.

Per arm, learning evidence requires final arrivals ≥4/16 with gain ≥4 in at least
two seeds. Scoped holding requires ≥12/16 immediate holds, gain ≥8, at least two per
heading stratum and no physical failures in at least two seeds. Paired improvement
requires the early arm to gain at least four immediate holds over long in at least
two seeds. Failed gates remain results; no model promotion or approach/navigation claim.

Inspect all completed and active training traces: reconstruct every physical outcome
and complete-interval dwell, count first-thirty-tick rows, eligible pre-action rows,
episode lengths, outcomes, controls and exploration deviations. These measurements
test whether exposure changed; they do not replace the development gates.

## Budgets and reproducibility

Main training: six ×32×1,024 = **196,608 ticks**, at most **3,072 optimizer steps**;
1,800 seconds per process, external guard 1,820. Use six CPU workers with one Torch
thread each. Keep every update checkpoint, endpoint export and complete/active trace.

Preflight per arm, seed 17: two original updates and resume checkpoint 1 to 2; **6,144
total ticks**, at most **96 optimizer steps**. Four training processes and one
verification process, each capped 180 seconds. Full resumed state must match bitwise;
main seed 17 states 0/2 must match preflight. Native/Python export parity uses sixteen
fixtures per arm preflight and sixteen per main endpoint: **224 comparisons/setup
ticks**. Reporting cap 900 seconds, external 920. Twelve subprocesses must exit zero.

Retain six frozen configs, source archive/inventory, native bridge and dependency
versions, three parent configs and update 32 checkpoints, both relevant parent results,
all 206 new checkpoints and twenty exports, admissions, parity and process logs.
Freeze before any learning. Verify all 27 prior manifests and copied parent inputs
unchanged. Local retention does not imply off-machine backup. No commit is implied.

## Results

Early failure concentrates training on holding and lowers throttle, but both arms still produce 0/48 final holds or later arrivals at 32 updates.

All six main runs completed the frozen 32 updates. Initial and final results below
use the same original long evaluation tasks, with no early-failure termination.

| Arm | Seed | Initial arrivals / immediate holds | Final arrivals / immediate holds | Final physical failures | Maximum final dwell | Learning / scoped hold gates |
|---|---:|---|---|---:|---:|---|
| long | 17 | 0/16 / 0/16 | 0/16 / 0/16 | 1 | 3 | False / False |
| early | 17 | 0/16 / 0/16 | 0/16 / 0/16 | 0 | 9 | False / False |
| long | 29 | 0/16 / 0/16 | 0/16 / 0/16 | 0 | 3 | False / False |
| early | 29 | 0/16 / 0/16 | 0/16 / 0/16 | 0 | 1 | False / False |
| long | 43 | 0/16 / 0/16 | 0/16 / 0/16 | 0 | 4 | False / False |
| early | 43 | 0/16 / 0/16 | 0/16 / 0/16 | 0 | 11 | False / False |

Across-seed gates: `{"long": {"learning_evidence": false, "scoped_hold": false}, "early": {"learning_evidence": false, "scoped_hold": false}}`. Paired early-arm immediate-hold improvement gate: **False**.

### Training exposure and controls

Training samples actions; evaluation uses the native mean action. Training success is
reported separately and does not qualify a model. Fractions below count actual retained
training transitions, including active episodes at the budget boundary.

| Arm | Seed | Outcomes | First 30 ticks | Eligible before action | Maximum training dwell | Final latent deviations |
|---|---:|---|---:|---:|---:|---|
| long | 17 | {'collision': 16, 'deadline': 8, 'budget_censored': 4} | 2.56% | 0.25% | 3 | 0.6059 / 0.6076 / 0.6196 |
| early | 17 | {'hold_broken': 8624, 'budget_censored': 2} | 100.00% | 100.00% | 12 | 0.5567 / 0.5859 / 0.6225 |
| long | 29 | {'deadline': 14, 'collision': 4, 'budget_censored': 4} | 2.01% | 0.18% | 3 | 0.6034 / 0.6119 / 0.6132 |
| early | 29 | {'hold_broken': 8806, 'budget_censored': 3} | 100.00% | 100.00% | 13 | 0.5662 / 0.5752 / 0.5988 |
| long | 43 | {'collision': 5, 'deadline': 13, 'budget_censored': 4} | 2.01% | 0.19% | 3 | 0.6022 / 0.6088 / 0.6286 |
| early | 43 | {'hold_broken': 8540, 'budget_censored': 4} | 100.00% | 100.00% | 14 | 0.5681 / 0.5800 / 0.6148 |

Final evaluated controls, averaged over all action ticks:

| Arm | Seed | Mean throttle | Mean brake | Mean absolute steering |
|---|---:|---:|---:|---:|
| long | 17 | 40.31% | 48.33% | 0.03359 |
| early | 17 | 15.78% | 52.36% | 0.08338 |
| long | 29 | 45.51% | 55.05% | 0.03816 |
| early | 29 | 15.53% | 52.51% | 0.11844 |
| long | 43 | 35.88% | 46.06% | 0.08284 |
| early | 43 | 13.80% | 54.46% | 0.06422 |

The exposure change worked: all 98,304 early-arm training decisions began within every holding tolerance, versus 202/98,304 (0.21%) in the matched long arm. First-thirty-tick coverage rose from 2,160/98,304 (2.20%) to 100%. Early training recorded 25,970 hold_broken terminals, nine nonempty budget-censored attempts and no successes; maximum dwell was 12/13/14 ticks by seed. The control recorded 25 collisions, 35 deadlines and twelve nonempty censored attempts, with maximum dwell three ticks. Newly reset zero-step slots remain in checkpoints and started-episode accounting but contribute no trace rows.

There is partial behavioral progress despite all frozen gates failing. Early-arm final evaluated mean throttle is 15.78%, 15.53% and 13.80%, versus 40.31%, 45.51% and 35.88% in the controls. Mean lengths of completed sampled episodes increased from 2.79/2.73/2.73 ticks in update 1 to 4.85/4.55/4.48 in update 32. These are descriptive training measures, not independent success evidence; per-update rows and hold-start controls are retained in validation/post-run-inspection.json. All 48 early final evaluations reached the original deadline, with no collisions or successes. Long final evaluations had 47 deadlines and one collision.

The remaining failure is concrete. Early seeds 17 and 43 first exceed speed tolerance at ticks 10 and 12 in every development case; seed 29 exceeds spin tolerance at tick 2 in every case. Their longest final dwells are 9/1/11 ticks. The corresponding control maxima are 3/3/4. Early-arm latent steering deviations remain about 0.56, only modestly below their initial 0.6065. Concentrating training has therefore improved throttle and sampled survival, but has not produced precise low-spin holding. This bounded result neither proves that longer training will solve it nor identifies exploration, reward or network capacity as the sole remaining cause.

### Verification and retention

Native surface tests: **418 passed, two ignored**. Eighteen Python tests passed,
covering paired sampling, unchanged development tasks, true-terminal rewards/GAE,
first-exit replay, thirty complete success intervals and report reconstruction.
All twelve experiment subprocesses exited zero within their guards. Exact full-state
resume passed for both arms; main seed 17 states at updates 0/2 match preflight bitwise.
Initial models and physical starts match across arms. Long-arm update 32 model tensors,
Adam state and CPU/Python RNG exactly reproduce all three parent update 32 checkpoints.
Every saved training/evaluation outcome and complete-interval dwell was reconstructed.

All **224 native/Python parity comparisons** passed (192 endpoints, 32 preflight); endpoint maximum raw error 4.76837e-06, action error 7.45058e-08. Export/checkpoint tensors match; parity preserves parameters and RNG.

Main training: **196,608 action ticks / 3,033 optimizer steps**. Preflight: 6,144 ticks / 96 steps. Evaluation: **262,229 action ticks across 192 episodes**. Parity: 224 neutral setup ticks. Main process times: 218.6–324.0 seconds; reporting 10.94 seconds. No budget extension.

Archive `data/training-experiments/learned-flight-early-hold-v1-20261024` retains
1,545 source files, native runtime, six configs, eight copied parent inputs, all 206
new checkpoints and twenty exports, admissions, training/evaluation traces, parity
and process logs. All 27 prior manifests and copied parent inputs remain unchanged.
No model promotion, confirmation use or approach/navigation claim. See
results (`docs/data/flight-early-hold-v1-results.json`, in git history) and
[archive receipt](data/flight-early-hold-v1-archive.json).

## Next decision

Propose a separately frozen matched 128-update long/early comparison with the same seeds, tasks, architecture, input scales, reward coefficients, PPO and exploration. The observed throttle reduction and longer sampled attempts justify testing additional learning at fixed settings. Use fresh runs, verify their update 32 model/Adam/RNG states against this archive, and preserve the original long-task evaluation and existing gates. Do not modify the sealed 32-update runs or use reserved confirmation tasks. This extension is proposed, not run; no model is promoted.
