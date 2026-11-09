# Paired motor training with persistent movement exploration

**Status: complete; both training arms fail the joint gate, 2026-10-25.** Experiment
`learned-flight-motor-persistent-v1-20261025`, journal LF-057. The user authorized
training with persistent exploration for movement and narrower noise for stopping
after the [frozen-policy audit](flight-motor-exploration-v1.md).

## Frozen paired comparison

Start both arms from all three original LF-053 final models, seeds **17/29/43**.
Copy actor, critic and learned deviations exactly; initialize fresh Adam, collector,
update counter, history and seeded Torch/Python RNG. Bind parent hashes. This is a
weights-only stage transition, not an old checkpoint resume. Keep seed 17's failed
heading behavior without recentering, replacement or checkpoint selection.

Keep the **LF-055 104-case matrix, ordering, task seeds, reward, architecture,
inputs, continuous controls, native dynamics, 60 Hz timing and 128-update budget**.
The 24 brake, 48 speed-tracking and 32 heading-rehearsal cases are unchanged.
Training speeds are 4/10/16, development 6/12/18; zero-speed braking and turning
rehearsal retain every original case. See [LF-055](flight-motor-velocity-v1.md)
for the full Cartesian matrix. Both arms use the same four persistent round-robin
slots, with 26 candidates each. No case additions or adaptive curriculum changes.

| Arm | Positive speed commands | Zero speed commands | Steering |
|---|---|---|---|
| control | Original independent throttle/brake noise | Original independent noise | Original independent noise |
| persistent | 8× learned actuator deviation, rho .95 | Original deviation, rho 0 | Original independent noise |

Unlike LF-056's mean-steering audit, **both training arms sample steering** from
the original learned deviation, without temporal persistence or extra scaling.
The comparison changes only the movement actuator exploration contract. Original
noise on stopping is narrower relative to the movement setting; it is not a newly
reduced stopping deviation. One learned motor network owns all actuators.

The positive-command condition reads the existing command-speed input. It changes
training exploration only, with no action override, controller handoff, native
teacher or rescue. Native mean-action exports remain unchanged in format and
are scored without exploration. The control arm uses the original sampling and
PPO calculation paths. Compare its final model/Adam/collector/RNG/history and
export against LF-055, excluding the intentionally different contract metadata;
this is a historical reproduction check, not permission to relax exact resume.

## Conditional policy and saved state

For current actor means `mu`, previous means recomputed under current parameters
`mu_prev`, recorded previous latent `a_prev`, correlation `rho`, and learned
clamped/exponentiated deviation `sigma`, the conditional Gaussian is

`Normal(mu + rho*(a_prev-mu_prev), m*sigma*sqrt(1-rho^2))`.

Steering always has rho 0 and m 1. Movement actuators have rho .95 and m 8;
stopping actuators have rho 0 and m 1. At episode start, invalidate history and
set effective rho to zero, giving the full marginal initial deviation. Keep
history across rollout/update boundaries, and clear it on every terminal reset.
Store prior observations, sampled latents and validity per slot in both rollout
records and checkpoints. The current policy recomputes both actor means during
PPO, including gradients through the previous mean; do not freeze it to behavior
policy outputs. Use the actual conditional variance and stable tanh/sigmoid
Jacobian terms in likelihood and entropy. All latents execute through the usual
actuator transforms, including extreme but valid samples.

Exact continuation requires the full same-stage source/runtime/config identity,
optimizer, collector, history, RNG and autoregressive history. A small optional
policy-terms callback in shared PPO leaves its default path unchanged. No new
network inputs or architecture parameters, no native physics changes.

## Fixed optimizer, reward and physical acceptance

Use the LF-055 smooth cost:
`-[1-cos(heading_error)+.1*(spin/1.8)^2+(velocity_error/5)^2]/60`,
plus +1 on motor success, −5 on a physical failure and zero extra at deadline.
No effort penalty, native imitation or altered terminal criteria.

