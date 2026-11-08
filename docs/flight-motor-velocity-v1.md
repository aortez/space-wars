# Motor braking and velocity-tracking pilot

**Status: complete; joint gate failed in all three seeds, 2026-10-25.** Experiment
`learned-flight-motor-velocity-v1-20261025`, journal LF-055. The user authorized
braking and velocity tracking after the [retention audit](flight-motor-retention-v1.md).

## Question and stage transition

Can the same small motor network learn to brake substantial translation and
sustain a requested nonzero velocity while preserving its learned heading control?
Keep the eighteen-input schema, 3,463-parameter independent actor/critic model,
continuous controls, 60 Hz decisions/physics and production simulator unchanged.
No native decisions, teacher, action overrides, rescue, handoff or navigation integration.

Use all three LF-053 final update-64 models, seeds **17/29/43**, unchanged by
LF-054. Copy every model tensor, including critic and learned log deviations.
Initialize fresh Adam state, episode collector, update counter, history and seeded
Torch/Python RNG streams. This is a **weights-only stage transition**, not a
changed-runtime exact resume. Bind each input checkpoint by SHA-256; keep strict
same-stage checkpoint identity. Retain seed 17's known heading offset without
repair, replacement or selection. Export only initial/final policies for scoring.

## Frozen coverage

Each split has **104 cases**: 24 brake-to-rest, 48 velocity-tracking and the
32 original heading cases. Four persistent round-robin slots have 26 cases each.
No case additions, adaptive sampling or curriculum promotions after observing results.

| Family | Training | Development |
|---|---|---|
| Brake /24 | Initial speeds 6,14,26; four velocity directions; heading deltas ±45° | Speeds 8,18,30; same full crossing; deltas ±60° |
| Track /48 | Command speeds 4,10,16; eight initial velocity modes; deltas ±45° | Command speeds 6,12,18; same full crossing; deltas ±60° |
| Turn /24, stationary /8 | Exact LF-053 training cases | Exact LF-053/LF-054 development cases |

Brake directions are forward, reverse, left and right relative to requested heading.
Tracking modes, expressed as (forward,lateral) times command speed, are
(0,0),(.5,0),(1.5,0),(-1,0),(0,1),(0,-1),(1,1),(1,-1).
Thus the complete Cartesian matrix includes acceleration, deceleration, reversal
and both lateral signs. New-case task seeds are 5510001/5510002; heading rehearsal
retains 5310001/5310002. Independent per-case world seeds draw full-circle world
bearings and desired headings within ±60° of radial outward. This keeps intended
motion in clear space at altitude 150 without extending the supported world envelope.
New cases start at zero spin; original turns retain their prior spin matrix.
Directions are world-frame vectors; tests check actual post-neutral-reset heading,
COM velocity error and input encoding. No teleport after setup. Gravity/planet spin
remain zero. Commands stay fixed throughout each episode.

Training ends on original acquisition or six-second deadline. Development runs
all 104 cases for **900 ticks/fifteen seconds**, with acquisition required by tick
360. Acquisition remains heading error ≤5°, COM velocity-vector error ≤1 unit/s,
and spin ≤0.2 rad/s for thirty complete consecutive intervals. Any subsequent
tolerance breach permanently fails retention, even after recovery. Physical
failures terminate immediately. A passing speed result therefore includes sustained
tracking, not just an early crossing. This is not exact stopping or position holding.
Report tail (last 120 available ticks) velocity RMS and actuator means, including
failures; short traces are explicitly identifiable by their tick counts.

## Reward and bounded training

Per-tick reward is
`-[1-cos(heading_error) + .1*(spin/1.8)^2 + (velocity_error/5)^2]/60`,
plus +1 on physical motor success or −5 on collision/damage/loss/boundary escape.
Deadline adds zero. The velocity cost is explicitly **five times** LF-053's weight;
heading/spin and terminal terms are unchanged. Apply it consistently to all training
families, including heading rehearsal. There is no position-distance potential,
native imitation, threshold-derived shaping discontinuity or effort penalty.

Freeze **128 PPO updates** per seed: four slots ×256 ticks/update, four epochs,
minibatch 256, learning rate .0003, Adam epsilon 1e-5, gamma .99, lambda .95,
clip .2, value weight .5, entropy weight .001, combined gradient clip .5, target
KL .03. Same stochastic transformed-Gaussian sampler. No extensions or retries;
900 seconds internal/920 external per pilot. Each training case must complete at
least three episodes. That is coverage evidence, not equal time allocation.
This stage changes task coverage, velocity cost and optimizer state together; it
is not a one-factor causal comparison with LF-053.

