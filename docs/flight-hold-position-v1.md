# Stationary hold-position learning diagnostic

**Status: completed bounded learning diagnostic, 2026-10-24.** Experiment
`learned-flight-hold-position-v1-20261024`, following the failed
[input-scaling comparison](flight-input-scaling-v1.md). Test whether the existing
learner can preserve an already-settled state, before combining settling with approach.
This is a separate task recipe, not an addition to the approach dataset.

## Fixed task and learner

`hold-position-v1` starts the goal exactly at ship COM with zero translational velocity
and zero spin. Both target velocity and gravity/planet spin are zero. Use the existing
practice planet, clear space at altitude 140–150, no added obstacles or terrain edits.
Four strata divide the start heading offset relative to the radial bearing into equal
quadrants of [−π, π]; bearing remains uniform over the full circle. World seeds,
bearings, altitudes and within-quadrant headings are sampled by deterministic per-index
RNG keyed by recipe, partition seed and index. Admission validates the exact recipe,
full hull/start clearance, goal clearance and the existing geometric route checks.
No policy-dependent samples or failure additions.

Use 64 training tasks, 16 per heading stratum, at seed **4310001**; sixteen development
tasks, four per stratum, at **4310002**. Seed **4310003** is reserved and never generated.
Earlier confirmation partitions 4210003/4110003 remain unused. Sixteen development
starts are evaluated at initialization and update 128, with no later checkpoint search.

Three fresh models, learner seeds **17, 29, 43**, default `schema-v1` scaling, unchanged
39,447-parameter architecture. Keep continuous v2 actions, 60 Hz physics/decisions,
independent Gaussian rho=0 exploration, initial log deviations −.5, and current reward:
potential shaping, time penalty, success/failure bonuses and graded deadline. No native
pilot, teacher action labels, fallback, warm start or handcrafted controller in training.

Keep PPO at 128 updates, four slots ×256 ticks, four epochs/minibatch 256, learning rate
.0003, gamma .9995, lambda .98, clip .2, value weight .5, entropy .01, gradient clip .5,
target KL .03. Copy the previous default-arm configs, changing only recipe/partition
seeds. Maintain all checkpoints and complete/active traces. Native runtime changes
only to admit the new recipe; physics, policy inference and success logic are unchanged.

## Outcomes and scope

Physical success remains COM distance ≤3, speed ≤1, absolute spin ≤native-f32 .2,
for **30 complete consecutive intervals**, with free heading. Start eligibility is
not success: the clock/dwell counter begin at zero. Deadline remains 1,800 ticks and
physical failures still take precedence. A return after leaving tolerance can qualify
as an arrival, but is not an immediate hold.

Primary native mean-control evaluation: **96 episodes**, at most 172,800 action ticks.
Report per seed initial/final arrivals, immediate holds, heading coverage, physical
failures, first tolerance-exit time/reason, maximum dwell and actual controls. Retain
all trajectories and independently reconstruct outcomes/dwell. No additional sampled
or approach evaluation in this diagnostic.

- Learning evidence: final ≥4/16 physical successes and gain ≥4 over initialization,
  in at least two seeds, with every process complete.
- Scoped hold capability: final **≥12/16 immediate holds**, gain ≥8 over initialization,
  ≥2 immediate holds in each heading stratum and no physical failures, in at least two
  seeds. An immediate hold is success exactly on tick 30. Keep later returns separate.

This tests only half a second of maintaining a stationary, already-eligible start in
clear, gravity-free space. It does not establish recovery from disturbances, longer
station keeping, approach/braking, obstacle navigation or general flight. Even if a
gate passes, do not automatically expand training or promote a gameplay model.

## Preflight, budgets and retained evidence

Native tests check that the new admission preserves criteria and rejects nonzero
start errors and wrong heading strata. Integration fixtures check that zero commands
require exactly 30 intervals and that half-throttle breaks velocity tolerance. These
are environment correctness fixtures; their commands are never training labels or a
native-controller benchmark. Keep test/build logs and test source.

