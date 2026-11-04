# Protected per-command saved-gradient audit v1

LF-068, 2026-10-25. C4 audit 22; learning attempts remain 33.
Frozen before diagnostic replay. The user authorized inspecting each requested
speed's contribution to saved updates, including conflicting learning signals,
while keeping the stopping baseline protected.

## Scope and hypotheses

LF-067 balanced collection presents commands 4/10/16 in every rollout but adds no
strict tracking successes over initialization. Inspect the actual saved updates
of both **cyclic / balanced**, seeds **17/29/43**, at updates **1/16/43/64/86**.
These points are fixed before inspecting gradients. This is a sparse audit of
30 updates, not an exhaustive account of all 516 training updates.

Distinguish per-command gradient conflict, advantage centering, shared gradient
clipping, entropy, and incoming optimizer momentum. Gradient disagreement is not
by itself proof that the network is too small, or that a different objective
would improve physical outcomes. No reward, architecture or training change is
part of this experiment.

## Exact replay and stopping protection

Copy each selected rollout and its before/after model, optimizer and random
states. Reproduce 256 six-row sampling calls without physics, then four epochs
of six 256-row minibatches using the original loss and optimizer expression
order. Require **bit-exact final model, optimizer, RNG and recorded metrics**
against each saved update. Additional autograd calls must not consume RNG or
alter accumulated gradients. Preserve the original model's frozen parameters.

Only private copies change. Record these as **720 diagnostic optimizer steps**,
184,320 replayed minibatch rows and 46,080 checked behavior samples. No new
training trajectory, physics tick, training checkpoint or policy export. Retain
the diagnostic replay states as evidence, without selecting or promoting them.

Check 60 before/after frozen-parent actor states and **23,040 saved stopping
rows**. Zero-command logits and action distributions must remain exactly equal;
stopping actor gradients and zero-command control Jacobians must be exactly zero.
The original strict evaluation, failures and LF-053 stopping baseline remain
unchanged. No native fallback, rescue or controller handoff is introduced.

## Gradient and Adam accounting

Split each minibatch's actual actor loss into commands 4/10/16 using the original
**shared movement denominator and .5 coefficient**. Do not silently reweight an
unequal minibatch by averaging each command separately. Record stopping policy,
entropy and weighted value gradients separately. Their sum must match the
original combined gradient within float32 tolerance (absolute 1e-6 /relative
1e-4). Store the vectors, indices, group counts and clipping factors.

Measure pairwise gradient cosine on both all trainable actor parameters and the
deterministic-action parameters excluding noise. Norms <=1e-12 or absent commands
give undefined cosine, not agreement. Report negative cosines and those below
−.5, with the number of defined comparisons. These repeated minibatches are not
independent statistical trials. Report per-seed/per-update differences.

Track the first-moment contribution from each component through each optimizer
step, with **incoming momentum as a separate component**. Allocate parameter
changes through the actual Adam second-moment denominator, bias correction and
actual joint gradient clipping. Use float64 bookkeeping and retain float32
rounding as an explicit residual, bounded at 2e-6 per coordinate per step.
Retain the additional telescoping-rounding residual across 24 steps.

This allocation is **conditional on the actual combined optimizer path**. It is
not the causal update that would result from removing a command; clipping and
Adam's second moments would then differ. Verify allocated changes plus rounding
reconstruct actual changes. Distinguish critic's zero direct actor gradient
from its indirect effect through the global norm; compare actual clipping with
the clip factor for actor/entropy alone. No alternative optimizer path is run.

## Normalization and control projections

At the saved before-policy, compute full-rollout command gradients for raw
advantages, raw divided by the pooled movement standard deviation, the existing
pooled movement centering, and own-command centering **with that same pooled
standard deviation**. The latter isolates centering from scaling; it is a
diagnostic, not a newly tested training objective. Record group advantage means,
standard deviations, positive counts and raw-to-pooled sign changes.

Use the inherited **54 common anchors**, commands **0/4/10/16**, and
actual-minus-command offsets **−3/0/+3**, giving **648 rows**. Preserve heading,
spin, lateral motion and previous actions while translating COM/body-origin
velocities together. Require **7,776 native/Python inference comparisons** on
initial/final saved policies. No native environment reset or step is needed.

At each before-policy calculate force Jacobians for the mean across 54 anchors,
the previously illustrative anchor 51, and saved ready-near-target under/over
groups at each training command. Readiness uses heading <=5 degrees, spin <=.2,
lateral speed <=1; near-target sides are [−3,0) and (0,3]. Missing groups stay
explicit. Seed 17 may not supply ready higher-speed states.

Project each command's negative gradient and conditional Adam allocation onto
these controls, then compare the full saved update's linear prediction with its
actual nonlinear force change on the identical inputs. Positive change helps an
underspeed state; negative change helps an overspeed state. At target speed
report force/bias without assigning a universally helpful direction. The force
calculation is an instantaneous control term, not a simulated trajectory or
qualification result. Fixed-input probes do not reproduce closed-loop history.

