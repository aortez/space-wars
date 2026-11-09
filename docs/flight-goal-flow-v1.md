# Goal-path and policy-gradient audit

**Status: complete, 2026-10-24.** Experiment
`learned-flight-goal-flow-v1-20261024` follows LF-045's unsuccessful goal-scaling
pilot. Inspect both schema and goal-only32 arms, seeds 17/29/43, before updates
1 and 16 (saved checkpoints 0 and 15). No optimizer step or policy change occurs.

## Exact replay and fixed measurements

Restore each of twelve saved states under its original strict source/config/runtime
contract. Collect its original 1,024-row rollout and require the complete collector
state to equal the saved following checkpoint (1 or 16). Independently reconstruct
rewards, values, terminal-aware GAE and return targets exactly. Preserve model,
optimizer and history. Save the post-collection RNG; analysis must not advance it.
Do not claim to reproduce an entire optimizer update: no optimizer update is applied.

For each rollout inspect the actual first 256-row PPO minibatch, selected from the
saved post-collection RNG with the production permutation and entropy draw. Use
the production head-only critic routing, quarter noise and globally normalized
advantages. Separate policy, weighted critic and weighted entropy gradients, then
split the policy contribution by settled/position/velocity/spin family. Each family
retains the full minibatch denominator; its contribution vectors must sum to the
whole policy gradient. Report all-parameter and goal-column norms, global clipping
scale, and position-versus-other-family goal-gradient alignment. These are raw
gradient measurements, not Adam-update attribution. Critic-derived advantages and
global clipping remain part of training even though direct critic gradients do not
reach the shared encoder. Verify the first minibatch is below the existing KL limit.

On all 256 position rows in each selected rollout, trace the production forward
path through joined features, both fusion preactivations/tanh layers, actor outputs
and decoded controls. Measure Jacobians with respect to each physical ship-local
goal coordinate, consistently accounting for the 300-unit schema scale and any
training-only rescaling. Retain every preactivation, activation and derivative.
Report tanh slopes and the fraction at or below .01, plus how much each tanh reduces
the incoming goal derivative. This threshold is descriptive, not a qualification gate.
Independently reconstruct the derivative chain from the captured activations/weights.

For an algebraic comparison, replace either or both fusion tanh slopes by one in
the derivative chain, leaving all forward outputs unchanged. Report the resulting
control-derivative norms. This estimates local attenuation by the activation slopes;
it does not create a usable controller or demonstrate an architecture improvement.
Synthetic finite-difference and saturation tests validate units and interpretation.

Retain all rollout rows' sampled controls, advantages, critic targets and score
pressure. Group position rows by signed direction, distance decreasing/increasing,
and observed heading alignment improving/worsening. Alignment can change through
translation as well as rotation; these descriptive groups do not supply target
actions or prove a sampled action caused its advantage. Report absent directions
explicitly: first/last rollout selection need not cover every signed position case.

Separately inspect every completed position trajectory from all sixteen original
pilot updates. Report directions, outcomes, radius entry, distance and accumulated
turn with the same physical reconstruction used by LF-044. These read-only path
summaries provide broader experience context; they do not extend the selected
gradient sample. Active, budget-censored paths are excluded from this completed-path
summary but their selected rollout rows remain in the gradient/layer analysis.

## Budgets and retention

Twelve sequential cases, each ≤180 seconds; global limit 1,200 seconds with an
external 1,220-second watchdog. Replay exactly 12,288 original rollout action ticks,
plus the active prefixes and neutral setup counts calculated from frozen checkpoints
in replay-budget.json. No extra seed, checkpoint scan, resampling, retry, new flight
evaluation, counterfactual flight, optimizer branch or continuation.

Freeze 1,573 sources, all 24 required checkpoints, completed position traces, parent
results, runtime/dependencies and all 41 previous manifest hashes. Eighteen focused
tests cover derivative units, finite differences, function-preserving scaling,
saturation interpretation, family normalization, critic routing and RNG neutrality.
Production sources and runtime are unchanged from LF-045; native validation remains
the retained 421-pass/two-ignored baseline. Retain rollouts, reconstructed targets,
collector/RNG snapshots, gradient vectors, layer tensors, row summaries, scripts,
timings and interpretation. Verify input/source hashes and seal the archive. Local
retention is not off-machine backup. No model promotion or confirmation use.

