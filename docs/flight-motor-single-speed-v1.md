# Protected single-command speed-16 learning v1

LF-069, 2026-10-25. C4 learning attempt 34; audit count remains 22.
Frozen before preflight and scored training. The user authorized the speed-16
diagnostic proposed after LF-068's exact saved-gradient audit.

## Question and scope

Can the existing learner acquire speed-16 control when other positive-speed
commands do not contribute updates? LF-068 found cross-command interference,
incorrect own-command signals, broad force shifts and strong critic clipping
in some runs. This diagnostic narrows the learning task before another network
or objective change. It is not a pure parameter-sharing ablation: command-16
exposure increases and the collection distribution changes.

Run one arm, **speed16**, at seeds **17/29/43**, from the original LF-053
parents with fresh optimizer, collector and RNG. No previous final model is
used as a warm start. Keep the protected format-2 residual, 5,259 parameters /
3,493 trainable, frozen parent actor/steering/log deviations and exactly zero
residual gate for stopping. No native guidance, rescue or controller handoff.

## Training tasks and fixed recipe

Keep six persistent slots, alternating three movement /three stopping, with
256 ticks per slot and **86 updates**. Each main run has **132,096 training
ticks**, including **66,048 at command 16** and 66,048 stopping. Command-16
exposure is three times LF-067 balanced's 22,016 rows; the total training and
optimizer budgets per run are unchanged. Max 2,064 optimizer steps per main run.

Use the original eight command-16 cases, unmodified: rest/slow/fast/reverse
initial forward speeds 0/8/24/−16, two headings each. Preserve original indices
32/34/36/38/40/42/44/46. Partition the ordered movement list by strides of three,
giving pools of 3/3/2 cases in slots 0/2/4. Preserve LF-067's three eight-case
stopping pools, including their order, in slots 1/3/5. Every training candidate
appears once in exactly one pool; no duplicated movement cases. Require at
least eight completed visits to every movement case and seven to every stopping
case in each main run. Report unequal per-case exposure from pool lengths and
failure-dependent progression.

Keep lambda 1, gamma .99, the dense heading/velocity/spin reward and original
terminal terms, Adam .0003 /epsilon 1e−5, four epochs, minibatches 256, PPO clip
.2, entropy .001, value coefficient .5, **joint** gradient clip .5, KL .03,
separate movement/stopping advantage normalization and fixed .5 coefficients.
Controls and physics remain 60 Hz. The collector recognizes the new experiment
kind as using the existing velocity reward; no reward formula changes.

Movement acquisition must occur by tick 360 and retention continues through
900. Stopping acquisition training is unchanged. Keep
`record-physical-failures-v1`: physical failures terminate and are recorded,
retain their terminal penalty and never count as success. No run extension,
replacement seed, early checkpoint selection or adaptive hyperparameter change.

## Evaluation and pass criteria

Evaluate the **initial and final** policy on 104 development cases per seed.
The movement panel has **24 command-16 cases**, six each at rest/slow/fast/reverse.
Use the existing 24 held-out aligned headings/world starts, rescaling their
forward velocity to 0/8/24/−16 and setting requested speed to 16. Preserve each
start's random seed, heading, altitude and all other physics conditions. The
three former speed groups become `start_variant` 0/1/2, with six distinct starts
per mode. Their seeds are disjoint from training. Retain the existing 80
zero-command cases byte-for-byte: 24 aligned stopping, 24 legacy braking and
32 original turning/holding cases.

Physical qualification stays heading error <=5 degrees, velocity-vector error
<=1, absolute spin <=.2, thirty complete consecutive qualifying intervals,
acquisition by 360 and uninterrupted retention through 900. Physical failure
has priority. Speed is now a **trained command with held-out starts**, not an
unseen-speed generalization test.