After freezing source/spec/config/runtime, run seed 17 for two original-config updates,
then resume its update-1 checkpoint for update 2. Require complete bitwise equality of
parameters, optimizer, collector, RNG and history. Main seed-17 states at 0/2 must match
preflight exactly. Check the preflight export on sixteen original hold observations;
check every initial/final main export on all sixteen development observations. Retain
all 112 native/Python input/output comparisons. Verify tensors against checkpoints,
raw atol/rtol 1e−5, action absolute tolerance 1e−5, and parameter/RNG neutrality.

- Preflight: **3,072 training ticks**, ≤48 optimizer steps; original and resumed
  subprocesses each ≤180 s; preflight verifier ≤180 s.
- Main: **393,216 training ticks**, ≤6,144 optimizer steps; three concurrent CPU
  workers, one Torch thread each; training plus evaluation ≤1,800 s per worker,
  external timeout 1,820 s.
- Evaluation: at most 96 episodes /172,800 action ticks plus ordinary setup ticks.
- Parity: 112 inference comparisons and 112 neutral setup ticks; no action rollout.
- Final report: ≤300 s, external 320 s. No extra policy-response grid or new flight trial.

Stop on missing/nonfinite artifacts, contract/parity/replay mismatch, invalid physical
traces or budget exhaustion. Preserve failures; no automatic budget extension, extra
seeds or checkpoint selection. Freeze three parent configs/results, specification,
1,541 selected source files, new native bridge/dependency versions and all **25 previous
manifest hashes**. Retain all newly written checkpoints/exports, admissions, complete
physical traces, parity fixtures, logs, process statuses and report. Seal a new archive
and update the governing plan/journal. Prior archives stay unchanged; local retention
is not off-machine backup and no commit is implied.

## Results

The stationary hold diagnostic produced 0/48 final holds or later arrivals; both learning gates failed.

All three runs completed 128 updates; all seven subprocesses exited zero. No seed,
checkpoint or budget was substituted. Initial/final physical results:

| Seed | Initial arrivals / immediate holds | Final arrivals / immediate holds | Final physical failures | Longest final dwell (ticks) | Learning gate | Scoped hold gate |
|---|---|---|---:|---:|---|---|
| 17 | 0/16 / 0/16 | 0/16 / 0/16 | 0 | 10 | failed | failed |
| 29 | 0/16 / 0/16 | 0/16 / 0/16 | 0 | 6 | failed | failed |
| 43 | 0/16 / 0/16 | 0/16 / 0/16 | 0 | 15 | failed | failed |

Across-seed learning gate: **failed**. Scoped hold gate: **failed**.

### What the final policies did

Controls below are means over all final evaluated action ticks. First-exit ranges
refer to the first post-step frame outside any tolerance; successful immediate holds
have no exit. All per-attempt values and simultaneous violation reasons are retained.

| Seed | Mean throttle | Mean brake | Mean absolute steering | First tolerance-exit tick range | First-exit reasons (episodes, may overlap) |
|---|---:|---:|---:|---|---|
| 17 | 15.38% | 71.39% | 0.0174 | 2–11 | {'velocity': 9, 'spin': 7} |
| 29 | 23.96% | 70.00% | 0.0315 | 2–7 | {'velocity': 5, 'spin': 11} |
| 43 | 12.22% | 54.76% | 0.0188 | 2–16 | {'velocity': 12, 'spin': 4} |

All initial evaluations collided; all final evaluations survived to the deadline without a physical failure. The learner again reduced aggressive motion without learning the required task. Final mean throttle remained **15.38%, 23.96% and 12.22%** by seed, against mean braking of **71.39%, 70.00% and 54.76%**. Throttle never fell below five percent in any final evaluated tick.

Every final policy left the arrival tolerances within **2–16 ticks** (about .03–.27 seconds). The first violation was speed in **26/48** cases and spin in **22/48**; none first failed position tolerance. Maximum complete dwell was **10, 6 and 15 ticks** by seed, below the required thirty. Starting at zero error is therefore insufficient to induce holding, even though the independent zero-command correctness fixtures preserve it.

