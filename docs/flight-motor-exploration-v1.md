# Motor throttle/brake exploration audit

**Status: complete; persistent exploration passes the coverage gate in two of three models, 2026-10-25.** Experiment
`learned-flight-motor-exploration-v1-20261025`, journal LF-056. The user authorized
trying throttle/brake exploration with the earlier models after
[LF-055](flight-motor-velocity-v1.md) found almost no sampled thrust or brake release.

## Question and unchanged policy

Can broader throttle/brake exploration expose useful acceleration while the learned
steering keeps heading stable? This is a frozen-policy coverage audit, not training
or a demonstration of learned speed tracking. Use all three original LF-053 final
models (seeds 17/29/43), including seed 17's known heading offset. Verify original
checkpoint hashes and policy tensors. No model edits, output bias recentering,
optimizer, new policy export, teacher, rescue or controller handoff.

Steering uses the **current learned mean output in every arm**, recomputed from
actual observations each 60 Hz tick. This deliberately removes steering noise to
isolate actuator exploration. The baseline matches the original *throttle/brake*
distribution, not LF-055's complete stochastic three-actuator policy. Steering is
neither fixed nor supplied by a scripted stabilizer. Dynamics, inputs and all
actuator limits remain unchanged; sampled actions execute without filtering.

## Frozen comparison

Let the original actor logits be `mu`, and its learned, clamped/exponentiated
throttle/brake deviations be `sigma`. Execute `sigmoid(mu + m*sigma*z)` separately
for throttle and brake. No directional mean shift or correlation between actuators.
All arms use tanh of the learned steering mean.

| Arm | Deviation multiplier m | Residual persistence rho |
|---|---:|---:|
| baseline | 1 | 0 |
| wide4 | 4 | 0 |
| wide8 | 8 | 0 |
| wide8_persistent | 8 | .95 |

Wider deviations are an audit-only distribution change; do not alter stored
`log_std` or the training sampler. These settings bracket the narrow inherited
low-thrust distribution. Their actuator outputs remain in [0,1]. For persistent
noise, `z[0]=epsilon[0]`, then
`z[t]=rho*z[t-1]+sqrt(1-rho^2)*epsilon[t]`. This preserves the unit-Gaussian
residual marginal from the first tick and sustains exploratory pushes across
adjacent ticks. The actor mean still updates every tick.

Freeze all standard-normal innovations in advance with private Torch generators.
The same case/repetition gets exactly the same innovations for all four arms and
all three models; resulting physical states and actor means may diverge. Four
noise seeds per physical start, no seed search or retries. Global Torch RNG and
model parameters must remain unchanged.

## Fixed cases and physical measurements

Ten physical starts, four noise repetitions each: **40 episodes per model/arm**.
Six acceleration starts cross command speeds **6/12/18** with two independently
seeded world orientations. Four braking controls cross initial speeds **8/24**
with forward/reverse velocity and command speed zero. All starts are aligned with
the requested heading, initially spin-free, zero gravity and a stationary world
frame. Desired direction is ±30° from radial outward; full-circle world bearing
comes from private per-case seed 5610001+case. Altitude remains 150. Acceleration
starts at rest. Noise seed is 5620000+episode index.

Every episode continues for **360 ticks/six seconds**, unless a physical failure
occurs. Use the existing optional retention contract with a **180-tick acquisition
deadline** and 360-tick horizon. Native motor success still uses heading ≤5°,
velocity-vector error ≤1 unit/s, spin ≤0.2 rad/s and 30 complete qualifying
intervals, with permanent post-acquisition exit memory. These short audit horizons
do not replace LF-054/LF-055's longer qualification tests.

Primary **useful speed exposure** is at least 30 complete consecutive intervals
with forward velocity between **50% and 150% of command speed**, lateral speed
≤1 unit/s, heading ≤5° and spin ≤0.2 rad/s. Both ends of each physics interval
must satisfy the bounds. This deliberately measures exposure to useful motion;
it does not claim precise speed tracking. Record time to first exposure, longest
qualifying run, heading/spin interval fraction, maximum forward/lateral speed,
overspeed intervals, native acquisition/retention, every physical failure and
throttle/brake statistics. Reconstruct all metrics from native frames and actions.

Braking controls reveal how a broadly applied exploration setting affects an
already competent behavior. Report acquisition and uninterrupted retention for
all 16 controls per arm/model. They are mandatory descriptive controls, not a
braking-competence gate for stochastic exploration. Failure there forbids claiming
that an arm is a generally qualified controller or is ready for universal use.