Each main arm/seed gets **128 updates**, four slots ×256 ticks/update, four
PPO epochs, minibatch 256, learning rate .0003, Adam epsilon 1e-5, gamma .99,
lambda .95, clip .2, value weight .5, entropy weight .001, combined gradient clip
.5 and target KL .03. Cap each main process at 900 seconds internally/920 externally.
No retries, extensions or intermediate-checkpoint scoring.

Every main training case must complete at least three episodes. Score initial and
final native mean-action exports on all 104 development cases. Acquisition must
occur by tick 360; continue through tick 900. Require heading ≤5°, COM velocity-vector
error ≤1 unit/s and spin ≤0.2 for thirty complete intervals. Any subsequent tolerance
exit permanently fails retention. Physical failures terminate immediately.

Keep the **same LF-055 joint gate**: at least two of the same three seeds must
pass all conditions in an arm:

- Braking ≥20/24; ≥6/8 per initial-speed group and ≥10/12 per turn sign.
- Tracking ≥40/48; ≥12/16 per command speed and ≥4/6 per initial-velocity mode.
- Tracking gain ≥24 over the same model's initial mean policy.
- Retain ≥20/24 original turns, ≥9/12 per turn sign, ≥7/8 stationary cases acquired
  at tick 30, and lose **none** of the parent's heading successes. Seeds 29/43
  must preserve all 32; seed 17 cannot pass through an empty prior-success set.
- Zero final physical failures across all 104 cases.

Report both arms, paired tracking/braking gains, all training failures and all
heading regressions, even if no arm passes. The experiment's pass flag requires
the persistent arm to pass this competence gate; improvement over a poor control
alone is insufficient. No reserved confirmation use, model promotion or navigation
qualification is implied.

## Bounded preflight before main trials

After implementation tests, freeze the entire specification/source/configs before
preflight. Six preflights use two updates each on the declared training subset
**indices 0/24/40/56**, one persistent slot per case: one zero-speed braking start
and movement commands 4/10/16. Use the intended stochastic steering and actuator
samplers, original parent weights and the same optimizer settings. No evaluation
or selection from these fitted preflight weights; every main run starts again from
its original LF-053 parent. Preflight cap: 120 seconds internal/140 external.

For each preflight, repeat update 2 in a fresh process from checkpoint 1 and demand
exact full-state/rollout equality. Reconstruct native rewards and outcomes, require
all four cases completed and no physical failures. Recompute all behavior
likelihoods at their recorded pre-update weights. In each experimental preflight,
require at least 1% of movement samples above 20% throttle and at least 1% below
10% brake, ensuring the intended sampler actually broadens action exposure.
Report sampled motion and heading/spin fractions; these are diagnostics, not a
replacement for the eventual deterministic competence gate. If preflight fails,
stop before main training without tuning the frozen pilot.

## Retention and total budgets

Six preflights plus six preflight continuations, one verifier; six main pilots
plus six main continuations of update 128 from checkpoint 127, one final verifier:
**26 bounded processes**. Each verifier gets 300 seconds internal/320 external.

- Main training: 786,432 ticks; main repeated update ticks: 6,144.
- Preflight training including repeated updates: 18,432 ticks.
- Total: **811,008 training action ticks**, at most **12,672 optimizer steps**.
- At most **14,760 active-trace restoration ticks**; report neutral setups separately.
- **1,248 scored evaluation episodes**, at most **1,123,200 action ticks** and
  **1,248 native/Python inference comparisons**.
- Recompute **798,720 behavior log probabilities** over all unrepeated main/preflight
  rollouts, with float32 batching tolerance 1e-4 absolute/1e-5 relative; exact repeated
  rollout comparisons cover the twelve restarted updates.

