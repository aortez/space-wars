# Protected movement-actor comparison v1

LF-063, 2026-10-25. Frozen before preflight and main training. C4 learning
attempt 31; audit count remains 19. The user authorized the proposed matched
movement-actor comparison after LF-062's saved-policy feedback audit.

## Question and arms

Can a trainable movement actor overcome the inherited wrong-sign velocity response
more effectively than the existing additive residual, while preserving original
zero-command behavior? Use seeds **17/29/43** and original LF-053 parent weights
in both arms. Both use sustained training; this is not another horizon comparison.

- **residual:** exactly the LF-061 sustained recipe and format-2 model. Freeze the
  original actor and add `g * residual(x,error)` to its actuator logits.
- **movement:** format **3**, `motor18-protected-movement20x32x32-v3`. Blend a
  trainable movement actor with the frozen original actor using the same
  `g = requested_speed/(requested_speed+1)`. Steering remains the original learned
  steering. Only throttle/brake have a new trainable computation.

Let `B` be the original actuator logits, `M` the movement actor's logits and
`b=[2,-2]` the fixed initialization offset. Evaluate movement outputs as
`B + g * ((M-B)+b)`, mathematically `(1-g)*B + g*(M+b)`.
Zero requested speed returns the original logits exactly. At positive speeds,
training can change the copied movement features and actuator outputs; the original
actor's fixed contribution is reduced to `1-g`. No native actions, scripted brake
rule, success-triggered intervention or runtime controller handoff is introduced.

## Matched initialization and capacity

Both arms have **5,259 parameters, 3,493 trainable**. Each additional actor path is
20→32→32→2 with tanh hidden layers, consuming the original eighteen inputs plus
the existing body-frame desired-minus-current COM velocity errors. Original actor
weights and log deviations stay frozen; the independent critic remains trainable.
Actuator noise uses the unchanged gated residual deviation parameters.

The residual arm keeps its original random hidden layers and zero output weights
with biases [2,-2]. The movement arm copies original actor hidden layers and its
two actuator output rows, padding the first layer's two error-feature columns with
zeros. Its [2,-2] offset is fixed in the versioned graph, not an extra parameter.
These feature initializations intentionally differ: the intervention is the
trainable copied movement representation plus blending, not only a scalar weight.

To match floating-point computation, the movement first layer evaluates the copied
18-input projection and the two error columns separately. Python uses the original
three-row output matrix shape, with the frozen steering row discarded. This preserves
the copied actor function exactly at initialization: `M=B`, so both arms begin at
`B + g*[2,-2]`. Constructors consume the same RNG; no extra draw is introduced by
copying weights. Tests require exact actor/value/distribution/RNG matching, not
only approximate similarity. Native formats 1/2 remain supported unchanged.

Verify matched first rollouts in both preflight and main runs, all **312 paired
initial development traces**, and **2,430 paired initial feedback outputs**.
The residual controls must also reproduce LF-061 sustained checkpoints **0/1/16/64/128**,
rollouts **1/128**, and initial/final exports: **15 state, 6 rollout and 6 export
comparisons**, ignoring only checkpoint contract metadata identifying this experiment.

## Unchanged learning recipe and strict evaluation

Use the existing 48 training tasks: aligned movement commands **4/10/16**, four
initial-speed modes and two bearings, plus 24 zero-command stopping starts. Each
rollout contains **512 movement +512 stopping actions**. Movement continues through
tick 900; success requires acquisition by 360 and no subsequent breach. Early
acquisition has no terminal bonus. Stopping uses the original acquisition rule.
Keep LF-055's dense velocity reward, +1 terminal success and -5 physical-failure
penalty, gamma .99, lambda .95, separate group advantage normalization and fixed
.5 actor-loss coefficients. Stopping residual gradients remain zero by construction;
stopping still trains the separate critic. Continuous learned controls run at 60 Hz.

Require at least three completed visits per movement case and seven per stopping
case in main training. Recorded collisions/damage/ship-loss/boundary failures end
immediately, receive their original penalty and remain in statistics. They are
permitted training outcomes, never successful control.

