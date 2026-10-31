# Learning signal audit v1

**Completed 2026-10-23: exact reconstruction verified; a flat score among surviving
timeouts is the clearest finding. No model was updated.**

Specification and results for `learned-flight-signal-audit-v1-20261023`. The exact
pre-run specification is retained separately in the diagnostic archive.
Parent: [credit assignment comparison](flight-credit-assignment-v1.md). The user
authorized investigation of advantages, critic predictions and policy/critic
gradient interaction after three controlled changes failed to improve arrival.
This is a diagnostic, not a new training experiment or capability qualification.

## Frozen scope

Use all six parent runs: `control` and `long_credit`, seeds 17/29/43. Reconstruct
the next original rollout from checkpoints **0, 8, 32, 64, 96 and 127**. That is
36 rollouts / 36,864 sampled action ticks. Restore the original model, optimizer,
sampler, live physics traces and RNGs using the unchanged source/runtime contract.
Assert each resulting collector state equals the archived next checkpoint's
collector exactly. Compute diagnostics without calling an optimizer step. Check
that model and optimizer state remain unchanged.

### Advantage and reward attribution

Retain the reconstructed observations, latents, old likelihoods, advantages and
critic targets. Record every tick's physical before/after errors, actions and
separate position/velocity/spin shaping, time and terminal rewards. Distinguish
raw from rollout-normalized advantages; normalized positive credit is relative
to the rollout, not a positive task reward.

Summarize four mutually exclusive button combinations and overlapping descriptive
groups: thrust while closing/opening distance, thrust aligned/away from the goal,
steering while forward-goal alignment improves/worsens, braking inside/outside
the arrival radius, and reductions in speed/spin. Local +Y is forward; alignment
uses actual COM-to-goal direction and frame angle. Alignment thresholds are ±.8;
steering activity is absolute command >.1 and change tolerance is 1e-6. These are
observational categories, not causal action comparisons or proof of a useful route.
Momentum can cause closing even while thrust is unhelpful. Do not label such
groups as successful maneuvers or use them as training labels.

At unchanged pre-update weights, report advantage-weighted action-score pressure
on thrust/brake outputs across the rollout. Positive pressure means a direct
policy-gradient tendency to increase that output, before shared-feature gradients,
entropy, clipping and Adam. It is not the complete realized optimizer update.

### Shared gradient diagnosis

Recover the actual first minibatch ordering and entropy sample from the original
post-collection RNG state. Compute separate gradients for the clipped policy loss,
weighted value loss and weighted entropy term. Verify their sum matches the total
gradient. Report norms, policy/critic cosine, policy/total cosine and the applied
global clipping factor, both for all parameters and shared encoder/trunk parameters.
Retain gradient vectors. Larger critic gradients or a negative cosine indicate a
local imbalance/conflict; they do not alone prove harmful training. Adam's moments
and later minibatches matter. No alternative optimizer updates are performed.

### Frozen-policy critic checks

For reconstructed rollouts beginning at checkpoints **32 and 96**, inspect their
ending live states in slots **0 and 3** (near/far strata), with **two independent
seeded continuations per state**. This gives 24 boundary states / 48 continuations.
Freeze the pre-update policy whose value supplied the rollout bootstrap. Keep its
original stochastic action distribution, reward, physical criteria and remaining
episode deadline. Run each original episode to a real terminal, without resetting
into new tasks or updating weights. Retain the full continuation trace, predicted
value and realized discounted reward. This avoids comparing a frozen prediction
against a later training policy that has changed.

Use diagnostic RNG seed `70000000 + arm_index*10000000 + learner_seed*100000
+ checkpoint*100 + slot*10 + repeat`, with control arm index zero. Two draws
per state are a noisy check of expected return, not a ground-truth value function.
Aggregate signed prediction error and absolute error, retaining seed/state detail
and the two-sample spread. These episodes do not enter any training/development
success gate and do not touch reserved confirmation partition 4110003.

