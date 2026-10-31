# Protected command-balanced collection comparison v1

LF-067, 2026-10-25. C4 learning attempt 33; audit count remains 21.
Frozen before preflight/main execution. The user authorized the paired
command-balanced training test following LF-066's saved-update audit.

## Hypothesis and matched schedules

LF-066 found incorrect speed balance and weak high-speed feedback despite useful
local corrections. Changes from updates without a requested speed often undo
changes made while that speed is represented. Test whether representing all
training command speeds in every PPO rollout improves retained speed control.
This is a schedule hypothesis, not proof that batch composition is the sole cause.

Use two **six-slot** schedules, both with **three movement /three stopping**
environments and **256 ticks per slot per update**:

- **cyclic:** assign the unchanged 48 ordered candidates by `rows[i::6]`. Each
  movement slot cycles through its eight cases, including all three speeds.
  Initial movement slots all request speed 4; later commands depend on episode
  completions. This retains the old round-robin scheduling rule at six slots.
- **balanced:** dedicate movement slots 0/2/4 to commands **4/10/16**, respectively,
  cycling their original eight cases in unchanged order. Stopping slots 1/3/5
  use **exactly the cyclic arm's stopping pools**. Every rollout must contain
  **256 movement rows per speed** and **768 stopping rows**.

Each candidate appears in exactly one pool, once per pool cycle. Task definitions,
initial-speed modes, headings and case indices are unchanged. The intervention is
slot assignment/temporal task mixture; failure-dependent case progression and
total per-command exposure may differ as consequences. Report actual exposure.
PPO minibatches remain uniformly shuffled rather than explicitly stratified.

The previous four-slot collector could not simultaneously represent all three
movement commands while preserving equal stopping exposure. Both new arms use
the same larger collection shape and **86 updates**, giving **132,096 training
rows and at most 2,064 optimizer steps per main run**, versus LF-065's
131,072/2,048. The six-slot cyclic arm is a **new matched control**, not an exact
reproduction of LF-065. Compare within LF-067; do not attribute a cross-experiment
difference solely to scheduling. The 256-tick GAE horizon and 256-row minibatch
size remain unchanged. All 48 training cases must complete at least three
movement visits or seven stopping visits per main run.

## Protected model, objective and execution

Both arms start from the original LF-053 parents at seeds **17/29/43** with fresh
optimizer, collector and RNG. No LF-065 final policy is a warm start. Require
exact paired initial model, optimizer and RNG; main collectors intentionally
differ. Use the existing format-2 protected residual, **5,259 parameters /3,493
trainable**, frozen original actor/steering/log deviations, and requested-speed
gate that is exactly zero for stopping. No native guidance or runtime handoff.

Keep **lambda 1, gamma .99**, the same dense heading/velocity/spin reward,
terminal success/failure terms, independent Gaussian actuator sampling and 60 Hz
controls/physics. Keep Adam .0003, four epochs, minibatches 256, clip .2, entropy
.001, value coefficient .5, joint gradient clip .5 and KL threshold .03. Keep
separate movement/stopping advantage normalization and fixed .5 group coefficients.
There is no new per-command reward, centering, loss weighting or scripted control.

Positive-speed training continues to tick 900 and requires acquisition by 360
followed by uninterrupted retention. The original stopping acquisition task is
unchanged. Use `record-physical-failures-v1`: physical failures terminate, remain
in the evidence and never count as success. Do not extend a run after failure or
choose an intermediate checkpoint for evaluation.

## Physical qualification and diagnostics

Use the same **104 development cases** initially/finally: 24 tracking at unseen
commands 6/12/18, 24 aligned stopping, 24 original braking and 32 original
turning/holding. Physical gates remain heading <=5 degrees, velocity-vector error
<=1, spin <=.2, thirty complete qualifying consecutive intervals, acquisition by
360 and uninterrupted retention through 900. Physical failure has priority.

Movement qualification requires >=20/24 tracking, >=6/8 at every command,
>=4/6 in every initial-speed mode, gain >=12 over initialization and no physical
failure. The full stage also requires all original stopping/braking/heading gates
with no lost parent successes. Overall success requires **two balanced seeds**
to qualify. Separately, paired improvement requires **at least four additional
retained cases in two seeds** versus cyclic. No confirmation use, model promotion
or navigation claim follows automatically.