Retain **804 new checkpoints, 48 policy exports and 792 rollout tensors**, all
completed/active training and development traces, parent inputs, historical controls,
source/runtime/dependencies, frozen matrices/configs, tests, commands, times and exits.
Verify twelve exact resumes, all parent initializations, 192 original retention
traces and 312 paired initial development traces. Source/input hashes and all
**52 previous manifest hashes** must remain unchanged before sealing. Local
retention is not off-machine backup.

## Results

Persistent movement exploration greatly broadens training actions, but every final model still passes 0/48 speed-tracking cases; both arms fail the joint gate.

All six pilots finish the declared **128 updates**, following a passing six-run
preflight. Every initial/final speed-tracking set has zero retained cases; no final
tracking case even acquires by the six-second deadline. All 1,248 development
episodes reach the full fifteen-second horizon without a physical failure.

| Arm | Seed | Brake retained /24 | Track retained /48 | Original turns retained /24 | Stationary retained /8 |
|---|---:|---:|---:|---:|---:|
| Control | 17 | 0 | 0 | 0 | 0 |
| Control | 29 | 0 | 0 | 0 | 0 |
| Control | 43 | 24 | 0 | 24 | 0 |
| Persistent movement | 17 | **24** | 0 | **24** | **8** |
| Persistent movement | 29 | 0 | 0 | 0 | 0 |
| Persistent movement | 43 | 0 | 0 | 0 | 0 |

The control arm reproduces **LF-055 exactly**, apart from its deliberately changed
contract metadata: final model, Adam, collector, RNG, update/history and policy
export all match in every seed. This supports the integrity of the unchanged
control calculation path; it is not an old-runtime checkpoint resume.

The persistent variant improves seed **17** from its original roughly 19.57°
zero-speed heading offset to **3.00°**, passing all 24 braking, 24 turn and eight
stationary retention cases. That is a measured zero-speed gain, but the model still
fails all nonzero-speed tasks. Seeds **29 and 43 lose all 32 original heading
successes** under the experimental arm. Their zero-speed heading errors settle
near **6.60° and 13.46°**, respectively. Narrow noise during stopping does not
prevent shared learned parameters from regressing during mixed-task training.

All models still decelerate near rest. Experimental braking-case median late
speeds are **.261/.231/.158** units/s; failed braking qualification in seeds 29/43
comes from heading error. No velocity-stage model is promoted or substituted for
the earlier heading-capable baseline.

### Exploration changes substantially; mean propulsion does not

Across main-training movement samples:

| Arm | Samples | Throttle >20% | Throttle >50% | Brake <10% | Throttle >50% and brake <10% |
|---|---:|---:|---:|---:|---:|
| Control | 193,148 | 2 | 0 | 0 | 0 |
| Persistent movement | 194,333 | **49,636** | **30,395** | **45,061** | **7,249** |

The experimental sampler produces strong propulsion and brake-release actions
with ordinary stochastic steering enabled. Completed movement-training episodes
reach peak COM speeds **50.49/55.28/39.43** units/s in seeds 17/29/43. The
zero-speed sampler remains narrow: across **198,883 stopping samples**, none has
throttle above 50% or brake below 10%, and only one exceeds 20% throttle.
The command-conditioned exploration contract therefore executes as declared.

Nevertheless, final native mean-action tracking trajectories use median throttle
only **1.44%/1.70%/.91%**, with brakes **61.06%/82.84%/59.60%**. At requested
speed 12, their median late velocity RMS is **11.736/11.771/11.832** units/s.
The corresponding controls are 11.385/11.790/11.531: no consistent improvement.
This is a gap between explored action coverage and learned mean-action selection,
not evidence that extra sampling alone has solved velocity control.

No completed sampled-training movement episode succeeds either: experimental
seeds finish **175/172/192 movement episodes**, all at deadline. Their 10/0/87
training successes occur in zero-speed families. Experimental learned base
actuator deviations remain roughly .46–.52; after the fixed 8× multiplier, movement
marginal deviations are **3.67–4.13** in latent units at the final checkpoint.
These are descriptive checkpoint measurements, not a proof that persistent noise
or the reward is the sole cause. Model weights are shared across task families,
and stochastic training return differs from deterministic deployment behavior.

