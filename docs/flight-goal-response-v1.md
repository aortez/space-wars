# Continuous-policy goal and velocity response probe

**Status: completed frozen inference audit, 2026-10-24.** Experiment
`learned-flight-goal-response-v1-20261024`. Follow the [first continuous training
experiment](flight-continuous-approach-v1.md), where policies usually entered the goal
radius but never met its speed tolerance. Inspect existing final seeds 17/29/43;
no optimizer, reward change, checkpoint selection or new flight trial.

## Fixed anchors and counterfactual inputs

For each model use all sixteen original development starting observations and the
first recorded post-step state inside the goal radius in each of its sixteen final
mean-control trajectories. If a trajectory never entered, record the missing anchor;
do not replace it. This selects 48 starts and 45 near-goal anchors (16/14/15), or
**93 anchors**. Restore the exact archived prefixes for near-goal observations and
verify frames and previous actions. The selected prefixes contain **5,555 action
ticks** total. Resets/replay also perform 93 ordinary neutral setup ticks; report both.

At each anchor keep position, orientation, angular velocity, geometry, object poses,
world velocities, health, task clock, dwell state and previous controls fixed. Move
the stationary goal to these ship-local offsets: origin once, then distances
**1.5, 3, 6, 12, 24 and 60** in each of four cardinal directions (forward +y,
back −y, right +x, left −x). Cross the 25 positions with signed ship-forward COM
speeds **−4, −1, 0, .5, 1, 2, 4 and 8**, with COM lateral velocity zero. This makes
**200 inputs per anchor** plus its unchanged original input: **18,693 native/Python
output comparisons**. Forward speed is approach speed only for a goal ahead.

Changes are controlled feature interventions, not admitted new tasks or claims that
every combination was visited during training. Near-goal anchors preserve the actual
prior controls and clock; starting anchors have their original neutral history.
Results from these two contexts must be reported separately. The model is feedforward,
but previous controls are inputs, so neutral-history probes alone are insufficient.

Encode goal displacement using the existing 300-unit scale. Changing COM velocity
must consistently change all representations: target-minus-COM velocity, COM world
velocity, origin world velocity, hit-ray and nearest-surface relative velocities, and
present-object relative velocities. Use the existing 70-unit velocity scale. The
origin-minus-COM velocity offset is preserved because spin/pose stay fixed. Fixed
world/control-frame velocities do not change. Absent payloads and all geometry remain
unchanged. Validate named feature indices against the native schema and test the
transformation independently, including masked hits/objects and identity transforms.

Use unchanged v2 mean decoding for each model, checking raw actor/value outputs and
executed tanh/sigmoid controls against native Rust on every input (absolute/relative
raw tolerance 1e−5; absolute control tolerance 1e−5). Verify exported tensors against
the copied final checkpoint and model/RNG neutrality. No stochastic action sampling.

## Predeclared comparisons

Report every model and both anchor contexts, with all counterfactual outputs retained.

- Distance response: forward goal at 3 versus 12 units, at forward COM speed 2.
  Report throttle/brake/steering deltas and full forward-distance ranges at this speed.
- Velocity response: forward goal at 3, comparing speeds 1 and 4. Report command
  changes separately from the braking force's inherent velocity dependence.
- Direction response: right versus left goal at distance 12 and speed zero. Positive
  steering turns toward local +x under the existing rate-control sign convention.
- Arrival demand: goal at zero and at +3, forward speeds zero and one. Record applied
  throttle/brake and calculated instantaneous forward actuator acceleration.

For descriptive categorization only, call a paired throttle/brake response material
when either command changes by at least **.05** (five percentage points). Use the same
.05 threshold for the right-minus-left steering response magnitude; report its sign.
These are engineering summaries, not significance tests, success gates or independent
samples. Retain raw changes and distributions even below this threshold.