## Budget and evidence

Allow at most **390,000 physics ticks including trace restoration** across retained
workers (65,000 per parent run), plus at most 10,000 preflight ticks. Analytic bound
for the declared retained cases is 382,320 ticks: 36 restorations/rollouts at most
8,220 each plus 48 prefix/continuation paths at most 1,800 each. No repeat of poor
cases or adaptive search. Each case has a 180-second wall guard; any reconstruction,
source/runtime, budget or nonfinite failure aborts and is reported, not scored as
evidence. Ordinary workers may run concurrently.

Tests check heading convention, reward decomposition including terminal potential
reset, gradient-vector summaries and one real archived rollout/gradient probe.
Keep the production trainer unchanged. Freeze this specification and diagnostic
source before retained execution. Retain all row data, reconstructed rollout and
gradient tensors, continuation traces, outputs, tests, commands, dependency/source
identities and parent input hashes. Parent archives are read-only; verify all eleven
existing experiment manifests unchanged. Seal this diagnostic separately with a
SHA-256 inventory and compact receipt. Local retention is not an off-machine backup.

The result should separate verified implementation behavior, descriptive evidence
and unresolved causal hypotheses. Recommend the next concrete investigation or
controlled change from the findings; do not promote a policy, alter arrival criteria
or start another learning run as part of this audit.

## Results

All **36 reconstructed rollouts** matched their archived next collector states
exactly, covering **36,864 original sampled action ticks**. All model/optimizer
states remained unchanged. The **48 frozen-policy continuations** completed within
the original episode deadlines; all ended in deadline, with no successes or other
terminal outcomes. Total retained physics work including restores was **223,317
ticks**, plus **1,024 preflight ticks**, below the frozen budget. Concurrent workers
took 31.6–48.8 seconds each. No reconstruction, budget or numerical guard failed.

### Main finding: surviving timeouts have identical complete returns

The two continuations from every one of the **24 predefined boundary states**
used different actions, yet each pair had the same discounted return to floating
point precision. The actual paths and final distances differed substantially.

An illustrative pair (long-credit seed 43, checkpoint 32, near slot) began at the
same saved state, 4.75 units from the goal with speed 30.24 units/s:

| Continuation | Closest distance | Final distance | Discounted return |
| --- | --- | --- | --- |
| Repeat 0 | 0.42 | 29.30 | -1.6750660384641036 |
| Repeat 1 | 3.41 | 86.65 | -1.6750660384641034 |

Neither completed arrival; passing near the goal at high speed is still failure.
This example was selected after replay as the pair with the largest final-distance
difference. The return equivalence holds for all 24 pairs, not only this example.

A separately retained, post-replay algebraic check verified all 48 returns against:

```
timeout_return = -.001 * (1 - gamma**N)/(1 - gamma)
                 -2 * gamma**(N-1) - Phi(saved_start)
```

Here N is the remaining tick count to the deadline. Maximum formula error was
**9.11×10⁻¹⁵**. There is no final-distance, final-speed or final-spin term. Setting
terminal potential to zero makes all shaping telescope to the same initial-state
offset. From the same state, any trajectory that survives to that deadline without
success receives the same complete return. The current policy can therefore learn
to avoid collisions without learning a durable preference among partial approaches.

This is intended potential-shaping algebra, not evidence of a sign error in its
implementation. It exposes a limitation of the **underlying task reward**: genuine
successful arrival is the only positive terminal distinction between surviving
trajectories. Such arrivals were almost absent in the parent training runs. Dense
looking intermediate feedback does not imply that partial progress retains value
in the completed episode objective. This plateau is verified; it is not a proof
that it is the sole cause of every learning failure or that PPO cannot solve a
sparse-success task.

### Advantages do not uniformly punish useful-looking movement

