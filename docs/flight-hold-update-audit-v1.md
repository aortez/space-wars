# Exact PPO-update audit of stationary hold training

**Status: completed exact-replay audit, 2026-10-24.** Experiment
`learned-flight-hold-update-audit-v1-20261024`, following the failed
[stationary hold diagnostic](flight-hold-position-v1.md). No new long training run,
flight evaluation, checkpoint scan, model promotion or loss-setting change.

## Fixed selection and reconstruction

Use all hold-run learner seeds **17, 29, 43**. Select pre-update checkpoints
**0, 32, 64, 127**, comparing their next original updates **1, 33, 65, 128**.
These twelve cases are fixed before output/gradient measurements; selection does not
seek the largest rise/drop or a favorable seed. Copy both endpoints, each seed's
initial native-parity observations and development admission, all three configs and
parent results. Reuse the exact parent source contract and native bridge.

For each case restore the pre-update collector and its four active traces, optimizer,
Torch/Python RNG and model. Collect precisely the next original 1,024-tick rollout.
Require the resulting collector to equal the next archived checkpoint completely.
Reconstruct per-step rewards, values, terminal flags, GAE advantages and critic targets;
require exact equality with the production batch. Then require the production PPO
update to reproduce archived parameters, Adam state, RNG, metrics and history exactly.
Stop and retain evidence on any mismatch rather than attributing a different update.

Fixed post-collection observations are the full 1,024-row rollout and the same sixteen
original development hold starts per seed. Reuse the parent's saved native observations,
with zero error and neutral previous controls; no new resets are needed for these probes.
Probe unchanged decoding: mean throttle, mean brake and **mean absolute steering**,
plus all raw actor/value outputs and per-observation commands. Signed steering averages
alone can hide opposite turns. No dynamic outcome is inferred from a command change.

## Loss measurements and matched update variants

At the actual first minibatch of each original update, using its original permutation
and entropy samples, differentiate separately:

- PPO policy loss with original normalized advantages and clipped likelihood ratio;
- the weighted critic loss on the shared representation;
- the negative weighted continuous-action entropy loss.

Require the component gradients to sum to the full gradient within float tolerances.
Record norms and shared-parameter alignment, global clipping scale, and raw directional
projections onto throttle, brake and absolute-steering means on both observation sets.
These projections describe unit gradient descent at one minibatch, **not Adam update
attribution**. The full multi-minibatch comparisons below capture the actual optimizer.

Starting each branch from identical pre-update weights/Adam and post-collection RNG,
run exactly one original-budget PPO update on the same rollout:

1. `full`: original value weight .5 and entropy weight .01;
2. `no_entropy`: entropy coefficient zero, value weight unchanged;
3. `no_value`: value coefficient zero, entropy weight unchanged;
4. `policy_only`: both coefficients zero.

Use unchanged production update code, including the original advantages/critic targets,
minibatch order, gradient clipping, KL stopping and entropy sampling even where its
coefficient is zero. Retain all branch states, RNG, metrics and before/after probes.
Historical Adam moments remain present in every branch; zeroing an immediate loss term
does not remove its previous influence or the critic's role in the fixed advantages.
Differences are not additive and may include different KL-stop behavior. No branch is
continued, flown, exported as a gameplay candidate or substituted for its parent.

Report every seed/checkpoint/branch, with command changes in percentage points and
raw projections clearly separated. Compare each coefficient probe with the full update
on both rollout and original starts. Report sign consistency and magnitudes without
selecting only helpful cases. No physical learning/qualification gate is claimed.

Retain per-row rewards decomposed into position/velocity/spin shaping, time and terminal
terms, including the existing graded deadline. Record raw/normalized advantages,
critic values/targets, sampled latents/actions and direct policy score pressure for
steering/throttle/brake. Predeclared overlapping groups: all rows; first thirty episode
ticks; within position radius; within all tolerances; throttle ≤.05 or ≥.5; and increasing/
decreasing speed or spin. These associations and the first-minibatch gradients do not
identify the cause of the entire failed training history.

## Budgets, validation and retention

The selected checkpoints' existing collector metadata fixes **32,436 restored action
ticks +12,288 original collection ticks =44,724 action ticks**, plus **55 neutral setup
ticks**. Per-case counts are frozen in `replay-budget.json`. Twelve cases ×four branches
×at most sixteen minibatches gives **≤768 diagnostic optimizer steps**. This is replay
and one-update counterfactual analysis, not an added training budget for a model.

- Overall wall guard 1,200 s; each case ≤180 s and cannot extend the global guard;
  external process timeout 1,220 s.
