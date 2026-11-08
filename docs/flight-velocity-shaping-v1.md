# Velocity shaping comparison v1

**Completed 2026-10-23: removing velocity shaping increased sampled thrust but
did not improve arrival. Both arms failed the learning and progression gates.**

Specification and results for `learned-flight-velocity-v1-20261023`. The exact
pre-run specification is retained separately in the archive. Parent:
[persistent exploration comparison](flight-persistent-exploration-v1.md).
The user authorized this comparison after discussing potential shaping and PPO
credit assignment. Hypothesis: removing velocity shaping will make goal-directed
travel easier to learn without changing the physical arrival standard.

## Frozen comparison

Two fresh arms, each with learner seeds **17, 29, 43**:

- `control`: velocity potential weight **0.25**, exactly the existing reward.
- `no_velocity`: velocity potential weight **0**, with every other term unchanged.

The sole between-arm configuration difference is `velocity_shaping_weight`.
Both use independent Gaussian latent exploration (`gaussian-latent-ar1-v1`, rho=0),
the same 39,445-parameter network and critic, observation schema, PPO settings,
task pools, initialization seeds and 60 Hz observation/action/physics cadence.
No native decisions, labels, fallback or rescue. No new recurrent state, action
holds, task additions, tuning of spin shaping or changes to credit assignment.

```
Phi = -0.50*tanh(distance/100)
      -velocity_shaping_weight*tanh(relative_speed/20)
      -0.25*tanh(abs_spin)
reward = gamma*Phi(next) - Phi(current) - 0.001 + terminal_reward
```

Terminal potential is zero for every terminal outcome; gamma remains .9995.
Terminal rewards remain +10 success, -2 deadline and -10 physical/controller
failure. Environment errors abort. A zero weight removes only intermediate
velocity shaping; the low-speed success requirement remains unchanged. Complete
discounted shaping returns telescope in both arms, so this tests finite-learning
behavior rather than asserting that stopping is the mathematical optimal policy.
No cross-arm reward total is used as a competence metric.

Keep lambda .98, four epochs, minibatches 256, learning rate .0003, clip .2,
value weight .5, entropy weight .01, gradient clip .5 and target KL .03. Rollouts
contain 256 ticks in each of four persistent slots. Boundaries bootstrap; physical
terminals do not. Weights are fresh, not continued from a selected prior run.

## Tasks, budget and gates

Use the unchanged `clear-control-v1` recipe: four distance strata 4–12, 12–24,
24–48 and 48–80 units; maximum initial speeds 1/2/4/6 and spins .2/.4/.6/.8;
full headings and bearings, stationary goals, altitude 140–150, zero gravity and
planet spin. No added obstacles or edits. Native geometric admission remains
private evidence, not policy input or training labels. Training partition 4110001
has 16 accepted tasks per stratum; development partition 4110002 has four.

Arrival requires distance ≤3, target-relative speed ≤1 unit/s and absolute spin
≤0.2 rad/s for **30 complete consecutive ticks**. Heading is free. Deadline stays
1,800 ticks. Inspect initial and final deterministic policies on all 16 development
tasks. A changed reward does not relax evaluation.

Each run gets **128 updates / 131,072 training ticks**, with a 1,200-second wall
guard including evaluations. Six runs total **786,432 training ticks**, plus at
most 345,600 development ticks. Run concurrently when practical and report actual
concurrent timing. No budget extensions, replacement seeds or checkpoint searches.
Retain every update, all completed traces and active budget-censored traces.
Infrastructure/nonfinite failures stop the affected run and do not count as passes.

Predeclared physical gates:

1. **Arrival benefit:** the no-velocity final policy exceeds its paired control by
   ≥4/16 successes in at least two seeds, and passes the original learning gate.
2. **Learning:** ≥4/16 final successes and gain ≥4 versus initialization, in at
   least two of three seeds, separately for each arm.
3. **Progression:** ≥12/16 final successes, gain ≥8, ≥2 successes per stratum and
   zero physical failures, in at least two seeds, separately for each arm.

Only an arm passing progression gets reserved confirmation partition 4110003,
four tasks per stratum, all three final policies. Require ≥12/16 successes,
≥2 per stratum and zero physical failures in at least two seeds. Conditional
maximum 172,800 extra physics ticks if both arms qualify, 300-second wall guard
per policy. Otherwise leave that partition unused. Obstacle training is excluded.

Report diagnostic behavior separately from gates: thrust/brake use, thrust-run
length, goal-radius entries, simultaneous tolerance satisfaction, dwell, minimum
and final distance, closing ticks, thrust while distance decreases, first goal
entry speed/spin, and entry followed by leaving the radius. These can distinguish
movement/overshoot from inactivity; they cannot turn a failed arrival into success
or by themselves prove goal understanding. Preserve the original physical best
reference rule; qualification always uses the final policy.

## Verification and retention

Test exact compatibility of the control reward, isolation of the velocity term,
both variants' discounted telescoping and terminal/time rewards, unchanged first
rollout observations/actions with changed rewards, and bitwise fresh-process
resume for both coefficients. Keep existing policy/exploration tests passing.
Source/config identity must refuse incompatible resume. Verify each final trained
export against independent Rust inference on all development starts.

