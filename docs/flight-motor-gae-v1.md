# Protected motor GAE-lambda comparison v1

LF-065, 2026-10-25. C4 learning attempt 32; audit count remains 20.
Frozen before preflight/main execution. The user authorized lambda .95 versus 1
while preserving the stopping baseline, following LF-064's action/return probe.

## Hypothesis and single training change

LF-064 found that intermediate critic terms reverse some beneficial underspeed
return rankings. Its seed-43 overspeed corrections already received favorable
TD/GAE credit, so this is not a proven explanation of the braking failure.
Test whether reducing intermediate-critic dependence improves learned control.

- **lambda095:** the existing protected residual sustained recipe, lambda .95.
- **lambda1:** exactly the same recipe with lambda 1.0.

Only the arm identifier and **GAE lambda** differ between the two configurations.
Both start from original LF-053 parent weights at seeds **17/29/43**, with fresh
optimizer, collector and RNG as before; LF-063 final actors are not warm starts.
Gamma remains **.99**. Lambda applies to both movement and stopping trajectories,
changing actor advantages and critic targets. Stopping actor outputs are still
protected by the existing network graph; its separate critic remains trainable.

Lambda 1 removes intermediate value predictions from the rollout return target:
the target becomes discounted observed rewards plus the final live-state bootstrap.
The learned V(start) baseline, endpoint bootstrap, terminal cuts and group advantage
normalization remain. This can increase estimator variance. A 256-tick training
rollout is different from LF-064's 120-tick diagnostic, and that audit examined
final policies at unseen command speeds. Success is a measured outcome here,
not assumed from finite-return sign agreement in the previous probe.

## Protected model and unchanged execution

Use format 2, `motor18-protected-actuator20x32x32-v2`, with **5,259 parameters /
3,493 trainable** in both arms. The frozen original actor owns steering; throttle
and brake add a learned 20→32→32→2 residual seeing eighteen original observations
and two derived body-frame velocity errors. The residual and its noise adjustment
are multiplied by requested_speed/(requested_speed+1), exactly zero at speed zero.
Original actor weights and log deviations remain frozen. No native controller,
scripted correction, fallback or runtime handoff is introduced.

Keep 60 Hz controls/physics, deterministic learned steering and the existing
independent Gaussian actuator sampling, with no persistent noise or multiplier.
Keep the original initial residual biases [2,-2], zero output weights, hidden
initialization, actor/critic separation and continuous tanh/sigmoid transforms.
Both arms must have identical initial weights, optimizer, physical collector and
RNG. Their first behavior rollout must match exactly: observations, latents,
conditional likelihoods and collected physical states. Advantages and returns
must differ, because changing them is the intervention under test.

The shared runner and feedback helper gain an explicit experiment-stage argument;
their previous defaults remain unchanged. The collector recognizes the new
experiment identity and uses the same velocity reward. Native code and runtime
are unchanged. No reward, task, architecture or control change accompanies lambda.

## Fixed tasks, optimization and physical gates

Use the same **48 training tasks**: commands 4/10/16, rest/slow/fast/reverse starts
and two bearings, interleaved with the original 24 stopping starts. Each update
collects **512 movement +512 stopping actions** over four persistent slots.
Positive-speed trials continue through tick 900, requiring acquisition by 360
and uninterrupted retention for terminal success. Stopping retains its original
acquisition rule. No bonus is awarded for early movement acquisition.

Keep the dense heading/velocity/spin reward, +1 terminal success and -5 physical
failure penalty. Recorded physical failures terminate immediately, remain in the
statistics and are admitted under `record-physical-failures-v1`. They never count
as successful control. Main training must complete at least three visits to every
movement case and seven to every stopping case. Forgone continuation after a
physical failure is accounted separately and never counted as performed physics.

Both arms run **128 updates**, Adam .0003, four epochs, minibatches 256, clip .2,
entropy .001, value coefficient .5, combined gradient clip .5 and KL threshold .03.
Movement/stopping advantages retain separate normalization with fixed .5 actor
group coefficients. Report KL stops and actual optimizer steps; no automatic
budget extension or intermediate-checkpoint selection.