The sampled training episodes also recorded no successes: **28 collisions, 197 deadlines and twelve censored attempts**. Maximum training dwell was only 9/5/6 ticks. Training exploration and mean evaluation differ, but neither delivered the intended behavior.

This failure does not require an approach maneuver, obstacle avoidance or nonzero goal-distance encoding. It supports investigating how training changes the actuator outputs before adding task complexity or a larger network. It does not identify a unique cause: exploration, advantages/critic estimates, entropy regularization and shared gradients remain unresolved. In particular, the implemented continuous-action entropy includes the sigmoid Jacobian and can oppose strongly reduced throttle demand; its actual contribution relative to the policy and value terms has not been measured here. No entropy, reward or loss coefficient was changed.

### Training evidence

The learner samples continuous actions during training; evaluation uses the native
mean action. The following are training episode outcomes, with active attempts retained
as censored. These are not substitutes for the fixed development evaluation.

| Seed | Training outcomes | Maximum training dwell | Final latent deviations (steer / throttle / brake) |
|---|---|---:|---|
| 17 | {'collision': 17, 'deadline': 62, 'budget_censored': 4} | 9 | 0.5965 / 0.6120 / 0.6365 |
| 29 | {'deadline': 69, 'collision': 4, 'budget_censored': 4} | 5 | 0.5601 / 0.6221 / 0.6315 |
| 43 | {'collision': 7, 'deadline': 66, 'budget_censored': 4} | 6 | 0.5708 / 0.6127 / 0.6590 |

### Verification and retention

Native surface tests passed: **414 passed, two ignored**, none failed. Twelve Python
sampling/admission/interval/gate/report tests passed. The zero-command correctness
fixtures succeeded only after all thirty complete intervals; half-throttle fixtures
left speed tolerance. These controls were not training examples or a pilot benchmark.
Preflight resumed states matched bitwise, and main seed-17 states at updates 0/2
matched preflight exactly. All three runs used identical admissions and unchanged
physical criteria. Every saved physical outcome/dwell was reconstructed.

All **112 native/Python comparisons** passed, with 96 endpoint fixtures and sixteen preflight fixtures. Endpoint maximum raw error 1.19209e-06, action error 1.19209e-07. Exports exactly match checkpoint tensors; parameter/RNG neutrality was verified. The rebuilt bridge changes admission/dispatch only, not physics or inference.

Main training used **393,216 ticks / 6,144 optimizer steps**. Preflight used 3,072 ticks / 48 steps. The 96 evaluations used **131,445 action ticks**; parity used 112 neutral setup ticks. Training took 355.5–356.7 seconds per concurrent worker; training plus evaluation took 472.3–474.1 seconds. Reporting took 8.97 seconds. No budget extension.

Archive `data/training-experiments/learned-flight-hold-position-v1-20261024` retains
1,541 source files, native runtime, three configs, four copied parent inputs, all 391
new checkpoints and ten exports, admissions, training/evaluation traces, parity fixtures
and process logs. All 25 previous manifests and referenced parent inputs remain unchanged.
No model promotion, approach/navigation claim or confirmation use. See
[results](data/flight-hold-position-v1-results.json) and
[archive receipt](data/flight-hold-position-v1-archive.json).

## Next decision

Propose a separately frozen **exact PPO-update audit on the hold runs**. Reconstruct selected saved rollouts/updates and require the original production updates to reproduce exactly. Measure how the policy, entropy and shared value-loss components affect throttle and steering on both rollout observations and fixed hold-start observations; retain original advantages, RNG, optimizer state and clipping effects in comparisons. Declare checkpoint selection and replay/analysis budgets before generating results. This tests the continuous hold-training signal directly; the historical binary update audit does not settle it. Do not launch another long training run or change entropy, reward, architecture or task distribution on suspicion alone. The audit is proposed, not implemented or run. Keep all archives and original physical criteria; no current model is promoted and confirmation stays unused.