## Results

The audit confirms a strongly attenuated goal path in two seeds and uneven position-learning signals; no policy has changed.

The rescaling mechanism works: in the first rollout of every seed, goal-column
policy-gradient norms are exactly **32×** the schema arm while physical goal
sensitivities remain identical. LF-045's negative result is therefore not explained
by a missing scaling operation.

The second fusion tanh passes only **2.3–6.5%** of the incoming goal-derivative norm
in seeds 17/43 at the inspected states. Seed 29 passes **30.5–33.9%**. The first
fusion tanh passes roughly 29–52%, so attenuation also occurs earlier, but the
second layer is particularly restrictive in seeds 17/43. This is partial saturation
and direction-dependent damping, not a claim that every neuron is saturated.

Replacing only the second tanh's slopes with one in the algebraic derivative chain
raises the mean control-sensitivity norm by **15.6–45.0×** in seeds 17/43 and about
**1.9–2.0×** in seed 29. No forward output was changed or flown. Removing slopes
from both layers does not always help more: readout directions can cancel. These
calculations identify local attenuation without demonstrating that a changed
activation or architecture would learn to fly.

The table describes the models **before** each named update: checkpoints 0 and 15,
not the post-update-1 or post-update-16 policies. Pass percentages are means of
per-row derivative-norm ratios. Control sensitivity is the mean norm of the decoded
three-control Jacobian with respect to the two physical goal coordinates.

| Arm | Seed | Update being prepared | First tanh passes | Second tanh passes | Control sensitivity / physical goal unit | Second-slope-one comparison multiplier |
|---|---:|---:|---:|---:|---:|---:|
| schema | 17 | 1 | 33.5% | 2.3% | 3.72e-07 | 45.0× |
| schema | 17 | 16 | 35.8% | 5.6% | 9.58e-07 | 15.6× |
| schema | 29 | 1 | 52.4% | 33.9% | 0.000159 | 1.9× |
| schema | 29 | 16 | 51.6% | 30.9% | 0.000151 | 2.0× |
| schema | 43 | 1 | 35.0% | 6.5% | 7.6e-07 | 17.3× |
| schema | 43 | 16 | 29.1% | 3.9% | 5.77e-07 | 16.1× |
| goal | 17 | 1 | 33.5% | 2.3% | 3.72e-07 | 45.0× |
| goal | 17 | 16 | 33.3% | 5.1% | 9.52e-07 | 17.3× |
| goal | 29 | 1 | 52.4% | 33.9% | 0.000159 | 1.9× |
| goal | 29 | 16 | 52.5% | 30.5% | 0.000177 | 2.0× |
| goal | 43 | 1 | 35.0% | 6.5% | 7.6e-07 | 17.3× |
| goal | 43 | 16 | 29.7% | 4.2% | 7.54e-07 | 19.5× |

### Training signals and experience

Position-policy gradient contributions oppose the combined contributions of the
other families in **8/12 inspected first minibatches**. They align in the other
four, so cancellation is not a uniform explanation. Direct critic gradients on
the goal connections are zero, as required by head-only training. Weighted entropy
is much smaller than the policy contribution on those connections.

Global clipping multiplies raw gradients by **.0288–.0949** in these minibatches.
Critic-head gradients contribute to that common norm despite being detached from
the encoder. These are gradient scales; Adam state and epsilon affect actual
parameter updates, which this audit deliberately does not execute. Parameter-space
norms also depend on parameterization and are not measures of control competence.

There is a marked distinction between raw and normalized advantages. Every
inspected leftward position row has negative normalized advantage. In the last
rollouts of seeds 29/43, however, about **88–98%** of those raw advantages are
positive. Other task families' larger returns raise the whole-batch mean that is
subtracted. Negative normalized advantages alone do not prove a reward bug or an
incorrect update: discouraging poorly sampled actions can still improve a policy.
The retained signed score-pressure and family-gradient vectors preserve that nuance.

The selected position rows cover **2,000 forward and 1,072 leftward observations**,
with no backward/rightward observations. Thus the gradient findings cannot be
generalized to those absent directions. Across the full pilot, the 28 completed
position paths include 18 sideways/backward attempts (nine in each arm); none
enters the goal radius. All six sampled position successes are forward. These
are paired training attempts, not independent confirmation; unfinished paths are
excluded from this completed-path summary as declared.

