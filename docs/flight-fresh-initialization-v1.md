# Fresh initialization versus preserved holding weights

**Status: complete; stopped at update 64, 2026-10-25.** Experiment
`learned-flight-fresh-initialization-v1-20261025`, journal LF-052, follows the
failed first-stage [bearing curriculum](flight-bearing-curriculum-v1.md).
Test whether standard fresh initialization learns that same task more successfully
than the preserved holding-policy start. This is a bounded initialization
comparison; failure will prompt a discussion of low-level skill decomposition.

## Single changed factor

Both arms use LF-051's **curriculum** config, stage 0, seeds **17/29/43**, and
**64 updates**. `warm` loads the same original holding weights through the existing
weights-only initializer. `fresh` removes `warm_start_sha256` and omits the
`--warm-start` argument, leaving the standard `FlightModel` initialization intact.
Do not initialize either arm from LF-051's trained endpoint. No production source,
native runtime, model architecture, reward or task-generator change is needed.

The 39,546-parameter residual architecture, schema input scaling, head-only critic
gradients, global advantage centering, learning rate .000075, gamma .9995,
GAE .98, PPO settings and four persistent collector slots remain fixed. Both use
quarter-scale independent Gaussian exploration (rho=0). Fresh initialization
resets **all** parameters, including learned log deviations to the standard -.5;
warm starts retain their learned deviations. Thus initial realized action
variance can differ despite the same exploration configuration. This experiment
tests the complete initialization choice, not mean weights in isolation.

Construct both models under the same learner seed. The warm initializer preserves
the post-construction Torch RNG; both arms start with identical sampler RNG,
Python RNG, empty Adam and initial collectors. Both start with the same seeded
goal hidden layer and zero-output goal branch. Their remaining weights and
initial functions deliberately differ. Verify each against its own native export
and report each initial physical baseline separately.

## Tasks and physical requirements

Use LF-051's fixed stage-0 training matrix, seed 4510001: sixteen candidates in
each of settled / position / velocity / spin, 64 total, cycled without replacement.
Position-goal distance remains 4–8; absolute goal bearing is `asin(3/d)+5 degrees`,
alternating left/right across rounds, eight cases on each sign. Keep starts,
disturbances, world seeds, deadlines and the other three families unchanged.
The 16,384 position action ticks per pilot must complete all sixteen position
cases at least once. Episodes continue through rollout boundaries.

Evaluate both arms at updates **0 and 64**, deterministic mean controls:

- Original broad recovery: sixteen cases, development seed 4510002.
- Stage-0 positions: sixteen cases at indices `4*j+1`, same development seed.
- Original holds: sixteen cases/four heading strata, seed 4310002.

Keep decisions and physics at **60 Hz**, continuous three-axis controls, distance
≤3, speed ≤1, absolute spin ≤native-f32 .2, and **thirty complete consecutive dwell
intervals**. Recovery/stage deadlines are 900 ticks; original holds are 1800.
No native teacher, fallback or runtime handoff is introduced. These clear-space
tasks do not qualify obstacle navigation.

Require **the same two or more fresh seeds jointly** to meet every gate:

- At least 12/16 stage arrivals, with at least 6/8 on each sign.
- Stage gain at least two over both its own initial policy and matched warm final.
- At least 12/16 immediate original holds, at least two in every heading stratum.
  Fresh policies must earn these fixed thresholds regardless of initial holding.
- Final broad recovery at least the **greater of fresh and warm initial counts**.
  A weaker fresh baseline never lowers the required physical performance.
- Zero physical failures across all three final suites.
- Every position training case completed at least once in both matched arms.

Retain every seed. Higher training reward, goal response or radius entries do
not replace physical arrival. Report settling failures and original nonforward
position outcomes separately. No best-checkpoint selection or later-stage run.

## Exact controls and retained evidence

Preflight verifies the 64 training, sixteen broad-development, sixteen stage and
sixteen hold cases: **112 admissions**. Verify original warm weights/exports and
standard fresh initialization, paired sampler RNG and goal branch, plus **24
native/Python comparisons** on the four saved original collector observations
per model. Native reset requires at most sixteen tasks per call.

Warm complete checkpoints **0/1/16/64** must equal LF-051 curriculum checkpoints
with **no source/config normalization**. All 288 warm physical traces across
three seeds, three suites and two endpoints must reproduce exactly. Fresh
baselines are distinct, not expected to match warm trajectories.

Run six exact **63→64** restart checks, including model, Adam, collector, sampler,
RNG, history and saved advantage evidence. Endpoint parity adds **576 comparisons**,
for **600 total**. Reconstruct outcomes, dwell and rewards; verify 390 saved raw/
normalized advantage records and 384 measured checkpoint update norms. Preserve
actual initial position bearings; inherited modulo-four labels are `round_mod4`,
not the alternating curriculum's physical directions.