## Predeclared joint gate

At least **two of the same three seeds** must pass every condition:

- Brake: ≥20/24 retained; ≥6/8 at each initial speed and ≥10/12 per turn sign.
- Track: ≥40/48 retained; ≥12/16 at each command speed and ≥4/6 per initial mode.
- Track learning gain: ≥24 additional retained cases versus its initial policy.
- Heading regression: ≥20/24 retained turns, ≥9/12 per direction, ≥7/8 stationary
  cases acquired at tick 30; **no lost parent heading success**. Since seeds 29/43
  passed all 32 previously, each must preserve all 32. Seed 17's empty prior success
  set cannot grant a vacuous competence pass.
- Zero final physical failures across all 104 development cases.

Score native mean-action exports only. Inspect all initial/final trajectories.
Initial heading-retention traces must exactly reproduce all 96 LF-054 traces.
Do not infer braking learning if the inherited policy already passes that family.
No reserved confirmation use, model promotion or navigation qualification.

## Evidence and budgets

Three pilots and three fresh-process replays of update 128 from checkpoint 127,
then one bounded verifier (300 internal/320 external seconds). Main training
393,216 action ticks; restart repetition 3,072; total **396,288**. At most 6,192
optimizer steps and 4,308 active-trace restoration ticks. Six initial/final
104-case evaluations: **624 episodes**, at most **561,600** action ticks and
624 native/Python inference comparisons. Record neutral setup ticks separately.

Retain 390 new checkpoints, 12 initial/final/restart exports, 387 rollout tensors,
all completed and active training traces, all scored traces, parity data, source
snapshot, input checkpoints/exports/reference traces, runtime/dependencies, task
matrices/configs, specification, commands, timings and tests. Three exact same-stage
resumes must match model, Adam, collector, RNG, history and repeated rollout tensors.
Verify parent tensors and empty optimizer at update zero. Verify every source and
input hash and all 50 prior manifest hashes before sealing. Local retention is
not off-machine backup.

## Results

Braking is already present in the two passing parent policies; the velocity-training stage learns no sustained nonzero-speed tracking and regresses heading retention.

All three seeds complete the frozen 128 updates. **Zero of three** passes the
joint gate. Initial and final scored outcomes below require acquisition by six
seconds and uninterrupted retention through fifteen seconds.

| Seed | Brake retained /24, before → after | Track retained /48, before → after | Original turns retained /24 | Original stationary retained /8 |
|---:|---:|---:|---:|---:|
| 17 | 0 → 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| 29 | 24 → 0 | 0 → 0 | 24 → 0 | 8 → 0 |
| 43 | 24 → 24 | 0 → 0 | 24 → 24 | 8 → 0 |

The initial policies for seeds **29 and 43 already pass all 24 braking cases**,
including unseen initial speeds 8/18/30 in all four velocity directions and both
turn signs. This is new evidence about inherited ability, not a learning gain
from this stage. All three final models slow near rest: median late speed on
braking cases is .609/.251/.437 units/s. Seed 17's and seed 29's zero braking
success counts reflect their failed joint heading requirement, not a failure to
decelerate translational velocity. Zero physical failures occur in all 624 scored
initial/final episodes; every episode reaches the full fifteen-second horizon.

No nonzero-speed case acquires by the deadline, before or after training. Final
median velocity RMS over the last two seconds, by requested speed:

| Seed | Requested 6 | Requested 12 | Requested 18 |
|---:|---:|---:|---:|
| 17 | 5.390 | 11.385 | 17.384 |
| 29 | 5.789 | 11.790 | 17.792 |
| 43 | 5.548 | 11.531 | 17.515 |

These policies still produce very little forward motion. Their final tracking
throttle medians are about **1.9–2.9%**, with brakes around **47–86%** across
requested speeds. A larger desired speed barely changes their late actuator
choices. This evidence identifies weak commanded propulsion in the observed
trajectories; it does not by itself identify the responsible parameters or updates.

### Heading regressions caught by the long tests

Seed **17** improves its zero-speed equilibrium from about 19.57° heading error
to **8.13°**, still outside the 5° gate. Seed **29** worsens from approximately
.10° to **31.01°**, losing all 32 original successes despite lower residual speed.
Seed **43** retains all turns and braking cases, but loses all eight stationary
retention checks. Those stationary cases acquire at **tick 30**, then cross the
heading tolerance at **tick 105 / 1.75 seconds**, with speed about .426 and spin
.100. The first-exit rule correctly rejects later recovery. A test ending at the
first half-second dwell would have hidden this regression.