Calculate instantaneous forward actuator acceleration from the actual control law:
use origin velocity relative to the control frame (including the preserved origin/COM
rotation offset), the speed governor, scaled forward thrust and velocity-opposing
braking capped at its native limit. This is an algebraic diagnostic, not a rollout,
handcrafted controller or long-term stability proof. Positive forward acceleration
at speed one illustrates pressure to leave the arrival-speed band; it does not by
itself prove failure under future changing commands. No evaluation against a native
pilot and no target action labels are generated for training.

## Budgets, interpretation and retention

Freeze specification, selected source, all three final exports/checkpoints, one
shared development admission, all 48 parent mean traces, parent results and unchanged
bridge before generating outputs. Synthetic transformation/summary tests use no
physics or trained-policy inference. No production learner or bridge changes.

- At most 93 anchors, 5,555 replayed action ticks plus 93 neutral setup ticks.
- Exactly 200 interventions plus one original input per available anchor; at most
  18,693 native/Python comparisons, in batches bounded to one anchor.
- Overall wall guard 600 seconds. No optimizer steps or flight evaluations. Stop on
  incompatible contracts, reconstruction/parity mismatch, nonfinite values or budget.
- No extra model seed, checkpoint scan, confirmation partition, new training task or
  automatic extension. The parent seed-43 exit anomaly remains a provenance limitation;
  verified retained weights are used descriptively, not promoted or requalified.

A small goal response would support weak learned conditioning in these observations,
not prove that the network cannot represent it or that normalization/reward is the
unique cause. A larger command response with inadequate stopping demand would suggest
a different next hypothesis. Do not choose a fix from response magnitude alone.
Counterfactual combinations and repeated anchors are not independent confirmation.

Retain original/modified inputs, native and Python outputs, per-anchor summaries,
source/runtime/input hashes, replay selections, tests/logs/commands and the completed
report. Verify all **23 previous manifests** and referenced parent inputs unchanged,
then seal a new archive and update the plan/journal. Local retention is not off-machine
backup; no commit or model promotion is implied.

## Results

All 93 anchors and 18,693 comparisons completed in **88.14 seconds**, exit zero.
The near-goal anchors missing by design were seed 29 tasks 3/15 and seed 43 task 3.
Exact replay consumed 5,555 action ticks plus 93 neutral setup ticks. There were
no optimizer steps, new flight trials, sampled actions or reserved confirmation.

The models show **very weak goal-distance and velocity conditioning** in these
contexts. Across all anchors, moving the forward goal from 12 to 3 units changed
neither throttle nor brake by more than **0.02391 percentage points**. Increasing
forward speed from 1 to 4 changed neither command by more than **0.18049 percentage
points**. All 93 paired responses of each type fall below the predeclared five-point
threshold. The raw magnitudes, rather than the threshold alone, support this finding.

Values below are mean command changes in **percentage points**, not percentages of
the original command. Distance is near minus far; velocity is fast minus slow.

| Seed | Context | Anchors | Distance Δ throttle | Distance Δ brake | Speed Δ throttle | Speed Δ brake |
|---|---|---:|---:|---:|---:|---:|
| 17 | start | 16 | -0.00512 | +0.00909 | -0.01431 | +0.12569 |
| 17 | near | 16 | -0.00460 | +0.00979 | -0.00995 | +0.11944 |
| 29 | start | 16 | +0.00785 | -0.01455 | +0.04232 | -0.04356 |
| 29 | near | 14 | +0.00799 | -0.01565 | +0.03161 | -0.03203 |
| 43 | start | 16 | -0.01433 | +0.00798 | -0.06030 | +0.01345 |
| 43 | near | 15 | -0.01329 | +0.00709 | -0.04560 | +0.00541 |

Even across the full forward-distance sweep from 0 to 60 at speed 2, the largest
throttle span is only 0.16157 percentage points and brake span 0.14800 points.
Seed 29 often moves slightly in the opposite direction to stopping demand: more
throttle and less brake as the goal gets closer or the ship gets faster. Those
changes are also tiny; their sign alone should not be overinterpreted.

