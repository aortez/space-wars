# Near-goal stabilization and recovery comparison

**Status: completed bounded experiment, 2026-10-24.**
Experiment `learned-flight-settle-recover-v1-20261024`, following the
[robustness audit](flight-hold-robustness-v1.md), tests whether systematic correction
experience and a granular state cost extend the three successful short-hold policies.

## Fixed task, model and comparison

Two arms, `legacy` and `dense`, learner seeds 17, 29, 43, **64 updates each**. Initialize
all tensors, including learned exploration deviations, from each seed's final early
policy in the [extended hold experiment](flight-extended-hold-v1.md). This is explicit
weights-only initialization with a fresh Adam optimizer and reseeded sampler/RNG, not
an exact continuation of old training. Hash-pin the exported policy and verify tensor
identity against its parent checkpoint. Within each seed, both arms begin with identical
model, optimizer, RNG, starts and runtime. Keep the 39,447-parameter architecture,
schema-v1 scaling, continuous controls and 60 Hz unchanged. No native teacher, rescue,
fallback or handoff.

Recipe `settle-recover-v1` has four equally weighted sampling slots:

| Family | Initial error | Direction coverage |
|---|---|---|
| settled | zero position, velocity and spin | uniform world bearing/heading |
| position | goal distance 4–8; zero velocity/spin | four body-relative quadrants, jitter ±π/8 |
| velocity | at goal, speed 1.2–2; zero spin | four body-relative quadrants, jitter ±π/8 |
| spin | at goal, zero speed, configured absolute spin .3–.5 | alternating positive/negative |

Native neutral setup reduces absolute spin by .025; record actual initial errors.
All starts have altitude 140–150, no gravity, planet spin, obstacles or terrain edits.
Training: 64 cases, 16/family, partition seed **4510001**. Development: 16 cases,
4/family, seed **4510002**. Confirmation seed **4510003 remains unused**. The signed
case matrix is generated before training; rejected cases are not replaced. Require all
geometric admissions and the existing conservative travel-time check to pass.

Version-2 tasks require **300 complete consecutive intervals (five seconds)** within
COM distance 3, speed 1, absolute spin native-f32 .2, free heading; deadline **900 ticks
(fifteen seconds)**. Leaving tolerances resets dwell and permits recovery. Physical
failure precedence and original collision/damage rules remain unchanged.

Both arms use existing potential/time/success/physical-failure/graded-deadline rewards.
Only `dense` adds this per-step cost using the completed step's error magnitudes:

`-.003 * (.5*d²/(9+d²) + .25*v²/(1+v²) + .25*s²/(.04+s²))`.

It is bounded in [−.003, 0], granular inside tolerances, and minimized at zero error.
No command, fuel or heading penalty. Legacy weight is zero and preserves existing reward
terms exactly. The maximum undiscounted time-plus-state cost over 900 ticks is 3.6;
with worst deadline grade −6, a conservative discounted terminal-return lower bound
is −9.6 after removing the common initial potential, above immediate physical failure
(≤−10). Synthetic discounted error paths check approach-and-hold over idle and quiet
holding over equally successful residual-error holding. These are incentive checks,
not evidence of physically reachable maneuvers.

PPO remains four slots ×256 ticks, four epochs/minibatch 256, learning rate .0003,
gamma .9995, lambda .98, clip .2, value .5, entropy .01, gradient clip .5, target KL .03,
independent Gaussian rho 0 exploration. No architecture, normalization or exploration
retuning during this comparison.

## Evaluation and gates

Native deterministic mean actions at initialization and update 64 only, all six runs:

- Primary: the sixteen new development tasks, dwell 300/deadline 900.
- Matched short request: the same sixteen starts, dwell 30/deadline 900. Changing only
  requested dwell still changes observation features; report this separately from primary.
- Original regression: the parent's sixteen development starts, dwell 30/deadline 1800.
  All initial traces must exactly reproduce the 48 copied parent successes.

Each suite has 192 episodes, total **576**; maxima 172,800 +172,800 +345,600 = **691,200
simulation action ticks**. Report all families, physical failures, full dwell reconstruction,
first entry/exit, controls, exploration deviations and training coverage. No post-hoc
case additions, checkpoint selection by intermediate score or confirmation evaluation.

Across-seed gates require the condition in at least two of three seeds per arm:

- Learning evidence: final primary success ≥4/16 and gain ≥4.
- Sustained recovery: final ≥12/16, gain ≥8, ≥3/4 in every family, zero physical failures.
- Short-hold retention: original immediate holds ≥12/16, ≥2 per heading stratum,
  zero physical failures. Also disclose every loss from the original 16/16 per seed.