## Budget, verification and retention

Six audit children, at most three concurrent, each <=600 seconds internally and
620 seconds externally; one independent verifier <=300/320 seconds. Stop on a
failed assertion and retain failures. No automatic training extension or replay
selection. Python tests cover unequal weighting, missing groups, centering,
exact replay against the unmodified trainer with existing momentum, known Adam
allocations, finite-difference control Jacobians, zero-command protection and
sampling RNG. Reuse seven existing force/input tests and inherited native tests;
no Rust/runtime change.

Archive `learned-flight-motor-gradient-v1-20261025` on the artifact disk. Freeze
all 1,635 inherited sources plus three new audit files, the runtime, specification,
runner and selected input inventory. Copy **105 inputs**: 60 checkpoints, 30
rollouts, 12 initial/final exports, anchors and two parent result files. Preserve
all **63 prior manifests** and verify the complete LF-067 parent archive before
and after execution. Retain every tensor, result, command, log and validation
record; append findings below without editing this frozen specification.
Confirmation remains unused, no model is promoted, and navigation is unqualified.


## Results — all declared replays complete

**The commands do compete through the shared actor, but conflict alone does not
explain the failure to learn feedback.** Every selected update reproduces the
saved model, optimizer, RNG and metrics exactly. All seven child processes exit
zero, with no post-freeze code change, retry or new training. The six audits take
2.97–3.37 seconds externally; verification takes 3.65 seconds internally /4.67
externally. The 720 optimizer steps only reconstruct already saved updates.

### Conflicting gradients and actual control effects

On deterministic-action parameters, excluding noise, the balanced runs have:

| Seed | Negative minibatch command-pair cosines | Cosines below −.5 | Mean cosine |
|---|---:|---:|---:|
| 17 | 173 /360 | 119 /360 | +.036 |
| 29 | 173 /360 | 100 /360 | +.024 |
| 43 | 183 /360 | 125 /360 | −.003 |
| Total | **529 /1,080 (49.0%)** | **344 /1,080 (31.9%)** | +.019 |

Including the noise parameters gives 530 negative and 308 below −.5. Each
minibatch has all three movement commands. A negative cosine means descent on
one command locally increases another command's sampled surrogate loss; it does
not measure physical performance, nor prove persistent opposition. These are
correlated minibatches from only five updates per run. Full-rollout before-policy
pooled gradients conflict in **17/45** command pairs, so minibatch disagreement
also includes sampling variation and movement along the PPO update path.

The cyclic sample contains 33/72 negative defined pairs, all at update 64; the
other 1,008 possible pairs are undefined because commands are absent. None of
these fixed cyclic snapshots contains command 16. This limits what this sparse
control sample says about cyclic command-16 learning and prevents treating its
conflict rate as a matched all-command comparison. No checkpoints were added
after seeing this coverage limitation.

Across the balanced runs' 45 command/update pairs on the 54-anchor mean panel,
**38 change force in the same direction at both −3 and +3 speed error**. Only
three increase acceleration below target while decreasing it above target; four
change both sides in the wrong direction. Thus an update commonly shifts the
acceleration/braking level broadly instead of improving correction on both sides
of the requested speed. Only 17/45 strengthen the sampled force-response slope.
The cyclic sample has 44/45 same-direction changes. These are local changes;
a broad shift can help an existing bias and is not automatically harmful.

Linear projections have the same sign as actual nonlinear changes in all 90
balanced off-target mean-panel comparisons, with mean absolute error .00374 and
maximum .01360 acceleration units. Anchor 51 gives the same direction counts.
Saved ready-near-target groups exist in 29 balanced command/side/update cells;
17 receive corrective actual changes. Five cells have a corrective own-command
pooled gradient but a harmful combined saved update. These group counts are not
row-weighted success rates. Seed 17 contributes only two ready cells, both at
command 4; its heading limitation remains a major coverage restriction.

### Concrete saved-state attribution

Balanced seed 29, update 86 has **256 ready overspeed rows requesting 4**, with
mean actual speed **5.458**. Its actual force changes from **−.29387 to −.08910**:
less deceleration while already above target.

| Contribution to local force change | Conditional Adam allocation |
|---|---:|
| Command 4 | **−.12136** |
| Command 10 | +.06377 |
| Command 16 | **+.25897** |
| Incoming momentum | −.00669 |
| Entropy | +.00044 |
| Stopping actor /direct critic | 0 /0 |
| Float32 rounding | −.000002 |
| Linear total | **+.19513** |
| Actual nonlinear change | **+.20477** |

Here the speed-4 signal strengthens braking, but shared changes from other
commands more than undo it. The same update has 188 ready underspeed command-16
rows, mean speed 14.064. Their force increases by **+.45517**, which is locally
corrective. The command-16 allocation at those rows is +.53313. Useful acceleration
learning at the high command spreads into lower-command overspeed states.

