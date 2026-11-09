# Arrival-corridor bearing curriculum: first-stage comparison

**Status: complete; stopped at stage 0/update 64, 2026-10-25.** Experiment
`learned-flight-bearing-curriculum-v1-20261025` follows LF-050. Longer-lasting noise
and wider propulsion exploration did not produce nonforward arrivals while retaining
holding. Test a fixed procedural progression toward larger directional corrections.
The current execution is one paired **stage-0, 64-update** comparison, not the whole
curriculum or a navigation qualification run.

## Full progression, fixed before training

Keep the original `settle-recover-v1` task generator and four equal sampling slots:
settled / position / velocity / spin. Change only **training position-goal bearings**.
Preserve the original per-index starts, goal distances 4–8, velocity/spin disturbances,
world seeds, deadlines and physical criteria. All other families remain bitwise
unchanged. Per-index randomness stays private; global Python/Torch RNG is untouched.

For stages 0–3, let d be the original position-goal distance and R=3 the existing
arrival radius. The straight-flight arrival corridor has half-angle `asin(R/d)`.
Set the goal's absolute ship-relative bearing to `asin(R/d) + stage_margin`,
positive rotation toward the left on even round indices and right on odd rounds.
There are eight cases per sign in each sixteen-case position pool. The margin is
nominal additional heading deflection beyond the straight-flight corridor, not a
replacement success metric. Simply placing small-angle goals inside that corridor
would permit successful straight travel without learning a correction.

| Stage | Margin beyond arrival corridor | Position-goal distribution |
|---|---:|---|
| 0 | 5° | Balanced left/right; absolute bearings approximately 27°–54° |
| 1 | 20° | Balanced left/right; approximately 42°–69° |
| 2 | 60° | Balanced left/right; approximately 82°–109° |
| 3 | 100° | Balanced left/right; approximately 122°–149° |
| 4 | Original generator | Original forward/left/backward/right round cycle and ±22.5° jitter |

Stage 4 returns the original candidates exactly, including their original goal
vectors. No stage substitutes a reward for bearing alignment, scripted maneuver,
native teacher, fallback or controller handoff. One network chooses all three
controls at **60 Hz**. Runtime inference and architecture are unchanged.

The complete conditional progression is capped at **64 updates per stage**, five
stages, both arms, three seeds: at most **1,966,080 pilot training ticks** plus
**30,720 restart-check training ticks**, **31,200 optimizer steps**, **108,000
active-prefix restore ticks**, **2,880 evaluation episodes / 3,456,000 evaluation
ticks**, and 2,880 endpoint parity reset/comparisons. There are no within-stage
extensions or repeated seeds. This ceiling includes the current stage, not an
additional budget. Only stage 0 is executed here; later stages require a retained
passing gate and a separate frozen execution record within this ceiling.

For a later stage, retain all three seeds in both arms. Initialize each from its
own preceding final export, with fresh Adam, fresh collectors and RNG seed
`original_learner_seed + 1000*stage`. Both arms make this same explicit weights-only
transition. Keep unfinished prior episodes archived as budget-censored; never
reinterpret them as terminals or call a changed-contract transition exact resume.
The candidate advances its bearing stage; the broad arm keeps its original tasks.
Task seeds and quotas remain fixed. No favorable model/checkpoint selection.

Stages 1–3 require ≥12/16 stage arrivals, ≥6/8 on each sign, hold retention and
physical safety in the same two seeds. If the stage-entry policy is below the
stage threshold, require gain ≥2; otherwise retain at least its stage-entry count.
Stage 4 instead requires ≥12/16 position arrivals and ≥3/4 in each original
four-direction group. Preserve broad recovery counts at least at both original
initialization and current stage entry. Stage-0 additionally has the paired
learning-benefit requirement below. Report the broad comparison at every stage.
Failure stops progression; no cases or thresholds are added in response to it.

## Current paired comparison

`broad` uses LF-048's original global-centering config without modification.
`curriculum` adds only
`position_curriculum={kind: arrival-corridor-bearing-v1, stage: 0}`. The explicit
setting is allowed only with `settle-recover-v1` and thirty-tick dwell. Malformed
settings fail before run output or physics. Stage identities belong to checkpoint
config; changed-stage exact resumes are refused. Ordinary development/heldout
sampling keeps the original broad distribution. Separate stage development cases
are generated explicitly. Production changes are limited to curriculum.py and
run.py; default tasks and runtime are unchanged.

