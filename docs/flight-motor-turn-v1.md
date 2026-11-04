# Small motor network: turn and settle

**Status: complete; scoped gate passed in two of three seeds, 2026-10-25.** Experiment
`learned-flight-motor-turn-v1-20261025`, journal LF-053. The user accepted trying
[heading/speed motor control](design/learned-motor-controller-v1.md) after LF-052.
Test a small learned controller on zero-speed turns before returning to navigation.

## Frozen task and model

Use real 60 Hz native physics with learned-flight mode, open wings, stationary
planet, zero gravity, altitude 150 and zero initial COM velocity. All three
continuous actuators remain learned. Motor commands specify a fixed world heading
relative to the actual completed reset and zero desired speed. Success requires
heading error ≤5°, COM speed ≤1 and absolute spin ≤native-f32 .2 together for
**30 complete consecutive intervals**, before a **360-tick** deadline. Physical
failures take priority. Position displacement is measured, not an arrival gate;
this does not relax or qualify the separate position-arrival contract.

Training task seed **5310001** generates 32 cases: 24 turns formed by absolute
angles 15/45/90/150°, both signs, initial spins −.6/0/+.6; eight zero-angle,
zero-spin stationary checks. Each index has private deterministic world seed,
world bearing and initial heading, with no rejected-case replacement. Four
persistent collector slots cycle `index % 4` pools of eight. Rollout boundaries
never terminate episodes. Verify all 32 cases complete in each pilot.

Development task seed **5310002** supplies 24 turns at unseen absolute angles
30/60/120/165°, both signs and spins −.45/0/+.45, plus eight stationary checks.
Evaluate all 32 cases at updates **0 and 64**, deterministic native mean controls,
for each learner seed **17/29/43**. No development case trains the policy.
No confirmation partition is used.

The **3,463-parameter** network has eighteen ship-state/command inputs and
independent actor/critic 32×32 tanh branches. No polar or object encoder. Fresh
orthogonal weights, low-gain actor output, initial thrust logit bias **−4**,
other biases zero, initial log deviations **−.7**. This declared low-thrust
initialization may already pass stationary checks; only turning gains count as
learning evidence. All outputs and deviations train; no action overrides.

## Reward and optimization

Per physics tick, the dense cost is

`−[1−cos(heading_error) + .1*(spin/1.8)^2 + .2*(velocity_error/5)^2] / 60`.

Add +1 on physical motor success, −5 on a physical failure, and no additional
terminal reward at deadline. No inherited goal-distance potential, deadline
position penalty, native labels, teacher actions or reward-derived success.
The short task and direct errors form a new learning baseline; success is not
promised by the smooth cost.

Use independent Gaussian sampling with the existing transformed-action PPO
likelihood, standard fresh Adam (epsilon 1e−5), learning rate **.0003**, gamma
**.99**, GAE lambda **.95**, clip .2, value weight .5, entropy weight **.001**,
global gradient norm limit .5, target KL .03, four epochs, minibatches 256.
Each of **64 updates** collects 256 ticks in four slots (1,024 actions).
One CPU thread, deterministic Torch algorithms; no inherited warm-start optimizer
or weights. Retain every seed, checkpoint and rollout. No checkpoint selection.

## Qualification and evidence

Require the **same two or more seeds** jointly to have:

- At least **20/24** final turn successes and **9/12** on each sign.
- At least **12** additional turn successes over that seed's initial model.
- At least **7/8** immediate stationary checks (success exactly at tick 30).
- **Zero physical failures** across all 32 final development cases.

Full training-case exposure is mandatory. Report completion time, final errors,
maximum displacement and dwell, both direction counts, sampled training outcomes
and KL stops. This gate qualifies only the declared zero-speed turning slice.
It does not qualify speed tracking, disturbance rejection, obstacle navigation,
long-duration station keeping or runtime planner integration.

Require native/Python agreement on all **192 endpoint reset observations** and
three exact **63→64** process resumes, including weights, Adam, collector, RNG,
history and the saved update-64 rollout tensors. Independently reconstruct
heading/velocity errors, dwell, physical outcomes and rewards from retained native
traces. There are no native policy decisions in training targets.

