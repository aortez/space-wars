# Motor heading retention and steering-response audit

**Status: complete; retention gate passed in two of three seeds, 2026-10-25.** Experiment
`learned-flight-motor-retention-v1-20261025`, journal LF-054, follows the
[small motor pilot](flight-motor-turn-v1.md). The user authorized continuation:
check longer heading retention and inspect the failed seed's heading offset.
Use all three final update-64 policies without training or model selection.

## Fixed physical comparison

Retain LF-053's **32 development cases**, task seed 5310002, for each learner
seed **17/29/43**: 24 turns at absolute angles 30/60/120/165°, both signs and
spins −.45/0/+.45, plus eight stationary checks. Starts, zero-speed commands,
worlds, 60 Hz physics/actions and physical tolerances remain unchanged.

Add only `retention_horizon_ticks=900` to each task: **fifteen seconds**. The
original **360-tick/six-second acquisition deadline** still applies. Record the
first time heading error ≤5°, COM velocity error ≤1 and absolute spin ≤native-f32
.2 have all held for thirty complete consecutive intervals. Continue through
that first success and the original deadline, unless a physical failure occurs.

A retained case must acquire by tick 360 and never leave any tolerance after its
first successful dwell through tick 900. Record the first later violation
permanently; returning within tolerance cannot erase it. First acquisition after
360 also cannot pass. Terminal success occurs only at the observation horizon
if both conditions hold; otherwise terminal outcome is deadline. Collision,
damage, ship loss and boundary escape remain immediate physical failures.
These are additional retention criteria, not a reinterpretation of LF-053's
already-recorded first-dwell successes.

The native carrier flight deadline/dwell now equals the observation horizon,
preventing the carrier's positional rule from stopping the audit early. Optional
retention tasks use **motor trace version 2** and explicit first-success/first-exit
metadata. Tasks without the option keep version 1, their original termination
rules and serialized fields. The eighteen policy inputs, export schema, network,
actuators and physics are unchanged. Validate every new trajectory's prefix against
its original LF-053 final trace, normalizing only added observation metadata and
the deliberately deferred terminal outcome. No physical frame, action, input or
dwell count may differ.

Require the same **two or more seeds** jointly to retain at least 20/24 turns,
with at least 9/12 on each sign, and at least 7/8 stationary cases acquired at
exactly tick 30, with zero physical failures. In addition, **every case that
succeeded for that seed in LF-053 must remain successful**. The two previously
passing seeds therefore each need all 32 prior successes retained. A seed with
no previous successes cannot pass vacuously; the competence thresholds still apply.
All three seeds and every failure remain in the report.

Measure post-acquisition heading/speed/spin maxima, first exit, acquisition time,
final signed heading error and COM displacement. There is still no position-hold
gate: a zero-speed motor tolerance does not qualify navigation arrival or precise
station keeping. This audit does not test changing commands, nonzero-speed
tracking, gravity, obstacle avoidance or broader cases.

## Matched-state steering inspection

Use six saved input anchors from **seed 17's original final evaluation**: cases
0 (left 30°), 1 (right 30°), and 24 (stationary), each at reset and the final
tick 360. These are predetermined observations from existing traces; no new
anchor flights or physical mutations are made.

Evaluate **all three policies on these identical anchors**. For each, retain
its original input, then vary only heading-error sine/cosine through integer
degrees **−45…+45**, inclusive. Other state, speed command, capability and previous
action inputs stay fixed. There are 92 inputs per anchor, six anchors, three
policies: **1,656 native/Python comparisons**. Save every output and report steering
at zero requested heading error and adjacent grid intervals containing steering
zero crossings. Brackets are measurements at one-degree resolution, not exact
roots or a simulated stability proof. No adaptive follow-up queries.

Check the six original-input actions for seed 17 against its initial actions and
tick-361 actions in the continued trajectories. Preserve all weights, Torch RNG
and source input vectors. This can expose a conditional offset in the learned
steering response; it cannot establish which training updates caused it.

## Verification, budget and retention

Five bounded processes: three flight workers (≤600 seconds internally /620
externally), one response worker and one verifier (≤300/320 each). No scored
retry, new recipe, optimizer step or automatic continuation into velocity training.

