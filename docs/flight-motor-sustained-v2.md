# Protected sustained speed-control comparison v2

**Result: comparison completed; original stopping preserved; neither arm qualifies.**

The recipe below was frozen before execution and is retained separately in the
archive. Observed results follow the specification.

LF-061, 2026-10-25. C4 learning
attempt 30; audit count remains 18. The user authorized recorded training failures,
all three seeds, protected original stopping and unchanged strict evaluation.

LF-060 remains sealed as a failed preflight. This new attempt changes only the
preflight failure/exposure admission policy and its reporting. It reruns all six
preflights from the original parents, then initializes all main runs afresh.
Network, initialization, optimizer, reward, tasks, seeds and budgets are unchanged
from LF-060. No weight from its short preflight is continued.

## Question and matched arms

Does training through continued speed holding improve retained control, while
preserving the original learned stopping baseline? LF-059 found acquisition
without retention, weak command dependence and an aggressive initialization that
lost stopping before training. The user authorized sustained speed-holding training
with the original stopping baseline preserved.

Use seeds **17/29/43** in both arms, initialized from the original LF-053 models:

- **acquire:** movement training ends at the first qualified 30-interval dwell,
  or the original 360-tick deadline/physical failure.
- **sustain:** movement training continues through **900 ticks** after any early
  acquisition. Award the existing +1 terminal success only at tick 900 if acquired
  by tick 360 and never subsequently breached. Missing acquisition, or any breach,
  gives the existing deadline outcome at the horizon. Physical failures still end
  immediately and receive the existing -5.

The only between-arm configuration change, apart from the arm label, is the
movement retention horizon. Stopping training retains the original acquisition
rule in both arms. Use the same dense LF-055 velocity reward, gamma .99/lambda .95,
terminal zero-bootstrap, optimizer, tasks, seeds and total action exposure. There
is no immediate success bonus at first acquisition in the sustained arm. Acquisition
is a recorded event, not a training reset. A later recovery does not erase a breach.

Every rollout has **512 movement +512 stopping actions** in both arms. Episodes
and state distributions may differ because of the changed horizon; equal action
counts do not mean equal episode counts or identical later experience. Continue
joint movement/stopping actor-loss terms, separately normalized advantages and
fixed .5 coefficients, as in LF-059's joint arm. Critic training uses all rows.

## Exact stopping preservation and representation

Both arms use one new versioned neural graph:
`motor18-protected-actuator20x32x32-v2`, exported as motor format **2**. Keep all
original actor weights and original log deviations frozen. The original steering
function is unchanged at every command. The separate original critic remains
trainable. Add a trainable **20→32→32→2 tanh actuator residual** and two residual
log-deviation parameters. Total **5,259 parameters; 3,493 trainable**.

The residual sees the original 18 motor inputs plus desired-minus-current COM
velocity in body coordinates, scaled by 20:
`[-sin(heading_error)*command_speed - body_vx,
   cos(heading_error)*command_speed - body_vy] / 20`.
These are observation-derived errors, not action targets or native control labels.

Add the residual to the two original actuator logits, multiplied by
`g = command_speed / (command_speed + 1)`. Apply the same gate to the two residual
log-deviation adjustments. Steering uses the frozen learned mean; actuator samples
are independent Gaussians transformed through sigmoid, at **60 Hz**.

At exactly zero requested speed, the residual and deviation adjustments are
**exactly zero for every observation**. Original stopping outputs and actuator
sampling deviations therefore cannot be modified by optimizer updates. Stopping
actor/entropy terms remain present, but their derivatives into the gated residual
are mathematically zero; stopping still trains the independent critic. This is
stronger protection than rehearsal. It is an explicit scoped architecture choice,
not evidence that removing stopping gradients alone worked in LF-059.

There is no success-triggered intervention, native fallback or controller handoff.
The same graph computes every action. Exact zero-command function preservation
does not qualify arbitrary command-transition flights or unseen stopping states;
physical retention is checked against the original saved trials.

Initialize residual hidden layers with the declared orthogonal/tanh recipe,
residual output weights to zero, output bias to **[+2, -2] logits**, and residual
log-deviation adjustments to zero. This gives a bounded common positive-speed
propulsion preference while leaving zero speed untouched. It is **not** the
LF-059 global 25%/10% actuator reset, and positive-speed behavior is not identical
to the original parent. Evaluate the new baseline before training. Copy original
actor/critic/deviations; fresh Adam, sampler RNG, collector and history per run.

The new trainable representation and protected initialization are common to both
arms, so this comparison isolates continuation semantics within the new model;
it is not an isolated comparison against LF-059's frozen-feature readout. Explicit
time input and the deadline/bootstrap question remain deferred.