Implementation checks before freezing: **seven Python tests**, including
64 candidate reset fixtures, sixteen inference fixtures, malformed export/input
rejection, independent actor/critic gradients and exact two-update continuation;
**44 native flight-environment tests**, including four new motor tests; and
**463 AI library tests passed, one ignored**. The continuation unit test collects
3,072 training ticks separately from the declared pilot budget and does not
supply selected pilot results.

## Budget and retention

Seven processes: three pilots, three restart checks, one verifier. Each training
process has a **900-second internal / 920-second external** limit; verification
has **300/320 seconds**. No scored retry, automatic extension or second recipe.

Use **199,680 training ticks**: 196,608 pilot and 3,072 restart-check actions;
**≤3,120 optimizer steps**, **≤4,308 active-prefix restoration ticks**,
**192 evaluation episodes / ≤69,120 evaluation action ticks**. Endpoint parity
uses the same evaluation reset observations, so adds no reset or physics ticks.
Report collector starts, evaluation resets and resume resets separately.

Retain **198 new checkpoints**, **12 pilot/resume policy exports**, **195 rollout
tensor files**, all completed and active native traces, three configs, both task
matrices, sources, native executable/dependencies, tests, specification, commands,
process exits and timings. Freeze source/config/runtime hashes and the hashes of
all **48 previous manifests** before execution. Preserve the parent LF-052 results
and calibration report as copied provenance. Local retention is not off-machine
backup. No model is promoted by this experiment alone.

## Results

The small motor network passes the turn-and-settle gate in two of three seeds: both complete all 24 unseen turns and retain all eight stationary checks.

| Seed | Turns: initial → final /24 | Final left / right, each /12 | Immediate stationary checks: initial → final /8 | Physical failures /32 | Frozen gate |
|---:|---|---|---|---:|---|
| 17 | 0 → 0 | 0 / 0 | 8 → 0 | 0 | Fail |
| 29 | 0 → 24 | 12 / 12 | 8 → 8 | 0 | Pass |
| 43 | 0 → 24 | 12 / 12 | 8 → 8 | 0 | Pass |

All three initial policies complete **0/24 turns** and **8/8 immediate stationary
checks**. After 64 updates, seeds **29 and 43 each complete 24/24 turns**, including
12/12 left and 12/12 right, and retain 8/8 stationary checks. Each completes all six
cases at every unseen magnitude: 30/60/120/165 degrees, across the three declared
initial spins. Both meet every frozen gate. Seed **17 completes 0/24 turns and
0/8 stationary checks**; it is retained as a failed seed. All final attempts in
all seeds have zero collisions, damage, ship loss or boundary escape.

Aggregate final performance is **48/72 turns and 16/24 stationary checks**. The
predeclared same-two-seed gate passes; this does not make training reliable across
all initializations. The low-thrust initialization already provides the stationary
baseline, so holding alone is not credited as learned improvement. The measured
learning result is the 0→24 turn gain in each passing seed.

This demonstrates a scoped ability to learn bidirectional heading control and
settle under the real simulator/PPO/native-export pipeline. The network, task,
reward, initialization and optimizer settings changed together for this new
baseline; the result does not isolate network size or prove why the earlier
navigation experiments failed. Nonzero-speed commands, braking from substantial
translation, gravity, moving frames, changing commands, long holds and obstacle
navigation remain unqualified.

### Measured behavior and failed seed

Passing seed 29 completes turns in **1.18–3.53 seconds**, median **1.97 seconds**;
seed 43 takes **1.33–3.50 seconds**, median **2.27 seconds**. These times include
the half-second dwell. Their final heading errors are respectively **0.009–0.175°**
and **0.318–1.321°**, comfortably inside the 5° boundary. Maximum final spin is
**.0118/.0494 rad/s**, and maximum final COM speed is **.424/.313 units/s**.
Maximum COM displacement during their turns is **2.60/2.35 units**. Displacement
is a reported diagnostic here, not a motor qualification criterion or proof of
position keeping.