The single-speed learning gate requires >=20/24 retained tracking cases,
>=4/6 in **every initial-speed mode**, gain >=12 over the initial policy, and
no physical failure in the development panel. The joint gate also requires the
unchanged stopping/braking/heading gates with no lost original successes.
Overall success requires **two of three seeds** to pass the joint gate. Preserve
seed 17's original deficiencies rather than weakening its requirements. Even a
pass would not qualify multi-speed tracking, navigation or gameplay integration.
Confirmation stays unused; no automatic promotion.

## Preflight and verification

Three four-update preflights use singleton pools at original indices
**32/1/40/17/44/33**: movement rest/fast/reverse at speed 16, alternating with
three stopping cases. Require each case to complete, >=512 movement samples
above .05 throttle, >=512 below .4 brake, and >=1,620 observed or explicitly
forgone movement ticks beyond acquisition tick 360. Training physical failures
remain recorded rather than invalidating the preflight automatically.

Exactly repeat the final preflight update from checkpoint 3 before main training.
Main runs restart from original parents, not preflight weights. Repeat each
main final update from checkpoint 85. Require six exact resumptions, including
rollout, model, optimizer, collector, RNG and metrics. Inspect every completed
training trajectory and every evaluation trajectory, preserving failures.

Verify every rollout has 768 command-16 and 768 zero-command rows, every saved
behavior likelihood, all frozen-parent actor weights and all stopping
logits/distributions. Independently reconstruct GAE and lambda-1 discounted
targets at every preflight update and main updates **1/16/43/86**. Check complete
legacy evaluation traces against the original parent and every final
zero-command trace against its initial trace.

Retain the inherited 54-anchor force panel at commands **0/16**, offsets
**−6/−.25/0/+.25/+6**, initially/finally for all seeds. Preserve fractional
metadata. Record throttle/brake response, target force and off-target force;
require native/Python inference parity and exact zero-command before/after
outputs. This algebraic diagnostic adds no physics and does not replace the
strict physical gate. Do not evaluate intermediate policies or select among them.

Expected checks: **414,720 likelihoods**, **207,360 saved stopping rows**,
**36,864 GAE /independent lambda-1 targets**, **3,864 native inference comparisons**
(624 evaluation starts +3,240 feedback rows), **336 original full traces**,
**240 initial/final zero-command traces**, and **810 zero-command feedback rows**.
Retain and verify 282 checkpoints, 276 rollouts and 24 policy exports, including
exact-repeat outputs. There are six declared parent initializations.

## Budget and retention

Fourteen child processes: three preflights, three preflight resumptions, one
preflight verifier, three main runs, three main resumptions and one final verifier.
At most three run concurrently. Preflight caps are 180 seconds internally /200
externally; main/resume caps 900/920; verifiers 600/620. Run orchestration phases
separately so training and verification completion are independently recorded.
Stop dependent work on a failed assertion and retain partial evidence.

Total training including preflights and exact repeats is **423,936 ticks** and
at most **6,624 optimizer steps**. Main training alone is 396,288 ticks. Allow
at most 24,000 restoration physics ticks. Evaluation is 624 episodes and at
most 561,600 ticks. Count neutral setup ticks separately. Require at least 2 GiB
free before freeze and maintain the existing 512 MiB artifact-disk reserve.

Twenty-two Python tests pass before freeze, covering task/pool invariants,
unchanged initialization and reward routing, independent target arithmetic,
actual synthetic PPO updates, exact stopping, fractional feedback offsets,
strict gain/mode/failure gates and invalid configurations. The unchanged native
binary inherits five model and 48 environment tests. No Rust changes.

Freeze **1,642 source files** (four new files and one collector reward-dispatch
addition), all configurations/tasks, runtime, runner and **177 original input
files**. Archive on the artifact disk as
`learned-flight-motor-single-speed-v1-20261025`. Preserve all **64 prior manifests**
and verify complete LF-067/LF-068 archives before and after the experiment. Retain
all checkpoints, trajectories, rollouts, exports, logs, commands, failures and
results. Append findings below this frozen specification. Local retention is
not an off-machine backup.


## Results — complete, with partial feedback learning but no qualified skill

