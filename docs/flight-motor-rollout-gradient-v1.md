# Repeated single-speed rollout gradient diagnostic v1

LF-072, 2026-10-25. C4 audit 25; learning attempts remain 34.
Frozen before scored repeated rollouts. The user authorized testing whether
gradients consistently favor correction or flip under repeated sampling, while
keeping the stopping baseline protected.

## Fixed starts and independent repeats

Use all fifteen LF-069 starts previously inspected in LF-070/LF-071: learner
seeds 17/29/43, before updates 1/16/43/64/86. Each trial restores the identical
checkpoint actor/critic and full six-slot collector state, including physical
prefixes, active episodes, pool cursors, elapsed time and retention history.
Use a private deep copy of collector state per trial. Never mutate a parent.

Collect **sixteen independent 256-tick rollouts per start**, plus one original-RNG
reference replay. Noise seed = 720000000 + learner_seed*10000 + update*100 +
repeat, with repeat 0–15. The reference restores the saved Torch RNG and must
reproduce the entire original rollout and after-collector state exactly.
The 240 independent seeds are distinct and frozen. Trials do not continue each
other; policy weights remain fixed throughout.

Reuse the original collector, independent Gaussian actuator sampling, learned
deterministic steering, tasks, reward, gamma .99, lambda 1, 60 Hz physics/decisions
and reset-on-terminal behavior. Keep three movement and three stopping slots.
Record collisions and other failures without discarding or replacing trials.
Subsequent states, episode lengths/resets and ready-state exposure may diverge
between repeats; this is a full-rollout sampling diagnostic, not identical-state
action differentiation at every later tick.

## Gradients and fixed measurements

Compute raw-advantage and actual movement-pooled normalized actor gradients at
the unchanged before-policy, with the original .5 movement coefficient and
pooled movement denominator. Split them into six disjoint ready/unready ×
under/near/over regions as in LF-070. Readiness remains heading <=5 degrees,
spin <=.2 and lateral speed <=1; signed speed error is actual forward speed minus
command 16, with under <−1, near [−1,+1], over >+1. Missing or tiny gradients
remain explicit, not interpreted as agreement.

These are full-rollout policy-loss gradients **before any optimizer step**.
Check the behavior ratios are inside the PPO clip interval and region gradients
sum to the original movement loss gradient. There is no entropy/critic loss,
global clipping, minibatch/epoch path, Adam moment update or weight mutation in
this measurement. Thus it isolates the sampled actor signal; it does not predict
an actual complete PPO update, whose interference was audited in LF-070.

For each start, fix force Jacobians using the original rollout and before-policy:
LF-070's 29 probes (54-anchor mean panel, anchor 51, original saved regions and
target-speed copies) plus that start's LF-071 anchors, forty total across starts.
Project each repeat's six region gradients and combined movement gradient onto
these **same fixed states**. Positive force change corrects underspeed; negative
corrects overspeed. Target force and response-strength projections remain
separate. Gradient projections are first-order directional quantities, not
measured post-update force changes or a closed-loop controller.

Retain independent per-row local mean-logit credit, raw/pooled advantages, reward
components, region exposure, all parameter gradients and fixed Jacobians. Report
sixteen-repeat means, sample standard deviations, sign counts and missing counts;
compare raw versus pooled results and original-reference directions. Use 1e−6
as the descriptive projected-sign threshold and 1e−12 as the cosine norm floor.
Pairwise gradient cosines are correlated summaries, not independent trials.
Report results per start/seed, keeping seed 17's readiness gaps visible. Any
aggregate that averages repeats gives each rollout equal weight; do not silently
reweight sparse groups. Reference rollouts are excluded from repeat statistics.

## Protection and verification

All 255 rollouts must exactly reconstruct observations, original actions (within
float32 actuator tolerance), rewards and GAE targets from saved native traces.
Independently check lambda-1 discounted targets, and replay sampling from each
trial's saved starting RNG without rerunning physics. Verify gradient/projection
and credit tensors independently from the saved rollout. Original reference
projections must match the LF-070 full-rollout gradient measurements.

Check exact original parent logits/distributions on **195,840 stopping rows**,
zero stopping actor gradient, zero fixed zero-command Jacobians, unchanged model
weights and parent actor tensors. Fifteen existing native exports supply **8,140
native/Python comparisons** on fixed probes and real LF-071 anchors. No new export,
checkpoint, teacher/controller, fallback, rescue or handoff is introduced.
Fresh-noise stopping trajectories may differ; the stopping policy itself and all
original flight results, including failed seed 17, remain protected.