Consequently, even seed 43 fails the joint gate independently of its absent speed
tracking. Keep the earlier LF-053/LF-054 policies as the scoped heading-capable
baseline; no LF-055 model replaces them or is promoted.

### What the saved exploration samples show

A post-hoc, read-only inspection of **all 393,216 saved main-training samples**
uses recorded actuator latents, with no new physics, model inference, optimizer
steps or changed acceptance criteria. There are **193,148 nonzero-speed training
samples**. Only **two** use throttle above 20%; none uses throttle above 50%,
and **none** releases brake below 10%.

| Seed | Tracking samples | Throttle median | Throttle 95th percentile | Maximum throttle | Brake median |
|---:|---:|---:|---:|---:|---:|
| 17 | 62,076 | 3.02% | 6.55% | 21.74% | 54.80% |
| 29 | 61,952 | 2.30% | 5.16% | 20.75% | 80.72% |
| 43 | 69,120 | 1.65% | 3.75% | 12.98% | 53.92% |

The low-throttle initialization used for the original turning pilot survives as
a strong learned action preference. **Insufficient propulsion exploration is a
supported next hypothesis**: training almost never tries substantial thrust or
released brakes, even while the command requests motion. This is not a causal
proof that exploration alone explains the failure, nor evidence that the network
is too small. Task mixing, the stronger velocity cost and the fresh optimizer also
changed together. Do not respond by extending this run or adding cases ad hoc.

### Verification, coverage and retained evidence

All **17 Python tests** and **48 native environment tests** pass. Tests cover
all 208 train/development resets, actual command/velocity encoding, parent-hash
rejection, unchanged initialization tensors/RNG, empty Adam state, refusal of
changed-runtime exact resume, same-stage continuation, dense cost collection and
joint-gate rejection. The native executable is byte-identical to LF-054; no native
physics, observation or inference code changed. The existing learner dispatches
explicitly to the new stage; legacy default training behavior is retained.

All three update-zero states exactly copy their declared parent's model tensors
and exported policy, with fresh optimizer/collector/history. All **96 initial
heading-retention traces** exactly reproduce LF-054. All **624 native/Python
inference comparisons** pass. All **three fresh-process update-128 resumes**
exactly match full checkpoints and repeated rollout tensors. Physical frame
errors, dwell, retention, terminal outcomes and training rewards are reconstructed
from retained traces. Every one of the 104 training cases completes **three or
four times in every seed**.

Sampled-training completed outcomes are 3 successes/361 deadlines for seed 17,
0/364 for seed 29, and 79/315 for seed 43. Each pilot executes 2,048 optimizer
steps with **no KL early stops**. Stochastic-training outcomes differ from scored
mean-action evaluation; neither reward nor training success substitutes for the gate.

All seven bounded processes exit zero without retry. Pilot internal times are
**99.31–106.23 seconds**, external times **100.50–107.37 seconds**; restart
processes take 2.22–2.32 seconds and verification takes 14.43 seconds externally.
Use **396,288 training action ticks** including 3,072 replayed update ticks,
**6,192 optimizer steps**, **1,336 restoration ticks**, **561,600 evaluation
action ticks** and **1,780 neutral setup ticks**. Every declared limit is met.

Retain 1,600 frozen source files, 104 copied inputs, 390 new checkpoints, 12 policy
exports, 387 rollout tensors, all traces, tests, timings and post-hoc analysis.
All 50 prior manifest hashes and input origins remain unchanged. See
[results](data/flight-motor-velocity-v1-results.json) and
[archive receipt](data/flight-motor-velocity-v1-archive.json). No reserved
confirmation use, model promotion or navigation qualification. Local retention
is not off-machine backup.

## Next decision

Freeze a small **propulsion-exploration audit** before another training run.
Use all three original LF-053 final policies, whose heading-retention behavior is
measured by LF-054; preserve LF-055 as a failed stage, without selecting an
intermediate checkpoint. Compare unchanged exploration with explicitly bounded
throttle/brake exploration variants on fixed aligned-speed cases, keeping the
learned controller in charge of all actuators. The immediate question is whether
samples can produce meaningful forward acceleration while heading remains stable.
Freeze variants, common random draws, physical metrics and a short budget first;
no optimizer steps in that audit. It would test exploration coverage, not prove
that PPO can learn velocity control. Preserve the LF-055 full matrix and turning/
retention regressions for any later training comparison. No architecture change,
native teacher, fallback, controller handoff or automatic budget extension.