Native inference gains strict format-2 tensor validation and residual computation,
while retaining format-1 support. Native motor observations, dynamics, acquisition
and retention thresholds remain unchanged. The existing version-2 retention API
is now used in training; its serialization and physical rules are unchanged.

## Tasks and unchanged development gates

Reuse LF-059's exact **48 training starts** (seed 5910001): 24 aligned movement
cases with commands 4/10/16 and rest/half-speed/1.5-speed/reverse starts, plus 24
aligned stopping cases with initial speeds 0/6/14 and four translation directions.
Two independently seeded bearings per combination; zero initial spin/gravity,
stationary planet and altitude 150. Slots 0/2 move, slots 1/3 stop. Sustain adds
only `retention_horizon_ticks: 900` to positive-speed training tasks.

Require at least **three completed visits** to each sustained movement case and
at least **seven** to each acquisition/stopping case per main run. Preserve every
failed episode. Report training acquisition, post-acquisition ticks, later breaches,
continued ticks beyond 360, rewards, useful acceleration and all physical outcomes.

Initial/final development uses the unchanged **104 cases** from LF-059: 24 aligned
tracking at unseen speeds 6/12/18, 24 aligned stopping at initial speeds 0/8/18,
24 original LF-055 braking cases and 32 original LF-054 turn/hold cases. All use
acquisition by 360 and uninterrupted retention through 900. Heading <=5°,
velocity-vector error <=1, spin <=.2 for thirty complete consecutive intervals;
no post-acquisition breach or physical failure.

The per-seed movement-learning gate remains >=20/24 tracking, >=6/8 per command,
>=4/6 per initial-speed mode, gain >=12 over its own initial baseline, and no
physical failures. The full stage additionally requires >=20/24 aligned stopping
including >=7/8 stationary, >=20/24 original braking, original heading gate and
no lost original heading success. Overall success requires **two sustained seeds**
to pass. Separately report paired support for sustained training only if at least
two seeds retain >=4 more tracking cases than acquisition controls. No automatic
checkpoint selection, promotion, budget extension or extra seeds.

Verify all original 56 zero-speed reference trajectories per seed **exactly** at
both initial and final evaluation: **672 comparisons**. Also require all 80
zero-command development trajectories to be exactly unchanged before/after each
run: **480 comparisons**. The known seed-17 heading limitation is preserved and
reported, not repaired by dropping the seed or adding assistance.

## Validation, budgets and evidence

Before freeze, **24 Python tests** pass. The unchanged native runtime retains
LF-060's passing **3 native model tests** and **48 native flight-environment tests**;
their original logs and build hash are retained without claiming a new native run.
Four added tests distinguish recorded physical failure from unexplained truncation,
retain the collision penalty and verify that one final collision still fails. New checks cover protected actor/distribution
invariance after real Adam updates, zero stopping residual gradients, velocity
error features, native format-1 compatibility and format-2 parity, delayed reward
through a 900-tick qualified flight, continued collector episodes across rollouts,
compressed serialization and contract rejection. These implementation checks run
outside the experiment's training/evaluation counts.

Six **four-update preflights** use fixed cases 0/1/40/33, followed by one exact
last-update continuation each. Require all four cases completed, exact resume,
protected actor checks, valid physical termination/reward evidence and reconstructed
likelihoods. Unknown outcomes, corrupted data, broken preservation and unexplained
early termination remain fatal. The explicit admission policy is
`record-physical-failures-v1`; the original strict policy remains available.

Collisions, damage, ship loss and boundary escape still terminate immediately,
receive the existing -5 penalty and remain named failures in all statistics.
They do not block the main learning experiment when correctly recorded. Each run
must still have >=512/2,048 movement samples above 5% throttle and, independently,
>=512 below 40% brake.

For sustained preflights, report actual movement ticks beyond 360 separately from
continuation time forgone through physical termination. For each such terminated
movement episode, forgone time is `max(0,900-max(360,termination_tick))`. Require
actual plus explicitly accounted-for forgone continuation >=1,080 ticks. Forgone
time is never counted as performed physics, observed experience or success. For
example, LF-060 seed 17 has 871 actual ticks plus 209 forgone after its tick-691
collision. With no physical failures, the original >=1,080 actual-tick requirement
is unchanged. The audited trace must independently justify every outcome.

These checks qualify implementation and exposure accounting only. Final physical
qualification remains strict; neither recorded failures nor forgone time earn
credit there. Stop on implementation/accounting failure without changing the
frozen recipe. Discard all preflight weights before main initialization.

Six **128-update main runs**, then exact last-update continuations from update127.
256 ticks x4 slots/update; four epochs and minibatches of 256, at most sixteen
optimizer steps/update. Learning rate .0003, clip .2, entropy .001, value weight .5,
combined gradient clip .5, KL limit .03. No scheduler or automatic extension.

