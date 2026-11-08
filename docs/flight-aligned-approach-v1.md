# Aligned approach diagnostic v1

Completed diagnostic, 2026-10-23: **0/48 final arrivals; learning and scoped
progression gates failed.** Experiment `learned-flight-aligned-approach-v1-20261023`.
The original pre-run specification is retained separately in the archive.
Parent: [graded-deadline comparison](flight-graded-deadline-v1.md). The user
approved testing nearby goals with initially aligned headings and low initial
speed/spin to establish whether the learner can acquire approach and stopping
before restoring arbitrary headings and longer distances.

## Question and scope

Can the existing model and PPO recipe learn physically successful short approaches
when the need to first turn toward the goal is removed? Use **three fresh learner
seeds 17/29/43**, not continued checkpoints. Retain the parent's graded reward arm
(weight four), network, exploration and optimization settings; change only the
sampling distribution and its independent admission checks. This is a scoped
learnability diagnostic, not a paired causal test of individual task attributes.
Distance, heading alignment and initial motion change together. Previous broad-task
results are context, not a matched control or generalization evaluation.

The ship retains all three controls: steering, thrust and brake. Observations,
actions and physics remain **60 Hz**. No native controller, labels, fallback,
rescue or prescribed action sequence is involved. Successful physical arrival
qualifies regardless of the exact learned button sequence; inspect actions to
understand how it approaches and stops, not to add a post-hoc requirement.

## Frozen task recipe

`aligned-approach-v1` has four equally represented distance strata:

| Stratum | Initial COM distance |
| --- | --- |
| 0 | 4–6 units |
| 1 | 6–8 units |
| 2 | 8–10 units |
| 3 | 10–12 units |

Initial relative velocity and spin are **exactly zero**, the simplest case of the
proposed low-motion start. Sample spawn bearing and heading offset independently
across the full circle; place the stationary goal along the actual local +Y ship
forward direction, rotated by bearing + heading offset. Thus all world orientations
remain represented while relative heading error starts at zero. Use uniform distance
within each stratum, altitude 140–150, zero gravity/planet spin, no added obstacles
or terrain edits. Existing planet and boundary hazards still apply. Preserve
full open-wing physics and current sensing.

Independent native admission checks the recipe, actual hull clearance, direct route
and goal clearance. Heading alignment is checked within 1e-5 in unit-vector distance
for floating-point tolerance; this is not an allowed sampled heading range. Admission
is geometric evidence, not a flight demonstration, and supplies no policy inputs or
labels. Reject incompatible cases, retain every rejection, cap generation at 128
rounds and fail an incomplete quota. Do not add individual observed failures.

Partitions: **4210001** training, 16 accepted tasks per stratum (64 total);
**4210002** development, four per stratum (16 total); **4210003** reserved
confirmation, four per stratum only if progression passes. Deterministic per-index
RNG uses the recipe name, partition seed and index. Four persistent collector slots
cycle through their stratum pools. Previous confirmation partition 4110003 remains
unused and is not part of this experiment.

Arrival remains COM distance ≤3, relative speed ≤1 unit/s and absolute spin ≤.2
rad/s for **30 complete consecutive ticks**, with free final heading. Deadline
remains **1,800 ticks / 30 seconds**. Every initial distance exceeds the success
radius, so a stationary/braking policy cannot satisfy the task by waiting.

## Reward, model and budget

Retain the 39,445-parameter model, 4,009-value observation, independent latent
Gaussian actions (rho=0), gamma=.9995, lambda=.98, 256-tick rollouts × four slots,
Adam learning rate .0003, four epochs, minibatch 256, PPO clip .2, value weight .5,
entropy weight .01, gradient clip .5 and target KL .03. No architecture, optimizer,
critic-sharing or rollout-length changes.

Reward is the preceding graded arm exactly: potential
`-.5*tanh(distance/100) -.25*tanh(speed/20) -.25*tanh(abs_spin)`, discounted potential
difference, time cost −.001, success +10 and physical/controller failure −10. At
actual deadlines only, with `excess=max(final_distance-3,0)`, terminal score is
`-2 - 4*excess/(30+excess)`. Terminal potential is zero. Rollout boundaries bootstrap;
actual deadlines do not. Censored endpoints receive no terminal grade. The 30-unit
scale is deliberately unchanged despite the narrower task, to avoid another reward
change. This is an experimental configuration; the application default weight remains
zero. No cross-experiment reward-total comparison is a competence metric.