The matched comparison fixes initial model weights, seeds, task recipe and
optimizer settings. It does not promise identical realized action sequences or
episode schedules: the legacy control and joint conditional Gaussian consume
random samples differently, and subsequent trajectories and updates diverge.

### Preflight and correctness evidence

All **30 Python tests** and **48 native environment tests** pass. New tests cover
conditional means/variances, full marginal noise at reset, original stopping
likelihood, transformed densities, gradients through the previous policy mean,
residual clearing, real-collector likelihood reconstruction, exact continuation
and rejection of cross-arm contracts. The native runtime and network remain unchanged.

All six preflights complete, visit every declared case and have no physical failure.
Experimental movement samples exceed the fixed throttle/brake coverage thresholds:
322–464 of 1,536 samples per seed exceed 20% throttle; 455–503 release brake below
10%. Sampled steering is active. Preflight weights are discarded; all main runs
start again from the original LF-053 models with fresh Adam/collector/history.

All **twelve exact resumes** reproduce complete checkpoints and repeated rollouts.
All **798,720 behavior likelihoods** reconstruct at the original pre-update weights;
maximum observed absolute log-probability difference is **1.91e-6**, below the
frozen batching tolerance. Conditional history matches across rollout boundaries,
and reset validity/cleared state checks pass. No KL early stopping occurs: each
main pilot completes all **2,048 optimizer steps**. The failed gate is not explained
by skipped updates or a detected likelihood/resume mismatch.

Every one of the 104 main training cases completes **three or four times in every
run**. Physical errors, dwell, retention, outcomes and rewards reconstruct from
native traces. All **1,248 native/Python inference comparisons**, **192 original
parent retention traces**, **312 paired initial development traces**, twelve exact
parent initializations and three historical control checks pass.

### Budgets and retained result

All **26 bounded processes** exit zero without retry. Main pilots take
**111.75–124.16 seconds** internally and **113.92–126.38 seconds** externally.
Final verification takes **89.87 seconds** internally/91.79 externally, within its
frozen limit. Use **811,008 training action ticks**, **12,672 optimizer steps**,
**9,599 restoration ticks**, **1,123,200 scored evaluation ticks** and **3,665
neutral setup ticks**. Every declared cap is met; no extensions or extra training.

Retain **804 checkpoints, 48 policy exports, 792 rollouts**, every training and
evaluation trace, 1,605 frozen sources, 110 copied inputs, configurations/tasks,
preflight results, tests, commands, timings and read-only summaries. All **52 prior
manifest hashes** and copied input origins remain unchanged. See
results (`docs/data/flight-motor-persistent-v1-results.json`, in git history) and
[archive receipt](data/flight-motor-persistent-v1-archive.json). No reserved
confirmation use, model promotion or navigation qualification. Local retention
is not off-machine backup.

## Next decision

Freeze a **read-only learning-signal and update audit** of the retained LF-057
rollouts/checkpoints before another training change. Trace movement samples from
physical velocity error through reward, bootstrap/advantages, conditional policy
likelihood and actual changes in mean throttle/brake output on matched states.
Separate underspeed from overspeed and account for heading error, so useful braking
is not confused with suppressed propulsion. Inspect whether the persistent noisy
training objective favors conservative mean actions, whether advantages carry the
needed credit, and whether updates retain heading through movement/stopping blocks.
These are hypotheses to distinguish, not established causes.

Freeze update indices, state anchors, comparisons and a bounded analysis budget
before the audit; include all three seeds and both arms. Use existing evidence,
with no new optimizer updates, extra training time, reward changes or selected
intermediate model. Keep the original LF-053/LF-054 heading baseline and all failed
LF-055/LF-057 results. Seed 17's new zero-speed competence is recorded separately;
it does not promote the failed velocity stage. No native teacher, fallback,
controller handoff or navigation integration.
