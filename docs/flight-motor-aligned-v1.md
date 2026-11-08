# Aligned motor speed-control comparison v1

LF-059, 2026-10-25. Frozen before preflight or main training. C4 learning
experiment 28; audit count remains 18.

## Question and comparison

Can repeated aligned acceleration/deceleration experience teach deterministic
speed control, and does removing stopping actor updates improve that learning?
LF-058 found rare useful acceleration receiving favorable credit, much stronger
stopping bias scores, and conservative aggregate updates. This comparison targets
those issues while retaining a direct control arm and explicit capability gates.

Two arms, seeds **17/29/43**, original LF-053 parents in every run:

- **joint:** actor learns from movement and stopping.
- **movement_only:** actor learns from movement; stopping actor/entropy loss is
  zero. Stopping trajectories still train the separate critic and are retained.

Every rollout has **512 movement + 512 stopping rows** in both arms, with two
persistent slots assigned to each command group. Movement normalization and its
loss coefficient do not change between arms. Normalize advantages independently
within movement and stopping over each rollout. Each minibatch actor loss is
`0.5 * (movement_mean_loss + stopping_weight * stopping_mean_loss)`; entropy uses
the same coefficients. `stopping_weight` alone changes from 1 to 0. Value loss
uses all rows in both arms. PPO passes, minibatches, Adam settings and combined
norm clipping are identical. Different learned actions may change episode
schedules; equal movement action counts do not mean identical later trajectories.

This removes direct stopping actor gradients and normalization competition. It
does not isolate the common critic, combined norm clipping, shared readout
parameters or Adam history from every indirect effect. The matched comparison
measures the whole stated loss change, not a claim of perfect causal separation
of every downstream training mechanism.

## Common diagnostic changes and limits

All controls remain continuous and evaluated at **60 Hz**. Keep the existing
3,463-parameter motor architecture, 18 inputs and native inference/export format.
Freeze the original actor's two feature layers and steering output row. Steering
uses its **learned deterministic mean** during both training and evaluation.
Only the two actuator readout rows/biases and their deviations learn, alongside
the independent critic: **66 actuator readout + 2 deviation + 1,697 critic = 1,765
effective trainable parameters**. Verify frozen weights at every checkpoint.

Initialize actuator readout weights to zero, biases to logits of **25% throttle /
10% brake**, and actuator log deviations to **-.7**. Copy all other weights from
the original LF-053 parent; reset Adam/RNG/collector/history for this new stage.
This declared reset is a new initial baseline, not an exact continuation of the
old policy. Evaluate that baseline before learning. Independent Gaussian actuator
noise replaces broad AR noise in both arms; steering has no stochastic likelihood
term. This avoids requiring a rare broad-noise excursion to start accelerating,
and gives movement and stopping the same likelihood sensitivity at equal sigma.
No action is overridden after sampling or inference.

Freezing steering preserves its function on identical inputs, not its physical
trajectory when thrust/brake and therefore inputs change. Seed 17's known steering
limitation is retained. All three seeds stay in the comparison. Fixed features
limit what the actuator readout can learn; a failure does not rule out a trainable
feature representation. This is a controlled entry-stage test, not an isolated
proof that stopping updates explain all LF-057 failure. Multiple common changes
separate both arms from LF-057; only the stopping-loss coefficient distinguishes
the two new arms.

Use the exact LF-055 velocity reward and original gamma .99 / lambda .95,
finite-episode deadline zero-bootstrap, 360-tick training deadline, PPO clip .2,
learning rate .0003, value weight .5, entropy .001, norm clip .5 and KL limit .03.
No time input, deadline correction, new reward term or architecture expansion.

## Frozen tasks and gates

Training world seed **5910001**, development **5910002**. All new starts have
zero heading error and spin, no gravity and stationary planet, altitude 150.
Absolute headings/bearings vary deterministically. New training cases:

- 24 movement cases: commands **4/10/16**, initial forward speed **0/0.5/1.5/-1**
  times command, two independently seeded bearings per combination; no initial
  lateral velocity. This includes acceleration and overspeed braking.
- 24 stopping cases: initial speed **0/6/14**, four translation directions,
  two bearings. Desired speed is zero. Eight stationary cases are included.