Each run gets **128 updates / 131,072 training ticks**, and a **1,200-second wall
guard** including initial/final evaluation. Total: **393,216 training ticks** and
at most **172,800 development ticks**. Report actual concurrent timings. No budget
extension, replacement seeds, extra training or checkpoint search. Qualification
uses the final update-128 policy, not the best-reference pointer. Initial and final
policies are evaluated deterministically on all 16 development tasks.

## Frozen physical gates

- Learning: ≥4/16 final successes and gain ≥4 versus initialization, in at least
  two seeds.
- Scoped progression: ≥12/16 final successes, gain ≥8, ≥2 successes in every
  distance stratum and zero physical failures, in at least two seeds.
- Only if scoped progression passes, evaluate all three final policies on reserved
  partition 4210003. Require ≥12/16, ≥2 per stratum and zero physical failures in
  at least two seeds. At most 86,400 additional physics ticks and 300 seconds per
  policy. Otherwise leave that partition unused.

Incomplete/nonfinite/infrastructure-failed runs cannot pass. Success would establish
only this aligned stationary-start skill, not arbitrary-heading flight or obstacle
navigation. It would support a separately frozen expansion. Failure would narrow the
remaining question to learning this simpler maneuver under the present recipe; it
would not prove that PPO or the network architecture is incapable in general.

Report outcomes per seed and distance stratum, initial/final distance and reduction,
goal entries, speed/spin/dwell, collisions/damage, thrust/brake use, optimizer metrics
and sampled training outcomes separately from deterministic evaluation. In successful
traces inspect thrust and braking around approach/arrival. Do not infer competence
from exploratory thrust frequency or smaller distance alone.

## Verification and retention

Before learning: test reproducible sampling/partition separation, recipe bounds and
native rejection, actual reset heading versus goal direction, forward-thrust motion,
stationary-brake non-success, unchanged physical gates and rejection of mixed recipe
rows. Test exact fresh-process continuation using the new recipe; retain existing
policy, exploration, velocity, credit and graded-reward tests. Run native generation
regressions, Python compilation and whitespace checks. Compare old task admission
results against the parent to verify the new dispatch preserves the old recipe.
These are preflight checks, not learned flight evidence.

Freeze selected dirty/untracked source, three configs and this specification before
the retained build/run. Record the actual Rust/Python runtime and dependency lock.
Check trained exports against Rust on **48 development starts**. Retain all **387
checkpoints**, six policy exports, admissions, complete/censored traces, commands,
latest/best references and checks. Seal with a SHA-256 inventory and verify all
thirteen preceding experiment manifests unchanged. Keep the frozen specification
separate from the completed report. Local retention is not an off-machine backup;
external dependency/toolchain binaries are excluded. No commit or clean checkout
is implied.

## Results

All three runs completed **393,216 training ticks** and **156,128 development
ticks**, taking 493.9–527.5 concurrent seconds per run. There were no budget
extensions, replacement seeds or task/reward changes. See [compact results](data/flight-aligned-approach-v1-results.json)
and the [archive receipt](data/flight-aligned-approach-v1-archive.json).

| Measure | Seed 17 | Seed 29 | Seed 43 |
| --- | ---: | ---: | ---: |
| Initial development successes | 0/16 | 0/16 | 0/16 |
| Final development successes | 0/16 | 0/16 | 0/16 |
| Final deadlines | 16 | 16 | 16 |
| Final goal-radius entries | 0 | 0 | 0 |
| Final thrust fraction | 0% | 0% | 0% |
| Final brake fraction | 100% | 100% | 0% |
| Sampled training successes | 0 | 0 | 2 |
| Training collisions | 18 | 6 | 9 |
| Training deadlines | 61 | 67 | 66 |
| Training goal-radius entries / attempts | 71/83 | 69/77 | 66/81 |
| Training attempts meeting all instantaneous tolerances | 30 | 19 | 32 |
| Maximum training dwell | 29 ticks | 14 ticks | 30 ticks |