Initial/final evaluation uses the unchanged **104 development cases**: 24 tracking
at unseen commands 6/12/18, 24 aligned stopping, 24 original braking and 32 original
turning/holding. Require acquisition by 360 and uninterrupted retention to 900:
heading <=5 degrees, velocity-vector error <=1 unit/s, spin <=.2 rad/s and thirty
complete qualifying consecutive intervals. Physical failure takes priority.

Movement qualification requires >=20/24 tracking, >=6/8 at every command,
>=4/6 in each initial-speed mode, gain >=12 over initialization and no physical
failure. Complete stage qualification also requires the original stopping,
braking and turn/hold gates with no lost parent success. Overall success requires
two lambda-1 seeds to pass. Paired improvement separately requires two seeds
gaining >=4 retained cases over their lambda-.95 controls. Criteria remain strict;
no confirmation use, model promotion or navigation claim follows automatically.

## Reproduction and target verification

Require the lambda-.95 control to reproduce LF-063 residual checkpoints
**0/1/16/64/128**, rollouts **1/128** and initial/final exports for all three seeds:
**15 checkpoint states, six rollouts, six exports**, excluding only experiment
contract metadata. Require **312 paired initial development traces**, **672
original-reference traces** and **480 initial/final zero-command traces** to
compare exactly. Preserve seed 17's original failures as well as passing baselines.

Reconstruct rewards, terminal flags and observations from native training traces
for each completed run. At all four preflight updates and main updates
**1/16/64/128**, recompute original values in the collector's four-row batch shape
and require **exact saved advantages/returns**: **49,152 rows** across both phases.
For the **24,576 lambda-1 rows**, independently check the discounted-return
recurrence, cutting at every terminal/reset and using only live endpoint
bootstraps (1e-4 absolute /1e-5 relative tolerance for rounding). Retain target
statistics and first-rollout differences for both command groups.

Repeat LF-063's diagnostic on the frozen 54 shared anchors, commands 6/12/18 and
actual-minus-requested speed offsets -6/-.25/0/+.25/+6. Retain actuator responses,
local velocity slopes and algebraic control acceleration. Initial paired outputs
must match **2,430** times, and **9,720 native/Python feedback comparisons** must
pass. These are inference-only diagnostics, not new physical evaluations or a
replacement for retained control.

## Tests, preflight, budgets and retention

**20 Python tests pass**: nine new tests for lambda isolation, original-control
configuration, exact initialization/RNG, independent lambda-1 return algebra,
terminal/reset handling, the real collector on synthetic items, real PPO updates
with stopping protection, and incompatible resumes; plus seven existing movement
and four failure-policy regression tests. No new native physics is required by
these tests. The unchanged runtime's five native model tests and 48 environment
tests remain inherited evidence.

Six four-update preflights use the existing cases 0/1/40/33; each repeats its last
update exactly from checkpoint 3. Require completed-case coverage, at least 512
movement actions above 5% thrust and at least 512 below 40% brake, and >=1,080
continuation ticks beyond 360 including explicitly accounted physical-failure
forgone time. Require exact resumes, valid outcomes/rewards, stopping protection
and target checks before main training. Main runs restart from original parents.
Repeat each main final update exactly from checkpoint 127.

**26 bounded processes**, at most three concurrent. Preflight/resume caps:
**180 internal /200 external seconds**; main/resume **900/920**; verification
**600/620** to cover added target reconstruction. Maximum totals including repeated
updates: **823,296 training ticks, 12,864 optimizer steps, 24,000 restoration ticks,
1,248 physical evaluations /1,123,200 scored ticks, 10,968 native/Python comparisons,
811,008 likelihood checks, 405,504 protected stopping rows and 816 frozen-parent
checkpoint checks**. Main movement/stopping exposure is 393,216 actions each.
Neutral setup ticks are counted separately. Stop on implementation/accounting
failure; preserve partial traces and logs. No automatic retry.

Retain **816 checkpoints, 804 rollouts, 48 exports**, every trace, target check,
feedback output, failure, source, config, runtime, command and log. Freeze all
hashes before preflight. Preserve all **60 earlier manifests**. Require >=2 GiB
free on the artifact disk before freeze and retain the 512 MiB per-run reserve.