**All three seeds finish at 0/24 retained speed-16 cases**, versus 0/24 initially.
No final case acquires on time and retains; the movement and joint gates fail
for every seed. All runs complete 86 updates /2,064 optimizer steps with no early
KL stop. Main wall times are 46.52 /81.08 /82.66 seconds internally; every one
of the fourteen child processes exits zero within its frozen budget. There are
no post-freeze source changes, training retries, replacement seeds or extensions.

The result is more informative than a uniformly unchanged failure. **All three
final policies have locally corrective throttle, brake and force response at
all 54 command-16 anchors**: increasing speed decreases throttle and increases
brake. Seed 29's initial brake slope is wrong at all 54 anchors; seed 43's initial
throttle and brake slopes are both wrong at all 54. The protected residual can
learn the appropriate local direction, but the acquired response is insufficient
for timely, accurately centered speed holding in this budget.

### Physical behavior

| Seed | Initial → final retained | Final rest-start speed at tick 900 | Final rest heading error | Initial → final development collisions |
|---|---:|---:|---:|---:|
| 17 | 0 → 0 /24 | 13.257 | 21.96° | 5 → 0 |
| 29 | 0 → 0 /24 | 13.695 | 1.99° | 0 → 5 |
| 43 | 0 → 0 /24 | 17.798 | 1.64° | 0 → 4 |

Rest figures are means across all six held-out rest starts. All collisions are
in the six fast-start cases, which begin at speed 24. Preserve them as physical
failures; post-collision velocities are not steady speed-control outcomes.
The surviving fast starts remain outside the tracking tolerance.

Seed 17 remains limited by its inherited steering behavior: all four modes end
near speed 13.18–13.56 with heading errors around 22°. Its stopping/heading
baseline deficiencies are unchanged, and none of its movement cases acquires.

Seed 29 learns substantially more acceleration: its rest endpoint rises from
7.021 to 13.695, with slow/reverse endpoints 14.137 /13.353. Correction remains
slow on both sides. Representative rest/fast cases are still at **10.859 /21.608**
at tick 360, and 12.301 /20.487 at tick 600. None of the final cases acquires at
any time. The initial fast cases had briefly acquired before losing retention;
that behavior is not preserved by positive-speed training.

Seed 43 reaches the target band from rest, slow and reverse starts, but **all 18
such cases acquire after tick 360 and later leave the band**. In the first rest
case, acquisition is at tick **508** (8.47 s), retention is first breached at
**713** (11.88 s), and speed reaches **17.798** at tick 900. The corresponding
slow/reverse cases acquire at **390 /618** and exit at **595 /824**, ending at
**18.138 /17.357**. Thus extending only the acquisition deadline would not turn
these into retained successes. The first fast case is still at speed **20.935**
at tick 360; no final fast case acquires.

The held-out panel varies absolute headings/world starts, while normalized
aligned motor states are very similar across its six cases per mode. These are
not six independent training seeds or evidence of broad command generalization.

### Learned response direction, strength and bias

On the unchanged 54-anchor panel, the mean command-16 force is:

| Seed | Force at actual speed 10 | Force at requested speed 16 | Force at actual speed 22 | Local force slope per speed unit |
|---|---:|---:|---:|---:|
| 17 | +.826 | **−.700** | −2.020 | −.248 |
| 29 | +.608 | **+.212** | −.208 | −.0698 |
| 43 | +1.374 | **+.492** | −.482 | −.165 |

All final local slopes have the corrective sign at all anchors. Seed 29's
initial mean slope was +.00690 and seed 43's +.05567, both noncorrective on
average. However, the final control term is not centered at zero at the target:
seed 17 decelerates there, while seeds 29/43 accelerate. Seed 29's overspeed
braking response is especially weak. These fixed-input quantities preserve
previous actions and other anchor observations; they are instantaneous native
control terms, not measured closed-loop equilibria or substitutes for flight
qualification. The actual trajectories independently establish slow correction
and, for seed 43, late acquisition followed by overshoot.

### Training exposure and protected baseline

