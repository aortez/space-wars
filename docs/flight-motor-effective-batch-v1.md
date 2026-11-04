# Larger effective optimizer batches for speed holding v1

LF-073, 2026-10-25. C4 learning attempt 35; audit count remains 25.
Frozen before native preflight or scored training. The user authorized the
larger effective-batch comparison after LF-072 found frequent gradient sign
changes across independently sampled rollouts from identical saved starts.

## Question and bounded decision

Does averaging more sampled actor gradients before each optimizer step improve
speed-16 acquisition and sustained holding? Steadier gradient diagnostics alone
are not success. Compare two arms at seeds **17/29/43**, always initializing
from the original LF-053 stopping parents with fresh optimizer, collector and
RNG. Preserve their known failures, including seed 17. No warm start from a
selected later policy, fallback, rescue, confirmation use or model promotion.

| Setting | current | large |
|---|---:|---:|
| Persistent slots (movement / stopping) | 3 / 3 | 3 / 3 |
| Independent return segment length | 256 ticks | 256 ticks |
| Consecutive segments before updating | 1 | 4 |
| Rows collected at fixed behavior weights | 1,536 | 6,144 |
| Optimizer minibatch | 256 | 1,024 |
| Main collection/update cycles | 84 | 21 |
| Epochs over each collected row | 4 | 4 |
| Maximum Adam steps per main run | 2,016 | 504 |
| Physics ticks per main run | 129,024 | 129,024 |
| Movement / stopping ticks | 64,512 / 64,512 | 64,512 / 64,512 |

Choose 84 rather than LF-069's 86 segments to divide equally by four. The
current arm must exactly reproduce LF-069 at checkpoint 84, including model,
optimizer, full collector, RNG and update history (apart from contract metadata).
Both arms begin identically and their first 256-tick segment must be identical.
Large's four segments are sequential trajectories, not independent resets.

The existing vectorized loss averages each 1,024-row batch **before** one Adam
step; this is a true larger effective gradient batch, not four sequential
256-row steps. Separate movement/stopping advantage normalization covers the
whole current collection: 1,536 versus 6,144 rows. Each minibatch retains fixed
.5 group coefficients and its own group means. Thus collection/update frequency,
normalization statistics and optimizer-step count also change. Four epochs and
row reuse are matched, but four times fewer optimizer steps can slow adaptation;
this is not a pure noise-only ablation. Do not compensate with extra epochs,
learning-rate changes, additional training or an adaptive extension.

## Unchanged model, objective and physical criteria

Use the LF-069 command-16 tasks, pool partition/order and 104 held-out development
cases: 24 tracking starts (six each rest/slow/fast/reverse), plus 80 original
stopping/braking/heading cases. Six slots still alternate movement and stopping.
Maintain the protected format-2 residual (5,259 parameters, 3,493 trainable),
original actor/steering/log deviations, exact zero-command gate, deterministic
learned steering and independent Gaussian actuator exploration. Decisions and
physics remain 60 Hz.

Keep dense heading/velocity/spin rewards and original terminal terms, gamma .99,
lambda 1, Adam .0003 /epsilon 1e-5, PPO clip .2, entropy .001, value weight .5,
joint gradient clip .5, KL .03. Each 256-tick segment bootstraps independently
at its own end; concatenating four segments must not extend the return horizon.
Physical training failures terminate, incur their original penalty and remain
recorded under `record-physical-failures-v1`; never relabel them as success.

Evaluate only initial/final policies, never select intermediate checkpoints.
Acquisition requires heading error <=5 degrees, velocity-vector error <=1,
absolute spin <=.2, thirty consecutive qualifying intervals by tick 360;
retention must remain uninterrupted through tick 900. Physical failures have
priority. A seed's movement gate requires >=20/24 retained, >=4/6 in every mode,
gain >=12 over initialization, and no physical failure anywhere in development.
Joint qualification also requires all original stopping/braking/heading gates
and no lost baseline successes. Overall qualification requires two of three
seeds. Separately flag a meaningful comparative retention improvement only if
at least two seeds gain >=4 retained cases over current without increasing
physical failures. Report acquisition time, exits, endpoint errors and failures
even when both gates fail. These are development results, not confirmation or
navigation qualification.

## Preflight, verification and gradient evidence

Six preflights, one per arm/seed, each collect four segments: current four updates,
large one. Use LF-069's singleton preflight pools. Require all cases visited,
at least 512 movement samples above .05 throttle and 512 below .4 brake, and
at least 1,620 observed or explicitly forgone movement ticks after tick 360.
Retain failures. Exactly repeat each preflight's final update before main runs;
main runs restart from original parents. Exactly repeat each main final update.
Compare full rollouts, checkpoints, optimizer, collector, RNG and metrics.