Conflict is not the only failure. At update 43, that same seed's 256 command-4
overspeed rows receive a **+.20358** force change while their own pooled gradient
also points toward more acceleration. At update 16, its own command-16 gradient
points toward less acceleration on 77 ready underspeed rows. Merely removing
other commands would not guarantee the right local signal in every case.

The Adam figures use the actual shared variance/clipping path and a before-policy
Jacobian. They are additive bookkeeping, not a causal ablation or a simulated
outcome after removing a command. Rounding and nonlinear remainder stay explicit.

### Advantage centering, critic clipping and momentum

Pooled movement centering produces strongly different sign mixtures by command:

| Training command | Selected balanced rows | Positive pooled advantages | Raw-to-pooled sign changes |
|---|---:|---:|---:|
| 4 | 3,840 | 2,272 (59.2%) | 1,999 |
| 10 | 3,840 | 3,063 (79.8%) | 1,648 |
| 16 | 3,840 | 1,077 (28.0%) | 476 |

Seven of fifteen command-4 groups are entirely positive after pooling and four
entirely nonpositive. This is evidence that relative group baselines matter;
it does not imply positive advantage always means more throttle. Action-score
correlation determines the parameter gradient.

Per-command centering is **not an established fix**. On the same 45 balanced
full-rollout pairs, raw advantages yield 21 negative cosines, existing pooled
normalization 17, and own-command centering at the same scale **22**. Corrective
own-command projections on the 90 off-target mean-panel cells number 47 raw,
42 pooled and 45 own-centered; on saved ready groups the counts are 18/17/17.
No variant supplies consistently correct control or removes interference. No
alternative variant was optimized or evaluated in flight.

The critic has **zero direct actor gradient**, but shares global clipping.
It reduces the gradient scale beyond actor/entropy-only clipping in **284/360**
balanced minibatches. Mean actual clip factor is **.596**, versus **.953** with
actor/entropy alone. The mean actual/actor-only ratios by seed are **.734/.804/.331**;
seed 43 reaches **.0768** in the most suppressed selected minibatch. The cyclic
mean ratio is .959. A separate clipping experiment would be needed to measure
its causal effect: Adam's accumulated second moments can compensate for some
scaling, and clipping does not itself reverse the instantaneous actor gradient.

Incoming momentum is material. Across the 90 balanced off-target mean-panel
cells, absolute projected allocations sum to 5.24 /5.10 /12.13 for commands
4/10/16, **5.44 for incoming momentum**, and **.064 for entropy**. These sums
compare local magnitudes; they are not shares of net progress. Entropy is small
in this sample. Stopping and direct critic allocations are exactly zero in
control space. Maximum first-moment decomposition error is 1.74e−8; maximum
per-step parameter-rounding residual is **1.20e−7**, below the frozen 2e−6 bound.

### Verification and retained state

All **30 model/optimizer/RNG/metric replays**, **7,776 native inference checks**,
**46,080 behavior-sampling checks**, **23,040 saved stopping-row checks** and
**60 frozen-parent checks** pass. Another 4,860 zero-command probe rows remain
exactly unchanged. All zero-command control Jacobians and stopping policy
gradients are zero. The verifier independently checks saved replay states,
component-gradient sums, actual-delta reconstruction and reported projections.

Fifteen distinct Python tests pass: fourteen in the original run and the one
corrected cosine test on rerun. Its original exact-equality expectation of −1
failed at −.9999999999999998; the correction uses approximate equality and
changes no audit implementation. Both logs are retained. The pre-freeze synthetic tests also execute 25 optimizer
steps on random fixtures, separate from the 720 saved-update replay steps.
Five model and 48
environment native tests are inherited with the unchanged binary. No new Rust
code or native test execution was needed. No failures occurred after freeze.

Preserve all 1,638 sources, 105 copied inputs, 30 diagnostic replay tensor files,
six run summaries, full results, analysis, commands and logs. Original policies,
optimizer/checkpoint inputs, stopping baselines and all **63 prior manifests**
remain unchanged. The full LF-067 parent manifest verifies before and after the
audit. The archive and copied inputs are local retention, not an off-machine
backup. No new training or physics, policy export, confirmation use or promotion.
LF-067's physical outcome remains 0/8/0 retained tracking cases in each arm;
no additional flight capability or navigation qualification is claimed.

## Next decision

A useful next diagnostic is **single-command speed-16 learning**, using the same
protected residual and original parents across all three seeds, with underspeed
and overspeed starts and strict acquisition/retention evaluation on held-out
starts. Freeze its scope and budget separately. First establish whether this
learner can acquire feedback for one requested speed when other positive-speed
commands do not contribute updates. It would be an offline diagnostic, not a
runtime controller switch or a new deployment architecture.

This narrows the task and reallocates command exposure; it is not a pure causal
ablation of parameter sharing. If isolated speed holding still fails, investigate
local credit/state coverage before assuming a larger network will solve it. If it
succeeds repeatably, shared command conditioning and interference become stronger
candidates for a matched follow-up. Separate actor/critic clipping is also a
concrete hypothesis, but changing it simultaneously would obscure interpretation.
No further training is run or authorized by this report itself.