- No replacement cases, extra seeds, continuations, output-based scan, new flight
  trials, stochastic evaluation or confirmation partition.
- Stop on contract/runtime/source mismatch, missing/nonfinite evidence, collector/GAE/
  update mismatch, or budget exhaustion. Keep failed artifacts and do not silently retry.
- Six synthetic tests exercise continuous entropy signs, absolute-steering summaries,
  gradient/projection accounting, matched branches/batch immutability, continuous row
  labels and the graded terminal reward. They do not load trained weights or replay physics.

Freeze 1,543 source files, all 34 copied parent inputs, source/runtime/dependency identities,
specification, budgets and all **26 previous manifest hashes** before replay. Production
Python and native code stay unchanged. Retain rollouts, reconstructed collectors/targets,
RNG, row data, gradient vectors, all 48 branch snapshots/probes, results, process status
and helpers. Verify parent inputs and previous manifests unchanged, seal a new archive,
and update the governing plan/journal. All confirmation partitions remain unused.
Local retention is not off-machine backup; no commit or model promotion is implied.

## Results

Exact hold-update replays show small entropy effects and very little training data within holding tolerances.

All twelve original updates reproduced **bitwise-identical parameters, optimizer
state and RNG**, with exact collector reconstruction, GAE targets, metrics and history.
All 48 branches completed; no run or case was replaced.

The immediate entropy term generally resists lower throttle, but these measurements
do **not** identify it as the main cause of failed holding. Removing it lowers final
hold-start throttle in 11/12 updates; the change relative to the original update ranges
from **−0.08892 to +0.00128 percentage points**, averaging −0.02732 points. On rollout
states it lowers throttle in 11/12, with a maximum reduction of .10096 points. All four
branches execute sixteen minibatches in every case, so these differences do not arise
from unequal KL stopping.

The two original updates that increase hold-start throttle—seed 17 update 1 and seed
29 update 128—still increase it in **every** variant, including policy-only. Zeroing
immediate value loss lowers hold-start throttle in only 6/12 cases. Removing either
term therefore does not explain away those updates; original advantages and inherited
Adam history remain part of every branch.

Steering also lacks a simple loss-term fix. The original update increases mean absolute
steering on hold starts in 8/12 cases, and those eight increases persist in all variants.
Removing entropy actually increases absolute steering relative to the full update in
8/12 cases, by up to .001400; removing value loss reduces it in 9/12, but does not reverse
the eight original increases. These are immediate command changes, not demonstrated
changes in physical hold performance.

The data coverage is more striking. Of **12,288** audited rows, only **560 (4.56%)** are
in the first thirty episode ticks, **770 (6.27%)** are within the position radius, and
**71 (0.58%)** begin within all holding tolerances. Four selected rollouts contain no
first-window or fully qualified rows at all. The fixed selection includes initialization,
which overrepresents early windows: the complete parent training started only **237
episodes** in 393,216 action ticks. Even if every episode contributes thirty early ticks,
at most **7,110 ticks (1.81%)** could be in those initial windows. This is an arithmetic
upper bound from the frozen parent results, not an exhaustive classification of its
remaining traces.

The hold recipe resets at the goal, but breaking the initial hold does not end training:
the original task can continue for up to thirty seconds and reward a later return.
Consequently it spends most of its data on states after the initial hold has already
been lost. That is a plausible mismatch for learning immediate holding, not proof that
shorter episodes alone will solve the problem. This audit neither validates the critic's
long-horizon predictions nor evaluates alternative branches in flight.

### All selected hold-start throttle changes

Values are **percentage-point changes from the pre-update policy** on the same sixteen
hold starts. They are not changes in success rate. Case numbers below are actual update numbers, not checkpoint-selection ranks.

| Seed | Update | Full | No entropy | No value | Policy only | First-30 rows / 1,024 | Fully qualified rows / 1,024 |
|---|---:|---:|---:|---:|---:|---:|---:|
| 17 | 1 | +1.27219 | +1.27160 | +1.24039 | +1.24825 | 120 | 11 |
| 17 | 33 | -0.97637 | -1.00940 | -1.08202 | -1.09739 | 0 | 0 |
| 17 | 65 | -0.47415 | -0.52292 | -0.60765 | -0.70083 | 60 | 8 |
| 17 | 128 | -0.96942 | -1.01176 | -1.01982 | -1.03657 | 30 | 10 |
| 29 | 1 | -1.14672 | -1.14784 | -1.27433 | -1.27594 | 120 | 11 |
| 29 | 33 | -0.37318 | -0.40101 | -0.42465 | -0.45575 | 0 | 0 |
| 29 | 65 | -0.83396 | -0.83268 | -0.78607 | -0.79114 | 0 | 0 |
| 29 | 128 | +0.39331 | +0.35423 | +0.40489 | +0.28482 | 0 | 0 |
| 43 | 1 | -0.26300 | -0.26321 | -0.14834 | -0.15163 | 120 | 11 |
| 43 | 33 | -1.33152 | -1.35664 | -1.26573 | -1.32776 | 60 | 6 |
| 43 | 65 | -0.21558 | -0.30450 | -0.16857 | -0.22162 | 30 | 4 |
| 43 | 128 | -0.85599 | -0.87815 | -0.80121 | -0.83680 | 20 | 10 |