Inspect every completed/active training trace and every evaluation trace.
Verify all behavior likelihoods, steering and stopping logits/distributions,
parent actor weights, task schedules and declared physics totals. Require at
least three visits per movement case and seven per stopping case in main;
report all unequal/failure-dependent exposure rather than hiding it. Verify
segmented GAE and independent lambda-1 targets at every preflight update and
main current updates **1/16/44/64/84**, large **1/4/11/16/21**. Except the first,
these selected endpoints have matched physics exposure. Compare full legacy
traces to originals and every final zero-command trace to its initial trace.

Use the existing 54-anchor force panel at commands 0/16 and actual speed offsets
-6/-.25/0/+.25/+6, initially/finally in both arms. Require native/Python parity
and exact zero-command outputs. At the five declared large-arm checkpoints,
inspect each saved collection at unchanged before-update weights. Use one fixed
permutation (seed `730000 + learner_seed*100 + update`) partitioned into 24
256-row and six 1,024-row groups, plus the full 6,144-row gradient. Normalize
advantages once over the saved collection, preserve each group's actual loss
mean, and record full actor gradients, movement counts, cosine to full gradient,
and projections on mean fixed-anchor force at actual speeds 10/16/22. Report
means, dispersion and corrective signs. Correlated partitions are descriptive,
not independent training seeds or proof of population-gradient quality. Exclude
entropy, critic, clipping and Adam from this algebraic diagnostic; do not equate
it with applied parameter changes or flight performance.

## Budgets, provenance and stopping rule

Twenty-seven child processes: six preflights, six preflight repeats, preflight
verifier, six main runs, six main repeats, final verifier and algebraic analyzer.
At most three run concurrently. Preflight internal/external limits 180/200 seconds;
main/resume 900/920; each verifier and analyzer 600/620. Stop dependent work on
any failed assertion; preserve partial evidence, with no silent retry, replacement
seed, source patch after freeze or budget extension.

Main training: **774,144 ticks** and at most **7,560 Adam steps** across six runs.
Including preflights and exact repeats: **857,088 training ticks** and at most
**8,208 optimizer steps**. Restore budget 48,000 physics ticks. Evaluation:
1,248 episodes and at most 1,123,200 ticks. Count neutral setups separately.
Algebraic analysis adds no physics or optimizer steps. The 20-test Python suite
uses synthetic collectors and adds no native physics; retain all attempts. Two
pre-freeze test errors were corrected: unsupported state retrieval in a synthetic
fixture, and a missing zero-entropy configuration field in gradient analysis.
Unchanged native code inherits five model and 48 environment tests.

Archive all source/config/task/runtime inventories, original inputs and current
control-reference checkpoints, rollouts, models/exports, collector/RNG state,
full/partial trajectories, commands, process status, failures and analysis on
the artifact disk as `learned-flight-motor-effective-batch-v1-20261025`. Require
2 GiB free at freeze and retain a 512 MiB reserve. Verify all 68 previous manifests
and the complete LF-069 and LF-072 archives before and after. No original baseline
file is modified. The archive is local retention, not off-machine backup.

If larger batches merely steady gradients while failing to improve strict flight
performance, reassess the learning approach and model/objective fit before another
small parameter sweep. Append outcomes below; do not rewrite this frozen plan.


## Results — complete; larger batches do not improve retained speed holding

The [larger effective-batch comparison](flight-motor-effective-batch-v1.md)
finishes with **0/24 retained speed-16 cases in both arms for every seed**.
Physics exposure is matched at 129,024 ticks per run: current uses 84 collections
and 256-row optimizer batches; large uses 21 collections of four independently
bootstrapped 256-tick segments and 1,024-row optimizer batches. Large averages
before Adam but makes one-quarter as many steps; normalization scope and policy
refresh frequency also change. This bounded recipe fails the flight improvement
and qualification gates; it does not rule out all gradient-averaging designs.

For seeds 29/43, large brakes all six fast starts into tolerance by tick 360,
then continues below the target and loses retention. Their representative exits
are ticks 409/386, and rest endpoints are only 8.437/7.423 versus command 16.
Fast-start collisions fall from 3/6 to zero for these seeds, but large seed 17
has four collisions versus current's zero. On the same saved large-arm samples, larger optimizer-sized partitions roughly
halve fixed-force projection dispersion. For seeds 29/43, partitions opposing
the full gradient fall from 86/240 to 15/60, yet corrective signs remain mixed.
This is within-collection smoothing, not improved flight competence.