Run **96 scored episodes**, at most **86,400 evaluation action ticks**. Each
worker also restores case 24 at tick 450 and reproduces its remaining 450 ticks,
requiring **1,350 restoration action ticks**, **1,350 repeated continuation ticks**
and three neutral setup ticks total. Full replayed traces must equal their
uninterrupted counterparts exactly. Evaluation reset ticks plus continuation
restores total **99 neutral setups**.

Compare Python/native output at each scored episode's reset and final state:
**192 endpoint comparisons**, reusing those observations without added resets.
Together with the heading sweep: **1,848 comparisons**. Require 96 exact parent
prefixes, three exact native continuations, reconstructed physical errors/dwell/
outcomes and unchanged model parameters/RNG. Loading checkpoint weights for
read-only inference is explicit; this is not a changed-runtime PPO resume.
Existing strict training checkpoint identity remains unchanged.

Before execution, freeze **1,597 sources**, including the changed native motor
environment and two new audit/test files; the runner, specification, 32-case
matrix, new runtime/dependency lock and **104 copied inputs** (three final policies,
three checkpoints, 96 original final traces, parent results and development matrix).
Verify all **49 prior manifest hashes** and the complete parent archive. Preserve
all new physical traces, parity/probe records, tests, commands, timings and exits.
No new training checkpoints or policy exports are created.

Implementation checks: **13 Python tests** (six retention tests plus seven prior
motor tests), **48 native environment tests** (four new retention tests), and a
successful native bridge build. Unit-test optimization and scripted physics
fixtures are separate implementation work; no fitted pilot weights enter training.
Local retention is not off-machine backup. No reserved confirmation use or model
promotion is implied.

## Results

Both passing motor policies retain all 32 prior successes through fifteen-second trials; the failed policy consistently settles 19.57 degrees off target.

| Seed | Turns retained /24 | Stationary cases retained /8 | Lost prior successes | Median final heading error | Maximum COM displacement | Retention gate |
|---:|---:|---:|---:|---:|---:|---|
| 17 | 0 | 0 | 0 | 19.569° | 7.771 | Fail |
| 29 | 24 | 8 | 0 | 0.102° | 6.126 | Pass |
| 43 | 24 | 8 | 0 | 0.418° | 3.526 | Pass |

Seeds **29 and 43 each retain all 24 turns and all eight stationary cases**,
including every one of their original LF-053 successes. There are no later
heading, speed or spin tolerance exits after acquisition in either policy. Both
retain all twelve turns in each direction, and the stationary cases acquire at
tick 30. The predeclared same-two-seed gate passes. All 96 scored episodes reach
the full 900-tick horizon without a physical failure.

Their original acquisition times reproduce exactly. The longest original turn
acquisition is 212 ticks, leaving at least **688 ticks / 11.47 seconds** of
uninterrupted retention after the first successful dwell in the passing policies.
Stationary cases retain for another **870 ticks / 14.5 seconds**. Thus the prior
successes were not merely brief visits inside the acceptance window.

Seed **17** retains no turns or stationary checks and still fails the competence
gate. It never acquires during the extended trials. It loses no prior successes
because it had none; this does not grant it a vacuous retention pass. Across all
32 starts its final absolute heading error converges to **19.568–19.570°**,
median **19.569°**, with low speed and almost zero spin.

### Physical retention and remaining drift

After acquisition, the maximum observed heading errors are **.166°** for seed 29
and **1.276°** for seed 43; corresponding maximum COM speeds are **.420/.308**
units/s and spins **.0107/.0471** rad/s. At the horizon, their median heading
errors are **.102°/.418°**, maximum speeds **.410/.236** units/s and maximum spins
below **.000020** rad/s. These remain comfortably within the declared motor
thresholds throughout the measured retention interval.

The policies still drift. Maximum displacement reaches **6.126 units** for seed 29
and **3.526** for seed 43, compared with **7.771** for seed 17. This is consistent
with a residual speed below the allowed 1 unit/s; the motor task has no position
target. Stable heading and passing speed tolerance do not establish precise
station keeping or navigation arrival. The remaining translational error is
explicit evidence for the next velocity-control stage, not a reason to rewrite
this audit's frozen gate.

### Failed-seed steering response

The matched-state probes support an offset in seed 17's learned steering
response. On all three **late** anchors, its steering zero crossing lies between
signed heading errors **−20° and −19°**. The recorded physical heading error at
those anchors is approximately −19.47° to −19.58° (desired minus actual heading).
Changing only the requested heading to make that error zero produces steering
approximately **−.170** instead of a neutral command. The long physical trials
then converge to the same roughly −19.57° error.