## Predeclared coverage gate

A variant passes only if the **same arm** meets all of these in at least two of
the three models:

- Useful speed exposure in ≥8/24 acceleration episodes and ≥2/8 at each speed.
- At least six additional exposed episodes versus its paired baseline.
- Heading ≤5° and spin ≤0.2 for ≥90% of all 24×360 acceleration intervals.
  Missing intervals after a physical failure count against this fraction.
- Zero physical failures across all 40 acceleration and braking episodes.

Report every arm/model, including seed 17. Multiple passing variants remain
separate results; do not silently select one. Passing permits discussion of a
bounded learning experiment, not model promotion. No reserved confirmation use,
velocity-control qualification, navigation integration or additional training.

## Evidence and budgets

Three bounded flight processes (600 seconds internal/620 external each), followed
by one verifier (300/320 seconds). **480 scored episodes**, at most **172,800
scored action ticks**. Save every native trace and every actor/innovation/residual/
latent/applied-action record. Compare native/Python inference at both endpoints:
**960 comparisons**. Preserve the source snapshot, original inputs, runtime,
cases, draws, specification, tests, commands, times and exits.

For each model/arm, snapshot the first acceleration case at tick 180, restore its
native prefix and sampler residual/cursor, then repeat the remaining 180 ticks.
All twelve continuations must exactly reproduce actions, actor outputs and the
complete native trace. Maximum **2,160 restoration ticks**, **2,160 repeated
continuation ticks**, **492 neutral setups**, and **12 sampler snapshots**.
These are audit snapshots, not training checkpoints. Zero training ticks,
optimizer steps, new training checkpoints or policy exports. No retries or budget
extensions. Verify all 51 previous manifest hashes and input origins before sealing.
Local retention is not off-machine backup.

## Results

Persistent throttle/brake exploration exposes useful acceleration in both heading-capable models; wider independent noise does not cover all requested speeds.

Only **wide8_persistent** passes the frozen coverage gate, in seeds **29 and 43**.
All models, critic tensors, learned deviations and global Torch RNG remain unchanged.
There are no training updates. Useful exposure counts require a half-second of
forward speed inside 50–150% of the request, low lateral speed, heading ≤5° and
spin ≤0.2; they are not precise speed-tracking successes.

| Model seed | Original noise /24 | 4× independent /24 | 8× independent /24 | 8× persistent /24 | Persistent heading/spin interval fraction |
|---:|---:|---:|---:|---:|---:|
| 17 | 0 | 0 | 0 | 0 | 1.74% |
| 29 | 0 | 0 | 8 | **19** | **98.26%** |
| 43 | 0 | 0 | 7 | **14** | **91.56%** |

At 8× independent noise, useful exposure occurs only at requested speed 6.
At the same deviation with persistence .95, both passing models reach all three
speed groups while meeting the aggregate heading/spin requirement:

| Model seed | Speed 6 /8 | Speed 12 /8 | Speed 18 /8 |
|---:|---:|---:|---:|
| 29 | 8 | 7 | 4 |
| 43 | 5 | 6 | 3 |

Both exceed the eight-episode total, two-per-speed and six-over-baseline gates.
All 480 episodes reach their full six-second horizon with **zero physical
failures**. Seed 17 remains a failed model, consistent with its inherited steering
offset. Its exploration produces motion, but heading/spin qualifies for only
1.15–1.74% of intervals across the broader arms; it gains no joint useful exposure.
No bias correction or seed replacement was introduced.

### Why persistence matters in this comparison

Median peak forward speed, pooled over all 24 acceleration trials:

| Model seed | Original noise | 4× independent | 8× independent | 8× persistent |
|---:|---:|---:|---:|---:|
| 17 | .671 | 2.613 | 6.314 | 17.602 |
| 29 | .546 | 2.385 | 5.997 | 16.313 |
| 43 | .340 | 1.869 | 5.544 | 14.868 |

The original throttle/brake distribution never exceeds 50% throttle or releases
brake below 10% in these acceleration trials. Broader noise substantially changes
action coverage. In seed 29's 8,640 acceleration decisions, 8× independent noise
produces 350 simultaneous >50% throttle/<10% brake actions; persistence produces
392. Corresponding counts for seed 43 are 317 and 381. Yet median peak speed rises
from about 6 to 16 and 5.5 to 14.9, respectively, and useful exposure extends to
the higher commands. This supports the value of **temporally coherent pushes**,
beyond merely making individual action samples wider. Frozen policy weights,
paired innovations and the shared 8× scale make persistence the declared difference
between those two arms; physical states and subsequent policy means naturally diverge.