**26 bounded processes**, at most three concurrent. Preflights/resumes: **180
internal /200 external seconds**; main/resumes: **900/920**; verification: **300/320**.
Totals/caps including repeated last updates: **823,296 training ticks**, **12,864
optimizer steps**, **24,000 restore ticks**, **1,248 evaluations /1,123,200 scored
physics ticks**, **1,248 inference comparisons**, **811,008 likelihood checks**,
**405,504 sampled zero-command actor/distribution checks**, **816 frozen-parent
checkpoint checks**. Main movement/stopping exposure remains **393,216 each**.
Count neutral setups separately.

Retain **816 checkpoints, 804 rollouts, 48 exports**, every trajectory, source,
configuration, runtime, command, failure and validation log. Checkpoints, rollouts
and traces use **lossless gzip compression**, including exact round-trip and
no-overwrite checks. Export policies remain native JSON. Compressed input copies
record both compressed-file and original-byte hashes. All original archives and
inputs remain untouched. Require >=2 GiB free before freeze and stop any run below
a 512 MiB free-space reserve; do not delete earlier experiments.

Archive: `data/training-experiments/learned-flight-motor-sustained-v2-20261025`.
Freeze source/config/task/runtime/input/runner hashes before preflight; preserve
all **56 prior manifests**. No confirmation-set use or navigation qualification.
Local retention is not off-machine backup.


Artifact storage: `/home/data2/space-wars3-artifacts/training-experiments/learned-flight-motor-sustained-v2-20261025`, linked from the archive path above. The workspace volume dropped below the pre-freeze 2 GiB reserve, so this new archive uses the second local disk. Capacity checks apply to the artifact volume; code remains in the existing standalone checkout. Earlier archives are untouched. This is local retention, not off-machine backup.


## Observed comparison

All six main runs complete 128 updates and 2,048 optimizer steps without KL early
stopping. Neither the strict stage gate nor the declared two-seed sustained
improvement gate passes. Sustained training adds eight retained cases in one seed;
all eight request speed 6 and cover all four initial-speed modes. Requests 12 and
18 have no retained successes in any final model.

| Arm | Seed | Initial tracking /24 | Final tracking /24 | Aligned stopping /24 | Original braking /24 | Original turn/hold /32 | Final collisions |
| --- | --- | --- | --- | --- | --- | --- | --- |
| acquire | 17 | 0 | 0 | 0 | 0 | 0 | 5 |
| acquire | 29 | 8 | 0 | 24 | 24 | 32 | 9 |
| acquire | 43 | 0 | 0 | 24 | 24 | 32 | 14 |
| sustain | 17 | 0 | 0 | 0 | 0 | 0 | 0 |
| sustain | 29 | 8 | 0 | 24 | 24 | 32 | 0 |
| sustain | 43 | 0 | 8 | 24 | 24 | 32 | 3 |

Every reported physical failure in this table is a collision in a movement case.
The 80 zero-command cases are exactly unchanged before and after training in each
run. All original 56 reference trajectories per seed also reproduce exactly at
both evaluations, including the original failures. Protection does not repair
seed 17's inherited steering limitation. The common positive-speed initializer
already retains eight speed-6 cases for seed 29; both trained arms lose those
movement successes. That regression is separate from preserved stopping.

Acquisition-only seeds 29/43 each acquire 16 development cases by the six-second
deadline and retain none through fifteen seconds. Their first qualified dwell is
not a reliable stopping point for evaluation. Sustained seed 43 acquires and
retains all eight speed-6 cases. Some speed-18 cases acquire late and then breach;
they fail both the original acquisition deadline and continuous retention rule.
Sustained seed 29 acquires no development tracking case on time.

## What the trajectories show

The sustained arm reduces final movement collisions from 5/9/14 to 0/0/3 across
seeds, but reduced collisions alone does not establish successful speed control.
Sustained seed 29 becomes conservative: its two stationary-start trials per
command have last-120-tick mean forward speeds approximately 2.43, 2.81 and 3.03
for requested speeds 6, 12 and 18. Its median final tracking velocity RMS error is
9.19 units/s, versus 9.07 in acquisition-only seed 29.

Sustained seed 43 has median tracking velocity RMS error 2.01 versus 23.84 for its
acquisition-only counterpart. Last-120-tick rest-trial means are about 6.81, 9.99
and 23.16 for requests 6/12/18. The speed-6 trial remains inside the allowed error;
the speed-12 trial settles about 2 units/s too slowly. In the speed-18 rest trial,
forward speed is 12.57 at tick 360, 17.49 at 600 and 24.64 at 900. Throttle rises
from .1390 to .1422 and brake falls from .1281 to .1218 between ticks 360 and 900,
even as the ship passes through and exceeds its target. Other state inputs also
change along the trace, so this is evidence for a corrective-response audit rather
than proof of a specific partial derivative or sole cause.