Archive `data/training-experiments/learned-flight-motor-gae-v1-20261025`, linked to
`/home/data2/space-wars3-artifacts/training-experiments/learned-flight-motor-gae-v1-20261025`.
Use the existing standalone checkout. Local retention is not off-machine backup.


## Results

All six preflights, six main runs, twelve resume checks and two verifiers finish
within their frozen budgets, without retry or source correction. Every main run
completes **128 updates /2,048 optimizer steps**, with no KL early stop. The
lambda-.95 control exactly reproduces LF-063's residual control at the declared
checkpoints, rollouts and exports.

**Lambda 1 improves seed 43's high-speed feedback and preserves seed 29's initial
low-speed successes, but neither arm qualifies for speed tracking.**

| Arm | Seed | Initial tracking /24 | Final tracking /24 | Original zero-command retained /80 | Final collisions |
|---|---|---|---|---|---|
| λ=.95 | 17 | 0 | 0 | 0 | 0 |
| λ=.95 | 29 | 8 | 0 | 80 | 0 |
| λ=.95 | 43 | 0 | 8 | 80 | 3 |
| λ=1 | 17 | 0 | 0 | 0 | 2 |
| λ=1 | 29 | 8 | 8 | 80 | 0 |
| λ=1 | 43 | 0 | 8 | 80 | 2 |

All tracking successes request speed **6**; none request 12 or 18. Seed 29's eight
lambda-1 successes were already present at initialization, so this is preservation
relative to the control's regression, not eight newly learned cases. Seed 43 gains
eight in both arms. Paired differences are **0/+8/0**: the strict stage gate and
the separate requirement for improvement in at least two seeds both fail.

Every original zero-command trajectory remains exact, including seed 17's
original failures. Seeds 29/43 preserve all **80/80** original stopping, braking
and heading cases in both arms. All final collisions occur in tracking; stopping
preservation does not make the moving policies safe or qualified.

### Corrective feedback changes, without sufficient retained control

At final seed 43, all **54** shared command-18 anchors in the lambda-.95 control
still increase throttle and decrease brake as actual speed rises. With lambda 1,
all **54** instead decrease throttle and increase brake. These signs use the
predeclared central difference at actual speed ±.25 around the command, holding
other inputs fixed consistently. This reverses the earlier high-speed feedback
failure on the fixed panel; it is more than a change in average propulsion.

At the same LF-062 anchor 51 and request 18:

| Arm | Actual speed | Throttle | Brake | Along-command control acceleration |
|---|---|---|---|---|
| λ=.95 | 12 | 13.908% | 12.808% | +1.135 units/s² |
| λ=.95 | 18 | 14.102% | 12.479% | +1.354 units/s² |
| λ=.95 | 24 | 14.191% | 12.202% | +1.505 units/s² |
| λ=1 | 12 | 15.745% | 16.146% | +0.627 units/s² |
| λ=1 | 18 | 14.554% | 16.207% | +0.066 units/s² |
| λ=1 | 24 | 13.296% | 16.273% | -0.526 units/s² |

The lambda-1 response accelerates from speed 12 and decelerates from speed 24;
the control accelerates at both. These are instantaneous algebraic control-force
estimates using the unchanged native force law, not new physical rollouts or
complete future returns. Correct local signs do not establish the correct balance
at every command, adequate response speed, stability over all states or safety.

Across all 54 anchors, lambda-1 seed 29 has correct throttle/brake slope signs at
each request 6/12/18, yet its mean target-speed acceleration is about **−2.04**
at request 12 and **−.87 units/s²** at request 18. Seed 43's mean target-speed
acceleration is about **−.99** at request 12, versus **−.01** at request 18.
Seed 43 still has wrong brake slope signs at six low-speed and four command-12
anchors. Seed 17 retains its original steering problem; actuator feedback alone
does not repair it. The full panel, including both phases and every seed, is
retained in `feedback.json` and `feedback-summary.json`.

### What the physical trajectories still fail to do

Forward speed at **tick 900** in the fixed rest-start development cases 0/16/32:

| Arm /seed | Request 6 | Request 12 | Request 18 |
|---|---|---|---|
| λ=.95 /17 | 1.192 | 1.320 | 1.422 |
| λ=.95 /29 | 2.429 | 2.810 | 3.033 |
| λ=.95 /43 | 6.810 | 9.992 | 24.644 |
| λ=1 /17 | 7.319 | 10.176 | 17.017 |
| λ=1 /29 | 5.825 | 8.187 | 10.060 |
| λ=1 /43 | 6.232 | 9.049 | 16.216 |

These are endpoint measurements, not assumed equilibrium speeds. For lambda-1
seed 43 at request 18, speed progresses **11.99 →14.29 →16.22** at ticks
360/600/900. It is still accelerating but never acquires the full qualifying band
in any of the eight command-18 cases. Its rest case ends with velocity error
**1.82** and heading error **1.43 degrees**, after missing the tick-360 deadline.
The control's rest case reaches **24.64**, so overshoot is substantially reduced
without meeting the acquisition/retention requirement.

At request 12, lambda-1 seeds 29/43 settle near **8.19/9.05**, below the command.
Their rest trajectories change by less than .004 between ticks 600 and 900.
The two fast-start seed-29 cases acquire at tick **141**, then exit at **162**;
the two fast-start seed-43 cases acquire at **277**, then exit at **376**. The
other six cases in each seed never acquire. At request 18, seed 29's two
fast-start cases acquire at **341**, then exit at **383**; its other six do not
acquire. Passing through the band while braking is not sustained speed control.
No higher-speed failure is rescued by merely relabeling late acquisition.

Seed 17's lambda-1 rest cases end with heading errors of roughly **20/21/24
degrees** at requests 6/12/18. That frozen baseline limitation remains distinct
from the other seeds' speed balance and response problems. `analysis.json` and
`trajectory-details.json` retain the full case breakdown and sampled trajectories.

### Training exposure and target changes

Each main run performs **65,536 movement and 65,536 stopping actions** and meets
the declared completed-case coverage. Completed movement outcomes are:

| Arm /seed | Success | Deadline | Collision |
|---|---|---|---|
| λ=.95 /17 | 0 | 72 | 0 |
| λ=.95 /29 | 6 | 66 | 0 |
| λ=.95 /43 | 2 | 68 | 4 |
| λ=1 /17 | 0 | 69 | 3 |
| λ=1 /29 | 9 | 58 | 6 |
| λ=1 /43 | 8 | 60 | 6 |

Incomplete active tails are excluded from outcome counts and included in action
exposure. Original stopping outcomes are identical across arms: 182 deadlines
for seed 17, 997 successes for seed 29 and 982 successes for seed 43. Training
collisions remain recorded failures and never count toward strict qualification.

The post-run descriptive coverage read uses all 786,432 main saved rows, with no
new inference, physics or optimization. A row is "ready" when heading error is
at most 5 degrees, spin at most .2 and lateral speed at most 1. Signed forward
speed error below −1 is underspeed, above +1 overspeed, and between is near-speed.
This near-speed category is **not** the full vector-error or retention gate.

| Arm /seed | Ready underspeed rows | Ready near-speed rows | Ready overspeed rows |
|---|---|---|---|
| λ=.95 /17 | 771 | 0 | 36 |
| λ=.95 /29 | 52,911 | 10,115 | 2,412 |
| λ=.95 /43 | 53,235 | 6,535 | 3,601 |
| λ=1 /17 | 942 | 0 | 36 |
| λ=1 /29 | 30,620 | 14,577 | 16,708 |
| λ=1 /43 | 30,363 | 21,679 | 10,234 |

Lambda 1 produces much broader near-speed and overspeed exposure in seeds 29/43.
For seed 43 at training command 16 alone, ready overspeed rows rise from **1,599
to 3,600**, and near-speed rows from **415 to 1,034**. For seed 29 at command 16,
the corresponding counts rise from **308 to 4,464** and **102 to 1,293**. Useful
states are no longer simply absent; the remaining issue includes what the updates
learn from them. These descriptive counts do not independently identify causality.

The paired first behavior rollout is identical before learning; only advantages
and return targets differ. Main first-rollout unnormalized advantage standard
deviations are:

| Seed | Movement std: λ=.95 → λ=1 | Stopping std: λ=.95 → λ=1 |
|---|---|---|
| 17 | 0.0506 → 0.1285 | 0.0052 → 0.0180 |
| 29 | 0.0148 → 0.0352 | 0.3077 → 0.3596 |
| 43 | 0.0283 → 0.0818 | 0.2993 → 0.3699 |

Both command groups change because lambda applies to both. Intermediate value
terms cancel from the lambda-1 return target, while its starting value baseline,
live endpoint bootstrap and separate group normalization remain. All six initial
behavior comparisons across preflight/main verify exactly. Later training states
diverge as a consequence of different policy updates, as expected.

### Work, validation and retention

Actual totals including preflights and repeated final updates: **823,296 training
ticks, 12,864 optimizer steps, 13,397 restoration ticks, 1,121,184 scored evaluation
ticks across 1,248 episodes, and 6,385 separately counted neutral setup ticks**.
The six main processes take **50.64–83.65 seconds externally**; the final verifier
takes **194.84 seconds internally /196.01 externally**. All 26 processes exit zero
within their caps. There is no automatic training extension or intermediate
checkpoint selection.

Verification passes:

- **20 Python tests**; five native model and 48 environment tests remain inherited
  evidence for the unchanged native source/runtime, not newly rerun tests.
- **12 exact resumes**, six matched first behavior rollouts, 312 paired initial
  development traces and 2,430 paired initial feedback outputs.
- **15 control checkpoint states, six rollouts and six exports** reproduce
  LF-063's residual control, excluding only experiment contract metadata.
- **49,152 GAE rows** reconstruct exactly from native rewards, terminal flags,
  observations and the corresponding value model. The **24,576 lambda-1 rows**
  also match an independent discounted-return recurrence; the largest observed
  rounding difference is **4.77e−6**, below the frozen tolerance.
- **10,968 native/Python comparisons**, 811,008 likelihood checks (maximum main
  error **9.54e−7**), 405,504 protected stopping actor/distribution rows and 816
  frozen-parent checkpoint checks.
- **672 complete original-reference traces** and **480 initial/final zero-command
  traces** compare exactly, including original seed-17 failures.

Retain **816 checkpoints, 804 rollouts, 48 exports, 1,629 frozen sources and 204
copied inputs**, all physical traces, target checks, feedback, analyses, failures,
configs, runtime, commands and logs. The source and frozen specification remain
unchanged; all **60 prior manifests** and copied input origins are verified.
The completed report, canonical plan, contract, journal and README are retained
with the run. See [results](data/flight-motor-gae-v1-results.json) and
[archive receipt](data/flight-motor-gae-v1-archive.json).

## Interpretation and next decision

Changing only lambda changes what the protected residual learns. In this panel,
lambda 1 can reverse seed 43's high-speed feedback without altering architecture,
reward, tasks or original stopping. It also avoids seed 29's low-speed regression.
This supports investigating estimator effects further, but the declared paired
improvement and strict competence gates both fail. The difference does not isolate
actor advantages from critic-target changes, nor prove that critic bias is the
sole cause. Stopping actor behavior is protected even though its critic changes.

Freeze a bounded read-only audit of the saved lambda-1 checkpoints and updates,
with the lambda-.95 controls retained. Measure where along-command acceleration
crosses zero and how strongly it changes with speed, at fixed inputs and across
the declared checkpoint history. Relate those responses to saved near-target
throttle/brake advantages and actual policy changes at training commands 4/10/16;
keep the unseen 6/12/18 panel descriptive. Include all three seeds and separate
the frozen steering limitation from actuator learning.

This addresses a narrower question than LF-062: corrective high-speed feedback
now exists in seed 43, but its balance point and response strength are not reliably
right for each command. The command-12 trials settle below the requested speed, while
the command-18 rest trial is still accelerating at the deadline. The proposed
audit should distinguish a stalled command-specific balance from an improving
but weak response before choosing a new learning change or bounded continuation.
It has not run. Preserve strict acquisition/retention criteria, original stopping
and every seed/failure. No automatic training extension, checkpoint selection,
confirmation use, native teacher, fallback, handoff or promotion. Navigation
remains unqualified.