Initial/final evaluation uses the unchanged **104 development cases**: 24 tracking
at unseen commands **6/12/18**, 24 aligned stopping, 24 original braking and 32
original turning/holding. Every case requires acquisition by 360 and uninterrupted
retention through 900: heading <=5 degrees, velocity-vector error <=1 unit/s,
absolute spin <=.2 rad/s and thirty complete consecutive qualified intervals.

Per-seed movement qualification requires >=20/24 tracking, >=6/8 at each command,
>=4/6 in each initial-speed mode, gain >=12 over initialization and no physical
failure. The complete stage additionally requires original aligned stopping,
braking and turn/hold gates with no lost original success. Overall success requires
two movement-arm seeds to pass. Paired improvement separately requires at least
two seeds gaining >=4 retained tracking cases over their residual controls.
No checkpoint selection, promotion, confirmation set or automatic extension.

Preserve every original zero-command trajectory, including seed 17's known failures:
**672 original reference comparisons** and **480 initial/final zero-command comparisons**.
Exact preservation does not repair an originally failing skill or qualify navigation.

## Velocity-response diagnostic

Reuse LF-062's frozen **54 shared anchors**. For both initial and final models,
vary command through **6/12/18** and along-command velocity error through
**-6/-.25/0/+.25/+6** while preserving other inputs and the COM/origin offset.
Record actuator logits/actions, local velocity slopes from ±.25 and instantaneous
control acceleration from the unchanged native force law. This gives **9,720
native/Python inference comparisons**, with no additional physics. Report wrong-sign
response and force balance alongside physical performance; this diagnostic cannot
replace the strict evaluation gate. No branch-gradient attribution is assumed to
carry unchanged from the format-2 audit to this new graph.

## Validation, preflight and budgets

Before freeze, **31 Python tests** and **5 native model tests** pass. Tests cover
matched tasks, exact initialization/RNG, zero-speed invariance after real updates,
trainable error-feature gradients, native initial equality, perturbed movement
weights, legacy runtime compatibility and cross-arm resume rejection. Physics
sources are unchanged; LF-060's earlier 48 environment tests remain retained evidence.

Six four-update preflights use cases 0/1/40/33, each followed by an exact final-update
repeat from checkpoint 3. Apply `record-physical-failures-v1`: require all four cases
completed and valid outcomes/rewards, action coverage >=512/2,048 above 5% throttle
and independently >=512 below 40% brake, exact resume and protected actor checks.
Both arms require >=1,080 observed continuation ticks beyond tick 360, allowing
explicitly recorded forgone continuation after physical termination as in LF-061.
Forgone ticks never count as performed physics or success. Stop on implementation/
accounting failure. Main runs restart from original parents, not preflight weights.

Six main runs of **128 updates**, followed by exact repeats of update 128 from
checkpoint 127. Same Adam learning rate .0003, four epochs, minibatches 256,
clip .2, entropy .001, value coefficient .5, combined gradient clip .5 and KL .03.

**26 bounded processes**, at most three concurrent. Preflight/resume caps are
**180 internal /200 external seconds**, main/resume **900/920**, verification
**300/320**. Totals/caps including repeated updates: **823,296 training ticks**,
**12,864 optimizer steps**, **24,000 restoration ticks**, **1,248 physical evaluations
/1,123,200 scored physics ticks**, **10,968 native/Python comparisons** (1,248
evaluation +9,720 feedback), **811,008 likelihood checks**, **405,504 zero-command
actor/distribution comparisons**, **816 frozen-parent checkpoint checks**. Main
movement/stopping action exposure is **393,216 each**; neutral setups counted separately.

Retain **816 checkpoints, 804 rollouts, 48 exports**, every physical trace, diagnostic,
failure, source, config, runtime, test and command. Use existing lossless gzip
evidence storage and verify original-byte hashes. Freeze hashes before preflight;
preserve all **58 earlier manifests**. Require >=2 GiB free on the artifact volume
before freeze, and retain the 512 MiB per-run reserve.