These descriptive analyses use the retained final policies and traces. They add
no optimizer updates, new physical trials, checkpoint selection or relaxed gates.
Raw per-case summaries are retained in `analysis.json` and `trajectory-details.json`.

## Training exposure and recorded failures

All six new preflights exactly reproduce LF-060's final model, optimizer, collector,
history, RNG and last rollout, apart from the expected contract source inventory.
All pass the newly declared admission checks. Seed 17's sustained preflight again
collides at tick 691: 871 actual continuation ticks plus 209 explicitly forgone
ticks meet the accounting check. The collision remains a penalized failure.

Main movement/stopping exposure is 65,536 actions each per run. All declared task
coverage minima pass. Training outcomes below count completed movement episodes;
longer episodes naturally produce fewer completions under equal action budgets.

| Arm | Seed | Successes | Deadlines | Collisions | Actual movement ticks beyond 360 |
| --- | --- | --- | --- | --- | --- |
| acquire | 17 | 0 | 182 | 0 | 0 |
| acquire | 29 | 119 | 138 | 0 | 0 |
| acquire | 43 | 139 | 121 | 0 | 0 |
| sustain | 17 | 0 | 72 | 0 | 38,896 |
| sustain | 29 | 6 | 66 | 0 | 38,896 |
| sustain | 43 | 2 | 68 | 4 | 38,736 |

Sustained seeds 29/43 acquire in 18/25 observed episodes, record 14,510/16,208
post-acquisition ticks and breach in 12/23 episodes. Their six/two completed
training successes are all at requested speed 4. The four main training collisions
occur in sustained seed 43 and remain in the archive and reward reconstruction.
Seed 17's short preflight collision does not recur in its full-matrix main run;
the main sampler and experience differ, so that is not a failed reproduction.

## Verification, budget and retention

All **26 processes** finish within their frozen limits, without retries or budget
extensions. The six main runs take about 42–71 seconds each; full verification
takes 137.61 seconds. All 24 Python tests pass. The unchanged native build retains
the earlier 3 model and 48 environment test results, with original logs/hash.

Verify **12 exact resumes**, **12 original-parent initializations**, **1,248
native/Python comparisons**, **811,008 likelihood reconstructions**, **405,504
zero-command actor/distribution comparisons**, and **816 frozen-parent checkpoints**.
Maximum reconstructed likelihood error is 9.54e-7. All **672 original complete
reference trajectories**, **480 initial/final zero-command trajectories** and
**312 paired initial trajectories** compare exactly. Every saved training/evaluation
terminal outcome and retention sequence is independently inspected.

Actual work: **823,296 training ticks**, **12,864 optimizer steps**, **8,279
restoration ticks**, **1,117,535 evaluation ticks across 1,248 episodes**, and
**6,894 neutral setup ticks**. Main movement/stopping exposure is **393,216 each**.
Forgone continuation is reported separately and contributes zero performed ticks.
No confirmation set, model promotion, native assistance or navigation qualification.

Retain **816 checkpoints, 804 rollouts, 48 exports**, every trace/log, **1,617
sources** and **176 copied inputs**, with lossless gzip and verified original-byte
hashes. All **56 previous manifests** remain unchanged. The failed LF-060 archive
is preserved with its original gate and result. See results (`docs/data/flight-motor-sustained-v2-results.json`, in git history)
and [archive receipt](data/flight-motor-sustained-v2-archive.json).

## Next decision

Inspect the saved policies' corrective response before another training change.
Freeze a bounded read-only comparison of all three seeds and both arms at matched
physical states: vary command speed and along-heading velocity consistently, then
measure how throttle and brake respond to underspeed and overspeed. Separate the
frozen original actor from the learned residual, and relate the responses to the
saved training states and update history. This would test whether the new branch
learns useful velocity feedback rather than only command-dependent propulsion.

The current evidence motivates that question: sustained seed 43 settles near 6.81
for a request of 6 and 9.99 for 12, but its speed-18 rest trial reaches 24.64 at tick
900 while slightly increasing throttle and reducing brake along the trajectory.
Sustained seed 29 instead stays near speeds 2.43/2.81/3.03 for requests 6/12/18.
These are observed trajectories, not controlled causal perturbations or proof of
one reward/representation defect. Do not infer that more training alone will fix
them, change rewards, select an intermediate checkpoint or add a controller rule.

Keep exact original zero-command protection and strict final qualification. Retain
all seeds, failures and earlier archives; the small speed-6 gain is not a promotion.
No automatic budget extension, confirmation use, native teacher, fallback or
controller handoff. Obstacle navigation remains a later qualification task.