Retain the existing **54-anchor** initial/final response panel, commands 6/12/18
and actual-minus-command offsets −6/−.25/0/+.25/+6. Report corrective slope signs
and algebraic acceleration alongside full physical outcomes, not as replacement
qualification. Require **9,720** native/Python feedback comparisons and **2,430**
exact paired initial outputs, plus **1,248** evaluation inference comparisons.

## Preflight, reconstruction and regression checks

Both schedules' four-update preflights use the same six singleton pools at case
indices **0/1/16/17/32/33**, including all three movement speeds. Require exact
paired preflight first rollouts and final states, excluding contract metadata.
These test six-slot execution; scheduler differences are separately tested and
verified on every main rollout. Require a completed visit to every preflight
case, >=512 movement actions above .05 throttle and >=512 below .4 brake, and
>=1,620 continuation ticks beyond 360 including explicit forgone physical-failure
time. Repeat the last preflight update exactly from checkpoint 3 before main
training. Main runs restart from original parents, not preflight weights.

Repeat each main final update exactly from checkpoint 85. Require **12 exact
resumes**, **312 paired initial development traces**, **672 complete original
reference traces**, and **480 before/after zero-command traces**, preserving
seed 17's original failures as well as passing baselines.

Reconstruct observations/rewards/terminal cuts from native training traces and
recompute saved GAE exactly at all preflight updates and main updates
**1/16/43/86**: **73,728 rows**. Independently check all 73,728 lambda-1 returns
against the discounted-reward recurrence with live endpoint bootstrap (1e−4
absolute /1e−5 relative tolerance). Verify pool/slot identities, per-command
counts, original frozen actor/steering, zero-command deviations and saved
likelihoods. There is no inference from rewards alone about physical competence.

The collector gains optional explicit pools and derives slot count from its
saved pools; its default stays four. The protected runner accepts stage pools
and rejects incompatible resumes. Existing verification helpers derive row
counts and target checkpoints from the declared stage. The feedback helper uses
the stage's final update. Native source/runtime and network structure are unchanged.

Validation covers **29 Python tests**: nine new scheduler/collector/regression
checks, nine GAE tests, seven movement tests and four failure-policy tests.
The first invocation has 28 passes and one test-fixture `KeyError` from removing
the arm key twice. Fix the assertion and rerun that test successfully; retain both
logs. The physical regression exactly reproduces LF-065 lambda-1 seed 43's last
four-slot rollout, model, optimizer, collector, RNG and metrics: **1,024 training
ticks, 16 optimizer steps, 714 restoration ticks and nine neutral setups**, counted
separately from the scored experiment. Its complete evidence is retained.
Synthetic checks add no physics; the unchanged five native model /48 environment
tests remain inherited evidence.

## Budgets and retention

**26 processes**, at most three concurrent. Preflight/resume caps are
180 internal /200 external seconds; main/resume 900/920; verifiers 600/620.
Maximum experiment totals including repeated updates: **847,872 training ticks,
13,248 optimizer steps, 48,000 restoration ticks, 1,248 evaluation episodes /
1,123,200 scored ticks**, **10,968 inference comparisons**, **829,440 likelihood
checks**, **414,720 protected stopping rows**, **564 frozen-parent checkpoint
checks** and **73,728 reconstructed targets**. Main movement/stopping actions
are **396,288 each**. Add the separately reported physical regression cost above;
count neutral setup ticks separately. Stop dependent work on implementation or
accounting failure and retain partial evidence; no automatic retry or extension.

Freeze source/config/task/specification/input hashes before preflight. Retain
**564 checkpoints, 552 rollouts, 48 exports**, all traces, failures, scheduler
counts, targets, response outputs, commands, tests and logs. Freeze **1,635 source
files**, including LF-066's two already-declared metadata corrections; the five
shared helper changes and four new experiment modules are identified explicitly.
Preserve **180 copied inputs** and all **62 previous manifests**. Require >=2 GiB
free before freeze and maintain a 512 MiB per-run reserve on the artifact disk.