Interleave movement/stopping candidates so slots 0/2 always move and 1/3 always
stop. All 48 cases must complete at least seven times per main run. Report physical
outcomes, heading/spin exposure, peak speed, actuator coverage and useful
acceleration. Preserve LF-058's strict useful-push definition (>50% throttle,
<10% brake, underspeed >1, heading <=5°, spin <=.2, lateral <=1 and immediate
velocity-error reduction >1e-4). Also report an explicitly separate gentle-push
metric using >10% throttle and <20% brake, with the same physical conditions.

Each initial/final development evaluation has **104 cases**:

- 24 aligned tracking: unseen commands **6/12/18**, same four speed modes and
  two new bearings each.
- 24 aligned stopping: unseen initial speeds **0/8/18**, same direction matrix.
- All 24 original LF-055 development braking cases and all 32 original LF-054
  heading/stationary cases, unchanged. These include turning starts.

Use original native heading <=5°, velocity-vector error <=1 unit/s and spin <=.2
for 30 complete consecutive intervals. Acquire by tick **360** and preserve all
conditions through tick **900**, with no later breach. Physical failure wins.
No success is inferred from reward, speed alone or briefly crossing the target.

Per-seed movement-learning gate: **>=20/24** tracking retained, **>=6/8** at every
requested speed, **>=4/6** in every initial-speed mode, gain **>=12** over its own
new baseline, and no physical failures. Separate complete-stage gate also requires
**>=20/24** aligned stopping including **>=7/8** stationary, **>=20/24** original
braking, original heading gate (>=20/24 turns, >=9 each direction, >=7/8 immediate
stationary holds) and **no lost original LF-054 heading success**. Compare to
original parent retention, not merely the deliberately reset actuator baseline.

Report both arms and every seed. Overall complete-stage pass requires at least
two movement_only seeds passing all gates. Separately, the stopping-interference
hypothesis receives the declared paired support only if movement_only retains
**at least four more tracking cases** than joint in at least two seeds. Report
movement learning even when retention or the paired hypothesis fails. No automatic
promotion, checkpoint selection, extra seed or training extension.

## Preflight, validation and retained budget

Nineteen Python tests pass before freeze: ten new checks covering task coordinates,
matched config difference, declared initialization/steering equivalence, actuator
likelihood, group normalization, stopping-loss gradients, actual Adam isolation,
collector reward/slot pairing, gates and cross-arm resume rejection; nine LF-058
regressions. The native-coordinate test resets all 152 tasks; the collector test
uses one 1,024-action rollout. These are implementation checks outside the declared
experiment runs. Native runtime is unchanged; LF-057's 48 native tests remain
inherited evidence, not newly rerun tests.

Six **two-update preflights**, then one exact final-update continuation per run.
Use fixed training indices **0/1/40/33**, no scored evaluation. Require no physical
failures, all four cases completed, exact checkpoint/rollout continuation, frozen
steering, reconstructed behavior likelihood, and at least 256/1,024 movement
samples with throttle >10%, independently at least 256 with brake <20%. Stop if
preflight fails; preserve evidence and do not repair the frozen recipe in place.
Discard preflight weights. Main runs start again from declared original parents.

Six **128-update main runs**, then exact update-128 continuation from update 127.
Every update collects 256 ticks x4 slots; four PPO epochs, minibatches 256, maximum
16 optimizer steps/update. Each preflight is bounded at **120 internal /140 external
seconds**, each main/resume at **900/920**, each verification at **300/320**.
At most three runs concurrently; **26 processes** total, no automatic retries.

Caps include repeats: **811,008 training action ticks**, **12,672 optimizer steps**,
**14,760 restore ticks**, **1,248 evaluation episodes /1,123,200 scored physics
ticks**, **1,248 native/Python comparisons** and **798,720 behavior likelihood
checks**. Main exposure is exactly **393,216 movement +393,216 stopping ticks**.
Count neutral setups separately. Retain **804 checkpoints, 48 exports, 792
rollouts**, every physical trace, configurations, sources/runtime, all failures,
commands/timings/tests and verification. Require at least 7 GiB free before freeze;
a run stops if free space drops below a 512 MiB retention reserve. Delete no prior
experiment to make room.

Archive: `data/training-experiments/learned-flight-motor-aligned-v1-20261025`.
Freeze source/config/task/runner/input hashes before preflight. Copy original
parents and all 56 original zero-speed development references per seed. Preserve
all **54 previous manifests** and input origins. No confirmation-set use, native
teacher, fallback, controller handoff or navigation qualification. Local retention
is not off-machine backup.

## Results