The audit does not support a blanket claim that every useful turn or thrust receives
negative learning credit. In the long-credit arm, thrust while closing distance had
positive **mean normalized advantage at all six inspected stages**. For example,
at checkpoint 64, closing thrust averaged +.497 versus -.259 for opening thrust;
goal-aligned thrust averaged +.753. These are grouped observations, not controlled
comparisons, and individual ticks can differ. State visitation and momentum confound
an action's causal contribution. Raw advantages also differ from the normalized
values used by PPO.

At the same time, total direct thrust-score pressure was negative in five of six
long-credit stage aggregates, while the sampled action mix and states varied. Thus
some locally favorable segments can coexist with net pressure toward lower thrust.
The data do not show a simple universal wrong-sign rule. Full per-stage/group
counts, reward components, raw/normalized advantages and score pressures are retained.

### Critic estimates and gradient interaction

For each of 12 boundary states per arm, compare the frozen critic with the mean of
its two sampled continuation returns:

| Arm | Mean signed error (prediction minus return) | Mean absolute error |
| --- | --- | --- |
| Control (.98) | +.311 | .321 |
| Long credit (1.0) | -.029 | .118 |

Control predictions were optimistic in 11/12 inspected states. The long-credit
probes had smaller errors on their own visited states, despite their failure to
learn arrival. These are different states/policies, a small sample, and only
deadline outcomes; this is not broad value-function calibration or evidence about
prediction quality for successful maneuvers. The almost-zero two-sample return
spread is explained by the timeout plateau, not identical action sampling.

The critic also did not consistently overwhelm the shared policy gradient:

| First-minibatch metric, shared encoder/trunk | Control | Long credit |
| --- | --- | --- |
| Median weighted critic/policy gradient norm ratio | .45 | 1.80 |
| Critic norm larger than policy norm | 8/18 | 14/18 |
| Policy and critic gradients opposed (negative cosine) | 8/18 | 6/18 |
| Combined gradient opposed to policy gradient | 2/18 | 3/18 |

There are local conflicts and occasional large critic gradients, especially with
long credit. This does not establish persistent harmful dominance: these are the
first minibatches of selected updates, with no alternate Adam steps or later-update
causal ablation. The gradient sum and clipping calculation checked out. No mismatch
in exact replay, old likelihood reconstruction or gradient accounting was found
within this audit's scope.

### Recommended next step

Discuss and freeze a comparison that gives **partial goal progress lasting value
in the base training objective**, while retaining the original hard physical arrival
gate. For example, a bounded graded deadline outcome can distinguish stopping far
away from a near miss without counting either as success. Its exact formula and
weights need a separate specification and checks for undesirable behavior; this
audit does not select them or launch training. Changing another potential coefficient
alone would preserve the same complete-return tie among timeouts.

Preserve the native-free scope, 60 Hz cadence, model, task distribution and immutable
prior experiments when designing that comparison. Critic sharing and rollout length
remain possible later investigations, but this audit supplies a concrete objective
limitation to address first. No default reward, success criterion, training config,
policy or checkpoint was changed by the diagnostic.

### Verification and retention

All **four diagnostic tests** passed, including an exact archived rollout and a
gradient probe that leaves weights unchanged. Python compilation and whitespace
checks passed. The production learner and Rust code were unchanged; the original
source/runtime contracts were used for loading. Retained worker tensors contain
reconstructed data and gradients, not newly trained models.

The [results](data/flight-learning-signal-audit-v1-results.json) include all stages,
gradient cases and boundary predictions. The
[archive receipt](data/flight-learning-signal-audit-v1-archive.json) identifies
the frozen source/specification, 36 rollout/gradient/row datasets, 48 continuation
traces, tests, algebraic check and parent input hashes. All eleven prior manifests
and the 81 referenced parent input files remain unchanged. Source and runtime
dependencies on the parent archive are explicit. Local retention is not an
off-machine backup and this audit establishes no new flight capability.