Archive `data/training-experiments/learned-flight-motor-command-balance-v1-20261025`,
linked to `/home/data2/space-wars3-artifacts/training-experiments/learned-flight-motor-command-balance-v1-20261025`.
Use the existing standalone checkout. Local retention is not off-machine backup.


## Results

**Command-balanced collection does not improve retained speed tracking in this
paired experiment. Original stopping remains exactly preserved.** All six main
runs finish **86 updates /2,064 optimizer steps**, with no KL early stop.

| Arm | Seed | Initial tracking /24 | Final tracking /24 | Original zero-command /80 | Final collisions |
|---|---|---|---|---|---|
| Cyclic | 17 | 0 | 0 | 0 | 1 |
| Cyclic | 29 | 8 | 8 | 80 | 1 |
| Cyclic | 43 | 0 | 0 | 80 | 6 |
| Balanced | 17 | 0 | 0 | 0 | 0 |
| Balanced | 29 | 8 | 8 | 80 | 3 |
| Balanced | 43 | 0 | 0 | 80 | 0 |

All eight successes in seed 29 request speed **6** and were present at
initialization. Neither arm learns any additional development tracking case.
No speed-12 or speed-18 trial passes. Paired gains are **0/0/0**; both the strict
stage gate and the separately declared two-seed improvement gate fail.

Seeds 29/43 preserve all **80/80** original stopping/braking/turning cases in both
arms. Every original zero-command trace remains exact, including seed 17's
original failures. All final collisions are movement cases. Balancing eliminates
seed 43's six final collisions while increasing seed 29's from one to three;
there is no uniform safety or competence improvement.

### The collection intervention was actually delivered

Every balanced update contains exactly **256 rows at each movement speed** and
**768 stopping rows**. No cyclic update contains all three commands. Total main
movement exposure is **66,048 rows per run** in both arms:

| Arm /seed | Speed-4 rows | Speed-10 rows | Speed-16 rows | Updates with all commands /86 |
|---|---|---|---|---|
| Cyclic /17 | 23,168 | 21,600 | 21,280 | 0 |
| Cyclic /29 | 23,539 | 21,600 | 20,909 | 0 |
| Cyclic /43 | 23,921 | 21,600 | 20,527 | 0 |
| Balanced /17 | 22,016 | 22,016 | 22,016 | 86 |
| Balanced /29 | 22,016 | 22,016 | 22,016 | 86 |
| Balanced /43 | 22,016 | 22,016 | 22,016 | 86 |

The schedules use the same 48 physical tasks, and every main run meets the
minimum three movement/seven stopping completed visits per case. Per-command
cyclic totals differ slightly because of pool order and failure-dependent resets;
balanced totals are exactly 22,016 each. This is a test of the declared slot
assignment and temporal mixture, not a claim of perfectly equal per-command
sample counts across arms. The stopping pools are identical.

Both arms use the same six-slot shape, 256-tick GAE horizon and optimization
budget. LF-065's four-slot result is historical context only: this cyclic arm is
a new matched control. Its different seed-43 result cannot be assigned to the
balanced intervention or to lambda alone.

### Failure modes shift between overshoot and undershoot

Forward speed at **tick 900** in fixed rest-start cases 0/16/32:

| Arm /seed | Request 6 | Request 12 | Request 18 |
|---|---|---|---|
| Cyclic /17 | 6.684 | 8.499 | 9.784 |
| Cyclic /29 | 6.844 | 9.175 | 13.966 |
| Cyclic /43 | 7.646 | 17.182 | 30.160 |
| Balanced /17 | 5.662 | 7.101 | 8.005 |
| Balanced /29 | 6.869 | 9.772 | 26.075 |
| Balanced /43 | 4.018 | 5.346 | 6.123 |

Balanced seed 43 stays below all three commands and never acquires the band in
these rest cases. Its command-18 endpoint is **6.123**, compared with **30.160**
in cyclic. This removes overshoot in that trial by producing much less movement,
without yielding tracking success. The cyclic request-6 rest case acquires at
tick **139**, then exits at **256**. Its request-12/18 rest cases first acquire
at **411/483**, already late, then exit at **562/521**.