**Removing stopping actor updates did not improve aligned speed control. Neither
arm passes the movement-learning or complete-stage gate.** Joint seed 43 retains
all eight unseen **6-unit/s** tracking cases, across all four initial-speed modes
and both bearings. It retains none at 12 or 18. Every movement-only seed retains
0/24; the paired tracking differences are **0 / 0 / -8**, failing the declared
isolation-improvement criterion. No model is promoted.

| Arm | Seed | Tracking acquired by 6 s | Tracking retained /24 | Aligned stopping /24 | Original braking /24 | Original zero-speed turn/hold /32 | Final collisions /104 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| joint | 17 | 0 | 0 | 0 | 0 | 0 | 0 |
| joint | 29 | 6 | 0 | 0 | 0 | 0 | 4 |
| joint | 43 | 12 | 8 | 0 | 0 | 0 | 0 |
| movement_only | 17 | 0 | 0 | 0 | 0 | 0 | 2 |
| movement_only | 29 | 8 | 0 | 0 | 0 | 0 | 80 |
| movement_only | 43 | 10 | 0 | 0 | 0 | 0 | 0 |

All initial actuator-reset baselines retain **0/104** cases. The reset to constant
25% throttle /10% brake is physically aggressive over fifteen seconds: initial
collisions are **100/101/102 of 104** by seed in each arm. The short preflight has
no physical failures but does not establish long-duration stability. This is an
important limitation of the chosen common initialization. Improvements over this
baseline are not preservation of the original competent zero-speed controller;
the separate original-parent gates correctly reject every final model.

### What the trajectories show

Learning changes acceleration substantially, but the resulting policies show weak
requested-speed dependence. In the fixed development **rest-start cases 0/16/32**,
final forward velocity at tick 900 is:

| Arm/seed | Request 6 | Request 12 | Request 18 |
| --- | ---: | ---: | ---: |
| joint 17 | 3.52 | 3.74 | 4.04 |
| joint 29 | 9.97 | 9.91 | 9.91 |
| joint 43 | 5.57 | 6.05 | 6.63 |
| movement_only 17 | 8.65 | 9.08 | 9.90 |
| movement_only 43 | 8.05 | 7.96 | 7.94 |

These are different real trajectories with different commands and world bearings,
not a matched-observation sensitivity experiment. Nevertheless, the observed
closed-loop behavior is much closer to a narrow cruising-speed range than to
following the three requested speeds. Joint seed 43's eight valid low-speed
successes are real, but do not establish useful command conditioning across the
range. The frozen heading feature representation and restricted actuator readout
are material limits of this diagnostic, not demonstrated sufficient capacity.

Movement-only seed 29 does not settle. In case 0 requesting speed 6 from rest,
COM speed is **21.22 at tick 360**, **33.39 at tick 600** and **46.02 at tick 830**,
followed by a native collision at tick **837**. Throttle remains substantial while
brake remains around 10%; the continuation reveals runaway acceleration that the
six-second training window does not fully observe. Its 80 final failures are
collisions, not an inference/parity error or a newly relaxed safety criterion.

For zero-speed commands, joint seeds 29/43 end with median heading error only
**0.040° /0.083°**, but median velocity error **10.57 /5.20 units/s** on the original
turn/hold suite. Movement-only seed 43 is similarly near **1.05°** while moving at
**8.21 units/s**. Thus failure of the combined original heading/hold task should
not be described as proof of forgetting the frozen steering function. The ship
cannot stop with the new actuator behavior. Seed 17 still has its known angular
limitation: final tracking median heading errors **17.0° joint /20.4° isolated**.

### Useful experience and acquisition versus retention

Each main run receives exactly **65,536 movement and 65,536 stopping actions**.
All 48 training cases complete at least seven times; per-run coverage ranges
from **7–8 to 7–10**. The new gentle-push measure records useful aligned acceleration
repeatedly, particularly in seeds 29/43:

| Arm | Seed | Helpful gentle pushes | Movement heading/spin-good endpoints | Completed tracking successes in training |
| --- | ---: | ---: | ---: | ---: |
| joint | 17 | 1,438 | 2,413 /65,536 | 0 |
| joint | 29 | 7,841 | 46,236 /65,536 | 11 |
| joint | 43 | 14,855 | 58,962 /65,536 | 82 |
| movement_only | 17 | 1,482 | 2,330 /65,536 | 0 |
| movement_only | 29 | 8,079 | 46,945 /65,536 | 4 |
| movement_only | 43 | 13,362 | 54,165 /65,536 | 48 |