Freeze **1,585 source files**, six configs, **314 copied inputs**, specification,
runner, unchanged native runtime/dependencies and **47 prior manifest hashes**
before execution. Inputs retain original warm exports, initial residual exports,
configs, parent checkpoints 0/1/16/64, all warm reference traces, admissions and
parent results. New files are the audit and its tests; production sources stay
unchanged. The 75-test Python suite covers initialization, gates, prior curriculum
geometry, strict continuation, PPO and physical contracts. Native evidence is
the unchanged retained 421-pass/two-ignored baseline, not a new native test run.

## Budget and stopping rule

Twenty processes: one preflight, six pilots, six restart checks, six extra
evaluation/parity checks, one verifier. Training processes have a 1800-second
internal/1820-second external limit; other processes 600/620 seconds. Stop on
failure; no scored retry, extension or automatic stage advance.

Collect **399,360 training ticks**, including 6,144 restart-check ticks; use at
most **6,240 optimizer steps**, **21,600 active-prefix restoration ticks**,
**576 evaluation episodes / 691,200 evaluation action ticks**. Neutral setups
are separate: 112 preflight admissions, 480 pilot admissions, 576 evaluation
resets, 576 endpoint parity resets, plus measured collector starts and resume
resets. Report these costs separately from learning throughput.

Retain **396 new checkpoints**, 24 pilot/resume exports, six preflight exports,
390 advantage records, 384 update-delta records, all completed/active training
traces, evaluation/parity outputs, tests, commands and timings. Previous archives
stay immutable. Local retention is not off-machine backup. This run spends no
reserved confirmation cases and makes no model promotion.

## Results

Fresh initialization finishes with no arrivals or holds and 108 collisions in 144 final attempts; the warm control exactly reproduces LF-051.

| Arm | Seed | Stage arrivals: initial → final /16 | Final left / right, each /8 | Broad recovery: initial → final /16 | Original immediate holds: initial → final /16 | Stage radius entries: initial → final /16 |
|---|---:|---|---|---|---|---|
| warm | 17 | 0 → 0 | 0 / 0 | 10 → 10 | 16 → 16 | 0 → 4 |
| fresh | 17 | 0 → 0 | 0 / 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| warm | 29 | 1 → 2 | 0 / 2 | 9 → 12 | 16 → 16 | 2 → 3 |
| fresh | 29 | 0 → 0 | 0 / 0 | 0 → 0 | 0 → 0 | 0 → 0 |
| warm | 43 | 0 → 0 | 0 / 0 | 13 → 12 | 16 → 16 | 0 → 0 |
| fresh | 43 | 0 → 0 | 0 / 0 | 0 → 0 | 0 → 0 | 0 → 0 |

Fresh policies finish with **0/48 stage arrivals, 0/48 broad recovery successes
and 0/48 original holds**, unchanged from their unsuccessful initial baselines.
No fresh seed meets the competence, paired learning-gain, holding, broad-recovery
or physical-safety gate. Full position training exposure passes in every pilot.
The experiment stops at update 64; no stage advances or model is promoted.

Warm controls finish with **2/48 stage arrivals**, both rightward in seed 29,
**34/48 broad recovery successes** and **48/48 immediate original holds**.
These reproduce LF-051 exactly, including all endpoint physical traces. Their
initial counts were 1/48, 32/48 and 48/48, respectively. Neither arm produces a
leftward stage arrival. Original broad position success is 1/12 in warm, all
forward, and 0/12 in fresh; neither demonstrates original sideways/backward arrival.

Fresh final evaluations contain **108 collisions / 144 attempts**: 31 stage,
29 broad recovery and all 48 original holds. The other 36 attempts reach their
deadlines. Initial fresh evaluations contained 95 collisions. Warm evaluations
have zero physical failures at both endpoints. These failures are retained as
collisions, not relabeled as slow arrivals or hidden in aggregate success counts.

Simply removing the holding warm start does not solve this task under the fixed
budget. This does not prove that holding specialization is irrelevant or that
fresh training cannot work. The unchanged learning rate, exploration and budget
were inherited from the warm-start setup; this is not an optimized fresh-training
recipe. It also resets learned noise deviations with the other parameters, so it
cannot isolate actor-mean specialization from the rest of initialization. Warm
policies reuse previously learned competence; this is not a comparison of total
training cost from scratch.

### Movement and settling

Fresh policies never enter the stage goal radius in any of the 48 cases, at
either endpoint. Their final median per-episode mean thrust is approximately
**.506/.524/.484** for seeds 17/29/43; brake is **.490/.500/.488**. These are
measurements from retained stage trajectories, not intervention trials. They
remain near simultaneous half-thrust/half-brake behavior without useful arrival.

The same retained original-hold case 0 illustrates departure from rest in all
three fresh seeds. Initial position, velocity and spin errors are zero, but the
policies exceed the speed tolerance at **tick 3**, then leave the goal radius at
**ticks 37/37/38**. They eventually collide at ticks **470/1067/1321**, respectively.
This is a failure to maintain the starting hold, well before final arrival timing
could matter. These three case traces illustrate the failure; the full 48-case
holding outcome remains the qualification evidence.