Seed 17 reaches low speed and low spin but settles at an incorrect heading.
Its median final turn error is **19.53°** (range **9.77–20.82°**); every turning
case has zero valid dwell. Even its stationary checks rotate away from the
requested heading and finish approximately **19.54°** off target. All 32 cases
reach the six-second deadline. This is a specific heading-accuracy failure,
not a physical collision or insufficient translational braking in those final
frames. Its largest drift is 3.21 units. The reason this seed learned that bias
has not been established by this pilot.

These are measurements of retained deterministic evaluation trajectories, with
no additional inference or trial. The experiment terminates at first completed
dwell, so it does not establish that the heading remains stable for a longer
interval afterward.

### Training evidence

All 32 training cases complete at least once in every pilot. Seed 17 has 180
completed episodes, all deadlines. Seed 29 has 187 completed episodes: ten sampled
turn successes, five stationary successes and 172 deadlines. Seed 43 has 186:
seven sampled turn successes, six stationary successes and 173 deadlines.

The exploratory training trajectories have far fewer successes than the
native deterministic evaluation policies. All three seeds use the same frozen
sampling scheme and all 1,024 planned pilot optimizer steps; no KL early stop
occurs. Every raw rollout tensor, checkpoint and physical trace is retained.
This difference between sampled and mean-control outcomes is explicit; noisy
execution is not claimed to meet the deterministic gate.

### Verification and retention

All **seven Python tests**, **44 native environment tests**, and **463 AI library
tests** pass (one AI test ignored). The new native environment tests verify heading
semantics, complete-interval dwell, carrier-arrival isolation, invalid actions and
exact active replay. Python checks cover all 64 declared reset fixtures, independent
actor/critic gradients, sixteen unit-test native inference fixtures, malformed
exports/inputs, reward/gate behavior and a fresh-process continuation.

All **three 63→64 exact resumes** reproduce full weights, Adam, collector, RNG and
history, plus their update-64 rollout tensors. All **192 endpoint native/Python
comparisons** pass. Native physical errors, outcomes, dwell and rewards reconstruct
from all completed/active training traces and evaluation traces. All seven declared
processes exit zero, without scored retry or budget extension.

Each pilot, including its initial/final evaluations, takes **20.60–21.27 seconds**
inside the process (concurrent external process times **21.72–22.37 seconds**).
This is measured for the new small task and observation path; it is not an isolated
network-speed comparison with the former navigation runs. The verifier takes
**2.71 seconds** internally.

Use **199,680 training ticks**, including 3,072 restart-check ticks,
**2,825 restoration action ticks**, **3,120 optimizer
steps** and **44,981 evaluation action ticks / 192 episodes**.
There are **781 neutral setup ticks**: 565 pilot resets,
192 evaluation resets, 12 resume restores and 12 resume
terminal resets. Endpoint parity reuses the evaluation reset observations.
Unit-test optimization and reset fixtures are separate implementation checks,
not pilot performance samples.

Retain **1,595 sources**, three configs, both matrices, two copied provenance inputs,
native executable/dependencies, **198 checkpoints**, **12 policy exports**, **195
rollout tensor files**, all traces, tests, commands and timings. All 48 previous
manifests and their copied inputs remain unchanged. See
[results](data/flight-motor-turn-v1-results.json) and
[archive receipt](data/flight-motor-turn-v1-archive.json). No confirmation use,
production model promotion or navigation qualification. Local retention is not
off-machine backup.

### Next decision

Retain this as a passing, narrowly scoped motor-learning checkpoint, including
the failed seed. Keep the architecture and physical thresholds fixed for the
next investigation. Before adding speed tracking, freeze a robustness audit of
heading retention beyond the first half-second dwell and a read-only inspection
of seed 17's heading/steering bias. Evaluate all three final policies; do not
silently discard the failed seed or select a favorable intermediate checkpoint.

Longer-horizon evaluation needs an explicit motor test contract, since the
current environment terminates at first completed dwell. Freeze that audit's
horizons, cases, continuation semantics and budget before running it. If heading
control remains sound in the two passing policies, then freeze the separate
brake-to-rest/velocity-tracking stage and retain turning regression tests. Do not
combine new task coverage, reward changes and extra training ad hoc. No further
training, long-hold audit or navigation integration is performed in LF-053.