### Mean absolute-steering changes on the same starts

Steering changes are in raw command units (absolute command ranges from zero to one).
A positive change means greater mean turning demand, without predicting resulting spin.

| Seed | Update | Full | No entropy | No value | Policy only |
|---|---:|---:|---:|---:|---:|
| 17 | 1 | +0.048688 | +0.049410 | +0.044921 | +0.045429 |
| 17 | 33 | +0.023755 | +0.024044 | +0.022958 | +0.023223 |
| 17 | 65 | +0.003528 | +0.003489 | +0.002115 | +0.002048 |
| 17 | 128 | -0.003486 | -0.003613 | -0.004672 | -0.004898 |
| 29 | 1 | +0.010468 | +0.010675 | +0.011051 | +0.011312 |
| 29 | 33 | +0.020516 | +0.021244 | +0.020814 | +0.021615 |
| 29 | 65 | -0.000440 | +0.000428 | -0.000978 | -0.000369 |
| 29 | 128 | +0.003907 | +0.003809 | +0.003351 | +0.003501 |
| 43 | 1 | +0.044409 | +0.044992 | +0.042254 | +0.042815 |
| 43 | 33 | -0.044691 | -0.044511 | -0.047021 | -0.046733 |
| 43 | 65 | +0.024310 | +0.025710 | +0.028697 | +0.029000 |
| 43 | 128 | -0.005325 | -0.005401 | -0.007193 | -0.007001 |

### Raw first-minibatch gradient cross-check

The weighted entropy gradient projects toward higher hold-start throttle in all nine
non-initial cases (11/12 overall). This agrees with the local pressure from transformed
sigmoid-action entropy. Its parameter-gradient norm is smaller than the policy gradient
in every selected first minibatch. Every full first-minibatch gradient is clipped, with
scales .238–.741. These facts do not supply an additive Adam attribution: for example,
seed 29 update 65 has positive raw entropy projection, yet removing entropy leaves
slightly *higher* final throttle. Multi-minibatch updates, changing gradients and inherited
moments matter. All component vectors, shared-gradient alignments, projection values,
row rewards/advantages and per-observation before/after outputs are retained.

### Validation and retention

Six synthetic tests passed. All exact-replay requirements passed without changing
production source, reward, physics or action decoding. The original native bridge is
unchanged. Diagnostic comparisons retained fixed batches/advantages and historical
optimizer state; no branch was continued, flown or promoted.

The audit used **44,724 action ticks** (32,436 restored +12,288 collected), **55 neutral setup ticks**, and **768 diagnostic optimizer steps**. Total analysis time was **61.32 seconds**; subprocess exit zero. No budget extension or confirmation use.

Archive `data/training-experiments/learned-flight-hold-update-audit-v1-20261024` retains
1,543 source files, all 34 copied parent inputs, exact runtime, frozen budgets/spec,
twelve reconstructed rollouts/collectors/targets, RNG, gradients, row data and all 48
branch states with probes. All 26 previous manifests and referenced inputs remain
unchanged. See results (`docs/data/flight-hold-update-audit-v1-results.json`, in git history) and
[archive receipt](data/flight-hold-update-audit-v1-archive.json).

## Next decision

Propose a separately frozen **hold-specific early-failure comparison**: end a training
episode when it first leaves the holding tolerances, giving an explicit hold-failure
outcome, while keeping the thirty-complete-interval success requirement unchanged.
Compare it with the current long-episode hold task under declared matched seeds and
bounded budgets; keep reward coefficients, architecture, input scales, PPO and entropy
fixed so the termination rule is the isolated change. Declare the new failure outcome
and terminal reward explicitly—do not disguise it as a collision or bootstrap it as a
rollout cut. Retain the original physical evaluation and immediate-hold metric for
comparison. This changes the training task's failure rule and must be specified as a
new contract, not silently applied to existing runs. It is proposed, not implemented
or run. No model is promoted; all previous archives, physical success tolerances and
unused confirmation partitions remain intact.