Balanced seed 29 retains the low-speed cases but overshoots at request 18: it
first acquires at **588** and exits at **614**, ending at **26.075**. Cyclic seed
29 instead ends at **13.966** and never acquires that rest trial. Neither is
successful control. Endpoints are measurements, not assumed equilibria.

Seed 17 retains its original steering limitation; balanced rest cases end with
roughly 18–19 degrees of heading error. The stopping protection intentionally
preserves this failed parent as evidence as well as preserving the passing seeds.

### Balanced coverage does not guarantee corrective feedback

At request **18**, the fixed 54-anchor panel gives the following final responses.
Wrong throttle slope means throttle rises with actual speed; wrong brake slope
means brake falls with speed. Acceleration is the unchanged native control-force
estimate, in units/s², with all other inputs held fixed consistently.

| Arm | Seed | Wrong throttle slope /54 | Wrong brake slope /54 | Mean acceleration at 18 | Mean acceleration at 24 |
|---|---|---|---|---|---|
| Cyclic | 17 | 0 | 0 | -0.593 | -0.854 |
| Cyclic | 29 | 0 | 0 | -0.213 | -0.836 |
| Cyclic | 43 | 0 | 54 | +1.601 | +1.651 |
| Balanced | 17 | 0 | 0 | -1.886 | -2.035 |
| Balanced | 29 | 54 | 54 | +1.485 | +1.678 |
| Balanced | 43 | 0 | 54 | -2.079 | -1.669 |

Balanced seed 29 exhibits both wrong signs at **all 54 anchors at each request
6/12/18**. At the previously declared anchor 51 for request 18, acceleration
rises **+1.165 →+1.482 →+1.712** as actual speed rises **12 →18 →24**. Its cyclic
counterpart gives **+.482 →−.253 →−.863**, a corrective direction on this panel.

Balanced seed 43 decreases throttle with rising speed, but also decreases brake
at all 54 anchors. At anchor 51, request-18 acceleration is
**−2.405/−1.939/−1.517** at actual speeds 12/18/24. Braking weakens with speed
while remaining too strong near the target. This is consistent with low-speed
settling and inadequate learned regulation; it is not a new physical rollout.
The cyclic seed-43 response also fails: acceleration stays positive and rises
across the same three speeds. Lambda 1 does not guarantee the earlier LF-065
feedback improvement under different collection/update grouping.

`feedback.json` and `feedback-summary.json` retain every prescribed anchor,
command, phase and seed. `analysis.json` and `trajectory-details.json` retain
the full physical breakdown rather than selecting only these rest examples.

### Training outcomes and state coverage

Completed movement training episodes:

| Arm /seed | Training success | Deadline | Collision |
|---|---|---|---|
| Cyclic /17 | 0 | 70 | 2 |
| Cyclic /29 | 5 | 63 | 5 |
| Cyclic /43 | 10 | 59 | 4 |
| Balanced /17 | 0 | 70 | 2 |
| Balanced /29 | 1 | 67 | 5 |
| Balanced /43 | 0 | 72 | 0 |

Incomplete active tails remain in exposure counts but are excluded from episode
outcomes. Stopping outcomes match across arms: **183 deadlines** for seed 17,
**1,006 successes** for seed 29 and **995 successes** for seed 43. Physical
training failures remain recorded and do not count toward qualification.

Post-run descriptive coverage over all **792,576** main rollout rows uses the
same readiness screen as LF-065/066: heading <=5 degrees, spin <=.2 and lateral
speed <=1. Along-command speed error below −1 is under, above +1 over, and
[-1,+1] near. Near-speed rows are not equivalent to the full physical gate.

| Arm /seed | Ready underspeed | Ready near speed | Ready overspeed |
|---|---|---|---|
| Cyclic /17 | 942 | 0 | 36 |
| Cyclic /29 | 28,087 | 16,185 | 19,438 |
| Cyclic /43 | 36,857 | 19,383 | 8,056 |
| Balanced /17 | 921 | 0 | 36 |
| Balanced /29 | 31,762 | 13,452 | 18,537 |
| Balanced /43 | 63,211 | 1,954 | 872 |

