# Saved motor velocity-feedback audit v1

**Result: wrong-sign velocity feedback is confirmed; the original actor dominates
the partial learned correction in seed 43. No training or physical trials.**

The specification below was frozen before execution. A later verification-only
rounding correction is documented with the original failed log retained.

LF-062, 2026-10-25. Read-only audit; C4 remains thirty learning attempts
and adds its nineteenth audit. The user authorized inspecting saved policies to
understand why throttle and brake do not correct overshoot. No training, physics
steps, optimizer construction, new policy exports, checkpoint selection or changed
qualification gates. LF-061 remains sealed and unchanged.

## Questions and fixed evidence

Does increasing actual velocity above a fixed command reduce propulsion and
increase braking? Which response comes from the frozen original actor, raw velocity
features in the learned residual, and its explicit desired-minus-current velocity
features? Are observed trajectories explained by policy feedback, the native brake
cap, input saturation, or simply different visited states? Does the saved update
history show that feedback emerging, and how much relevant training exposure and
directional policy credit existed?

Use both LF-061 arms (`acquire`, `sustain`) and all seeds 17/29/43. Copy all 129
checkpoints and 128 rollouts per run, original initial/final exports and final
development rest-start traces for cases **0/16/32** (commands **6/12/18**). Freeze
common anchors at ticks **0/360/600** of each of these eighteen traces: **54 states**.
All selected traces extend beyond tick 600. Every policy is tested against the
same bank, with provenance retained; it is not restricted to its own visited states.
Use parent results and manifest as additional inputs: **1,574 copied files**.

## Controlled response and branch decomposition

At every anchor, hold heading error, lateral velocity, spin, gravity, actuator
limits and previous actions fixed. Sweep command speed through
**0/4/6/10/12/16/18/20**, and actual along-command velocity minus requested speed
through **-12/-6/-3/-1/0/1/3/6/12**. Translate COM and body-origin velocities by the
same vector; preserve their rotational offset. Recompute the existing derived
velocity-error features through the unchanged model graph. Negative actual forward
velocity is allowed; requested speed is always nonnegative. The counterfactuals
are consistent instantaneous observations, not new physical trajectories.

Record original actor logits, gated residual logits, final throttle/brake, hidden
tanh saturation (absolute activation >.95), and estimated instantaneous control
acceleration along the requested heading. Fixed checkpoints **0/1/4/16/32/64/128**
give **163,296 grid rows**. Repeat final grids with previous-action inputs set to
zero: **23,328 neutral-history rows**, explicitly a diagnostic history perturbation.

At zero forward-speed error for all anchor/command combinations, differentiate
actuator logits with respect to increasing actual forward velocity. Separate
original actor, residual raw-COM/origin velocity and residual explicit-error paths;
verify their sum and a central finite difference with ±.25 units/s. Also record
derivatives with respect to command and previous action, probability-space velocity
derivatives, and total versus fixed-action acceleration slopes: **2,592 rows**.
Zero-command residuals and their velocity derivatives must remain exactly zero.

For every saved update **0..128**, evaluate all common anchors at commands
**6/12/18** with errors **-3/0/3**: **376,164 rows**. This compares the same inputs
through training, without cherry-picking an intermediate policy. Initial/final
grid logits and value must agree with the original native exports:
**46,656 native/Python comparisons**, using schema/load/infer/close only.

## Native physics interpretation and training evidence

Read the existing production control law; do not change or simulate it. Open-wing
thrust is `45 * throttle * clamp((70-origin_forward_speed)/10,0,1)` along the ship's
forward axis. Braking opposes body-origin velocity with magnitude
`brake * min(4*origin_speed,40)`. Project their sum along the command heading.
Braking reaches its cap at origin speed 10; the throttle governor does not taper
until forward speed 60. Thus a roughly fixed throttle/brake pair can find a passive
low-speed equilibrium but provide no intrinsic correction in the 10–60 range.
This algebraic control term excludes future turning, contacts and future policy
actions. It is not a new physics rollout or proof of closed-loop stability.

Inspect all **786,432** saved main-training rows, reconstruct their behavior
likelihoods, and use the original separately normalized movement/stopping advantages.
Report movement, stopping, aligned underspeed/near-speed/overspeed, with overspeed
split by training commands 4/10/16. Alignment requires heading <=5 degrees, spin
<=.2 and lateral speed <=1. Record mean and sampled control acceleration, advantage,
mean throttle/brake, actual before/after-update changes on those same states, and
Gaussian mean-logit score times advantage. A sample is directionally more corrective
when its change from the mean acceleration opposes speed error; that metric does
not establish its actual future return or isolate a causal PPO mechanism.

This is a response/coverage audit, not a rerun of full reward/GAE reconstruction.
LF-061's prior verified records supply that provenance. Retain all groups, including
empty ones, and distinguish aggregate score signs from full parameter gradients.

## Validation, bounds and retention

Six synthetic tests cover consistent rotated perturbations, signed velocity error,
brake cap/governor, branch decomposition and finite differences, exact zero-command
protection, and training-readiness masks. Every loaded model remains unchanged;
parameter gradients are disabled, only input derivatives are computed. Torch RNG
must remain unchanged through each audit. Verify **774 frozen-parent checkpoints**.

