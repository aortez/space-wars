# Clear-space flight learnability v1

Completed bounded C4 experiment: **the predefined learning and progression gates
failed**. All three final policies had zero successful development arrivals.
The specification below was frozen before `learned-flight-clear-control-v1-20261022`
ran; its criteria were not changed in response to results. It follows the [C3 integration smoke](flight-ppo-smoke-v1.md).
The question is whether the unchanged model and reward learn physical goal
arrival in a simpler, declared distribution across multiple fresh initializations.
This experiment does not test obstacles, gravity compensation or moving targets.

## Frozen recipe and hypothesis

Keep C3's 39,445-parameter polar/set architecture, observation schema, reward,
PPO hyperparameters and 60 Hz controls/physics. Start each learner from fresh
weights; no C3 weight reuse, labels, native comparison, rescue or takeover.

Use four simultaneous clear-space strata, with one persistent collector slot
per stratum. Position goals are stationary, in uniformly sampled world directions.
Starting heading and bearing are uniform over a full circle; starting velocity
has uniform speed and direction; spin is uniform over its signed range.

| Stratum | Goal distance | Initial speed | Initial absolute spin |
| --- | --- | --- | --- |
| 0: short settling | 4–12 | 0–1 | ≤0.2 rad/s |
| 1: short translation | 12–24 | 0–2 | ≤0.4 rad/s |
| 2: medium translation | 24–48 | 0–4 | ≤0.6 rad/s |
| 3: longer translation | 48–80 | 0–6 | ≤0.8 rad/s |

Each task uses a seeded practice planet, altitude uniformly 140–150, zero gravity,
zero planet rotation, no added obstacles and no terrain edits. The planet and
world boundary still exist and collisions remain failures. A distinct recipe admission path permits the shorter goal ranges and validates
these tighter task limits, then applies the existing native geometric checks
for start/goal clearance and conservative route feasibility;
all attempts/rejections are retained. The original sampler keeps its 30-unit
minimum and original admission behavior. No controller is used for admission. The
new sampler is identified by `clear-control-v1/stratum-N`; the existing candidate
and admission schema remains version 1. Candidate randomness is independently
keyed by recipe, partition seed and candidate index, not by admission outcomes.
Each slot cycles its own accepted pool; equal tick exposure need not produce
equal completed-episode counts. This is a fixed mixture, not an adaptive curriculum.

Arrival remains COM distance ≤3, relative speed ≤1 and absolute spin ≤0.2 for
30 complete ticks, heading free. Deadline remains 1,800 ticks. Tasks start outside
the arrival radius. Every physical terminal and unfinished episode is accounted
for; rollout boundaries do not end tasks. Reward remains C3's bounded potential
difference plus −0.001/tick, +10 success, −10 physical/controller failure, −2 deadline.
No extra penalty against braking is added in response to the smoke result.

## Partitions, budgets and gates

Learner seeds **17, 29, 43**, each independently initialized. All share the same
training task pool to isolate learner-seed variation. Task partition seeds:
4110001 training, 4110002 development, 4110003 reserved confirmation. Training has
16 accepted tasks per stratum (64 total); development has four per stratum (16).
Admission has a 128-round limit; incomplete quota aborts with its audit preserved.
The reserved confirmation partition is generated/evaluated only if the basic
flight gate below passes. It has four tasks per stratum and is not used for
training, development selection or revising this run.

Each learner receives **128 updates ×256 ticks ×4 slots = 131,072 training ticks**,
393,216 in total. Initial and final deterministic development evaluations use
16 episodes each: at most 172,800 additional ticks across three seeds. The wall
guard is 1,200 seconds per learner invocation, including evaluation/checkpoints,
checked between collector/evaluation ticks. No automatic extension or rerun of
poor seeds. Independent runs may execute concurrently on this 16-logical-CPU host;
record per-run timings rather than claim isolated benchmark performance.

Save initialization and all 128 update checkpoints, full optimizer/sampler/RNG
state, completed training traces and active traces. Retain initial/final exports
and distinct latest/best references. Best compares initial versus final by the
C3 physical rule; **qualification always evaluates the final policy**. There is
no intermediate checkpoint search or best-seed replacement.

Two separate gates are fixed before training:

- **Evidence of learning:** final development success ≥4/16 and gain ≥4 successes
  over that seed's untrained policy, in at least two of three learner seeds.
- **Basic-flight progression gate:** final success ≥12/16, gain ≥8 over baseline,
  at least two successes in every stratum and zero physical failures, in at least
  two of three seeds. Deadlines are failures to arrive but are counted separately
  from impacts/damage/boundary loss.

If the progression gate passes, evaluate all three final policies on the reserved
16-task confirmation partition, with ≤86,400 additional physics ticks and a
separate 300-second wall guard per policy. Require ≥12/16 successes, ≥2 in every
stratum and zero physical failures in at least two seeds to confirm this scoped
basic-flight result. This small suite does not establish broad generalization.
If the development gate fails, leave confirmation unused. Do not begin obstacle
training unless basic control is supported; its recipe requires its own declared
experiment. A failed hypothesis is a completed experiment, not achieved competence.

Nonfinite training, infrastructure errors, missing outcomes or contract violations
abort and retain the latest completed checkpoint plus available failure evidence.
Wall budget exhaustion records an interrupted run, not a silently shortened
successful experiment. Every seed is reported, including failed/interrupted ones.

## Inspection and retention