- Dense improvement over legacy: at least four additional primary successes in two seeds.

Passing remains scoped development evidence, without model promotion or a general
navigation claim. Starts have isolated errors; combined errors and obstacles are outside
this experiment.

## Preflight, budgets and retention

Before the six main runs: seed 17, sixteen original updates per arm and checkpoint
15→16 exact resume. **34,816 ticks**, at most 544 optimizer steps. Four training processes
plus verification, 300 seconds each. Require exact complete resumed checkpoint equality.
Coverage gate: each family has 4,096 rows; all four directional classes (two spin signs)
have started; each nonsettled family has ≥256 above-limit correction rows; settled
starts include eligible pre-action states. Record full eligible-state/dwell coverage:
passing the exposure gate alone does not establish sustained success experience.
Main seed-17 states 0/16 must exactly match preflight.

Main training: **393,216 ticks**, at most 6,144 optimizer steps, six single-Torch-thread
CPU workers, 1,800 seconds each, external 1,820. Extra evaluation: six processes,
1,200 seconds each, external 1,220. Report: 900 seconds, external 920. Total 18 subprocesses;
any failure stops dependent work rather than extending the run. Native/Python parity:
16 fixtures at each main endpoint and 16 per preflight arm, **224 comparisons**.
Matched-short admission adds 96 reset setups. Setup ticks are separate from action ticks.

Retain **426 new checkpoints**, **20 exports**, all completed and active training traces,
all 576 evaluation traces, admissions, reward reconstruction, parity, process logs and
incentive/coverage checks. Freeze 1,552 source files, rebuilt native runtime, configs,
specification and 60 parent inputs before learning. Inputs: three checkpoints, three
exports, three configs, original development admission, 48 original traces and two
result reports. Preserve all 30 previous manifests and copied inputs unchanged.
Local retention is not off-machine backup; no commit is implied.

## Results

The near-goal comparison fails sustained recovery and short-hold retention; the added state cost does not improve this bounded run.

| Arm | Seed | Primary initial → final /16 | Final successes: settled / position / velocity / spin (each /4) | Matched short initial → final /16 | Original immediate holds initial → final /16 |
|---|---:|---|---|---|---|
| legacy | 17 | 0 → 0 | 0 / 0 / 0 / 0 | 10 → 1 | 16 → 4 |
| dense | 17 | 0 → 0 | 0 / 0 / 0 / 0 | 10 → 0 | 16 → 0 |
| legacy | 29 | 0 → 1 | 0 / 0 / 1 / 0 | 9 → 11 | 16 → 14 |
| dense | 29 | 0 → 0 | 0 / 0 / 0 / 0 | 9 → 2 | 16 → 6 |
| legacy | 43 | 1 → 0 | 0 / 0 / 0 / 0 | 13 → 0 | 16 → 2 |
| dense | 43 | 1 → 0 | 0 / 0 / 0 / 0 | 13 → 2 | 16 → 1 |

Primary five-second successes start at 1/48 per arm and finish at 1/48 for legacy, 0/48 for dense. No position-error case succeeds in either final arm. Matched short-request successes fall from 32/48 to 12/48 legacy and 4/48 dense. Original immediate holds fall from 48/48 to 20/48 legacy and 7/48 dense; legacy has one additional later arrival. Every aggregate gate fails. Legacy seed 29 is the partial exception: 14/16 original immediate holds, 11/16 matched short recoveries and one primary velocity recovery, but that does not meet the multi-seed criteria. The dense cost has the intended synthetic preference, yet produces no empirical improvement here. This does not prove all dense rewards are ineffective or identify network capacity as the cause. The comparison isolates the added reward term within the new task; changes from parent training also include task distribution, duration, optimizer reset and removal of first-exit termination.

Across-seed gates: `{"legacy": {"learning_evidence": false, "sustained_recovery": false, "short_hold_retained": false}, "dense": {"learning_evidence": false, "sustained_recovery": false, "short_hold_retained": false}}`. Dense improvement: **False**. Paired dense-minus-legacy primary successes: seed 17: +0, seed 29: -1, seed 43: +0.

Physical failures across all 576 evaluation episodes: **0**.

### Training exposure

The preflight passed its declared signed-direction exposure gate, but did not establish successful correction or sustained-hold experience. Neither arm saw a success; maximum dwell was 25 intervals for legacy and 12 for dense. Both had zero fully eligible pre-action rows in position-error episodes. This limitation was reported before the main comparison proceeded.