Each seed receives exactly **66,048 command-16 samples** plus 66,048 stopping
samples. Every original movement case completes at least eight visits and every
stopping case at least seven. Movement case visit ranges are 8–12 /8–13 /8–13;
unequal pool lengths and physical failures affect per-case exposure as declared.
No completed movement training episode succeeds: deadline/collision counts are
**67/6**, **56/20**, **58/16** for seeds 17/29/43. Stopping training outcomes are
183 deadlines for seed 17, and 1,006 /995 successes for seeds 29/43.

Observed movement rows with ready heading/spin/lateral motion are:

| Seed | Underspeed by >1 | Within ±1 | Overspeed by >1 |
|---|---:|---:|---:|
| 17 | 1,505 | 0 | 42 |
| 29 | 42,534 | 3,823 | 10,772 |
| 43 | 45,120 | 3,148 | 13,224 |

These are descriptive coverage counts, not full acquisition/retention successes.
Seeds 29/43 encounter substantial ready overspeed experience as well as much
more underspeed experience. Missing overspeed exposure alone cannot explain
this result. Seed 17 still has almost no ready high-speed learning states.

Original stopping is **exactly protected**. Both successful baseline seeds
retain **80/80 zero-command evaluation cases**; seed 17 retains its original
failures. All complete legacy traces match their original parents, and every
final zero-command trajectory equals its initial trajectory. No baseline file
or prior model is overwritten or promoted.

## Validation, costs and retained evidence

All 22 Python tests pass on the first invocation. Their synthetic collector/PPO
fixtures are separate from native training; the old lambda variants also retain
their reward/target and stopping tests. The unchanged binary inherits five model
and 48 environment tests; no new Rust build or native test run was needed.

All fourteen declared child processes pass. Preflight records one seed-17
movement collision under the frozen failure policy and still passes required
sampling/continuation checks. All six exact resumptions, 414,720 likelihood
comparisons, 207,360 saved stopping-row checks, 36,864 GAE/independent lambda-1
rows and 3,864 native inference comparisons pass. Verify 336 complete original
traces, 240 initial/final zero-command traces and 810 zero-command feedback rows.
Final verification takes 89.80 seconds internally /90.83 externally.

Actual native experiment work, including preflight and exact repeats:

- **423,936 training ticks**, including 396,288 main-training ticks.
- **6,624 optimizer steps**, including 6,192 main-training steps.
- **9,908 restoration ticks**.
- **560,165 evaluation ticks** across 624 episodes.
- **3,294 neutral setup ticks**.

Retain 282 checkpoints, 276 rollouts, 24 exports, 1,642 source files, 177 copied
inputs, all complete/partial training and evaluation traces, analysis, commands,
logs and verification. All 64 prior manifests and the complete LF-067/LF-068
archives remain unchanged. Source/config/task/runtime hashes remain frozen.
See results (`docs/data/flight-motor-single-speed-v1-results.json`, in git history),
analysis (`docs/data/flight-motor-single-speed-v1-analysis.json`, in git history) and
[archive receipt](data/flight-motor-single-speed-v1-archive.json).

## Interpretation and next decision

Removing other requested speeds and tripling speed-16 exposure learns a useful
local response direction but does **not** establish competent speed holding.
Cross-command interference is therefore not the sole obstacle. This bounded
result does not prove the architecture cannot represent the task or that a
longer run would necessarily fail. Positive-speed control remains unqualified;
no confirmation, policy promotion or navigation claim follows.

Freeze a saved-update audit of **underspeed versus overspeed contributions within
these single-command runs** next. Inspect declared early/middle/final checkpoints
and all three seeds, keeping ready-state coverage explicit. Separate the useful
braking/acceleration credit, the actor gradients from each error region, incoming
momentum and critic-driven clipping, then project onto response strength and
force at the target. This can test whether opposite error regions undo useful
updates, whether local credit already points the wrong way, or whether correct
updates are too small. Existing correct local slopes make response strength and
speed balance concrete targets for that investigation. Freeze its own budget
before execution; no additional training or audit has been run here.