Six audit processes, at most three concurrent, each capped at **600 internal /620
external seconds**. One verification process capped at **300/320 seconds**. Stop
on integrity/contract/parity failure without silently changing the frozen recipe.
No new weights, environment resets, restoration steps or scored evaluations.

Archive `data/training-experiments/learned-flight-motor-feedback-v1-20261025`, linked
to `/home/data2/space-wars3-artifacts/training-experiments/learned-flight-motor-feedback-v1-20261025`.
Freeze source/runtime/input/anchor/specification/runner hashes. Retain sources,
original native runtime, copied evidence, complete response tensors, training
summaries, commands, tests, logs and verification. Preserve all **57 prior
manifests**. Code remains in the standalone checkout. Local retention is not an
off-machine backup. No confirmation use, promotion or navigation qualification.


## Controlled response: what is going wrong

The observed overshoot is a property of the saved policy function, not just a
comparison of different visited states. Holding the other inputs fixed, both
seed-43 final policies increase throttle and reduce brake with increasing actual
speed at **all 54** anchors when the requested speed is 18. Sustained seed 43 has
positive acceleration slope at 51/54 of these states, and positive estimated
acceleration in **all 162** command-18 overspeed grid states (errors +3/+6/+12).

For a concrete common aligned state, use anchor 51: sustained seed 43's speed-18
rest case at tick zero. Every policy was evaluated at this same anchor. Heading,
lateral motion, spin, gravity and previous actions remain fixed:

| Actual speed, target 18 | Throttle | Brake | Estimated forward control acceleration |
| --- | --- | --- | --- |
| 12 | 13.908% | 12.808% | +1.135 units/s² |
| 18 | 14.102% | 12.479% | +1.354 units/s² |
| 24 | 14.191% | 12.202% | +1.505 units/s² |

It accelerates at the target and accelerates more when already too fast. These
are instantaneous native-control-law estimates, not new simulated trajectories.
They agree qualitatively with the preserved LF-061 overshoot trace.

## Frozen actor versus learned correction

The velocity-error features have the correct sign. The explicit residual error
path supplies a corrective response, but the unchanged original actor supplies a
larger wrong-sign response. At the same anchor and requested/actual speed 18:

| Contribution to logit derivative per +1 unit/s actual speed | Throttle | Brake |
| --- | --- | --- |
| Frozen original actor | +0.004757 | -0.007858 |
| Residual raw COM/origin velocity inputs | -0.001227 | +0.000027 |
| Residual explicit velocity-error inputs | -0.001552 | +0.003120 |
| Total | **+0.001978** | **-0.004711** |

The residual cancels about 58% of the inherited throttle slope and 40% of its
brake slope here, leaving both totals in the wrong direction. The fixed-state
±3-unit/s slope has these wrong signs in **all 129 checkpoints**, including the
initializer. Training changes the operating level substantially: at target speed
18 the control estimate moves from -2.23 units/s² initially to +1.35 finally,
without reversing the slope. Intermediate checkpoints are diagnostic history,
not selected replacement models.

The original actor was trained for zero-speed turning/stopping; its positive-speed
response was not qualified. Freezing it preserves zero-command competence but also
freezes this extrapolated response. The additive residual can theoretically cancel
it; the measured residual does not cancel enough under this recipe. This is an
optimization burden, not a proof that the model class cannot represent control.

Sustained seed 43's mean residual tanh saturation is only 0.64%/3.73% of units in
its first/second hidden layers across the grid, and its example actuator outputs
are far from sigmoid extremes. Resetting previous actions changes overspeed
throttle/brake by only about .046/.043 percentage points on average at command 18.
These measurements do not support widespread saturation or previous-action inputs
as the dominant explanation for this model's wrong-sign response.

## Why low-speed holding can look better

Native braking is `brake * min(4*speed,40)` opposing the body-origin velocity.
Below speed 10, a roughly constant throttle/brake pair has a natural equilibrium:
in aligned flight, speed is approximately `45*throttle/(4*brake)`. LF-061's
speed-6 steady controls, about .1062 throttle and .1754 brake, predict approximately
6.81 units/s, matching its retained trace. That balance is supplied largely by the
plant's speed-dependent braking, despite the policy's wrong-sign actuator slope.

Above speed 10, braking stops increasing automatically. Thrust does not taper
until speed 60, well above the requested 12/18 range. An approximately fixed
propulsion bias can therefore produce continuing acceleration. In the command-6
anchor set, sustained seed 43 has mean acceleration slope -0.671, mostly from the
fixed-action physics contribution -0.700. At command 18 the mean slope is +0.0287,
with only -0.0032 from fixed-action physics. The control response is insufficient
to stabilize that regime.

This explains the low-speed/high-speed contrast for the inspected model. It does
not imply all failures have the same cause or make the physical brake cap a bug.

## All-seed comparison and saved training evidence

At command 18, over all 54 anchors and three positive error levels:

| Arm | Seed | Overspeed states with deceleration /162 | Throttle slope sign at all 54 target-speed anchors | Brake slope sign at all 54 target-speed anchors |
| --- | --- | --- | --- | --- |
| acquire | 17 | 0 | Decreases | Increases |
| acquire | 29 | 0 | Decreases | Decreases |
| acquire | 43 | 0 | Increases | Decreases |
| sustain | 17 | 162 | Decreases | Increases |
| sustain | 29 | 162 | Decreases | Decreases |
| sustain | 43 | 0 | Increases | Decreases |

Correct slope signs alone are insufficient: acquisition-only seed 17 still has
positive propulsion at every tested overspeed state. Sustained seeds 17/29 instead
brake strongly throughout this region, including below the target: at actual speed
12 with target 18 the common-anchor estimates are -12.11/-15.39 units/s². Those
models suppress movement rather than track the command. This accords with their
unqualified LF-061 trajectories.

Sustained seed 43 receives **53,235 aligned underspeed rows** and **3,601 aligned
overspeed rows** out of 65,536 movement actions. Its overspeed counts at training
commands 4/10/16 are **1,720/282/1,599**. Relevant overspeed experience exists but
is much less common than underspeed. The combined overspeed mean-logit scores
favor less throttle and more brake, and its aggregate same-state updates do so.
That aggregate hides important speed differences:

| Training command, sustained seed 43 | Aligned overspeed rows | Mean throttle-logit score | Mean brake-logit score | Mean actual brake change per update on those rows |
| --- | --- | --- | --- | --- |
| 4 | 1,720 | -0.1241 | +0.1017 | +0.004923 |
| 10 | 282 | -0.0248 | -0.0243 | -0.006836 |
| 16 | 1,599 | -0.0196 | -0.0151 | -0.001224 |

The critical high-speed group does not consistently push braking upward. Scores
are advantage-weighted Gaussian mean sensitivities, not full-network clipped PPO
gradients; actual changes include all minibatches, shared parameters and optimizer
history. These findings constrain explanations but do not establish one reward,
advantage or architecture change as a sufficient remedy. The audit does not
reconstruct new counterfactual returns or claim that more training will solve it.

## Verification and correction record

All six audit processes complete in approximately 7–14 seconds each. All **46,656
native/Python comparisons**, **786,432 saved likelihood checks**, input-gradient
sum/finite-difference checks and **774 unchanged original-actor checkpoint checks**
pass. Model parameters and Torch RNG remain unchanged. Retain **163,296 full grid
rows**, **376,164 fixed-input history rows**, **23,328 neutral-history rows** and
**2,592 derivative rows**. No physical/reset/restore/training/optimizer steps,
new checkpoints, exports, confirmation use or promotion occur.

The original seventh process (verification) fails an overly strict diagnostic
comparison: sigmoid on a strided base-logit view differs from sigmoid on contiguous
total logits by at most **5.960464477539063e-8**, one float32 ULP at that scale.
All zero-command logits are bitwise identical and all residual logits are exactly
zero. This is a diagnostic tensor-layout artifact, not a changed stopping policy.

Preserve the original source snapshot, failed verifier and six completed audit
outputs. A declared verification-only revision compares exact logits and sigmoid
outputs using identical contiguous layouts. An added regression test passes with
the original six tests (**7 tests total**). A separate corrected verifier passes
in 1.12 seconds, reading the existing outputs; the six model audits are not rerun.
The revision sources, hashes, explanation and both verification logs are retained.
There are seven originally planned processes plus this one bounded corrective
verification process, with no altered data matrix or physical criterion.

Retain **1,619 frozen sources**, **1,574 copied inputs**, unchanged native runtime,
all response tensors, training summaries, descriptive analysis and commands.
Current-source differences from the freeze are exactly the two declared audit/
test revision files. All **57 prior manifests** and input origins remain unchanged.
See [results](data/flight-motor-feedback-v1-results.json) and
[archive receipt](data/flight-motor-feedback-v1-archive.json).

## Next decision

Design a bounded comparison of how movement learning shares the protected
baseline. The current additive residual must cancel the full frozen actor's
unhelpful positive-speed response before it can produce stabilizing feedback.
A candidate is a trainable movement actor blended with the frozen stopping actor
through the existing continuous command-speed gate, so zero-command outputs remain
exact while positive-speed learning can replace more of the inherited response.
This remains one learned graph, with no native controller or runtime handoff.

Specify initialization, function matching, trainable parameters and stopping
invariance before testing that candidate. Keep the current residual as the matched
control, all three seeds, tasks, reward, exploration, budgets and strict final
physical gates. Do not simultaneously change rewards or add a scripted brake rule.
This is a proposed comparison, not an implemented or proven fix.

The audit also finds uneven training pressure: in sustained seed 43's 1,599 aligned
overspeed rows at training command 16, mean brake-logit score and actual same-state
brake changes are negative. Freeing the movement actor may not solve that credit
problem. Retain these findings and inspect feedback as a diagnostic alongside the
unchanged acquisition/retention evaluation. No automatic training extension,
checkpoint selection, confirmation use or promotion; obstacle navigation remains
unqualified.