Inspect success/failure trajectories, minimum/final goal errors, dwell, spin,
thrust/brake fractions and physical outcomes separately from reward. Specifically
check whether deterministic policies leave the brake-only behavior and reach the
goal with controlled speed/spin. Retain all development trajectories so success
examples cannot hide other failures. Training outcomes are stochastic and do not
substitute for the deterministic development gate.

Preflight checks cover sampler bounds, determinism, partition differences, native
admission, quota exhaustion, unmodified physical tolerances and gate logic. The
C3 integration suite must continue to pass, including exact restart and inference
parity. Freeze source, executable, configs and this specification before running;
retain hashes, versions, all checkpoints, outcomes, traces and test logs under the
experiment directory. Previous seven experiment manifests remain unchanged.
External dependency/toolchain binaries are excluded; this is local retention,
not an off-machine backup. No source commit is implied by the dirty snapshot.

## Results

All three seeds completed 128 updates and their full 131,072-tick budgets:
**393,216 training ticks** in total. Initial/final development evaluation used
157,956 additional ticks. Runs executed concurrently and took 446.6–477.4 seconds
each including evaluation; training/checkpoint time was 306.8–319.2 seconds,
410.7–427.2 ticks/s per learner. These are concurrent workload measurements.

| Learner seed | Initial successes / collisions / deadlines | Final successes / collisions / deadlines | Training collisions / deadlines | Budget-censored training attempts |
| --- | --- | --- | --- | --- |
| 17 | 0 / 2 / 14 | 0 / 0 / 16 | 6 / 67 | 4 |
| 29 | 0 / 0 / 16 | 0 / 0 / 16 | 12 / 66 | 4 |
| 43 | 0 / 12 / 4 | 0 / 0 / 16 | 4 / 68 | 4 |

Every final policy timed out on every development task: **0/48 successes**.
Neither gate passed in any seed. The reserved confirmation set was not generated
or evaluated, and obstacle training was not started. The relative best-reference
rule selected each final policy through fewer collisions or lower final position
error; this does not qualify a controller. All 223 completed training episodes
also failed (22 collisions, 201 deadlines); 12 active attempts are censored.
All 235 starts and all training ticks reconcile with retained traces/checkpoints.

All 64 training tasks and all 16 development tasks were admitted without rejection.
The three learners consumed identical task-pool files, with distinct learner RNG
seeds. The training recipe was not expanded or altered during the run.

### Observed behavior and diagnostic limits

All 48 final deterministic evaluations held brake for every tick and never used
thrust. None entered the 3-unit goal radius; the closest approach across those
attempts was 4.01 units. Final mean position errors were 32.67, 32.63 and 32.27
units for seeds 17/29/43. Mean relative speeds were 0.153/0.0022/0.0026, and mean
absolute spins 0.125/0.0080/0.0287. The policies suppress motion, but do not navigate.

Post-run trace analysis, including active attempts, found only 10/235 training
attempts entering the position radius. Five ever met all three instantaneous
tolerances; the longest dwell was 25 ticks, below the required 30. No trajectory
produced a successful terminal reward. Sampled thrust-on runs averaged
1.38–1.56 ticks (23–26 ms); none lasted 30 ticks. Overall training thrust fractions
were 12.2–13.8%, brake fractions 58.5–69.0%. This measures the learned stochastic
behavior over the whole run, not a separately evaluated stochastic policy.

A separate, unscored inference diagnostic reversed only the local goal vector
on each of 16 fixed development starting states. Final mean absolute steering
changes were 0.000216, 0.004857 and 0.001033; no steering signs or thrust/brake
decisions changed. Mean sampled thrust probabilities on the original starting
states had fallen to 1.49%, 0.73% and 2.58%. These are counterfactual input probes,
not flown alternative tasks or a causal diagnosis.

The evidence supports a hypothesis that temporally fragmented exploration and
rare successful experience are obstructing useful goal-directed learning. It
does **not** establish which of exploration, reward/credit assignment, feature
scales, model capacity or the chosen budget is decisive. A larger model or an
unbounded continuation is not justified by this experiment alone. Keep the
physical success definition and 60 Hz decision cadence; a future experiment can
test more persistent exploratory controls with correct action likelihoods and
checkpoint state, changing one declared factor at a time.

### Verification and retention

Eleven Python integration tests passed, including the new recipe/gate checks and
bitwise fresh-process continuation with the clear-control sampler. All 651 scenario
tests passed (two ignored), including the test that preserves the original 30-unit
admission minimum and rejects relaxed arrival criteria for the new recipe. All
463 AI library tests passed (one ignored). Rust format, Python compilation and
whitespace checks passed. Clippy completed with 20 existing warnings and none in
the changed bridge/admission implementation.

All updates had finite metrics; none triggered the KL stopping threshold.
Trained PyTorch/Rust parity passed 16 development starts for each of three policies
(48 total), with identical discrete controls and maximum absolute output error
1.91×10⁻⁶. Source, runtime and prior-archive hashes were verified.

The [compact results](data/flight-clear-control-v1-results.json) retain every
initial/final outcome, per-seed gates, throughput, parity and detailed post-run
diagnostics. The [archive receipt](data/flight-clear-control-v1-archive.json)
identifies all **387 original checkpoints**, six exports, completed/active training
traces, all 96 development traces, fixed configs and source/runtime snapshots.
The seven earlier archives remain unchanged. This is a negative learnability
result within a completed experiment, not demonstrated flight capability.

C4 follow-up: specify a small controlled test of exploration persistence and goal
response before increasing training scope or budget. Reuse this declared task
recipe only with an explicit new experiment relationship; do not append individual
failures. Keep the unused confirmation partition isolated, and require physical
arrival evidence before proceeding to obstacle navigation.