Warm policies reproduce seven stage-radius entries, all rightward. Four seed-17
entries (indices 5/21/45/53) remain above the speed limit on every inside-radius
frame despite acceptable spin; none begins valid dwell. Seed 29's failed entry
53 reaches only 27 qualifying dwell intervals before its deadline. Thus warm
policies retain both a directional limitation and measured braking/settling
failures. Fresh initialization removes neither limitation in this experiment.

### Training evidence

Every declared position training case completes at least once in every pilot.
Completed position episodes are **20/20/19** for fresh and **18/20/18** for warm.
Fresh records **zero sampled training successes across all four families**, zero
sampled position successes and zero position-eligible rows. Warm records 760
sampled successes, including four position successes, all rightward in seed 29.

Fresh receives **35,919/49,152 positive normalized position advantages**, versus
8,003/49,152 for warm, and every fresh goal branch changes. Positive relative
advantages and nonzero parameter updates therefore do not demonstrate useful
physical learning. They are not correct-action labels. Fresh completes all
3,072 planned pilot optimizer steps with no KL stop; warm uses 3,064 with one
KL early stop in seed 43. Another 96 optimizer steps belong to restart checks.
No additional training or inference is used for these read-only aggregations.

| Arm | Seed | Sampled position successes | Position-eligible rows /16384 | Positive position advantages /16384 | Sum of goal-branch update norms | KL stops /64 |
|---|---:|---:|---:|---:|---:|---:|
| warm | 17 | 0 | 0 | 5691 | 0.138574 | 0 |
| fresh | 17 | 0 | 0 | 12312 | 0.164786 | 0 |
| warm | 29 | 4 | 437 | 2022 | 0.125672 | 0 |
| fresh | 29 | 0 | 0 | 11171 | 0.186287 | 0 |
| warm | 43 | 0 | 0 | 290 | 0.163471 | 1 |
| fresh | 43 | 0 | 0 | 12436 | 0.191122 | 0 |

Update norms are Euclidean checkpoint-to-checkpoint changes summed across updates,
not raw gradients or a competence metric.

### Verification and retention

All **75 Python tests** pass. All 112 declared admissions succeed. The warm
initial weights/exports match LF-051; fresh weights match standard initialization.
Initial collectors, empty Adam, Python/Torch sampler RNG and the seeded zero-output
goal branch match between arms. Initial functions and learned exploration
deviations differ as declared. Warm complete checkpoints **0/1/16/64** and all
**288 warm physical traces** reproduce LF-051 exactly, without normalizing any
source, config, state or identity field.

All six **63→64 exact resumes**, **600 native/Python comparisons**, physical
outcome/dwell/reward checks, **390 advantage reconstructions** and **384 measured
update-delta records** pass. Native test evidence remains the unchanged retained
**421-pass/two-ignored** baseline; no new native test run is claimed.

All twenty processes exit zero without scored retries. Concurrent pilot times
are **276.4–312.4 seconds**, extra evaluation/parity processes
**89.8–169.7 seconds**, verifier **51.2 seconds**;
these are not isolated throughput measurements. Use **399,360 training ticks**
(393,216 pilots plus 6,144 restart checks), **9,002 restoration ticks**,
**6,232 optimizer steps** and **360,506 evaluation
action ticks / 576 episodes**, all within the frozen limits.

There are **2,971 neutral setup ticks**: 112 preflight admissions,
480 pilot admissions, 576 evaluation resets, 576 endpoint parity resets,
1,180 pilot training resets, 24 resume restores and
23 resume terminal resets. These remain separate
from action and restoration budgets.

Retain 1,585 sources, 314 copied inputs, six configs, unchanged runtime,
396 new checkpoints, 24 pilot/resume exports, six preflight exports, all traces,
diagnostics, tests, commands and timings. Sealing checks source/config/runtime
hashes, copied input sources and all 47 prior manifests. The generic `best.json`
metadata does not select a qualified model: the frozen gate assesses update 64.
See [results](data/flight-fresh-initialization-v1-results.json) and
[archive receipt](data/flight-fresh-initialization-v1-archive.json). No confirmation
use, later-stage training or promotion; local retention is not off-machine backup.

### Next decision

Stop this arrival-training ablation sequence and discuss the low-level skill
decomposition before another training run. Retain the successful holding models
as experimental baselines; neither current arm qualifies directional navigation.
Keep the physical arrival requirements and the existing five-stage curriculum
archived without extending its failed progression.

A concrete option to discuss is a learned motor skill that receives desired
heading/speed commands and current ship state, then produces the existing
continuous steering, thrust and brake controls. First define separate fixed
turn-and-settle and brake-to-rest diagnostic tasks, then test combined command
tracking and changing commands. Their criteria should directly measure heading
error, spin and velocity response while preserving the eventual arrival contract.
This would help determine whether the missing capability is basic motor control
before asking a navigation policy to coordinate goal approach and stopping.

This is a proposed interface and training decomposition, not an approved runtime
architecture or a claim that it will work. Resolve command feasibility, inputs,
initialization, rewards and fixed coverage together before freezing an experiment.
A later learned navigation layer could issue commands continuously; this does not
require a native teacher, fallback or runtime handoff between controllers. No
motor-skill implementation or new training experiment is executed in LF-052.