Before retained learning, freeze selected dirty/untracked source, this document,
six configurations and runtime identity; build/capture the executed Rust bridge.
Keep 774 checkpoints, twelve exports, all training/development traces, admissions,
latest/best references, commands, tests, reports and a SHA-256 inventory. Verify
the nine prior archive manifests remain unchanged. Source retention excludes
external dependency/toolchain binaries; local retention is not an off-machine
backup. No commit or clean checkout is implied.

## Results

All six runs completed 128 updates each: **786,432 training ticks** plus
**315,912 development ticks**. Concurrent elapsed times were 561.8–599.2 seconds
per run, with training/checkpoint throughput 303.6–306.0 ticks/s per learner.
No numerical failure, wall guard or KL early stop fired. The initial model,
optimizer, sampler, RNG state, task admissions and initial evaluation matched
within each seed pair. No acceptance criteria or budgets changed.

| Arm | Seed | Final arrivals | Training success / collision / deadline | Training thrust | Final thrust / brake |
| --- | --- | --- | --- | --- | --- |
| Control | 17 | 0/16 | 0 / 4 / 68 | 15.0% | 0% / 100% |
| Control | 29 | 0/16 | 0 / 7 / 68 | 14.5% | 0% / 0% |
| Control | 43 | 0/16 | 1 / 4 / 67 | 10.9% | 0% / 3.14% |
| No velocity shaping | 17 | 0/16 | 0 / 12 / 64 | 22.3% | 0% / 100% |
| No velocity shaping | 29 | 0/16 | 0 / 12 / 64 | 24.7% | 0% / 100% |
| No velocity shaping | 43 | 0/16 | 0 / 4 / 68 | 16.6% | 0% / 100% |

Each run also retained four budget-censored attempts; training control statistics
include their active traces. Initial evaluations had zero successes for all runs.
Every final development episode ended at its deadline, without collision or
damage. None entered the goal radius. Arrival benefit, learning and progression
all failed; paired success gains were `[0, 0, 0]`. Confirmation partition 4110003
remains unused and no obstacle training followed.

### Physical interpretation

Removing velocity shaping changed sampled training behavior: thrust use increased
in every seed. Goal-radius entries were 17/236 attempts versus 13/231 in the
control; simultaneous instantaneous tolerance satisfaction occurred on five
attempts versus four. However, the no-velocity arm reached at most 17 dwell ticks,
with zero training successes and 28 collisions. The control recorded one success,
15 collisions and a 30-tick dwell. Thrust bursts remained short in both arms:
means 1.35–1.46 ticks, none lasting 30 ticks.

The additional training thrust did not survive as useful deterministic behavior.
Every final no-velocity policy braked continuously without thrust. Its mean best
distance improvement across 16 tasks was only 0.31–0.35 units per seed. It never
entered the goal radius, so this was not an approach-and-overshoot result. The
controls' best-distance improvements were 0.26/3.14/3.64 units; the coasting seeds
finished substantially farther away on average. Those incidental approaches do
not demonstrate goal-directed control either. No final policy thrust while
closing distance because none used thrust at all.

The test does **not** support velocity shaping alone as the explanation for the
failure. It also does not prove that shaping is harmless or that PPO/the network
cannot learn flight. The remaining spin term, critic accuracy, credit assignment,
exploration and their interactions are not isolated by this experiment. Removing
one shaping term did not produce the hoped-for travel behavior within this budget.

The next proposed experiment should isolate longer credit assignment, as discussed
before this run. Freeze its exact setting and budget separately; hold reward,
exploration, model, tasks and physical arrival criteria fixed within that comparison.
Spin shaping can be tested separately afterward if warranted. Do not combine those
changes, extend completed runs or promote the no-velocity reward to a proven fix.
The default reward remains the original coefficient; both variants are explicit
experiment configurations.

### Verification and retention

All **20 Python tests** passed, including the original policy/exploration suites,
exact control-formula compatibility, isolation of the velocity term, discounted
telescoping, terminal/time invariants, identical first-rollout physical trajectories
and bitwise fresh-process resume for both coefficients. Python compilation and
whitespace checks passed. Rust source was unchanged and the rebuilt executable
is byte-identical to the prior experiment's bridge.

As an additional read-only reproducibility check, all three controls reproduced
the prior independent-exploration runs **bitwise through update 128**: model,
optimizer, collector/trajectories, RNGs and training metrics. Contracts correctly
differ because source and explicit reward configuration changed; old checkpoints
are not silently accepted for resume. All **96 trained Rust/PyTorch parity
fixtures** passed with matching actions and maximum output error **1.43×10⁻⁶**.

The [results](data/flight-velocity-shaping-v1-results.json) contain per-seed gates,
trajectories, action statistics and approach diagnostics. The
[archive receipt](data/flight-velocity-shaping-v1-archive.json) identifies all
**774 checkpoints**, twelve exports, training/development traces, frozen selected
source, runtime/configuration identities and validation logs. Nine previous
experiment manifests remain unchanged. This is local retention, not an off-machine
backup. Evidence remains limited to three seeds, the bounded budget and the declared
stationary-goal practice worlds; no obstacle-navigation capability is claimed.