Both arms use the original residual-architecture holding starts, seeds 17/29/43,
39,546 parameters and zero-output goal branches. Keep original weights-only
initialization, head-only critic gradients, global advantage centering, learning
rate .000075, quarter independent Gaussian noise (rho zero), rewards, gamma .9995,
GAE .98, PPO settings and input scaling unchanged. Config updates=64; stop-after=64.
Neither failed exploration candidate is introduced into this learning experiment.

Training seed 4510001 has sixteen candidates per family (64 total), cycled in the
four persistent collector slots. At 256 ticks/slot/update, 64 updates provide
16,384 position actions: enough to complete every one of the sixteen declared
position cases even if each takes its full 900-tick deadline. Verify every case
actually completes at least once. Training episodes continue across rollout
boundaries; no sampler replacement or curriculum advance occurs during this run.

Development seed 4510002 supplies the original sixteen broad cases (four/family),
and sixteen separate stage-0 position cases at indices `4*j+1`, j=0..15. Original
hold seed 4310002 supplies sixteen holding cases/four heading strata. Evaluate
all three suites deterministically at updates 0/64 in both arms. Keep the original
physical contract: COM distance ≤3, speed ≤1, absolute spin ≤native-f32 .2 and
thirty complete consecutive dwell intervals. Recovery/stage deadlines stay 900;
original holds stay 1800. No reserved confirmation evaluation is performed.

## Current gates and evidence

Require **at least two seeds jointly** to have:

- Stage competence: ≥12/16 stage-0 arrivals and ≥6/8 on each sign.
- Learning benefit: stage-0 gain ≥2 over both the original policy and the matched
  broad arm's final policy, evaluated on those identical stage cases.
- Original-hold retention: ≥12/16 immediate holds, ≥2 in each heading stratum.
- Original broad recovery: at least its own initialization's success count.
- Zero physical failures across all three final evaluation suites.
- Every training position case completed at least once in both arms.

Retain each seed even if it fails. A stage success is not full-direction correction,
obstacle navigation or production qualification. Report original nonforward cases
separately. Larger steering responses or higher training rewards cannot pass the gate.

Preflight verifies all five planned stage matrices: 64 training and sixteen stage
position-development cases per stage, **400 admissions**, no replacement. It also
requires exact original weights, exports and sampler RNG in all six initial models,
plus 24 Python/native comparisons on saved original collector observations.
Require the broad arm's complete checkpoints 0/1/16 to reproduce LF-048 after only
the two declared Python source hashes are normalized by the audit. Actual resumes
stay strict. Require six **63→64 exact resumes**, including model, Adam, sampler,
collector, RNG, history and saved advantage evidence.

All original initial recovery/hold traces must reproduce exactly; stage initial
traces must match between arms. Endpoint parity covers sixteen cases in each of
three suites at two endpoints/six pilots: **576 comparisons/reset fixtures**, 600
comparisons including preflight. Reconstruct physical dwell, rewards and every
saved normalized advantage. Retain actual checkpoint-to-checkpoint parameter changes,
completed position episodes with measured initial bearings, and stage trajectories.
Round-index modulo-four labels in inherited diagnostics are identifiers, not the
curriculum's actual compass directions; record them explicitly as `round_mod4`.

## Current budgets and retention

Twenty processes: one preflight, six pilots, six exact restart checks, six extra
evaluation/parity checks, one verifier. Training processes ≤1800 seconds (1820
external watchdog), other processes ≤600 (620 watchdog). Stop on failure; no scored
retry, automatic stage advance or budget extension.

Use **399,360 newly collected training ticks** including 6,144 restart-check ticks,
≤6,240 optimizer steps, ≤21,600 restoration ticks, **576 evaluation episodes** and
≤691,200 evaluation action ticks. Preflight admission adds 400 neutral setups,
training/development admissions add 480, and endpoint parity adds 576. Episode
reset/setup ticks are separate from action budgets; retain collector starts and
evaluation counts rather than hiding these costs in learning throughput.

Retain **396 new checkpoints**, 24 pilot/resume exports, six preflight exports,
390 raw/normalized advantage records, 384 update-delta records, all completed and
active training traces, all evaluation/geometry/parity outputs and timings. Freeze
**1,583 sources**, six configs, **119 copied inputs**, specification, runner,
unchanged native runtime/dependencies and all **46 prior manifest hashes** before
experiment execution. Sixty-nine Python tests cover geometry, unchanged default
sampling, strict validation, reset/continuation, family labels, optimization and
physical gates. Native evidence remains the unchanged 421-pass/two-ignored baseline.
Local retention is not off-machine backup. No commit, confirmation use or promotion
is implied by this experiment.