| Arm | Seed | Update being prepared | Position raw advantage mean | Position normalized advantage mean | Positive raw → normalized rows | Position-versus-other goal-gradient cosine |
|---|---:|---:|---:|---:|---:|---:|
| schema | 17 | 1 | 0.1551 | -0.2485 | 100.0% → 0.0% | -0.186 |
| schema | 17 | 16 | 0.1310 | 0.0150 | 100.0% → 64.5% | +0.498 |
| schema | 29 | 1 | 3.5038 | -0.2333 | 100.0% → 25.8% | -0.308 |
| schema | 29 | 16 | 0.1121 | -0.7408 | 88.3% → 0.0% | +0.085 |
| schema | 43 | 1 | 0.1415 | -0.6657 | 100.0% → 0.0% | -0.502 |
| schema | 43 | 16 | 0.1276 | -0.6100 | 98.4% → 0.0% | -0.589 |
| goal | 17 | 1 | 0.1551 | -0.2485 | 100.0% → 0.0% | -0.186 |
| goal | 17 | 16 | 0.1299 | -0.0250 | 99.6% → 62.1% | +0.683 |
| goal | 29 | 1 | 3.5038 | -0.2333 | 100.0% → 25.8% | -0.308 |
| goal | 29 | 16 | 0.1161 | -0.4940 | 87.9% → 0.0% | +0.455 |
| goal | 43 | 1 | 0.1415 | -0.6657 | 100.0% → 0.0% | -0.502 |
| goal | 43 | 16 | 0.1277 | -0.6100 | 98.4% → 0.0% | -0.600 |

### Verification and retention

All twelve original collectors replay exactly and every reward/value/GAE target
reconstructs exactly. The analytic derivative chains match automatic differentiation;
synthetic finite differences and saturation checks pass. Family contributions sum
to the complete first-minibatch policy gradient, and loss components sum to its
total gradient. The inspected first minibatches satisfy their existing KL limit.
Model weights, optimizer states, histories and post-collection RNG remain unchanged.

The process completed in **35.97 seconds**, exit zero, with **18 focused tests
passing**. Replay consumed **12,288 original rollout action ticks**, **6,568 prefix
action ticks**, and **105 neutral setup ticks**. Analyze **3,072 position rows**,
twelve first minibatches and all **28 completed position trajectories**. There were
zero optimizer steps, new training trials or flight evaluations. No case was retried.

Preserve 1,573 frozen sources, 53 copied inputs (24 checkpoints, 28 traces and parent
results), all derivative/gradient tensors, reconstructed batches/targets, collector
and RNG snapshots, reports and scripts. Production sources/runtime are unchanged
from LF-045; the retained native baseline is 421 passed/two ignored. All 41 previous
manifests and copied inputs remain unchanged. See results (`docs/data/flight-goal-flow-v1-results.json`, in git history)
and [archive receipt](data/flight-goal-flow-v1-archive.json). No model was promoted
or reserved confirmation used. Local retention is not off-machine backup.

### Next decision

The next proposed experiment is a small **learned goal-residual branch** feeding
the actor's raw steering/throttle/brake outputs directly, bypassing the existing
fusion tanh layers. Use the two local goal coordinates at a declared task scale,
one small hidden layer, and a zero-initialized final layer so the original policy's
starting controls are preserved. Keep the existing sensor encoder and trainable
holding policy, head-only value training, reward, normalization, exploration and
task distribution fixed for the paired comparison. This is a learned path inside
the same controller; it introduces no native teacher, target-action labels or handoff.

Before training, require initial-function and native/Python parity checks, matched
sampling RNG and first rollout, versioned export/checkpoint compatibility, and a
bounded multi-seed pilot with the existing physical position-gain/holding gates.
Keep schema scaling in the main path; do not combine this architecture test with
advantage-normalization or exploration changes. The weak experience and mixed-task
signals remain open issues. A more direct goal path is a testable response to the
measured attenuation, not a guaranteed cure. Freeze its architecture and budgets
before execution. No residual branch has been implemented or trained in LF-046.