These counts use >10% throttle and <20% brake, with the original alignment,
underspeed and immediate-error-reduction requirements. They are **not** directly
comparable to LF-058's >50%/<10% strict metric. The strict metric yields only
**66 samples per arm** here; the initialization/noise aims for gentler acceleration.
No claim of broad stochastic coverage follows from the gentle count alone.

Native training stops immediately after thirty qualified intervals and gives +1.
It does not observe whether the same policy subsequently overshoots or drifts out.
The retained evaluation makes that difference explicit: movement-only seed 43
acquires **10** cases by tick 360 and retains **zero**; joint seed 29 acquires **six**
and retains zero. Joint seed 43 acquires twelve and retains eight. Stochastic
training successes likewise demonstrate acquisition only. A policy that passes
through the target speed can be rewarded before its later failure is visible.

This is a concrete objective/evaluation mismatch worth addressing. It is not proof
that changing termination alone will solve command sensitivity or retention.
LF-058's deadline/bootstrap concern remains separate; this run deliberately did
not change it. The common fixed-feature readout, short horizon, initializer and
actor/critic optimization also limit causal attribution. The matched arm result
specifically rejects removing stopping actor loss as a sufficient improvement in
this recipe. It does not prove stopping gradients never compete in other designs.

### Correctness, bounds and retention

All **19 Python tests** pass. Six two-update preflights pass their declared gates,
and their weights are discarded. All main models start afresh from the declared
parent transfer and actuator reset. Every pilot finishes **128 updates /2,048
optimizer steps**, with no KL early stopping or extra budget.

All **twelve exact resumes** reproduce complete checkpoints and repeated rollouts.
All **798,720 behavior likelihoods** reconstruct, maximum absolute log-probability
error **9.54e-7**. All **1,248 native/Python inference checks**, **804 frozen-steering
checkpoint checks**, twelve declared initializations, **312 paired initial
trajectories**, three identical first main rollouts and **336 original-parent
initial-state comparisons** pass. Frozen steering weights do not imply identical
physical trajectories after changing actuators. The three initial checkpoints
match across arms apart from their declared contracts.

All **26 processes** exit zero within budget, without retry. Main pilots take
**66.77–75.40 seconds internally /67.81–76.44 externally**. Final verification takes
**105.57 /106.59 seconds**. Use **811,008 training ticks**, **12,672 optimizer steps**,
**9,498 restoration ticks**, **910,956 evaluation ticks** and **3,677 neutral setups**.
Evaluation ticks fall below the cap because collisions terminate episodes early.
Training also records three collisions across main movement episodes (joint seed
29: two; movement-only seed 29: one). All failures are retained.

Retain **804 checkpoints, 48 exports, 792 rollouts**, every training/evaluation
trace, 1,611 frozen sources, 176 copied inputs, tasks/configurations, tests,
commands, timings and analysis scripts. Original inputs and all **54 prior
manifests** remain unchanged. See [results](data/flight-motor-aligned-v1-results.json)
and [archive receipt](data/flight-motor-aligned-v1-archive.json). The run stays above
the frozen disk reserve. No confirmation use, model promotion or navigation
qualification. Local retention is not off-machine backup.

## Next decision

Keep stopping actor updates in the next proposed baseline; the isolated arm has
not earned adoption. Before another training run, specify a **sustained speed-control
training objective** that keeps observing the ship after first acquisition and
teaches it to maintain the command, with the existing fifteen-second development
gate unchanged. Separate this acquisition/retention change from the unresolved
deadline-bootstrap question and from any training-budget increase.

Also resolve initialization and representation explicitly. The constant propulsion
reset does not preserve the known stopping behavior, and a frozen heading feature
extractor with only an actuator readout has not learned adequate command dependence.
Preserve the original LF-053/LF-054 zero-speed baseline and inspect how a proposed
actuator representation would respond to velocity error before choosing its next
frozen comparison. Do not treat the reset or removal of stopping updates as defaults.
A trainable actuator branch with explicit velocity-error inputs is a candidate,
not an implemented or proven fix. Do not silently combine all these changes into
another uncontrolled retry.

No additional training is authorized by a failed gate alone. Retain all six final
models, including the eight valid low-speed successes and all collisions. Preserve
60 Hz continuous learned ownership and the bans on native teachers, fallback and
controller handoff; obstacle navigation remains a later qualification task.