## Results

The first bearing-curriculum stage preserves all 48 holds but finishes at 2/48 stage arrivals versus 3/48 in the broad control; progression stops.

| Arm | Seed | Stage arrivals: initial → final /16 | Final left / right, each /8 | Broad recovery: initial → final /16 | Original immediate holds: initial → final /16 | Stage radius entries: initial → final /16 |
|---|---:|---|---|---|---|---|
| broad | 17 | 0 → 0 | 0 / 0 | 10 → 10 | 16 → 16 | 0 → 0 |
| curriculum | 17 | 0 → 0 | 0 / 0 | 10 → 10 | 16 → 16 | 0 → 4 |
| broad | 29 | 1 → 3 | 0 / 3 | 9 → 10 | 16 → 13 | 2 → 5 |
| curriculum | 29 | 1 → 2 | 0 / 2 | 9 → 12 | 16 → 16 | 2 → 3 |
| broad | 43 | 0 → 0 | 0 / 0 | 13 → 13 | 16 → 16 | 0 → 0 |
| curriculum | 43 | 0 → 0 | 0 / 0 | 13 → 12 | 16 → 16 | 0 → 0 |

Initial policies complete **1/48 stage cases**. After 64 updates, the curriculum
completes **2/48**, compared with **3/48** in the broad control. Per-seed curriculum
counts are **0/2/0 of sixteen**, versus **0/3/0** in the control. All completed stage
arrivals are rightward cases from seed 29: neither arm completes any of the 24
leftward cases. No seed passes stage competence or paired learning benefit, so
**no later stage is run**. This result does not establish that curricula cannot
work; this fixed first stage did not improve learning from these starts within
its declared budget.

The curriculum retains **48/48 immediate original holds**, compared with **45/48**
in the control. The control loses three holds in seed 29. All curriculum seeds
pass the scoped holding gate. Original broad recovery finishes at **34/48 versus
33/48**, from a common initial 32/48. That aggregate gain comes from other recovery
families: original position success falls to **1/12** in the curriculum, compared
with **2/12** in the control, all forward. Seed 43 loses its original forward
arrival and fails broad-recovery retention. Neither arm completes an original
sideways/backward development position case. There are zero physical failures in
all evaluation suites; unsuccessful cases reach their deadlines.

### Partial progress and settling failures

There is partial movement progress without competent arrival. The curriculum
enters the stage goal radius in **7/48 cases**, versus **5/48** in the control,
from a common initial 2/48. All final entries are rightward. Curriculum seed 17
adds four entries (indices 5/21/45/53), but **every inside-radius frame exceeds the
speed limit**. Their spin is within tolerance, yet none starts a valid dwell.
The four trajectories spend 227/18/62/211 frames inside the radius, respectively.
This is a measured translational braking/settling failure in those trajectories,
not simply an inability to reach the goal neighborhood.

Curriculum seed 29 has one additional failed radius entry, case 53, reaching only
**27 consecutive qualifying dwell intervals** before the deadline. The matching
broad-control case reaches 28. Broad seed 29's other failed entry, case 13, has
350 inside-radius frames, all speed-compliant, but alternating spin eligibility
prevents even one complete qualifying interval. These read-only classifications
use the retained physical traces; they add no new acceptance threshold or rollout.
The frozen thirty-interval arrival criterion remains unchanged.

### Training coverage and learning signals

Every one of the sixteen declared position training cases completes at least
once in each pilot. Thus this result includes the full declared position pool,
rather than only the first few candidates. Completed position episodes are
18/20/18 in the curriculum and 18/21/22 in the control. The curriculum records only
**four sampled position successes**, all rightward in seed 29, versus **ten** in
the broad control, all forward. Curriculum position-eligible rows are **0/437/0**,
compared with **0/361/442**. Aggregate sampled training successes across all families
are 760 versus 757; they do not demonstrate directional position competence.

Positive globally normalized position advantages total **8,003/49,152** in the
curriculum and **8,618/49,152** in the control. All 390 saved advantage records
reconstruct exactly, and goal-branch parameters change in every run. These are
learning-signal diagnostics, not correct-action labels or proof of useful updates.
Each arm has one KL early stop, both in seed 43. Pilot optimizer steps are **3,064**
for the curriculum and **3,065** for the broad arm; all 64 rollout/update boundaries
complete. Another 96 optimizer steps belong to the six exact restart checks.