On those **identical late ship states**, seed 29's steering zero crossings are
between **0° and 1°**, and seed 43's between **−1° and 0°**. At the reset anchors,
seed 17's crossings are −15° to −14° for the two spinning turn starts and −17°
to −16° for the stationary start. The variation with ship state means this is
not established to be one constant output-bias parameter.

These observations locate the behavioral failure in the policy's conditional
steering response and are consistent with the wrong-heading equilibrium seen
in physics. They do **not** identify which training updates, reward terms or
parameters caused it. A zero crossing from a finite heading sweep alone is not
a dynamic stability proof. No controller correction, changed weight, adaptive
probe or extra flight was introduced to repair the failed seed.

| Policy seed | Shared anchor | Steering at zero heading error | Steering-zero bracket |
|---:|---|---:|---|
| 17 | Seed 17 case 0, tick 360 | -0.170029 | [[-20, -19]]° |
| 17 | Seed 17 case 1, tick 360 | -0.169860 | [[-20, -19]]° |
| 17 | Seed 17 case 24, tick 360 | -0.169924 | [[-20, -19]]° |
| 29 | Seed 17 case 0, tick 360 | 0.003569 | [[0, 1]]° |
| 29 | Seed 17 case 1, tick 360 | 0.004013 | [[0, 1]]° |
| 29 | Seed 17 case 24, tick 360 | 0.003845 | [[0, 1]]° |
| 43 | Seed 17 case 0, tick 360 | -0.013718 | [[-1, 0]]° |
| 43 | Seed 17 case 1, tick 360 | -0.013589 | [[-1, 0]]° |
| 43 | Seed 17 case 24, tick 360 | -0.013638 | [[-1, 0]]° |

### Verification and retention

All **13 Python tests** and **48 native environment tests** pass. New checks
cover continuation after acquisition, first-exit memory despite recovery,
acquisition after the original deadline, legacy serialization, invalid horizons,
active replay and the exact scope of heading-input interventions. Existing motor
model, inference and continuation checks remain passing.

All **96 parent prefixes** reproduce LF-053 actions, physical frames, observations
and dwell counts exactly. Only declared retention metadata, trace version/task
option and deferred terminal outcomes differ. All **three tick-450 native
continuations** reproduce complete uninterrupted traces exactly. All **1,848
native/Python comparisons** pass: 192 endpoints and 1,656 heading-grid inputs.
The six seed-17 original-input actions also match recorded actions exactly.
Weights, global Torch RNG and original probe input vectors remain unchanged.
No changed-runtime training checkpoint resume or training result is claimed.

All five bounded processes exit zero without retry. External flight-process times
are **5.27–5.47 seconds**, response inspection **1.32 seconds**, and verification
**2.42 seconds**. Use **86,400 scored evaluation action ticks**, **1,350 repeated
continuation action ticks**, **1,350 restoration action ticks** and **99 neutral
setup ticks**, within every declared limit. There are **zero training ticks,
optimizer steps, new checkpoints or new policy exports** in this audit; unit-test
training remains separate implementation verification.

Retain 1,597 frozen sources, 104 copied inputs, the task matrix, runtime/dependencies,
96 scored traces, three continuation traces, every parity/probe result, tests,
commands and timings. All 49 prior manifest hashes and copied input sources remain
unchanged. See [results](data/flight-motor-retention-v1-results.json) and
[archive receipt](data/flight-motor-retention-v1-archive.json). No reserved
confirmation use, model promotion or navigation qualification. Local retention
is not off-machine backup.

### Next decision

Freeze a separate **brake-to-rest and velocity-tracking stage** for the same
small motor architecture. The current passing policies provide measured heading
control, while residual translation remains. Define a complete procedural matrix
of initial velocity magnitudes/directions and speed commands before collecting
training data, with matched before/after evaluation and the existing turning and
long-retention cases kept as regressions.

Retain all three seeds. Record seed 17's approximately twenty-degree steering
offset as a known failure; do not silently recenter its output, discard it or
replace it with another seed. Specify how the next training stage starts from
all three retained policies, its optimizer reset/continuation semantics, physical
velocity-error requirements, reward, coverage and stopping budget before running.
The exact speed range and new numerical tracking gates remain to be frozen for
that experiment. No velocity training or runtime navigation integration occurs
in LF-054. The original position-arrival criteria and no-native-fallback boundary
remain unchanged.