These are exploration-generated trajectories. The models' mean policies have not
learned to select those trajectories in response to speed commands. Even the
persistent arm has **zero uninterrupted native speed-tracking successes** in every
model. A half-second inside the wider exposure window is useful training coverage,
not established motor competence or sustained target-speed control.

### Braking and overspeed tradeoffs

Broader exploration disrupts stopping. Both heading-capable models retain all
16 braking controls with original actuator noise. With the persistent arm, each
retains only **1/16**; on-time acquisition is 5/16 for seed 29 and 6/16 for seed 43.
The independent 4×/8× arms retain none. Braking was a mandatory descriptive control,
not an acceptance requirement for this exploration-coverage gate; its failure is
explicitly preserved. This result does not justify applying broad actuator noise
to every training task or using it as a runtime controller.

The persistent arm also overshoots. Peak forward speeds reach **46.61/47.41**
units/s for seeds 29/43. Forward velocity exceeds 150% of the requested speed at
**1,044/921 of 8,640** acceleration endpoints (12.08%/10.66%). Heading and spin
remain jointly within limits for 98.26%/91.56% of complete intervals, but maximum
heading errors still reach 9.21°/10.56°. The useful-exposure window excludes those
out-of-bounds intervals. The small audit establishes access to useful motion and
records these limits; it does not establish safe general navigation.

### Verification and retention

All **23 Python tests** and **48 native environment tests** pass. The six new
checks cover matrix/draw determinism without global RNG use, the exact baseline
actuator distribution and learned mean steering, stationary-start AR recurrence
and residual restoration, native coordinate/horizon behavior, complete-interval
exposure counting, and rejection of incomplete speed coverage, heading loss,
missing gain or physical failure. Existing learner and retention tests still pass.

No existing source or native executable changed; the audit adds two Python files.
All **960 native/Python endpoint comparisons** pass. All **12 tick-180 sampler
and native continuations** reproduce every later actor output, innovation,
residual, latent, applied action and complete native trace exactly. The verifier
reconstructs every sampled action from the frozen innovations and actor records,
and every physical error, motor dwell/exit and exposure metric from native frames.
Original model parameters, global RNG, all copied input origins and **51 prior
manifest hashes** remain unchanged.

All four bounded processes exit zero without retry. Internal flight times are
**24.52–24.76 seconds**, external **26.57–26.87 seconds**; verification takes
9.79 seconds externally (7.96 internally). Use **172,800 scored action ticks**,
**2,160 restoration ticks**, **2,160 repeated continuation ticks** and **492
neutral setups**. Every limit is met. No optimizer steps, training ticks, new
training checkpoints or policy exports are created. Twelve sampler snapshots
retain audit continuation state only.

Retain 1,602 frozen sources, eight copied inputs, complete case/draw/variant
matrices, all 480 native traces and sample records, twelve continuation traces,
tests, commands, timings and full results. See
results (`docs/data/flight-motor-exploration-v1-results.json`, in git history) and
[archive receipt](data/flight-motor-exploration-v1-archive.json). No confirmation
use, model promotion or navigation qualification. Local retention is not off-machine backup.

## Next decision

Freeze a **bounded paired velocity-learning comparison** using all three original
LF-053 policies. Compare the original exploration with the supported persistent
actuator variant, preserving the LF-055 task matrix, reward and physical gates.
Keep wider exploration confined to positive-speed training episodes; retain
original actuator noise on zero-speed braking/heading rehearsal. One learned
motor network still owns all controls. The audit's brake failures make an
unconditional broad-noise rollout an unsuitable default.

Before scored training, implement and test the correct conditional AR likelihood,
per-actuator deviations, residual resets, saved state and exact continuation.
Declare steering exploration explicitly and match it across arms: LF-056 used
learned mean steering, so it has not established coverage with simultaneous
stochastic steering. A bounded preflight must check that intended training sampler
without changing the frozen pilot after results. Keep all seeds and final
checkpoints, with no checkpoint selection, native fallback or automatic extensions.
Passing the exploration audit permits this learning test; learned speed tracking,
zero-speed retention and obstacle navigation still require their own evidence.