For seed 43, ready near-speed exposure falls from **19,383 to 1,954** rows under
balancing; at training command 16, it falls from **603 to 126**. All commands
are represented throughout, but useful near-target states become much less
common in this seed. Seed 29's near-speed rows also decrease, while its final
tracking count stays unchanged. These are observed consequences of training,
not independent proof of a sole credit, architecture or reward defect.

### Validation, cost and retained evidence

All **26 child processes** exit zero within their declared caps. Six paired
preflights and their last-update resumes pass, including exact initial rollouts
and final states across arms. Main runs start fresh from the original parents;
their final-update resumes also match exactly. Main internal wall times are
**97.66–238.33 seconds**; final verification takes **196.71 internally /197.79
externally**. These are observed wall times, not a controlled throughput claim.

The outer execution session reports **exit 143** when polled after the runner's
completion marker. Its cause is not determined from the available tool output.
All 26 recorded children, final results and verification are complete; preserve
the outer-session status separately. No child run is retried, no experiment
budget is extended and no frozen source is changed after execution begins.

Actual experiment work including preflights/resumes: **847,872 training ticks,
13,248 optimizer steps, 21,412 restoration ticks, 1,120,396 scored evaluation
ticks across 1,248 episodes, and 6,579 neutral setup ticks**. The separate
four-slot regression adds **1,024 training ticks, 16 optimizer steps, 714
restoration ticks and nine setups**. Thus training including this regression
is 848,896 ticks; do not hide test work in the scored comparison.

Verification passes:

- **29 distinct Python tests**, with the pre-freeze assertion-fixture failure
  and successful focused correction retained. The four-slot physical regression
  exactly reproduces the saved LF-065 rollout, model, optimizer, collector and
  RNG. Five native model/48 environment tests remain inherited evidence for
  the unchanged runtime.
- **12 exact resumes**, twelve original-parent initializations, three exact
  paired preflight first rollouts/final states, **312 paired initial development
  traces** and **2,430 paired initial feedback outputs**.
- **73,728 exact reconstructed GAE rows** and independent lambda-1 return
  checks; the largest observed recurrence rounding difference is **5.25e−6**.
- **10,968 native/Python comparisons**, **829,440 saved likelihood checks**,
  **414,720 protected stopping actor/distribution rows** and **564 frozen-parent
  checkpoint checks**.
- **672 complete original-reference traces** and **480 before/after zero-command
  traces** compare exactly. Every balanced update has the declared speed counts.

Retain **564 checkpoints, 552 rollouts, 48 exports, 1,635 frozen sources and 180
copied inputs**, all native traces, failures, target/scheduler checks, feedback,
analyses, source/runtime, tests and logs. All **62 earlier manifests** and copied
input origins remain unchanged. The original frozen specification is retained
separately from this completed report. No confirmation use, model promotion or
navigation qualification. See [results](data/flight-motor-command-balance-v1-results.json)
and [archive receipt](data/flight-motor-command-balance-v1-archive.json).

## Interpretation and next decision

This comparison does not support the expectation that the declared command-balanced
schedule, by itself, improves retained tracking under this recipe and budget.
It fixes missing command coverage within a rollout, but feedback and calibration
still fail in different ways across seeds. It does not rule out all scheduling
choices or establish one universal failure mechanism. Original stopping remains
protected throughout, so these are movement-learning failures rather than losses
of the original zero-command controller.

Freeze a bounded saved-gradient audit of the command-balanced runs and their
cyclic controls before another training change. At declared checkpoints and
saved rollout states, separate the movement actor gradient by requested speed;
measure alignment/conflict and each group's projected effect on speed corrections
for the other commands. Compare raw advantages with the existing pooled movement
normalization, and separate actor, entropy and critic/global-gradient-clipping
effects. Include all three seeds and preserve the distinction between local
gradient predictions and the actual saved PPO parameter changes.

LF-066 measured cross-command effects when commands were sometimes absent. LF-067
now supplies all three commands in each balanced rollout, yet useful control does
not emerge. The next question is how those simultaneous learning signals combine,
not whether they are present. Do not infer that per-command normalization, a
larger network or more training will fix it without a matched test. The audit
needs its own frozen scope and budget; it has not run. Original stopping,
strict physical gates and all prior results remain protected. No automatic
training extension, checkpoint selection, confirmation use or model promotion.