Archive `data/training-experiments/learned-flight-motor-movement-v1-20261025`, linked
to `/home/data2/space-wars3-artifacts/training-experiments/learned-flight-motor-movement-v1-20261025`.
Code remains in the standalone checkout. Local retention is not off-machine backup.


## Results

All six preflights pass, and all six main runs finish **128 updates /2,048
optimizer steps** without KL early stops. No process fails, retries or extends
its budget. The original residual arm reproduces LF-061's sustained control
exactly at all declared checkpoints, rollouts and exports.

**The movement actor does not improve tracking. Neither arm qualifies.**

| Arm | Seed | Initial tracking /24 | Final tracking /24 | Original zero-command retained /80 | Final collisions |
|---|---:|---:|---:|---:|---:|
| Residual | 17 | 0 | 0 | 0 | 0 |
| Residual | 29 | 8 | 0 | 80 | 0 |
| Residual | 43 | 0 | 8 | 80 | 3 |
| Movement | 17 | 0 | 0 | 0 | 0 |
| Movement | 29 | 8 | 0 | 80 | 0 |
| Movement | 43 | 0 | 0 | 80 | 6 |

The eight residual-arm tracking successes all request speed 6. No arm passes
any speed-12 or speed-18 case. Every original zero-command trajectory remains
exact, including seed 17's original failures; preservation is not new competence.
Paired tracking differences are **0/0/-8** for seeds 17/29/43. Both the strict
stage gate and the separately declared paired-improvement gate fail.

### Physical behavior and controlled feedback

The following values are forward speed averaged over the final 120 ticks of
the fixed rest-start development cases 0/16/32. They describe each trajectory,
not equilibrium estimates or aggregate success rates.

| Arm /seed | Request 6 | Request 12 | Request 18 |
|---|---:|---:|---:|
| Residual /17 | 1.192 | 1.320 | 1.422 |
| Movement /17 | 1.157 | 1.256 | 1.315 |
| Residual /29 | 2.429 | 2.810 | 3.033 |
| Movement /29 | 2.956 | 3.459 | 3.706 |
| Residual /43 | 6.810 | 9.990 | 23.158 |
| Movement /43 | 7.085 | 13.202 | 26.378 |

Seeds 17/29 remain too slow. Movement seed 43 moves more strongly but fails
to regulate speed: its request-6 rest trial reaches **7.086** at tick 900,
outside the unchanged one-unit velocity tolerance. Its request-12 trial crosses
the target and reaches **13.661**; request 18 reaches **28.092**, versus **24.644**
for the residual control. Passing briefly through a target is not retained control.

The matched-state diagnostic confirms the feedback problem persists. For final
seed 43 in **both arms**, all 54 anchors at each request 6/12/18 show increasing
throttle and decreasing brake as actual speed increases. The new actor has freedom
to replace most of the original positive-speed response, but the learned total
response still points the wrong way. This is a controlled input perturbation;
unlike a changing physical trajectory, all other observed inputs are held fixed.

At LF-062 anchor 51, request 18, the force-law diagnostic gives:

| Arm | Actual speed | Throttle | Brake | Along-command control acceleration |
|---|---:|---:|---:|---:|
| Residual | 12 | 13.908% | 12.808% | +1.135 units/s² |
| Residual | 18 | 14.102% | 12.479% | +1.354 units/s² |
| Residual | 24 | 14.191% | 12.202% | +1.505 units/s² |
| Movement | 12 | 13.339% | 11.391% | +1.446 units/s² |
| Movement | 18 | 13.409% | 11.058% | +1.611 units/s² |
| Movement | 24 | 13.446% | 10.787% | +1.736 units/s² |

These are instantaneous algebraic acceleration estimates using the existing
native control force, not additional physical rollouts or complete future returns.
The movement arm has slightly less throttle here, but the brake reduction more
than offsets that: overspeed acceleration is larger. Across all anchors, final
movement seeds 17/29 still command substantial deceleration at the target speeds.
Their failure is not the same overshoot pattern as seed 43.