| Arm | Seed | Training outcomes | Maximum dwell /300 | Eligible pre-action rows: settled / position / velocity / spin | Final latent deviations: steer / thrust / brake |
|---|---:|---|---:|---|---|
| legacy | 17 | {"deadline": 72, "budget_censored": 4} | 26 | 486 / 1 / 248 / 259 | 0.4597 / 0.5953 / 0.6444 |
| dense | 17 | {"deadline": 72, "budget_censored": 4} | 21 | 329 / 0 / 128 / 167 | 0.4527 / 0.5692 / 0.6478 |
| legacy | 29 | {"deadline": 72, "budget_censored": 4} | 55 | 2296 / 646 / 2214 / 2367 | 0.4818 / 0.5543 / 0.6023 |
| dense | 29 | {"deadline": 72, "budget_censored": 4} | 52 | 1687 / 542 / 1529 / 1560 | 0.4804 / 0.5642 / 0.6106 |
| legacy | 43 | {"deadline": 72, "budget_censored": 4} | 33 | 670 / 224 / 381 / 413 | 0.4887 / 0.5500 / 0.6124 |
| dense | 43 | {"deadline": 72, "budget_censored": 4} | 27 | 549 / 237 / 274 / 349 | 0.4730 / 0.5446 / 0.6451 |

All 432 completed main training episodes end at the deadline, with 24 active episodes censored at the fixed budget. None succeeds or ends in physical failure. Maximum sampled dwell across all runs is 55/300 intervals, less than one second versus the required five. Eligible position-error rows are sparse: legacy 1/646/224 and dense 0/542/237 for seeds 17/29/43, out of 16,384 rows per family per run. The signed-direction exposure gate was met, but simultaneous low-error experience remained scarce. Final nominal mean throttle is about 8.3%/3.1%/8.2% for legacy and 12.4%/4.2%/8.9% for dense; all nominal five-second tests fail. This supports reconsidering task difficulty and retention before extending training, without proving one exclusive cause of the regression. Native/Python parity, exact restart and baseline reproduction passed, so the observed losses are not explained by those checked integration paths.

### Reward and verification

Synthetic discounted approach-and-hold return: legacy 7.4729, dense 7.2850; stationary distance-six deadline: −2.2026 and −3.0725. Equally long successful quiet/offset holds tie under the old reward (8.3324), while dense reward distinguishes quiet (8.3324) from distance 1.5 (8.2488), speed .5 (8.2906), or spin .1 (8.2906). Thus the added term provides granular state preference without changing physical success. Synthetic paths do not establish learnability or achievable trajectories.

Native tests: **421 passed, two ignored**. Focused Python tests: **22 passed**. Both exact resumes passed; seed-17 main checkpoints 0/16 reproduce preflight. Within every seed, initial model/optimizer/RNG/collector match across arms; copied parent weights and all 48 original initial evaluation traces reproduce exactly. All 224 native/Python comparisons, frozen-input/source checks, independent training reward and complete-dwell reconstructions passed. Every one of the 64 fixed training cases was visited in every run.

Main training used **393,216 action ticks** and **6,045 optimizer steps**, plus **34,816 preflight ticks**. Evaluation used **403,095 action ticks** in 576 episodes. All 18 subprocesses exited zero within budget; main run times 282.75–286.04 seconds. No budget extension, confirmation use, model promotion or general-navigation claim.

Archive `data/training-experiments/learned-flight-settle-recover-v1-20261024` retains 1,552 sources, rebuilt runtime, 60 parent inputs, 426 new checkpoints, 20 exports, all training/evaluation traces, admissions, reward/coverage/parity evidence and logs. All 30 earlier manifests and copied inputs remain unchanged. See [results](data/flight-settle-recover-v1-results.json) and [receipt](data/flight-settle-recover-v1-archive.json).

## Next decision

Keep the LF-033 short-hold policies as the preserved reference; do not promote either new arm. Before further training, specify a staged shorter-horizon recovery curriculum with fixed signed-error families, explicit progression gates and an original-hold retention requirement. First establish repeatable correction to a short hold; extend requested dwell only after correction and retention pass across seeds. Freeze the progression schedule and distributions rather than appending failed examples. Use the existing reward as the comparison baseline; the present dense term has no demonstrated benefit. Check sampled simultaneous-tolerance coverage as well as case exposure at each stage. Keep confirmation partitions unused, the network unchanged, and native teachers/handoffs absent. This is a proposed next design decision, not another training run.