Training attempt counts include four budget-censored attempts per seed. Overall,
**206/241** sampled attempts entered the goal radius, **81/241** met all instantaneous
tolerances at least once, and **2** completed the full dwell. These are descriptive
training outcomes under changing weights, not frozen-policy evaluation. Sampled
thrust averaged 20.1% of ticks. There were 33 training collisions, 194 deadlines,
two successes and twelve censored attempts.

All final evaluation attempts timed out, with no collision or damage. Both physical
gates failed. Reserved confirmation 4210003 remains unused, as does the earlier
4110003 partition. No wider flight or obstacle training followed.

### Final policies stay at the start

All final policies suppressed thrust. Seeds 17/29 braked continuously; seed 43 used
neither thrust nor brake. Mean distance reduction from start was .089, .093 and
approximately zero units respectively. Starting and final distance by stratum:

| Stratum | Initial mean | Final seed 17 | Final seed 29 | Final seed 43 |
| --- | ---: | ---: | ---: | ---: |
| 0 | 4.75 | 4.64 | 4.65 | 4.75 |
| 1 | 7.03 | 6.97 | 6.96 | 7.03 |
| 2 | 9.21 | 9.18 | 9.11 | 9.21 |
| 3 | 10.97 | 10.80 | 10.86 | 10.97 |

No final trajectory entered the radius or accumulated any dwell. Easier starts made
approach common during exploration, but did not produce deterministic travel within
the fixed budget. This combined task change does not isolate which starting attribute
caused the difference from the broader recipe.

### The two sampled successes

Both occurred in seed 43, on distinct training tasks, with every one of the last
30 ticks independently checked against position, speed and spin limits:

| Episode | Start distance | Duration | Thrust ticks | Brake ticks | Longest thrust run | Final distance / speed / absolute spin |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 60 | 9.291 | 612 ticks (10.2 s) | 20 | 274 | 1 tick | 1.889 / .690 / .037 |
| 72 | 4.491 | 408 ticks (6.8 s) | 6 | 163 | 1 tick | 1.743 / .084 / .082 |

These demonstrate sampled feasible maneuvers under full learned action ownership.
They are not evidence that the final frozen model can repeat them. Their thrust
inputs are isolated pulses, not sustained thrust phases. The episodes may span
optimizer updates, and training tasks differ from development tasks.

### Validation and retention

All **45 Python tests** and **10 native generation tests** passed. Preflight verified
actual forward orientation and motion, stationary-brake non-success, native rejection,
partition/gate isolation and exact fresh-process checkpoint continuation. Python
compilation, Rust formatting and whitespace checks passed. No physics or network
architecture code changed; native changes add recipe admission and dispatch only.
The old training/development admission results reproduced exactly. Initial model,
optimizer and RNG states matched the corresponding parent graded-reward seed.

All **48 trained Rust/Python parity fixtures** passed with matching actions and maximum
absolute error **1.19×10⁻⁶**. No nonfinite updates or KL early stops occurred; maximum
update-mean pre-clipping gradient norms were 1.41/2.00/1.77. Every completed terminal
reward matched its outcome and the frozen graded formula; censored episodes received
no terminal score. All 387 checkpoints, six exports, traces and frozen provenance
are retained locally. Thirteen prior manifests remain unchanged; local retention
is not off-machine backup. Defaults and physical criteria are unchanged.

### Interpretation and proposed next diagnostic

The initial need to turn and handle motion was not the sole barrier: removing it
still left final deterministic policies stationary. However, approach and brief
low-speed/low-spin states were frequent during sampled training. Settling remained
unreliable, and the deterministic final behavior did not reproduce even the sampled
approaches. This does not establish a network-capacity limit or a unique reward bug.

A focused next diagnostic is **frozen-policy sampled versus deterministic evaluation**
on a separately declared development replay budget, with fixed weights and seeded
sampling. The current deterministic button rule selects thrust only when its
probability is at least .5; a lower probability can still produce occasional useful
pulses during sampling. The two training successes make this difference worth
measuring, but do not establish that thresholding is the cause. Compare physical
outcomes, goal entries, speed/spin/dwell and action probabilities for the same final
models before selecting another training change. No such extra evaluation was run
here, and the existing deterministic qualification gate has not been redefined.