`feedback.json` retains every prescribed input, logit, action, acceleration and
native comparison. Post-run `feedback-summary.json` reports all seeds, phases and
commands, including slope signs rather than selecting the seed-43 example alone.
`analysis.json` and `trajectory-details.json` retain the physical breakdown.

### Training exposure and verification

Each main run executes **65,536 movement and 65,536 stopping actions** and meets
the minimum completed-case coverage. Completed movement outcomes are:

| Arm /seed | Success | Deadline | Collision |
|---|---:|---:|---:|
| Residual /17 | 0 | 72 | 0 |
| Residual /29 | 6 | 66 | 0 |
| Residual /43 | 2 | 68 | 4 |
| Movement /17 | 0 | 72 | 0 |
| Movement /29 | 6 | 66 | 0 |
| Movement /43 | 3 | 65 | 6 |

Incomplete active tails are excluded from episode-outcome counts and included
in action exposure. Seed 43's movement arm acquires the band in 27 training
episodes and breaches it in 24; the residual arm acquires in 25 and breaches
in 23. The extra movement actor does not turn acquisition into reliable retention.

All **26 processes** finish within their declared caps. Main runs take
49.28–73.77 seconds; final verification takes 170.70 seconds internally. Actual
work including preflights and repeated updates is **823,296 training ticks,
12,864 optimizer steps, 12,618 restoration ticks, 1,120,694 scored evaluation
ticks over 1,248 episodes, and 6,384 separately counted neutral setup ticks**.
No confirmation set is used and no model is promoted.

Verification passes:

- **31 Python tests and 5 native model tests**; the retained 48 native environment
  tests are inherited evidence because physics is unchanged.
- **12 exact resumes**, six paired first rollouts, 312 paired initial physical
  traces and 2,430 paired initial feedback outputs.
- **15 control checkpoint states, six control rollouts and six control exports**
  reproduce LF-061, excluding only experiment-identifying contract metadata.
- **10,968 native/Python inference comparisons**, 811,008 saved likelihood checks
  (maximum observed main error 9.54e-7), 405,504 protected zero-command actor/
  distribution rows and 816 frozen-parent checkpoint checks.
- **672 complete original-reference traces** and 480 initial/final zero-command
  traces compare exactly. Original seed-17 failures remain visible.

Retain **816 checkpoints, 804 rollouts, 48 exports, 1,623 frozen sources and 204
copied inputs**, all traces/diagnostics, runtime, source archive, tests, commands
and logs. Verify all **58 previous manifests** and original input hashes unchanged.
The frozen source/specification and all results are retained separately from this
completed report. See results (`docs/data/flight-motor-movement-v1-results.json`, in git history) and
[archive receipt](data/flight-motor-movement-v1-archive.json).

## Interpretation and next decision

This comparison rejects the narrow expectation that the copied movement actor
and reduced frozen contribution, with this same initialization function and
training recipe, would yield better retained tracking. It does not rule out all
architectures or prove a sole reward, critic or exploration defect. The movement
arm changes feature initialization and parameterization together; those effects
are not separately identified. Exact original-control reproduction and original
stopping preservation make this a failed learning intervention, not a regression
in the baseline or native model loader.

The next proposed diagnostic is a bounded **matched-state action/return probe**:
restore identical aligned underspeed, target and overspeed states from both saved
arms, then compare small throttle/brake perturbations with the current action
under a fixed continuation. Measure physical error and discounted rewards across
declared horizons, alongside current critic predictions and the training-credit
calculation. Freeze the state panel, action durations, continuation and budgets
before running. Include all seeds and retain failure/termination outcomes.

Earlier credit audits associated sampled actions with outcomes; they did not
branch alternative actions from identical physical states. This probe would test
whether corrective actions actually receive better local returns, and whether
the learned credit estimate agrees. It would not prove long-term optimality or
serve as a scripted controller, teacher labels or a new training objective.
No such probe or additional training has run here. Stopping protection, strict
physical gates and all prior archives remain in force; navigation is unqualified.