| Arm | Seed | Sampled position successes | Position-eligible rows /16384 | Positive position advantages /16384 | Sum of goal-branch update norms | KL stops /64 |
|---|---:|---:|---:|---:|---:|---:|
| broad | 17 | 0 | 0 | 6524 | 0.152336 | 0 |
| curriculum | 17 | 0 | 0 | 5691 | 0.138574 | 0 |
| broad | 29 | 4 | 361 | 1555 | 0.120358 | 0 |
| curriculum | 29 | 4 | 437 | 2022 | 0.125672 | 0 |
| broad | 43 | 6 | 442 | 539 | 0.177296 | 1 |
| curriculum | 43 | 0 | 0 | 290 | 0.163471 | 1 |

Update norms are Euclidean norms of complete checkpoint-to-checkpoint changes,
summed across updates, not raw gradients or a competence metric.

### Verification and retention

All **69 Python tests** pass. All five procedural stage matrices pass their
400 admissions without replacements. Initial weights, exports, model function and
sampler RNG match the original policies. All original initial recovery/hold traces
reproduce exactly, and stage-initial traces match between arms. The broad arm's
complete checkpoints 0/1/16 reproduce LF-048 after only the two declared source
hashes are normalized for audit; actual resume remains strict.

All six **63→64 resumes** reproduce the full model, Adam, collector, RNG and history,
including update-64 advantage records. All **600 native/Python comparisons** pass.
Physical outcomes/dwell/rewards, 390 advantage records and 384 update-delta records
are verified. Native test evidence remains the unchanged retained **421-pass/
two-ignored** baseline; no new native test run is claimed.

All twenty declared processes exit zero without scored retries. Concurrent pilot
process times are **240.3–264.6 seconds**, not isolated throughput measurements.
Extra evaluation/parity processes take 82.8–95.0 seconds; the verifier takes 50.8.
Use **399,360 newly collected training ticks** (393,216 pilot plus 6,144 restart),
**8,766 restoration ticks**, **6,225 optimizer steps**, and **241,890 evaluation
action ticks across 576 episodes**, all within frozen limits. There are **3,934
neutral setup ticks**: 400 preflight admissions, 480 pilot admissions, 576 evaluation
resets, 576 parity resets, 1,850 pilot training resets, 24 resume restores and 28
resume terminal resets. These are separate from action/restore budgets.

Preserve 1,583 frozen sources, 119 copied inputs, six configs, unchanged runtime,
396 new checkpoints, 24 pilot/resume exports, six preflight exports, every physical
trace and diagnostic, tests, commands and timings. All 46 prior manifests and
copied input sources remain unchanged; sealing verifies source/config/runtime hashes.
The generic run `best.json` metadata is not used for stage advancement: the frozen
gate assesses update 64. See results (`docs/data/flight-bearing-curriculum-v1-results.json`, in git history)
and [archive receipt](data/flight-bearing-curriculum-v1-archive.json). Local retention
is not off-machine backup. No confirmation use, later-stage training or promotion.

### Next decision

Stop the curriculum at stage 0 and retain all five stage definitions as an
unqualified experimental recipe. The current gate fails; do not widen bearings,
relax the speed/dwell criteria or spend the remaining progression ceiling. Keep
the default task generator and production inference unchanged. Both the partial
rightward radius entries and their braking failures remain evidence, without
calling them successful directional navigation.

Next, freeze a separate **initialization comparison on this same stage-0 task
matrix**: fresh network initialization versus the preserved holding-policy warm
start, matched architecture, learner seeds, training tasks, rewards, optimizer,
exploration and budget. This tests whether prior holding specialization is a
limiting starting point; the present result does not establish that causal claim.
Require the warm-start control to reproduce this experiment, share sampler RNG
where initialization permits, and evaluate both initial baselines explicitly.
Fresh initialization is expected to change initial behavior, so do not claim
function-preserving parity between the two arms.

Keep the same independent physical arrival and original-hold requirements. Report
left/right outcomes, original broad recovery, full training-pool exposure and
failed settling separately. A fresh policy must earn final holding retention as
well as directional competence. Freeze its exact gates and budget before running;
no fresh-initialization experiment is implemented or executed in LF-051. If that
comparison also fails, revisit the low-level skill decomposition explicitly before
adding further curriculum stages. No native teacher, fallback or runtime handoff.

