# Protected sustained speed-control comparison v1

**Result: stopped at preflight; no main training or development evaluation.**

LF-060, 2026-10-25. C4 learning attempt 29; audit count remains 18. The recipe
below was frozen before execution; its planned main runs were not reached.
The original specification is retained separately in the archive.

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

Before freeze, **20 Python tests**, **3 native model tests** and **48 native
flight-environment tests** pass. New checks cover protected actor/distribution
invariance after real Adam updates, zero stopping residual gradients, velocity
error features, native format-1 compatibility and format-2 parity, delayed reward
through a 900-tick qualified flight, continued collector episodes across rollouts,
compressed serialization and contract rejection. These implementation checks run
outside the experiment's training/evaluation counts.

Six **four-update preflights** use fixed cases 0/1/40/33, followed by one exact
last-update continuation each. Require all four cases completed, no physical
failures, exact resume, protected actor checks and reconstructed likelihoods.
Each run must have >=512/2,048 movement samples above 5% throttle and, independently,
>=512 below 40% brake. Each sustained preflight must include >=1,080 movement ticks
beyond the original 360-tick deadline. These are plumbing/exposure checks, not
speed-control qualification. Stop on failure without revising the frozen recipe.
Discard preflight weights before main initialization.

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

Archive: `data/training-experiments/learned-flight-motor-sustained-v1-20261025`.
Freeze source/config/task/runtime/input/runner hashes before preflight; preserve
all **55 prior manifests**. No confirmation-set use or navigation qualification.
Local retention is not off-machine backup.


## Observed preflight result

The frozen gate failed. All six four-update runs and their exact last-update
continuations completed, but the original verifier stopped at a **seed-17
sustained collision at tick 691 (11.52 seconds)**. No main training, main resumes,
initial/final development evaluations or model selection occurred. A supplemental
read-only audit reviewed all six completed preflights with the same correctness
checks and original gates; it did not retry training or change the specification.

| Arm | Seed | Completed movement outcomes | Completed stopping outcomes | Movement ticks after 360 | Preflight gate |
| --- | --- | --- | --- | --- | --- |
| acquire | 17 | 4 deadlines | 4 deadlines | 0 | Pass |
| acquire | 29 | 16 successes | 43 successes | 0 | Pass |
| acquire | 43 | 7 successes, 1 deadline | 44 successes | 0 | Pass |
| sustain | 17 | 1 collision, 1 deadline | 4 deadlines | 871 | Fail |
| sustain | 29 | 2 deadlines | 43 successes | 1,080 | Pass |
| sustain | 43 | 2 deadlines | 44 successes | 1,080 | Pass |

Each primary preflight contains exactly 2,048 movement and 2,048 stopping actions.
Different completion counts reflect duration and resets, not different action
budgets. No preflight passing result implies control competence. All six satisfy
both declared action-coverage thresholds and complete all four fixed task cases.
Sustained seed 17 fails both physical safety and the 1,080-tick continuation
exposure minimum because the collision terminates one trial early.

The collision occurs in case 40: requested speed 16, initial speed 24, initially
aligned heading. Heading error reaches about 29.93 degrees at tick 120 and
36.79 degrees at tick 360; immediately before collision it is 29.77 degrees with
velocity error 14.30 units/s. The original learned steering is exactly unchanged,
including seed 17's known limitations. This trace shows failure under the longer
horizon; it does not isolate steering as the sole cause of collision. Stopping
protection preserves original failures as well as successes.

Sustained seed 29 acquires in three observed movement episodes (including one
still active at the final checkpoint), accumulates 1,541 post-acquisition ticks
and records two breached episodes. Seed 43 acquires in two episodes, accumulates
1,470 post-acquisition ticks and breaches both. Neither completes a retained
training success in four updates. These small preflights demonstrate continued
experience of the retention problem; they cannot establish whether the planned
128-update training would learn it or outperform acquisition training.

## Preservation, validation and actual budget

All implementation tests pass: **20 Python, 3 native model and 48 native flight
environment tests**. Native format-1 compatibility and format-2 parity were tested
before freeze. The behavioral preflight failure is separate from these tests.

The supplemental audit verifies six declared original-parent initializations,
six exact resumed final states/rollouts, all **24,576** saved likelihoods,
**12,288** original zero-command actor/deviation comparisons and **36** unchanged
parent-actor checkpoints (including repeated checkpoints via exact equality).
Steering outputs also match the frozen parent on every saved observation.
Every saved reward, terminal outcome and acquisition/breach sequence is inspected.
All six runs perform 64 optimizer steps plus 16 in each last-update repeat, with
no KL early stops. The frozen zero-speed function and its sampling deviations
remain intact; the planned 672 full parent-trajectory and 480 initial/final
zero-command trajectory comparisons were **not run**.

Actual consumption: **30,720 training ticks** (24,576 primary +6,144 repeats),
**480 optimizer steps**, **5,171 restoration ticks** and **323 neutral setup ticks**.
No development physics, evaluation episodes or experiment inference comparisons
were performed. Pre-freeze implementation tests are outside these counts.
Twelve training/resume processes exit successfully; the thirteenth process is
the verifier's expected nonzero exit at the failed gate. All are within budget.
The additional read-only review completes in 14.47 seconds, with no physics or
optimizer steps. The original failed verifier log remains unchanged.

Retain **36 checkpoints, 30 rollouts, 24 exports**, all traces/logs, **1,616 source
files** and **176 copied inputs**. Checkpoints, rollouts, traces and selected input
copies are losslessly compressed; original-byte hashes and source origins are
verified. All **55 earlier manifests** remain unchanged. See the machine-readable
[results](data/flight-motor-sustained-v1-results.json) and
[archive receipt](data/flight-motor-sustained-v1-archive.json).

## Interpretation and next decision

Separate implementation checks from learning qualification in the next frozen
protocol. LF-060's all-seed no-collision preflight gate blocked the comparison
because seed 17 remains an imperfect original controller. That gate was too
restrictive for a learning experiment: a correctly recorded collision can be
training evidence, while corruption, incorrect rewards or failed preservation
checks must stop execution. Keep the final development no-physical-failure gate
unchanged, retain seed 17, and never count its collision as success.

The proposed next attempt should retain the same paired acquisition/sustained
recipe and original stopping protection, but explicitly permit recorded physical
training failures and account for their effect on continuation exposure. Freeze
that change and its budgets separately; do not reopen or relabel LF-060 as passed.
No main learning comparison, additional training, confirmation use or model
promotion follows from the failed preflight. Preserve 60 Hz learned continuous
ownership, all earlier archives, and the bans on native teachers, fallback and
controller handoff. Obstacle navigation remains unqualified.