The steering difference between right and left goals has magnitude at most .002417,
with zero of 93 reaching .05. Its sign is correct in 1/32 seed-17 contexts, 30/30
seed-29 and 31/31 seed-43 contexts. This is descriptive: the training task starts
aligned, so turning toward a sideways goal is outside its core practiced maneuver.

### Arrival demand

Putting the goal at the ship and setting forward COM speed to one still produces
**positive instantaneous forward actuator acceleration in all 93 contexts**.
The same is true at speed zero and at a goal three units ahead. Throttle remains
roughly 13–26%, despite substantial simultaneous braking. The following ranges use
the goal-at-origin, speed-one case; controls are context means.

| Seed | Context | Mean throttle | Mean brake | Forward acceleration range (units/s²) |
|---|---|---:|---:|---:|
| 17 | start | 14.23% | 69.43% | 3.251–4.167 |
| 17 | near | 14.14% | 69.50% | 3.175–4.373 |
| 29 | start | 23.17% | 64.03% | 6.494–9.094 |
| 29 | near | 22.31% | 64.43% | 6.432–8.889 |
| 43 | start | 18.55% | 67.78% | 5.030–7.210 |
| 43 | near | 17.88% | 68.37% | 4.889–5.927 |

These are estimates from the unchanged actuator equation and encoded origin/frame
velocity, not additional simulated outcomes. Faster motion naturally increases
braking force even with constant brake command. The command measurements show that
this mechanical effect is much stronger than the learned change in brake demand.
They support the earlier observation of a sustained throttle/brake balance, without
claiming that this algebraic check proves the subsequent trajectory.

### Interpretation and next decision

The failure is broader than mistiming an otherwise strong braking response: these
saved policies barely change their commands when the task's distance or speed changes.
This reproduces across original starts and actual near-goal contexts, so neutral
previous-control inputs alone do not explain the weak response.

This does **not** establish that the network lacks capacity, that normalization is
the cause, or that the reward is sound. Training incentives, exploration and learned
feature weights remain possible contributors. Feature interventions preserve the
specified geometry and velocity relationships, but some combinations are outside
visited training states; these correlated contexts are not independent confirmation.

The next proposed experiment is a **single controlled input-scaling comparison**:
fresh models with the existing representation versus a task-scale encoding of goal
position and target-relative velocity. The existing normalization maps a 3–12-unit
goal to .01–.04 and a 1–4-unit/s speed to .014–.057. A local scale is a plausible
learning aid, not a demonstrated fix. Freeze exact scales, matched seeds, training
budgets and unchanged physical success gates before running. Keep reward, network
width/depth, actuators, task distribution and PPO settings fixed; report this same
response probe alongside physical arrival results. Do not rescale these trained
models and mistake the resulting out-of-distribution outputs for learning progress.
No such training or representation change was made in this audit.

### Verification and retention

Six synthetic tests cover the intervention grid, coherent velocity features,
masked payloads, stationary-target guard, origin-based actuator equation and the
distinction between mechanical braking and learned command response. Compilation
and whitespace checks passed. Every restored frame and previous control matched;
exports exactly matched checkpoint tensors; model parameters and RNG were unchanged.
Native/Python maximum raw error was **1.90735e−6**, action error **1.49012e−7**,
within the frozen tolerances on every comparison.

Archive `data/training-experiments/learned-flight-goal-response-v1-20261024`
retains 1,537 source files, 56 copied parent inputs, native runtime, pre-run spec,
all anchor/input/output batches, summaries and logs. All 23 previous manifests and
referenced parent inputs remain unchanged. Parent seed 43's unexplained exit 143 is
still a provenance limitation; this clean audit does not promote or requalify it.
No model was promoted. See [machine-readable results](data/flight-goal-response-v1-results.json)
and [archive receipt](data/flight-goal-response-v1-archive.json).