Ten focused tests pass on their first invocation: four new tests plus six
selected existing error-region tests. They check unique seeds, RNG/weight
preservation, source deep-copy isolation, same-seed exact repetition, distinct
noise trajectories, region/gradient accounting, fixed Jacobians and terminal
reconstruction. Native tests use **72 new ticks, 2,709 restore ticks and 21 setups**;
no test performs an optimizer update. Five model /48 environment tests remain
inherited evidence for the unchanged binary.

## Budget and retention

Three seed children, at most three concurrent, each <=900 internal /930 external
seconds; one verifier <=600/620. Stop dependent work on an assertion or budget
failure; retain partial evidence, with no automatic retry, reselection or extension.
There are **255 rollouts /391,680 diagnostic physics ticks**: 240 independent
rollouts /368,640 ticks and fifteen reference rollouts /23,040 ticks. Exact
prefix restoration adds **348,347 ticks**. Count six initial setups per trial
(1,530 total) plus every terminal reset separately. Verification adds no physics.
There are zero training ticks, optimizer steps, new model checkpoints or exports.
Collector-state snapshots are diagnostic evidence, not new trained checkpoints.

Freeze **1,649 source files and 264 copied inputs**, runtime, runner, complete
specification and all noise seeds before execution. No existing source changes;
add only the diagnostic and its tests. Keep full traces, completed/active collector
state, rollouts, RNGs, rewards, gradients, projections, results and logs. Preserve
all **67 earlier manifests** and verify full LF-069/LF-070/LF-071 archives before
capture and at seal. Require >=2 GiB free on the artifact disk at capture and a
512 MiB per-process running reserve.

Archive `data/training-experiments/learned-flight-motor-rollout-gradient-v1-20261025`
on `/home/data2/space-wars3-artifacts/training-experiments/` through the existing
checkout's archive symlink. No worktree. Local retention is not off-machine backup.
No confirmation use, policy selection/promotion or navigation qualification.
Append findings without changing this frozen specification.


## Results — gradient direction changes substantially with rollout noise

**All twenty original ready underspeed/overspeed groups that have fixed saved
probe states produce both corrective and counterproductive gradient directions
across sixteen independent repeats.** This holds for both the group's own pooled
gradient and the combined movement gradient. No such group has a consistent
direction across all repeats. The relevant source group is present in every
one of these comparisons; an absent group is not being counted as a sign flip.

All fifteen original-RNG references reproduce the complete original rollout and
after-collector state exactly. Their force projections match LF-070's original
full-rollout gradient measurements to maximum absolute difference **3.55e−15**.
The direction changes arise in the new noise realizations, not a changed policy,
measurement panel, collector initialization or numerical replay defect.

### Direction counts and averaged signals

For the two seeds with competent original stopping/heading baselines, 29/43:

| Fixed original group | Starts | Own gradient corrective /total repeats | Starts with corrective own mean | Combined gradient corrective /total repeats | Starts with corrective combined mean |
|---|---:|---:|---:|---:|---:|
| Ready underspeed | 10 | 107/160 | 8/10 | 96/160 | 6/10 |
| Ready overspeed | 6 | 61/96 | 5/6 | 51/96 | 4/6 |

Every start in this table has both signs. Each sixteen-repeat mean gives every
rollout equal weight. The means favor local correction in most groups, but there
are still noncorrective means, and combining state groups can weaken or reverse
an individually useful average. Sixteen repeats do not establish the population
gradient direction with certainty, especially where spread is much larger than
the mean. A positive mean also need not mean a majority of draws is positive.

Including seed 17, own-gradient means are corrective in 10/13 underspeed and 5/7
overspeed groups; combined means are corrective in 8/13 and 5/7. Its only original
ready-overspeed group contains **two rows in every repeat**, so report its high
variability without treating it as broad ready-state coverage. Starts without
original ready states have no corresponding saved-region Jacobian and are not
included in these twenty group comparisons; their fixed-panel measurements and
new region coverage remain in the analysis.

The same direction instability appears with **raw advantages**, before pooled
centering: all ten healthy-seed underspeed and all six overspeed groups still
produce both signs, and their own means are corrective in 8/10 and 5/6. The
independent per-row mean-logit credit also flips in every one of these sixteen
groups. Thus the variability is already present in sampled local credit, before
shared parameter gradients or optimizer history are introduced.

### Concrete examples

**Seed 43, before update 64, overspeed:** every repeat contains **433 ready-overspeed
rows**, exactly as the original rollout. Thirteen of sixteen own gradients favor
braking; three favor increased force. The original gradient was noncorrective.
Its corrective projection is **−.493**, versus a repeat mean **+1.115**, standard
deviation **1.524** and median **+.793**. The original deterministic-action
gradient has cosine **−.987** with the repeated mean gradient. These are gradient
projections, not actual applied force changes; no optimizer step was taken.