All 27 bounded processes pass, including twelve exact resumptions. Current
reproduces LF-069's checkpoint 84 exactly except experiment contract metadata;
both arms have identical initial states and first segments. All 480 final
zero-command traces equal their initial traces, 672 complete legacy traces
match originals, and stopping outputs/distributions stay exact. Seeds 29/43
retain 80/80 zero-command cases; seed 17's original limitations stay recorded.
Retain all evidence and 68 earlier manifests. C4 now has **35 learning attempts
and 25 audits**. No confirmation use, promotion or navigation qualification.

| Seed | Arm | Final retained | Rest-start mean speed at tick 900 | Development physical failures | Main Adam steps |
|---|---|---:|---:|---:|---:|
| 17 | current | 0/24 | 15.164 | 0 | 2016 |
| 17 | large | 0/24 | 15.092 | 4 | 504 |
| 29 | current | 0/24 | 12.376 | 3 | 2016 |
| 29 | large | 0/24 | 8.437 | 0 | 504 |
| 43 | current | 0/24 | 19.547 | 6 | 2016 |
| 43 | large | 0/24 | 7.423 | 0 | 504 |

All initial policies also retain 0/24 tracking cases. All main runs reach their
frozen endpoint with no KL early stopping. There are no post-freeze source
changes, training retries, replacement seeds or extensions.

### Acquisition and overshoot

Large seed 29's representative fast start first qualifies at tick **359** and
exits at **409**; seed 43 qualifies at **337** and exits at **386**. All six fast
starts per seed acquire on time and lose retention. Their representative final
speeds are **8.509 /7.588**. Rest, slow and reverse starts never acquire; their
endpoints are around 8.44 for seed 29 and 7.42 for seed 43. Improved braking from
above target has not become a centered velocity controller.

Current seed 29 never acquires in the final panel. Current seed 43 acquires all
18 rest/slow/reverse cases, but all later leave tolerance: representative
acquisition/exit pairs are **455/571**, **334/450**, and **571/687**. Slow starts
acquire on time but still fail retention. Current fast starts collide in 3/6
cases for seed 29 and 6/6 for seed 43; large avoids these collisions. Preserve
all four large-seed-17 collisions as failures. Seed 17's inherited heading
limitation prevents treating its near-target forward speed as tracking success.

The six held-out cases per mode differ in absolute headings/world starts while
having similar normalized aligned states. They are not independent training
seeds or evidence of generalization to navigation. Endpoint means above use
survivors at tick 900; post-collision velocities are not equilibria.

### Fixed-state control response

Mean instantaneous native control term on the unchanged 54-anchor command-16
panel (not measured acceleration in a new rollout):

| Seed | Arm | Force at speed 10 | Force at target 16 | Force at speed 22 | Local force slope |
|---|---|---:|---:|---:|---:|
| 17 | current | +1.319 | -0.138 | -1.466 | -0.2428 |
| 17 | large | +0.679 | +0.044 | -0.494 | -0.0984 |
| 29 | current | +0.467 | +0.060 | -0.354 | -0.0700 |
| 29 | large | -0.948 | -1.079 | -1.151 | -0.0161 |
| 43 | current | +1.487 | +0.785 | -0.057 | -0.1359 |
| 43 | large | -1.439 | -1.469 | -1.481 | -0.0034 |

Large seeds 29/43 apply negative force even at speed 10 on these fixed anchors
(mean -.948 /-1.439) and retain substantial negative target bias (-1.079 /-1.469).
Their mean force slopes are weak: -.0161 /-.0034 per speed unit versus current's
-.0700 /-.1359. Both large policies have a noncorrective mean brake slope;
large seed 43 has corrective net force slope at only 36/54 anchors. The large
batch recipe has not learned stronger, appropriately balanced feedback.

These fixed states preserve other inputs and previous actions. Their response
strength and target bias help describe the failure, but the closed-loop traces
establish the actual flight result. Exact zero-command outputs remain protected.

### Gradient dispersion versus flight competence

Use the same five saved large-arm collections per seed, unchanged behavior
weights, shared pooled advantage normalization and one fixed permutation per
collection. Compare 24 small partitions against six large partitions. Below,
negative cosine counts mean a minibatch gradient opposes the full collection's
gradient. Dispersion ratios are median large/small standard deviations of
fixed-force projections at speeds 10/16/22, over five saved collections:

| Seed | Opposite full gradient, 256 rows | Opposite full gradient, 1,024 rows | Projection dispersion ratio: under / target / over |
|---|---:|---:|---:|
| 17 | 47/120 | 11/30 | 0.473 / 0.517 / 0.512 |
| 29 | 45/120 | 9/30 | 0.476 / 0.473 / 0.471 |
| 43 | 41/120 | 6/30 | 0.429 / 0.422 / 0.414 |