**Seed 29, before update 16, overspeed:** the state whose braking problem motivated
these probes remains difficult. Seven repeats are corrective and nine are not.
The own-gradient mean is **+.362**, but its median is **−.231** and standard
deviation **2.720**. The combined mean is **−.648**, still noncorrective. Ready
overspeed coverage ranges from 186 to 256 rows, versus 256 originally. This is
strong evidence of variability, not a clean demonstration that averaging alone
solves the update.

**Seed 29, before update 86, underspeed:** all sixteen repeats contain **768 ready-
underspeed rows and no other movement region**. Fourteen gradients favor more
force and two do not. The mean corrective projection is +2.348 with standard
deviation 2.806. Even a single-region rollout with unchanged region counts can
produce the wrong local direction. State values and sampled actions still differ;
constant counts do not mean identical full trajectories.

### Connection to the paired action/return probe

At all 32 LF-071 fixed anchors belonging to seeds 29/43, both own-region and
combined gradients produce both signs across these repeats. Own-gradient means
are corrective at **16/20 underspeed anchors and 10/12 overspeed anchors**;
combined means at **12/20 and 8/12**, respectively.

LF-071 showed that local corrective actions usually improve measured return and
that the current endpoint critic preserves all original-cutoff mean rankings.
LF-072 now shows that a single original sampled rollout can point against the
mean of fresh rollouts from the exact same start. Together these results support
sampling variance as a material contributor. They do not rule out continuation
tradeoffs, representation effects or state-group interference, and they do not
establish that a complete PPO/Adam update follows the full-rollout actor gradient.
Entropy, clipping, Adam momentum and minibatch sequencing were deliberately not
executed in this diagnostic.

## Validation, outcomes and cost

All four declared processes pass without source correction, retry, seed
replacement or budget extension. Sampling processes take **56.54 /80.86 /82.27
seconds internally**, 57.95 /82.09 /83.44 externally. Independent verification
takes **50.67 seconds internally /51.73 externally**.

All **391,680 sampling and reconstructed reward/target rows** verify; the
independent lambda-1 recurrence differs only by float32 rounding, at most
**1.15e−5**, inside the frozen tolerance. All gradients, credits, fixed Jacobians
and projected directions reproduce from retained evidence. Every collector start
is exact. All **195,840 stopping rows** retain exact original parent logits and
distributions, stopping actor gradients remain zero, and all **8,140 native/Python
comparisons** pass. Weights and outer Python/Torch RNG stay unchanged.

Actual diagnostic work:

- **240 independent rollouts /368,640 physics ticks**.
- **15 original-RNG reference rollouts /23,040 physics ticks**.
- **348,347 prefix-restoration ticks** and **4,079 neutral setups**, including
  2,549 terminal resets in addition to the 1,530 initial setups.
- Ten tests pass on the first invocation; their native checks add **72 new ticks,
  2,709 restore ticks and 21 setups**, with zero optimizer work.

Independent trials record **2,047 success /352 deadline episode terminations**;
references record **128 success /22 deadline terminations**. These are collector
episode outcomes across stopping and movement tasks, not a new qualification
evaluation. There are no collision, damage, ship-loss or boundary-escape outcomes
in this selected rollout window. Retain every deadline and all original prior
failures; no failure rule was relaxed.

There are **zero training ticks, optimizer steps, new model checkpoints or new
policy exports**. The 255 collector snapshots are diagnostic state evidence.
Keep all 255 rollouts, gradients/credit/RNG/trace evidence, fifteen fixed Jacobian
panels, 1,649 frozen sources and 264 copied inputs. Full LF-069/LF-070/LF-071
archives and all 67 prior manifests remain unchanged. The existing native binary
retains its inherited five model /48 environment test evidence. No production
source changed; only the new diagnostic and its tests were added.
See [results](data/flight-motor-rollout-gradient-v1-results.json),
[findings](data/flight-motor-rollout-gradient-v1-findings.json) and
[archive receipt](data/flight-motor-rollout-gradient-v1-archive.json).

## Decision and next step

The experiment answers the immediate question: these actor signals frequently
flip direction under repeated sampling from identical training starts. Larger
averages often favor useful correction, but remaining combined-gradient problems
make a guaranteed fix unwarranted.

The next proposed learning experiment is a bounded comparison of the current
effective actor batch with a larger batch that **averages gradients before each
optimizer step**. Simply collecting more rows while retaining the same small
optimizer minibatches would not directly test this mechanism. Freeze a matched
physics budget, the normalization/accumulation rule, number of optimizer steps
and compute limits before execution; report any unavoidable differences in update
frequency or data reuse. Keep initial policies, reward, architecture, sampling
distribution, all three seeds, protected stopping and strict final acquisition/
retention gates. Treat improved gradient consistency and actual flight competence
as separate outcomes. No such learning comparison has run here; no model is
promoted or qualified. Learning attempts remain 34 and C4 audits now total 25.