Across all fifteen saved collections, projection dispersion is smaller for
large partitions at all three measured speeds. For healthy baseline seeds
29/43, median dispersion ratios are .438 /.434 /.431. Opposite-full-gradient
fractions decrease from 86/240 (35.8%) to 15/60 (25%). However, corrective
underspeed projections only change from 131/240 to 34/60; corrective overspeed
projections change from 110/240 to 26/60. Consistency with a sampled full
collection is distinct from a useful corrective direction.

This within-collection analysis does not repeat LF-072's independent trajectory
sampling or estimate the population gradient. It holds normalization and sampled
rows fixed while changing the grouping. Each minibatch uses its own movement
denominator, so its unweighted mean need not exactly equal the full gradient.
The retained evidence includes full gradients, counts, individual projections,
cosines and fixed Jacobians. Entropy, critic, clipping, Adam and applied policy
changes are excluded. Smoother partitions do not demonstrate better steering,
correct force balance, or improved final retention.

### Training outcomes, verification and retention

Completed training episodes, excluding partial active traces:

| Seed | Arm | Movement outcomes | Stopping outcomes |
|---|---|---|---|
| 17 | current | `{"collision": 6, "deadline": 64}` | `{"deadline": 177}` |
| 29 | current | `{"collision": 18, "deadline": 55}` | `{"success": 982}` |
| 43 | current | `{"collision": 16, "deadline": 57}` | `{"success": 972}` |
| 17 | large | `{"collision": 14, "deadline": 58}` | `{"deadline": 177}` |
| 29 | large | `{"collision": 1, "deadline": 68}` | `{"success": 982}` |
| 43 | large | `{"deadline": 69}` | `{"success": 972}` |

Both preflight seed-17 arms retain one collision. All failure penalties and
terminal semantics verify. Inspect every completed and active training trace,
all 1,248 evaluation episodes, and all saved behavior likelihoods. Strict
movement and joint gates fail for both arms; the comparative retention gate
also fails.

Verification passes **811,008 likelihoods**, **405,504
protected stopping rows**, **152,064 reconstructed GAE /independent
lambda-1 targets**, and **7,728 native/Python inference
comparisons**. All 672 complete original legacy traces, 480 initial/final
zero-command traces and 1,620 zero-command feedback rows match. Twelve exact
resumptions reproduce complete model, optimizer, collector, rollout, RNG and
metrics. Both main control endpoints and preflights reproduce LF-069. Final
verification takes 223.51 seconds externally; the algebraic
analyzer takes 6.88. All 27 processes remain within their caps.

Actual native work, including preflight and exact repeats:

- **857,088 training physics ticks**, of which 774,144 are main training.
- **8,208 Adam steps**.
- **18,093 restoration physics ticks**.
- **1,120,534 evaluation physics ticks**, across 1,248 episodes.
- **6,631 neutral setup ticks**.

The final 20-test suite passes after the two documented pre-freeze errors.
All three test invocations are retained; together they execute 144 synthetic
optimizer steps and no native physics. No Rust source changes; inherit the five
model and 48 environment tests for the unchanged binary.

Retain 354 checkpoints, 342 rollouts, 48 inference exports, 5,239 completed
training traces plus all partial traces, fifteen gradient evidence bundles,
1,654 frozen source files and 183 copied inputs. All 68 earlier manifests and
complete LF-069/LF-072 archives remain unchanged. See
[results](data/flight-motor-effective-batch-v1-results.json),
[findings](data/flight-motor-effective-batch-v1-findings.json), and
[archive receipt](data/flight-motor-effective-batch-v1-archive.json).

## Interpretation and next decision

This recipe does not deliver competent speed holding. It trades some excessive
speed and collisions for excessive slowing, with no retention gain. The matched
physics comparison deliberately includes fewer optimizer steps, less frequent
policy refresh and broader normalization; it cannot establish that gradient
averaging itself is useless or that the architecture cannot represent control.
Those limits are recorded rather than used to justify an automatic extension.

Reassess the training approach before another PPO parameter sweep. Discuss a
bounded supervised feasibility test using simulator-derived corrective targets:
can the existing protected network fit an appropriately centered, sufficiently
strong response across under/at/overspeed states, and then converge in closed-loop
flight? Target generation must be independent of the native controller. Choose
and freeze that method, coverage and physics budget before executing it; a good
static fit alone will not qualify flight. Preserve all three seeds, the original
stopping function, strict physical gates and complete evidence. No follow-up
training, automatic extension, confirmation use or model promotion has run.
